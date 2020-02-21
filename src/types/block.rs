//! Interfaces common to Block devices
use super::device::Device;
use bitflags::bitflags;
use std::fs;

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
pub trait BlockDevice: Device {
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
    fn partitions(&self) -> Vec<Box<dyn BlockDevicePartition>>;
}

/// A Partition of a Linux Block Device
pub trait BlockDevicePartition: Device {
    fn parent(&self) -> Box<dyn BlockDevice>;

    /// How many bytes the beginning of the partition is
    /// offset from the disk's natural alignment.
    fn alignment_offset(&self) -> u64 {
        fs::read_to_string(self.device_path().join("alignment_offset"))
            .map(|s| s.trim().parse().unwrap())
            .unwrap()
    }

    /// How many bytes the beginning of the partition is offset from the
    /// disk's natural alignment.
    fn discard_alignment_offset(&self) -> u64 {
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
    fn size(&self) -> u64 {
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
    fn start(&self) -> u64 {
        fs::read_to_string(self.device_path().join("start"))
            .map(|s| s.trim().parse::<u64>().unwrap() * 512)
            .unwrap()
    }
}
