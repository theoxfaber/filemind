//! TOML configuration loader, rule merging, and runtime parameter resolution.
//!
//! FileMind looks for a config file at:
//!   - `$FILEMIND_CONFIG` (env override)
//!   - `~/.config/filemind/config.toml` (default)
//!
//! Missing file is not an error — built-in defaults apply.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{FileMindError, Result};

// ─── Top-level config ────────────────────────────────────────────────────────

/// Root configuration structure, mirroring `config.toml`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    /// General runtime settings.
    #[serde(default)]
    pub general: GeneralConfig,

    /// Per-category overrides and custom categories.
    #[serde(default)]
    pub categories: HashMap<String, CategoryConfig>,
}

// ─── General settings ────────────────────────────────────────────────────────

/// Global runtime parameters — all tunable, no magic numbers.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeneralConfig {
    /// Default output directory.  Supports `~` expansion.
    #[serde(default = "default_output_dir")]
    pub output_dir: String,

    /// Prefix organized files with `YYYY-MM-DD — Category — `.
    #[serde(default)]
    pub smart_rename: bool,

    /// Number of files classified in parallel.
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,

    /// Files with confidence below this threshold go to `Needs Review/`.
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f32,

    /// How to handle destination path conflicts.
    #[serde(default)]
    pub conflict: ConflictStrategy,

    /// Copy files instead of moving them (default: copy).
    #[serde(default = "default_true")]
    pub copy: bool,

    /// Watcher debounce interval in milliseconds.
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,

    /// Maximum bytes of text extracted from any single file.
    #[serde(default = "default_extract_bytes")]
    pub extract_bytes: usize,

    /// Partial hash window size in bytes (for dedup size-bucketing).
    #[serde(default = "default_max_hash_bytes")]
    pub max_hash_bytes: usize,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            output_dir: default_output_dir(),
            smart_rename: false,
            concurrency: default_concurrency(),
            min_confidence: default_min_confidence(),
            conflict: ConflictStrategy::default(),
            copy: true,
            debounce_ms: default_debounce_ms(),
            extract_bytes: default_extract_bytes(),
            max_hash_bytes: default_max_hash_bytes(),
        }
    }
}

fn default_output_dir() -> String {
    "output".to_string()
}
fn default_concurrency() -> usize {
    4
}
fn default_min_confidence() -> f32 {
    0.50
}
fn default_true() -> bool {
    true
}
fn default_debounce_ms() -> u64 {
    200
}
fn default_extract_bytes() -> usize {
    4096
}
fn default_max_hash_bytes() -> usize {
    65536
}

// ─── Output format ───────────────────────────────────────────────────────────

/// Machine-readable output format for pipeline composability.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputFormat {
    /// Default human-readable terminal output.
    #[default]
    Human,
    /// Newline-delimited JSON (one object per line).
    Json,
    /// CSV with header row.
    Csv,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "human" => Ok(OutputFormat::Human),
            "json" => Ok(OutputFormat::Json),
            "csv" => Ok(OutputFormat::Csv),
            other => Err(format!(
                "unknown output format: '{other}' (expected: human, json, csv)"
            )),
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Human => write!(f, "human"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Csv => write!(f, "csv"),
        }
    }
}

// ─── Conflict strategy ───────────────────────────────────────────────────────

/// What to do when the destination path already exists.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// Skip the file silently.
    Skip,
    /// Overwrite the existing file.
    Overwrite,
    /// Rename the incoming file (`file (1).pdf`).
    #[default]
    RenameNew,
    /// Rename the existing file before writing.
    RenameExisting,
}

// ─── Per-category config ─────────────────────────────────────────────────────

/// Per-category configuration — overrides or extends built-in rules.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CategoryConfig {
    /// Additional / replacement keyword list for tier-2 scoring.
    pub keywords: Option<Vec<KeywordEntry>>,

    /// Override the output sub-folder (default: the category name).
    pub output_folder: Option<String>,

    /// Restrict this category to specific extensions.
    pub extensions: Option<Vec<String>>,
}

/// A single weighted keyword entry in `config.toml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KeywordEntry {
    /// The keyword or phrase to match (case-insensitive).
    pub word: String,
    /// Relative weight — higher means stronger signal.
    pub weight: f32,
    /// Optional human-readable note explaining why this keyword exists.
    #[serde(default)]
    pub note: Option<String>,
}

// ─── Config loading ──────────────────────────────────────────────────────────

impl Config {
    /// Load configuration from the default path, falling back to built-in
    /// defaults if the file does not exist.
    ///
    /// # Errors
    /// Returns [`FileMindError::Io`] if the file exists but cannot be read,
    /// or [`FileMindError::TomlParse`] if it cannot be deserialized.
    pub fn load() -> Result<Self> {
        let path = Self::resolve_path();
        Self::load_from(&path)
    }

    /// Load configuration from `path`.
    ///
    /// If `path` does not exist, returns [`Config::default()`].
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path).map_err(FileMindError::Io)?;
        let cfg: Config = toml::from_str(&raw)?;
        Ok(cfg)
    }

    /// Returns the resolved config file path, respecting `$FILEMIND_CONFIG`.
    pub fn resolve_path() -> PathBuf {
        if let Ok(env) = std::env::var("FILEMIND_CONFIG") {
            return PathBuf::from(env);
        }
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("filemind")
            .join("config.toml")
    }

    /// Returns the effective output directory with `~` expanded.
    pub fn effective_output_dir(&self) -> PathBuf {
        expand_tilde(&self.general.output_dir)
    }

    /// Returns the effective output folder for `category`, respecting any
    /// user override in `[categories.<name>].output_folder`.
    pub fn output_folder_for(&self, category: &str) -> String {
        // Look up by lowercase key
        let key = category.to_lowercase().replace([' ', '/'], "_");
        if let Some(cat) = self.categories.get(&key) {
            if let Some(folder) = &cat.output_folder {
                return folder.clone();
            }
        }
        category.to_string()
    }

    /// Add a keyword to a category in the user config file.
    ///
    /// Reads the current file, parses it, appends the keyword, and writes back.
    pub fn add_keyword_to_file(
        config_path: &Path,
        category: &str,
        word: &str,
        weight: f32,
    ) -> Result<()> {
        let mut cfg = if config_path.exists() {
            let raw = std::fs::read_to_string(config_path).map_err(FileMindError::Io)?;
            toml::from_str::<Config>(&raw)?
        } else {
            Config::default()
        };

        let cat_cfg = cfg.categories.entry(category.to_string()).or_default();
        let kws = cat_cfg.keywords.get_or_insert_with(Vec::new);
        kws.push(KeywordEntry {
            word: word.to_string(),
            weight,
            note: None,
        });

        let toml_str = toml::to_string_pretty(&cfg)?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).map_err(FileMindError::Io)?;
        }
        std::fs::write(config_path, toml_str).map_err(FileMindError::Io)?;
        Ok(())
    }

    /// Remove a keyword from a category in the user config file.
    ///
    /// Returns `true` if the keyword was found and removed.
    pub fn remove_keyword_from_file(
        config_path: &Path,
        category: &str,
        word: &str,
    ) -> Result<bool> {
        if !config_path.exists() {
            return Ok(false);
        }
        let raw = std::fs::read_to_string(config_path).map_err(FileMindError::Io)?;
        let mut cfg: Config = toml::from_str(&raw)?;

        let removed = if let Some(cat_cfg) = cfg.categories.get_mut(category) {
            if let Some(kws) = cat_cfg.keywords.as_mut() {
                let before = kws.len();
                kws.retain(|k| k.word.to_lowercase() != word.to_lowercase());
                kws.len() < before
            } else {
                false
            }
        } else {
            false
        };

        if removed {
            let toml_str = toml::to_string_pretty(&cfg)?;
            std::fs::write(config_path, toml_str).map_err(FileMindError::Io)?;
        }
        Ok(removed)
    }
}

/// Expands a leading `~` to the user's home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with('~') {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(path.trim_start_matches("~/").trim_start_matches('~'))
    } else {
        PathBuf::from(path)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let cfg = Config::default();
        assert_eq!(cfg.general.concurrency, 4);
        assert!((cfg.general.min_confidence - 0.50).abs() < f32::EPSILON);
        assert_eq!(cfg.general.conflict, ConflictStrategy::RenameNew);
        assert_eq!(cfg.general.debounce_ms, 200);
        assert_eq!(cfg.general.extract_bytes, 4096);
        assert_eq!(cfg.general.max_hash_bytes, 65536);
    }

    #[test]
    fn toml_round_trip() {
        let toml_str = r#"
[general]
output_dir = "~/Organized"
smart_rename = true
concurrency = 8
min_confidence = 0.6
conflict = "skip"
debounce_ms = 300
extract_bytes = 8192
max_hash_bytes = 131072

[categories.invoices]
output_folder = "Finance/Invoices"
keywords = [
  { word = "GST", weight = 2.5 },
  { word = "invoice", weight = 3.0 },
]
"#;
        let cfg: Config = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.general.concurrency, 8);
        assert_eq!(cfg.general.conflict, ConflictStrategy::Skip);
        assert_eq!(cfg.general.debounce_ms, 300);
        assert_eq!(cfg.general.extract_bytes, 8192);
        assert_eq!(cfg.general.max_hash_bytes, 131072);
        let inv = cfg.categories.get("invoices").expect("no invoices key");
        assert_eq!(inv.output_folder.as_deref(), Some("Finance/Invoices"));
        let kws = inv.keywords.as_ref().expect("no keywords");
        assert_eq!(kws.len(), 2);
        assert_eq!(kws[0].word, "GST");

        // Roundtrip: serialize and re-parse
        let serialized = toml::to_string_pretty(&cfg).expect("serialize failed");
        let cfg2: Config = toml::from_str(&serialized).expect("re-parse failed");
        assert_eq!(cfg2.general.concurrency, cfg.general.concurrency);
        assert_eq!(cfg2.general.debounce_ms, cfg.general.debounce_ms);
    }

    #[test]
    fn output_folder_falls_back_to_category() {
        let cfg = Config::default();
        assert_eq!(cfg.output_folder_for("Code"), "Code");
    }

    #[test]
    fn expand_tilde_replaces_home() {
        let expanded = expand_tilde("~/foo/bar");
        assert!(expanded.to_string_lossy().contains("foo/bar"));
        assert!(!expanded.to_string_lossy().starts_with('~'));
    }

    #[test]
    fn conflict_strategy_deserialize() {
        let cases = [
            ("\"skip\"", ConflictStrategy::Skip),
            ("\"overwrite\"", ConflictStrategy::Overwrite),
            ("\"rename_new\"", ConflictStrategy::RenameNew),
            ("\"rename_existing\"", ConflictStrategy::RenameExisting),
        ];
        for (input, expected) in cases {
            let parsed: ConflictStrategy = toml::from_str(&format!("conflict = {input}\n"))
                .map(|c: std::collections::HashMap<String, ConflictStrategy>| {
                    c.into_values().next().unwrap()
                })
                .expect("parse failed");
            assert_eq!(parsed, expected, "failed for input: {input}");
        }
    }
}
