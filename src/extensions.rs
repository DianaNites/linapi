//! Linux-specific extensions to std types
use nix::sys::memfd::{memfd_create, MemFdCreateFlag};
use std::{
    ffi::CString,
    fs::File,
    os::unix::{ffi::OsStringExt, io::FromRawFd},
    path::Path,
};

/// Extends [`File`]
pub trait FileExt {
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
}

impl FileExt for File {}
