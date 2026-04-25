//! Typed errors for FileMind.
//!
//! Every public function returns [`Result<T>`] (this crate's alias) so callers
//! can handle individual variants without stringly-typed error inspection.

use thiserror::Error;

/// The canonical error type for all FileMind operations.
#[derive(Error, Debug)]
pub enum FileMindError {
    /// Underlying OS / I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Configuration file parse or validation error.
    #[error("Config error: {0}")]
    Config(String),

    /// SQLite database error (manifest or sessions).
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Requested session does not exist in the manifest.
    #[error("Session {id} not found")]
    SessionNotFound { id: i64 },

    /// File was modified after being organized; undo is unsafe.
    #[error("Checksum mismatch for '{path}': stored {expected}, found {actual}")]
    ChecksumMismatch {
        path: String,
        expected: String,
        actual: String,
    },

    /// Text extraction could not be completed.
    #[error("Extraction failed for '{path}': {reason}")]
    ExtractionFailed { path: String, reason: String },

    /// File-system watcher returned an error.
    #[error("Watcher error: {0}")]
    Watcher(String),

    /// Conflict-resolution could not produce a unique path.
    #[error("Cannot resolve conflict for '{path}'")]
    ConflictResolution { path: String },

    /// TOML config could not be deserialized.
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    /// Directory walk failure.
    #[error("Walk error: {0}")]
    Walk(#[from] walkdir::Error),

    /// Zip archive creation failed.
    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, FileMindError>;
