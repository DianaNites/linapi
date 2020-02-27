//! Interfaces common to Block devices
use crate::{
    error::DeviceError,
    system::devices::raw::{Device, Power, RawDevice, Result},
};
use bitflags::bitflags;
use std::{
    fs,
    fs::{read_dir, DirEntry},
    path::{Path, PathBuf},
};

bitflags! {
    /// Flags corresponding to [`BlockDevice::capability`].
    ///
    /// See the [linux kernel docs][1] for details.
    ///
    /// # Note
    ///
    /// Most of these seem to officially be undocumented.
    /// They have been documented here on a best-effort basis.
    ///
    /// [1]: https://www.kernel.org/doc/html/latest/block/capability.html
    pub struct BlockCap: u32 {
        /// Device is removable?
        const REMOVABLE = 1;

        /// Block Device supports Asynchronous Notification of media change events.
        /// These events will be broadcast to user space via kernel uevent.
        const MEDIA_CHANGE_NOTIFY = 4;

        /// Device is a CD?
        const CD = 8;

        /// Device is currently online?
        const UP = 16;

        /// Partition info suppressed?
        const SUPPRESS_PARTITION_INFO = 32;

        /// Device supports extended partitions? Up to 256 partitions, less otherwise?
        const EXT_DEVT = 64;

        /// Unknown
        const NATIVE_CAPACITY = 128;

        /// Unknown
        const BLOCK_EVENTS_ON_EXCL_WRITE = 256;

        /// Partition scanning disabled?
        const NO_PART_SCAN = 512;

        /// Device hidden?
        const HIDDEN = 1024;
    }
}

/// A Linux Block Device
///
/// # Note
///
/// Except where otherwise noted, this interface is based on [this][1] kernel
/// documentation.
///
/// [1]: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-block
///
/// # Implementation
///
/// [`Device::refresh`] should be implemented to refresh this information, too.
pub trait Block: Device {
    /// Major Device Number
    ///
    /// # Note
    ///
    /// This interface uses the `dev` file, which is undocumented.
    fn major(&self) -> u32;

    /// Minor Device Number
    ///
    /// # Note
    ///
    /// This interface uses the `dev` file, which is undocumented.
    fn minor(&self) -> u32;

    /// Device capabilities. See [`BlockCap`] for details.
    ///
    /// # Note
    ///
    /// You can use [`BlockCap::bits`] to get the raw value and manually test
    /// flags if need be.
    ///
    /// Unknown flags *are* preserved.
    fn capability(&self) -> BlockCap;

    /// Size of the Block Device, in bytes.
    ///
    /// # Note
    ///
    /// This interface is undocumented, except in a
    /// [forgotten patch from 2015][1]. The interface has been stable for nearly
    /// 20 years, however.
    ///
    /// [1]: https://lore.kernel.org/lkml/1451154995-4686-1-git-send-email-peter@lekensteyn.nl/
    fn size(&self) -> u64;

    /// How many bytes the beginning of the device is
    /// offset from the disk's natural alignment.
    fn alignment_offset(&self) -> u64;

    /// How many bytes the beginning of the device is offset from the disk's
    /// natural alignment.
    fn discard_alignment_offset(&self) -> u64;

    /// Partitions this Block Device has
    fn partitions(&self) -> &Vec<Partition>;
}

/// A Partition of a Linux Block Device
#[derive(Debug)]
pub struct Partition {
    device_path: PathBuf,
}

impl Partition {
    pub fn device_path(&self) -> &Path {
        &self.device_path
    }

    /// How many bytes the beginning of the partition is
    /// offset from the disk's natural alignment.
    pub fn alignment_offset(&self) -> u64 {
        fs::read_to_string(self.device_path().join("alignment_offset"))
            .map(|s| s.trim().parse().unwrap())
            .unwrap()
    }

    /// How many bytes the beginning of the partition is offset from the
    /// disk's natural alignment.
    pub fn discard_alignment_offset(&self) -> u64 {
        fs::read_to_string(self.device_path().join("discard_alignment"))
            .map(|s| s.trim().parse().unwrap())
            .unwrap()
    }

    /// Size of the Partition, in bytes.
    ///
    /// # Note
    ///
    /// This interface is undocumented, except in a
    /// [forgotten patch from 2015][1]. The interface has been stable for nearly
    /// 20 years, however.
    ///
    /// [1]: https://lore.kernel.org/lkml/1451154995-4686-1-git-send-email-peter@lekensteyn.nl/
    pub fn size(&self) -> u64 {
        fs::read_to_string(self.device_path().join("size"))
            .map(|s| s.trim().parse::<u64>().unwrap() * 512)
            .unwrap()
    }

    /// Start position of the Partition on the disk, in bytes.
    ///
    /// # Note
    ///
    /// This interface is undocumented, except in a
    /// [forgotten patch from 2015][1]. The interface has been stable for nearly
    /// 20 years, however.
    ///
    /// [1]: https://lore.kernel.org/lkml/1451154995-4686-1-git-send-email-peter@lekensteyn.nl/
    pub fn start(&self) -> u64 {
        fs::read_to_string(self.device_path().join("start"))
            .map(|s| s.trim().parse::<u64>().unwrap() * 512)
            .unwrap()
    }
}

/// Represents a Block Device
#[derive(Debug)]
pub struct BlockDevice {
    dev: RawDevice,
    major: u32,
    minor: u32,
    capability: BlockCap,
    size: u64,
    alignment_offset: u64,
    discard_alignment_offset: u64,
    partitions: Vec<Partition>,
}

impl BlockDevice {
    pub fn from_device(dev: RawDevice) -> Self {
        Self {
            dev,
            major: 0,
            minor: 0,
            capability: BlockCap::empty(),
            size: 0,
            alignment_offset: 0,
            discard_alignment_offset: 0,
            partitions: Vec::new(),
        }
    }

    /// Get connected block devices
    ///
    /// # Note
    ///
    /// This skips partitions, which may appear in the block subsystems.
    pub fn get_connected() -> Result<Vec<Self>> {
        Ok(RawDevice::get_connected("block")?
            .into_iter()
            // partition is undocumented.
            .filter(|d| !d.device_path().join("partition").exists())
            .map(BlockDevice::from_device)
            .collect())
    }

    // TODO: Block Device ioctls
}

impl Device for BlockDevice {
    fn refresh(&mut self) -> Result<()> {
        self.dev.refresh()?;
        let (major, minor) = {
            let dev = std::fs::read_to_string(self.device_path().join("dev"))?;
            let mut dev = dev.trim().split(':');
            (
                dev.next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| DeviceError::InvalidDevice("Invalid major"))?,
                dev.next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| DeviceError::InvalidDevice("Invalid minor"))?,
            )
        };
        self.major = major;
        self.minor = minor;
        // Unknown bits are safe, and the kernel may add new flags.
        self.capability = unsafe {
            BlockCap::from_bits_unchecked(
                std::fs::read_to_string(self.device_path().join("capability"))?
                    .trim()
                    .parse()
                    .map_err(|_| DeviceError::InvalidDevice("Invalid capability"))?,
            )
        };
        self.size = std::fs::read_to_string(self.device_path().join("size"))?
            .trim()
            .parse()
            .map_err(|_| DeviceError::InvalidDevice("Invalid size"))?;
        self.alignment_offset =
            std::fs::read_to_string(self.device_path().join("alignment_offset"))?
                .trim()
                .parse()
                .map_err(|_| DeviceError::InvalidDevice("Invalid alignment_offset"))?;

        self.discard_alignment_offset =
            std::fs::read_to_string(self.device_path().join("discard_alignment"))?
                .trim()
                .parse()
                .map_err(|_| DeviceError::InvalidDevice("Invalid discard_alignment"))?;

        self.partitions.clear();
        for dir in read_dir(self.device_path())? {
            let dir: DirEntry = dir?;
            if !dir.path().join("partition").exists() {
                continue;
            }
            self.partitions.push(Partition {
                device_path: dir.path(),
            });
        }

        //
        Ok(())
    }

    fn device_path(&self) -> &Path {
        self.dev.device_path()
    }

    fn subsystem(&self) -> &str {
        self.dev.subsystem()
    }

    fn driver(&self) -> Option<&str> {
        self.dev.driver()
    }

    fn power(&self) -> &Power {
        self.dev.power()
    }
}

impl Block for BlockDevice {
    fn major(&self) -> u32 {
        self.major
    }

    fn minor(&self) -> u32 {
        self.minor
    }

    fn capability(&self) -> BlockCap {
        self.capability
    }

    fn size(&self) -> u64 {
        self.size
    }

    fn alignment_offset(&self) -> u64 {
        self.alignment_offset
    }

    fn discard_alignment_offset(&self) -> u64 {
        self.discard_alignment_offset
    }

    fn partitions(&self) -> &Vec<Partition> {
        &self.partitions
    }
}