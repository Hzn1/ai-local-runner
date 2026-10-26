//! Incremental Server-Sent Events parser.
//!
//! Byte chunks are decoded only after a complete line has arrived, so a UTF-8
//! character split across TCP reads is never corrupted.

use serde_json::Value;

use crate::events::Usage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    pending: Vec<u8>,
    data_lines: Vec<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed bytes and return every complete event.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        let mut events = Vec::new();
        self.pending.extend_from_slice(chunk);
        while let Some(index) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=index).collect();
            let line = &line[..line.len() - 1];
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            self.handle_line(line, &mut events);
        }
        events
    }

    /// Flush a final unterminated line, e.g. if the connection closed abruptly.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            let line = line.strip_suffix(b"\r").unwrap_or(&line).to_vec();
            self.handle_line(&line, &mut events);
        }
        if !self.data_lines.is_empty() {
            events.push(SseEvent {
                data: std::mem::take(&mut self.data_lines).join("\n"),
            });
        }
        events
    }

    fn handle_line(&mut self, line: &[u8], events: &mut Vec<SseEvent>) {
        let Ok(line) = std::str::from_utf8(line) else {
            return;
        };
        if line.is_empty() {
            if !self.data_lines.is_empty() {
                events.push(SseEvent {
                    data: std::mem::take(&mut self.data_lines).join("\n"),
                });
            }
            return;
        }
        if line.starts_with(':') {
            return;
        }
        if let Some(data) = line.strip_prefix("data:") {
            self.data_lines
                .push(data.strip_prefix(' ').unwrap_or(data).to_string());
        }
        // `event:` and `id:` are not used by llama-server.
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParsedChunk {
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub usage: Option<Usage>,
    pub done: bool,
    pub finish_reason: Option<String>,
}

/// Parse one `data:` payload. Returns `Ok(None)` for keep-alive or unknown
/// events, and an error for malformed JSON so the caller can log and skip.
pub fn parse_chunk(data: &str) -> Result<Option<ParsedChunk>, serde_json::Error> {
    if data.trim().is_empty() {
        return Ok(None);
    }
    if data.trim() == "[DONE]" {
        return Ok(Some(ParsedChunk {
            done: true,
            ..Default::default()
        }));
    }
    let value: Value = serde_json::from_str(data)?;
    let mut parsed = ParsedChunk::default();

    if let Some(choices) = value.get("choices").and_then(Value::as_array) {
        if let Some(choice) = choices.first() {
            if let Some(delta) = choice.get("delta") {
                parsed.content = content_of(delta.get("content"));
                parsed.reasoning = content_of(delta.get("reasoning_content"))
                    .or_else(|| content_of(delta.get("reasoning")));
            }
            if let Some(message) = choice.get("message") {
                parsed.content = parsed.content.or(content_of(message.get("content")));
                parsed.reasoning = parsed
                    .reasoning
                    .or_else(|| content_of(message.get("reasoning_content")));
            }
            parsed.finish_reason = choice
                .get("finish_reason")
                .and_then(Value::as_str)
                .filter(|reason| !reason.is_empty())
                .map(str::to_owned);
        }
    }

    if let Some(usage) = value.get("usage").filter(|usage| !usage.is_null()) {
        parsed.usage = Some(Usage {
            prompt_tokens: number(usage.get("prompt_tokens")),
            completion_tokens: number(usage.get("completion_tokens")),
            total_tokens: number(usage.get("total_tokens")),
        });
    } else if let Some(timings) = value.get("timings") {
        let prompt = number(timings.get("prompt_n"));
        let predicted = number(timings.get("predicted_n"));
        if prompt > 0 || predicted > 0 {
            parsed.usage = Some(Usage {
                prompt_tokens: prompt,
                completion_tokens: predicted,
                total_tokens: prompt + predicted,
            });
        }
    }

    Ok(Some(parsed))
}

fn content_of(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => (!text.is_empty()).then(|| text.clone()),
        Value::Array(parts) => {
            let text: String = parts
                .iter()
                .filter_map(|part| {
                    part.get("text")
                        .and_then(Value::as_str)
                        .or_else(|| part.as_str())
                })
                .collect();
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

fn number(value: Option<&Value>) -> u32 {
    value
        .and_then(Value::as_u64)
        .map(|value| value.min(u32::MAX as u64) as u32)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_multiline_data_and_done() {
        let mut parser = SseParser::new();
        let events = parser.push(b"data: {\"a\":1}\n\ndata: [DONE]\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "{\"a\":1}");
        assert_eq!(events[1].data, "[DONE]");
    }

    #[test]
    fn joins_multiple_data_lines() {
        let mut parser = SseParser::new();
        let events = parser.push(b"data: one\ndata: two\n\n");
        assert_eq!(events[0].data, "one\ntwo");
    }

    #[test]
    fn survives_split_utf8_and_crlf() {
        let text = "data: {\"content\":\"olá\"}\r\n\r\n";
        let bytes = text.as_bytes();
        let mut parser = SseParser::new();
        let mut events = Vec::new();
        for chunk in bytes.chunks(3) {
            events.extend(parser.push(chunk));
        }
        assert_eq!(events.len(), 1);
        assert!(events[0].data.contains("olá"));
    }

    #[test]
    fn ignores_comments_and_keepalives() {
        let mut parser = SseParser::new();
        assert!(parser.push(b": ping\n\n").is_empty());
    }

    #[test]
    fn flushes_unterminated_event() {
        let mut parser = SseParser::new();
        assert!(parser.push(b"data: {\"x\":1}").is_empty());
        assert_eq!(parser.finish()[0].data, "{\"x\":1}");
    }

    #[test]
    fn parses_content_reasoning_and_usage() {
        let chunk = parse_chunk(
            r#"{"choices":[{"delta":{"content":"hi","reasoning_content":"think"}}],
                "usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4}}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(chunk.content.as_deref(), Some("hi"));
        assert_eq!(chunk.reasoning.as_deref(), Some("think"));
        assert_eq!(chunk.usage.unwrap().total_tokens, 4);
    }

    #[test]
    fn parses_timings_fallback() {
        let chunk = parse_chunk(r#"{"choices":[],"timings":{"prompt_n":10,"predicted_n":5}}"#)
            .unwrap()
            .unwrap();
        assert_eq!(chunk.usage.unwrap().prompt_tokens, 10);
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_chunk("{not json").is_err());
    }

    #[test]
    fn empty_and_done_payloads() {
        assert!(parse_chunk("").unwrap().is_none());
        assert!(parse_chunk("[DONE]").unwrap().unwrap().done);
    }
}
