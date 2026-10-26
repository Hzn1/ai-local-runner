//! Bridges the transport-neutral core events to Tauri webview events.

use std::sync::Arc;

use tauri::{AppHandle, Emitter};
use tnn_core::EventSink;
use tnn_core::events::CoreEvent;

/// Build a sink that forwards each core event to the webview under the name
/// defined by [`CoreEvent::name`].
///
/// Tauri's `emit` is synchronous and non-blocking, so this is safe to call from
/// the engine supervisor and the chat streaming task.
pub fn tauri_sink(app: AppHandle) -> EventSink {
    EventSink::new(Arc::new(move |event: CoreEvent| {
        let name = event.name();
        if let Err(error) = app.emit(name, event.payload()) {
            // Never propagate: a closed window must not break the core.
            tracing::debug!(%error, event = name, "could not emit Tauri event");
        }
    }))
}
