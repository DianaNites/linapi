//! Abstractions for handling certain classes of device
//!
//! A "class" is a specific kernel subsystem
//!
//! Within the kernel, these distinctions do not exist and everything is
//! just a [`Device`].
//!
//! See the [sysfs rules][1] for details
//!
//! [1]: https://www.kernel.org/doc/html/latest/admin-guide/sysfs-rules.html
#![allow(unused_variables, unused_imports, clippy::all, dead_code)]
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
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
}

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
    /// # Example
    ///
    /// `drm`
    fn subsystem(&self) -> OsString {
        self.path()
            .join("subsystem")
            .read_link()
            .expect("subsystem cannot be missing")
            .file_name()
            .expect("subsystem cannot end in ..")
            .to_os_string()
    }

    /// Driver for this device
    ///
    /// Returns [`None`] if this device has no driver currently associated with
    /// it
    ///
    /// # Example
    ///
    /// `drm`
    fn driver(&self) -> Option<OsString> {
        self.path().join("driver").read_link().ok().map(|f| {
            f.file_name()
                .expect("driver cannot end in ..")
                .to_os_string()
        })
    }

    /// Returns an iterator over the "raw" device attributes
    ///
    /// This iterator yields <code>[std::io::Result]<[Attribute]></code>
    fn attributes(&self) -> io::Result<Attributes> {
        Attributes::new(self.path())
    }
}

pub type Attr = HashMap<String, io::Result<Vec<u8>>>;

/// Iterator over [`Device`] attributes
///
/// Created by [`Device::attributes`]
#[derive(Debug)]
pub struct Attributes {
    iter: ReadDir,
}

impl Attributes {
    fn new(path: &Path) -> io::Result<Self> {
        Ok(Self {
            iter: path.read_dir()?,
        })
    }
}

impl Iterator for Attributes {
    type Item = std::io::Result<Attribute>;

    fn next(&mut self) -> Option<Self::Item> {
        // FIXME: Has to recurse, potentially more than once,
        // into subdirectories, for sub-attributes.
        let next = self
            .iter
            .by_ref()
            // Filter out symlinks and sub-devices
            // FIXME: If an attribute subdirectory has permissions
            // preventing it from being read, it will incorrectly be skipped.
            // The permission error should instead make it to `Attribute::new`
            // and be exposed to the user
            .filter(|f| {
                if let Ok(entry) = f {
                    let path = entry.path();
                    !path.is_symlink() || (path.is_dir() && !path.join("subsystem").exists())
                } else {
                    true
                }
            })
            .next()?;

        Some(next.and_then(|f| Attribute::new(&f)))
    }
}

/// Represents a "raw" [`Device`] attribute
#[derive(Debug)]
pub struct Attribute {
    /// Attribute name
    name: OsString,
}

impl Attribute {
    fn new(entry: &DirEntry) -> io::Result<Self> {
        let name = entry.file_name();
        Ok(Self { name })
    }

    /// Attribute name
    ///
    /// # Example
    ///
    /// For an attribute `control` in a subdirectory `power`,
    /// this will be `power/control`.
    pub fn name(&self) -> &OsStr {
        &self.name
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

    #[test]
    fn devpath() {
        let _path = Path::new(
            "/System/devices/kernel/devices/pci0000:00/0000:00:08.1/0000:08:00.0/drm/card1",
        );
        let path = Path::new("/sys/devices/pci0000:00/0000:00:08.1/0000:08:00.0/drm/card1");
        panic!();
    }
}
