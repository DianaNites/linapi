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
    pub mod ioctl;
}

//
pub mod extensions;
pub mod modules;
pub mod types;
