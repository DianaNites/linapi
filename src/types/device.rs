//! Interfaces common to all devices
use crate::error::DeviceError;
use std::{path::Path, time::Duration};

pub type Result<T, E = DeviceError> = std::result::Result<T, E>;

/// [`DevicePower::control`] Controls
#[derive(Debug, Copy, Clone)]
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
#[derive(Debug, Copy, Clone)]
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

/// Describes a Linux Device.
///
/// This is the most general interface, so you can do the least with it.
///
/// This interface is constructed to follow the [sysfs rules][1].
///
/// Some basic information about the Device *should* be read on
/// construction through the [`Device::refresh`] method.
///
/// [1]: https://www.kernel.org/doc/html/latest/admin-guide/sysfs-rules.html
pub trait Device {
    /// Refresh information on a Device.
    fn refresh(&mut self) -> Result<()>;

    /// The canonical path to the Device.
    ///
    /// # Note
    ///
    /// This is the absolute canonical filesystem path of the Device, so it
    /// includes the leading `/sys`
    fn device_path(&self) -> &Path;

    /// Kernel name of the Device, ie `sda`. Identical to the last element of
    /// [`Device::device_path`]
    fn kernel_name(&self) -> &str {
        // Unwraps should be okay, if not it means `device_path` is invalid.
        self.device_path().file_stem().unwrap().to_str().unwrap()
    }

    /// Name of the driver for this Device, or [`None`].
    fn driver(&self) -> Option<&str>;

    /// Name of the subsystem for this Device.
    fn subsystem(&self) -> &str;

    /// Device Power Management
    ///
    /// See the [kernel docs][1] for details.
    ///
    /// [1]: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-devices-power
    fn power(&self) -> &PowerInfo;
}

#[derive(Debug)]
pub struct PowerInfo {
    pub(crate) control: DevicePowerControl,
    pub(crate) autosuspend_delay: Option<Duration>,
    pub(crate) status: DevicePowerStatus,
    pub(crate) async_: bool,
}

impl PowerInfo {
    //     /// Whether this Device is allowed to wake the system up from sleep
    // states.     /// If the Device does not support this, [`None`] is returned.
    //     ///
    //     /// # Note
    //     ///
    //     /// This is a temporary kludge API
    //     pub fn can_wakeup(&self) -> Option<bool> {
    //         fs::read_to_string(self.device_path.join("power/wakeup"))
    //             .map(|s| match s.trim() {
    //                 "enabled" => true,
    //                 "disabled" => false,
    //                 _ => panic!("Unexpected `power/wakeup` value"),
    //             })
    //             .ok()
    //     }

    /// Wakeup information.
    ///
    /// If this device is capable of waking the system up from sleep states,
    /// [`Some`] is returned.
    ///
    /// If the Device does not support this, [`None`] is returned.
    // TODO: Implement this
    fn _wakeup(&self) -> Option<()> {
        todo!()
    }

    /// Current Device control setting
    pub fn control(&self) -> DevicePowerControl {
        self.control
    }

    /// How long the device will wait after becoming idle before being
    /// suspended.
    ///
    /// [`None`] is returned if this is unsupported.
    pub fn autosuspend_delay(&self) -> Option<Duration> {
        self.autosuspend_delay
    }

    /// Current Power Management Status of the Device.
    pub fn status(&self) -> DevicePowerStatus {
        self.status
    }

    /// Whether the device is suspended/resumed asynchronously. during
    /// system-wide power transitions.
    ///
    /// This defaults to `false` for most devices.
    pub fn async_(&self) -> bool {
        self.async_
    }
}
