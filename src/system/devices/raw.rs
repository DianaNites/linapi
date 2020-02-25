//! Raw device Interface.
//!
//! Not much can be done without knowing what kind of device it is,
//! so you probably don't want to use this module directly.
use crate::{error::DeviceError, util, util::SYSFS_PATH};
use std::{
    fs::{read_dir, DirEntry},
    path::{Path, PathBuf},
    time::Duration,
};

pub type Result<T, E = DeviceError> = std::result::Result<T, E>;

#[derive(Debug, Copy, Clone)]
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

#[derive(Debug, Copy, Clone)]
pub enum Status {
    Suspended,
    Suspending,
    Resuming,
    Active,
    FatalError,
    Unsupported,
}

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

#[derive(Debug)]
pub struct Power {
    pub(crate) control: Control,
    pub(crate) autosuspend_delay: Option<Duration>,
    pub(crate) status: Status,
    pub(crate) async_: bool,
    pub(crate) wakeup: Option<Wakeup>,
}

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

    /// Current Device control setting
    pub fn control(&self) -> Control {
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
    pub fn status(&self) -> Status {
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
    ///
    /// # Note
    ///
    /// As this information is from the filesystem, it is not atomic or
    /// representative of a specific moment in time.
    /// Linux provides no way to do that.
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
    fn power(&self) -> &Power;
}

/// Represents one specific Device
#[derive(Debug)]
pub struct RawDevice {
    path: PathBuf,
    subsystem: Option<String>,
    driver: Option<String>,
    name: Option<String>,
    power: Option<Power>,
}

impl RawDevice {
    /// Get connected devices by their subsystem name
    pub fn get_connected(subsystem: &str) -> Result<Vec<Self>> {
        let sysfs = Path::new(SYSFS_PATH);
        let mut devices = Vec::new();
        //
        let path = sysfs.join("subsystem").join(subsystem).join("devices");
        if path.exists() {
            for dev in read_dir(path).unwrap() {
                let dev: DirEntry = dev.unwrap();
                let mut s = Self {
                    path: dev.path().canonicalize().unwrap(),
                    subsystem: None,
                    driver: None,
                    name: None,
                    power: None,
                };
                s.refresh()?;
                devices.push(s);
            }
        } else {
            let paths = &[
                sysfs.join("class").join(subsystem),
                // `/sys/bus/<subsystem>` is laid out differently from
                // `/sys/class/<subsystem>`
                sysfs.join("bus").join(subsystem).join("devices"),
            ];
            for path in paths {
                let path: &Path = path;
                if path.exists() {
                    for dev in read_dir(path).unwrap() {
                        let dev: DirEntry = dev.unwrap();
                        let mut s = Self {
                            path: dev.path().canonicalize().unwrap(),
                            subsystem: None,
                            driver: None,
                            name: None,
                            power: None,
                        };
                        s.refresh()?;
                        devices.push(s);
                    }
                }
            }
        }
        Ok(devices)
    }
}

impl Device for RawDevice {
    fn device_path(&self) -> &Path {
        &self.path
    }

    fn refresh(&mut self) -> Result<()> {
        self.subsystem = Some(util::read_subsystem(&self.path)?);
        self.driver = util::read_driver(&self.path)?;
        self.power = Some(Power {
            control: util::read_power_control(&self.path)?,
            autosuspend_delay: util::read_power_autosuspend_delay(&self.path)?,
            status: util::read_power_status(&self.path)?,
            async_: util::read_power_async(&self.path)?,
            wakeup: util::read_power_wakeup(&self.path)?,
        });
        Ok(())
    }

    fn subsystem(&self) -> &str {
        // Unwrap should be okay, `refresh` sets it.
        self.subsystem.as_ref().unwrap()
    }

    fn driver(&self) -> Option<&str> {
        self.driver.as_deref()
    }

    fn power(&self) -> &Power {
        // Should be okay, `refresh` sets it.
        self.power.as_ref().unwrap()
    }
}
