//! Interface to devices on linux
//!
//! Linux primarily exposes connected devices through sysfs,
//! most of those interfaces undocumented.
use crate::types::{
    util,
    BlockDevice as BlockDeviceTrait,
    Device as DeviceTrait,
    Result,
    SYSFS_PATH,
};
use std::{
    fs::{read_dir, DirEntry},
    path::{Path, PathBuf},
};

/// Represents one specific Device
#[derive(Debug)]
pub struct Device {
    path: PathBuf,
    subsystem: Option<String>,
    driver: Option<String>,
    name: Option<String>,
    power: Option<crate::types::PowerInfo>,
}

impl Device {
    /// Get connected devices by their subsystem name
    pub fn get_connected(subsystem: &str) -> Result<Vec<Device>> {
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

impl DeviceTrait for Device {
    fn device_path(&self) -> &Path {
        &self.path
    }

    fn refresh(&mut self) -> Result<()> {
        self.subsystem = Some(util::read_subsystem(&self.path)?);
        self.driver = util::read_driver(&self.path)?;
        self.power = Some(crate::types::PowerInfo {
            control: util::read_power_control(&self.path)?,
            autosuspend_delay: util::read_power_autosuspend_delay(&self.path)?,
            status: util::read_power_status(&self.path)?,
            async_: util::read_power_async(&self.path)?,
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

    fn power(&self) -> &crate::types::PowerInfo {
        // Should be okay, `refresh` sets it.
        self.power.as_ref().unwrap()
    }
}

/// Represents a Block Device
#[derive(Debug)]
pub struct BlockDevice(Device);

impl BlockDevice {
    pub fn from_device(dev: Device) -> Self {
        Self(dev)
    }

    /// Get connected block devices
    ///
    /// # Note
    ///
    /// This skips partitions, which may appear in the block subsystems.
    pub fn get_connected() -> Result<Vec<Self>> {
        Ok(Device::get_connected("block")?
            .into_iter()
            // partition is undocumented.
            .filter(|d| !d.device_path().join("partition").exists())
            .map(BlockDevice::from_device)
            .collect())
    }

    // TODO: Block Device ioctls
}

impl DeviceTrait for BlockDevice {
    fn refresh(&mut self) -> Result<()> {
        self.0.refresh()?;
        todo!()
    }

    fn device_path(&self) -> &Path {
        self.0.device_path()
    }

    fn subsystem(&self) -> &str {
        self.0.subsystem()
    }

    fn driver(&self) -> Option<&str> {
        self.0.driver()
    }

    fn power(&self) -> &crate::types::PowerInfo {
        self.0.power()
    }
}

impl BlockDeviceTrait for BlockDevice {}
