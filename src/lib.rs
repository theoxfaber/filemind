//! FileMind — production-grade, deterministic, content-aware file organizer.
//!
//! This crate exposes the core library API.  The `filemind` binary wires
//! everything together via `src/main.rs`.
//!
//! # Library Usage
//!
//! ```rust,no_run
//! use filemind::classifier;
//! use filemind::config::Config;
//! use filemind::extractor;
//! use std::path::Path;
//!
//! let path = Path::new("invoice.pdf");
//! let config = Config::default();
//! let extracted = extractor::extract(path).unwrap();
//! let result = classifier::classify(path, &extracted, &config);
//! println!("{} → {} ({:.0}%)", path.display(), result.category, result.confidence * 100.0);
//! ```

pub mod audit;
pub mod classifier;
pub mod config;
pub mod engine;
pub mod error;
pub mod extractor;
pub mod manifest;
pub mod organizer;
pub mod session;
pub mod ui;
