//! An interface to Block Devices
use std::{
    fs::{self, DirEntry},
    io::prelude::*,
    path::Path,
};

const SYS_ROOT: &str = "/sys";

// TODO: What attributes do all block devices have?
pub struct Device {
    major: u32,
    minor: u32,
    removable: bool,
}

/// Get connected devices
///
/// # Panics
///
/// - If reading `/sys` somehow.
pub fn get_devices() -> Vec<Device> {
    for dir in fs::read_dir(Path::new(SYS_ROOT).join("dev/block")).unwrap() {
        let dir: DirEntry = dir.unwrap();
        //
    }
    todo!()
}
