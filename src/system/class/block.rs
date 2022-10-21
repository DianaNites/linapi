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

use std::{
    convert::TryFrom,
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
};

use super::{Device, GenericDevice};

// use super::Device;

/// A linux block device
#[derive(Debug, Clone)]
pub struct Block {
    /// Canonical, full, path to the device.
    path: PathBuf,
}

impl Device for Block {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl TryFrom<GenericDevice> for Block {
    type Error = io::Error;

    fn try_from(dev: GenericDevice) -> Result<Self, Self::Error> {
        if dev.subsystem()? == "block" {
            Ok(Self { path: dev.path })
        } else {
            Err(io::ErrorKind::InvalidInput.into())
        }
    }
}

// pub struct TryFromDeviceError {}
