<p align="center">
  <img src="https://img.shields.io/badge/rust-1.75+-orange?style=flat-square&logo=rust" alt="MSRV" />
  <img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License" />
  <img src="https://img.shields.io/badge/binary-6.3MB-brightgreen?style=flat-square" alt="Binary Size" />
  <img src="https://img.shields.io/badge/CI-passing-success?style=flat-square" alt="CI" />
</p>

<h1 align="center">
  🧠 FileMind
</h1>

<p align="center">
  <strong>Deterministic, content-aware file organizer — zero AI, zero network, single binary.</strong>
</p>

<p align="center">
  FileMind reads your files' content, metadata, and magic bytes to classify them into<br/>
  organized folders with full undo, explainable confidence scores, and TOML-based rules.
</p>

---

## Why FileMind?

Most file organizers are glorified extension sorters.  FileMind runs a **3-tier classification engine** that actually reads your files:

| Tier | Signal | Speed | Example |
|------|--------|-------|---------|
| **Tier 1** | Extension + magic bytes | ~0 ms | `.pdf` → Documents, `%PDF-1.4` header confirmed |
| **Tier 2** | Keyword scoring on extracted text | ms range | `"Invoice #1234 Total Due $500"` → Documents/Invoices |
| **Tier 3** | Filename + path heuristics | ~0 ms | `receipt_amazon.pdf` → invoices boost |

Every decision is **explainable** (`--explain`), **undoable** (`filemind undo`), and **auditable** (`filemind audit`).

## Quick Start

```bash
# Install from source (Rust 1.75+)
cargo install --path .

# Organize your Downloads folder
filemind organize -i ~/Downloads -o ~/Organized

# Preview without touching anything
filemind organize -i ~/Downloads --dry-run --explain

# Watch a folder for new files
filemind watch ~/Downloads

# Undo the last organize session
filemind undo
```

## Features

### 🔬 Content-Aware Classification
- **PDF text extraction** (pure Rust — no Tesseract, no Python)
- **Magic byte detection** via the `infer` crate (covers 200+ formats)
- **Keyword scoring** with sqrt-dampened frequency weighting
- **45+ file extensions** mapped with tuned base confidence scores

### 📊 Full Observability
```bash
# Per-file evidence breakdown
filemind organize -i . --explain

# Category summary
filemind status

# Full analytics with confidence distribution
filemind stats

# Classification drift audit
filemind audit
```

### ⚡ Performance
- **Batched SQLite writes** — single transaction, not N individual inserts
- **Size-bucketed hashing** — files >1MB use partial hash (first 64KB + last 64KB + size), avoiding 4GB RAM spikes
- **Rayon + Tokio** — CPU-bound classification on thread pool, async I/O for orchestration
- **6.3MB stripped binary** with LTO

### 🔧 Fully Configurable
```toml
# ~/.config/filemind/config.toml
[general]
output_dir = "~/Organized"
smart_rename = true
concurrency = 8
min_confidence = 0.60
conflict = "rename_new"
debounce_ms = 300
extract_bytes = 8192

[categories.invoices]
keywords = [
  { word = "GST", weight = 3.0 },
  { word = "purchase order", weight = 2.5 },
]
```

### 📋 Audit & Drift Detection
The killer feature — re-classify your already-organized files to catch misclassifications:

```bash
# Check for drift (non-destructive)
filemind audit

# Actually move flagged files
filemind audit --apply

# Machine-readable output
filemind audit --output-format json
```

### 🛡 Full Undo with Integrity Verification
Every session is checkpointed.  Undo verifies SHA-256 before restoring:

```bash
# List sessions
filemind sessions

# Undo session #3
filemind undo --session 3

# Inspect session details
filemind sessions --show 3
```

### 📏 Rule Management
```bash
# List all active rules with notes
filemind rules list

# Test classification on a single file
filemind rules check invoice.pdf

# Add a custom keyword
filemind rules add invoices "purchase order" 2.5

# Remove a keyword
filemind rules remove invoices "purchase order"

# Export built-in keywords to customize
filemind keywords export > my_keywords.toml
```

### 🔌 Pipeline Composability
```bash
# JSON output for scripting
filemind organize -i . --output-format json | jq '.category'

# CSV output for spreadsheets
filemind status --output-format csv > status.csv

# .filemindignore support (same syntax as .gitignore)
echo "*.tmp" >> .filemindignore
filemind organize -i .
```

## Architecture

```
src/
├── main.rs         # CLI dispatcher (clap derive)
├── lib.rs          # Library root
├── classifier.rs   # 3-tier classification engine
├── config.rs       # TOML config loading + rule merging
├── engine.rs       # Async pipeline: walk → extract → classify → act
├── extractor.rs    # Content extraction (PDF, text, source code)
├── manifest.rs     # SQLite manifest (batched writes)
├── session.rs      # Undo, size-bucketed hashing
├── organizer.rs    # File operations + conflict resolution
├── audit.rs        # Classification drift detection
├── ui.rs           # Terminal rendering (banner, tables, stats)
├── watcher.rs      # File-system watch mode
├── completions.rs  # Shell completion generation
└── error.rs        # Typed error hierarchy (thiserror)

assets/
└── keywords.toml   # Built-in keyword list (embedded at compile time)
```

## Design Decisions

| Decision | Rationale |
|----------|-----------|
| No `Arc<Mutex<Connection>>` | Results collected via channels, batch-inserted in single transaction |
| Partial hashing for large files | 64KB head + 64KB tail + size = unique enough, avoids OOM |
| `infer` crate over manual magic bytes | Single source of truth for MIME detection, 200+ formats |
| Embedded `keywords.toml` | Auditable, exportable, user-customizable without recompilation |
| `ignore` crate for `.filemindignore` | Same engine as ripgrep — battle-tested glob matching |
| Classifier returns raw confidence | Engine applies the confidence gate, not the classifier |

## Building

```bash
# Debug
cargo build

# Release (LTO + strip)
cargo build --release

# Run tests (34 unit tests)
cargo test --lib

# Clippy (zero warnings policy)
cargo clippy --all-targets --all-features -- -D warnings

# Benchmarks
cargo bench
```

## License

MIT
