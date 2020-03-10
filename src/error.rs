//! Error handling stuff
use displaydoc::Display;
use std::io;
use thiserror::Error;

/// Error type for [`linapi::modules`]
#[derive(Debug, Display, Error)]
pub enum ModuleError {
    /// IO Failed
    Io(#[from] io::Error),

    /// Couldn't load module {0}: {1}
    LoadError(String, String),

    /// Couldn't unload module {0}: {1}
    UnloadError(String, String),

    /// Module was invalid: `{0}`
    InvalidModule(String),
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
