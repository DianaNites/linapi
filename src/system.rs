//! This module provides ways to get information about a running Linux system
use crate::{
    system::devices::raw::Device,
    util::{read_uevent, write_uevent},
};
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

/// All [`Device`]s have a `uevent` file.
impl<T> UEvent for T
where
    T: Device,
{
    fn write(&self, action: UEventAction, uuid: Option<String>, args: HashMap<String, String>) {
        write_uevent(&self.device_path().join("uevent"), action, uuid, args)
    }

    fn read(&self) -> HashMap<String, String> {
        read_uevent(&self.device_path().join("uevent"))
    }
}
