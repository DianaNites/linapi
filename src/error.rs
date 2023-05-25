//! Error handling stuff
use std::io;

use displaydoc::Display;
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
