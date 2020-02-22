//! Interface to devices on linux
//!
//! Linux primarily exposes connected devices through sysfs,
//! most of those interfaces undocumented.
use crate::{
    error::DeviceError,
    types::{util, BlockCap, BlockDevice as BlockDeviceTrait, Device, Result, SYSFS_PATH},
};
use std::{
    fs::{read_dir, DirEntry},
    path::{Path, PathBuf},
};

/// Represents one specific Device
#[derive(Debug)]
pub struct RawDevice {
    path: PathBuf,
    subsystem: Option<String>,
    driver: Option<String>,
    name: Option<String>,
    power: Option<crate::types::PowerInfo>,
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
        self.power = Some(crate::types::PowerInfo {
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

    fn power(&self) -> &crate::types::PowerInfo {
        // Should be okay, `refresh` sets it.
        self.power.as_ref().unwrap()
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
    partitions: Vec<Box<dyn crate::types::BlockDevicePartition>>,
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

        for dir in read_dir(self.device_path())? {
            let dir: DirEntry = dir?;
            if !dir.path().join("partition").exists() {
                continue;
            }
            // self.partitions.push(Box::new());
        }
        todo!();

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

    fn power(&self) -> &crate::types::PowerInfo {
        self.dev.power()
    }
}

impl BlockDeviceTrait for BlockDevice {
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

    fn partitions(&self) -> &Vec<Box<dyn crate::types::BlockDevicePartition>> {
        &self.partitions
    }
}
