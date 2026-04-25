mod classifier;
mod config;
mod extractor;
mod manifest;
mod organizer;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;

#[derive(Parser)]
#[command(
    name = "filemind",
    version = "2.0.0",
    about = "🧠 FileMind — AI-powered file organizer (Rust edition)",
    long_about = "FileMind scans files, extracts their content, classifies them with\n\
                  Google Gemini AI, and organizes them into a clean folder hierarchy.\n\
                  100% terminal-native. No web server. No Python. Just Rust."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan and organize files from an input directory
    Organize {
        /// Path to the directory containing messy files
        #[arg(short, long, default_value = ".")]
        input: String,

        /// Output directory for organized files
        #[arg(short, long, default_value = "output")]
        output: String,

        /// Enable smart semantic renaming (YYYY-MM-DD - Category - Name)
        #[arg(short, long)]
        smart_rename: bool,

        /// Dry-run: show what would happen without moving any files
        #[arg(short, long)]
        dry_run: bool,

        /// Concurrency: number of files to classify in parallel
        #[arg(short, long, default_value_t = 4)]
        concurrency: usize,
    },

    /// Show the manifest of previously organized files
    Status {
        /// Output directory to read manifest from
        #[arg(short, long, default_value = "output")]
        output: String,
    },

    /// Package the output directory into a .zip archive
    Pack {
        /// Output directory to zip
        #[arg(short, long, default_value = "output")]
        output: String,

        /// Destination zip file path
        #[arg(short, long, default_value = "filemind_organized.zip")]
        zip: String,
    },

    /// Sync the output directory to another local path
    Sync {
        /// Output directory to sync from
        #[arg(short, long, default_value = "output")]
        output: String,

        /// Target local path to copy organized files into
        #[arg(short, long)]
        target: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env if present
    let _ = dotenvy::dotenv();

    ui::print_banner();

    let cli = Cli::parse();

    match cli.command {
        Commands::Organize {
            input,
            output,
            smart_rename,
            dry_run,
            concurrency,
        } => {
            organizer::run(&input, &output, smart_rename, dry_run, concurrency).await?;
        }
        Commands::Status { output } => {
            manifest::print_status(&output)?;
        }
        Commands::Pack { output, zip } => {
            organizer::pack(&output, &zip)?;
        }
        Commands::Sync { output, target } => {
            organizer::sync(&output, &target)?;
        }
    }

    println!("\n{}", "✅ FileMind done.".green().bold());
    Ok(())
}
