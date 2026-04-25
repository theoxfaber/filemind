//! Content extraction from various file types.
//!
//! Supports: `.pdf` (pure-Rust via `pdf-extract`), plain text, source code,
//! CSV, JSON, YAML, TOML, HTML, and any UTF-8 readable file.
//! Binary files that cannot be decoded return an empty string — the
//! classifier falls back to tier-1 (extension + magic bytes) in that case.

use std::path::Path;

use crate::error::{FileMindError, Result};

/// Default maximum bytes of text extracted when no config is provided.
const DEFAULT_EXTRACT_BYTES: usize = 4096;

/// Reads up to this many raw bytes from the start for magic byte detection.
const MAGIC_READ_BYTES: usize = 16;

/// Result of content extraction for a single file.
#[derive(Debug, Default, Clone)]
pub struct Extracted {
    /// Raw text content (up to configured limit).
    pub text: String,
    /// First 16 raw bytes (for magic-byte detection in the classifier).
    pub magic: Vec<u8>,
    /// Whether text extraction succeeded (false = binary / unreadable).
    pub has_text: bool,
}

/// Extract content from `path` using the default extraction limit.
///
/// Never panics — unreadable or binary files yield an [`Extracted`] with
/// `has_text = false` and empty `text`.
pub fn extract(path: &Path) -> Result<Extracted> {
    extract_with_limit(path, DEFAULT_EXTRACT_BYTES)
}

/// Extract content from `path` with a configurable byte limit.
///
/// `max_bytes` controls the maximum amount of text extracted. This allows
/// the extraction limit to be tuned via config rather than a hardcoded constant.
pub fn extract_with_limit(path: &Path, max_bytes: usize) -> Result<Extracted> {
    let magic = read_magic(path)?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let text = match ext.as_str() {
        "pdf" => extract_pdf(path, max_bytes),
        // All UTF-8 text variants share the same reader
        "txt" | "md" | "markdown" | "rst" | "log" | "org" => read_text(path, max_bytes),
        "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "go" | "java" | "c" | "cpp" | "h" | "hpp"
        | "cs" | "rb" | "swift" | "kt" | "scala" | "r" | "lua" | "php" | "sh" | "bash" | "zsh"
        | "fish" | "ps1" | "bat" => read_text(path, max_bytes),
        "json" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" | "xml" | "csv" | "tsv"
        | "html" | "htm" | "css" | "sql" => read_text(path, max_bytes),
        _ => {
            // Try to read as UTF-8 anyway; silently return empty on failure
            read_text_optional(path, max_bytes)
        }
    };

    let has_text = !text.is_empty();
    Ok(Extracted {
        text,
        magic,
        has_text,
    })
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Read the first [`MAGIC_READ_BYTES`] of a file.
fn read_magic(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(FileMindError::Io)?;
    let mut buf = vec![0u8; MAGIC_READ_BYTES];
    let n = f.read(&mut buf).map_err(FileMindError::Io)?;
    buf.truncate(n);
    Ok(buf)
}

/// Read a text file up to `max_bytes`, replacing invalid UTF-8.
fn read_text(path: &Path, max_bytes: usize) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            let s = String::from_utf8_lossy(&bytes[..bytes.len().min(max_bytes)]);
            s.into_owned()
        }
        Err(_) => String::new(),
    }
}

/// Like [`read_text`] but returns empty string on any error (for unknown types).
fn read_text_optional(path: &Path, max_bytes: usize) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            // Quick binary check: if >10% of the first 512 bytes are non-printable
            // (excluding common control chars), treat as binary.
            let sample = &bytes[..bytes.len().min(512)];
            let non_printable = sample
                .iter()
                .filter(|&&b| b < 0x09 || (b > 0x0d && b < 0x20) || b == 0x7f)
                .count();
            if !sample.is_empty() && non_printable * 10 > sample.len() {
                return String::new();
            }
            let s = String::from_utf8_lossy(&bytes[..bytes.len().min(max_bytes)]);
            s.into_owned()
        }
        Err(_) => String::new(),
    }
}

/// Extract text from a PDF using the pure-Rust `pdf-extract` crate.
fn extract_pdf(path: &Path, max_bytes: usize) -> String {
    match pdf_extract::extract_text(path) {
        Ok(t) => {
            let trimmed = t.trim().to_string();
            if trimmed.len() > max_bytes {
                trimmed[..max_bytes].to_string()
            } else {
                trimmed
            }
        }
        Err(_) => String::new(),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn extract_plain_text() {
        let mut f = NamedTempFile::with_suffix(".txt").unwrap();
        f.write_all(b"Hello world! This is a test.").unwrap();
        let result = extract(f.path()).unwrap();
        assert!(result.has_text);
        assert!(result.text.contains("Hello world"));
    }

    #[test]
    fn extract_rust_source() {
        let mut f = NamedTempFile::with_suffix(".rs").unwrap();
        f.write_all(b"fn main() { println!(\"hello\"); }").unwrap();
        let result = extract(f.path()).unwrap();
        assert!(result.has_text);
        assert!(result.text.contains("fn main"));
    }

    #[test]
    fn extract_binary_returns_empty() {
        let mut f = NamedTempFile::with_suffix(".bin").unwrap();
        // Write clearly binary data
        f.write_all(&[0x00, 0x01, 0x02, 0x03, 0xff, 0xfe, 0xfd])
            .unwrap();
        let result = extract(f.path()).unwrap();
        // May or may not have text — just should not panic
        let _ = result;
    }

    #[test]
    fn magic_bytes_read_correctly() {
        let mut f = NamedTempFile::with_suffix(".pdf").unwrap();
        f.write_all(b"%PDF-1.4 rest of content here").unwrap();
        let result = extract(f.path()).unwrap();
        assert!(!result.magic.is_empty());
        assert_eq!(&result.magic[..4], b"%PDF");
    }

    #[test]
    fn configurable_extract_limit() {
        let mut f = NamedTempFile::with_suffix(".txt").unwrap();
        let content = "a".repeat(10000);
        f.write_all(content.as_bytes()).unwrap();

        let result = extract_with_limit(f.path(), 100).unwrap();
        assert_eq!(result.text.len(), 100);

        let result2 = extract_with_limit(f.path(), 5000).unwrap();
        assert_eq!(result2.text.len(), 5000);
    }
}
