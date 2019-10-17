//! Block device `ioctl`s.
use std::fs::File;
use std::os::unix::prelude::*;
// TODO: Proper error types

mod _impl {
    use nix::*;
    /// Argument to the [`block_page`] ioctl.
    #[repr(C)]
    pub struct BlockPageIoctlArgs {
        /// Requested operation
        pub op: nix::libc::c_int,

        /// Always zero, kernel doesn't use.
        pub flags: nix::libc::c_int,

        /// size_of::<BlockPagePartArgs>().
        /// Also unused by the kernel size is hard-coded.
        pub data_len: nix::libc::c_int,

        /// [`BlockPagePartArgs`]
        pub data: *mut nix::libc::c_void,
    }

    ioctl_write_ptr_bad!(
        /// The `BLKPG` ioctl, defined in
        /// <linux/blkpg.h>
        ///
        /// Incorrectly defined as `_IO`, actually takes one argument
        block_page,
        0x1269,
        super::BlockPageIoctlArgs
    );
}
#[doc(inline)]
use _impl::BlockPageIoctlArgs;

// See <linux/blkpg.h>
// Codes for `BlockPageIoctlArgs::op`
const BLOCK_ADD_PART: i32 = 1;
const BLOCK_DEL_PART: i32 = 2;
const _BLOCK_RESIZE_PART: i32 = 3;

/// Used in [`BlockPageIoctlArgs::data`]
#[repr(C)]
struct BlockPagePartArgs {
    /// Starting offset, in bytes
    start: nix::libc::c_longlong,

    /// Length, in bytes.
    length: nix::libc::c_longlong,

    /// Partition number
    pno: nix::libc::c_int,

    /// Unused by the kernel.
    dev_name: [nix::libc::c_char; 64],

    /// Unused by the kernel.
    vol_name: [nix::libc::c_char; 64],
}

/// Add a partition number `part` to the block device identified by `fd`.
///
/// The partition starts at `start` bytes and ends at `end` bytes.
/// This is an offset from the start of the `fd`. Note that `end` is exclusive.
///
/// The kernel requires that partitions be aligned to the logical block size.
/// This will usually be the case, as most partition tables also require this.
/// If this is not the case, your `start` is likely invalid.
///
/// This uses the `BLKPG` ioctl.
///
/// # Errors
///
/// # Panics
///
/// - If `part` is >= 65536.
/// - if `fd` is not a block device.
pub fn add_partition(fd: &File, part: i32, start: i64, end: i64) -> nix::Result<nix::libc::c_int> {
    assert!(
        fd.metadata().unwrap().file_type().is_block_device(),
        "File {:?} was not a block device",
        fd,
    );
    assert!(part >= 65536, "Invalid partition number: {}", part);
    let mut part = BlockPagePartArgs {
        start,
        length: end - start,
        pno: part,
        dev_name: [0; 64],
        vol_name: [0; 64],
    };
    let args = BlockPageIoctlArgs {
        op: BLOCK_ADD_PART,
        flags: 0,
        data_len: std::mem::size_of::<BlockPagePartArgs>() as i32,
        data: &mut part as *mut _ as *mut _,
    };
    unsafe { _impl::block_page(fd.as_raw_fd(), &args) }
}

/// Remove a partition number `part` from the block device at `fd`.
///
/// This uses the `BLKPG` ioctl.
///
/// # Errors
///
/// - If `part` doesn't exist. Safe to ignore.
///
/// # Panics
///
/// - if `fd` is not a block device.
pub fn remove_partition(fd: &File, part: i32) -> nix::Result<nix::libc::c_int> {
    assert!(
        fd.metadata().unwrap().file_type().is_block_device(),
        "File {:?} was not a block device",
        fd,
    );
    let mut part = BlockPagePartArgs {
        start: 0,
        length: 0,
        pno: part,
        dev_name: [0; 64],
        vol_name: [0; 64],
    };
    let args = BlockPageIoctlArgs {
        op: BLOCK_DEL_PART,
        flags: 0,
        data_len: std::mem::size_of::<BlockPagePartArgs>() as i32,
        data: &mut part as *mut _ as *mut _,
    };
    unsafe { _impl::block_page(fd.as_raw_fd(), &args) }
}

/// Convenience function to remove existing partitions before
/// calling `add_partition`. Ignores missing partitions.
///
/// # Panics
///
/// - if `fd` is not a block device.
pub fn remove_existing_partitions(fd: &File) -> nix::Result<nix::libc::c_int> {
    for i in 1..=64 {
        match remove_partition(fd, i) {
            Ok(_) => (),
            Err(nix::Error::Sys(nix::errno::Errno::ENXIO)) => (),
            e @ Err(_) => return e,
        }
    }
    Ok(0)
}
