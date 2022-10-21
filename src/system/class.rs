//! Abstractions for handling certain classes of device
//!
//! A "class" is a specific kernel subsystem
//!
//! Within the kernel, these distinctions do not exist and everything is
//! just a [`Device`].
//!
//! See the [sysfs rules][1] and [sysfs-devices][2] file for details
//!
//! [1]: https://www.kernel.org/doc/html/latest/admin-guide/sysfs-rules.html
//! [2]: https://www.kernel.org/doc/Documentation/ABI/stable/sysfs-devices
#![allow(unused_variables, unused_imports, clippy::all, dead_code)]
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fmt::Debug,
    fs::{DirEntry, ReadDir},
    io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use self::imp::Sealed;

pub mod block;

mod imp {
    #![allow(unused_imports)]
    use super::*;

    pub trait Sealed {}

    impl Sealed for block::Block {}
    impl Sealed for GenericDevice {}

    pub fn read_attrs(path: &Path, buf: &mut Vec<PathBuf>) -> io::Result<()> {
        for dir in path.read_dir()? {
            let dir = dir?;
            let ty = dir.file_type()?;
            let path = dir.path();
            if ty.is_symlink() {
                continue;
            }
            if ty.is_dir() {
                let _ = read_attrs(&path, buf);
            }
            buf.push(path);
        }
        Ok(())
    }
}
use imp::read_attrs;

/// A kernel "Device"
///
/// Exposes the lower level information underlying every kernel device
pub trait Device: Sealed {
    /// Full path to the device
    ///
    /// # Example
    ///
    /// `/sys<devpath>`
    ///
    /// `/sys/devices/pci0000:00/0000:00:08.1/0000:08:00.0/drm/card1`
    fn path(&self) -> &Path;

    /// Unique key identifying the device under sysfs.
    ///
    /// Always starts with a `/`.
    ///
    /// # Example
    ///
    /// `/devices/pci0000:00/0000:00:08.1/0000:08:00.0/drm/card1`
    fn devpath(&self) -> &OsStr {
        // TODO: This will only work for `/sys`
        // Yeah yeah linux says its a system configuration error to not have
        // sysfs at `/sys` but I don't care and will make a distro that has it
        // elsewhere.
        OsStr::from_bytes(&self.path().as_os_str().as_bytes()[4..])
    }

    /// Kernel name of the device.
    ///
    /// Identical to the last component of [`Device::devpath`]
    ///
    /// # Example
    ///
    /// `card1`
    fn kernel_name(&self) -> &OsStr {
        self.path().file_name().expect("devpath cannot end in ..")
    }

    /// Kernel subsystem
    ///
    /// # Errors
    ///
    /// This can fail if you dont have permission to access the device
    /// directory.
    ///
    /// # Example
    ///
    /// `drm`
    fn subsystem(&self) -> io::Result<OsString> {
        Ok(self
            .path()
            .join("subsystem")
            .read_link()?
            .file_name()
            .expect("subsystem cannot end in ..")
            .to_os_string())
    }

    /// Driver for this device
    ///
    /// Returns [`None`] if this device has no driver currently associated with
    /// it
    ///
    /// # Errors
    ///
    /// This can fail if you dont have permission to access the device
    /// directory.
    ///
    /// # Example
    ///
    /// `drm`
    fn driver(&self) -> io::Result<Option<OsString>> {
        self.subsystem()?;
        Ok(self.path().join("driver").read_link().ok().map(|f| {
            f.file_name()
                .expect("driver cannot end in ..")
                .to_os_string()
        }))
    }

    /// Returns the path to every visible attribute, recursively, sorted.
    ///
    /// This list includes directories, which you may not have permission to
    /// list the contents of.
    ///
    /// There may be attributes you do not have permission to see
    ///
    /// Use [`Path::strip_prefix`] with [`Device::path`] to get just the
    /// attribute path
    ///
    /// # Example
    ///
    /// \[`<path>/mq`, `<path>/mq/0`, `<path>/mq/0/cpu_list`]
    fn attributes(&self) -> io::Result<Vec<PathBuf>> {
        let mut v = Vec::new();
        read_attrs(&self.path(), &mut v)?;
        v.sort_unstable();
        Ok(v)
    }

    /// Returns the parent device, if it exists
    fn parent(&self) -> Option<GenericDevice> {
        while let Some(parent) = self.path().parent() {
            if parent.join("subsystem").exists() {
                return GenericDevice::new(parent).ok();
            }
        }
        None
    }
}

/// A generic linux [`Device`]
#[derive(Debug)]
pub struct GenericDevice {
    path: PathBuf,
}

impl GenericDevice {
    /// Create a new [`Device`] from `path`, resolving symlinks.
    ///
    /// # Errors
    ///
    /// If `path` is not a device under sysfs
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let path = path.canonicalize()?;
        // FIXME: only works with `/sys`
        if path.starts_with("/sys/devices") {
            Ok(Self { path })
        } else {
            Err(io::ErrorKind::InvalidInput.into())
        }
    }
}

impl Device for GenericDevice {
    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

    #[test]
    fn attributes() -> Result<()> {
        let dev = GenericDevice::new("/sys/block/nvme1n1/")?;
        let _ = dbg!(dev.subsystem());
        let _ = dbg!(dev.driver());
        for attr in dev.attributes() {
            // dbg!(&attr);
            // dbg!(&attr.map(|a| a.name()));
        }
        panic!();
        // Ok(())
    }

    #[test]
    #[cfg(no)]
    fn devpath() {
        let _path = Path::new(
            "/System/devices/kernel/devices/pci0000:00/0000:00:08.1/0000:08:00.0/drm/card1",
        );
        let path = Path::new("/sys/devices/pci0000:00/0000:00:08.1/0000:08:00.0/drm/card1");
        panic!();
    }
}
