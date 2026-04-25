//! Terminal UI: ASCII banner, colored output, and `--explain` rendering.

use colored::Colorize;

use crate::classifier::{Evidence, Tier};
use crate::manifest::{Manifest, SessionRow};

// ─── Banner ───────────────────────────────────────────────────────────────────

/// Print the FileMind v3 ASCII banner to stdout.
pub fn print_banner() {
    println!(
        "{}",
        r#"
  ███████╗██╗██╗     ███████╗███╗   ███╗██╗███╗   ██╗██████╗
  ██╔════╝██║██║     ██╔════╝████╗ ████║██║████╗  ██║██╔══██╗
  █████╗  ██║██║     █████╗  ██╔████╔██║██║██╔██╗ ██║██║  ██║
  ██╔══╝  ██║██║     ██╔══╝  ██║╚██╔╝██║██║██║╚██╗██║██║  ██║
  ██║     ██║███████╗███████╗██║ ╚═╝ ██║██║██║ ╚████║██████╔╝
  ╚═╝     ╚═╝╚══════╝╚══════╝╚═╝     ╚═╝╚═╝╚═╝  ╚═══╝╚═════╝
"#
        .cyan()
        .bold()
    );
    println!(
        "  {}  {}",
        "🧠 Deterministic file organizer".bold(),
        "— v3.0 · zero AI · zero network · single binary".dimmed()
    );
    println!("  {}", "github.com/theoxfaber/filemind".dimmed());
    println!();
}

// ─── --explain rendering ──────────────────────────────────────────────────────

/// Render the `--explain` output for a single file classification.
///
/// Printed to stderr so it does not pollute stdout in pipelines.
pub fn print_explain(
    filename: &str,
    category: &str,
    confidence: f32,
    tier: &Tier,
    evidence: &[Evidence],
) {
    eprintln!(
        "  {} {} → {} {}",
        "✓".green().bold(),
        filename.white().bold(),
        category.cyan().bold(),
        format!("[confidence: {:.2}]", confidence).yellow()
    );
    for ev in evidence {
        match ev {
            Evidence::Extension {
                ext,
                base_confidence,
            } => {
                eprintln!(
                    "    {}  .{} extension  {}",
                    "tier-1".dimmed(),
                    ext,
                    format!("+{:.2}", base_confidence).green()
                );
            }
            Evidence::MagicBytes { description, boost } => {
                eprintln!(
                    "    {}  magic bytes: {}  {}",
                    "tier-1".dimmed(),
                    description,
                    format!("+{:.2}", boost).green()
                );
            }
            Evidence::KeywordMatch {
                keyword,
                weight,
                count,
                offsets,
            } => {
                let offset_str = offsets
                    .iter()
                    .map(|o| o.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                eprintln!(
                    "    {}  \"{}\" ×{}  {}  (offsets: {})",
                    "tier-2".dimmed(),
                    keyword.yellow(),
                    count,
                    format!("+{:.2}", weight).green(),
                    offset_str.dimmed()
                );
            }
            Evidence::FilenamePattern { pattern, boost } => {
                eprintln!(
                    "    {}  filename \"{}\"  {}",
                    "tier-3".dimmed(),
                    pattern,
                    format!("+{:.2}", boost).green()
                );
            }
            Evidence::PathSignal { segment, boost } => {
                eprintln!(
                    "    {}  path segment \"{}\"  {}",
                    "tier-3".dimmed(),
                    segment,
                    format!("+{:.2}", boost).green()
                );
            }
        }
    }
    eprintln!(
        "    {}  decisive tier: {}",
        "→".dimmed(),
        tier.to_string().bold()
    );
}

// ─── Session list table ───────────────────────────────────────────────────────

/// Print a formatted table of sessions.
pub fn print_sessions(sessions: &[SessionRow]) {
    if sessions.is_empty() {
        println!("{}", "No sessions found.".yellow());
        return;
    }
    println!("\n{}", " 📋 FileMind Sessions ".black().on_cyan().bold());
    println!(
        "  {:<5} {:<30} {:<10} {:<12} {}",
        "ID".bold(),
        "Timestamp".bold(),
        "Files".bold(),
        "Status".bold(),
        "Input".bold()
    );
    println!("  {}", "─".repeat(80).dimmed());
    for s in sessions {
        let status_colored = match s.status.as_str() {
            "completed" => s.status.green(),
            "undone" => s.status.red(),
            _ => s.status.yellow(),
        };
        println!(
            "  {:<5} {:<30} {:<10} {:<12} {}",
            s.id.to_string().cyan(),
            &s.timestamp[..s.timestamp.len().min(29)],
            s.file_count,
            status_colored,
            s.input_dir.dimmed()
        );
    }
    println!();
}

// ─── Status table ─────────────────────────────────────────────────────────────

/// Print a category summary from the manifest.
pub fn print_status(manifest: &Manifest) {
    match manifest.category_summary() {
        Err(e) => eprintln!("Error reading manifest: {e}"),
        Ok(rows) => {
            if rows.is_empty() {
                println!(
                    "{}",
                    "No files organized yet. Run `filemind organize` first.".yellow()
                );
                return;
            }
            let total: i64 = rows.iter().map(|(_, c, _)| c).sum();
            println!(
                "\n{}",
                format!(" 📊 FileMind Status — {total} files organized ")
                    .black()
                    .on_cyan()
                    .bold()
            );
            for (cat, count, avg_conf) in &rows {
                println!(
                    "  {:.<45} {} files  avg {:.0}%",
                    format!("{cat} ").cyan(),
                    count.to_string().yellow().bold(),
                    avg_conf * 100.0
                );
            }
            println!();
        }
    }
}

// ─── Rules listing ─────────────────────────────────────────────────────────────

/// Print the active built-in keyword rules (trimmed to first 5 per category).
pub fn print_rules() {
    use crate::classifier::BUILTIN_KEYWORDS;
    use std::collections::BTreeMap;

    let mut by_cat: BTreeMap<&str, Vec<(&str, f32)>> = BTreeMap::new();
    for kw in BUILTIN_KEYWORDS.iter() {
        by_cat
            .entry(kw.category)
            .or_default()
            .push((kw.word, kw.weight));
    }

    println!("\n{}", " 📜 Active Rules ".black().on_cyan().bold());
    for (cat, words) in &by_cat {
        println!("  {} {}", "▸".cyan(), cat.bold());
        for (word, weight) in words.iter().take(8) {
            println!("    {:.<35} weight {:.1}", format!("\"{}\" ", word), weight);
        }
    }
    println!();
}
