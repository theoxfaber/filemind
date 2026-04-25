use anyhow::{Context, Result};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const GEMINI_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Classification {
    pub category: String,
    pub confidence: u8,
    pub reasoning: String,
}

impl Default for Classification {
    fn default() -> Self {
        Self {
            category: "Unclassified".to_string(),
            confidence: 0,
            reasoning: "Classification was not attempted.".to_string(),
        }
    }
}

/// Calls the Gemini API and returns a structured Classification result.
/// Retries once on failure. Falls back to "Unclassified" if both attempts fail.
pub async fn classify(
    client: &Client,
    api_key: &str,
    filename: &str,
    text: &str,
) -> Classification {
    match call_gemini(client, api_key, filename, text).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("  [warn] First attempt failed for '{filename}': {e}");
            // retry once after a short pause
            tokio::time::sleep(Duration::from_millis(1200)).await;
            match call_gemini(client, api_key, filename, text).await {
                Ok(c) => c,
                Err(e2) => {
                    eprintln!("  [warn] Second attempt failed for '{filename}': {e2}");
                    Classification {
                        category: "Unclassified".to_string(),
                        confidence: 0,
                        reasoning: "Gemini API call failed after 2 attempts.".to_string(),
                    }
                }
            }
        }
    }
}

async fn call_gemini(
    client: &Client,
    api_key: &str,
    filename: &str,
    text: &str,
) -> Result<Classification> {
    let prompt = build_prompt(filename, text);

    let body = serde_json::json!({
        "contents": [{ "parts": [{ "text": prompt }] }],
        "generationConfig": {
            "temperature": 0.1,
            "maxOutputTokens": 256
        }
    });

    let resp = client
        .post(format!("{GEMINI_URL}?key={api_key}"))
        .json(&body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("HTTP request to Gemini failed")?;

    let status = resp.status();
    let raw: serde_json::Value = resp.json().await.context("Failed to decode Gemini JSON")?;

    if !status.is_success() {
        anyhow::bail!(
            "Gemini API error {}: {}",
            status,
            raw["error"]["message"].as_str().unwrap_or("unknown")
        );
    }

    let generated_text = raw["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .context("No text in Gemini response")?;

    parse_classification(generated_text)
}

fn build_prompt(filename: &str, text: &str) -> String {
    format!(
        r#"You are an expert file organization agent.
Analyze the filename and extracted text to determine the most logical category for this file.
If the extracted text is empty or unclear, rely on the filename and extension.

### Examples:
Input: Filename: "invoice_123.pdf", Text: "Billed to: John Doe, Total: $50"
Output: {{"category": "Invoices", "confidence": 100, "reasoning": "Standard invoice layout."}}

Input: Filename: "vacation_photo.jpg", Text: ""
Output: {{"category": "Photos", "confidence": 90, "reasoning": "Image file with descriptive name."}}

Input: Filename: "script.py", Text: "import os\nprint('hello')"
Output: {{"category": "Code", "confidence": 100, "reasoning": "Python source code."}}

Input: Filename: "medical_report.pdf", Text: "Patient: Alice, Diagnosis: Flu"
Output: {{"category": "Medical", "confidence": 95, "reasoning": "Patient and diagnostic info."}}

Input: Filename: "xyz_random.dat", Text: "binary data 010101"
Output: {{"category": "Misc", "confidence": 30, "reasoning": "Unknown binary content."}}

Now analyze:
Filename: {filename}
Extracted Text (first 3000 chars):
{text}

Respond with confidence 0-100.
Respond ONLY with valid JSON (no markdown fences):
{{"category": "<Category Name>", "confidence": <int>, "reasoning": "<Concise explanation>"}}"#
    )
}

fn parse_classification(raw: &str) -> Result<Classification> {
    // Strip markdown code fences if present
    let re = Regex::new(r"```(?:json)?\s*([\s\S]*?)\s*```").unwrap();
    let cleaned = if let Some(caps) = re.captures(raw) {
        caps[1].to_string()
    } else {
        raw.to_string()
    };

    // Find the JSON object in the response
    let json_re = Regex::new(r"\{[\s\S]*\}").unwrap();
    let json_str = json_re
        .find(&cleaned)
        .map(|m| m.as_str())
        .unwrap_or(&cleaned);

    let v: serde_json::Value =
        serde_json::from_str(json_str).context("Failed to parse Gemini JSON output")?;

    let category = v["category"]
        .as_str()
        .unwrap_or("Unclassified")
        .to_string();
    let confidence = v["confidence"].as_u64().unwrap_or(0) as u8;
    let reasoning = v["reasoning"]
        .as_str()
        .unwrap_or("No reasoning provided.")
        .to_string();

    Ok(Classification {
        category,
        confidence,
        reasoning,
    })
}
