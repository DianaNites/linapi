//! Interfaces common to Block devices
use crate::util::{DEV_PATH, SYSFS_PATH};
use bitflags::bitflags;
use displaydoc::Display;
use nix::sys::stat;
use std::{
    fs,
    fs::DirEntry,
    io,
    io::prelude::*,
    os::{linux::fs::MetadataExt, unix::fs::FileTypeExt},
    path::{Path, PathBuf},
};
use thiserror::Error;

///
#[derive(Debug, Display, Error)]
pub enum Error {
    /// IO Failed: {0}
    Io(#[from] io::Error),

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
    let major = i.next().ok_or_else(|| Error::Invalid)?;
    let minor = i.next().ok_or_else(|| Error::Invalid)?;
    //
    let major = major.parse::<u64>().map_err(|_| Error::Invalid)?;
    let minor = minor.parse::<u64>().map_err(|_| Error::Invalid)?;
    //
    Ok((major, minor))
}

/// Search for and open a special file in [`DEV_PATH`] with matching
/// major/minors
///
/// File is opened for both reading and writing.
///
/// [`None`] is returned if it doesn't exist.
fn open_from_major_minor(major: u64, minor: u64) -> Result<Option<fs::File>> {
    for dev in fs::read_dir(DEV_PATH)? {
        let dev: DirEntry = dev?;
        if !dev.file_type()?.is_block_device() {
            continue;
        }
        let meta = dev.metadata()?;
        let dev_id = meta.st_dev();
        if (major, minor) == (stat::major(dev_id), stat::minor(dev_id)) {
            return Ok(Some(
                fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(dev.path())?,
            ));
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
#[derive(Debug)]
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
    /// For devices with partitions, their partitions are **not** returned by
    /// this method. You can get partitions using [`Block::partitions`]
    ///
    /// # Errors
    ///
    /// - If I/O does
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

    /// Canonical path to the block device.
    ///
    /// You normally shouldn't need this, but it could be useful if
    /// you want to manually access information not exposed by this crate.
    pub fn path(&self) -> &Path {
        &self.path
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
        open_from_major_minor(self.major, self.minor)
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
        open_from_major_minor(self.major, self.minor)
    }

    /// Get the byte size of the device, if possible.
    pub fn size(&self) -> Result<u64> {
        dev_size(&self.path)
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
}

// Private
impl<'a> Power<'a> {
    fn new(path: &'a Path) -> Self {
        Self { path }
    }
}
