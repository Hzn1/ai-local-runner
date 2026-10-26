//! Tauri IPC commands. This layer is deliberately thin: validate shape, call
//! `tnn-core`, map typed errors. No business logic lives here.

use std::path::PathBuf;

use serde_json::Value;
use tauri::State;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;
use tnn_core::engine::{BenchmarkResult, EngineInfo, EngineStatus};
use tnn_core::{
    AppError, BootstrapState, ErrorPayload, HardwareProfile, InstalledModel, ModelCard,
    SendMessageRequest, SendMessageResponse, Settings, StorageSummary,
};

use crate::state::AppState;

type CmdResult<T> = Result<T, ErrorPayload>;

fn other(detail: impl Into<String>) -> ErrorPayload {
    ErrorPayload::from(AppError::Other {
        detail: detail.into(),
    })
}

// ------------------------------------------------------------------ startup

#[tauri::command]
pub async fn get_bootstrap_state(state: State<'_, AppState>) -> CmdResult<BootstrapState> {
    Ok(state.core.bootstrap().await?)
}

#[tauri::command]
pub fn scan_hardware(state: State<'_, AppState>) -> CmdResult<HardwareProfile> {
    Ok(state.core.scan_hardware()?)
}

// ------------------------------------------------------------------- models

#[tauri::command]
pub fn list_models(state: State<'_, AppState>) -> CmdResult<Vec<ModelCard>> {
    let mut cards = state.core.model_cards()?;
    // Include user-imported models alongside catalog entries.
    for installed in state.core.db.installed_models()? {
        if !cards.iter().any(|card| card.model.id == installed.id) {
            cards.push(state.core.installed_card(&installed));
        }
    }
    Ok(cards)
}

#[tauri::command]
pub async fn download_model(state: State<'_, AppState>, model_id: String) -> CmdResult<()> {
    Ok(state.core.download_model(&model_id).await?)
}

#[tauri::command]
pub fn pause_download(state: State<'_, AppState>, model_id: String) -> CmdResult<()> {
    Ok(state.core.pause_download(&model_id)?)
}

#[tauri::command]
pub async fn resume_download(state: State<'_, AppState>, model_id: String) -> CmdResult<()> {
    Ok(state.core.resume_download(&model_id).await?)
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, model_id: String) -> CmdResult<()> {
    Ok(state.core.cancel_download(&model_id)?)
}

#[tauri::command]
pub async fn delete_model(state: State<'_, AppState>, model_id: String) -> CmdResult<()> {
    Ok(state.core.delete_model(&model_id).await?)
}

#[tauri::command]
pub async fn import_model(state: State<'_, AppState>, path: String) -> CmdResult<ModelCard> {
    let installed: InstalledModel = state.core.import_model(PathBuf::from(path)).await?;
    Ok(state.core.installed_card(&installed))
}

#[tauri::command]
pub fn get_storage_summary(state: State<'_, AppState>) -> CmdResult<StorageSummary> {
    Ok(state.core.storage_summary()?)
}

// ------------------------------------------------------------------- engine

#[tauri::command]
pub async fn start_engine(state: State<'_, AppState>, model_id: String) -> CmdResult<EngineInfo> {
    Ok(state.core.start_engine(&model_id).await?)
}

#[tauri::command]
pub async fn stop_engine(state: State<'_, AppState>) -> CmdResult<()> {
    state.core.stop_engine().await;
    Ok(())
}

#[tauri::command]
pub async fn engine_status(state: State<'_, AppState>) -> CmdResult<EngineStatus> {
    Ok(state.core.engine_status().await)
}

// --------------------------------------------------------------------- chat

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    request: SendMessageRequest,
) -> CmdResult<SendMessageResponse> {
    Ok(state.core.send_message(
        &request.conversation_id,
        &request.content,
        &request.request_id,
    )?)
}

#[tauri::command]
// The core spawns a generation task, so dispatch on Tauri's async runtime.
pub async fn continue_response(
    state: State<'_, AppState>,
    conversation_id: String,
    request_id: String,
) -> CmdResult<SendMessageResponse> {
    Ok(state
        .core
        .continue_response(&conversation_id, &request_id)?)
}

#[tauri::command]
pub fn cancel_generation(state: State<'_, AppState>, request_id: String) -> CmdResult<()> {
    state.core.cancel_generation(&request_id);
    Ok(())
}

#[tauri::command]
pub async fn regenerate(
    state: State<'_, AppState>,
    conversation_id: String,
    request_id: String,
) -> CmdResult<SendMessageResponse> {
    Ok(state.core.regenerate(&conversation_id, &request_id)?)
}

#[tauri::command]
pub fn list_conversations(
    state: State<'_, AppState>,
) -> CmdResult<Vec<tnn_core::ConversationSummary>> {
    Ok(state.core.list_conversations()?)
}

#[tauri::command]
pub fn get_conversation(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<tnn_core::Conversation> {
    Ok(state.core.get_conversation(&id)?)
}

#[tauri::command]
pub fn create_conversation(
    state: State<'_, AppState>,
    model_id: Option<String>,
) -> CmdResult<tnn_core::Conversation> {
    Ok(state.core.create_conversation(model_id)?)
}

#[tauri::command]
pub fn rename_conversation(state: State<'_, AppState>, id: String, title: String) -> CmdResult<()> {
    Ok(state.core.rename_conversation(&id, &title)?)
}

#[tauri::command]
pub fn delete_conversation(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.core.delete_conversation(&id)?)
}

#[tauri::command]
pub fn export_conversation(
    state: State<'_, AppState>,
    id: String,
    format: String,
) -> CmdResult<String> {
    let path = state.core.export_conversation(&id, &format)?;
    Ok(path.to_string_lossy().to_string())
}

// ----------------------------------------------------------------- settings

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.core.settings()?)
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, patch: Value) -> CmdResult<Settings> {
    Ok(state.core.update_settings(&patch)?)
}

// -------------------------------------------------------------- diagnostics

#[tauri::command]
pub async fn run_benchmark(
    state: State<'_, AppState>,
    model_id: String,
) -> CmdResult<BenchmarkResult> {
    Ok(state.core.run_benchmark(&model_id).await?)
}

#[tauri::command]
pub async fn export_diagnostics(state: State<'_, AppState>) -> CmdResult<String> {
    let path = state.core.export_diagnostics(false).await?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_data_dir(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let path = state.core.app_data_dir();
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|error| other(error.to_string()))
}

#[tauri::command]
pub fn open_logs(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let path = state.core.logs_dir();
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|error| other(error.to_string()))
}

// ------------------------------------------------------------------ updater

#[tauri::command]
pub async fn check_for_updates(
    app: tauri::AppHandle,
) -> CmdResult<Option<tnn_core::events::UpdateAvailable>> {
    let updater = app.updater().map_err(|error| other(error.to_string()))?;
    let update = updater
        .check()
        .await
        .map_err(|error| other(error.to_string()))?;
    Ok(update.map(|update| tnn_core::events::UpdateAvailable {
        version: update.version.clone(),
        notes: update.body.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    use tnn_core::{AppCore, CoreEvent, EventSink, MessageRole, MessageStatus, ModelSource, Paths};

    #[test]
    fn continue_response_ipc_dispatches_without_a_runtime_on_the_calling_thread() {
        assert!(tokio::runtime::Handle::try_current().is_err());
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(
            dir.path().join("data"),
            dir.path().join("missing-engine"),
            dir.path().join("missing-baseline"),
        )
        .unwrap();
        let (sender, receiver) = mpsc::channel();
        let core = Arc::new(
            AppCore::new(
                paths,
                EventSink::new(Arc::new(move |event| {
                    let _ = sender.send(event);
                })),
                "0.1.0",
            )
            .unwrap(),
        );
        let model_path = dir.path().join("fake.gguf");
        std::fs::write(&model_path, b"GGUF").unwrap();
        core.db
            .upsert_installed(&InstalledModel {
                id: "bonsai2-2b-pq2".into(),
                display_name: "Test".into(),
                file_name: "fake.gguf".into(),
                path: model_path.to_string_lossy().into_owned(),
                sha256: "a".repeat(64),
                size_bytes: 4,
                installed_at: 0,
                last_used_at: None,
                source: ModelSource::Catalog,
            })
            .unwrap();
        let conversation = core.create_conversation(None).unwrap();
        for (role, content, status) in [
            (MessageRole::User, "Explain", MessageStatus::Complete),
            (
                MessageRole::Assistant,
                "First half",
                MessageStatus::Truncated,
            ),
        ] {
            core.db
                .append_message(&conversation.id, role, content, None, status, None)
                .unwrap();
        }
        let (_, log_guard) = tracing_appender::non_blocking(std::io::sink());
        let app = tauri::test::mock_builder()
            .manage(AppState {
                core: Arc::clone(&core),
                _log_guard: log_guard,
            })
            .invoke_handler(tauri::generate_handler![continue_response])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let response = tauri::test::get_ipc_response(
            &window,
            tauri::webview::InvokeRequest {
                cmd: "continue_response".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body: tauri::ipc::InvokeBody::Json(serde_json::json!({
                    "conversationId": conversation.id,
                    "requestId": "continue-ipc",
                })),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.into(),
            },
        )
        .unwrap()
        .deserialize::<SendMessageResponse>()
        .unwrap();
        assert_eq!(response.request_id, "continue-ipc");

        // No engine is installed: generation must report a recoverable error
        // through events instead of panicking at tokio::spawn in the handler.
        loop {
            if let CoreEvent::ChatError(event) =
                receiver.recv_timeout(Duration::from_secs(5)).unwrap()
            {
                assert_eq!(event.request_id, "continue-ipc");
                assert_eq!(event.error.code, "engine_spawn_failed");
                break;
            }
        }
        let loaded = core.get_conversation(&conversation.id).unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[1].id, response.assistant_message_id);
        assert_eq!(loaded.messages[1].content, "First half");
        assert_eq!(loaded.messages[1].status, MessageStatus::Truncated);
    }
}
