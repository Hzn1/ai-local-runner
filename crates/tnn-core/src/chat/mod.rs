//! Chat domain: context construction, SSE parsing and request shapes.

pub mod context;
pub mod sse;

pub use context::{ChatMessage, build_context, estimate_tokens};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::catalog::Sampling;

/// A message in the wire format accepted by llama-server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct WireMessage {
    pub role: String,
    pub content: String,
}

impl From<ChatMessage> for WireMessage {
    fn from(message: ChatMessage) -> Self {
        Self {
            role: message.role,
            content: message.content,
        }
    }
}

/// OpenAI-compatible streaming request body.
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<WireMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    pub min_p: f32,
    pub presence_penalty: f32,
    pub repetition_penalty: f32,
    pub repeat_last_n: u32,
    pub chat_template_kwargs: ChatTemplateKwargs,
    pub cache_prompt: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatTemplateKwargs {
    pub enable_thinking: bool,
}

impl ChatCompletionRequest {
    pub fn new(
        messages: Vec<ChatMessage>,
        max_tokens: u32,
        sampling: &Sampling,
        enable_thinking: bool,
    ) -> Self {
        Self {
            model: "local".to_string(),
            messages: messages.into_iter().map(Into::into).collect(),
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
            max_tokens,
            temperature: sampling.temperature,
            top_p: sampling.top_p,
            top_k: sampling.top_k,
            min_p: sampling.min_p,
            presence_penalty: sampling.presence_penalty,
            repetition_penalty: sampling.repeat_penalty,
            repeat_last_n: sampling.repeat_last_n,
            chat_template_kwargs: ChatTemplateKwargs { enable_thinking },
            cache_prompt: true,
        }
    }
}
