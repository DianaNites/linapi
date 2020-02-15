//! An interface to the Linux `/sys` filesystem, or sysfs.
//!
//! This module provides a (hopefully) idiomatic interface to the Linux sysfs,
//! allowing both reading and modifying configuration.
//!
//! # Implementation Details
//!
//! This is the userspace interface to low-level kernel details, and is subject
//! to change between kernel versions.
//!
//! The kernel documents this filesystem in a variety of different places, and
//! this crate attempts to document it's exposed interfaces correctly and as
//! best as possible. Sources will be linked if possible.
//!
//! # Stability
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
//! library.
//!
//! [1]: https://www.kernel.org/doc/Documentation/ABI/README

/// Technically Linux requires sysfs to be at `/sys`, calling it a system
/// configuration error otherwise.
///
/// But theres an upcoming distro planning to experiment with filesystem layout
/// changes, including of `/sys`, so do this to allow easily changing it.
pub(crate) const SYSFS_PATH: &str = "/sys";

pub mod modules;

pub mod interfaces;

mod util;
