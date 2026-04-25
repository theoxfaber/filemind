//! File operations: copy/move with conflict resolution and smart renaming.

use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::config::ConflictStrategy;
use crate::error::{FileMindError, Result};

// ─── Smart rename ─────────────────────────────────────────────────────────────

/// Generate a smart filename: `YYYY-MM-DD — <Category> — <original>`.
pub fn smart_rename(filename: &str, category: &str) -> String {
    let date = Utc::now().format("%Y-%m-%d");
    let safe_cat: String = category
        .chars()
        .map(|c| if c == '/' { '-' } else { c })
        .collect();
    format!("{date} — {safe_cat} — {filename}")
}

// ─── Conflict resolution ──────────────────────────────────────────────────────

/// Resolve a destination path according to the conflict strategy.
///
/// Returns the final path to write to, creating any intermediate directories.
///
/// # Errors
/// Returns [`FileMindError::ConflictResolution`] if a unique path cannot
/// be determined within 999 attempts.
pub fn resolve_destination(
    dest_dir: &Path,
    filename: &str,
    strategy: &ConflictStrategy,
) -> Result<PathBuf> {
    std::fs::create_dir_all(dest_dir).map_err(FileMindError::Io)?;
    let candidate = dest_dir.join(filename);

    if !candidate.exists() {
        return Ok(candidate);
    }

    match strategy {
        ConflictStrategy::Skip => Ok(candidate),
        ConflictStrategy::Overwrite => Ok(candidate),
        ConflictStrategy::RenameNew => unique_path(dest_dir, filename),
        ConflictStrategy::RenameExisting => {
            let existing_renamed = unique_path(dest_dir, filename)?;
            std::fs::rename(&candidate, &existing_renamed).map_err(FileMindError::Io)?;
            Ok(candidate)
        }
    }
}

/// Produce a unique path by appending ` (N)` before the extension.
fn unique_path(dir: &Path, filename: &str) -> Result<PathBuf> {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();

    for n in 1..=999 {
        let name = format!("{stem} ({n}){ext}");
        let candidate = dir.join(&name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(FileMindError::ConflictResolution {
        path: dir.join(filename).to_string_lossy().into_owned(),
    })
}

// ─── Copy / move ──────────────────────────────────────────────────────────────

/// Copy `src` to `dest`, creating parent dirs as needed.
pub fn copy_file(src: &Path, dest: &Path) -> Result<()> {
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p).map_err(FileMindError::Io)?;
    }
    std::fs::copy(src, dest).map_err(FileMindError::Io)?;
    Ok(())
}

/// Move `src` to `dest`, creating parent dirs as needed.
///
/// Falls back to copy + delete if the rename crosses filesystem boundaries.
pub fn move_file(src: &Path, dest: &Path) -> Result<()> {
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p).map_err(FileMindError::Io)?;
    }
    match std::fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Cross-device rename: fall back to copy + remove
            std::fs::copy(src, dest).map_err(FileMindError::Io)?;
            std::fs::remove_file(src).map_err(FileMindError::Io)?;
            Ok(())
        }
    }
}

/// Zip the entire `output_dir` into `zip_path`.
pub fn pack_to_zip(output_dir: &Path, zip_path: &Path) -> Result<()> {
    use std::io::Write;
    use walkdir::WalkDir;
    use zip::write::SimpleFileOptions;

    let file = std::fs::File::create(zip_path).map_err(FileMindError::Io)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for entry in WalkDir::new(output_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        if path.components().any(|c| c.as_os_str() == ".filemind") {
            continue;
        }
        let rel = path
            .strip_prefix(output_dir)
            .unwrap_or(path)
            .to_string_lossy();
        zip.start_file(rel.as_ref(), opts)?;
        let data = std::fs::read(path).map_err(FileMindError::Io)?;
        zip.write_all(&data).map_err(FileMindError::Io)?;
    }
    zip.finish()?;
    Ok(())
}

/// Mirror `output_dir` into `target` (copy all files, preserving sub-structure).
pub fn sync_to_dir(output_dir: &Path, target: &Path) -> Result<()> {
    use walkdir::WalkDir;

    for entry in WalkDir::new(output_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let src = entry.path();
        if src.components().any(|c| c.as_os_str() == ".filemind") {
            continue;
        }
        let rel = src.strip_prefix(output_dir).unwrap_or(src);
        let dest = target.join(rel);
        copy_file(src, &dest)?;
    }
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn smart_rename_format() {
        let name = smart_rename("report.pdf", "Documents/Invoices");
        assert!(name.contains("Documents-Invoices"));
        assert!(name.contains("report.pdf"));
        assert!(name.chars().next().unwrap().is_ascii_digit());
    }

    #[test]
    fn test_conflict_rename_new() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("file.txt"), b"existing").unwrap();
        let dest =
            resolve_destination(dir.path(), "file.txt", &ConflictStrategy::RenameNew).unwrap();
        assert_ne!(dest, dir.path().join("file.txt"));
        assert!(dest.to_string_lossy().contains("(1)"));
    }

    #[test]
    fn test_conflict_rename_99() {
        let dir = TempDir::new().unwrap();
        // Create file.txt and file (1).txt through file (98).txt
        std::fs::write(dir.path().join("file.txt"), b"existing").unwrap();
        for n in 1..=98 {
            std::fs::write(dir.path().join(format!("file ({n}).txt")), b"existing").unwrap();
        }
        let dest =
            resolve_destination(dir.path(), "file.txt", &ConflictStrategy::RenameNew).unwrap();
        assert!(dest.to_string_lossy().contains("(99)"));
    }

    #[test]
    fn overwrite_returns_same_path() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("file.txt"), b"existing").unwrap();
        let dest =
            resolve_destination(dir.path(), "file.txt", &ConflictStrategy::Overwrite).unwrap();
        assert_eq!(dest, dir.path().join("file.txt"));
    }

    #[test]
    fn copy_file_works() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("sub/dst.txt");
        std::fs::write(&src, b"hello").unwrap();
        copy_file(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello");
    }

    #[test]
    fn test_cross_device_move_fallback() {
        // Simulate: move within same tmpdir succeeds, verify file ends up at dest
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("sub/dst.txt");
        std::fs::write(&src, b"hello").unwrap();
        move_file(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello");
        assert!(!src.exists());
    }
}
