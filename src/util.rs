//! Utility functions

/// Technically Linux requires sysfs to be at `/sys`, calling it a system
/// configuration error otherwise.
///
/// But our upcoming distro is planning to experiment with filesystem layout
/// changes, including of `/sys`, so do this to allow easily changing it.
pub const SYSFS_PATH: &str = "/sys";

/// Kernel Module location. Same reasons as [`SYSFS_PATH`].
pub const MODULE_PATH: &str = "/lib/modules";

/// Device file location. Same reasons as [`SYSFS_PATH`].
pub const DEV_PATH: &str = "/dev";
