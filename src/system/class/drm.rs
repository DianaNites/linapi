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
        let sysfs = Path::new(SYSFS_PATH);
        let mut devices = Vec::new();
        // Have to check both paths
        let paths = if sysfs.join("subsystem").exists() {
            vec![sysfs.join("subsystem/drm/devices")]
        } else {
            vec![sysfs.join("class/drm"), sysfs.join("bus/drm/devices")]
        };
        for path in paths {
            if !path.exists() {
                continue;
            }
            for dev in path.read_dir()? {
                let dev = dev?;
                let path = dev.path();
                let name = dev.file_name();
                let name = name.to_str().expect("invalid utf-8 in drm kernel names");
                let ty = dev.file_type()?;
                if !ty.is_symlink() || name.starts_with("render") {
                    continue;
                }
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
        devices.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        devices.dedup_by(|a, b| a.path == b.path);
        // Remove sub-devices, connectors in this context.
        devices.dedup_by(|a, b| a.path.starts_with(&b.path));

        Ok(devices)
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
