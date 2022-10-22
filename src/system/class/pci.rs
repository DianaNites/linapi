//! Abstraction for handling devices in the pci subsystem
//!
//! # Implementation
//!
//! These interfaces are poorly documented, and much of the interface will
//! depend on the specific PCI device.
//!
//! Much of the interface depends on the GPU driver

use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

use super::{Device, GenericDevice};

pub struct Pci {
    path: PathBuf,
}

impl Pci {
    /// PCI Device Class
    ///
    /// In the form of (Class ID, sub class ID, prog-if ID)
    pub fn class(&self) -> io::Result<(String, String, String)> {
        let class_id = fs::read_to_string(self.path.join("class"))?;
        Ok((
            class_id[..2].to_owned(),
            class_id[2..4].to_owned(),
            class_id[4..6].to_owned(),
        ))
    }

    /// PCI Device Revision
    pub fn revision(&self) -> io::Result<String> {
        let revision = fs::read_to_string(self.path.join("revision"))?;
        Ok(revision[2..4].to_owned())
    }

    /// PCI Device ID
    pub fn device(&self) -> io::Result<String> {
        let revision = fs::read_to_string(self.path.join("device"))?;
        Ok(revision[2..6].to_owned())
    }

    /// PCI Device vendor
    pub fn vendor(&self) -> io::Result<String> {
        let revision = fs::read_to_string(self.path.join("vendor"))?;
        Ok(revision[2..6].to_owned())
    }

    /// PCI Device subsystem vendor ID
    pub fn subsystem_vendor(&self) -> io::Result<String> {
        let revision = fs::read_to_string(self.path.join("subsystem_vendor"))?;
        Ok(revision[2..6].to_owned())
    }

    /// PCI Device subsystem ID
    pub fn subsystem_device(&self) -> io::Result<String> {
        let revision = fs::read_to_string(self.path.join("subsystem_device"))?;
        Ok(revision[2..6].to_owned())
    }
}

impl Device for Pci {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl TryFrom<GenericDevice> for Pci {
    type Error = io::Error;

    fn try_from(dev: GenericDevice) -> Result<Self, Self::Error> {
        if dev.subsystem()? == "pci" {
            Ok(Self { path: dev.path })
        } else {
            Err(io::ErrorKind::InvalidInput.into())
        }
    }
}
