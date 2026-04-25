<p align="center">
  <img src="https://img.shields.io/badge/v3.0-rust--native-F74C00?style=for-the-badge&logo=rust&logoColor=white" alt="Rust Native">
  <img src="https://img.shields.io/badge/zero--AI-deterministic-10B981?style=for-the-badge" alt="Zero AI">
  <img src="https://img.shields.io/badge/single--binary-4.8MB-6366F1?style=for-the-badge" alt="Single Binary">
</p>

<h1 align="center">🧠 FileMind</h1>

<p align="center">
  <strong>Content-aware file organizer that reads your files, not your API key.</strong><br>
  <sub>3-tier deterministic classifier · explainable confidence scores · full undo · single binary</sub>
</p>

<p align="center">
  <a href="#install">Install</a> •
  <a href="#quick-start">Quick Start</a> •
  <a href="#how-it-works">How It Works</a> •
  <a href="#configuration">Config</a> •
  <a href="#commands">Commands</a>
</p>

---

## The Problem

Every existing file organizer falls into one of three traps:

| Approach | Example | The Catch |
|:---------|:--------|:----------|
| 🤖 AI-powered | ChatGPT / Gemini wrappers | Non-deterministic, needs API key or GPU |
| 📎 Extension-only | Hazel, hazelnut | `report.pdf` and `invoice.pdf` both → "Documents" |
| 🐍 Python + content | tfeldmann/organize | Needs pip, virtualenv, slow startup |

**FileMind** actually reads file content — PDF text, source code, CSV headers, magic bytes — and classifies with a deterministic 3-tier engine. Every decision is transparent.

---

## Install

```bash
# Build from source
cargo install --path .

# Or grab the release binary
cargo build --release
# → target/release/filemind (4.8 MB)
```

No Python. No npm. No Docker. No API keys. Just a single binary.

---

## Quick Start

```bash
# Organize your Downloads folder
filemind organize -i ~/Downloads -o ~/Organized

# See what would happen (no files moved)
filemind organize -i ~/Downloads --dry-run

# See WHY each file was classified
filemind organize -i ~/Downloads --explain
```

### `--explain` output

```
✓ receipt_amazon.pdf → Documents/Invoices [94%]
  ├─ tier-1  .pdf extension              base 0.60
  ├─ tier-1  magic: %PDF header           +0.10
  ├─ tier-2  "invoice" ×3 found           +3.00  @142, @890, @2103
  ├─ tier-2  "total due" ×1 found         +2.50
  ├─ tier-3  filename contains "receipt"   +0.12
  └─ decisive: tier-2 (content keywords)
```

Every classification shows exactly which tier decided, which keywords matched, and at what byte offsets.

---

## How It Works

Three tiers run independently. The category with the highest combined score wins.

```
                    ┌─────────────────┐
   file.pdf ──────▶│  Tier 1: Magic  │──▶ Documents @ 0.70
                    │  Extension+MIME │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
   "invoice #12"──▶│  Tier 2: Text   │──▶ Documents/Invoices @ 1.00
   "total due $5"  │  Keyword Scoring │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
   receipt_*.pdf──▶│  Tier 3: Name   │──▶ +0.12 boost
                    │  Path Heuristics│
                    └─────────────────┘
```

### Tier 1 — Extension + Magic Bytes

Maps 200+ extensions. Reads first 16 bytes for magic detection (`%PDF`, `PK\x03\x04`, `\x89PNG`, `fLaC`...).

### Tier 2 — Content Keyword Scoring

Extracts up to 4 KB of text (PDF via pure-Rust `pdf-extract`, source code, plaintext). Scores against weighted keyword lists:

```
Documents/Invoices   → invoice(3.0) total_due(2.5) bill_to(2.5) receipt(2.0)
Documents/Medical    → diagnosis(3.0) prescription(3.0) patient(2.5) dosage(2.0)
Documents/Legal      → agreement(3.0) whereas(3.0) jurisdiction(3.0) liability(2.5)
Code                 → fn(2.0) struct(2.0) #include(3.0) def(2.0) class(1.5)
Finance              → portfolio(2.5) dividend(3.0) balance_sheet(3.0) revenue(2.0)
```

Subcategories inherit parent confidence — `Documents/Invoices` gets the `.pdf` base from `Documents`.

### Tier 3 — Filename + Path Heuristics

Pattern matching on filenames (`receipt_*`, `IMG_*`, `Screenshot*`) and parent directory names (`invoices/`, `medical/`, `src/`).

---

## Commands

```
filemind organize   Scan and organize files by detected category
filemind watch      Live-watch a directory, organize new files automatically
filemind undo       Restore files from a previous session (SHA-256 verified)
filemind sessions   List or inspect past organize sessions
filemind status     Category summary table from the manifest
filemind rules      Inspect active classification rules
filemind pack       Zip the organized output directory
filemind sync       Mirror output to another local path
filemind completions Generate shell completions (bash/zsh/fish/elvish)
```

### Organize options

```bash
filemind organize \
  -i ~/Downloads \       # Input directory
  -o ~/Organized \       # Output directory (default: ./output)
  --explain \            # Show classification reasoning
  --smart-rename \       # Prefix: "2024-04-25 — Invoices — receipt.pdf"
  --dry-run \            # Preview without moving files
  --copy \               # Copy instead of move (default)
  -c 8                   # Parallel workers (default: 4)
```

---

## Undo

Every run is recorded in SQLite. Files can be restored with integrity verification.

```bash
filemind sessions         # List all sessions
filemind undo             # Undo the last session
filemind undo --session 3 # Undo a specific session
```

Before restoring, FileMind verifies the **SHA-256 checksum** of each file. If a file was modified after organizing, it warns and skips — no silent data loss.

---

## Configuration

`~/.config/filemind/config.toml` — override with `$FILEMIND_CONFIG`.

```toml
[general]
output_dir = "~/Organized"
concurrency = 4
min_confidence = 0.5       # Below → "Needs Review/"
conflict = "rename_new"    # skip | overwrite | rename_new | rename_existing
copy = true

# Add custom keywords to existing categories
[categories.invoices]
keywords = [
  { word = "GST", weight = 2.5 },
  { word = "GSTIN", weight = 3.0 },
]
output_folder = "Finance/Invoices"

# Create entirely new categories
[categories.recipes]
keywords = [
  { word = "ingredients", weight = 3.0 },
  { word = "preheat", weight = 2.0 },
  { word = "tablespoon", weight = 1.5 },
]
```

---

## Project Structure

```
src/
├── classifier.rs    ← 3-tier engine (the core innovation)
├── extractor.rs     ← PDF, text, code content extraction
├── engine.rs        ← Async pipeline: walk → extract → classify → act
├── manifest.rs      ← SQLite persistence layer
├── session.rs       ← Undo with SHA-256 integrity checks
├── organizer.rs     ← File ops, conflict resolution, smart rename
├── config.rs        ← TOML config loader
├── ui.rs            ← Progress bars, --explain rendering
├── watcher.rs       ← Live directory monitoring (notify)
├── error.rs         ← Typed errors via thiserror
├── completions.rs   ← Shell completion generation
├── lib.rs           ← Public library API
└── main.rs          ← CLI dispatcher (clap v4)
```

### Dependencies

| Crate | Why |
|:------|:----|
| `clap` | CLI parsing + completions |
| `tokio` | Async pipeline orchestration |
| `rusqlite` | SQLite manifest (bundled, no system dep) |
| `pdf-extract` | Pure-Rust PDF text extraction |
| `notify` | Cross-platform file watching |
| `sha2` / `md5` | Undo integrity + dedup |
| `infer` | Magic-byte MIME detection |
| `indicatif` | Terminal progress bars |

---

## Quality

```
✓ 24 tests passing          cargo test --lib
✓ Zero clippy warnings      cargo clippy -- -D warnings
✓ Formatted                 cargo fmt -- --check
✓ CI on Ubuntu + macOS      GitHub Actions
✓ 4.8 MB release binary     opt-level=3, LTO, stripped
```

---

<p align="center">
  <sub>MIT License · Built with Rust 🦀</sub>
</p>
