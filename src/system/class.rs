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

use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::Path,
};

use self::imp::Sealed;

pub mod block;

mod imp {
    #![allow(unused_imports)]
    use super::*;

    pub trait Sealed {}

    // impl Sealed for Device {}
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
}
