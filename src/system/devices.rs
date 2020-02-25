//! Interface to devices on linux
//!
//! Linux primarily exposes connected devices through sysfs,
//! most of those interfaces undocumented.

pub mod block;
pub mod raw;
