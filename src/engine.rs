//! Async pipeline: walk → extract → classify (rayon) → act → batch-insert.
//!
//! Uses Tokio for async I/O orchestration and Rayon (via `spawn_blocking`)
//! for CPU-bound parallel classification.
//!
//! **Key design change from v2**: Results are collected via an mpsc channel
//! and batch-inserted into SQLite in a single transaction after the pipeline
//! completes, eliminating the `Arc<Mutex<Connection>>` bottleneck.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Serialize;
use tokio::sync::Semaphore;
use tokio::task;

use crate::classifier::{self, Evidence};
use crate::config::{Config, ConflictStrategy, OutputFormat};
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
    pub output_format: OutputFormat,
    pub no_ignore: bool,
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
            output_format: OutputFormat::Human,
            no_ignore: false,
        }
    }
}

// ─── JSON/CSV output structure ───────────────────────────────────────────────

/// Machine-readable output for a single organized file.
#[derive(Debug, Serialize)]
pub struct OrganizeResult {
    pub file: String,
    pub category: String,
    pub confidence: f32,
    pub tier: String,
    pub destination: String,
    pub action: String,
    pub skipped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
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
    evidence: Vec<Evidence>,
    file_size: i64,
}

// ─── Main pipeline entry point ────────────────────────────────────────────────

/// Run the full organize pipeline.
///
/// Returns the number of files successfully organized.
pub async fn run(opts: PipelineOptions, config: Arc<Config>) -> Result<usize> {
    std::fs::create_dir_all(&opts.output_dir).map_err(FileMindError::Io)?;

    let manifest = Manifest::open(&opts.output_dir)?;
    let session_id = if !opts.dry_run {
        manifest.new_session(&opts.input_dir, &opts.output_dir)?
    } else {
        0
    };

    // Collect candidate files (respecting .filemindignore)
    let (files, ignored_count) = collect_files(&opts.input_dir, opts.no_ignore)?;
    if files.is_empty() {
        if opts.output_format == OutputFormat::Human {
            println!("No files found in {}", opts.input_dir.display());
        }
        return Ok(0);
    }

    if ignored_count > 0 && opts.output_format == OutputFormat::Human {
        eprintln!("  Ignored {ignored_count} files (.filemindignore)");
    }

    // Progress bar setup (suppressed for machine output)
    let show_progress = opts.output_format == OutputFormat::Human;
    let mp = MultiProgress::new();
    let pb = if show_progress {
        let style = ProgressStyle::with_template(
            "{spinner:.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos:>4}/{len:4} {msg}",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]);
        let pb = mp.add(ProgressBar::new(files.len() as u64));
        pb.set_style(style);
        pb.set_message("Starting…");
        Some(Arc::new(pb))
    } else {
        None
    };

    let sem = Arc::new(Semaphore::new(opts.concurrency));
    let opts = Arc::new(opts);
    let extract_bytes = config.general.extract_bytes;

    // Channel for collecting results — replaces Arc<Mutex<Connection>>
    let (tx, mut rx) = tokio::sync::mpsc::channel::<FileResult>(files.len().max(1));

    let mut handles = Vec::with_capacity(files.len());

    for file_path in files {
        let sem = Arc::clone(&sem);
        let opts = Arc::clone(&opts);
        let config = Arc::clone(&config);
        let pb = pb.clone();
        let tx = tx.clone();

        let handle: task::JoinHandle<()> = task::spawn(async move {
            let _permit = match sem.acquire().await {
                Ok(p) => p,
                Err(_) => return,
            };

            // Dedup check (size-bucketed MD5)
            let md5 = match session::md5_of_file(&file_path) {
                Ok(h) => h,
                Err(_) => return,
            };

            let fsize = session::file_size(&file_path).unwrap_or(0);

            // Extract + classify (CPU-bound → spawn_blocking)
            let path_clone = file_path.clone();
            let config_clone = Arc::clone(&config);
            let result = task::spawn_blocking(move || {
                let extracted = extractor::extract_with_limit(&path_clone, extract_bytes)?;
                let cls = classifier::classify(&path_clone, &extracted, &config_clone);
                Ok::<_, FileMindError>((extracted, cls))
            })
            .await;
            let (_extracted, cls) = match result {
                Ok(Ok(v)) => v,
                _ => return,
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
                        return;
                    }
                }
            };

            // SHA-256 for undo integrity
            let sha256 = session::sha256_of_file(&file_path).unwrap_or_default();

            // Show --explain output (to stderr so stdout stays clean)
            if opts.explain && opts.output_format == OutputFormat::Human {
                ui::print_explain(
                    filename,
                    &final_category,
                    cls.confidence,
                    &cls.tier_used,
                    &cls.evidence,
                );
            }

            // Perform the file operation
            let mut skipped = false;
            let mut skip_reason = None;

            if !opts.dry_run {
                if matches!(opts.conflict, ConflictStrategy::Skip) && dest.exists() {
                    skipped = true;
                    skip_reason = Some("conflict:skip".into());
                } else {
                    let op_result = if opts.copy {
                        organizer::copy_file(&file_path, &dest)
                    } else {
                        organizer::move_file(&file_path, &dest)
                    };
                    if let Err(e) = op_result {
                        eprintln!("  file error: {e}");
                        return;
                    }
                }
            }

            if let Some(pb) = &pb {
                pb.inc(1);
                pb.set_message(format!(
                    "{} → {} [{:.0}%]",
                    filename,
                    final_category,
                    cls.confidence * 100.0
                ));
            }

            let _ = tx
                .send(FileResult {
                    path: file_path,
                    category: final_category,
                    confidence: cls.confidence,
                    tier_used: cls.tier_used.to_string(),
                    md5,
                    sha256,
                    dest,
                    skipped,
                    skip_reason,
                    evidence: cls.evidence,
                    file_size: fsize,
                })
                .await;
        });

        handles.push(handle);
    }

    // Drop the sender so rx.recv() completes when all tasks finish
    drop(tx);

    // Wait for all tasks
    for h in handles {
        let _ = h.await;
    }

    // Collect results
    let mut results: Vec<FileResult> = Vec::new();
    while let Some(r) = rx.recv().await {
        results.push(r);
    }

    if let Some(pb) = &pb {
        let organized = results.iter().filter(|r| !r.skipped).count();
        pb.finish_with_message(format!("Done — {organized} files organized"));
    }

    // Batch insert into manifest (single transaction — the key performance win)
    let organized = results.iter().filter(|r| !r.skipped).count();
    if !opts.dry_run {
        let entries: Vec<NewEntry> = results
            .iter()
            .filter(|r| !r.skipped)
            .map(|r| NewEntry {
                session_id,
                original_path: r.path.clone(),
                final_path: r.dest.clone(),
                category: r.category.clone(),
                confidence: r.confidence,
                tier_used: r.tier_used.clone(),
                md5: r.md5.clone(),
                sha256: r.sha256.clone(),
                file_size: r.file_size,
            })
            .collect();

        manifest.insert_batch(&entries)?;
        manifest.set_session_count(session_id, entries.len() as i64)?;
        manifest.close_session(session_id)?;
    }

    // Machine-readable output
    match opts.output_format {
        OutputFormat::Json => {
            for r in &results {
                let out = OrganizeResult {
                    file: r.path.to_string_lossy().into_owned(),
                    category: r.category.clone(),
                    confidence: r.confidence,
                    tier: r.tier_used.clone(),
                    destination: r.dest.to_string_lossy().into_owned(),
                    action: if opts.copy { "copy" } else { "move" }.to_string(),
                    skipped: r.skipped,
                    skip_reason: r.skip_reason.clone(),
                };
                if let Ok(json) = serde_json::to_string(&out) {
                    println!("{json}");
                }
            }
        }
        OutputFormat::Csv => {
            println!("file,category,confidence,tier,destination,action,skipped");
            for r in &results {
                println!(
                    "\"{}\",\"{}\",{:.2},\"{}\",\"{}\",\"{}\",{}",
                    r.path.display(),
                    r.category,
                    r.confidence,
                    r.tier_used,
                    r.dest.display(),
                    if opts.copy { "copy" } else { "move" },
                    r.skipped,
                );
            }
        }
        OutputFormat::Human => {} // already printed via progress bar
    }

    Ok(organized)
}

// ─── File collection with .filemindignore ─────────────────────────────────────

/// Walk `dir` and return all regular files, respecting `.filemindignore`.
///
/// Returns `(files, ignored_count)`.
fn collect_files(dir: &Path, no_ignore: bool) -> Result<(Vec<PathBuf>, usize)> {
    if no_ignore {
        return collect_files_simple(dir);
    }

    // Use the `ignore` crate (same as ripgrep) for .filemindignore support
    let mut builder = ignore::WalkBuilder::new(dir);
    builder
        .hidden(true) // skip hidden files
        .git_ignore(false) // don't use .gitignore
        .git_global(false)
        .git_exclude(false);

    // Add .filemindignore files
    let local_ignore = dir.join(".filemindignore");
    if local_ignore.exists() {
        builder.add_ignore(&local_ignore);
    }
    if let Some(home) = dirs::home_dir() {
        let global_ignore = home.join(".filemindignore");
        if global_ignore.exists() {
            builder.add_ignore(&global_ignore);
        }
    }

    let mut files = Vec::new();
    let mut total_entries = 0usize;

    for entry in builder.build().filter_map(|e| e.ok()) {
        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            files.push(entry.path().to_path_buf());
        }
        total_entries += 1;
    }

    // Count of files that would have been found without ignore
    let all_count = collect_files_simple(dir).map(|(f, _)| f.len()).unwrap_or(0);
    let ignored = all_count.saturating_sub(files.len());

    // Subtract the directory entries from the difference
    Ok((files, ignored.min(total_entries)))
}

/// Simple file walk without ignore support (used as fallback and for counting).
fn collect_files_simple(dir: &Path) -> Result<(Vec<PathBuf>, usize)> {
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
    Ok((files, 0))
}
