<![CDATA[<div align="center">

# 🧠 FileMind

**The first Rust-native, single-binary, content-aware file organizer with explainable confidence scores.**

*Zero AI · Zero network · Zero Python · Single binary*

[![CI](https://github.com/theoxfaber/filemind/actions/workflows/ci.yml/badge.svg)](https://github.com/theoxfaber/filemind/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org)

</div>

---

## Why FileMind?

Every existing file organizer is one of:

| Tool | Method | Limitation |
|------|--------|------------|
| AI-powered tools | GPT / Gemini / Ollama | Non-deterministic, requires API key or GPU |
| [organize](https://github.com/tfeldmann/organize) | Python rules + content | Needs pip, slow, no binary |
| [hazel](https://www.noodlesoft.com/) / [hazelnut](https://github.com/jhrcook/hazelnut) | Extension/name rules only | No content reading |

**FileMind v3** reads file content (PDF text, source code, CSV/JSON headers, magic bytes) and classifies with a **3-tier deterministic engine** that emits explainable confidence scores. Every decision is transparent — no black box.

---

## Features

- **3-Tier Deterministic Classifier** — Extension + magic bytes → keyword scoring on content → filename/path heuristics
- **`--explain` flag** — See exactly *why* each file was classified (offsets, weights, tier)
- **Full undo** — Every session recorded in SQLite; `filemind undo` restores with checksum verification
- **TOML config** — Define custom categories with weighted keywords at `~/.config/filemind/config.toml`
- **Watch mode** — `filemind watch <dir>` organizes new files automatically
- **Smart rename** — `YYYY-MM-DD — Category — filename.pdf`
- **Shell completions** — bash, zsh, fish, elvish
- **Single binary** — `cargo install filemind` or download from releases
- **Cross-platform** — Linux, macOS, Windows

---

## Quick Start

### Install

```bash
# From source
cargo install --path .

# Or build release binary
cargo build --release
# Binary at: target/release/filemind
```

### Organize a directory

```bash
# Basic organize (copies files to ./output/)
filemind organize -i ~/Downloads

# Dry run — see what would happen
filemind organize -i ~/Downloads --dry-run

# With explainable output
filemind organize -i ~/Downloads --explain

# Smart rename + custom output
filemind organize -i ~/inbox -o ~/Organized --smart-rename -c 8
```

### Example `--explain` output

```
  ✓ receipt_amazon.pdf → Documents/Invoices [confidence: 0.94]
    tier-1  .pdf extension        +0.60
    tier-1  magic bytes: PDF      +0.10
    tier-2  "invoice" ×3          +3.00  (offsets: 142, 890, 2103)
    tier-2  "total due" ×1        +2.50
    tier-2  "amount" ×2           +1.50
    tier-3  filename "receipt"    +0.12
    →  decisive tier: tier-2 (content keywords)
```

---

## All Commands

```
filemind organize -i <dir> [-o <dir>] [--dry-run] [--explain] [--smart-rename] [-c <n>] [--copy]
filemind watch <dir>              # Live watch mode — organize on new files
filemind undo [--session <id>]    # Restore files from last or specific session
filemind sessions [--show <id>]   # List or inspect sessions
filemind status [-o <dir>]        # Show manifest summary table
filemind rules list               # Show active classification rules
filemind rules check <file>       # Classify a single file with --explain
filemind pack [-o <dir>] [--zip <file>]      # Zip the output folder
filemind sync [-o <dir>] --target <path>     # Mirror output to target
filemind completions <shell>      # Generate shell completions (bash/zsh/fish/elvish)
```

---

## How the 3-Tier Classifier Works

All three tiers run independently and produce a confidence score `[0.0–1.0]`. The highest combined score wins.

### Tier 1 — Extension + Magic Bytes (~0 ms, always runs)

Maps 200+ file extensions to categories. Reads first 16 bytes for magic-byte detection (`%PDF`, `PK\x03\x04`, `\x89PNG`, etc.). Produces a base confidence, e.g. `.pdf` → Documents @ 0.60.

### Tier 2 — Keyword Scoring (ms range, runs when text extractable)

Extracts up to 4 KB of text content, then scores against weighted keyword lists per category:

| Category | Keywords |
|----------|----------|
| Invoices | invoice, total due, bill to, amount, receipt, payment, subtotal |
| Medical | diagnosis, prescription, patient, dosage, clinic, physician |
| Legal | agreement, whereas, party, liability, jurisdiction, contract |
| Code | fn, struct, impl, import, def, #include, function, class |
| Finance | portfolio, dividend, equity, balance sheet, quarterly |
| Research | abstract, bibliography, hypothesis, methodology, references |

**Subcategory inheritance**: When a subcategory like `Documents/Invoices` matches keywords, it inherits its parent's base confidence from tier-1.

### Tier 3 — Filename + Path Heuristics (~0 ms, always runs)

Regex patterns on filenames (`receipt_*`, `IMG_*`, `Screenshot*`), plus path segment signals (if a parent folder is named `invoices/`, boost the invoice score).

---

## Configuration

FileMind looks for config at `~/.config/filemind/config.toml` (override with `$FILEMIND_CONFIG`).

```toml
[general]
output_dir = "~/Documents/Organized"
smart_rename = false
concurrency = 4
min_confidence = 0.5       # Below this → "Needs Review/" folder
conflict = "rename_new"    # skip | overwrite | rename_new | rename_existing
copy = true                # Copy files (true) or move them (false)

[categories.invoices]
keywords = [
  { word = "invoice", weight = 3.0 },
  { word = "GST", weight = 2.5 },       # India-specific
]
output_folder = "Finance/Invoices"

# Create entirely new categories
[categories.recipes]
keywords = [
  { word = "ingredients", weight = 3.0 },
  { word = "preheat", weight = 2.0 },
  { word = "serves", weight = 1.5 },
]
output_folder = "Personal/Recipes"
extensions = [".pdf", ".txt"]
```

---

## Undo System

Every `organize` run creates a session in SQLite. Files can be restored with checksum verification.

```bash
# List all sessions
filemind sessions

# Inspect a specific session
filemind sessions --show 3

# Undo the last session
filemind undo

# Undo a specific session
filemind undo --session 3
```

Undo verifies the SHA-256 checksum of each file before restoring — if a file was modified after organizing, it warns and skips (no silent data loss).

---

## Architecture

```
src/
├── main.rs          # clap v4 dispatcher — subcommand routing
├── lib.rs           # Public library API
├── error.rs         # Typed errors (thiserror) — no unwrap() in library code
├── config.rs        # TOML loader, rule merging, ConflictStrategy
├── extractor.rs     # Content extraction: PDF (pdf-extract), text, code, CSV
├── classifier.rs    # 3-tier deterministic engine — the core innovation
├── engine.rs        # Async pipeline: walk → extract → classify → act
├── organizer.rs     # File ops: copy/move, smart rename, conflict resolution
├── manifest.rs      # SQLite manifest (rusqlite) — every operation recorded
├── session.rs       # Session log for full undo support
├── ui.rs            # indicatif progress bars, --explain rendering
├── watcher.rs       # notify-based watch mode
└── completions.rs   # Shell completions via clap_complete
```

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` + `clap_complete` | CLI parsing + shell completions |
| `tokio` | Async runtime for concurrent pipeline |
| `rusqlite` (bundled) | SQLite manifest + session tracking |
| `pdf-extract` | Pure-Rust PDF text extraction (no C deps) |
| `notify` | Cross-platform file watching |
| `indicatif` + `console` | Terminal progress bars + colors |
| `sha2` + `md5` | Undo integrity + dedup checksums |
| `infer` | Magic-byte MIME detection |
| `thiserror` + `anyhow` | Typed + ergonomic error handling |

---

## Code Quality

- ✅ Every public function has doc comments
- ✅ All errors typed via `thiserror` — no `unwrap()` in library code
- ✅ Zero clippy warnings (`cargo clippy -- -D warnings`)
- ✅ `cargo fmt` enforced
- ✅ 24 unit tests covering classifier, config, extractor, organizer, sessions
- ✅ GitHub Actions CI: fmt + clippy + release build on Ubuntu + macOS

---

## License

MIT

---

<div align="center">
<sub>Built with 🦀 Rust — deterministic, explainable, fast.</sub>
</div>
]]>
