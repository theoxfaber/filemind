<div align="center">

```
  ███████╗██╗██╗     ███████╗███╗   ███╗██╗███╗   ██╗██████╗
  ██╔════╝██║██║     ██╔════╝████╗ ████║██║████╗  ██║██╔══██╗
  █████╗  ██║██║     █████╗  ██╔████╔██║██║██╔██╗ ██║██║  ██║
  ██╔══╝  ██║██║     ██╔══╝  ██║╚██╔╝██║██║██║╚██╗██║██║  ██║
  ██║     ██║███████╗███████╗██║ ╚═╝ ██║██║██║ ╚████║██████╔╝
  ╚═╝     ╚═╝╚══════╝╚══════╝╚═╝     ╚═╝╚═╝╚═╝  ╚═══╝╚═════╝
```

**The intelligent, content-aware file organizer — now in Rust.**

[![Rust](https://img.shields.io/badge/Built_with-Rust-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue?style=flat-square)](LICENSE)
[![Powered by Gemini](https://img.shields.io/badge/AI-Google_Gemini-4285F4?style=flat-square&logo=google)](https://ai.google.dev/)

*Transform digital chaos into structured clarity — no web server, no Python, pure terminal.*

</div>

---

## 🧠 What is FileMind?

**FileMind** scans a directory of messy files, extracts their content (PDFs, code, text), sends it to **Google Gemini AI** for classification, and organizes everything into a clean folder hierarchy — all from your terminal.

**v2.0 is a complete rewrite in Rust.** Faster, smaller, zero-dependency runtime (no Python, no venv, no uvicorn), and 100% terminal-native.

---

## ✨ Features

| Feature | Description |
|---|---|
| 🔍 **Deep Content Analysis** | Extracts text from PDFs, `.txt`, `.md`, `.rs`, `.py`, `.json`, `.csv`, and 10+ more formats |
| 🧠 **AI-Powered Classification** | Google Gemini 2.0 Flash classifies files with confidence scores + reasoning |
| ✨ **Smart Renaming** | Optional `YYYY-MM-DD — Category — filename` semantic rename |
| 🛡️ **MD5 Deduplication** | Never processes the same file twice, across sessions |
| ⚡ **Concurrent Pipeline** | Configurable parallelism (`-c 8`) for batch processing |
| 📊 **Live Progress Bar** | Real-time spinner with file-by-file status |
| 📦 **Zip Export** | Pack your organized output into a `.zip` with one command |
| 🔄 **Local Sync** | Mirror output to any path on your filesystem |
| 🗂️ **Persistent Manifest** | JSON log of every organized file (category, confidence, md5, timestamp) |
| 🖥️ **Terminal-First** | No web server. No browser. No background daemon. Pure CLI. |

---

## 🚀 Quick Start

### 1. Prerequisites

- [Rust 1.75+](https://rustup.rs/) (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- A [Google Gemini API key](https://aistudio.google.com/app/apikey) (free tier available)

### 2. Clone & Build

```bash
git clone https://github.com/theoxfaber/filemind.git
cd filemind
cargo build --release
```

The binary is at `./target/release/filemind`.

Install it system-wide (optional):
```bash
cargo install --path .
```

### 3. Configure API Key

```bash
# Option A: .env file (recommended)
echo "GEMINI_API_KEY=your_key_here" > .env

# Option B: shell export
export GEMINI_API_KEY=your_key_here
```

---

## 🖥️ Usage

```
filemind [COMMAND] [OPTIONS]
```

### `organize` — The main pipeline

```bash
# Organize files in ./inbox → ./output
filemind organize --input ./inbox --output ./output

# Enable smart semantic renaming
filemind organize -i ./inbox -o ./output --smart-rename

# Dry-run: see what would happen, touch nothing
filemind organize -i ./inbox --dry-run

# Increase concurrency to 8 parallel Gemini calls
filemind organize -i ./inbox -c 8
```

**Output structure example:**
```
output/
├── Invoices/
│   └── 2025-04-25 — Invoices — receipt_amazon.pdf
├── Code/
│   └── script.py
├── Medical/
│   └── blood_test_results.pdf
└── Needs Review/
    └── unknown_binary.dat
```

### `status` — View the manifest

```bash
filemind status --output ./output
```

```
 📋 FileMind Manifest — 14 files

  Code (3)
    → script.py  [100%]
    → main.rs  [100%]
    → utils.ts  [98%]
  Invoices (5)
    → 2025-04-25 — Invoices — receipt.pdf  [100%]
    ...
  Needs Review (2)
    → mystery_file.dat  [0%]
```

### `pack` — Create a zip archive

```bash
filemind pack --output ./output --zip filemind_organized.zip
```

### `sync` — Copy to another directory

```bash
filemind sync --output ./output --target ~/Documents/Organized
```

---

## 📂 Supported File Types

| Type | Extensions | Method |
|---|---|---|
| Plain text | `.txt`, `.md`, `.log` | Direct read |
| Source code | `.rs`, `.py`, `.js`, `.ts`, `.sh` | Direct read |
| Data/config | `.json`, `.csv`, `.yaml`, `.toml`, `.xml` | Direct read |
| PDF | `.pdf` | Pure-Rust extraction (`pdf-extract`) |
| Web | `.html`, `.htm`, `.css` | Direct read |
| Other | anything else | Filename-only classification |

> **No Tesseract required.** OCR for scanned images is not needed for the vast majority of files. Pure-Rust PDF text extraction handles most documents.

---

## 🏗️ Architecture

```
src/
├── main.rs        # CLI dispatcher (clap)
├── config.rs      # API key resolution
├── extractor.rs   # Text extraction (PDF + plain text)
├── classifier.rs  # Async Gemini API client with retry
├── organizer.rs   # File pipeline, zip, sync, dedup
├── manifest.rs    # Persistent JSON manifest
└── ui.rs          # ASCII banner, colored output
```

**Key design decisions:**
- **`tokio` async** with a `Semaphore`-bounded concurrency pool — no thread-per-file overhead
- **`reqwest` + `rustls`** — pure-Rust TLS, no OpenSSL system dependency
- **`pdf-extract`** — no `tesseract` / no C deps for PDF text
- **MD5 dedup** persisted in `manifest.json` — survives restarts
- **`indicatif`** progress bars — always know what's happening

---

## ⚙️ Configuration

All config is via environment variables (or `.env`):

| Variable | Required | Description |
|---|---|---|
| `GEMINI_API_KEY` | ✅ Yes | Your Google Gemini API key |

---

## 🤝 Contributing

PRs welcome. The codebase is intentionally small and modular. Each file has one responsibility.

```bash
cargo fmt        # Format
cargo clippy     # Lint
cargo test       # Test
```

---

## 📄 License

MIT — see [LICENSE](LICENSE).

---

<div align="center">

*"The best file manager is the one you never have to manage."* 🚀

**[theoxfaber](https://github.com/theoxfaber)**

</div>
