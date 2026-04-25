//! FileMind — production-grade, deterministic, content-aware file organizer.
//!
//! This crate exposes the core library API.  The `filemind` binary wires
//! everything together via `src/main.rs`.

pub mod classifier;
pub mod config;
pub mod engine;
pub mod error;
pub mod extractor;
pub mod manifest;
pub mod organizer;
pub mod session;
pub mod ui;
