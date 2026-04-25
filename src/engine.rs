//! Async pipeline: walk → extract → classify (rayon) → act → manifest.
//!
//! Uses Tokio for async I/O orchestration and Rayon (via `spawn_blocking`)
//! for CPU-bound parallel classification.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::sync::Semaphore;
use tokio::task;

use crate::classifier;
use crate::config::{Config, ConflictStrategy};
use crate::error::{FileMindError, Result};
use crate::extractor;
use crate::manifest::{Manifest, NewEntry};
use crate::organizer;
use crate::session;
use crate::ui;

// ─── Pipeline options ─────────────────────────────────────────────────────────

/// Options controlling a single `organize` run.
#[derive(Debug, Clone)]
pub struct PipelineOptions {
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
    pub dry_run: bool,
    pub explain: bool,
    pub smart_rename: bool,
    pub concurrency: usize,
    pub min_confidence: f32,
    pub conflict: ConflictStrategy,
    pub copy: bool,
}

impl PipelineOptions {
    /// Build from the user's [`Config`] plus any CLI overrides.
    pub fn from_config(config: &Config, input_dir: PathBuf, output_dir: PathBuf) -> Self {
        Self {
            input_dir,
            output_dir,
            dry_run: false,
            explain: false,
            smart_rename: config.general.smart_rename,
            concurrency: config.general.concurrency,
            min_confidence: config.general.min_confidence,
            conflict: config.general.conflict.clone(),
            copy: config.general.copy,
        }
    }
}

// ─── A single file's pipeline result ─────────────────────────────────────────

#[derive(Debug)]
#[allow(dead_code)]
struct FileResult {
    path: PathBuf,
    category: String,
    confidence: f32,
    tier_used: String,
    md5: String,
    sha256: String,
    dest: PathBuf,
    skipped: bool,
    skip_reason: Option<String>,
    evidence: Vec<crate::classifier::Evidence>,
}

// ─── Main pipeline entry point ────────────────────────────────────────────────

/// Run the full organize pipeline.
///
/// Returns the number of files successfully organized.
pub async fn run(opts: PipelineOptions, config: Arc<Config>) -> Result<usize> {
    std::fs::create_dir_all(&opts.output_dir).map_err(FileMindError::Io)?;

    // Open the manifest (SQLite)
    let manifest = Arc::new(Manifest::open(&opts.output_dir)?);
    let session_id = if !opts.dry_run {
        manifest.new_session(&opts.input_dir, &opts.output_dir)?
    } else {
        0
    };

    // Collect candidate files
    let files = collect_files(&opts.input_dir)?;
    if files.is_empty() {
        println!("No files found in {}", opts.input_dir.display());
        return Ok(0);
    }

    // Progress bar setup
    let mp = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "{spinner:.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos:>4}/{len:4} {msg}",
    )
    .unwrap()
    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]);
    let pb = Arc::new(mp.add(ProgressBar::new(files.len() as u64)));
    pb.set_style(style);
    pb.set_message("Starting…");

    let sem = Arc::new(Semaphore::new(opts.concurrency));
    let opts = Arc::new(opts);
    let mut handles = Vec::with_capacity(files.len());

    for file_path in files {
        let sem = Arc::clone(&sem);
        let manifest = Arc::clone(&manifest);
        let opts = Arc::clone(&opts);
        let config = Arc::clone(&config);
        let pb = Arc::clone(&pb);

        let handle: task::JoinHandle<Option<FileResult>> = task::spawn(async move {
            let _permit = sem.acquire().await.ok()?;

            // Dedup check (MD5)
            let md5 = match session::md5_of_file(&file_path) {
                Ok(h) => h,
                Err(_) => return None,
            };
            if !opts.dry_run && manifest.is_duplicate(&md5).unwrap_or(false) {
                pb.inc(1);
                pb.set_message(format!(
                    "⏭  duplicate: {}",
                    file_path.file_name().unwrap_or_default().to_string_lossy()
                ));
                return Some(FileResult {
                    path: file_path,
                    category: String::new(),
                    confidence: 0.0,
                    tier_used: String::new(),
                    md5,
                    sha256: String::new(),
                    dest: PathBuf::new(),
                    skipped: true,
                    skip_reason: Some("duplicate".into()),
                    evidence: vec![],
                });
            }

            // Extract + classify (CPU-bound → spawn_blocking / rayon)
            let path_clone = file_path.clone();
            let config_clone = Arc::clone(&config);
            let result = task::spawn_blocking(move || {
                let extracted = extractor::extract(&path_clone)?;
                let cls = classifier::classify(&path_clone, &extracted, &config_clone);
                Ok::<_, FileMindError>((extracted, cls))
            })
            .await;
            let (_extracted, cls) = match result {
                Ok(Ok(v)) => v,
                _ => return None,
            };

            // Confidence gate
            let final_category = if cls.confidence < opts.min_confidence {
                "Needs Review".to_string()
            } else {
                config.output_folder_for(&cls.category)
            };

            // Determine final filename
            let filename = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let final_name = if opts.smart_rename && final_category != "Needs Review" {
                organizer::smart_rename(filename, &cls.category)
            } else {
                filename.to_string()
            };

            // Resolve destination
            let dest_dir = opts.output_dir.join(&final_category);
            let dest = if opts.dry_run {
                dest_dir.join(&final_name)
            } else {
                match organizer::resolve_destination(&dest_dir, &final_name, &opts.conflict) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("  conflict error: {e}");
                        return None;
                    }
                }
            };

            // SHA-256 for undo integrity
            let sha256 = session::sha256_of_file(&file_path).unwrap_or_default();

            // Show --explain output
            if opts.explain {
                ui::print_explain(
                    filename,
                    &final_category,
                    cls.confidence,
                    &cls.tier_used,
                    &cls.evidence,
                );
            }

            // Perform the file operation
            if !opts.dry_run {
                // Skip if strategy says so and dest exists
                if matches!(opts.conflict, ConflictStrategy::Skip) && dest.exists() {
                    pb.inc(1);
                    return Some(FileResult {
                        path: file_path,
                        category: final_category,
                        confidence: cls.confidence,
                        tier_used: cls.tier_used.to_string(),
                        md5,
                        sha256,
                        dest,
                        skipped: true,
                        skip_reason: Some("conflict:skip".into()),
                        evidence: cls.evidence,
                    });
                }

                let op_result = if opts.copy {
                    organizer::copy_file(&file_path, &dest)
                } else {
                    organizer::move_file(&file_path, &dest)
                };
                if let Err(e) = op_result {
                    eprintln!("  file error: {e}");
                    pb.inc(1);
                    return None;
                }

                // Record in manifest
                let entry = NewEntry {
                    session_id,
                    original_path: file_path.clone(),
                    final_path: dest.clone(),
                    category: final_category.clone(),
                    confidence: cls.confidence,
                    tier_used: cls.tier_used.to_string(),
                    md5: md5.clone(),
                    sha256: sha256.clone(),
                };
                if let Err(e) = manifest.insert_file(&entry) {
                    eprintln!("  manifest error: {e}");
                }
                let _ = manifest.increment_session_count(session_id);
            }

            pb.inc(1);
            pb.set_message(format!(
                "{} → {} [{:.0}%]",
                filename,
                final_category,
                cls.confidence * 100.0
            ));

            Some(FileResult {
                path: file_path,
                category: final_category,
                confidence: cls.confidence,
                tier_used: cls.tier_used.to_string(),
                md5,
                sha256,
                dest,
                skipped: false,
                skip_reason: None,
                evidence: cls.evidence,
            })
        });

        handles.push(handle);
    }

    // Collect results
    let mut organized = 0usize;
    for h in handles {
        if let Ok(Some(r)) = h.await {
            if !r.skipped {
                organized += 1;
            }
        }
    }

    pb.finish_with_message(format!("Done — {organized} files organized"));

    if !opts.dry_run {
        manifest.close_session(session_id)?;
    }

    Ok(organized)
}

// ─── File collection ──────────────────────────────────────────────────────────

/// Walk `dir` and return all regular files, skipping hidden dirs.
fn collect_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| {
            !e.file_name()
                .to_str()
                .map(|s| s.starts_with('.'))
                .unwrap_or(false)
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        files.push(entry.path().to_path_buf());
    }
    Ok(files)
}
