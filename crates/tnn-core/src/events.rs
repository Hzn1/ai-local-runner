//! Core events and the transport-neutral event sink.
//!
//! The Tauri layer subscribes with a closure that forwards each event to the
//! webview using the event names in [`CoreEvent::name`]. Tests can swap in a
//! channel-backed sink.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::ErrorPayload;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DownloadState {
    Queued,
    Downloading,
    Paused,
    Verifying,
    Installed,
    Failed,
    Cancelled,
}

impl DownloadState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Verifying => "verifying",
            Self::Installed => "installed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "queued" => Self::Queued,
            "paused" => Self::Paused,
            "verifying" => Self::Verifying,
            "installed" => Self::Installed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Downloading,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DownloadProgress {
    pub model_id: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub speed_bps: u64,
    pub eta_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DownloadStateEvent {
    pub model_id: String,
    pub state: DownloadState,
    pub error: Option<ErrorPayload>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EngineState {
    Stopped,
    Starting,
    Loading,
    Ready,
    Unloading,
    Crashed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineStateEvent {
    pub state: EngineState,
    pub model_id: Option<String>,
    pub tier: Option<String>,
    pub compatibility_mode: bool,
    pub error: Option<ErrorPayload>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChatDelta {
    pub request_id: String,
    pub delta: String,
    pub reasoning_delta: Option<String>,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChatDone {
    pub request_id: String,
    pub usage: Usage,
    pub elapsed_ms: u64,
    pub tokens_per_sec: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChatErrorEvent {
    pub request_id: String,
    pub error: ErrorPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChatCancelled {
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchmarkProgress {
    pub model_id: String,
    pub stage: String,
    pub percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateAvailable {
    pub version: String,
    pub notes: Option<String>,
}

/// Transport-neutral event.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "payload")]
pub enum CoreEvent {
    DownloadProgress(DownloadProgress),
    DownloadState(DownloadStateEvent),
    EngineState(EngineStateEvent),
    ChatDelta(ChatDelta),
    ChatDone(ChatDone),
    ChatError(ChatErrorEvent),
    ChatCancelled(ChatCancelled),
    BenchmarkProgress(BenchmarkProgress),
    UpdateAvailable(UpdateAvailable),
}

impl CoreEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::DownloadProgress(_) => "download://progress",
            Self::DownloadState(_) => "download://state",
            Self::EngineState(_) => "engine://state",
            Self::ChatDelta(_) => "chat://delta",
            Self::ChatDone(_) => "chat://done",
            Self::ChatError(_) => "chat://error",
            Self::ChatCancelled(_) => "chat://cancelled",
            Self::BenchmarkProgress(_) => "benchmark://progress",
            Self::UpdateAvailable(_) => "update://available",
        }
    }

    /// Serialized body emitted under [`CoreEvent::name`]. This is what the
    /// Tauri layer forwards to the webview, without the redundant tag wrapper.
    pub fn payload(&self) -> serde_json::Value {
        let value = match self {
            Self::DownloadProgress(payload) => serde_json::to_value(payload),
            Self::DownloadState(payload) => serde_json::to_value(payload),
            Self::EngineState(payload) => serde_json::to_value(payload),
            Self::ChatDelta(payload) => serde_json::to_value(payload),
            Self::ChatDone(payload) => serde_json::to_value(payload),
            Self::ChatError(payload) => serde_json::to_value(payload),
            Self::ChatCancelled(payload) => serde_json::to_value(payload),
            Self::BenchmarkProgress(payload) => serde_json::to_value(payload),
            Self::UpdateAvailable(payload) => serde_json::to_value(payload),
        };
        value.unwrap_or(serde_json::Value::Null)
    }
}

/// A callback invoked for every core event.
pub type EventFn = Arc<dyn Fn(CoreEvent) + Send + Sync>;

pub fn noop_sink() -> EventFn {
    Arc::new(|_| {})
}

/// Convenience wrapper that swallows panics in user callbacks so a UI bug can
/// never take down the engine supervisor.
#[derive(Clone)]
pub struct EventSink(EventFn);

impl EventSink {
    pub fn new(f: EventFn) -> Self {
        Self(f)
    }

    pub fn noop() -> Self {
        Self(noop_sink())
    }

    pub fn emit(&self, event: CoreEvent) {
        let f = self.0.clone();
        // Event callbacks must be quick (Tauri emits are non-blocking). This is
        // deliberately not spawned: ordering matters for chat deltas.
        f(event);
    }
}

impl Default for EventSink {
    fn default() -> Self {
        Self::noop()
    }
}

impl std::fmt::Debug for EventSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EventSink")
    }
}
