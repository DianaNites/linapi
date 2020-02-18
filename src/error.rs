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

    /// Module was invalid: `{0}`
    InvalidModule(String),
}

/// Error text.
pub(crate) mod text {
    pub const INVALID_EXTENSION: &str = "invalid or missing extension";

    pub const COMPRESSION: &str = "unsupported or invalid compression";

    pub const NOT_FOUND: &str = "not found";

    pub const MODINFO: &str = "invalid .modinfo";
}