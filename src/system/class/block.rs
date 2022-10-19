//! Information about block devices
//!
//! # Implementation
//!
//! These interfaces are poorly documented, and what does exist is
//! scattered and and inconsistent.
//!
//! See [stable/sysfs-block][1] and [testing/sysfs-block][2]
//!
//! [1]: https://www.kernel.org/doc/Documentation/ABI/stable/sysfs-block
//! [2]: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-block

use std::path::PathBuf;

// use super::Device;

/// A linux block device
#[derive(Debug, Clone)]
pub struct Block {
    /// Canonical, full, path to the device.
    path: PathBuf,
}
