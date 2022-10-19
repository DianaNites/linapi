//! Linux-specific extensions to std types
// #![allow(unused_imports, dead_code)]
use std::{
    fs::{File, OpenOptions},
    io,
    os::unix::{fs::OpenOptionsExt as StdOpenOptionsExt, io::AsRawFd},
    path::Path,
};

use bitflags::bitflags;
use rustix::{
    fd::{AsFd, IntoFd},
    fs::{fallocate, flock, memfd_create, FallocateFlags, FlockOperation, MemfdFlags, OFlags},
};

/// Internal ioctl stuff
mod _impl {
    use std::{convert::TryInto, marker::PhantomData, mem};

    use nix::{
        ioctl_none,
        ioctl_write_ptr_bad,
        libc::{c_char, c_int, c_longlong, c_void},
    };

    pub const BLOCK_ADD_PART: i32 = 1;
    pub const BLOCK_DEL_PART: i32 = 2;
    pub const _BLOCK_RESIZE_PART: i32 = 3;

    #[repr(C)]
    pub struct BlockPageIoctlArgs<'a> {
        /// Requested operation
        op: c_int,

        /// Always zero, kernel doesn't use?
        flags: c_int,

        /// size_of::<BlockPagePartArgs>().
        /// Also unused by the kernel, size is hard-coded?
        data_len: c_int,

        /// [`BlockPagePartArgs`]
        data: *mut c_void,

        _phantom: PhantomData<&'a mut BlockPagePartArgs>,
    }

    impl<'a> BlockPageIoctlArgs<'a> {
        pub fn new(op: i32, data: &'a mut BlockPagePartArgs) -> Self {
            BlockPageIoctlArgs {
                op,
                flags: 0,
                data_len: mem::size_of::<BlockPagePartArgs>().try_into().unwrap(),
                data: data as *mut _ as *mut _,
                _phantom: PhantomData,
            }
        }
    }

    #[repr(C)]
    pub struct BlockPagePartArgs {
        /// Starting offset, in bytes
        start: c_longlong,

        /// Length, in bytes.
        length: c_longlong,

        /// Partition number
        part_num: c_int,

        /// Unused by the kernel?
        dev_name: [c_char; 64],

        /// Unused by the kernel?
        vol_name: [c_char; 64],
    }

    impl BlockPagePartArgs {
        pub fn new(part_num: i32, start: i64, end: i64) -> Self {
            let length = end - start;
            BlockPagePartArgs {
                start,
                length,
                part_num,
                dev_name: [0; 64],
                vol_name: [0; 64],
            }
        }
    }

    ioctl_none! {
        /// The `BLKRRPART` ioctl, defined in
        /// <linux/fs.h>
        block_reread_part,
        0x12,
        95
    }

    ioctl_write_ptr_bad!(
        /// The `BLKPG` ioctl, defined in
        /// <linux/blkpg.h>
        ///
        /// Incorrectly defined as `_IO`, actually takes one argument
        block_page,
        0x1269,
        BlockPageIoctlArgs
    );
}

/// Internal implementation details
mod imp {
    use super::*;

    pub trait FileExtSeal: AsFd {}
    impl FileExtSeal for File {}

    pub trait OpenOptionsExtSeal: StdOpenOptionsExt {}
    impl OpenOptionsExtSeal for OpenOptions {}
}

/// Impl for [`FileExt::lock`] and co.
#[inline]
fn lock_impl<Fd: AsFd>(fd: Fd, lock: LockType, non_block: bool) -> rustix::io::Result<()> {
    flock(
        fd,
        match lock {
            LockType::Shared => {
                if non_block {
                    FlockOperation::NonBlockingLockShared
                } else {
                    FlockOperation::LockShared
                }
            }
            LockType::Exclusive => {
                if non_block {
                    FlockOperation::NonBlockingLockExclusive
                } else {
                    FlockOperation::LockExclusive
                }
            }
        },
    )
}

/// Impl for [`FileExt::create_memory`] and co.
#[inline]
fn create_memory_impl(path: &Path, flags: MemfdFlags) -> io::Result<File> {
    memfd_create(path, flags)
        .map_err(Into::<io::Error>::into)
        .map(|f| f.into_fd())
        .map(Into::<File>::into)
}

/// Type of lock to use for [`FileExt::lock`]
#[derive(Debug, Copy, Clone)]
pub enum LockType {
    /// Multiple processes may acquire this lock
    Shared,

    /// Only one process may acquire this lock
    Exclusive,
}

bitflags! {
    /// Flags for [`FileExt::allocate_flags`]
    ///
    /// Unknown flags will be truncated.
    pub struct AllocateFlags: u32 {
        /// Keep the file size the same
        ///
        /// This may be useful to pre-allocate space at the end of a file,
        /// for appending.
        const KEEP_SIZE = 0x01;

        /// Unshare any shared data on Copy On Write filesystems, to
        /// actually guarantee subsequent writes do not fail.
        const UNSHARE = 0x40;

        /// Zero all data within the region instead of leaving it as-is.
        const ZERO = 0x10;
    }
}

/// Extends [`File`] with linux-specific methods
///
/// This trait is sealed
pub trait FileExt: imp::FileExtSeal {
    /// Like [`File::create`], except the file exists only in memory.
    /// The file is opened for both reading and writing.
    ///
    /// # Implementation
    ///
    /// This uses `memfd_create(2)`.
    /// The `MFD_CLOEXEC` and `MFD_ALLOW_SEALING` flags are set.
    ///
    /// As the file exists only in memory, `path` doesn't matter
    /// and is only used as a debugging marker in `/proc/self/fd/`.
    /// The same name/path may exist multiple times.
    ///
    /// # Errors
    ///
    /// - If `path` is too long.
    /// - If `path` has any internal null bytes.
    /// - The per process/system file limit is reached.
    /// - Insufficient memory.
    #[inline]
    fn create_memory<P: AsRef<Path>>(path: P) -> io::Result<File> {
        create_memory_impl(
            path.as_ref(),
            MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
        )
    }

    /// Create an unnamed temporary regular file on `path`s filesystem in
    /// write-only mode.
    ///
    /// See the [`OpenOptionsExt::tmpfile`] function for more details.
    #[inline]
    fn tmpfile<P: AsRef<Path>>(path: P) -> io::Result<File> {
        File::options().write(true).tmpfile(true).open(path)
    }

    /// Apply an advisory lock
    ///
    /// A single file can only have one [`LockType`] at a time.
    /// It doesn't matter whether the file was opened for reading or writing.
    ///
    /// Locks are associated with a file descriptor, and any duplicates
    /// refer to the same lock.
    ///
    /// Calling this on an already locked file will change the [`LockType`].
    ///
    /// This may block until the lock can be acquired.
    /// See [`FileExt::lock_nonblock`] if thats unacceptable.
    ///
    /// # Implementation
    ///
    /// This uses `flock(2)`.
    ///
    /// # Errors
    ///
    /// - Kernel runs out of memory for lock records
    /// - If interrupted by a signal handler.
    #[inline]
    fn lock(&self, lock: LockType) -> io::Result<()> {
        lock_impl(self.as_fd(), lock, false).map_err(Into::into)
    }

    /// Apply an advisory lock, without blocking
    ///
    /// See [`FileExt::lock`] for more details
    ///
    /// # Errors
    ///
    /// - Same as [`FileExt::lock`]
    /// - [`io::ErrorKind::WouldBlock`] if the operation would block
    #[inline]
    fn lock_nonblock(&self, lock: LockType) -> io::Result<()> {
        lock_impl(self.as_fd(), lock, true).map_err(Into::into)
    }

    /// Remove an advisory lock
    ///
    /// This may block until the lock can be acquired.
    /// See [`FileExt::unlock_nonblock`] if thats unacceptable.
    ///
    /// # Implementation
    ///
    /// This uses `flock(2)`.
    ///
    /// This will retry as necessary on `EINTR`
    ///
    /// # Errors
    ///
    /// - If interrupted by a signal handler.
    #[inline]
    fn unlock(&self) -> io::Result<()> {
        flock(self.as_fd(), FlockOperation::Unlock).map_err(Into::into)
    }

    /// Remove an advisory lock, without blocking
    ///
    /// See [`FileExt::unlock`] for more details
    ///
    /// # Errors
    ///
    /// - [`io::ErrorKind::WouldBlock`] if the operation would block
    /// - If interrupted by a signal handler.
    #[inline]
    fn unlock_nonblock(&self) -> io::Result<()> {
        flock(self.as_fd(), FlockOperation::NonBlockingUnlock).map_err(Into::into)
    }

    /// Allocate space on disk for `size` bytes without overwriting
    /// existing data
    ///
    /// If `size` is greater than the file size, the file size will become
    /// `size` after this call
    ///
    /// Subsequent writes up to `size` bytes are guaranteed not to fail
    /// because of lack of disk space.
    ///
    /// [`File::set_len`] is similar, but *truncates* the file,
    /// making it sparse, meaning it does not actually take any disk space,
    /// and writes may fail due to a lack of space.
    ///
    /// See [`FileExt::allocate_flags`] for more advanced usage
    ///
    /// # Implementation
    ///
    /// This uses `fallocate(2)`
    ///
    /// Subsequent writes may still fail if on a Copy On Write filesystem
    /// and the file has shared data.
    ///
    /// This may allocate more space than requested due to filesystem block
    /// sizes
    ///
    /// # Errors
    ///
    /// - If `self` is not opened for writing.
    /// - If `self` is not a regular file.
    /// - If I/O does.
    /// - If interrupted by a signal handler.
    /// - If `size` is zero
    #[inline]
    fn allocate(&self, size: u64) -> io::Result<()> {
        fallocate(self.as_fd(), FallocateFlags::empty(), 0, size).map_err(Into::into)
    }

    /// Allocate space on disk at `offset + len`
    ///
    /// Any existing data within this region is kept as-is.
    ///
    /// See [`FileExt::allocate`] for more details
    ///
    /// # Errors
    ///
    /// - Same as [`FileExt::allocate`]
    /// - If an unsupported operation is requested
    #[inline]
    fn allocate_flags(&self, offset: u64, len: u64, flags: AllocateFlags) -> io::Result<()> {
        fallocate(
            self.as_fd(),
            FallocateFlags::from_bits_truncate(flags.bits()),
            offset,
            len,
        )
        .map_err(Into::into)
    }

    /// Deallocates space on disk at `offset + len`
    ///
    /// The file size will not change after this call.
    ///
    /// # Implementation
    ///
    /// This uses `fallocate(2)`
    ///
    /// # Errors
    ///
    /// - If the filesystem doesn't support this operation
    #[inline]
    fn deallocate(&self, offset: u64, len: u64) -> io::Result<()> {
        fallocate(
            self.as_fd(),
            FallocateFlags::PUNCH_HOLE | FallocateFlags::KEEP_SIZE,
            offset,
            len,
        )
        .map_err(Into::into)
    }

    /// Remove the range `offset + len`
    ///
    /// After this operation the file is `len` bytes smaller, and
    /// any data past `offset+len` appears starting from `offset`.
    ///
    /// This is the counterpart to [`FileExt::insert`]
    ///
    /// # Example
    ///
    /// If you have a file containing `HELLO WORLD`,
    /// collapsing at offset 3 and len 7 results in a file containing
    /// `HELD`
    ///
    /// # Errors
    ///
    /// - If `offset+len` is or goes past EOF
    /// - If the filesystem doesn't support this operation
    /// - If the filesystem requires a specific granularity, and `offset` and
    ///   `len` are not the correct granularity.
    #[inline]
    fn collapse(&self, offset: u64, len: u64) -> io::Result<()> {
        fallocate(self.as_fd(), FallocateFlags::COLLAPSE_RANGE, offset, len).map_err(Into::into)
    }

    /// Insert unallocated space at `offset + len`, without overwriting any
    /// existing data
    ///
    /// Any existing data at `offset` is shifted `len` bytes further in the file
    ///
    /// This is the counterpart to [`FileExt::collapse`]
    ///
    /// # Errors
    ///
    /// - If `offset+len` is or goes past EOF
    /// - If the filesystem doesn't support this operation
    /// - If the filesystem requires a specific granularity, and `offset` and
    ///   `len` are not the correct granularity.
    #[inline]
    fn insert(&self, offset: u64, len: u64) -> io::Result<()> {
        fallocate(self.as_fd(), FallocateFlags::INSERT_RANGE, offset, len).map_err(Into::into)
    }

    /// Tell the kernel to re-read the partition table.
    /// This call may be unreliable and require reboots.
    ///
    /// You may instead want the newer [`FileExt::add_partition`], or
    /// [`FileExt::remove_partition`]
    ///
    /// # Implementation
    ///
    /// This uses the `BLKRRPART` ioctl.
    ///
    /// # Errors
    ///
    /// - If `self` is not a block device
    /// - If the underlying ioctl does.
    fn reread_partitions(&self) -> io::Result<()> {
        match unsafe { _impl::block_reread_part(self.as_fd().as_raw_fd()) } {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Inform the kernel of a partition, number `part`.
    ///
    /// The partition starts at `start` bytes and ends at `end` bytes,
    /// relative to the start of `self`.
    ///
    /// # Implementation
    ///
    /// This uses the `BLKPG` ioctl.
    ///
    /// # Errors
    ///
    /// - If `self` is not a block device.
    /// - If the underlying ioctl does.
    fn add_partition(&self, part: i32, start: i64, end: i64) -> io::Result<()> {
        let mut part = _impl::BlockPagePartArgs::new(part, start, end);
        let args = _impl::BlockPageIoctlArgs::new(_impl::BLOCK_ADD_PART, &mut part);
        match unsafe { _impl::block_page(self.as_fd().as_raw_fd(), &args) } {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Remove partition number `part`.
    ///
    /// # Implementation
    ///
    /// This uses the `BLKPG` ioctl.
    ///
    /// # Errors
    ///
    /// - If `self` is not a block device.
    /// - If the underlying ioctl does.
    fn remove_partition(&self, part: i32) -> io::Result<()> {
        let mut part = _impl::BlockPagePartArgs::new(part, 0, 0);
        let args = _impl::BlockPageIoctlArgs::new(_impl::BLOCK_DEL_PART, &mut part);
        match unsafe { _impl::block_page(self.as_fd().as_raw_fd(), &args) } {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

impl FileExt for File {}

/// Extends [`OpenOptions`] with linux-specific methods
///
/// This trait is sealed
pub trait OpenOptionsExt: imp::OpenOptionsExtSeal {
    /// Sets the option to create a tmpfile
    ///
    /// In order for a tmpfile to be created, [`OpenOptions::write`] or
    /// [`OpenOptions::read`] and [`OpenOptions::write`] must be used,
    /// and the supplied path must be a directory.
    ///
    /// # Implementation
    ///
    /// `O_TMPFILE`
    #[inline]
    fn tmpfile(&mut self, tmpfile: bool) -> &mut Self {
        if tmpfile {
            self.custom_flags(OFlags::TMPFILE.bits() as i32)
        } else {
            self
        }
    }
}

impl OpenOptionsExt for OpenOptions {}

#[cfg(test)]
mod tests {
    // #![allow(warnings)]
    use std::{
        error::Error,
        io::{prelude::*, SeekFrom},
        os::linux::fs::MetadataExt,
    };

    use super::*;

    const TEST_STR: &str = "HELLO WORLD"; // 11
    const TEST_STR_ZERO: &str = "\0\0\0\0\0\0\0\0\0\0\0";

    /// Test that the various `fallocate` things work properly
    #[test]
    fn fallocate() -> Result<(), Box<dyn Error>> {
        let mut buf = String::new();
        let mut f = File::create_memory("fallocate create_memory test file")?;
        f.lock(LockType::Exclusive)?;
        f.lock(LockType::Shared)?;
        f.unlock()?;

        write!(f, "{TEST_STR}")?;
        f.rewind()?;

        f.allocate(TEST_STR.len() as u64)?;
        f.read_to_string(&mut buf)?;
        f.rewind()?;
        assert_eq!(buf, TEST_STR, "allocate overwrote TEST_STR");
        assert_eq!(TEST_STR.len() as u64, f.metadata()?.len());

        f.deallocate(0, TEST_STR.len() as u64)?;
        assert_eq!(TEST_STR.len() as u64, f.metadata()?.len());
        buf.clear();
        f.read_to_string(&mut buf)?;
        assert_eq!(buf, TEST_STR_ZERO, "deallocate didn't overwrite");

        // Assumes current directories filesystem supports `collapse` and etc
        // Tested on ext4
        let mut f = File::options()
            .write(true)
            .read(true)
            .tmpfile(true)
            .open(".")?;
        let block = f.metadata()?.st_blksize();
        f.allocate(block * 3)?;

        write!(f, "ONE ")?;
        f.seek(SeekFrom::Start(block))?;
        write!(f, "TWO ")?;
        f.seek(SeekFrom::Start(block * 2))?;
        write!(f, "THREE")?;
        f.rewind()?;

        // Remove the middle block
        f.collapse(block, block)?;

        buf.clear();
        f.read_to_string(&mut buf)?;
        buf.retain(|c| c != '\0');
        assert_eq!(buf, "ONE THREE", "collapse didn't work correctly");
        dbg!(&buf);

        // panic!();
        Ok(())
    }
}
