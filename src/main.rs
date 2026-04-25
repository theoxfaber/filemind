//! FileMind v3 — binary entry point and CLI dispatcher.

mod completions;
mod watcher;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};
use clap_complete::Shell;
use colored::Colorize;

use filemind::{
    config::Config,
    engine::{run as engine_run, PipelineOptions},
    manifest::Manifest,
    organizer, session, ui,
};

// ─── CLI definition ───────────────────────────────────────────────────────────

/// FileMind — deterministic, content-aware file organizer.
///
/// Zero AI. Zero network. Single binary.
#[derive(Parser)]
#[command(
    name = "filemind",
    version = "3.0.0",
    about = "🧠 FileMind v3 — deterministic, content-aware file organizer",
    long_about = "FileMind classifies files by reading their content and metadata.\n\
                  No AI, no network, no Python — just a single Rust binary.\n\
                  Full undo, explainable confidence scores, TOML config."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan a directory and organize files by detected category.
    Organize {
        /// Input directory containing files to organize.
        #[arg(short, long, default_value = ".")]
        input: String,

        /// Output directory for organized files (overrides config).
        #[arg(short, long)]
        output: Option<String>,

        /// Preview what would happen without writing any files.
        #[arg(long)]
        dry_run: bool,

        /// Show per-file evidence breakdown (why each file was classified this way).
        #[arg(long)]
        explain: bool,

        /// Prefix filenames with `YYYY-MM-DD — Category — ` (overrides config).
        #[arg(long)]
        smart_rename: bool,

        /// Copy files instead of moving them.
        #[arg(long)]
        copy: bool,

        /// Number of files classified in parallel (overrides config).
        #[arg(short, long)]
        concurrency: Option<usize>,
    },

    /// Watch a directory and organize new files as they appear.
    Watch {
        /// Directory to watch.
        dir: String,
    },

    /// Undo a previous organize session.
    Undo {
        /// Session ID to undo (defaults to the most recent session).
        #[arg(long)]
        session: Option<i64>,

        /// Output directory where the manifest lives.
        #[arg(short, long)]
        output: Option<String>,
    },

    /// List or inspect organize sessions.
    Sessions {
        /// Output directory where the manifest lives.
        #[arg(short, long)]
        output: Option<String>,

        /// Show all file operations for a specific session.
        #[arg(long)]
        show: Option<i64>,
    },

    /// Show a summary of organized files by category.
    Status {
        /// Output directory where the manifest lives.
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Inspect the active classification rules.
    Rules {
        #[command(subcommand)]
        cmd: RulesCmd,
    },

    /// Pack the output directory into a zip archive.
    Pack {
        /// Output directory to zip.
        #[arg(short, long)]
        output: Option<String>,

        /// Destination zip file path.
        #[arg(long, default_value = "filemind_organized.zip")]
        zip: String,
    },

    /// Mirror the output directory to another local path.
    Sync {
        /// Output directory to sync from.
        #[arg(short, long)]
        output: Option<String>,

        /// Target directory to copy organized files into.
        #[arg(short, long)]
        target: String,
    },

    /// Generate shell completions.
    Completions {
        /// Shell to generate completions for.
        shell: Shell,
    },
}

#[derive(Subcommand)]
enum RulesCmd {
    /// List all active built-in and user-defined rules.
    List,

    /// Classify a single file and show the full evidence breakdown.
    Check {
        /// Path to the file to classify.
        file: String,
    },
}

// ─── Entry point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load config (always, even if some commands don't need it)
    let config = Arc::new(Config::load().context("Failed to load config")?);

    // Print banner for interactive commands (not completions)
    if !matches!(cli.command, Commands::Completions { .. }) {
        ui::print_banner();
    }

    match cli.command {
        Commands::Organize {
            input,
            output,
            dry_run,
            explain,
            smart_rename,
            copy,
            concurrency,
        } => {
            let input_path = PathBuf::from(&input);
            let output_path = output
                .map(PathBuf::from)
                .unwrap_or_else(|| config.effective_output_dir());

            let mut opts = PipelineOptions::from_config(&config, input_path, output_path);
            opts.dry_run = dry_run;
            opts.explain = explain;
            if smart_rename {
                opts.smart_rename = true;
            }
            if copy {
                opts.copy = true;
            }
            if let Some(c) = concurrency {
                opts.concurrency = c;
            }

            if dry_run {
                println!(
                    "{}",
                    "⚡ DRY-RUN mode — no files will be written.\n"
                        .yellow()
                        .bold()
                );
            }

            let n = engine_run(opts, Arc::clone(&config)).await?;
            println!(
                "\n{} {} files organized.",
                "✅".green(),
                n.to_string().yellow().bold()
            );
        }

        Commands::Watch { dir } => {
            watcher::watch(&PathBuf::from(dir), config).await?;
        }

        Commands::Undo { session, output } => {
            let output_path = output
                .map(PathBuf::from)
                .unwrap_or_else(|| config.effective_output_dir());
            let manifest = Manifest::open(&output_path)?;

            let sid = if let Some(id) = session {
                id
            } else {
                manifest
                    .last_active_session()?
                    .context("No completed sessions found to undo.")?
            };

            let report = session::undo_session(&manifest, sid)?;
            println!("\n{}", report.to_string().green().bold());
            for w in &report.warnings {
                eprintln!("{w}");
            }
        }

        Commands::Sessions { output, show } => {
            let output_path = output
                .map(PathBuf::from)
                .unwrap_or_else(|| config.effective_output_dir());
            let manifest = Manifest::open(&output_path)?;

            if let Some(id) = show {
                let entries = manifest.files_for_session(id)?;
                if entries.is_empty() {
                    println!("{}", "No files found for that session.".yellow());
                } else {
                    println!(
                        "\n{}",
                        format!(" Session {id} — {} files ", entries.len())
                            .black()
                            .on_cyan()
                            .bold()
                    );
                    for e in &entries {
                        println!(
                            "  {} {} → {} [{:.0}%]",
                            "→".dimmed(),
                            e.original_path.white(),
                            e.final_path.cyan(),
                            e.confidence * 100.0
                        );
                    }
                }
            } else {
                let sessions = manifest.list_sessions()?;
                ui::print_sessions(&sessions);
            }
        }

        Commands::Status { output } => {
            let output_path = output
                .map(PathBuf::from)
                .unwrap_or_else(|| config.effective_output_dir());
            let manifest = Manifest::open(&output_path)?;
            ui::print_status(&manifest);
        }

        Commands::Rules { cmd } => match cmd {
            RulesCmd::List => {
                ui::print_rules();
            }
            RulesCmd::Check { file } => {
                let path = PathBuf::from(&file);
                if !path.exists() {
                    anyhow::bail!("File not found: {file}");
                }
                let extracted = filemind::extractor::extract(&path)?;
                let result = filemind::classifier::classify(&path, &extracted, &config);
                ui::print_explain(
                    path.file_name().and_then(|n| n.to_str()).unwrap_or(&file),
                    &result.category,
                    result.confidence,
                    &result.tier_used,
                    &result.evidence,
                );
                println!(
                    "\n  Final: {} → {} [confidence: {:.2}]",
                    file.white(),
                    result.category.cyan().bold(),
                    result.confidence
                );
            }
        },

        Commands::Pack { output, zip } => {
            let output_path = output
                .map(PathBuf::from)
                .unwrap_or_else(|| config.effective_output_dir());
            let zip_path = PathBuf::from(&zip);
            println!("📦 Packing {} → {}", output_path.display(), zip.cyan());
            organizer::pack_to_zip(&output_path, &zip_path)?;
            println!("{} Archive written to {}", "✅".green(), zip.cyan().bold());
        }

        Commands::Sync { output, target } => {
            let output_path = output
                .map(PathBuf::from)
                .unwrap_or_else(|| config.effective_output_dir());
            let target_path = filemind::config::expand_tilde(&target);
            println!(
                "🔄 Syncing {} → {}",
                output_path.display(),
                target_path.display().to_string().cyan()
            );
            organizer::sync_to_dir(&output_path, &target_path)?;
            println!("{}", "✅ Sync complete.".green().bold());
        }

        Commands::Completions { shell } => {
            completions::generate_completions(shell);
        }
    }

    Ok(())
}
