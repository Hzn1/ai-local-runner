//! Process-wide Tauri state. Everything here is cheap to clone or lives for
//! the whole process.

use std::sync::Arc;

use tnn_core::AppCore;
use tracing_appender::non_blocking::WorkerGuard;

pub struct AppState {
    pub core: Arc<AppCore>,
    /// Keeps the non-blocking log writer alive for the lifetime of the app.
    pub _log_guard: WorkerGuard,
}
