//! Error handling stuff
use displaydoc::Display;
use std::{fmt, io};
use thiserror::Error;

/// Error type for [`crate::system::modules`]
#[derive(Debug, Display, Error)]
pub enum ModuleError {
    /// I/O Error
    Io(#[from] io::Error),

    /// Couldn't load module {0}: {1}
    LoadError(String, String),

    /// Couldn't unload module {0}: {1}
    UnloadError(String, String),

    /// Module was invalid: `{0}`
    InvalidModule(String),
}

#[derive(Debug, Display)]
pub enum ModuleErrorKind {
    /// Couldn't load module: `{0}`
    LoadError(String),

    /// Couldn't unload module: `{0}`
    UnloadError(String),

    /// Module compression `{0}` unsupported
    UnsupportedCompression(String),

    /// Module .modinfo was invalid
    InvalidModInfo,

    /// Module did not exist at `{0}`
    NotFound(String),

    /// Module was invalid: `{0}`
    InvalidModule(String),
}

/// Error type for [`crate::system::modules`]
#[derive(Debug)]
pub struct ModuleError_ {
    kind: ModuleErrorKind,
    source: Option<anyhow::Error>,
}

impl ModuleError_ {
    pub fn new(kind: ModuleErrorKind, source: impl Into<anyhow::Error>) -> Self {
        Self {
            kind,
            source: Some(source.into()),
        }
    }

    pub fn with_none(kind: ModuleErrorKind) -> Self {
        Self { kind, source: None }
    }
}

impl std::error::Error for ModuleError_ {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref())
    }
}

impl fmt::Display for ModuleError_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

/// Error text.
pub(crate) mod text {
    pub const INVALID_EXTENSION: &str = "invalid or missing extension";

    pub const COMPRESSION: &str = "unsupported or invalid compression";

    pub const NOT_FOUND: &str = "not found";

    pub const NAME: &str = "invalid module name";

    pub const MODINFO: &str = "invalid .modinfo";
}
