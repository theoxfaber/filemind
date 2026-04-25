//! Classification drift audit — the killer feature.
//!
//! Re-classifies every file in an already-organized directory and reports
//! drift: files whose current category no longer matches what the classifier
//! would assign. This catches misclassifications that were placed with low
//! confidence, or files that should be re-categorized after keyword updates.

use std::path::PathBuf;
use std::sync::Arc;

use crate::classifier;
use crate::config::Config;
use crate::error::Result;
use crate::extractor;
use crate::manifest::Manifest;
use crate::organizer;
use crate::ui;

/// Result of an audit run.
pub struct AuditReport {
    /// Total files examined.
    pub total: usize,
    /// Files with confidence ≥ 0.85.
    pub high_confidence: usize,
    /// Files with 0.50 ≤ confidence < 0.85.
    pub medium_confidence: usize,
    /// Files flagged as possible misclassifications.
    pub drifts: Vec<ui::AuditDrift>,
}

/// Run an audit on an already-organized directory.
///
/// For each file in the manifest:
/// 1. Re-classify with the current classifier (keywords may have changed)
/// 2. Compare new category to stored category
/// 3. Flag as drift if different AND new_confidence > stored_confidence + threshold
pub fn run_audit(
    manifest: &Manifest,
    config: &Arc<Config>,
    min_drift: f32,
) -> Result<AuditReport> {
    let all_files = manifest.all_files()?;
    let mut high = 0usize;
    let mut medium = 0usize;
    let mut drifts = Vec::new();

    for entry in &all_files {
        let final_path = PathBuf::from(&entry.final_path);

        // Count confidence buckets
        if entry.confidence >= 0.85 {
            high += 1;
        } else if entry.confidence >= 0.50 {
            medium += 1;
        }

        // Re-classify if file still exists
        if !final_path.exists() {
            continue;
        }

        let extracted = match extractor::extract(&final_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let new_result = classifier::classify(&final_path, &extracted, config);

        // Check for drift: different category and new confidence exceeds stored by threshold
        let stored_cat = &entry.category;
        if new_result.category != *stored_cat
            && new_result.confidence > entry.confidence + min_drift
        {
            // Build reason from evidence
            let reason = new_result
                .evidence
                .iter()
                .filter_map(|e| match e {
                    classifier::Evidence::KeywordMatch {
                        keyword, count, ..
                    } => Some(format!("\"{}\" ×{}", keyword, count)),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(", ");
            let reason = if reason.is_empty() {
                format!(
                    "re-analysis suggests {} over {}",
                    new_result.category, stored_cat
                )
            } else {
                format!("{} detected in content", reason)
            };

            drifts.push(ui::AuditDrift {
                file_path: entry.final_path.clone(),
                current_category: stored_cat.clone(),
                current_confidence: entry.confidence,
                new_category: new_result.category.clone(),
                new_confidence: new_result.confidence,
                reason,
            });
        }
    }

    Ok(AuditReport {
        total: all_files.len(),
        high_confidence: high,
        medium_confidence: medium,
        drifts,
    })
}

/// Apply audit suggestions: move flagged files to their new categories.
pub fn apply_audit(
    manifest: &Manifest,
    config: &Arc<Config>,
    output_dir: &std::path::Path,
    min_drift: f32,
) -> Result<usize> {
    let report = run_audit(manifest, config, min_drift)?;
    let mut moved = 0usize;

    for drift in &report.drifts {
        let src = PathBuf::from(&drift.file_path);
        if !src.exists() {
            continue;
        }

        let filename = src
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let dest_dir = output_dir.join(&drift.new_category);
        let dest = match organizer::resolve_destination(
            &dest_dir,
            filename,
            &config.general.conflict,
        ) {
            Ok(d) => d,
            Err(_) => continue,
        };

        if organizer::move_file(&src, &dest).is_ok() {
            moved += 1;
        }
    }

    Ok(moved)
}
