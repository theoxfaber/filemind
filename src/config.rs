use anyhow::{bail, Result};

/// Returns the Gemini API key from the environment.
pub fn gemini_api_key() -> Result<String> {
    match std::env::var("GEMINI_API_KEY") {
        Ok(k) if !k.is_empty() => Ok(k),
        _ => bail!(
            "GEMINI_API_KEY is not set.\n\
             Create a .env file with:\n\
             GEMINI_API_KEY=your_key_here\n\
             or export it in your shell."
        ),
    }
}
