//! The linux sysfs contains a lot of different things, but there are some
//! common interfaces, which we define here.
use super::util::{read_uevent, write_uevent};
use bitflags::bitflags;
use std::{
    collections::HashMap,
    fs,
    io::prelude::*,
    path::{Path, PathBuf},
    time::Duration,
};

/// Supported [`UEvent`] actions
pub enum UEventAction {
    Add,
    Remove,
    Change,
}

/// [`DevicePower::control`] Controls
pub enum DevicePowerControl {
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

/// [`DevicePower::status`] Status.
pub enum DevicePowerStatus {
    Suspended,
    Suspending,
    Resuming,
    Active,
    FatalError,
    Unsupported,
}

/// Wakeup information for [`DevicePower::wakeup`]
pub struct DevicePowerWakeup {}

impl DevicePowerWakeup {
    /// Whether this Device is allowed to wake the system up from sleep states.
    ///
    /// If the Device does not support this, [`None`] is returned.
    fn can_wakeup(&self) -> bool {
        todo!()
    }

    /// How many times this Device has signaled a wakeup event.
    fn count(&self) -> u32 {
        todo!()
    }

    /// How many times this Device has completed a wakeup event.
    fn count_active(&self) -> u32 {
        todo!()
    }

    /// How many times this Device has aborted a system sleep state transition.
    fn count_abort(&self) -> u32 {
        todo!()
    }

    /// How many times a wakeup event timed out.
    fn count_expired(&self) -> u32 {
        todo!()
    }

    /// Whether a wakeup event is currently being processed.
    fn active(&self) -> bool {
        todo!()
    }

    /// Total time spent processing wakeup events from this Device.
    fn total_time(&self) -> Duration {
        todo!()
    }

    /// Maximum time spent processing a *single* wakeup event.
    fn max_time(&self) -> Duration {
        todo!()
    }

    /// Value of the monotonic clock corresponding to the time of
    /// signaling the last wakeup event associated with this Device?
    fn last_time(&self) -> Duration {
        todo!()
    }

    /// Total time this Device has prevented the System from transitioning to a
    /// sleep state.
    fn prevent_sleep_time(&self) -> Duration {
        todo!()
    }
}

bitflags! {
    /// Flags corresponding to a Block Devices `capability` file.
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

/// Everything? in sysfs has a uevent file.
///
/// See the [kernel docs][1] for more info
///
/// [1]: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-uevent
pub trait UEvent {
    /// Write a synthetic `uevent`
    fn write(&self, action: UEventAction, uuid: Option<String>, args: HashMap<String, String>);

    /// Return the Key=Value pairs in the `uevent` file.
    fn read(&self) -> HashMap<String, String>;
}

/// Information about one specific Device.
///
/// This is the most general interface, so you can do the least with it.
///
/// This interface is constructed to follow the [sysfs rules][1]
///
/// [1]: https://www.kernel.org/doc/html/latest/admin-guide/sysfs-rules.html
pub trait Device {
    /// The canonical path to the Device.
    ///
    /// # Note
    ///
    /// This is the absolute canonical filesystem path of the Device, so it
    /// includes the leading `/sys`
    fn device_path(&self) -> PathBuf;

    /// Kernel name of the Device, ie `sda`. Identical to the last element of
    /// [`Device::device_path`]
    fn kernel_name(&self) -> String {
        self.device_path()
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .into()
    }

    /// Name of the driver for this Device, or [`None`].
    fn driver(&self) -> Option<String> {
        fs::read_link(self.device_path().join("driver"))
            .map(|s| s.file_stem().unwrap().to_str().unwrap().into())
            .ok()
    }

    /// Name of the subsystem for this Device.
    fn subsystem(&self) -> String {
        fs::read_link(self.device_path().join("subsystem"))
            .map(|s| s.file_stem().unwrap().to_str().unwrap().into())
            .unwrap()
    }
}

/// Device Power Management Interface
///
/// All Devices should have this
///
/// See the [kernel docs][1] for details.
///
/// # Note
///
/// This interface is 'testing' and may change between kernel versions, if a
/// critical flaw is found.
///
/// [1]: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-devices-power
pub trait DevicePower {
    /// Whether this Device is allowed to wake the system up from sleep states.
    ///
    /// If the Device does not support this, [`None`] is returned.
    // TODO: One optional `Wakeup` struct with all info.
    fn can_wakeup(&self) -> Option<bool>;

    /// Current Device control setting
    fn control(&self) -> DevicePowerControl;

    /// How long the device will wait after becoming idle before being
    /// suspended.
    ///
    /// [`None`] is returned if this is unsupported.
    fn autosuspend_delay(&self) -> Option<Duration>;

    /// Current Power Management Status of the Device.
    fn status(&self) -> DevicePowerStatus;

    /// Whether the device is suspended/resumed asynchronously. during
    /// system-wide power transitions.
    ///
    /// This defaults to `false` for most devices.
    fn r#async(&self) -> bool;
}

impl<T> DevicePower for T
where
    T: Device,
{
    fn can_wakeup(&self) -> Option<bool> {
        fs::read_to_string(self.device_path().join("power/wakeup"))
            .map(|s| match s.trim() {
                "enabled" => true,
                "disabled" => false,
                _ => panic!("Unexpected `power/wakeup` value"),
            })
            .ok()
    }
    fn control(&self) -> DevicePowerControl {
        fs::read_to_string(self.device_path().join("power/control"))
            .map(|s| match s.trim() {
                "auto" => DevicePowerControl::Auto,
                "on" => DevicePowerControl::On,
                _ => panic!("Unexpected `power/control` value"),
            })
            .unwrap()
    }
    fn autosuspend_delay(&self) -> Option<Duration> {
        fs::read_to_string(self.device_path().join("power/autosuspend_delay_ms"))
            .map(|s| Duration::from_millis(s.trim().parse().unwrap()))
            .ok()
    }
    fn status(&self) -> DevicePowerStatus {
        fs::read_to_string(self.device_path().join("power/runtime_status"))
            .map(|s| match s.trim() {
                "suspended" => DevicePowerStatus::Suspended,
                "suspending" => DevicePowerStatus::Suspending,
                "resuming" => DevicePowerStatus::Resuming,
                "active" => DevicePowerStatus::Active,
                "error" => DevicePowerStatus::FatalError,
                "unsupported" => DevicePowerStatus::Unsupported,
                _ => panic!("Unexpected `power/runtime_status` value"),
            })
            .unwrap()
    }
    fn r#async(&self) -> bool {
        fs::read_to_string(self.device_path().join("power/async"))
            .map(|s| match s.trim() {
                "enabled" => true,
                "disabled" => false,
                _ => panic!("Unexpected `power/async` value"),
            })
            .unwrap()
    }
}

impl<T> UEvent for T
where
    T: Device,
{
    fn write(&self, action: UEventAction, uuid: Option<String>, args: HashMap<String, String>) {
        write_uevent(&self.device_path().join("uevent"), action, uuid, args)
    }

    fn read(&self) -> HashMap<String, String> {
        read_uevent(&self.device_path().join("uevent"))
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
pub trait BlockDevice: Device {
    /// Major Device Number
    ///
    /// # Note
    ///
    /// This interface uses the `dev` file, which is undocumented.
    fn major(&self) -> u64 {
        fs::read_to_string(self.device_path().join("dev"))
            .map(|s| s.split(':').next().unwrap().parse().unwrap())
            .unwrap()
    }

    /// Minor Device Number
    ///
    /// # Note
    ///
    /// This interface uses the `dev` file, which is undocumented.
    fn minor(&self) -> u64 {
        fs::read_to_string(self.device_path().join("dev"))
            .map(|s| s.rsplit(':').next().unwrap().parse().unwrap())
            .unwrap()
    }

    /// Device capabilities. See [`BlockCap`] for details.
    ///
    /// # Note
    ///
    /// You can use [`BlockCap::bits`] to get the raw value and manually test
    /// flags if need be.
    ///
    /// Unknown flags *are* preserved.
    fn capability(&self) -> BlockCap {
        // Unknown bits are safe, and the kernel may add new flags.
        unsafe {
            BlockCap::from_bits_unchecked(
                fs::read_to_string(self.device_path().join("capability"))
                    .map(|s| s.parse().unwrap())
                    .unwrap(),
            )
        }
    }

    /// Size of the Block Device, in bytes.
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
            .map(|s| s.parse::<u64>().unwrap() * 512)
            .unwrap()
    }

    /// How many bytes the beginning of the device is
    /// offset from the disk's natural alignment.
    fn alignment_offset(&self) -> u64 {
        fs::read_to_string(self.device_path().join("discard_alignment"))
            .map(|s| s.parse().unwrap())
            .unwrap()
    }

    /// How many bytes the beginning of the device is offset from the disk's
    /// natural alignment.
    fn discard_alignment_offset(&self) -> u64 {
        fs::read_to_string(self.device_path().join("discard_alignment"))
            .map(|s| s.parse().unwrap())
            .unwrap()
    }

    /// Partitions this Block Device has
    fn partitions(&self) -> Vec<Box<dyn BlockDevicePartition>> {
        todo!()
    }
}

/// A Partition of a Linux Block Device
pub trait BlockDevicePartition: Device {
    fn parent(&self) -> Box<BlockDevice>;

    /// How many bytes the beginning of the partition is
    /// offset from the disk's natural alignment.
    fn alignment_offset(&self) -> u64 {
        fs::read_to_string(self.device_path().join("alignment_offset"))
            .map(|s| s.parse().unwrap())
            .unwrap()
    }

    /// How many bytes the beginning of the partition is offset from the
    /// disk's natural alignment.
    fn discard_alignment_offset(&self) -> u64 {
        fs::read_to_string(self.device_path().join("discard_alignment"))
            .map(|s| s.parse().unwrap())
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
            .map(|s| s.parse::<u64>().unwrap() * 512)
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
            .map(|s| s.parse::<u64>().unwrap() * 512)
            .unwrap()
    }
}
