use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

const MANIFEST_FILE: &str = "manifest.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ManifestEntry {
    pub original_name: String,
    pub final_name: String,
    pub category: String,
    pub confidence: u8,
    pub reasoning: String,
    pub md5: String,
    pub organized_at: DateTime<Utc>,
}

/// Loads the manifest from output_dir/manifest.json.
pub fn load(output_dir: &str) -> Result<Vec<ManifestEntry>> {
    let path = Path::new(output_dir).join(MANIFEST_FILE);
    if !path.exists() {
        return Ok(vec![]);
    }
    let data = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read manifest at {}", path.display()))?;
    let entries: Vec<ManifestEntry> =
        serde_json::from_str(&data).unwrap_or_default();
    Ok(entries)
}

/// Appends an entry to the manifest, deduplicating by MD5.
pub fn append(output_dir: &str, entry: ManifestEntry) -> Result<()> {
    let mut entries = load(output_dir)?;
    // Remove any existing entry with the same md5 before appending
    entries.retain(|e| e.md5 != entry.md5);
    entries.push(entry);
    save(output_dir, &entries)
}

/// Returns true if the hash already exists in the manifest.
pub fn is_duplicate(output_dir: &str, md5: &str) -> bool {
    load(output_dir)
        .unwrap_or_default()
        .iter()
        .any(|e| e.md5 == md5)
}

fn save(output_dir: &str, entries: &[ManifestEntry]) -> Result<()> {
    std::fs::create_dir_all(output_dir).ok();
    let path = Path::new(output_dir).join(MANIFEST_FILE);
    let json = serde_json::to_string_pretty(entries)?;
    std::fs::write(&path, json)
        .with_context(|| format!("Failed to write manifest to {}", path.display()))
}

/// Prints a pretty summary of the manifest to stdout.
pub fn print_status(output_dir: &str) -> Result<()> {
    use colored::Colorize;

    let entries = load(output_dir)?;
    if entries.is_empty() {
        println!("{}", "No organized files yet. Run `filemind organize` first.".yellow());
        return Ok(());
    }

    println!(
        "\n{}\n",
        format!(" 📋 FileMind Manifest — {} files ", entries.len())
            .black()
            .on_cyan()
            .bold()
    );

    let mut by_cat: std::collections::BTreeMap<&str, Vec<&ManifestEntry>> =
        std::collections::BTreeMap::new();
    for e in &entries {
        by_cat.entry(&e.category).or_default().push(e);
    }

    for (cat, files) in &by_cat {
        println!("  {} ({})", cat.bold().cyan(), files.len().to_string().yellow());
        for f in files.iter().take(10) {
            println!(
                "    {} {}  [{:.0}%]",
                "→".dimmed(),
                f.final_name.white(),
                f.confidence
            );
        }
        if files.len() > 10 {
            println!("    {} … and {} more", "·".dimmed(), files.len() - 10);
        }
    }
    Ok(())
}
