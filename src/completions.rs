//! Shell completion generation via `clap_complete`.
//!
//! Usage: `filemind completions bash`

use clap_complete::{generate, Shell};

use super::Cli;

/// Generate completions for `shell` and write them to stdout.
pub fn generate_completions(shell: Shell) {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "filemind", &mut std::io::stdout());
}
