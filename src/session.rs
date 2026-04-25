//! Session management and full undo support.
//!
//! Every `organize` run creates a session.  Sessions can be listed and
//! individually undone — files are moved back to their original locations
//! only after their SHA-256 checksum is verified to be unchanged.

use std::path::{Path, PathBuf};

use colored::Colorize;
use sha2::{Digest, Sha256};

use crate::error::{FileMindError, Result};
use crate::manifest::Manifest;

// ─── Checksum helpers ─────────────────────────────────────────────────────────

/// Compute the MD5 hex string of a file.
///
/// # Errors
/// Returns [`FileMindError::Io`] if the file cannot be read.
pub fn md5_of_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(FileMindError::Io)?;
    Ok(format!("{:x}", md5::compute(&bytes)))
}

/// Compute the SHA-256 hex string of a file.
///
/// # Errors
/// Returns [`FileMindError::Io`] if the file cannot be read.
pub fn sha256_of_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(FileMindError::Io)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

// ─── Undo ─────────────────────────────────────────────────────────────────────

/// Undo a specific session, restoring all files to their original paths.
///
/// For each file:
/// 1. Verify its SHA-256 matches the stored value (fail-safe).
/// 2. Move it back to `original_path`.
/// 3. Mark the session as `undone` in the manifest.
///
/// # Errors
/// Returns [`FileMindError::SessionNotFound`] if the session does not exist,
/// or [`FileMindError::ChecksumMismatch`] if a file was modified after organizing.
pub fn undo_session(manifest: &Manifest, session_id: i64) -> Result<UndoReport> {
    let entries = manifest.files_for_session(session_id)?;
    if entries.is_empty() {
        return Err(FileMindError::SessionNotFound { id: session_id });
    }

    let mut restored = 0usize;
    let mut skipped = 0usize;
    let mut warnings: Vec<String> = Vec::new();

    for entry in &entries {
        let final_path = PathBuf::from(&entry.final_path);
        let original_path = PathBuf::from(&entry.original_path);

        if !final_path.exists() {
            warnings.push(format!(
                "  {} {} (already missing, skipped)",
                "⚠".yellow(),
                entry.final_path
            ));
            skipped += 1;
            continue;
        }

        // Verify SHA-256 before restoring
        let current_sha = sha256_of_file(&final_path)?;
        if current_sha != entry.sha256 {
            warnings.push(format!(
                "  {} {} — checksum mismatch, file modified after organizing (skipped)",
                "✗".red(),
                entry.final_path
            ));
            skipped += 1;
            continue;
        }

        // Restore: create parent dirs if needed, then rename
        if let Some(parent) = original_path.parent() {
            std::fs::create_dir_all(parent).map_err(FileMindError::Io)?;
        }
        std::fs::rename(&final_path, &original_path).map_err(FileMindError::Io)?;
        restored += 1;
    }

    // Update manifest
    manifest.delete_session_files(session_id)?;

    Ok(UndoReport {
        session_id,
        restored,
        skipped,
        warnings,
    })
}

/// Summary of an undo operation.
#[derive(Debug)]
pub struct UndoReport {
    pub session_id: i64,
    pub restored: usize,
    pub skipped: usize,
    pub warnings: Vec<String>,
}

impl std::fmt::Display for UndoReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Session {} — {} restored, {} skipped",
            self.session_id, self.restored, self.skipped
        )
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn md5_consistent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, b"hello filemind").unwrap();
        let h1 = md5_of_file(&path).unwrap();
        let h2 = md5_of_file(&path).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 32);
    }

    #[test]
    fn sha256_changes_on_modification() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, b"original content").unwrap();
        let h1 = sha256_of_file(&path).unwrap();
        std::fs::write(&path, b"modified content").unwrap();
        let h2 = sha256_of_file(&path).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn undo_nonexistent_session_errors() {
        let dir = TempDir::new().unwrap();
        let manifest = Manifest::open(dir.path()).unwrap();
        let result = undo_session(&manifest, 9999);
        assert!(matches!(result, Err(FileMindError::SessionNotFound { id: 9999 })));
    }
}
