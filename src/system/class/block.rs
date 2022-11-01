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
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use super::{Device, GenericDevice, SYSFS_PATH};

fn dev_size(path: &Path) -> io::Result<u64> {
    Ok(fs::read_to_string(path.join("size"))?
        .trim()
        .parse::<u64>()
        // Per [this][1] forgotten 2015 patch, this is in 512 byte sectors.
        // [1]: https://lore.kernel.org/lkml/1451154995-4686-1-git-send-email-peter@lekensteyn.nl/
        .map(|b| b * 512)
        .map_err(|_| ErrorKind::InvalidData)?)
}

bitflags::bitflags! {
    /// Flags corresponding to [`Block::capability`].
    ///
    /// See the [linux kernel docs][1] for details.
    ///
    /// # Note
    ///
    /// Most of these seem to officially be undocumented.
    /// They will be documented here on a best-effort basis.
    ///
    /// [1]: https://www.kernel.org/doc/html/latest/block/capability.html
    pub struct BlockCap: u32 {
        /// Set for removable media with permanent block devices
        ///
        /// Unset for removable block devices with permanent media
        const REMOVABLE = 1;

        /// Block Device supports Asynchronous Notification of media change events.
        /// These events will be broadcast to user space via kernel uevent.
        const MEDIA_CHANGE_NOTIFY = 4;

        /// CD-like
        const CD = 8;

        /// Alive, online, active.
        const UP = 16;

        /// Doesn't appear in `/proc/partitions`
        const SUPPRESS_PARTITION_INFO = 32;

        /// Unknown
        const EXT_DEVT = 64;

        /// Unknown
        const NATIVE_CAPACITY = 128;

        /// Unknown
        const BLOCK_EVENTS_ON_EXCL_WRITE = 256;

        /// Unknown
        const NO_PART_SCAN = 512;

        /// Unknown
        const HIDDEN = 1024;
    }
}

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

    /// Get device capabilities.
    ///
    /// Unknown flags *are* preserved
    ///
    /// See [`BlockCap`] for more details.
    pub fn capability(&self) -> io::Result<BlockCap> {
        // SAFETY: Bitflags is broken and uses unsafe incorrectly. this is always safe.
        unsafe {
            Ok(BlockCap::from_bits_unchecked(
                std::fs::read_to_string(self.path.join("capability"))?
                    .trim()
                    .parse()
                    .map_err(|_| ErrorKind::InvalidData)?,
            ))
        }
    }

    /// Get the byte size of the device, if possible.
    pub fn size(&self) -> io::Result<u64> {
        dev_size(&self.path)
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
    fn children() -> Result<()> {
        let devices = Block::devices();
        // dbg!(&devices);
        for device in devices? {
            // dbg!(&device.model());
            dbg!(&device);
            for child in device.children()? {
                let child = child?;
                dbg!(&child);
            }
            eprintln!("----\n");
        }
        panic!();
        Ok(())
    }

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
