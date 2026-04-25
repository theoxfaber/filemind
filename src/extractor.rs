use anyhow::Result;
use std::path::Path;

/// Maximum characters extracted from any file
const MAX_CHARS: usize = 3000;

/// Extracts readable text from a file based on its extension.
/// Supported: .txt, .md, .pdf, .rs, .py, .js, .ts, .json, .csv, .yaml, .toml, .html
/// Unsupported types return an empty string (no crash).
pub fn extract_text(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let text = match ext.as_str() {
        // Plain text types
        "txt" | "md" | "rs" | "py" | "js" | "ts" | "json" | "csv" | "yaml" | "yml"
        | "toml" | "html" | "htm" | "css" | "sh" | "log" | "xml" => {
            std::fs::read_to_string(path)
                .unwrap_or_default()
        }

        // PDF — pure-Rust extraction via pdf-extract
        "pdf" => extract_pdf(path),

        // Everything else: no text extraction, rely on filename only
        _ => String::new(),
    };

    Ok(truncate(text, MAX_CHARS))
}

fn extract_pdf(path: &Path) -> String {
    match pdf_extract::extract_text(path) {
        Ok(t) => t,
        Err(_) => String::new(),
    }
}

fn truncate(s: String, max: usize) -> String {
    if s.len() <= max {
        s
    } else {
        s[..max].to_string()
    }
}
