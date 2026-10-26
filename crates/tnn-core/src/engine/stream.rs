//! Streaming chat client: SSE parsing, 50 ms batching, cancellation.

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::Client;
use tokio_util::sync::CancellationToken;

use crate::chat::sse::{SseParser, parse_chunk};
use crate::chat::{ChatCompletionRequest, estimate_tokens};
use crate::errors::{AppError, Result};
use crate::events::{ChatDelta, CoreEvent, EventSink, Usage};

#[derive(Debug, Clone, Default)]
pub struct ChatOutcome {
    pub content: String,
    pub reasoning: String,
    pub usage: Usage,
    pub elapsed_ms: u64,
    pub tokens_per_sec: f32,
    pub cancelled: bool,
    pub truncated: bool,
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
    stream_chat_with_idle_timeout(
        client,
        base_url,
        api_key,
        request,
        request_id,
        cancel,
        events,
        Duration::from_secs(180),
    )
    .await
}

async fn stream_chat_with_idle_timeout(
    client: &Client,
    base_url: &str,
    api_key: &str,
    request: &ChatCompletionRequest,
    request_id: &str,
    cancel: CancellationToken,
    events: &EventSink,
    idle_timeout: Duration,
) -> Result<ChatOutcome> {
    let started = Instant::now();
    let deadline = tokio::time::sleep(idle_timeout);
    tokio::pin!(deadline);
    let pending_response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .bearer_auth(api_key)
        .json(request)
        .send();
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            return Ok(ChatOutcome {
                elapsed_ms: started.elapsed().as_millis() as u64,
                cancelled: true,
                ..ChatOutcome::default()
            });
        }
        response = pending_response => {
            response.map_err(|error| AppError::EngineCrash {
                detail: format!("chat request failed: {error}"),
            })?
        }
    };

    if !response.status().is_success() {
        let status = response.status().as_u16();
        deadline
            .as_mut()
            .reset(tokio::time::Instant::now() + idle_timeout);
        let body = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                return Ok(ChatOutcome {
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    cancelled: true,
                    ..ChatOutcome::default()
                });
            }
            _ = &mut deadline => return Err(stalled_engine()),
            body = response.text() => body.unwrap_or_default(),
        };
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
    // Prompt evaluation on baseline CPUs can take many minutes. Silence
    // before the first token does not establish that the engine has stalled.
    let mut received_output = false;

    'stream: loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                cancelled = true;
                break 'stream;
            }
            _ = &mut deadline, if received_output => {
                accumulator.flush(events, request_id);
                return Err(stalled_engine());
            }
            _ = ticker.tick() => {
                accumulator.flush(events, request_id);
            }
            item = stream.next() => {
                match item {
                    Some(Ok(bytes)) => {
                        for event in parser.push(&bytes) {
                            let before = accumulator.content.len() + accumulator.reasoning.len();
                            if accumulator.consume(&event.data)? {
                                saw_done = true;
                                break 'stream;
                            }
                            // Keep-alives and empty deltas must not keep a
                            // stalled generation alive indefinitely.
                            if accumulator.content.len() + accumulator.reasoning.len() > before {
                                received_output = true;
                                deadline.as_mut().reset(tokio::time::Instant::now() + idle_timeout);
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
            saw_done |= accumulator.consume(&event.data)?;
        }
    }
    accumulator.flush(events, request_id);

    let elapsed = started.elapsed();
    let elapsed_ms = elapsed.as_millis() as u64;
    let mut usage = accumulator.usage.unwrap_or_default();
    if usage.completion_tokens == 0 && !accumulator.content.is_empty() {
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
        && !(request.continue_final_message == Some(true)
            && accumulator.finish_reason.as_deref() == Some("stop"))
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
        truncated: accumulator.finish_reason.as_deref() == Some("length")
            || (!saw_done && !cancelled),
    })
}

fn stalled_engine() -> AppError {
    AppError::EngineCrash {
        detail: "the engine stopped responding; restart it and try again".into(),
    }
}

#[derive(Default)]
struct Accumulator {
    content: String,
    reasoning: String,
    usage: Option<Usage>,
    pending_content: String,
    pending_reasoning: String,
    index: u32,
    finish_reason: Option<String>,
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
        if parsed.finish_reason.is_some() {
            self.finish_reason = parsed.finish_reason;
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

    async fn stalled_server(
        status: Option<u16>,
        output: bool,
    ) -> (String, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            socket.read(&mut buffer).await.unwrap();
            if let Some(status) = status {
                socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n").as_bytes()).await.unwrap();
                if output {
                    let data = "data: {\"choices\":[{\"delta\":{\"content\":\"New text\"}}]}\n\n";
                    socket
                        .write_all(format!("{:x}\r\n{data}\r\n", data.len()).as_bytes())
                        .await
                        .unwrap();
                }
                // Keep-alive SSE comments must not defeat the idle limit.
                loop {
                    if socket.write_all(b"8\r\n: ping\n\n\r\n").await.is_err() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            } else {
                std::future::pending::<()>().await;
            }
        });
        (url, task)
    }

    fn continuation_request() -> ChatCompletionRequest {
        ChatCompletionRequest::new(
            vec![crate::chat::ChatMessage {
                role: "assistant".into(),
                content: "The unfinished answer".into(),
            }],
            32,
            &crate::models::Sampling::default(),
            false,
        )
    }

    #[tokio::test]
    async fn continuation_can_be_cancelled_before_response_headers() {
        let (url, server) = stalled_server(None, false).await;
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let canceller = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            token.cancel();
        });
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            stream_chat(
                &Client::new(),
                &url,
                "test",
                &continuation_request(),
                "continue",
                cancel,
                &EventSink::noop(),
            ),
        )
        .await;
        server.abort();
        canceller.await.unwrap();
        let outcome = result
            .expect("Stop must interrupt the pending request")
            .unwrap();
        assert!(outcome.cancelled);
        assert!(outcome.content.is_empty());
    }

    #[tokio::test]
    async fn timeout_applies_after_output_or_while_reading_an_http_error() {
        for status in [Some(200), Some(500)] {
            let (url, server) = stalled_server(status, true).await;
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                stream_chat_with_idle_timeout(
                    &Client::new(),
                    &url,
                    "test",
                    &continuation_request(),
                    "continue",
                    CancellationToken::new(),
                    &EventSink::noop(),
                    Duration::from_millis(50),
                ),
            )
            .await;
            server.abort();
            let error = result.expect("stalled requests must end").unwrap_err();
            assert_eq!(error.code(), "engine_crash");
            assert!(error.to_string().contains("stopped responding"));
        }
    }

    #[tokio::test]
    async fn slow_prompt_processing_can_outlast_the_stream_idle_limit() {
        for status in [None, Some(200)] {
            let (url, server) = stalled_server(status, false).await;
            let cancel = CancellationToken::new();
            let token = cancel.clone();
            let canceller = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(150)).await;
                token.cancel();
            });
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                stream_chat_with_idle_timeout(
                    &Client::new(),
                    &url,
                    "test",
                    &continuation_request(),
                    "continue",
                    cancel,
                    &EventSink::noop(),
                    Duration::from_millis(50),
                ),
            )
            .await;
            server.abort();
            canceller.await.unwrap();
            let outcome = result
                .expect("Stop must still work")
                .expect("slow preparation must not time out");
            assert!(outcome.cancelled);
            assert!(outcome.content.is_empty());
        }
    }

    #[tokio::test]
    async fn response_after_slow_prompt_processing_still_completes() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(150))
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string("data: {\"choices\":[{\"delta\":{\"content\":\"Continued answer\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"),
            )
            .mount(&server)
            .await;
        let outcome = stream_chat_with_idle_timeout(
            &Client::new(),
            &server.uri(),
            "test",
            &continuation_request(),
            "continue",
            CancellationToken::new(),
            &EventSink::noop(),
            Duration::from_millis(50),
        )
        .await
        .expect("a slow first response must succeed");
        assert_eq!(outcome.content, "Continued answer");
        assert!(!outcome.cancelled);
        assert!(!outcome.truncated);
    }

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
