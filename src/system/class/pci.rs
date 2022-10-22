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

pub struct Pci {
    path: PathBuf,
}

impl Pci {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Get connected PCI Devices, sorted.
    ///
    /// This includes sub-devices?
    ///
    /// # Errors
    ///
    /// - If unable to read any of the subsystem directories.
    // FIXME: Should it include sub-devices? nothing else does..
    // Does it actually??
    // need to add an api to Device that gets sub-devices
    pub fn devices() -> io::Result<Vec<Self>> {
        let sysfs = Path::new(SYSFS_PATH);
        let mut devices = Vec::new();
        // Have to check both paths
        let paths = if sysfs.join("subsystem").exists() {
            vec![sysfs.join("subsystem/pci/devices")]
        } else {
            vec![sysfs.join("bus/pci/devices"), sysfs.join("class/pci")]
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
        // devices.sort_unstable_by(|a, b| a.kernel_name().cmp(b.kernel_name()));
        devices.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        devices.dedup_by(|a, b| a.path == b.path);
        // Remove sub-devices
        // devices.dedup_by(|a, b| a.path.starts_with(&b.path));

        Ok(devices)
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
            let label = fs::read_to_string(label)?;
            Ok(Some(label[..label.len() - 1].to_owned()))
        } else {
            Ok(None)
        }
    }

    /// PCI Modalias value
    pub fn modalias(&self) -> io::Result<String> {
        let modalias = fs::read_to_string(self.path.join("modalias"))?;
        let modalias = &modalias[..modalias.len() - 1];
        Ok(modalias.to_owned())
    }

    // PCI irq value
    pub fn irq(&self) -> io::Result<String> {
        let irq = fs::read_to_string(self.path.join("irq"))?;
        let irq = &irq[..irq.len() - 1];
        Ok(irq.to_owned())
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
