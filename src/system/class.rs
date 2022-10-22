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
use std::{
    ffi::{OsStr, OsString},
    fmt::Debug,
    fs::{self},
    io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use self::imp::{read_attrs, Sealed, SYSFS_PATH};

pub mod block;
pub mod drm;

mod imp {
    use super::*;

    pub trait Sealed {}

    impl Sealed for block::Block {}
    impl Sealed for drm::Gpu {}
    impl Sealed for drm::Connector {}
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

    /// Technically Linux requires sysfs to be at `/sys`, calling it a system
    /// configuration error otherwise.
    ///
    /// Use this for testing purposes
    pub const SYSFS_PATH: &str = "/sys";
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
        OsStr::from_bytes(&self.path().as_os_str().as_bytes()[SYSFS_PATH.len()..])
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
        read_attrs(self.path(), &mut v)?;
        v.sort_unstable();
        Ok(v)
    }

    /// Returns the parent device, if it exists
    ///
    /// Errors traversing the chain are coerced to [`None`]
    fn parent(&self) -> Option<GenericDevice> {
        let mut parent = self.path().parent();
        while let Some(path) = parent {
            // FIXME: Probably should be try_exists?
            // Expose errors?
            if path.join("subsystem").exists() {
                return GenericDevice::new(path).ok();
            }
            parent = path.parent();
        }
        None
    }

    /// Returns the (major, minor) numbers of the device file for this device,
    /// if they exist.
    fn dev(&self) -> io::Result<Option<(u32, u32)>> {
        let dev = self.path().join("dev");
        if !dev.try_exists()? {
            return Ok(None);
        }
        let i = fs::read_to_string(dev)?;
        let mut i = i.trim().split(':');

        let major = i.next().ok_or(io::ErrorKind::InvalidInput)?;
        let minor = i.next().ok_or(io::ErrorKind::InvalidInput)?;

        let major = major
            .parse::<u32>()
            .map_err(|_| io::ErrorKind::InvalidInput)?;
        let minor = minor
            .parse::<u32>()
            .map_err(|_| io::ErrorKind::InvalidInput)?;

        Ok(Some((major, minor)))
    }
}

/// A generic linux [`Device`]
#[derive(Debug, Clone)]
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
        if path.starts_with(Path::new(SYSFS_PATH).join("devices")) {
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
        // let _ = dbg!(dev.subsystem());
        // let _ = dbg!(dev.driver());
        dbg!(dev.path());
        let mut d = dev;
        println!();
        while let Some(dev) = d.parent() {
            dbg!(&dev);
            dbg!(&dev.subsystem());
            println!();
            d = dev;
        }
        // for attr in dev.attributes() {
        //     // dbg!(&attr);
        //     // dbg!(&attr.map(|a| a.name()));
        // }
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
