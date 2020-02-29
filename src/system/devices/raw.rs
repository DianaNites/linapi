//! Raw device Interface.
//!
//! Not much can be done without knowing what kind of device it is,
//! so you probably don't want to use this module directly.
use crate::util;
use std::{path::Path, time::Duration};

pub type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

/// Wakeup information for [`DevicePower::wakeup`]
#[derive(Debug)]
pub struct Wakeup {
    pub(crate) can_wakeup: bool,
    pub(crate) count: u32,
    pub(crate) count_active: u32,
}

impl Wakeup {
    /// Whether this Device is allowed to wake the system up from sleep
    /// states.
    pub fn can_wakeup(&self) -> bool {
        self.can_wakeup
    }

    /// How many times this Device has signaled a wakeup event.
    pub fn count(&self) -> u32 {
        self.count
    }

    /// How many times this Device has completed a wakeup event.
    pub fn count_active(&self) -> u32 {
        self.count_active
    }

    /// How many times this Device has aborted a system sleep state
    /// transition.
    fn _count_abort(&self) -> u32 {
        todo!()
    }

    /// How many times a wakeup event timed out.
    fn _count_expired(&self) -> u32 {
        todo!()
    }

    /// Whether a wakeup event is currently being processed.
    fn _active(&self) -> bool {
        todo!()
    }

    /// Total time spent processing wakeup events from this Device.
    fn _total_time(&self) -> Duration {
        todo!()
    }

    /// Maximum time spent processing a *single* wakeup event.
    fn _max_time(&self) -> Duration {
        todo!()
    }

    /// Value of the monotonic clock corresponding to the time of
    /// signaling the last wakeup event associated with this Device?
    fn _last_time(&self) -> Duration {
        todo!()
    }

    /// Total time this Device has prevented the System from transitioning
    /// to a sleep state.
    fn _prevent_sleep_time(&self) -> Duration {
        todo!()
    }
}

/// Device Power Management
///
/// See the [kernel docs][1] for details.
///
/// [1]: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-devices-power
#[derive(Debug)]
pub struct Power {
    autosuspend_delay: Option<Duration>,
    async_: bool,
    wakeup: Option<Wakeup>,
}

// Public
impl Power {
    /// Wakeup information.
    ///
    /// If this device is capable of waking the system up from sleep states,
    /// [`Some`] is returned.
    ///
    /// If the Device does not support this, [`None`] is returned.
    pub fn wakeup(&self) -> Option<&Wakeup> {
        self.wakeup.as_ref()
    }

    /// How long the device will wait after becoming idle before being
    /// suspended.
    ///
    /// [`None`] is returned if this is unsupported.
    pub fn autosuspend_delay(&self) -> Option<Duration> {
        self.autosuspend_delay
    }

    /// Whether the device is suspended/resumed asynchronously. during
    /// system-wide power transitions.
    ///
    /// This defaults to `false` for most devices.
    pub fn async_(&self) -> bool {
        self.async_
    }
}

// Private
impl Power {
    fn _new(path: &Path) -> Result<Self> {
        Ok(Power {
            autosuspend_delay: util::read_power_autosuspend_delay(path)?,
            async_: util::read_power_async(path)?,
            wakeup: util::read_power_wakeup(path)?,
        })
    }
}
