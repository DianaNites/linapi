//! Interfaces common to all devices
use std::{fs, path::PathBuf, time::Duration};

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

mod _ignore {
    #![allow(dead_code)]
    use super::*;

    /// Wakeup information for [`DevicePower::wakeup`]
    pub struct DevicePowerWakeup {}

    impl DevicePowerWakeup {
        /// Whether this Device is allowed to wake the system up from sleep
        /// states.
        ///
        /// If the Device does not support this, [`None`] is returned.
        pub fn can_wakeup(&self) -> bool {
            todo!()
        }

        /// How many times this Device has signaled a wakeup event.
        pub fn count(&self) -> u32 {
            todo!()
        }

        /// How many times this Device has completed a wakeup event.
        pub fn count_active(&self) -> u32 {
            todo!()
        }

        /// How many times this Device has aborted a system sleep state
        /// transition.
        pub fn count_abort(&self) -> u32 {
            todo!()
        }

        /// How many times a wakeup event timed out.
        pub fn count_expired(&self) -> u32 {
            todo!()
        }

        /// Whether a wakeup event is currently being processed.
        pub fn active(&self) -> bool {
            todo!()
        }

        /// Total time spent processing wakeup events from this Device.
        pub fn total_time(&self) -> Duration {
            todo!()
        }

        /// Maximum time spent processing a *single* wakeup event.
        pub fn max_time(&self) -> Duration {
            todo!()
        }

        /// Value of the monotonic clock corresponding to the time of
        /// signaling the last wakeup event associated with this Device?
        pub fn last_time(&self) -> Duration {
            todo!()
        }

        /// Total time this Device has prevented the System from transitioning
        /// to a sleep state.
        pub fn prevent_sleep_time(&self) -> Duration {
            todo!()
        }
    }
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

/// All devices have power information
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
