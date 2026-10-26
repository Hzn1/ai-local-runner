//! `tnn-core` — the UI-independent heart of TNN Local AI.
//!
//! Design rules enforced here:
//! * no Tauri types, so everything is testable with plain `cargo test`;
//! * the engine is a supervised child process on loopback with a per-session
//!   key; the frontend never talks to it;
//! * no AVX2 assumption anywhere;
//! * thinking is disabled by default;
//! * SQLite is the single source of truth for chats, models and settings.

pub mod app;
pub mod chat;
pub mod diagnostics;
pub mod engine;
pub mod errors;
pub mod events;
pub mod hardware;
pub mod models;
pub mod paths;
pub mod storage;

/// Pinned PrismML llama.cpp release. Changing this requires repeating the
/// compatibility spike and updating `engine.lock.json` and the scripts.
pub const ENGINE_VERSION: &str = "prism-b10826-c9c34bc";
pub const ENGINE_REPO: &str = "https://github.com/PrismML-Eng/llama.cpp";
pub const CATALOG_VERSION: &str = "2026-10-25";

pub use app::{AppCore, BootstrapState, SendMessageRequest, SendMessageResponse, StorageSummary};
pub use errors::{AppError, ErrorPayload, RecoveryAction, Result};
pub use events::{
    ChatCancelled, ChatDelta, ChatDone, ChatErrorEvent, CoreEvent, DownloadProgress, DownloadState,
    DownloadStateEvent, EngineState, EngineStateEvent, EventSink, Usage,
};
pub use hardware::{CpuFeatures, HardwareProfile, HardwareTier};
pub use models::{Catalog, ModelCard, ModelSpec, ModelTier, Sampling, ValidationStatus};
pub use paths::Paths;
pub use storage::{
    Conversation, ConversationSummary, InstalledModel, Message, MessageRole, MessageStatus,
    ModelSource, Settings,
};
