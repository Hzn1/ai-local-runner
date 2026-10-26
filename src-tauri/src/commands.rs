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
