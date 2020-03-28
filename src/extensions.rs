//! Linux-specific extensions to std types
use nix::{
    errno::Errno,
    fcntl::{flock, FlockArg},
    sys::memfd::{memfd_create, MemFdCreateFlag},
};
use std::{
    ffi::CString,
    fs::File,
    io,
    os::unix::{
        ffi::OsStringExt,
        io::{AsRawFd, FromRawFd, RawFd},
    },
    path::Path,
};

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
    /// - if `path` is more than 249 bytes. This is a Linux Kernel limit.
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
                Err(nix::Error::Sys(Errno::EINTR)) => continue,
                Err(e @ nix::Error::Sys(Errno::ENOLCK)) => panic!("{}", e),
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
                Err(nix::Error::Sys(Errno::EINTR)) => continue,
                Err(nix::Error::Sys(e @ nix::errno::EWOULDBLOCK)) => return Err(e.into()),
                Err(e @ nix::Error::Sys(Errno::ENOLCK)) => panic!("{}", e),
                Err(_) => unreachable!("Lock had nix errors it shouldn't have"),
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
                Err(nix::Error::Sys(Errno::EINTR)) => continue,
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
                Err(nix::Error::Sys(Errno::EINTR)) => continue,
                Err(nix::Error::Sys(e @ nix::errno::EWOULDBLOCK)) => return Err(e.into()),
                Err(_) => unreachable!("Unlock had nix errors it shouldn't have"),
            }
        }
        Ok(())
    }
}

impl FileExt for File {}
