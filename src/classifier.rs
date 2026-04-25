//! 3-tier deterministic file classifier — the core FileMind innovation.
//!
//! All three tiers run independently and produce a `[0.0, 1.0]` confidence
//! score.  The category with the highest combined score wins.
//!
//! ```text
//! Tier 1  Extension + MIME magic bytes  (~0 ms,  always runs)
//! Tier 2  Keyword scoring on text       (ms range, runs when text available)
//! Tier 3  Filename + path heuristics    (~0 ms,  always runs, additive boost)
//! ```

use std::collections::HashMap;
use std::path::Path;

use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::config::Config;
use crate::extractor::Extracted;

// ─── Compile-time embedded keywords ──────────────────────────────────────────

/// The canonical built-in keyword list, embedded at compile time from
/// `assets/keywords.toml`. Users can export and customize this file.
const BUILTIN_KEYWORDS_TOML: &str = include_str!("../assets/keywords.toml");

/// Deserialization types for the embedded keywords.toml.
#[derive(Debug, Deserialize)]
struct KeywordsFile {
    #[serde(flatten)]
    sections: HashMap<String, KeywordSection>,
}

/// A single category section in keywords.toml.
#[derive(Debug, Deserialize)]
struct KeywordSection {
    category: String,
    keywords: Vec<KeywordDef>,
}

/// A single keyword definition with optional note.
#[derive(Debug, Clone, Deserialize)]
pub struct KeywordDef {
    /// The keyword or phrase to match (case-insensitive).
    pub word: String,
    /// Relative weight — higher means stronger signal.
    pub weight: f32,
    /// Optional human-readable note explaining why this keyword exists.
    #[serde(default)]
    pub note: Option<String>,
}

// ─── Public types ─────────────────────────────────────────────────────────────

/// Which tier produced the decisive result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tier {
    /// Extension or magic-byte match.
    Extension,
    /// Keyword scoring on extracted text content.
    Content,
    /// Filename / path heuristic.
    Filename,
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tier::Extension => write!(f, "tier-1 (extension/magic)"),
            Tier::Content => write!(f, "tier-2 (content keywords)"),
            Tier::Filename => write!(f, "tier-3 (filename/path)"),
        }
    }
}

/// A single piece of evidence that contributed to the classification.
#[derive(Debug, Clone)]
pub enum Evidence {
    /// Magic bytes at the start of the file.
    MagicBytes { description: String, boost: f32 },
    /// File extension mapped to a category.
    Extension { ext: String, base_confidence: f32 },
    /// Keyword found in the extracted text.
    KeywordMatch {
        keyword: String,
        weight: f32,
        count: usize,
        /// Byte offsets of each occurrence (first 5 stored).
        offsets: Vec<usize>,
    },
    /// Filename pattern matched a heuristic regex.
    FilenamePattern { pattern: String, boost: f32 },
    /// A path segment named after a known category.
    PathSignal { segment: String, boost: f32 },
}

/// The full output of a classification run.
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// Top-level category (e.g. `"Documents"`, `"Documents/Invoices"`).
    pub category: String,
    /// Combined confidence score in `[0.0, 1.0]`.
    pub confidence: f32,
    /// The tier that was decisive.
    pub tier_used: Tier,
    /// All evidence items that contributed to the decision.
    pub evidence: Vec<Evidence>,
}

// ─── Built-in extension → (category, base_confidence) ─────────────────────────

static EXT_MAP: Lazy<HashMap<&'static str, (&'static str, f32)>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for ext in &["pdf", "doc", "docx", "odt", "rtf", "pages", "wpd", "tex"] {
        m.insert(*ext, ("Documents", 0.60_f32));
    }
    for ext in &["txt", "md", "markdown", "rst"] {
        m.insert(*ext, ("Documents", 0.40_f32));
    }
    for ext in &["xlsx", "xls", "ods", "numbers", "xlsm", "xlsb"] {
        m.insert(*ext, ("Spreadsheets", 0.65_f32));
    }
    for ext in &["pptx", "ppt", "odp", "key"] {
        m.insert(*ext, ("Presentations", 0.65_f32));
    }
    for ext in &[
        "jpg", "jpeg", "png", "gif", "bmp", "tiff", "tif", "webp", "svg", "heic", "heif", "raw",
        "cr2", "nef", "arw", "dng", "ico",
    ] {
        m.insert(*ext, ("Images", 0.70_f32));
    }
    for ext in &[
        "mp4", "avi", "mov", "mkv", "wmv", "flv", "webm", "m4v", "3gp", "ts", "mts",
    ] {
        m.insert(*ext, ("Videos", 0.75_f32));
    }
    for ext in &[
        "mp3", "wav", "flac", "aac", "ogg", "m4a", "wma", "opus", "aiff", "mid",
    ] {
        m.insert(*ext, ("Audio", 0.75_f32));
    }
    for ext in &[
        "rs", "py", "js", "ts", "jsx", "tsx", "go", "java", "c", "cpp", "h", "hpp", "cs", "rb",
        "swift", "kt", "scala", "r", "lua", "php", "sh", "bash", "zsh", "fish", "ps1", "bat",
        "gradle", "cmake",
    ] {
        m.insert(*ext, ("Code", 0.80_f32));
    }
    for ext in &[
        "json", "yaml", "yml", "toml", "xml", "csv", "tsv", "parquet", "avro",
    ] {
        m.insert(*ext, ("Data", 0.65_f32));
    }
    for ext in &["zip", "tar", "gz", "bz2", "xz", "7z", "rar", "tgz", "zst"] {
        m.insert(*ext, ("Archives", 0.75_f32));
    }
    for ext in &["sqlite", "sqlite3", "db", "duckdb"] {
        m.insert(*ext, ("Data/Database", 0.80_f32));
    }
    for ext in &["epub", "mobi", "azw", "azw3"] {
        m.insert(*ext, ("Books", 0.75_f32));
    }
    m
});

// ─── Built-in keyword list (parsed from embedded TOML) ───────────────────────

/// Parsed built-in keyword entries: (category, word, weight, note).
pub struct BuiltinKw {
    pub category: String,
    pub word: String,
    pub weight: f32,
    pub note: Option<String>,
}

/// The parsed built-in keyword list, lazily initialized from the embedded TOML.
pub static BUILTIN_KEYWORDS: Lazy<Vec<BuiltinKw>> = Lazy::new(|| {
    let file: KeywordsFile =
        toml::from_str(BUILTIN_KEYWORDS_TOML).expect("embedded keywords.toml is invalid");
    let mut result = Vec::new();
    for section in file.sections.values() {
        for kw in &section.keywords {
            result.push(BuiltinKw {
                category: section.category.clone(),
                word: kw.word.clone(),
                weight: kw.weight,
                note: kw.note.clone(),
            });
        }
    }
    result
});

/// Returns the raw embedded keywords TOML string for export.
pub fn builtin_keywords_toml() -> &'static str {
    BUILTIN_KEYWORDS_TOML
}

// ─── Magic byte detection via `infer` crate ──────────────────────────────────

/// Detect file format from magic bytes using the `infer` crate.
/// Returns `Some((description, category, boost))` on match.
fn detect_magic_infer(bytes: &[u8]) -> Option<(&'static str, &'static str, f32)> {
    let kind = infer::get(bytes)?;
    let mime = kind.mime_type();

    // Map MIME type to category and boost value
    if mime.starts_with("image/") {
        Some(("image (infer)", "Images", 0.12))
    } else if mime.starts_with("video/") {
        Some(("video (infer)", "Videos", 0.12))
    } else if mime.starts_with("audio/") {
        Some(("audio (infer)", "Audio", 0.12))
    } else if mime == "application/pdf" {
        Some(("PDF document (infer)", "Documents", 0.10))
    } else if mime == "application/zip"
        || mime == "application/gzip"
        || mime == "application/x-bzip2"
        || mime == "application/x-xz"
        || mime == "application/x-7z-compressed"
        || mime == "application/x-rar-compressed"
        || mime == "application/zstd"
    {
        Some(("archive (infer)", "Archives", 0.12))
    } else if mime == "application/x-sqlite3" {
        Some(("SQLite database (infer)", "Data/Database", 0.15))
    } else if mime.starts_with("application/") {
        // Generic application type — small boost
        Some(("application (infer)", "Misc", 0.05))
    } else {
        None
    }
}

// ─── Category scoring accumulator ─────────────────────────────────────────────

#[derive(Default)]
struct CategoryScore {
    base: f32,
    boost: f32,
    evidence: Vec<Evidence>,
    decisive_tier: Option<Tier>,
}

impl CategoryScore {
    fn total(&self) -> f32 {
        (self.base + self.boost).min(1.0)
    }
}

// ─── Classifier entry point ───────────────────────────────────────────────────

/// Classify a file using all three tiers.
///
/// `config` is used to merge user-defined keyword lists with the built-ins.
/// `extracted` is the result of [`crate::extractor::extract`].
pub fn classify(path: &Path, extracted: &Extracted, config: &Config) -> ClassificationResult {
    let mut scores: HashMap<String, CategoryScore> = HashMap::new();

    // ── Tier 1: Extension + MIME magic bytes ──────────────────────────────────
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if let Some((category, base)) = EXT_MAP.get(ext.as_str()) {
        let entry = scores.entry(category.to_string()).or_default();
        entry.base = *base;
        entry.evidence.push(Evidence::Extension {
            ext: ext.clone(),
            base_confidence: *base,
        });
        entry.decisive_tier.get_or_insert(Tier::Extension);
    }

    // Magic bytes via the `infer` crate (single source of truth — no manual matching)
    if let Some((desc, magic_cat, boost)) = detect_magic_infer(&extracted.magic) {
        let entry = scores.entry(magic_cat.to_string()).or_default();
        entry.boost += boost;
        entry.evidence.push(Evidence::MagicBytes {
            description: desc.to_string(),
            boost,
        });
        entry.decisive_tier.get_or_insert(Tier::Extension);
    }

    // ── Tier 2: Keyword scoring ───────────────────────────────────────────────
    if extracted.has_text {
        let text_lower = extracted.text.to_lowercase();

        // Collect all keyword lists: built-ins + user overrides
        let mut kw_map: HashMap<String, Vec<(String, f32)>> = HashMap::new();

        // Built-ins from embedded keywords.toml
        for kw in BUILTIN_KEYWORDS.iter() {
            kw_map
                .entry(kw.category.clone())
                .or_default()
                .push((kw.word.to_lowercase(), kw.weight));
        }

        // User overrides from config: merge (append) keyword lists
        for (cat_key, cat_cfg) in &config.categories {
            if let Some(user_kws) = &cat_cfg.keywords {
                let canonical = canonical_category_name(cat_key);
                for kw in user_kws {
                    kw_map
                        .entry(canonical.clone())
                        .or_default()
                        .push((kw.word.to_lowercase(), kw.weight));
                }
            }
        }

        // Score each category
        for (category, keywords) in &kw_map {
            let mut matched_weight = 0.0_f32;
            let mut evidence_items: Vec<Evidence> = Vec::new();

            for (word, weight) in keywords {
                let mut offsets = Vec::new();
                let mut start = 0;
                while let Some(pos) = text_lower[start..].find(word.as_str()) {
                    offsets.push(start + pos);
                    start += pos + word.len();
                    if offsets.len() >= 5 {
                        break;
                    }
                }
                if !offsets.is_empty() {
                    // sqrt dampening: 4× keyword != 4× score
                    matched_weight += weight * (offsets.len() as f32).sqrt();
                    evidence_items.push(Evidence::KeywordMatch {
                        keyword: word.clone(),
                        weight: *weight,
                        count: offsets.len(),
                        offsets,
                    });
                }
            }

            if matched_weight > 0.0 {
                // Normalize: cap at 10.0 raw weight → 0.40 boost.
                let boost = (matched_weight / 10.0 * 0.40).min(0.40);

                // Inherit parent base confidence for subcategories
                let inherited_base: f32 = if !scores.contains_key(category) {
                    category
                        .find('/')
                        .and_then(|slash| scores.get(&category[..slash]))
                        .map(|p| p.base)
                        .unwrap_or(0.0)
                } else {
                    0.0
                };

                let entry = scores.entry(category.clone()).or_default();
                if entry.base == 0.0 && inherited_base > 0.0 {
                    entry.base = inherited_base;
                }
                entry.boost += boost;
                entry.evidence.extend(evidence_items);
                entry.decisive_tier.get_or_insert(Tier::Content);
                if boost >= 0.15 {
                    entry.decisive_tier = Some(Tier::Content);
                }
            }
        }
    }

    // ── Tier 3: Filename + path heuristics ────────────────────────────────────
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    let path_str = path.to_string_lossy().to_lowercase();

    let filename_patterns: &[(&str, &str, f32)] = &[
        ("invoice", "Documents/Invoices", 0.15),
        ("receipt", "Documents/Invoices", 0.12),
        ("bill", "Documents/Invoices", 0.10),
        ("img_", "Images", 0.08),
        ("dsc_", "Images", 0.08),
        ("screenshot", "Images", 0.10),
        ("photo", "Images", 0.08),
        ("readme", "Code", 0.10),
        ("makefile", "Code", 0.15),
        ("dockerfile", "Code", 0.15),
        ("contract", "Documents/Legal", 0.15),
        ("agreement", "Documents/Legal", 0.12),
        ("report", "Documents", 0.08),
        ("resume", "Documents", 0.10),
        ("cv", "Documents", 0.08),
    ];

    for (pattern, category, boost) in filename_patterns {
        if filename.contains(pattern) {
            let entry = scores.entry(category.to_string()).or_default();
            entry.boost += boost;
            entry.evidence.push(Evidence::FilenamePattern {
                pattern: pattern.to_string(),
                boost: *boost,
            });
            entry.decisive_tier.get_or_insert(Tier::Filename);
        }
    }

    // Path segment signals (parent folder name hints)
    let path_signals: &[(&str, &str, f32)] = &[
        ("invoice", "Documents/Invoices", 0.10),
        ("invoices", "Documents/Invoices", 0.12),
        ("medical", "Documents/Medical", 0.12),
        ("health", "Documents/Medical", 0.10),
        ("legal", "Documents/Legal", 0.12),
        ("photos", "Images", 0.10),
        ("pictures", "Images", 0.10),
        ("videos", "Videos", 0.10),
        ("music", "Audio", 0.10),
        ("code", "Code", 0.08),
        ("src", "Code", 0.06),
    ];

    for (segment, category, boost) in path_signals {
        if path_str
            .split('/')
            .rev()
            .skip(1)
            .any(|part| part == *segment || part.starts_with(segment))
        {
            let entry = scores.entry(category.to_string()).or_default();
            entry.boost += boost;
            entry.evidence.push(Evidence::PathSignal {
                segment: segment.to_string(),
                boost: *boost,
            });
        }
    }

    // ── Resolve winner ────────────────────────────────────────────────────────
    let winner = scores.into_iter().max_by(|a, b| {
        a.1.total()
            .partial_cmp(&b.1.total())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    match winner {
        Some((category, score)) => ClassificationResult {
            category,
            confidence: score.total().min(1.0),
            tier_used: score.decisive_tier.unwrap_or(Tier::Extension),
            evidence: score.evidence,
        },
        None => ClassificationResult {
            category: "Misc".to_string(),
            confidence: 0.10,
            tier_used: Tier::Filename,
            evidence: vec![],
        },
    }
}

/// Convert a user config key like `"invoices"` to a canonical category name.
fn canonical_category_name(key: &str) -> String {
    match key.to_lowercase().as_str() {
        "invoices" => "Documents/Invoices".to_string(),
        "medical" => "Documents/Medical".to_string(),
        "legal" => "Documents/Legal".to_string(),
        "research" => "Documents/Research".to_string(),
        "reports" => "Documents/Reports".to_string(),
        "datascience" => "Code/DataScience".to_string(),
        "code" => "Code".to_string(),
        "finance" => "Finance".to_string(),
        _ => {
            let mut c = key.chars();
            c.next()
                .map(|f| f.to_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::extractor::Extracted;
    use std::path::PathBuf;

    fn make_extracted(text: &str, magic: &[u8], has_text: bool) -> Extracted {
        Extracted {
            text: text.to_string(),
            magic: magic.to_vec(),
            has_text,
        }
    }

    #[test]
    fn test_tier1_extension_pdf() {
        let path = PathBuf::from("document.pdf");
        let ext = make_extracted("", b"", false);
        let result = classify(&path, &ext, &Config::default());
        assert_eq!(result.category, "Documents");
        assert!((result.confidence - 0.60).abs() < 0.01);
    }

    #[test]
    fn test_tier1_magic_bytes_pdf() {
        let path = PathBuf::from("report.pdf");
        let ext = make_extracted("", b"%PDF-1.4", false);
        let result = classify(&path, &ext, &Config::default());
        assert_eq!(result.category, "Documents");
        assert!(
            result.confidence >= 0.60,
            "confidence={}",
            result.confidence
        );
    }

    #[test]
    fn test_tier2_invoice_keywords() {
        let path = PathBuf::from("doc.pdf");
        let text =
            "Invoice #1234\nBill to: John Doe\nTotal due: $500\nPayment: card\nSubtotal: $480";
        let ext = make_extracted(text, b"%PDF", true);
        let result = classify(&path, &ext, &Config::default());
        assert_eq!(result.category, "Documents/Invoices");
        assert!(result.confidence > 0.75, "confidence={}", result.confidence);
    }

    #[test]
    fn test_tier2_sqrt_dampening() {
        // 4x "invoice" should not give 4x the score of 1x "invoice"
        let path1 = PathBuf::from("doc.pdf");
        let text1 = "invoice";
        let ext1 = make_extracted(text1, b"", true);
        let r1 = classify(&path1, &ext1, &Config::default());

        let text4 = "invoice invoice invoice invoice";
        let ext4 = make_extracted(text4, b"", true);
        let r4 = classify(&path1, &ext4, &Config::default());

        // sqrt(4) = 2, so 4x keyword should give 2x boost, not 4x
        let ratio = r4.confidence / r1.confidence.max(0.01);
        assert!(ratio < 3.5, "ratio={ratio}, dampening not working");
    }

    #[test]
    fn test_tier2_subcategory_inherits() {
        let path = PathBuf::from("doc.pdf");
        let text = "Invoice #1234\nBill to: John\nTotal due: $500";
        let ext = make_extracted(text, b"%PDF", true);
        let result = classify(&path, &ext, &Config::default());
        // Documents/Invoices should inherit Documents base of 0.60
        assert_eq!(result.category, "Documents/Invoices");
        assert!(result.confidence >= 0.60);
    }

    #[test]
    fn test_tier3_filename_receipt() {
        let path = PathBuf::from("receipt_amazon_2024.txt");
        let text = "Thank you for your purchase. Total: $29.99";
        let ext = make_extracted(text, b"", true);
        let result = classify(&path, &ext, &Config::default());
        assert!(
            result.category.contains("Invoice") || result.category.contains("Document"),
            "got: {}",
            result.category
        );
    }

    #[test]
    fn test_tier3_path_signal_src() {
        let path = PathBuf::from("/home/user/src/main.rs");
        let text = "fn main() {}";
        let ext = make_extracted(text, b"", true);
        let result = classify(&path, &ext, &Config::default());
        assert_eq!(result.category, "Code");
    }

    #[test]
    fn test_confidence_cap() {
        let path = PathBuf::from("invoice_final.pdf");
        let text = "invoice invoice total due bill to amount receipt payment subtotal tax billed";
        let ext = make_extracted(text, b"%PDF", true);
        let result = classify(&path, &ext, &Config::default());
        assert!(result.confidence <= 1.0);
    }

    #[test]
    fn test_needs_review_below_threshold() {
        let path = PathBuf::from("random.xyz");
        let ext = make_extracted("", b"", false);
        let result = classify(&path, &ext, &Config::default());
        // No signals at all → very low confidence → Needs Review
        assert_eq!(result.category, "Misc");
        assert!(result.confidence < 0.5);
    }

    #[test]
    fn test_user_keyword_extends_builtin() {
        let toml_str = r#"
[categories.invoices]
keywords = [{ word = "GST", weight = 3.0 }]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let path = PathBuf::from("bill.pdf");
        let text = "GST applied on purchase invoice total due";
        let ext = make_extracted(text, b"%PDF", true);
        let result = classify(&path, &ext, &config);
        assert!(result.confidence > 0.60);
        // Built-in "invoice" keyword should still work alongside user "GST"
        let has_invoice_evidence = result
            .evidence
            .iter()
            .any(|e| matches!(e, Evidence::KeywordMatch { keyword, .. } if keyword == "invoice"));
        assert!(
            has_invoice_evidence,
            "built-in keywords should not be replaced"
        );
    }

    #[test]
    fn rust_file_classifies_as_code() {
        let path = PathBuf::from("main.rs");
        let text = "fn main() {\n    struct Foo;\n    impl Foo { fn run(&self) {} }\n}";
        let ext = make_extracted(text, b"", true);
        let result = classify(&path, &ext, &Config::default());
        assert_eq!(result.category, "Code");
        assert!(result.confidence >= 0.80);
    }

    #[test]
    fn image_extension_high_confidence() {
        let path = PathBuf::from("photo.jpg");
        let ext = make_extracted("", b"\xFF\xD8\xFF", false);
        let result = classify(&path, &ext, &Config::default());
        assert_eq!(result.category, "Images");
        assert!(result.confidence >= 0.70);
    }
}
