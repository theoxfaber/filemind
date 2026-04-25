//! Terminal UI: ASCII banner, colored output, `--explain` rendering, and stats.

use std::collections::BTreeMap;

use colored::Colorize;

use crate::classifier::{Evidence, Tier, BUILTIN_KEYWORDS};
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
            let total: i64 = rows.iter().map(|(_, c, _, _)| c).sum();
            println!(
                "\n{}",
                format!(" 📊 FileMind Status — {total} files organized ")
                    .black()
                    .on_cyan()
                    .bold()
            );
            for (cat, count, avg_conf, _size) in &rows {
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

// ─── Stats command ────────────────────────────────────────────────────────────

/// Format a byte count as a human-readable size string.
fn format_size(bytes: i64) -> String {
    let b = bytes as f64;
    if b >= 1_073_741_824.0 {
        format!("{:.1}GB", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:.0}MB", b / 1_048_576.0)
    } else if b >= 1024.0 {
        format!("{:.0}KB", b / 1024.0)
    } else {
        format!("{bytes}B")
    }
}

/// Render a simple bar using █ blocks.
fn render_bar(fraction: f64, max_width: usize) -> String {
    let blocks = (fraction * max_width as f64).round() as usize;
    "█".repeat(blocks.min(max_width))
}

/// Print full stats from the manifest.
pub fn print_stats(manifest: &Manifest, days: i64) {
    let stats = match manifest.aggregate_stats() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {e}");
            return;
        }
    };

    if stats.total_files == 0 {
        println!(
            "{}",
            "No files organized yet. Run `filemind organize` first.".yellow()
        );
        return;
    }

    println!("\n{}", " 📊 FileMind Stats ".black().on_cyan().bold());
    println!(
        "  {}",
        "══════════════════════════════════════════════════════".dimmed()
    );

    // Overview box
    println!("\n  {}", "Overview".bold());
    println!("  ┌─────────────────┬────────┐");
    println!(
        "  │ Total files     │ {:>6} │",
        stats.total_files.to_string().yellow()
    );
    println!(
        "  │ Total size      │ {:>6} │",
        format_size(stats.total_size).yellow()
    );
    println!(
        "  │ Sessions        │ {:>6} │",
        stats.session_count.to_string().yellow()
    );
    println!("  │ Avg confidence  │ {:>5.3} │", stats.avg_confidence);
    println!("  └─────────────────┴────────┘");

    // Category breakdown
    if let Ok(cats) = manifest.category_summary() {
        println!("\n  {}", "By Category".bold());
        println!("  ┌──────────────────────┬───────┬──────────┬─────────────┐");
        println!(
            "  │ {:<20} │ {:>5} │ {:>8} │ {:>11} │",
            "Category", "Files", "Size", "Avg Conf"
        );
        println!("  ├──────────────────────┼───────┼──────────┼─────────────┤");
        for (cat, count, avg_conf, size) in &cats {
            let bar = render_bar(*avg_conf, 4);
            let cat_display = if cat.len() > 20 {
                format!("{}…", &cat[..19])
            } else {
                cat.clone()
            };
            println!(
                "  │ {:<20} │ {:>5} │ {:>8} │ {:>5.2} {} │",
                cat_display.cyan(),
                count,
                format_size(*size),
                avg_conf,
                bar
            );
        }
        println!("  └──────────────────────┴───────┴──────────┴─────────────┘");
    }

    // Confidence distribution
    if let Ok(dist) = manifest.confidence_distribution() {
        println!("\n  {}", "Confidence Distribution".bold());
        let max_count = dist.iter().map(|(_, c)| *c).max().unwrap_or(1);
        for (bucket, count) in &dist {
            let bar = render_bar(*count as f64 / max_count as f64, 24);
            let pct = if stats.total_files > 0 {
                *count as f64 / stats.total_files as f64 * 100.0
            } else {
                0.0
            };
            println!("  {}  {} {} ({:.0}%)", bucket, bar.cyan(), count, pct);
        }
    }

    // Recent activity
    if let Ok(activity) = manifest.recent_activity(days) {
        if !activity.is_empty() {
            println!(
                "\n  {}",
                format!("Recent Activity (last {days} days)").bold()
            );
            for (date, total, cats) in &activity {
                let cat_summary: Vec<String> = cats
                    .iter()
                    .take(3)
                    .map(|(c, n)| format!("{c} ×{n}"))
                    .collect();
                println!(
                    "  {}  +{} files   {}",
                    date.dimmed(),
                    total.to_string().yellow(),
                    cat_summary.join(", ")
                );
            }
        }
    }
    println!();
}

// ─── Rules listing ─────────────────────────────────────────────────────────────

/// Print the active built-in keyword rules with notes.
pub fn print_rules() {
    let mut by_cat: BTreeMap<String, Vec<(String, f32, Option<String>)>> = BTreeMap::new();
    for kw in BUILTIN_KEYWORDS.iter() {
        by_cat.entry(kw.category.clone()).or_default().push((
            kw.word.clone(),
            kw.weight,
            kw.note.clone(),
        ));
    }

    println!("\n{}", " 📜 Active Rules ".black().on_cyan().bold());
    for (cat, words) in &by_cat {
        println!("  {} {}", "▸".cyan(), cat.bold());
        for (word, weight, note) in words.iter().take(12) {
            let note_str = note
                .as_deref()
                .map(|n| format!("  # {n}"))
                .unwrap_or_default();
            println!(
                "    {:.<35} weight {:.1}{}",
                format!("\"{}\" ", word),
                weight,
                note_str.dimmed()
            );
        }
    }
    println!();
}

// ─── Audit output ─────────────────────────────────────────────────────────────

/// A single audit drift item for display.
pub struct AuditDrift {
    pub file_path: String,
    pub current_category: String,
    pub current_confidence: f32,
    pub new_category: String,
    pub new_confidence: f32,
    pub reason: String,
}

/// Print the audit report.
pub fn print_audit_report(
    output_dir: &str,
    total_files: usize,
    high_conf: usize,
    medium_conf: usize,
    drifts: &[AuditDrift],
) {
    println!(
        "\n{}",
        format!(" 📋 FileMind Audit — {output_dir} — {total_files} files ")
            .black()
            .on_cyan()
            .bold()
    );
    println!();
    println!(
        "  ✅ High confidence (≥0.85):  {} files  ({:.0}%)",
        high_conf.to_string().green(),
        if total_files > 0 {
            high_conf as f64 / total_files as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "  ⚠️  Medium confidence:         {} files  ({:.0}%)",
        medium_conf.to_string().yellow(),
        if total_files > 0 {
            medium_conf as f64 / total_files as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "  ❌ Possible misclassification: {} files   ({:.0}%)",
        drifts.len().to_string().red(),
        if total_files > 0 {
            drifts.len() as f64 / total_files as f64 * 100.0
        } else {
            0.0
        }
    );

    if !drifts.is_empty() {
        println!("\n  {}", "Possible misclassifications:".bold());
        println!("  {}", "────────────────────────────".dimmed());
        for d in drifts {
            println!("  {}", d.file_path.white().bold());
            println!(
                "    Currently:  {} [stored confidence: {:.2}]",
                d.current_category.cyan(),
                d.current_confidence
            );
            println!(
                "    Re-analysis: {} [new confidence: {:.2}]",
                d.new_category.yellow(),
                d.new_confidence
            );
            println!("    Reason: {}", d.reason);
            println!();
        }
    }
}
