//! Streaming chat client: SSE parsing, 50 ms batching, cancellation.

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::Client;
use tokio_util::sync::CancellationToken;

use crate::chat::sse::{SseParser, parse_chunk};
use crate::chat::{ChatCompletionRequest, estimate_tokens};
use crate::errors::{AppError, Result};
use crate::events::{ChatDelta, CoreEvent, EventSink, Usage};

#[derive(Debug, Clone)]
pub struct ChatOutcome {
    pub content: String,
    pub reasoning: String,
    pub usage: Usage,
    pub elapsed_ms: u64,
    pub tokens_per_sec: f32,
    pub cancelled: bool,
}

/// POST a streaming completion and emit batched `chat://delta` events.
///
/// Cancellation is not an error: the partial answer is returned with
/// `cancelled = true` so the caller can persist it as interrupted.
pub async fn stream_chat(
    client: &Client,
    base_url: &str,
    api_key: &str,
    request: &ChatCompletionRequest,
    request_id: &str,
    cancel: CancellationToken,
    events: &EventSink,
) -> Result<ChatOutcome> {
    let started = Instant::now();
    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .bearer_auth(api_key)
        .json(request)
        .send()
        .await
        .map_err(|error| AppError::EngineCrash {
            detail: format!("chat request failed: {error}"),
        })?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(map_http_error(status, &body));
    }

    let mut parser = SseParser::new();
    let mut accumulator = Accumulator::default();
    let mut stream = response.bytes_stream();
    let mut ticker = tokio::time::interval(Duration::from_millis(50));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticker.tick().await;
    let mut cancelled = false;
    let mut saw_done = false;

    'stream: loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                cancelled = true;
                break 'stream;
            }
            _ = ticker.tick() => {
                accumulator.flush(events, request_id);
            }
            item = stream.next() => {
                match item {
                    Some(Ok(bytes)) => {
                        for event in parser.push(&bytes) {
                            if accumulator.consume(&event.data)? {
                                saw_done = true;
                                break 'stream;
                            }
                        }
                    }
                    Some(Err(error)) => {
                        accumulator.flush(events, request_id);
                        return Err(AppError::EngineCrash {
                            detail: format!("stream interrupted: {error}"),
                        });
                    }
                    None => break 'stream,
                }
            }
        }
    }

    if !saw_done {
        for event in parser.finish() {
            let _ = accumulator.consume(&event.data)?;
        }
    }
    accumulator.flush(events, request_id);

    let elapsed = started.elapsed();
    let elapsed_ms = elapsed.as_millis() as u64;
    let mut usage = accumulator.usage.unwrap_or_default();
    if usage.completion_tokens == 0 {
        usage.completion_tokens = estimate_tokens(&accumulator.content);
    }
    if usage.prompt_tokens == 0 {
        usage.prompt_tokens = request
            .messages
            .iter()
            .map(|message| estimate_tokens(&message.content).saturating_add(4))
            .sum();
    }
    if usage.total_tokens == 0 {
        usage.total_tokens = usage.prompt_tokens.saturating_add(usage.completion_tokens);
    }
    let tokens_per_sec = if elapsed.as_secs_f32() > 0.0 {
        usage.completion_tokens as f32 / elapsed.as_secs_f32()
    } else {
        0.0
    };

    if !cancelled
        && accumulator.content.trim().is_empty()
        && accumulator.reasoning.trim().is_empty()
    {
        return Err(AppError::EngineGarbageOutput);
    }

    Ok(ChatOutcome {
        content: accumulator.content,
        reasoning: accumulator.reasoning,
        usage,
        elapsed_ms,
        tokens_per_sec,
        cancelled,
    })
}

#[derive(Default)]
struct Accumulator {
    content: String,
    reasoning: String,
    usage: Option<Usage>,
    pending_content: String,
    pending_reasoning: String,
    index: u32,
}

impl Accumulator {
    /// Returns true when the `[DONE]` sentinel was seen.
    fn consume(&mut self, data: &str) -> Result<bool> {
        let parsed = match parse_chunk(data) {
            Ok(Some(parsed)) => parsed,
            Ok(None) => return Ok(false),
            Err(error) => {
                // A single malformed chunk must not abort an otherwise good
                // answer; engine logs carry the detail.
                tracing::debug!(%error, "ignoring malformed SSE chunk");
                return Ok(false);
            }
        };

        if let Some(content) = parsed.content {
            self.content.push_str(&content);
            self.pending_content.push_str(&content);
        }
        if let Some(reasoning) = parsed.reasoning {
            self.reasoning.push_str(&reasoning);
            self.pending_reasoning.push_str(&reasoning);
        }
        if let Some(usage) = parsed.usage {
            self.usage = Some(usage);
        }
        Ok(parsed.done)
    }

    fn flush(&mut self, events: &EventSink, request_id: &str) {
        if self.pending_content.is_empty() && self.pending_reasoning.is_empty() {
            return;
        }
        events.emit(CoreEvent::ChatDelta(ChatDelta {
            request_id: request_id.to_string(),
            delta: std::mem::take(&mut self.pending_content),
            reasoning_delta: {
                let value = std::mem::take(&mut self.pending_reasoning);
                (!value.is_empty()).then_some(value)
            },
            index: self.index,
        }));
        self.index += 1;
    }

    #[allow(dead_code)]
    fn pending_len(&self) -> usize {
        self.pending_content.len() + self.pending_reasoning.len()
    }
}

pub(crate) fn map_http_error(status: u16, body: &str) -> AppError {
    let lower = body.to_ascii_lowercase();
    if lower.contains("out of memory")
        || lower.contains("failed to allocate")
        || lower.contains("oom")
    {
        return AppError::OutOfMemory;
    }
    if lower.contains("context")
        && (lower.contains("exceed")
            || lower.contains("too long")
            || lower.contains("overflow")
            || lower.contains("n_tokens"))
    {
        return AppError::ContextOverflow;
    }
    if status == 401 || status == 403 {
        return AppError::EngineSpawnFailed {
            detail: "the engine rejected the session key".into(),
        };
    }
    AppError::EngineCrash {
        detail: format!(
            "engine returned HTTP {status}: {}",
            body.chars().take(400).collect::<String>()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_out_of_memory() {
        assert!(matches!(
            map_http_error(500, "failed to allocate KV cache"),
            AppError::OutOfMemory
        ));
    }

    #[test]
    fn maps_context_overflow() {
        assert!(matches!(
            map_http_error(400, "the prompt exceeds the context window"),
            AppError::ContextOverflow
        ));
    }

    #[test]
    fn maps_rejected_key() {
        assert!(matches!(
            map_http_error(401, "invalid api key"),
            AppError::EngineSpawnFailed { .. }
        ));
    }

    #[test]
    fn keeps_other_failures_as_engine_crashes() {
        assert!(matches!(
            map_http_error(500, "busy"),
            AppError::EngineCrash { .. }
        ));
    }
}
