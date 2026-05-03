# FileMind

**Intelligent content-aware file organizer**

Automatically categorizes and organizes files based on actual content, not just file extensions. Understands 45+ file types including PDFs, media, code, documents, and more.

![Status](https://img.shields.io/badge/Status-Production%20Ready-green)
![Language](https://img.shields.io/badge/Language-Rust-orange)
![License](https://img.shields.io/badge/License-MIT-blue)

---

## What It Does

FileMind analyzes file contents and intelligently organizes them into meaningful categories. Instead of just looking at `.pdf` or `.jpg`, it actually reads inside and understands what the file is about.

**Example:**
```
Downloads/
├── resume.pdf          → moved to Documents/CVs/
├── vacation.jpg        → moved to Media/Photos/
├── config.yaml         → moved to Dev/Config/
├── notes.md            → moved to Documents/Notes/
└── mystery.bin         → moved to Unknown/ (with analysis)
```

---

## Why You Need This

**Problem:** Every developer has a messy Downloads or Documents folder.

**Solution:** Run FileMind once. Everything organized. No manual work.

---

## Key Features

### 1. **Content-Based Classification**
- Reads file headers and magic bytes
- Extracts metadata (PDF title, image EXIF, document author)
- Understands context, not just file type
- 45+ supported file types

### 2. **45+ Supported File Types**

**Documents:** PDF, DOCX, PPTX, XLS, TXT, MD, RST  
**Media:** JPG, PNG, GIF, MP4, MP3, WAV, AVI, MOV  
**Code:** PY, JS, TS, RS, GO, C, CPP, JAVA, SQL  
**Archives:** ZIP, RAR, 7Z, TAR, GZ  
**Dev:** JSON, YAML, TOML, ENV, DOCKERFILE, LOCK  
**Binaries:** EXE, DLL, SO, DYLIB  
**Data:** CSV, PARQUET, DB, SQLITE  
**Other:** 20+ more types

### 3. **Keyword Extraction**
- Reads PDF text and finds keywords
- Analyzes document titles and metadata
- Scores relevance for smarter categorization

### 4. **Deterministic Organization**
- Batched operations (fast, reliable)
- Consistent results every time
- Reproducible folder structure

### 5. **Undo System**
- Every operation is tracked
- `filemind undo` reverses last organization
- Full operation history

### 6. **Audit Mode**
- Preview changes before applying
- `--dry-run` shows what would be moved
- Zero risk testing

### 7. **Size Optimization**
- Size-bucketed organization (< 1MB, < 100MB, > 100MB)
- Identifies duplicates via hash
- Cleanup recommendations

---

## Quick Start

### Installation

```bash
git clone https://github.com/theoxfaber/filemind
cd filemind
cargo build --release

# Or install as command
cargo install --path .
```

### Basic Usage

```bash
# Organize Downloads folder
filemind organize ~/Downloads

# Preview first (dry run)
filemind organize ~/Downloads --dry-run

# With verbose output
filemind organize ~/Downloads --verbose

# Create custom structure
filemind organize ~/Documents --config my-structure.json

# Undo last operation
filemind undo
```

### Output

```
[FileMind] Analyzing 342 files...
[✓] 156 documents moved to Documents/
[✓] 78 images moved to Media/Photos/
[✓] 45 videos moved to Media/Videos/
[✓] 52 code files moved to Dev/
[✓] 11 archives moved to Archives/

Organization complete!
- Time taken: 2.3s
- Moved: 342 files
- Duplicates found: 12
- Could not classify: 3

Next steps:
  filemind show-duplicates    # See duplicate files
  filemind cleanup            # Remove duplicates
```

---

## How It Works

### 1. File Analysis
- Read file header (first 512 bytes) for magic bytes
- Extract metadata (title, author, duration, etc.)
- Determine primary content type

### 2. Classification
- Match magic bytes against known signatures
- Analyze metadata context
- Apply keyword extraction for accuracy
- Assign confidence score

### 3. Organization
- Create target folder structure
- Batch move operations for reliability
- Track all moves (for undo)
- Report duplicates

### 4. Optimization
- Identify duplicate files (same hash)
- Suggest cleanup actions
- Size analysis and recommendations

---

## Configuration

Create `filemind.json` for custom organization:

```json
{
  "root": "/Users/you",
  "structure": {
    "Documents": {
      "CVs": ["pdf"],
      "Receipts": ["pdf"],
      "Notes": ["txt", "md"],
      "Books": ["pdf", "epub"]
    },
    "Media": {
      "Photos": ["jpg", "png", "webp"],
      "Videos": ["mp4", "mkv", "mov"],
      "Audio": ["mp3", "flac", "wav"]
    },
    "Dev": {
      "Config": ["json", "yaml", "toml"],
      "Code": ["py", "rs", "js", "go"],
      "SQL": ["sql", "db"]
    },
    "Archives": ["zip", "7z", "rar"],
    "Unknown": []
  },
  "rules": {
    "min_file_size_kb": 10,
    "follow_symlinks": false,
    "ignore_hidden": true,
    "batch_size": 100
  }
}
```

Then run:
```bash
filemind organize . --config filemind.json
```

---

## Advanced Features

### Find Duplicates
```bash
filemind find-duplicates ~/Downloads

# Output:
# Hash: a1b2c3d4e5f6...
#   1. report.pdf (2.3 MB)
#   2. report_final.pdf (2.3 MB)
#   3. report_final_FINAL.pdf (2.3 MB)
```

### Cleanup Duplicates
```bash
filemind cleanup-duplicates ~/Downloads

# Interactively choose which to keep
# Others are moved to Trash/
```

### Size Analysis
```bash
filemind analyze-sizes ~/Downloads

# Output:
# Total size: 125 GB
# Largest files:
#   Video.mp4: 45 GB
#   Archive.zip: 32 GB
#   Database.db: 28 GB
# 
# Duplicates: 12 GB could be freed
# Unused: 3.2 GB (not accessed in 6 months)
```

### Audit Trail
```bash
filemind history

# Shows all operations with timestamps
# Use for recovering from accidental moves
```

---

## Performance

- **Latency:** 100-200 files/second
- **Memory:** ~50MB baseline + file count
- **Disk I/O:** Sequential reads, minimal writes
- **Batching:** 100-file batches for reliability

### Example: 10,000 files
```
Time: ~60 seconds
Memory: ~150MB
Success rate: 99.8%
Errors: 17 (permission denied, etc.)
```

---

## Project Structure

```
filemind/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── classifier.rs        # File classification engine
│   ├── magic.rs             # Magic byte matching
│   ├── metadata.rs          # Metadata extraction
│   ├── organizer.rs         # Move operations
│   ├── audit.rs             # History tracking
│   ├── dedup.rs             # Duplicate detection
│   └── types.rs             # Data structures
├── tests/                   # Integration tests
├── Cargo.toml
└── README.md
```

---

## Installation Methods

### From Source
```bash
git clone https://github.com/theoxfaber/filemind
cd filemind
cargo install --path .
```

### From Cargo
```bash
cargo install filemind
```

### From Release Binary
Download from [Releases](https://github.com/theoxfaber/filemind/releases)

---

## Testing

```bash
# Unit tests
cargo test

# Integration tests (actual file operations)
cargo test -- --include-ignored

# Benchmark
cargo bench
```

---

## Known Limitations

- **Symlinks:** Currently skips symbolic links (can be enabled)
- **Network drives:** Performance limited by network speed
- **Large files:** Files > 5GB analysis may be slow
- **Special chars:** Some filesystem characters may cause issues

---

## Future Roadmap

- [ ] Machine learning-based classification
- [ ] Watch mode (auto-organize on new files)
- [ ] Cloud sync integration
- [ ] GUI application
- [ ] Network share support

---

## Troubleshooting

### "Permission Denied" Errors
```bash
# Check permissions
ls -la ~/Downloads

# Run with sudo if needed (careful!)
sudo filemind organize ~/Protected
```

### Files Not Moving
```bash
# Use --verbose to see why
filemind organize . --verbose

# Check audit log
filemind history
```

### Undo Failed
```bash
# View undo history
filemind history --limit 10

# Manual recovery (files in FileMind/Trash)
find . -path "*FileMind/Trash*" -type f
```

---

## Contributing

Contributions welcome:
1. Fork the repo
2. Create feature branch
3. Add tests
4. Submit PR

---

## License

MIT License — see [LICENSE](LICENSE)

---

## Get In Touch

💬 **Bug report?** Open an issue  
💼 **Want to use FileMind professionally?** Available for consulting  
📧 **Questions?** DM me

---

**Built with Rust | Fast. Reliable. Open Source.**

⭐ If this saved you time, star the repo!
