//! Error handling stuff
use displaydoc::Display;
use std::io;
use thiserror::Error;

/// Error type for [`linapi::modules`]
#[derive(Debug, Display, Error)]
pub enum ModuleError {
    /// IO Failed: {0}
    Io(#[from] io::Error),

    /// Couldn't load module {0}: {1}
    LoadError(String, String),

    /// Couldn't unload module {0}: {1}
    UnloadError(String, String),

    /// Module was invalid: `{0}`
    InvalidModule(String),
}

/// Error type for [`linapi::types`]
#[derive(Debug, Display, Error)]
pub enum DeviceError {
    /// IO Failed: {0}
    Io(#[from] io::Error),

    /// Device was invalid: `{0}`
    InvalidDevice(&'static str),
}

/// Error text.
pub(crate) mod text {
    pub const INVALID_EXTENSION: &str = "invalid or missing extension";

    pub const COMPRESSION: &str = "unsupported or invalid compression";

    pub const NOT_FOUND: &str = "not found";

    pub const NAME: &str = "invalid module name";

    pub const PARAMETER: &str = "invalid module parameter name";

    pub const MODINFO: &str = "invalid .modinfo";
}

pub(crate) mod device_text {
    pub const DEVICE: &str = "missing expected attribute";
}
