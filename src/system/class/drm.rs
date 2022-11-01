//! Abstraction for handling devices in the drm subsystem
//!
//! # Implementation
//!
//! These interfaces are even more poorly documented than usual, and what does
//! exist is scattered and and inconsistent.
//!
//! Much of the interface depends on the GPU driver

use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

use super::{pci::Pci, Device, GenericDevice, SYSFS_PATH};

/// A GPU
#[derive(Debug, Clone)]
pub struct Gpu {
    /// Canonical, full, path to the device.
    path: PathBuf,
}

impl Gpu {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Return the DRM Version
    pub fn version() -> io::Result<String> {
        let sysfs = Path::new(SYSFS_PATH);
        // FIXME: Search all 3 paths..
        fs::read_to_string(sysfs.join("class/drm/version"))
    }

    /// Get connected GPUs, sorted.
    ///
    /// # Note
    ///
    /// ***Only*** GPUs are included in this. Render nodes and
    /// connectors are not.
    ///
    /// # Errors
    ///
    /// - If unable to read any of the subsystem directories.
    pub fn devices() -> io::Result<Vec<Self>> {
        Ok(GenericDevice::devices("drm")?
            .into_iter()
            .filter(|d| !d.kernel_name().starts_with("render"))
            .map(|d| Self::new(d.path))
            .collect())
    }

    /// Get connectors for this GPU
    pub fn connectors(&self) -> io::Result<Vec<Connector>> {
        let mut v = Vec::new();
        for dir in self.path.read_dir()? {
            let dir = dir?;
            let ty = dir.file_type()?;
            if !ty.is_dir() {
                continue;
            }
            let path = dir.path();
            let name = dir.file_name();
            let name = name.to_str().expect("invalid utf-8 in drm kernel names");
            let kernel_name = self.kernel_name();
            if !name.starts_with(kernel_name) {
                continue;
            }
            v.push(Connector::new(path));
        }
        Ok(v)
    }

    /// The parent PCI device of this GPU
    ///
    /// # Errors
    ///
    /// If the parent device no longer exists
    pub fn pci(&self) -> io::Result<Pci> {
        let mut parent = self.parent();
        while let Some(dev) = parent {
            parent = dev.parent();
            if let Ok(gpu) = dev.try_into() {
                return Ok(gpu);
            }
        }
        Err(io::ErrorKind::NotFound.into())
    }
}

impl Device for Gpu {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl TryFrom<GenericDevice> for Gpu {
    type Error = io::Error;

    fn try_from(dev: GenericDevice) -> Result<Self, Self::Error> {
        if dev.subsystem()? == "drm" && !dev.path().join("edid").exists() {
            Ok(Self { path: dev.path })
        } else {
            Err(io::ErrorKind::InvalidInput.into())
        }
    }
}

/// A connector, attached to a GPU.
#[derive(Debug, Clone)]
pub struct Connector {
    path: PathBuf,
}

impl Connector {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The parent GPU of this connector
    ///
    /// # Errors
    ///
    /// If the parent device no longer exists
    pub fn gpu(&self) -> io::Result<Gpu> {
        let mut parent = self.parent();
        while let Some(dev) = parent {
            parent = dev.parent();
            if let Ok(gpu) = dev.try_into() {
                return Ok(gpu);
            }
        }
        Err(io::ErrorKind::NotFound.into())
    }

    /// Name of this connector
    ///
    /// This does not include the card name
    ///
    /// # Example
    ///
    /// If the connector is at `/sys/class/drm/card0/card0-DP-1`, this will
    /// return `DP-1`
    pub fn name(&self) -> &str {
        let name = self.kernel_name();
        let (_, name) = name.split_once('-').expect("invalid connector name");
        name
    }
}

impl Device for Connector {
    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    #![allow(unreachable_code)]
    use super::*;

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

    #[test]
    fn devices() -> Result<()> {
        let devices = Gpu::devices();
        // dbg!(&devices);
        for device in devices? {
            dbg!(&device);
            // dbg!(&device.connectors());
            for con in device.connectors()? {
                dbg!(&con);
                dbg!(&con.gpu());
                dbg!(&con.name());
            }
        }
        panic!();
        Ok(())
    }
}
