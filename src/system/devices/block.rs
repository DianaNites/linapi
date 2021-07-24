//! This module provides ways to get information about connected Block devices
use crate::{
    extensions::FileExt,
    util::{DEV_PATH, SYSFS_PATH},
};
use bitflags::bitflags;
use displaydoc::Display;
use nix::sys::stat;
use std::{
    convert::TryInto,
    fs,
    fs::DirEntry,
    io,
    io::prelude::*,
    ops::Range,
    os::{linux::fs::MetadataExt, unix::fs::FileTypeExt},
    path::{Path, PathBuf},
    time::Duration,
};
use thiserror::Error;

/// Block Error type
#[derive(Debug, Display, Error)]
pub enum Error {
    /// IO Failed
    Io(#[from] io::Error),

    /// Invalid argument: {0}
    InvalidArg(&'static str),

    /// The device or attribute was invalid
    Invalid,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Parse the undocumented `dev` device attribute.
///
/// This seems to be formatted as `major:minor\n`
///
/// # Errors
///
/// - I/O
/// - Unexpected format
fn parse_dev(path: &Path) -> Result<(u64, u64)> {
    let i = fs::read_to_string(path.join("dev"))?;
    let mut i = i.trim().split(':');
    //
    let major = i.next().ok_or(Error::Invalid)?;
    let minor = i.next().ok_or(Error::Invalid)?;
    //
    let major = major.parse::<u64>().map_err(|_| Error::Invalid)?;
    let minor = minor.parse::<u64>().map_err(|_| Error::Invalid)?;
    //
    Ok((major, minor))
}

/// Search for the a device special file in [`DEV_PATH`] with matching
/// major/minors
///
/// File is opened for both reading and writing.
///
/// [`None`] is returned if it doesn't exist.
fn find_from_major_minor(major: u64, minor: u64) -> Result<Option<PathBuf>> {
    for dev in fs::read_dir(DEV_PATH)? {
        let dev: DirEntry = dev?;
        if !dev.file_type()?.is_block_device() {
            continue;
        }
        let meta = dev.metadata()?;
        let dev_id = meta.st_rdev();
        if (major, minor) == (stat::major(dev_id), stat::minor(dev_id)) {
            return Ok(Some(dev.path()));
        }
    }
    Ok(None)
}

fn dev_size(path: &Path) -> Result<u64> {
    fs::read_to_string(path.join("size"))?
        .trim()
        .parse::<u64>()
        // Per [this][1] forgotten 2015 patch, this is in 512 byte sectors.
        // [1]: https://lore.kernel.org/lkml/1451154995-4686-1-git-send-email-peter@lekensteyn.nl/
        .map(|b| b * 512)
        .map_err(|_| Error::Invalid)
}

bitflags! {
    /// Flags corresponding to [`Block::capability`].
    ///
    /// See the [linux kernel docs][1] for details.
    ///
    /// # Note
    ///
    /// Most of these seem to officially be undocumented.
    /// They will be documented here on a best-effort basis.
    ///
    /// [1]: https://www.kernel.org/doc/html/latest/block/capability.html
    pub struct BlockCap: u32 {
        /// Set for removable media with permanent block devices
        ///
        /// Unset for removable block devices with permanent media
        const REMOVABLE = 1;

        /// Block Device supports Asynchronous Notification of media change events.
        /// These events will be broadcast to user space via kernel uevent.
        const MEDIA_CHANGE_NOTIFY = 4;

        /// CD-like
        const CD = 8;

        /// Alive, online, active.
        const UP = 16;

        /// Doesn't appear in `/proc/partitions`
        const SUPPRESS_PARTITION_INFO = 32;

        /// Unknown
        const EXT_DEVT = 64;

        /// Unknown
        const NATIVE_CAPACITY = 128;

        /// Unknown
        const BLOCK_EVENTS_ON_EXCL_WRITE = 256;

        /// Unknown
        const NO_PART_SCAN = 512;

        /// Unknown
        const HIDDEN = 1024;
    }
}

/// A Block Device
#[derive(Debug, Clone)]
pub struct Block {
    /// Kernel name
    name: String,

    /// Canonical, full, path to the device.
    path: PathBuf,

    /// Major device number. Read from the undocumented `dev` file.
    major: u64,

    /// Minor device number. Read from the undocumented `dev` file.
    minor: u64,
}

// Public
impl Block {
    /// Get connected Block Devices.
    ///
    /// # Note
    ///
    /// Partitions are **not** included. Use [`Block::partitions`].
    ///
    /// The returned Vec is sorted by kernel name.
    ///
    /// # Errors
    ///
    /// - [`Error::Io`] for I/O errors
    pub fn get_connected() -> Result<Vec<Self>> {
        let sysfs = Path::new(SYSFS_PATH);
        let mut devices = Vec::new();
        // Per linux sysfs-rules, if /sys/subsystem exists, class should be ignored.
        // If it doesn't exist, both places need scanning.
        let mut paths = vec![sysfs.join("subsystem/block/devices")];
        if !paths[0].exists() {
            paths = vec![sysfs.join("class/block"), sysfs.join("block")];
        }
        for path in paths {
            if !path.exists() {
                continue;
            }
            for dev in path.read_dir()? {
                let dev: DirEntry = dev?;
                // Skip partitions. Note that this attribute is undocumented.
                if dev.path().join("partition").exists() {
                    continue;
                }
                devices.push(Self::new(dev.path().canonicalize()?)?);
            }
        }
        // FIXME: Better way to prevent duplicates than this?
        // Ok to only search one of `/sys/class/block` and `/sys/block`?
        devices.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        devices.dedup_by(|a, b| a.name == b.name);
        Ok(devices)
    }

    /// Create from a device file in `/dev`
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidArg`] if `path` is not a block device
    /// - [`Error::InvalidArg`] if `path` is a partition
    /// - [`Error::Io`] for I/O errors
    pub fn from_dev(path: &Path) -> Result<Self> {
        let sysfs = Path::new(SYSFS_PATH);
        let meta = path.metadata()?;
        if !meta.file_type().is_block_device() {
            return Err(Error::InvalidArg("path"));
        }
        let dev_id = meta.st_rdev();
        let (major, minor) = (stat::major(dev_id), stat::minor(dev_id));
        let path = sysfs.join("dev/block").join(format!("{}:{}", major, minor));
        let path = path.canonicalize()?;
        if path.join("partition").exists() {
            return Err(Error::InvalidArg("path"));
        }
        Self::new(path)
    }

    /// Canonical path to the block device.
    ///
    /// You normally shouldn't need this, but it could be useful if
    /// you want to manually access information not exposed by this crate.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Path to the device *file*, usually in `/dev`.
    pub fn dev_path(&self) -> Result<Option<PathBuf>> {
        find_from_major_minor(self.major, self.minor)
    }

    /// Kernel name for this device.
    ///
    /// This does not have to match whats in `/dev`
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get this devices partitions, if any.
    ///
    /// # Errors
    ///
    /// - If I/O does
    pub fn partitions(&self) -> Result<Vec<Partition>> {
        let mut devices = Vec::new();
        for dir in fs::read_dir(&self.path)? {
            let dir: DirEntry = dir?;
            let path = dir.path();
            if !dir.file_type()?.is_dir() || !path.join("partition").exists() {
                continue;
            }
            devices.push(Partition::new(path)?);
        }
        Ok(devices)
    }

    /// Open the device special file in `/dev` associated with this block
    /// device, if it exists.
    ///
    /// The device file is opened for reading and writing
    ///
    /// # Errors
    ///
    /// - If I/O does
    pub fn open(&self) -> Result<Option<fs::File>> {
        let path = find_from_major_minor(self.major, self.minor)?;
        match path {
            Some(path) => Ok(Some(
                fs::OpenOptions::new().read(true).write(true).open(path)?,
            )),
            None => Ok(None),
        }
    }

    /// Device major number
    pub fn major(&self) -> u64 {
        self.major
    }

    /// Device minor number
    pub fn minor(&self) -> u64 {
        self.minor
    }

    /// Get the byte size of the device, if possible.
    pub fn size(&self) -> Result<u64> {
        dev_size(&self.path)
    }

    /// Get device capabilities.
    ///
    /// Unknown flags *are* preserved
    ///
    /// See [`BlockCap`] for more details.
    pub fn capability(&self) -> Result<BlockCap> {
        // Unknown bits are safe, and the kernel may add new flags.
        Ok(unsafe {
            BlockCap::from_bits_unchecked(
                std::fs::read_to_string(self.path.join("capability"))?
                    .trim()
                    .parse()
                    .map_err(|_| Error::Invalid)?,
            )
        })
    }

    /// Get device power information
    ///
    /// See [`Power`] for details
    pub fn power(&self) -> Power {
        Power::new(&self.path)
    }

    /// Tell linux that partition `num` exists in the range `start_end`.
    ///
    /// `start_end` is a *byte* range within the whole device.
    /// This range is NOT inclusive of `end`.
    ///
    /// This does NOT modify partitions or anything on disk, only the kernels
    /// view of the device.
    ///
    /// This can be useful in cases where the kernel doesn't support your
    /// partition table, you can read it yourself and tell it.
    ///
    /// # Examples
    ///
    /// Add a partition
    ///
    /// ```rust,no_run
    /// # use linapi::system::devices::block::Block;
    /// let mut block = Block::get_connected().unwrap().remove(0);
    /// // Tell Linux there is one partition, starting at (1024 * 512) bytes
    /// // and covering the whole device.
    /// block.add_partition(0, 1024*512..block.size().unwrap() as i64);
    /// ```
    ///
    /// # Errors
    ///
    /// - If the ioctl does.
    ///
    /// # Implementation
    ///
    /// This uses the ioctls from `include/linux/blkpg.h`.
    pub fn add_partition(&mut self, num: u64, start_end: Range<i64>) -> Result<()> {
        let f = self.open()?.ok_or(Error::Invalid)?;
        // TODO: Better errors, rewrite, label.
        f.add_partition(
            num.try_into()
                .map_err(|_| Error::InvalidArg("Partition number was too large"))?,
            start_end.start,
            start_end.end,
        )
        .map_err(|_| Error::Invalid)?;
        Ok(())
    }

    /// Tell Linux to forget about partition `num`.
    ///
    /// # Examples
    ///
    /// Remove a partition
    ///
    /// ```rust,no_run
    /// # use linapi::system::devices::block::Block;
    /// let mut block = Block::get_connected().unwrap().remove(0);
    /// let part = block.partitions().unwrap().remove(0);
    /// block.remove_partition(part.number().unwrap());
    /// ```
    pub fn remove_partition(&mut self, num: u64) -> Result<()> {
        let f = self.open()?.ok_or(Error::Invalid)?;
        // TODO: Better errors, rewrite.
        f.remove_partition(
            num.try_into()
                .map_err(|_| Error::InvalidArg("Partition number was too large"))?,
        )
        .map_err(|_| Error::Invalid)?;
        Ok(())
    }

    /// Convenience function for looping through [`Block::partitions`] yourself.
    ///
    /// # Implementation
    ///
    /// For now this is slightly more efficient than doing it manually,
    /// opening the device only once instead of for each partition.
    pub fn remove_existing_partitions(&mut self) -> Result<()> {
        let f = self.open()?.ok_or(Error::Invalid)?;
        let parts = self.partitions()?;
        for part in parts {
            // TODO: Better errors, rewrite.
            f.remove_partition(
                part.number()?
                    .try_into()
                    .map_err(|_| Error::InvalidArg("Partition number was too large"))?,
            )
            .map_err(|_| Error::Invalid)?;
        }
        Ok(())
    }

    /// Get device model, if it exists.
    pub fn model(&self) -> Result<Option<String>> {
        // Unwraps should be okay, always a parent.
        // Note that this file is mostly undocumented, at this location,
        // but see [here][1] for more details
        // [1]: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-bus-pci-devices-cciss
        let path = self.path.parent().unwrap().parent().unwrap().join("model");
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(fs::read_to_string(path).map(|s| s.trim().to_owned())?))
    }

    /// Device logical block size, the smallest unit the device can address.
    ///
    /// This is usually 512
    pub fn logical_block_size(&self) -> Result<u64> {
        fs::read_to_string(self.path.join("queue/logical_block_size"))?
            .trim()
            .parse::<u64>()
            .map_err(|_| Error::Invalid)
    }
}

// Private
impl Block {
    fn new(path: PathBuf) -> Result<Self> {
        let (major, minor) = parse_dev(&path)?;
        Ok(Self {
            name: path
                .file_name()
                .and_then(|s| s.to_str())
                .map(Into::into)
                .unwrap(),
            path,
            major,
            minor,
        })
    }
}

/// A partition
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partition {
    /// Kernel name
    name: String,

    /// Canonical, full, path to the partition.
    path: PathBuf,

    /// Major device number. Read from the undocumented `dev` file.
    major: u64,

    /// Minor device number. Read from the undocumented `dev` file.
    minor: u64,
}

// Public
impl Partition {
    /// Open the device file for this partition.
    ///
    /// See [`Block::open`] for details
    pub fn open(&self) -> Result<Option<fs::File>> {
        let path = find_from_major_minor(self.major, self.minor)?;
        match path {
            Some(path) => Ok(Some(
                fs::OpenOptions::new().read(true).write(true).open(path)?,
            )),
            None => Ok(None),
        }
    }

    /// Get the byte size of the device, if possible.
    pub fn size(&self) -> Result<u64> {
        dev_size(&self.path)
    }

    /// Byte offset at which the partition starts
    pub fn start(&self) -> Result<u64> {
        // Note that this file is undocumented, but seems to contain the
        // partition start in units of 512 bytes.
        fs::read_to_string(self.path.join("start"))?
            .trim()
            .parse::<u64>()
            .map(|i| i * 512)
            .map_err(|_| Error::Invalid)
    }

    /// Kernel name for the partition.
    ///
    /// This does not have to match whats in `/dev`
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Canonical path to the partition.
    ///
    /// You normally shouldn't need this, but it could be useful if
    /// you want to manually access information not exposed by this crate.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Path to the device *file*, usually in `/dev`.
    pub fn dev_path(&self) -> Result<Option<PathBuf>> {
        find_from_major_minor(self.major, self.minor)
    }

    /// Partition number
    pub fn number(&self) -> Result<u64> {
        // Note that this file is undocumented, but seems to contain the partition
        // number.
        fs::read_to_string(self.path.join("partition"))?
            .trim()
            .parse::<u64>()
            .map_err(|_| Error::Invalid)
    }
}

// Private
impl Partition {
    fn new(path: PathBuf) -> Result<Self> {
        let (major, minor) = parse_dev(&path)?;
        Ok(Self {
            name: path
                .file_name()
                .and_then(|s| s.to_str())
                .map(Into::into)
                .unwrap(),
            path,
            major,
            minor,
        })
    }
}

/// See [`Power`] for details
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Status {
    Suspended,
    Suspending,
    Resuming,
    Active,
    FatalError,
    Unsupported,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Control {
    /// Device power is automatically managed by the system, and it may be
    /// automatically suspended
    Auto,

    /// Device power is *not* automatically managed by the system, auto suspend
    /// is not allowed, and it's woken up if it was suspended.
    ///
    /// In short, the device will remain "on" and fully powered.
    ///
    /// This does not prevent system suspends.
    On,
}

/// Device power information.
///
/// See the [kernel docs][1] for details
///
/// [1]: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-devices-power
#[derive(Debug, Copy, Clone)]
pub struct Power<'a> {
    path: &'a Path,
}

// Public
impl Power<'_> {
    /// Get current run-time power management setting
    ///
    /// See [`Control`] for details
    pub fn control(&self) -> Result<Control> {
        match fs::read_to_string(self.path.join("power/control"))?.trim() {
            "auto" => Ok(Control::Auto),
            "on" => Ok(Control::On),
            _ => Err(Error::Invalid),
        }
    }

    /// Set the current run-time power management setting
    ///
    /// See [`Control`] for details
    pub fn set_control(&mut self, control: Control) -> Result<()> {
        if self.control()? == control {
            return Ok(());
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(self.path.join("power/control"))?;
        match control {
            Control::Auto => file.write_all(b"auto")?,
            Control::On => file.write_all(b"on")?,
        };
        Ok(())
    }

    /// Current runtime PM status
    ///
    /// See [`Status`] for details
    pub fn status(&self) -> Result<Status> {
        match fs::read_to_string(self.path.join("power/runtime_status"))?.trim() {
            "suspended" => Ok(Status::Suspended),
            "suspending" => Ok(Status::Suspending),
            "resuming" => Ok(Status::Resuming),
            "active" => Ok(Status::Active),
            "error" => Ok(Status::FatalError),
            "unsupported" => Ok(Status::Unsupported),
            _ => Err(Error::Invalid),
        }
    }

    /// Current auto-suspend delay, if supported.
    pub fn autosuspend_delay(&self) -> Result<Option<Duration>> {
        let f = fs::read_to_string(self.path.join("power/autosuspend_delay_ms"));
        if let Err(Some(5)) = f.as_ref().map_err(|e| e.raw_os_error()) {
            return Ok(None);
        }
        let s = f?;
        Ok(Some(Duration::from_millis(
            s.trim().parse().map_err(|_| Error::Invalid)?,
        )))
    }

    /// Set the auto-suspend delay, if supported.
    ///
    /// If `delay` is larger than 1 second, it will be rounded to the nearest
    /// second by the kernel.
    pub fn set_autosuspend_delay(&mut self, delay: Duration) -> Result<Option<()>> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .open(self.path.join("power/autosuspend_delay_ms"))?;
        let f = write!(f, "{}", delay.as_millis());
        if let nix::Error::Sys(nix::errno::Errno::EIO) = nix::Error::last() {
            return Ok(None);
        }
        f?;
        Ok(Some(()))
    }

    /// Whether the device is suspended/resumed asynchronously, during
    /// system-wide power transitions.
    ///
    /// This defaults to `false` for most devices.
    pub fn async_(&self) -> Result<bool> {
        match fs::read_to_string(self.path.join("power/async"))?.trim() {
            "enabled" => Ok(true),
            "disabled" => Ok(false),
            _ => Err(Error::Invalid),
        }
    }

    /// Wakeup information.
    ///
    /// If this device is capable of waking the system up from sleep states,
    /// [`Some`] is returned.
    ///
    /// If the Device does not support this, [`None`] is returned.
    pub fn wakeup(&self) -> Option<Wakeup> {
        let path = self.path.join("power/wakeup");
        if !path.exists() {
            return None;
        }
        Some(Wakeup::new(self.path))
    }
}

// Private
impl<'a> Power<'a> {
    fn new(path: &'a Path) -> Self {
        Self { path }
    }
}

/// Device wakeup information
#[derive(Debug)]
pub struct Wakeup<'a> {
    path: &'a Path,
}

// Public
impl Wakeup<'_> {
    /// Whether the device is allowed to issue wakeup events.
    pub fn enabled(&self) -> Result<bool> {
        match fs::read_to_string(self.path.join("power/wakeup"))?.trim() {
            "enabled" => Ok(true),
            "disabled" => Ok(false),
            _ => Err(Error::Invalid),
        }
    }

    /// Set whether the device can wake the system up.
    pub fn set_enabled(&mut self, enabled: bool) -> Result<()> {
        if enabled == self.enabled()? {
            return Ok(());
        }
        let mut f = fs::OpenOptions::new()
            .write(true)
            .open(self.path.join("power/wakeup"))?;
        if enabled {
            write!(&mut f, "enabled")?;
        } else {
            write!(&mut f, "disabled")?;
        };
        Ok(())
    }
}

// Private
impl<'a> Wakeup<'a> {
    fn new(path: &'a Path) -> Self {
        Self { path }
    }
}

/// Helper for lots of repetitious wakeup functions.
macro_rules! wakeup_helper {
    ($(#[$outer:meta])* $name:ident, $file:literal) => {
        impl Wakeup<'_> {
            pub fn $name(&self) -> Result<u32> {
                fs::read_to_string(self.path.join(concat!("power/", $file)))?
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| Error::Invalid)
            }
        }
    };
}

/// Helper for lots of repetitious wakeup functions.
macro_rules! wakeup_helper_d {
    (   $(#[$outer:meta])*
        $name:ident, $file:literal) => {
        impl Wakeup<'_> {
            pub fn $name(&self) -> Result<Duration> {
                Ok(Duration::from_millis(
                    fs::read_to_string(self.path.join(concat!("power/", $file)))?
                        .trim()
                        .parse::<u64>()
                        .map_err(|_| Error::Invalid)?,
                ))
            }
        }
    };
}

wakeup_helper!(
    /// How many times this device has signaled a wakeup event.
    count,
    "wakeup_count"
);

wakeup_helper!(
    /// How many times this device has completed a wakeup event.
    count_active,
    "wakeup_active_count"
);

wakeup_helper!(
    /// How many times this Device has aborted a sleep state transition.
    count_abort,
    "wakeup_abort_count"
);

wakeup_helper!(
    /// How many times a wakeup event timed out.
    count_expired,
    "wakeup_expire_count"
);

wakeup_helper!(
    /// Whether a wakeup event is currently being processed.
    active,
    "wakeup_active"
);

wakeup_helper_d!(
    /// Total time spent processing wakeup events from this device.
    total_time,
    "wakeup_total_time_ms"
);

wakeup_helper_d!(
    /// Maximum time spent processing a *single* wakeup event.
    max_time,
    "wakeup_max_time_ms"
);

wakeup_helper_d!(
    /// Value of the monotonic clock corresponding to the time of
    /// signaling the last wakeup event associated with this device.
    last_time,
    "wakeup_last_time_ms"
);

wakeup_helper_d!(
    /// Total time this device has prevented the System from transitioning
    /// to a sleep state.
    prevent_sleep_time,
    "wakeup_prevent_sleep_time_ms"
);
