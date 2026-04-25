//! File-system watcher — monitors a directory and triggers the organize
//! pipeline on newly created files.
//!
//! Usage: `filemind watch <dir>`

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use colored::Colorize;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use filemind::config::Config;
use filemind::engine::{PipelineOptions, run as engine_run};
use filemind::error::{FileMindError, Result};

/// Watch `dir` indefinitely, organizing new files as they appear.
///
/// Blocks the current thread until the process receives SIGINT/SIGTERM.
pub async fn watch(dir: &Path, config: Arc<Config>) -> Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(64);

    let tx_clone = tx.clone();
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Name(_))
                ) {
                    for path in event.paths {
                        if path.is_file() {
                            let _ = tx_clone.blocking_send(path);
                        }
                    }
                }
            }
        })
        .map_err(|e| FileMindError::Watcher(e.to_string()))?;

    watcher
        .watch(dir, RecursiveMode::Recursive)
        .map_err(|e| FileMindError::Watcher(e.to_string()))?;

    println!(
        "{} {} — press Ctrl-C to stop",
        "👁  Watching".bold().cyan(),
        dir.display().to_string().white()
    );

    let output_dir = config.effective_output_dir();

    while let Some(new_file) = rx.recv().await {
        // Small debounce: wait briefly in case more events arrive
        tokio::time::sleep(Duration::from_millis(200)).await;

        println!(
            "  {} new file: {}",
            "→".cyan(),
            new_file.display().to_string().white()
        );

        // Create a synthetic single-file input dir pointing at the parent
        // and let the engine skip all non-matching files via the dedup check.
        let parent = new_file
            .parent()
            .unwrap_or(dir)
            .to_path_buf();

        let opts = PipelineOptions {
            input_dir: parent,
            output_dir: output_dir.clone(),
            dry_run: false,
            explain: false,
            smart_rename: config.general.smart_rename,
            concurrency: 1,
            min_confidence: config.general.min_confidence,
            conflict: config.general.conflict.clone(),
            copy: config.general.copy,
        };

        match engine_run(opts, Arc::clone(&config)).await {
            Ok(n) => {
                if n > 0 {
                    println!(
                        "  {} organized {} file(s)",
                        "✓".green(),
                        n.to_string().yellow()
                    );
                }
            }
            Err(e) => eprintln!("  {} watch organize error: {}", "✗".red(), e),
        }
    }

    Ok(())
}
