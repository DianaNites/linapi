//! Abstraction for handling devices in the block subsystem
//!
//! # Implementation
//!
//! These interfaces are poorly documented, and what does exist is
//! scattered and and inconsistent.
//!
//! See [stable/sysfs-block][1] and [testing/sysfs-block][2]
//!
//! [1]: https://www.kernel.org/doc/Documentation/ABI/stable/sysfs-block
//! [2]: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-block
#![allow(unused_variables, unused_imports, clippy::all, dead_code, unused_mut)]
use std::{
    convert::TryFrom,
    fs,
    io,
    path::{Path, PathBuf},
};

use super::{Device, GenericDevice, SYSFS_PATH};

/// A linux block device
#[derive(Debug, Clone)]
pub struct Block {
    /// Canonical, full, path to the device.
    path: PathBuf,
}

impl Block {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Get connected Block Devices, sorted.
    ///
    /// # Note
    ///
    /// Partitions are **not** included. Use [`Block::partitions`].
    ///
    /// # Errors
    ///
    /// - If unable to read any of the subsystem directories.
    pub fn devices() -> io::Result<Vec<Self>> {
        let sysfs = Path::new(SYSFS_PATH);
        let mut devices = Vec::new();
        // Have to check both paths
        let paths = if sysfs.join("subsystem").exists() {
            vec![sysfs.join("subsystem/block/devices")]
        } else {
            vec![sysfs.join("class/block"), sysfs.join("block")]
        };
        for path in paths {
            if !path.exists() {
                continue;
            }
            for dev in path.read_dir()? {
                let dev = dev?;
                let path = dev.path();
                let dev = path.read_link()?;
                let mut c = dev.components();
                for p in c.by_ref() {
                    if p.as_os_str() == "devices" {
                        break;
                    }
                }
                devices.push(Self::new(
                    Path::new(SYSFS_PATH).join("devices").join(c.as_path()),
                ));
            }
        }
        devices.sort_unstable_by(|a: &Block, b| a.path.cmp(&b.path));
        devices.dedup_by(|a, b| a.path == b.path);
        // Remove sub-devices, partitions in this context.
        devices.dedup_by(|a, b| a.path.starts_with(&b.path));

        Ok(devices)
    }

    /// Get device model, if it exists.
    pub fn model(&self) -> io::Result<Option<String>> {
        let mut parent = self.parent();
        while let Some(dev) = parent {
            let sub = dev.subsystem()?;
            if sub == "nvme" || sub == "scsi" {
                // Note that this file is mostly undocumented
                let model = dev.path().join("model");
                if !model.exists() {
                    return Ok(None);
                }
                return Ok(Some(
                    fs::read_to_string(model).map(|s| s.trim().to_owned())?,
                ));
            }
            parent = dev.parent();
        }
        Ok(None)
    }
}

impl Device for Block {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl TryFrom<GenericDevice> for Block {
    type Error = io::Error;

    fn try_from(dev: GenericDevice) -> Result<Self, Self::Error> {
        if dev.subsystem()? == "block" && !dev.path().join("partition").exists() {
            Ok(Self { path: dev.path })
        } else {
            Err(io::ErrorKind::InvalidInput.into())
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(unreachable_code)]
    use super::*;

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

    #[test]
    fn devices() -> Result<()> {
        let devices = Block::devices();
        // dbg!(&devices);
        for device in devices? {
            dbg!(&device.model());
        }
        panic!();
        Ok(())
    }
}
