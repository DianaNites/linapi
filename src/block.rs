//! An interface to Block Devices
use crate::sysfs::SYSFS_PATH;
use std::{
    fs::{self, DirEntry},
    io::prelude::*,
    path::{Path, PathBuf},
};

mod _impl {
    use nix::{
        libc::{c_char, c_int, c_longlong, c_void},
        *,
    };

    ioctl_read! {
        /// The `BLKGETSIZE64` ioctl.
        block_device_size_bytes, 0x12, 114, u64
    }
}

/// Get connected devices
///
/// # Panics
///
/// - If reading `/sys` does, somehow.
pub fn get_devices() -> Vec<()> {
    let path = Path::new(SYSFS_PATH).join("dev/block");

    // if `/sys/subsystem` exists, use it.
    let dir = Path::new(SYSFS_PATH).join("subsystem");
    let path = fs::metadata(&dir).map_or(path, |_| dir);

    for dir in fs::read_dir(path).unwrap() {
        let dir: DirEntry = dir.unwrap();
        //
    }
    todo!()
}
