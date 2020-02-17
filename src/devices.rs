//! Interface to devices on linux
//!
//! Linux primarily exposes connected devices through sysfs,
//! most of those interfaces undocumented.
use crate::types::{BlockDevice as BlockDeviceTrait, Device as DeviceTrait, SYSFS_PATH};
use std::{
    fs::{read_dir, DirEntry},
    path::{Path, PathBuf},
};

/// Represents one specific Device
#[derive(Debug)]
pub struct Device {
    path: PathBuf,
}

impl Device {
    /// Get connected devices by their subsystem name
    pub fn get_connected(subsystem: &str) -> Vec<Device> {
        let sysfs = Path::new(SYSFS_PATH);
        let mut devices = Vec::new();
        //
        let path = sysfs.join("subsystem").join(subsystem).join("devices");
        if path.exists() {
            for dev in read_dir(path).unwrap() {
                let dev: DirEntry = dev.unwrap();
                devices.push(Self {
                    path: dev.path().canonicalize().unwrap(),
                })
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
                        devices.push(Self {
                            path: dev.path().canonicalize().unwrap(),
                        })
                    }
                }
            }
        }
        devices
    }
}

impl DeviceTrait for Device {
    fn device_path(&self) -> &Path {
        &self.path
    }
}

/// Represents a Block Device
#[derive(Debug)]
pub struct BlockDevice {
    path: PathBuf,
}

impl BlockDevice {
    pub fn from_device(dev: Device) -> Self {
        Self { path: dev.path }
    }

    /// Get connected block devices
    ///
    /// # Note
    ///
    /// This skips partitions, which may appear in the block subsystems.
    pub fn get_connected() -> Vec<Self> {
        Device::get_connected("block")
            .into_iter()
            .filter(|d| d.device_path().join("partition").exists())
            .map(BlockDevice::from_device)
            .collect()
    }

    // TODO: Block Device ioctls
}

impl DeviceTrait for BlockDevice {
    fn device_path(&self) -> &Path {
        &self.path
    }
}

impl BlockDeviceTrait for BlockDevice {}
