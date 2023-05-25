//! Linux-specific extensions to std types
use std::{
    ffi::CString,
    fs::File,
    io,
    os::unix::{
        ffi::OsStringExt,
        fs::FileTypeExt,
        io::{AsRawFd, FromRawFd, RawFd},
    },
    path::Path,
};

use nix::{
    fcntl::{fallocate, flock, FallocateFlags, FlockArg},
    sys::memfd::{memfd_create, MemFdCreateFlag},
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

/// Impl for [`FileExt::lock`] and co.
fn lock_impl(fd: RawFd, lock: LockType, non_block: bool) -> nix::Result<()> {
    flock(
        fd,
        match lock {
            LockType::Shared => {
                if non_block {
                    FlockArg::LockSharedNonblock
                } else {
                    FlockArg::LockShared
                }
            }
            LockType::Exclusive => {
                if non_block {
                    FlockArg::LockExclusiveNonblock
                } else {
                    FlockArg::LockExclusive
                }
            }
        },
    )
}

/// Type of lock to use for [`FileExt::lock`]
#[derive(Debug, Copy, Clone)]
pub enum LockType {
    /// Multiple processes may acquire this lock
    Shared,

    /// Only one process may acquire this lock
    Exclusive,
}

/// Extends [`File`]
pub trait FileExt: AsRawFd {
    /// Like [`File::create`] except the file exists only in memory.
    ///
    /// As the file exists only in memory, `path` doesn't matter
    /// and is only used as a debugging marker in `/proc/self/fd/`
    ///
    /// # Implementation
    ///
    /// This uses `memfd_create(2)`
    ///
    /// # Panics
    ///
    /// - If `path` is more than 249 bytes. This is a Linux Kernel limit.
    /// - If `path` has any internal null bytes.
    /// - The per process/system file limit is reached.
    /// - Insufficient memory.
    fn create_memory<P: AsRef<Path>>(path: P) -> File {
        let path = path.as_ref();
        let fd = memfd_create(
            &CString::new(path.as_os_str().to_os_string().into_vec()).unwrap(),
            MemFdCreateFlag::MFD_CLOEXEC,
        )
        .unwrap();
        // Safe because this is a newly created file descriptor.
        unsafe { File::from_raw_fd(fd) }
    }

    /// Apply an advisory lock
    ///
    /// A single file can only have one [`LockType`] at a time.
    /// It doesn't matter whether the file was opened for reading or writing.
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
        let fd = self.as_raw_fd();
        loop {
            let e = lock_impl(fd, lock, false);
            match e {
                Ok(_) => break,
                Err(nix::Error::EINTR) => continue,
                Err(e @ nix::Error::ENOLCK) => panic!("{}", e),
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
        let fd = self.as_raw_fd();
        // FIXME: Can the non-blocking variants get interrupted?
        loop {
            let e = lock_impl(fd, lock, true);
            match e {
                Ok(_) => break,
                Err(nix::Error::EINTR) => continue,
                Err(e @ nix::Error::EWOULDBLOCK) => return Err(e.into()),
                Err(e @ nix::Error::ENOLCK) => panic!("{}", e),
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
        let fd = self.as_raw_fd();
        loop {
            let e = flock(fd, FlockArg::Unlock);
            match e {
                Ok(_) => break,
                Err(nix::Error::EINTR) => continue,
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
        let fd = self.as_raw_fd();
        // FIXME: Can the non-blocking variants get interrupted?
        loop {
            let e = flock(fd, FlockArg::UnlockNonblock);
            match e {
                Ok(_) => break,
                Err(nix::Error::EINTR) => continue,
                Err(e @ nix::Error::EWOULDBLOCK) => return Err(e.into()),
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
        let fd = self.as_raw_fd();
        loop {
            let e = fallocate(fd, FallocateFlags::empty(), 0, size).map(|_| ());
            match e {
                Ok(_) => break,
                Err(nix::Error::EINTR) => continue,
                // Not opened for writing
                Err(e @ nix::Error::EBADF) => return Err(e.into()),
                // I/O
                Err(e @ nix::Error::EFBIG) => return Err(e.into()),
                Err(e @ nix::Error::EIO) => return Err(e.into()),
                Err(e @ nix::Error::EPERM) => return Err(e.into()),
                Err(e @ nix::Error::ENOSPC) => return Err(e.into()),
                // Not regular file
                Err(e @ nix::Error::ENODEV) => return Err(e.into()),
                Err(e @ nix::Error::ESPIPE) => return Err(e.into()),
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
