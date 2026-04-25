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

use crate::config::Config;
use crate::extractor::Extracted;

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
    // Documents — office formats get higher base (format is conclusive)
    for ext in &["pdf", "doc", "docx", "odt", "rtf", "pages", "wpd", "tex"] {
        m.insert(*ext, ("Documents", 0.60_f32));
    }
    // Plain text — lower base because content matters more than extension
    for ext in &["txt", "md", "markdown", "rst"] {
        m.insert(*ext, ("Documents", 0.40_f32));
    }
    // Spreadsheets
    for ext in &["xlsx", "xls", "ods", "numbers", "xlsm", "xlsb"] {
        m.insert(*ext, ("Spreadsheets", 0.65_f32));
    }
    // Presentations
    for ext in &["pptx", "ppt", "odp", "key"] {
        m.insert(*ext, ("Presentations", 0.65_f32));
    }
    // Images
    for ext in &[
        "jpg", "jpeg", "png", "gif", "bmp", "tiff", "tif", "webp", "svg",
        "heic", "heif", "raw", "cr2", "nef", "arw", "dng", "ico",
    ] {
        m.insert(*ext, ("Images", 0.70_f32));
    }
    // Videos
    for ext in &["mp4", "avi", "mov", "mkv", "wmv", "flv", "webm", "m4v", "3gp", "ts", "mts"] {
        m.insert(*ext, ("Videos", 0.75_f32));
    }
    // Audio
    for ext in &["mp3", "wav", "flac", "aac", "ogg", "m4a", "wma", "opus", "aiff", "mid"] {
        m.insert(*ext, ("Audio", 0.75_f32));
    }
    // Source code
    for ext in &[
        "rs", "py", "js", "ts", "jsx", "tsx", "go", "java", "c", "cpp", "h",
        "hpp", "cs", "rb", "swift", "kt", "scala", "r", "lua", "php", "sh",
        "bash", "zsh", "fish", "ps1", "bat", "gradle", "cmake",
    ] {
        m.insert(*ext, ("Code", 0.80_f32));
    }
    // Data / config
    for ext in &["json", "yaml", "yml", "toml", "xml", "csv", "tsv", "parquet", "avro"] {
        m.insert(*ext, ("Data", 0.65_f32));
    }
    // Archives
    for ext in &["zip", "tar", "gz", "bz2", "xz", "7z", "rar", "tgz", "zst"] {
        m.insert(*ext, ("Archives", 0.75_f32));
    }
    // Database
    for ext in &["sqlite", "sqlite3", "db", "duckdb"] {
        m.insert(*ext, ("Data/Database", 0.80_f32));
    }
    // Ebooks
    for ext in &["epub", "mobi", "azw", "azw3"] {
        m.insert(*ext, ("Books", 0.75_f32));
    }
    m
});

// ─── Built-in keyword lists ───────────────────────────────────────────────────

/// A built-in keyword entry with category, word, and weight.
pub struct BuiltinKw {
    pub category: &'static str,
    pub word: &'static str,
    pub weight: f32,
}

pub static BUILTIN_KEYWORDS: Lazy<Vec<BuiltinKw>> = Lazy::new(|| {
    vec![
        // Invoices / Finance
        BuiltinKw { category: "Documents/Invoices", word: "invoice",      weight: 3.0 },
        BuiltinKw { category: "Documents/Invoices", word: "total due",    weight: 2.5 },
        BuiltinKw { category: "Documents/Invoices", word: "bill to",      weight: 2.5 },
        BuiltinKw { category: "Documents/Invoices", word: "amount",       weight: 1.5 },
        BuiltinKw { category: "Documents/Invoices", word: "receipt",      weight: 2.0 },
        BuiltinKw { category: "Documents/Invoices", word: "payment",      weight: 1.5 },
        BuiltinKw { category: "Documents/Invoices", word: "subtotal",     weight: 2.0 },
        BuiltinKw { category: "Documents/Invoices", word: "tax",          weight: 1.0 },
        BuiltinKw { category: "Documents/Invoices", word: "billed",       weight: 1.5 },
        // Medical
        BuiltinKw { category: "Documents/Medical", word: "diagnosis",     weight: 3.0 },
        BuiltinKw { category: "Documents/Medical", word: "prescription",  weight: 3.0 },
        BuiltinKw { category: "Documents/Medical", word: "patient",       weight: 2.5 },
        BuiltinKw { category: "Documents/Medical", word: "dosage",        weight: 2.0 },
        BuiltinKw { category: "Documents/Medical", word: "mg",            weight: 1.0 },
        BuiltinKw { category: "Documents/Medical", word: "clinic",        weight: 2.0 },
        BuiltinKw { category: "Documents/Medical", word: "physician",     weight: 2.5 },
        BuiltinKw { category: "Documents/Medical", word: "symptom",       weight: 2.0 },
        // Legal
        BuiltinKw { category: "Documents/Legal", word: "agreement",       weight: 3.0 },
        BuiltinKw { category: "Documents/Legal", word: "whereas",         weight: 3.0 },
        BuiltinKw { category: "Documents/Legal", word: "party",           weight: 1.5 },
        BuiltinKw { category: "Documents/Legal", word: "liability",       weight: 2.5 },
        BuiltinKw { category: "Documents/Legal", word: "jurisdiction",    weight: 3.0 },
        BuiltinKw { category: "Documents/Legal", word: "contract",        weight: 2.5 },
        BuiltinKw { category: "Documents/Legal", word: "shall",           weight: 1.0 },
        // Finance
        BuiltinKw { category: "Finance", word: "portfolio",               weight: 2.5 },
        BuiltinKw { category: "Finance", word: "dividend",                weight: 3.0 },
        BuiltinKw { category: "Finance", word: "equity",                  weight: 2.0 },
        BuiltinKw { category: "Finance", word: "balance sheet",           weight: 3.0 },
        BuiltinKw { category: "Finance", word: "quarterly",               weight: 1.5 },
        BuiltinKw { category: "Finance", word: "revenue",                 weight: 2.0 },
        // Code (text-based signal)
        BuiltinKw { category: "Code", word: "fn ",                        weight: 2.0 },
        BuiltinKw { category: "Code", word: "struct ",                    weight: 2.0 },
        BuiltinKw { category: "Code", word: "impl ",                      weight: 2.0 },
        BuiltinKw { category: "Code", word: "import ",                    weight: 1.5 },
        BuiltinKw { category: "Code", word: "def ",                       weight: 2.0 },
        BuiltinKw { category: "Code", word: "#include",                   weight: 3.0 },
        BuiltinKw { category: "Code", word: "function",                   weight: 1.5 },
        BuiltinKw { category: "Code", word: "class ",                     weight: 1.5 },
        BuiltinKw { category: "Code", word: "return",                     weight: 1.0 },
        // Research / Academic
        BuiltinKw { category: "Documents/Research", word: "abstract",     weight: 2.5 },
        BuiltinKw { category: "Documents/Research", word: "bibliography", weight: 3.0 },
        BuiltinKw { category: "Documents/Research", word: "hypothesis",   weight: 3.0 },
        BuiltinKw { category: "Documents/Research", word: "methodology",  weight: 2.5 },
        BuiltinKw { category: "Documents/Research", word: "references",   weight: 2.0 },
    ]
});

// ─── Magic byte patterns ──────────────────────────────────────────────────────

/// Returns `Some((description, boost))` if the magic bytes match a known format.
fn detect_magic(bytes: &[u8]) -> Option<(&'static str, f32)> {
    if bytes.starts_with(b"%PDF") {
        return Some(("PDF document header (%PDF)", 0.10));
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Some(("ZIP/Office archive (PK\\x03\\x04)", 0.10));
    }
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        return Some(("JPEG image (FFD8FF)", 0.12));
    }
    if bytes.starts_with(b"\x89PNG") {
        return Some(("PNG image (\\x89PNG)", 0.12));
    }
    if bytes.starts_with(b"GIF8") {
        return Some(("GIF image (GIF8)", 0.12));
    }
    if bytes.starts_with(b"ID3") || (bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0) {
        return Some(("MP3 audio (ID3 / sync bits)", 0.12));
    }
    if bytes.starts_with(b"fLaC") {
        return Some(("FLAC audio (fLaC)", 0.12));
    }
    if bytes.starts_with(b"RIFF") {
        return Some(("RIFF container (WAV/AVI)", 0.10));
    }
    if bytes.starts_with(b"\x1f\x8b") {
        return Some(("GZIP archive (\\x1f\\x8b)", 0.12));
    }
    if bytes.starts_with(b"BZh") {
        return Some(("BZIP2 archive (BZh)", 0.12));
    }
    if bytes.starts_with(b"\xfd7zXZ") {
        return Some(("XZ archive", 0.12));
    }
    if bytes.starts_with(b"SQLite format") {
        return Some(("SQLite database", 0.15));
    }
    None
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

    // Magic bytes can override or boost the category
    if let Some((desc, boost)) = detect_magic(&extracted.magic) {
        // Determine category from magic (may differ from extension)
        let magic_cat = magic_to_category(desc);
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

        // Built-ins
        for kw in BUILTIN_KEYWORDS.iter() {
            kw_map
                .entry(kw.category.to_string())
                .or_default()
                .push((kw.word.to_string(), kw.weight));
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

                // Inherit parent base confidence for subcategories (e.g.
                // "Documents/Invoices" inherits "Documents" base = 0.60).
                let inherited_base: f32 = if !scores.contains_key(category) {
                    category
                        .find('/')
                        .and_then(|slash| scores.get(&category[..slash]))
                        .map(|p| p.base)
                        .unwrap_or(0.0)
                } else {
                    0.0 // will not overwrite existing base
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
        // (pattern_substr, category, boost)
        ("invoice",    "Documents/Invoices", 0.15),
        ("receipt",    "Documents/Invoices", 0.12),
        ("bill",       "Documents/Invoices", 0.10),
        ("img_",       "Images",             0.08),
        ("dsc_",       "Images",             0.08),
        ("screenshot", "Images",             0.10),
        ("photo",      "Images",             0.08),
        ("readme",     "Code",               0.10),
        ("makefile",   "Code",               0.15),
        ("dockerfile", "Code",               0.15),
        ("contract",   "Documents/Legal",    0.15),
        ("agreement",  "Documents/Legal",    0.12),
        ("report",     "Documents",          0.08),
        ("resume",     "Documents",          0.10),
        ("cv",         "Documents",          0.08),
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
        ("invoice",  "Documents/Invoices", 0.10),
        ("invoices", "Documents/Invoices", 0.12),
        ("medical",  "Documents/Medical",  0.12),
        ("health",   "Documents/Medical",  0.10),
        ("legal",    "Documents/Legal",    0.12),
        ("photos",   "Images",             0.10),
        ("pictures", "Images",             0.10),
        ("videos",   "Videos",             0.10),
        ("music",    "Audio",              0.10),
        ("code",     "Code",               0.08),
        ("src",      "Code",               0.06),
    ];

    for (segment, category, boost) in path_signals {
        // Check if any path component (not the filename itself) matches
        if path_str
            .split('/')
            .rev()
            .skip(1) // skip filename
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
    let winner = scores
        .into_iter()
        .max_by(|a, b| a.1.total().partial_cmp(&b.1.total()).unwrap_or(std::cmp::Ordering::Equal));

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

/// Maps a magic-byte description to a coarse category.
fn magic_to_category(desc: &str) -> &'static str {
    if desc.contains("PDF") {
        "Documents"
    } else if desc.contains("ZIP") || desc.contains("GZIP") || desc.contains("BZIP2") || desc.contains("XZ") {
        "Archives"
    } else if desc.contains("JPEG") || desc.contains("PNG") || desc.contains("GIF") {
        "Images"
    } else if desc.contains("MP3") || desc.contains("FLAC") || desc.contains("RIFF") {
        "Audio"
    } else if desc.contains("SQLite") {
        "Data/Database"
    } else {
        "Misc"
    }
}

/// Convert a user config key like `"invoices"` to a canonical category name.
fn canonical_category_name(key: &str) -> String {
    match key.to_lowercase().as_str() {
        "invoices"  => "Documents/Invoices".to_string(),
        "medical"   => "Documents/Medical".to_string(),
        "legal"     => "Documents/Legal".to_string(),
        "research"  => "Documents/Research".to_string(),
        "code"      => "Code".to_string(),
        "finance"   => "Finance".to_string(),
        _ => {
            // Title-case the key as the category name
            let mut c = key.chars();
            c.next().map(|f| f.to_uppercase().to_string() + c.as_str()).unwrap_or_default()
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
    fn pdf_extension_gives_documents() {
        let path = PathBuf::from("report.pdf");
        let ext = make_extracted("", b"%PDF-1.4", false);
        let result = classify(&path, &ext, &Config::default());
        assert_eq!(result.category, "Documents");
        assert!(result.confidence >= 0.60);
    }

    #[test]
    fn invoice_keywords_boost_to_invoices() {
        let path = PathBuf::from("doc.pdf");
        let text = "Invoice #1234\nBill to: John Doe\nTotal due: $500\nPayment: card\nSubtotal: $480";
        let ext = make_extracted(text, b"%PDF", true);
        let result = classify(&path, &ext, &Config::default());
        assert_eq!(result.category, "Documents/Invoices");
        assert!(result.confidence > 0.75, "confidence={}", result.confidence);
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
    fn medical_keywords_detected() {
        let path = PathBuf::from("note.txt");
        let text = "Patient: Alice Smith\nDiagnosis: Flu\nPrescription: 500mg\nClinic: City Hospital";
        let ext = make_extracted(text, b"", true);
        let result = classify(&path, &ext, &Config::default());
        assert_eq!(result.category, "Documents/Medical");
        assert!(result.confidence > 0.35, "confidence={}", result.confidence);
    }

    #[test]
    fn filename_receipt_classifies_as_document_or_invoice() {
        let path = PathBuf::from("receipt_amazon_2024.txt");
        let text = "Thank you for your purchase. Total: $29.99";
        let ext = make_extracted(text, b"", true);
        let result = classify(&path, &ext, &Config::default());
        // .txt base puts it in Documents; with strong invoice content it
        // would promote to Documents/Invoices.  Either is acceptable.
        assert!(
            result.category.contains("Invoice") || result.category.contains("Document"),
            "expected Invoice or Document category, got: {}",
            result.category
        );
        assert!(result.confidence > 0.30, "confidence={}", result.confidence);
    }

    #[test]
    fn filename_receipt_with_invoice_content_classifies_as_invoice() {
        let path = PathBuf::from("receipt_amazon_2024.txt");
        let text = "Invoice #1234 receipt. Total due: $29.99. Payment received.";
        let ext = make_extracted(text, b"", true);
        let result = classify(&path, &ext, &Config::default());
        assert_eq!(result.category, "Documents/Invoices");
        assert!(result.confidence > 0.50, "confidence={}", result.confidence);
    }

    #[test]
    fn image_extension_high_confidence() {
        let path = PathBuf::from("photo.jpg");
        let ext = make_extracted("", b"\xFF\xD8\xFF", false);
        let result = classify(&path, &ext, &Config::default());
        assert_eq!(result.category, "Images");
        assert!(result.confidence >= 0.70);
    }

    #[test]
    fn confidence_capped_at_one() {
        let path = PathBuf::from("invoice_final.pdf");
        let text = "invoice invoice total due bill to amount receipt payment subtotal tax billed";
        let ext = make_extracted(text, b"%PDF", true);
        let result = classify(&path, &ext, &Config::default());
        assert!(result.confidence <= 1.0);
    }

    #[test]
    fn user_keyword_merges_with_builtin() {
        let toml_str = r#"
[categories.invoices]
keywords = [{ word = "GST", weight = 3.0 }]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let path = PathBuf::from("bill.pdf");
        let text = "GST applied on purchase";
        let ext = make_extracted(text, b"%PDF", true);
        let result = classify(&path, &ext, &config);
        // Should have some boosted invoices score
        assert!(result.confidence > 0.60);
    }
}
