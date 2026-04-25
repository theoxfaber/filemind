use crate::{classifier, config, extractor, manifest};
use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Computes the MD5 hash of a file.
pub fn file_md5(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    format!("{:x}", md5::compute(&bytes))
}

/// Smart rename: YYYY-MM-DD — Category — original_name
fn smart_rename(filename: &str, category: &str) -> String {
    let date = Utc::now().format("%Y-%m-%d");
    let clean_cat: String = category
        .chars()
        .filter(|c| c.is_alphanumeric() || " -_".contains(*c))
        .collect();
    format!("{date} — {clean_cat} — {filename}")
}

/// Main organizer: walks input_dir, classifies each file, copies to output_dir/Category/
pub async fn run(
    input_dir: &str,
    output_dir: &str,
    smart_rename_flag: bool,
    dry_run: bool,
    concurrency: usize,
) -> Result<()> {
    let api_key = config::gemini_api_key()?;

    // Collect all candidate files
    let files: Vec<PathBuf> = collect_files(input_dir);

    if files.is_empty() {
        println!("{}", "No files found to organize.".yellow());
        return Ok(());
    }

    println!(
        "\n{}  {}\n",
        "🔍 Found".bold(),
        format!("{} files", files.len()).cyan().bold()
    );

    if dry_run {
        println!("{}\n", "⚡ DRY-RUN mode — no files will be written.".yellow().bold());
    }

    std::fs::create_dir_all(output_dir).ok();

    let mp = MultiProgress::new();
    let overall_style = ProgressStyle::with_template(
        "{spinner:.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}",
    )
    .unwrap()
    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]);

    let overall = mp.add(ProgressBar::new(files.len() as u64));
    overall.set_style(overall_style);
    overall.set_message("Organizing files…");

    let client = Arc::new(
        Client::builder()
            .use_rustls_tls()
            .build()
            .context("Failed to build HTTP client")?,
    );
    let api_key = Arc::new(api_key);
    let sem = Arc::new(Semaphore::new(concurrency));
    let output_dir = Arc::new(output_dir.to_string());
    let overall = Arc::new(overall);

    let mut handles = Vec::new();

    for file_path in files {
        let client = Arc::clone(&client);
        let api_key = Arc::clone(&api_key);
        let sem = Arc::clone(&sem);
        let output_dir = Arc::clone(&output_dir);
        let overall = Arc::clone(&overall);

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            let md5 = file_md5(&file_path);
            let filename = file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // Dedup check
            if manifest::is_duplicate(&output_dir, &md5) {
                overall.inc(1);
                overall.set_message(format!("⏭  Skipped duplicate: {filename}"));
                return;
            }

            // Extract text
            let text = extractor::extract_text(&file_path).unwrap_or_default();

            // Classify
            let mut result = classifier::classify(&client, &api_key, &filename, &text).await;

            // Low-confidence override
            if result.confidence < 40 {
                result.reasoning =
                    format!("[Low Confidence {}%] {}", result.confidence, result.reasoning);
                result.category = "Needs Review".to_string();
            }

            let final_name = if smart_rename_flag && result.category != "Needs Review" {
                smart_rename(&filename, &result.category)
            } else {
                filename.clone()
            };

            if !dry_run {
                // Create category subfolder
                let dest_dir = Path::new(output_dir.as_str()).join(&result.category);
                std::fs::create_dir_all(&dest_dir).ok();
                let dest_path = dest_dir.join(&final_name);

                if let Err(e) = std::fs::copy(&file_path, &dest_path) {
                    eprintln!("  [error] Copy failed for {filename}: {e}");
                    overall.inc(1);
                    return;
                }

                // Update manifest
                let entry = manifest::ManifestEntry {
                    original_name: filename.clone(),
                    final_name: final_name.clone(),
                    category: result.category.clone(),
                    confidence: result.confidence,
                    reasoning: result.reasoning.clone(),
                    md5,
                    organized_at: Utc::now(),
                };
                let _ = manifest::append(&output_dir, entry);
            }

            overall.inc(1);
            overall.set_message(format!(
                "{} → {} [{:.0}%]",
                filename.dimmed(),
                result.category.cyan(),
                result.confidence
            ));
        });

        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }

    overall.finish_with_message("Done!");

    // Print final summary
    if !dry_run {
        manifest::print_status(&output_dir)?;
    }

    Ok(())
}

/// Creates a zip archive of output_dir.
pub fn pack(output_dir: &str, zip_path: &str) -> Result<()> {
    use colored::Colorize;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    println!("\n{} {}", "📦 Packing".bold(), zip_path.cyan());

    let zip_file =
        std::fs::File::create(zip_path).with_context(|| format!("Cannot create {zip_path}"))?;
    let mut zip = zip::ZipWriter::new(zip_file);

    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for entry in walkdir::WalkDir::new(output_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let name = path
            .strip_prefix(output_dir)
            .unwrap_or(path)
            .to_string_lossy();

        zip.start_file(name.as_ref(), options)
            .context("zip start_file failed")?;
        let data = std::fs::read(path)?;
        zip.write_all(&data).context("zip write failed")?;
    }

    zip.finish().context("zip finish failed")?;
    println!("{} {}", "✅ Archive written to".green(), zip_path.cyan().bold());
    Ok(())
}

/// Syncs output_dir to target_path by copying all files.
pub fn sync(output_dir: &str, target_path: &str) -> Result<()> {
    use colored::Colorize;

    // Resolve ~ to home directory
    let resolved = if target_path.starts_with('~') {
        let home = dirs::home_dir().context("Could not resolve home directory")?;
        home.join(&target_path[2..])
    } else {
        PathBuf::from(target_path)
    };

    println!(
        "\n{} {} → {}",
        "🔄 Syncing".bold(),
        output_dir.cyan(),
        resolved.display().to_string().cyan()
    );

    for entry in walkdir::WalkDir::new(output_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let src = entry.path();
        let rel = src.strip_prefix(output_dir).unwrap_or(src);
        let dest = resolved.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::copy(src, &dest).with_context(|| {
            format!("Failed to copy {} → {}", src.display(), dest.display())
        })?;
    }

    println!("{}", "✅ Sync complete.".green().bold());
    Ok(())
}

/// Walks input_dir and returns all regular files (non-recursive into hidden dirs).
fn collect_files(input_dir: &str) -> Vec<PathBuf> {
    walkdir::WalkDir::new(input_dir)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| {
            // Skip hidden directories like .git
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.')
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect()
}
