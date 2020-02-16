//! High level bindings to various Linux APIs and interfaces
//!
//! # Implementation details
//!
//! Most Linux APIs and interfaces are provided through files in `/sys` and
//! `/proc`.
//!
//! Most of these interfaces are also undocumented, and some may change between
//! kernel versions.
//!
//! This crate attempts to correctly document these interfaces, and provide
//! kernel documentation sources where possible.
//!
//! ## API
//!
//! The API layout is subject to change, and suggestions are welcome.
//!
//! ## Stability
//!
//! Linux has 3 ideas of stability for sysfs, documented [here][1]
//!
//! In short, there are two that matter:
//!
//! - 'Stable', no restrictions on use and backwards compatibility is guaranteed
//!   for at least 2 years.
//! - 'Testing', mostly stable and complete, new features may be added in a
//!   backwards compatible manner, and the interface may break if serious errors
//!   or security problems are found with it. Userspace should try to keep up
//!   with changes.
//!
//! Most sysfs interfaces are 'Testing', so keep that in mind when using this
//! library. Also keep in mind that some have been "Testing" and unchanged for
//! decades.
//!
//! [1]: https://www.kernel.org/doc/Documentation/ABI/README

mod raw {
    #![allow(dead_code)]
    pub mod block;
    pub mod ioctl;
}

pub mod extensions {
    //! Linux-specific Extensions to std types
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
}

//
pub mod modules;
pub mod types;
