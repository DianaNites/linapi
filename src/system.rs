//! This module provides ways to access information from a running Linux system
use std::collections::HashMap;

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

pub mod class;

use rustix::process::{uname as r_uname, Uname as RUname};

/// Uname struct
pub struct Uname(RUname);

impl Uname {
    /// System name
    pub fn sys_name(&self) -> &str {
        self.0.sysname().to_str().expect("non-ascii uname")
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

/// Uname
pub fn uname() -> Uname {
    Uname(r_uname())
}
