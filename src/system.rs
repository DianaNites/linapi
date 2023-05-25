//! This module provides ways to get information about a running Linux system
use std::collections::HashMap;

use rustix::process::{uname as r_uname, Uname as RUname};

pub mod devices;
pub mod modules;

/// Supported [`UEvent`] actions
pub enum UEventAction {
    Add,
    Remove,
    Change,
}

/// Allows sending synthetic uevents, and some seemingly undocumented
/// information about the device.
///
/// See the [kernel docs][1] for more info
///
/// [1]: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-uevent
pub trait UEvent {
    /// Write a synthetic `uevent`
    fn write(&self, action: UEventAction, uuid: Option<String>, args: HashMap<String, String>);

    /// Return the Key=Value pairs in the `uevent` file.
    fn read(&self) -> HashMap<String, String>;
}

/// Holds information about the current Kernel
pub struct Info(RUname);

impl Info {
    /// Operating System name
    #[doc(alias = "sysname")]
    pub fn sys(&self) -> &str {
        self.0.sysname().to_str().expect("non-ascii uname")
    }

    /// Network / Node / Host name
    #[doc(alias = "hostname")]
    pub fn host(&self) -> &str {
        self.0.nodename().to_str().expect("non-ascii uname")
    }

    /// Domain name
    #[doc(alias = "domainname")]
    pub fn domain(&self) -> &str {
        self.0.domainname().to_str().expect("non-ascii uname")
    }

    /// OS-specific release/version information
    #[doc(alias = "version")]
    pub fn info(&self) -> &str {
        self.0.version().to_str().expect("non-ascii uname")
    }

    /// OS release version
    pub fn release(&self) -> &str {
        self.0.release().to_str().expect("non-ascii uname")
    }

    /// Hardware architecture
    pub fn machine(&self) -> &str {
        self.0.machine().to_str().expect("non-ascii uname")
    }
}

/// Get information about the current kernel
///
/// # Implementation
///
/// This uses `uname(2)`
#[inline]
#[doc(alias = "uname")]
pub fn kernel_info() -> Info {
    Info(r_uname())
}
