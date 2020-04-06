//! High level bindings to various Linux APIs and interfaces
//!
//! # Implementation details
//!
//! Most Linux APIs and interfaces are provided through files in `/sys` and
//! `/proc`, so this library requires them to exist.
//!
//! Most of these interfaces are also undocumented, and some may change between
//! kernel versions.
//!
//! This crate attempts to correctly document these interfaces, and provide
//! kernel documentation sources where possible.
//! This is done on a best effort basis.
#![doc(html_root_url = "https://docs.rs/linapi/0.5.1")]

pub mod error;
pub mod extensions;

pub mod system;
mod util;
