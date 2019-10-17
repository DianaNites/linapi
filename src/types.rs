//! Types common to the crate
use std::os::unix::io::RawFd;

/// Linux File Descriptor.
///
/// FFI-safe, has the same representation as `i32`.
#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub struct FileDescriptor(pub RawFd);
