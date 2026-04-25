//! TOML configuration loader and rule merging.
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

/// Global runtime parameters.
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
        let inv = cfg.categories.get("invoices").expect("no invoices key");
        assert_eq!(
            inv.output_folder.as_deref(),
            Some("Finance/Invoices")
        );
        let kws = inv.keywords.as_ref().unwrap();
        assert_eq!(kws.len(), 2);
        assert_eq!(kws[0].word, "GST");
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
}
