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
    ffi::OsStr,
    fmt::Debug,
    fs::{self},
    io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use self::imp::{read_attrs, Sealed, SYSFS_PATH};

pub mod block;
pub mod drm;
pub mod pci;

mod imp {
    use super::*;

    pub trait Sealed {}

    impl Sealed for block::Block {}
    impl Sealed for drm::Gpu {}
    impl Sealed for drm::Connector {}
    impl Sealed for pci::Pci {}
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

/// Iterator over child devices
#[derive(Debug)]
pub struct Children<'a> {
    _path: &'a Path,
    iter: fs::ReadDir,
}

impl<'a> Children<'a> {
    fn new(path: &'a Path) -> io::Result<Self> {
        Ok(Self {
            _path: path,
            iter: path.read_dir()?,
        })
    }
}

impl<'a> Iterator for Children<'a> {
    type Item = io::Result<GenericDevice>;

    fn next(&mut self) -> Option<Self::Item> {
        for dev in &mut self.iter {
            let dev = match dev {
                Ok(d) => d,
                Err(e) => return Some(Err(e)),
            };
            let path = dev.path();
            let typ = match dev.file_type() {
                Ok(t) => t,
                Err(e) => return Some(Err(e)),
            };
            if !typ.is_dir() {
                continue;
            }
            let sub = path.join("subsystem");
            let exists = match sub.try_exists() {
                Ok(x) => x,
                Err(e) => return Some(Err(e)),
            };
            if !exists {
                continue;
            }
            let meta = match sub.symlink_metadata() {
                Ok(m) => m,
                Err(e) => return Some(Err(e)),
            };
            if !meta.is_symlink() {
                continue;
            }
            return Some(Ok(GenericDevice::new_unchecked(path)));
        }
        None
    }
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
    fn devpath(&self) -> &str {
        OsStr::from_bytes(&self.path().as_os_str().as_bytes()[SYSFS_PATH.len()..])
            .to_str()
            .expect("devpath cannot be invalid utf-8")
    }

    /// Kernel name of the device.
    ///
    /// Identical to the last component of [`Device::devpath`]
    ///
    /// # Example
    ///
    /// `card1`
    fn kernel_name(&self) -> &str {
        self.path()
            .file_name()
            .expect("devpath cannot end in ..")
            .to_str()
            .expect("kernel_name cannot be invalid utf-8")
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
    fn subsystem(&self) -> io::Result<String> {
        Ok(self
            .path()
            .join("subsystem")
            .read_link()?
            .file_name()
            .expect("subsystem cannot end in ..")
            .to_str()
            .expect("subsystem cannot be invalid utf-8")
            .to_owned())
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
    fn driver(&self) -> io::Result<Option<String>> {
        self.subsystem()?;
        Ok(self.path().join("driver").read_link().ok().map(|f| {
            f.file_name()
                .expect("driver cannot end in ..")
                .to_str()
                .expect("driver cannot be invalid utf-8")
                .to_owned()
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
    fn dev(&self) -> io::Result<Option<(u64, u64)>> {
        let dev = self.path().join("dev");
        if !dev.try_exists()? {
            return Ok(None);
        }
        let i = fs::read_to_string(dev)?;
        let mut i = i.trim().split(':');

        let major = i.next().ok_or(io::ErrorKind::InvalidInput)?;
        let minor = i.next().ok_or(io::ErrorKind::InvalidInput)?;

        let major = major
            .parse::<u64>()
            .map_err(|_| io::ErrorKind::InvalidInput)?;
        let minor = minor
            .parse::<u64>()
            .map_err(|_| io::ErrorKind::InvalidInput)?;

        Ok(Some((major, minor)))
    }

    /// Returns an iterator over child devices
    ///
    /// A "child device" is any subdirectory that is not a symlink and
    /// has a subsystem.
    fn children(&self) -> io::Result<Children> {
        Children::new(self.path())
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

    /// Create a new [`Device`] from `path', which must be canonical.
    fn new_unchecked<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref().to_path_buf();
        Self { path }
    }

    /// Get connected `subsystem` Devices, sorted.
    ///
    /// # Note
    ///
    /// Child devices are **not** included. Use [`Device::children`].
    ///
    /// This **will** expose devices that you don't have permission to see.
    ///
    /// # Errors
    ///
    /// - If unable to read any of the subsystem directories.
    pub fn devices(subsystem: &str) -> io::Result<Vec<Self>> {
        if subsystem.contains('/') {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let sysfs = Path::new(SYSFS_PATH);
        let mut devices = Vec::new();
        let paths = if sysfs.join("subsystem").exists() {
            vec![sysfs.join("subsystem").join(subsystem).join("devices")]
        } else if subsystem == "block" {
            // block is weird.
            vec![sysfs.join("class/block"), sysfs.join("block")]
        } else {
            vec![
                sysfs.join("bus").join(subsystem).join("devices"),
                sysfs.join("class").join(subsystem),
            ]
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
                // This dance around paths is required because
                // Path::canonicalize will error if, say, permissions aren't right.
                // We want to expose devices that you dont have permission to access
                devices.push(Self::new_unchecked(
                    Path::new(SYSFS_PATH).join("devices").join(c.as_path()),
                ));
            }
        }
        devices.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        devices.dedup_by(|a, b| a.path == b.path);
        // Remove sub-devices
        devices.dedup_by(|a, b| a.path.starts_with(&b.path));
        Ok(devices)
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
