//! Linux-specific extensions to std types
// #![allow(unused_imports, dead_code)]
use std::{
    fs::File,
    io,
    os::unix::{fs::FileTypeExt, io::AsRawFd},
    path::Path,
};

use nix::{
    errno::Errno,
    fcntl::{fallocate, FallocateFlags},
};
use rustix::{
    fd::{AsFd, IntoFd},
    fs::{flock, memfd_create, FlockOperation, MemfdFlags},
    io::Errno as Errno_,
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
}

/// Impl for [`FileExt::lock`] and co.
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
fn create_memory_impl(path: &Path, flags: MemfdFlags) -> File {
    memfd_create(path, flags).unwrap().into_fd().into()
}

/// Type of lock to use for [`FileExt::lock`]
#[derive(Debug, Copy, Clone)]
pub enum LockType {
    /// Multiple processes may acquire this lock
    Shared,

    /// Only one process may acquire this lock
    Exclusive,
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
    /// # Panics
    ///
    /// - If `path` is more than 249 bytes. This is a Linux Kernel limit.
    /// - If `path` has any internal null bytes.
    /// - The per process/system file limit is reached.
    /// - Insufficient memory.
    fn create_memory<P: AsRef<Path>>(path: P) -> File {
        create_memory_impl(
            path.as_ref(),
            MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
        )
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
    /// This will retry as necessary on `EINTR`
    ///
    /// # Panics
    ///
    /// - Kernel runs out of memory for lock records
    fn lock(&self, lock: LockType) {
        loop {
            let e = lock_impl(self.as_fd(), lock, false);
            match e {
                Ok(_) => break,
                Err(Errno_::INTR) => continue,
                Err(e @ Errno_::NOLCK) => panic!("{}", e),
                Err(_) => unreachable!("Lock had nix errors it shouldn't have"),
            }
        }
    }

    /// Apply an advisory lock, without blocking
    ///
    /// See [`FileExt::lock`] for more details
    ///
    /// # Errors
    ///
    /// - [`io::ErrorKind::WouldBlock`] if the operation would block
    fn lock_nonblock(&self, lock: LockType) -> io::Result<()> {
        // FIXME: Can the non-blocking variants get interrupted?
        loop {
            let e = lock_impl(self.as_fd(), lock, true);
            match e {
                Ok(_) => break,
                Err(Errno_::INTR) => continue,
                Err(e @ Errno_::WOULDBLOCK) => return Err(e.into()),
                Err(e @ Errno_::NOLCK) => panic!("{}", e),
                Err(_) => unreachable!("Lock_nonblock had nix errors it shouldn't have"),
            }
        }
        Ok(())
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
    fn unlock(&self) {
        loop {
            let e = flock(self.as_fd(), FlockOperation::Unlock);
            match e {
                Ok(_) => break,
                Err(Errno_::INTR) => continue,
                Err(_) => unreachable!("Unlock had nix errors it shouldn't have"),
            }
        }
    }

    /// Remove an advisory lock, without blocking
    ///
    /// See [`FileExt::unlock`] for more details
    ///
    /// # Errors
    ///
    /// - [`io::ErrorKind::WouldBlock`] if the operation would block
    fn unlock_nonblock(&self) -> io::Result<()> {
        // FIXME: Can the non-blocking variants get interrupted?
        loop {
            let e = flock(self.as_fd(), FlockOperation::NonBlockingUnlock);
            match e {
                Ok(_) => break,
                Err(Errno_::INTR) => continue,
                Err(e @ Errno_::WOULDBLOCK) => return Err(e.into()),
                Err(_) => unreachable!("Unlock_nonblock had nix errors it shouldn't have"),
            }
        }
        Ok(())
    }

    /// Allocate space on disk for at least `size` bytes
    ///
    /// Unlike [`File::set_len`], which on Linux creates a sparse file
    /// without reserving disk space,
    /// this will actually reserve `size` bytes of zeros, without having to
    /// write them.
    ///
    /// Subsequent writes up to `size` bytes are guaranteed not to fail
    /// because of lack of disk space.
    ///
    /// # Implementation
    ///
    /// This uses `fallocate(2)`
    ///
    /// This will retry as necessary on `EINTR`
    ///
    /// # Errors
    ///
    /// - If `self` is not opened for writing.
    /// - If `self` is not a regular file.
    /// - If I/O does.
    ///
    /// # Panics
    ///
    /// - If `size` is zero
    fn allocate(&self, size: i64) -> io::Result<()> {
        assert_ne!(size, 0, "Size cannot be zero");
        let fd = self.as_fd().as_raw_fd();
        loop {
            let e = fallocate(fd, FallocateFlags::empty(), 0, size).map(|_| ());
            match e {
                Ok(_) => break,
                Err(Errno::EINTR) => continue,
                // Not opened for writing
                Err(e @ Errno::EBADF) => return Err(e.into()),
                // I/O
                Err(e @ Errno::EFBIG) => return Err(e.into()),
                Err(e @ Errno::EIO) => return Err(e.into()),
                Err(e @ Errno::EPERM) => return Err(e.into()),
                Err(e @ Errno::ENOSPC) => return Err(e.into()),
                // Not regular file
                Err(e @ Errno::ENODEV) => return Err(e.into()),
                Err(e @ Errno::ESPIPE) => return Err(e.into()),
                Err(_) => unreachable!("Allocate had nix errors it shouldn't have"),
            }
        }
        Ok(())
    }

    // TODO: Dig holes, see `fallocate(1)`.

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
    fn reread_partitions(&self) -> io::Result<()>;

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
    fn add_partition(&self, part: i32, start: i64, end: i64) -> io::Result<()>;

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
    fn remove_partition(&self, part: i32) -> io::Result<()>;
}

impl FileExt for File {
    fn reread_partitions(&self) -> io::Result<()> {
        if !self.metadata()?.file_type().is_block_device() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "File was not a block device",
            ));
        }
        match unsafe { _impl::block_reread_part(self.as_raw_fd()) } {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn add_partition(&self, part: i32, start: i64, end: i64) -> io::Result<()> {
        if !self.metadata()?.file_type().is_block_device() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "File was not a block device",
            ));
        }
        let mut part = _impl::BlockPagePartArgs::new(part, start, end);
        let args = _impl::BlockPageIoctlArgs::new(_impl::BLOCK_ADD_PART, &mut part);
        match unsafe { _impl::block_page(self.as_raw_fd(), &args) } {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn remove_partition(&self, part: i32) -> io::Result<()> {
        if !self.metadata()?.file_type().is_block_device() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "File was not a block device",
            ));
        }
        let mut part = _impl::BlockPagePartArgs::new(part, 0, 0);
        let args = _impl::BlockPageIoctlArgs::new(_impl::BLOCK_DEL_PART, &mut part);
        match unsafe { _impl::block_page(self.as_raw_fd(), &args) } {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
