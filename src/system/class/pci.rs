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

use super::{Device, GenericDevice, SYSFS_PATH};

#[derive(Debug, Clone)]
pub struct Pci {
    path: PathBuf,
}

impl Pci {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Get connected PCI Devices, sorted.
    ///
    /// This **does not** include child devices.
    ///
    /// # Errors
    ///
    /// - If unable to read any of the subsystem directories.
    pub fn devices() -> io::Result<Vec<Self>> {
        Ok(GenericDevice::devices("pci")?
            .into_iter()
            .map(|d| Self::new(d.path))
            .collect())
    }

    /// PCI Device Class
    ///
    /// In the form of (Class ID, sub class ID, prog-if ID)
    pub fn class(&self) -> io::Result<(String, String, String)> {
        let class_id = fs::read_to_string(self.path.join("class"))?;
        let class_id = &class_id[2..8];
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

    /// Firmware name of the device, if it exists.
    pub fn label(&self) -> io::Result<Option<String>> {
        let label = self.path().join("label");
        if label.try_exists()? {
            let mut label = fs::read_to_string(label)?;
            label.pop();
            Ok(Some(label))
        } else {
            Ok(None)
        }
    }

    /// PCI Modalias value
    pub fn modalias(&self) -> io::Result<String> {
        let mut modalias = fs::read_to_string(self.path.join("modalias"))?;
        modalias.pop();
        Ok(modalias)
    }

    // PCI irq value
    pub fn irq(&self) -> io::Result<String> {
        let mut irq = fs::read_to_string(self.path.join("irq"))?;
        irq.pop();
        Ok(irq)
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
