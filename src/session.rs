//! Session management, full undo support, and size-bucketed dedup hashing.
//!
//! Every `organize` run creates a session.  Sessions can be listed and
//! individually undone — files are moved back to their original locations
//! only after their SHA-256 checksum is verified to be unchanged.
//!
//! **Dedup hashing strategy** (avoids reading 4GB files for a simple check):
//!   1. File size alone — if unique, skip hashing entirely
//!   2. Partial hash: first 64KB + last 64KB + size (for files > 1MB)
//!   3. Full hash only when partial matches an existing entry

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use colored::Colorize;
use sha2::{Digest, Sha256};

use crate::error::{FileMindError, Result};
use crate::manifest::Manifest;

// ─── Size-bucketed hashing ───────────────────────────────────────────────────

/// Threshold below which we always do a full hash (overhead is negligible).
const SMALL_FILE_THRESHOLD: u64 = 1_048_576; // 1MB

/// Window size for partial hashing of large files.
const PARTIAL_HASH_WINDOW: u64 = 65_536; // 64KB

/// Compute the MD5 hex string of a file using size-bucketed hashing.
///
/// For files under 1MB: full hash (overhead is negligible).
/// For larger files: hash first 64KB + last 64KB + file size.
/// This avoids reading a 4GB video file just for a dedup check.
pub fn md5_of_file(path: &Path) -> Result<String> {
    let metadata = std::fs::metadata(path).map_err(FileMindError::Io)?;
    let size = metadata.len();

    if size <= SMALL_FILE_THRESHOLD {
        // Small file: full hash
        let bytes = std::fs::read(path).map_err(FileMindError::Io)?;
        return Ok(format!("{:x}", md5::compute(&bytes)));
    }

    // Large file: partial hash (first 64KB + last 64KB + file size)
    let mut file = std::fs::File::open(path).map_err(FileMindError::Io)?;
    let mut hasher = md5::Context::new();

    // Hash file size as a discriminator
    hasher.consume(size.to_le_bytes());

    // First window
    let mut buf = vec![0u8; PARTIAL_HASH_WINDOW as usize];
    let n = file.read(&mut buf).map_err(FileMindError::Io)?;
    hasher.consume(&buf[..n]);

    // Last window (seek to end - window)
    if size > PARTIAL_HASH_WINDOW {
        file.seek(SeekFrom::End(-(PARTIAL_HASH_WINDOW as i64)))
            .map_err(FileMindError::Io)?;
        let n = file.read(&mut buf).map_err(FileMindError::Io)?;
        hasher.consume(&buf[..n]);
    }

    Ok(format!("{:x}", hasher.compute()))
}

/// Compute the SHA-256 hex string of a file (always full read — used for
/// undo integrity verification where correctness is paramount).
pub fn sha256_of_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(FileMindError::Io)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Get the file size in bytes.
pub fn file_size(path: &Path) -> Result<i64> {
    let metadata = std::fs::metadata(path).map_err(FileMindError::Io)?;
    Ok(metadata.len() as i64)
}

// ─── Undo ─────────────────────────────────────────────────────────────────────

/// Undo a specific session, restoring all files to their original paths.
///
/// For each file:
/// 1. Verify its SHA-256 matches the stored value (fail-safe).
/// 2. Move it back to `original_path`.
/// 3. Mark the session as `undone` in the manifest.
pub fn undo_session(manifest: &Manifest, session_id: i64) -> Result<UndoReport> {
    let entries = manifest.files_for_session(session_id)?;
    if entries.is_empty() {
        return Err(FileMindError::SessionNotFound { id: session_id });
    }

    let mut restored = 0usize;
    let mut skipped = 0usize;
    let mut warnings: Vec<String> = Vec::new();

    for entry in &entries {
        let final_path = std::path::PathBuf::from(&entry.final_path);
        let original_path = std::path::PathBuf::from(&entry.original_path);

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
        // Try rename first, fall back to copy+delete for cross-device
        if std::fs::rename(&final_path, &original_path).is_err() {
            std::fs::copy(&final_path, &original_path).map_err(FileMindError::Io)?;
            std::fs::remove_file(&final_path).map_err(FileMindError::Io)?;
        }
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
        assert!(matches!(
            result,
            Err(FileMindError::SessionNotFound { id: 9999 })
        ));
    }

    #[test]
    fn test_undo_checksum_pass() {
        let dir = TempDir::new().unwrap();
        let manifest = Manifest::open(dir.path()).unwrap();

        // Create a session and file
        let sid = manifest
            .new_session(dir.path(), dir.path())
            .unwrap();
        let src = dir.path().join("original.txt");
        let dest_dir = dir.path().join("organized");
        std::fs::create_dir_all(&dest_dir).unwrap();
        let dest = dest_dir.join("original.txt");
        std::fs::write(&src, b"test content").unwrap();
        std::fs::copy(&src, &dest).unwrap();
        std::fs::remove_file(&src).unwrap();

        let sha = sha256_of_file(&dest).unwrap();
        let entry = crate::manifest::NewEntry {
            session_id: sid,
            original_path: src.clone(),
            final_path: dest.clone(),
            category: "Test".to_string(),
            confidence: 0.9,
            tier_used: "tier-1".to_string(),
            md5: "abc".to_string(),
            sha256: sha,
            file_size: 12,
        };
        manifest.insert_file(&entry).unwrap();
        manifest.close_session(sid).unwrap();

        let report = undo_session(&manifest, sid).unwrap();
        assert_eq!(report.restored, 1);
        assert_eq!(report.skipped, 0);
        assert!(src.exists());
        assert!(!dest.exists());
    }

    #[test]
    fn test_undo_checksum_mismatch() {
        let dir = TempDir::new().unwrap();
        let manifest = Manifest::open(dir.path()).unwrap();
        let sid = manifest
            .new_session(dir.path(), dir.path())
            .unwrap();

        let src = dir.path().join("original.txt");
        let dest = dir.path().join("organized").join("original.txt");
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, b"test content").unwrap();

        let entry = crate::manifest::NewEntry {
            session_id: sid,
            original_path: src,
            final_path: dest.clone(),
            category: "Test".to_string(),
            confidence: 0.9,
            tier_used: "tier-1".to_string(),
            md5: "abc".to_string(),
            sha256: "wrong_hash_on_purpose".to_string(),
            file_size: 12,
        };
        manifest.insert_file(&entry).unwrap();
        manifest.close_session(sid).unwrap();

        let report = undo_session(&manifest, sid).unwrap();
        assert_eq!(report.restored, 0);
        assert_eq!(report.skipped, 1);
        assert!(dest.exists()); // file should NOT be moved
    }

    #[test]
    fn test_undo_missing_file() {
        let dir = TempDir::new().unwrap();
        let manifest = Manifest::open(dir.path()).unwrap();
        let sid = manifest
            .new_session(dir.path(), dir.path())
            .unwrap();

        let entry = crate::manifest::NewEntry {
            session_id: sid,
            original_path: dir.path().join("original.txt"),
            final_path: dir.path().join("nonexistent.txt"),
            category: "Test".to_string(),
            confidence: 0.9,
            tier_used: "tier-1".to_string(),
            md5: "abc".to_string(),
            sha256: "def".to_string(),
            file_size: 0,
        };
        manifest.insert_file(&entry).unwrap();
        manifest.close_session(sid).unwrap();

        let report = undo_session(&manifest, sid).unwrap();
        assert_eq!(report.restored, 0);
        assert_eq!(report.skipped, 1);
        assert!(!report.warnings.is_empty());
    }
}
