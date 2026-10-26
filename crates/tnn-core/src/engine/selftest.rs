//! Post-load self-test and garbage-output detection.
//!
//! This exists to catch the bitnet.cpp class of failure: a process that starts
//! and answers, but emits nonsense on CPUs without AVX2.

use std::collections::HashSet;
use std::time::Duration;

use reqwest::Client;
use serde_json::json;

use crate::errors::{AppError, Result};

pub async fn run_selftest(client: &Client, base_url: &str, api_key: &str) -> Result<()> {
    let body = json!({
        "model": "local",
        "messages": [{"role": "user", "content": "Reply with one short friendly greeting."}],
        "stream": false,
        "max_tokens": 12,
        "temperature": 0.0,
        "top_k": 1,
        "chat_template_kwargs": {"enable_thinking": false},
    });

    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .bearer_auth(api_key)
        .json(&body)
        .timeout(Duration::from_secs(90))
        .send()
        .await
        .map_err(|error| AppError::EngineCrash {
            detail: format!("self-test request failed: {error}"),
        })?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(map_selftest_error(status.as_u16(), &text));
    }

    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| AppError::EngineGarbageOutput)?;

    let content = value
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            value
                .pointer("/choices/0/text")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or_default();

    if looks_degenerate(content) {
        return Err(AppError::EngineGarbageOutput);
    }
    Ok(())
}

/// Heuristics: an answer must contain letters, not be a single repeated glyph,
/// and not contain an absurd run of one character.
pub fn looks_degenerate(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    let letters = trimmed.chars().filter(|ch| ch.is_alphabetic()).count();
    if letters < 2 {
        return true;
    }
    let unique: HashSet<char> = trimmed.chars().filter(|ch| !ch.is_whitespace()).collect();
    if unique.len() <= 1 {
        return true;
    }

    let mut run = 1_usize;
    let mut previous = None;
    for ch in trimmed.chars() {
        if Some(ch) == previous {
            run += 1;
            if run > 40 {
                return true;
            }
        } else {
            run = 1;
            previous = Some(ch);
        }
    }
    false
}

fn map_selftest_error(status: u16, body: &str) -> AppError {
    let lower = body.to_ascii_lowercase();
    if lower.contains("out of memory")
        || lower.contains("failed to allocate")
        || lower.contains("allocation")
    {
        return AppError::OutOfMemory;
    }
    AppError::EngineSpawnFailed {
        detail: format!("self-test returned HTTP {status}: {}", truncate(body, 400)),
    }
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_answers() {
        assert!(!looks_degenerate("Hello! How can I help?"));
        assert!(!looks_degenerate("Sim, posso ajudar."));
    }

    #[test]
    fn rejects_garbage() {
        assert!(looks_degenerate(""));
        assert!(looks_degenerate("!!!!!!!!"));
        assert!(looks_degenerate("a"));
        assert!(looks_degenerate(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
        assert!(looks_degenerate("..."));
    }
}
