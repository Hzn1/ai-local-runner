//! UI-independent application core: the single place where storage, models,
//! engine supervision and chat orchestration meet.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::ENGINE_VERSION;
use crate::chat::{build_context, estimate_tokens};
use crate::diagnostics::{Diagnostics, DiagnosticsContext};
use crate::engine::lifecycle;
use crate::engine::{BenchmarkResult, EngineInfo, EngineLaunch, EngineManager, EngineStatus};
use crate::errors::{AppError, Result};
use crate::events::{
    BenchmarkProgress, ChatCancelled, ChatDone, ChatErrorEvent, CoreEvent, EventSink,
};
use crate::hardware::HardwareProfile;
use crate::models::DownloadManager;
use crate::models::catalog::{Catalog, ModelCard, ModelSpec};
use crate::models::verify::free_disk_bytes;
use crate::paths::Paths;
use crate::storage::{
    Conversation, ConversationSummary, Db, DownloadRecord, InstalledModel, MessageRole,
    MessageStatus, Settings,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BootstrapState {
    pub app_version: String,
    pub catalog_version: String,
    pub engine_version: String,
    pub hardware: HardwareProfile,
    pub recommended_model_id: Option<String>,
    pub active_model_id: Option<String>,
    pub onboarding_complete: bool,
    pub models: Vec<ModelCard>,
    pub installed: Vec<InstalledModel>,
    pub conversations: Vec<ConversationSummary>,
    pub downloads: Vec<DownloadRecord>,
    pub settings: Settings,
    pub engine: EngineStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SendMessageResponse {
    pub request_id: String,
    pub user_message_id: String,
    pub assistant_message_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct SendMessageRequest {
    pub conversation_id: String,
    pub content: String,
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StorageSummary {
    pub installed_bytes: u64,
    pub free_disk_bytes: u64,
    pub models_dir: String,
}

pub struct AppCore {
    pub paths: Paths,
    pub db: Db,
    pub catalog: Catalog,
    hardware: RwLock<HardwareProfile>,
    pub engine: Arc<EngineManager>,
    pub downloads: Arc<DownloadManager>,
    pub diagnostics: Arc<Diagnostics>,
    events: EventSink,
    cancellations: Arc<Mutex<HashMap<String, CancellationToken>>>,
    app_version: String,
}

impl AppCore {
    pub fn new(paths: Paths, events: EventSink, app_version: impl Into<String>) -> Result<Self> {
        let app_version = app_version.into();
        let catalog = Catalog::embedded()?;
        let hardware = HardwareProfile::detect(&paths.data_dir)?;
        let db = Db::open(&paths.db_path)?;
        db.pause_stale_downloads()?;

        if let Err(error) = lifecycle::cleanup_orphan(&paths) {
            tracing::warn!(%error, "could not clean up a stale engine process");
        }

        let engine = Arc::new(EngineManager::new(paths.clone(), events.clone())?);
        let downloads = Arc::new(DownloadManager::new(
            paths.clone(),
            catalog.clone(),
            db.clone(),
            events.clone(),
        )?);
        let diagnostics = Arc::new(Diagnostics::new(
            paths.clone(),
            app_version.clone(),
            ENGINE_VERSION,
        ));

        Ok(Self {
            paths,
            db,
            catalog,
            hardware: RwLock::new(hardware),
            engine,
            downloads,
            diagnostics,
            events,
            cancellations: Arc::new(Mutex::new(HashMap::new())),
            app_version,
        })
    }

    pub fn app_version(&self) -> &str {
        &self.app_version
    }

    pub fn hardware(&self) -> HardwareProfile {
        self.hardware
            .read()
            .map(|hardware| hardware.clone())
            .unwrap_or_else(|_| HardwareProfile::synthetic_minimum())
    }

    pub fn scan_hardware(&self) -> Result<HardwareProfile> {
        let profile = HardwareProfile::detect(&self.paths.data_dir)?;
        if let Ok(mut hardware) = self.hardware.write() {
            *hardware = profile.clone();
        }
        Ok(profile)
    }

    pub fn settings(&self) -> Result<Settings> {
        self.db.load_settings()
    }

    pub fn update_settings(&self, patch: &Value) -> Result<Settings> {
        let current = self.db.load_settings()?;
        let next = current.patched(patch)?;
        if let Some(model_id) = &next.active_model_id {
            if self.db.installed_model(model_id)?.is_none() {
                return Err(AppError::ModelNotInstalled);
            }
        }
        self.db.save_settings(&next)?;
        Ok(next)
    }

    /// Make sure the selected model is still installed; fall back to the
    /// hardware recommendation or the only installed model.
    pub fn ensure_active_model(&self) -> Result<Option<String>> {
        let mut settings = self.db.load_settings()?;
        let installed = self.db.installed_models()?;
        let valid = settings
            .active_model_id
            .as_ref()
            .is_some_and(|id| installed.iter().any(|model| &model.id == id));
        if valid {
            return Ok(settings.active_model_id);
        }

        let hardware = self.hardware();
        let recommended = self
            .catalog
            .recommended_id(&hardware)
            .and_then(|id| installed.iter().find(|model| model.id == id))
            .or_else(|| installed.first())
            .map(|model| model.id.clone());

        settings.active_model_id = recommended.clone();
        self.db.save_settings(&settings)?;
        Ok(recommended)
    }

    pub fn model_spec(&self, model_id: &str) -> Option<&ModelSpec> {
        self.catalog.get(model_id)
    }

    pub fn model_cards(&self) -> Result<Vec<ModelCard>> {
        let installed = self.db.installed_models()?;
        let ids: std::collections::HashSet<String> =
            installed.into_iter().map(|model| model.id).collect();
        Ok(self.catalog.cards(&self.hardware(), &ids))
    }

    /// Build a card for any installed model, including user-imported GGUFs
    /// that are not part of the embedded catalog.
    pub fn installed_card(&self, installed: &InstalledModel) -> ModelCard {
        if let Ok(cards) = self.model_cards() {
            if let Some(card) = cards.into_iter().find(|card| card.model.id == installed.id) {
                return card;
            }
        }
        let hardware = self.hardware();
        let spec = ModelSpec {
            id: installed.id.clone(),
            display_name: installed.display_name.clone(),
            tier: crate::models::ModelTier::Fast,
            family: "Imported".into(),
            params_b: 0.0,
            quant: "unknown".into(),
            file_name: installed.file_name.clone(),
            url: None,
            sha256: Some(installed.sha256.clone()),
            size_bytes: installed.size_bytes,
            license: "See the model's original license".into(),
            license_url: String::new(),
            model_card_url: String::new(),
            revision: None,
            min_ram_bytes: 0,
            recommended_ram_bytes: 0,
            min_disk_bytes: installed.size_bytes,
            context_default: hardware.default_context(),
            context_max: 32_768,
            max_tokens_default: hardware.default_max_tokens(),
            sampling: Default::default(),
            enable_thinking: false,
            engine_min_tier: "x64".into(),
            architecture: None,
            description:
                "A GGUF model you imported from disk. It is not part of the curated catalog.".into(),
            badge: Some("Imported".into()),
            experimental: false,
            validation: crate::models::ValidationStatus::Validated,
            unavailable_reason: None,
        };
        ModelCard {
            model: spec,
            installed: true,
            recommended: false,
            compatible: true,
            unavailable_reason: None,
        }
    }

    pub async fn bootstrap(&self) -> Result<BootstrapState> {
        let active_model_id = self.ensure_active_model()?;
        let settings = self.db.load_settings()?;
        let installed = self.db.installed_models()?;
        let hardware = self.hardware();
        let installed_ids: std::collections::HashSet<String> =
            installed.iter().map(|model| model.id.clone()).collect();
        let models = self.catalog.cards(&hardware, &installed_ids);
        Ok(BootstrapState {
            app_version: self.app_version.clone(),
            catalog_version: self.catalog.catalog_version.clone(),
            engine_version: ENGINE_VERSION.to_string(),
            recommended_model_id: self.catalog.recommended_id(&hardware).map(str::to_string),
            hardware,
            active_model_id,
            onboarding_complete: settings.onboarding_complete,
            models,
            installed,
            conversations: self.db.list_conversations()?,
            downloads: self.db.downloads()?,
            settings: settings.clone(),
            engine: self.engine.status().await,
        })
    }

    pub async fn engine_status(&self) -> EngineStatus {
        self.engine.status().await
    }

    pub fn launch_for(&self, model_id: &str) -> Result<EngineLaunch> {
        let installed = self
            .db
            .installed_model(model_id)?
            .ok_or(AppError::ModelNotInstalled)?;
        if !std::path::Path::new(&installed.path).exists() {
            return Err(AppError::ModelNotFound);
        }
        let spec = self.catalog.get(model_id);
        Ok(EngineLaunch::create(
            installed,
            spec,
            &self.hardware(),
            &self.db.load_settings()?,
        ))
    }

    pub async fn start_engine(&self, model_id: &str) -> Result<EngineInfo> {
        let launch = self.launch_for(model_id)?;
        match self.engine.ensure(&launch).await {
            Ok(info) => {
                self.db.touch_model(model_id)?;
                Ok(info)
            }
            Err(error) => {
                self.diagnostics.record_error(&error);
                Err(error)
            }
        }
    }

    pub async fn stop_engine(&self) {
        self.cancel_all();
        self.engine.stop().await;
    }

    // -------------------------------------------------------------- chatting

    pub fn create_conversation(&self, model_id: Option<String>) -> Result<Conversation> {
        let settings = self.db.load_settings()?;
        let installed = self.db.installed_models()?;
        let model_id = match model_id.or(settings.active_model_id) {
            Some(id) => id,
            None => installed
                .first()
                .map(|model| model.id.clone())
                .ok_or(AppError::ModelNotInstalled)?,
        };
        if self.db.installed_model(&model_id)?.is_none() {
            return Err(AppError::ModelNotInstalled);
        }
        self.db.create_conversation(&model_id)
    }

    pub fn send_message(
        &self,
        conversation_id: &str,
        content: &str,
        request_id: &str,
    ) -> Result<SendMessageResponse> {
        let content = content.trim();
        if content.is_empty() {
            return Err(AppError::InvalidInput {
                detail: "message is empty".into(),
            });
        }
        if content.chars().count() > 32_000 {
            return Err(AppError::InvalidInput {
                detail: "message is too long".into(),
            });
        }
        self.begin_generation(conversation_id, Some(content.to_string()), true, request_id)
    }

    pub fn regenerate(
        &self,
        conversation_id: &str,
        request_id: &str,
    ) -> Result<SendMessageResponse> {
        let conversation = self.db.get_conversation(conversation_id)?;
        let user =
            self.db
                .last_user_message(conversation_id)?
                .ok_or_else(|| AppError::InvalidInput {
                    detail: "this conversation has no user message to regenerate".into(),
                })?;
        self.db.delete_messages_after(&conversation.id, user.seq)?;
        self.begin_generation(conversation_id, None, false, request_id)
    }

    fn begin_generation(
        &self,
        conversation_id: &str,
        content: Option<String>,
        append_user: bool,
        request_id: &str,
    ) -> Result<SendMessageResponse> {
        let conversation = self.db.get_conversation(conversation_id)?;
        let installed = self
            .db
            .installed_model(&conversation.model_id)?
            .ok_or(AppError::ModelNotInstalled)?;
        if !std::path::Path::new(&installed.path).exists() {
            return Err(AppError::ModelNotFound);
        }
        let settings = self.db.load_settings()?;
        let spec = self.catalog.get(&conversation.model_id).cloned();
        let launch = EngineLaunch::create(installed, spec.as_ref(), &self.hardware(), &settings);

        let user_message = if append_user {
            let content = content.unwrap_or_default();
            self.db.append_message(
                conversation_id,
                MessageRole::User,
                &content,
                None,
                MessageStatus::Complete,
                Some(estimate_tokens(&content)),
            )?
        } else {
            self.db
                .last_user_message(conversation_id)?
                .ok_or_else(|| AppError::InvalidInput {
                    detail: "this conversation has no user message".into(),
                })?
        };

        // Reload so the new user turn is part of the context.
        let conversation = self.db.get_conversation(conversation_id)?;
        let context = build_context(
            &settings.system_prompt,
            &conversation.messages,
            launch.context_size,
            launch.max_tokens,
        )?;

        let assistant_message = self.db.append_message(
            conversation_id,
            MessageRole::Assistant,
            "",
            None,
            MessageStatus::Streaming,
            None,
        )?;

        let cancel = CancellationToken::new();
        {
            let mut cancellations = self.cancellations.lock().map_err(|_| AppError::Storage {
                detail: "cancellation map is poisoned".into(),
            })?;
            cancellations.insert(request_id.to_string(), cancel.clone());
        }

        let engine = Arc::clone(&self.engine);
        let db = self.db.clone();
        let events = self.events.clone();
        let diagnostics = Arc::clone(&self.diagnostics);
        let cancellations = Arc::clone(&self.cancellations);
        let request_id_owned = request_id.to_string();
        let assistant_id = assistant_message.id.clone();
        let show_reasoning = settings.show_reasoning;

        tokio::spawn(async move {
            if cancel.is_cancelled() {
                let _ =
                    db.update_message(&assistant_id, "", None, MessageStatus::Interrupted, Some(0));
                events.emit(CoreEvent::ChatCancelled(ChatCancelled {
                    request_id: request_id_owned.clone(),
                }));
                if let Ok(mut map) = cancellations.lock() {
                    map.remove(&request_id_owned);
                }
                return;
            }

            let outcome = engine
                .chat(&launch, context, &request_id_owned, cancel)
                .await;

            match outcome {
                Ok(outcome) if outcome.cancelled => {
                    let reasoning = show_reasoning
                        .then(|| outcome.reasoning.clone())
                        .filter(|reasoning| !reasoning.is_empty());
                    let _ = db.update_message(
                        &assistant_id,
                        &outcome.content,
                        reasoning.as_deref(),
                        MessageStatus::Interrupted,
                        Some(outcome.usage.completion_tokens),
                    );
                    events.emit(CoreEvent::ChatCancelled(ChatCancelled {
                        request_id: request_id_owned.clone(),
                    }));
                }
                Ok(outcome) => {
                    let reasoning = show_reasoning
                        .then(|| outcome.reasoning.clone())
                        .filter(|reasoning| !reasoning.is_empty());
                    let _ = db.update_message(
                        &assistant_id,
                        &outcome.content,
                        reasoning.as_deref(),
                        MessageStatus::Complete,
                        Some(outcome.usage.completion_tokens),
                    );
                    events.emit(CoreEvent::ChatDone(ChatDone {
                        request_id: request_id_owned.clone(),
                        usage: outcome.usage,
                        elapsed_ms: outcome.elapsed_ms,
                        tokens_per_sec: outcome.tokens_per_sec,
                    }));
                }
                Err(error) if error.code() == "canceled" => {
                    let _ = db.update_message(
                        &assistant_id,
                        "",
                        None,
                        MessageStatus::Interrupted,
                        None,
                    );
                    events.emit(CoreEvent::ChatCancelled(ChatCancelled {
                        request_id: request_id_owned.clone(),
                    }));
                }
                Err(error) => {
                    diagnostics.record_error(&error);
                    let _ = db.update_message(&assistant_id, "", None, MessageStatus::Error, None);
                    events.emit(CoreEvent::ChatError(ChatErrorEvent {
                        request_id: request_id_owned.clone(),
                        error: error.payload(),
                    }));
                }
            }

            if let Ok(mut map) = cancellations.lock() {
                map.remove(&request_id_owned);
            }
        });

        Ok(SendMessageResponse {
            request_id: request_id.to_string(),
            user_message_id: user_message.id,
            assistant_message_id: assistant_message.id,
        })
    }

    pub fn cancel_generation(&self, request_id: &str) {
        if let Ok(map) = self.cancellations.lock() {
            if let Some(token) = map.get(request_id) {
                token.cancel();
            }
        }
    }

    fn cancel_all(&self) {
        if let Ok(map) = self.cancellations.lock() {
            for token in map.values() {
                token.cancel();
            }
        }
    }

    // ------------------------------------------------------------------ chats

    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>> {
        self.db.list_conversations()
    }

    pub fn get_conversation(&self, id: &str) -> Result<Conversation> {
        self.db.get_conversation(id)
    }

    pub fn rename_conversation(&self, id: &str, title: &str) -> Result<()> {
        self.db.rename_conversation(id, title)
    }

    pub fn delete_conversation(&self, id: &str) -> Result<()> {
        self.db.delete_conversation(id)
    }

    pub fn export_conversation(&self, id: &str, format: &str) -> Result<PathBuf> {
        let conversation = self.db.get_conversation(id)?;
        std::fs::create_dir_all(&self.paths.exports_dir)?;
        let (extension, contents) = match format {
            "json" => ("json", serde_json::to_string_pretty(&conversation)?),
            "markdown" | "md" => ("md", render_markdown(&conversation)),
            other => {
                return Err(AppError::InvalidInput {
                    detail: format!("unsupported export format {other}"),
                });
            }
        };
        let path = self
            .paths
            .exports_dir
            .join(format!("{}.{extension}", conversation.id));
        std::fs::write(&path, contents)?;
        Ok(path)
    }

    // ---------------------------------------------------------------- models

    pub async fn download_model(&self, model_id: &str) -> Result<()> {
        self.downloads.start(model_id).await
    }

    pub fn pause_download(&self, model_id: &str) -> Result<()> {
        self.downloads.pause(model_id)
    }

    pub async fn resume_download(&self, model_id: &str) -> Result<()> {
        self.downloads.start(model_id).await
    }

    pub fn cancel_download(&self, model_id: &str) -> Result<()> {
        self.downloads.cancel(model_id)
    }

    pub async fn import_model(&self, path: PathBuf) -> Result<InstalledModel> {
        self.downloads.import(path).await
    }

    pub async fn delete_model(&self, model_id: &str) -> Result<()> {
        if self.engine.is_loaded(model_id).await {
            self.engine.stop().await;
        }
        if let Some(model) = self.db.delete_installed(model_id)? {
            let path = PathBuf::from(&model.path);
            // Only delete files inside the managed models directory.
            if let (Ok(root), Ok(parent)) = (
                std::fs::canonicalize(&self.paths.models_dir),
                std::fs::canonicalize(path.parent().unwrap_or(&self.paths.models_dir)),
            ) {
                if parent.starts_with(&root) {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        if let Some(spec) = self.catalog.get(model_id) {
            let _ = std::fs::remove_file(self.paths.part_path(&spec.file_name));
        }
        self.db.delete_download(model_id)?;
        if self.settings()?.active_model_id.as_deref() == Some(model_id) {
            self.ensure_active_model()?;
        }
        Ok(())
    }

    pub fn storage_summary(&self) -> Result<StorageSummary> {
        let installed = self.db.installed_models()?;
        Ok(StorageSummary {
            installed_bytes: installed.iter().map(|model| model.size_bytes).sum(),
            free_disk_bytes: free_disk_bytes(&self.paths.models_dir),
            models_dir: self.paths.models_dir.to_string_lossy().to_string(),
        })
    }

    pub async fn run_benchmark(&self, model_id: &str) -> Result<BenchmarkResult> {
        let launch = self.launch_for(model_id)?;
        self.emit_benchmark(model_id, "starting", 5);
        let result = self.engine.benchmark(&launch).await;
        match &result {
            Ok(_) => self.emit_benchmark(model_id, "done", 100),
            Err(error) => {
                self.diagnostics.record_error(error);
                self.emit_benchmark(model_id, "failed", 100);
            }
        }
        result
    }

    fn emit_benchmark(&self, model_id: &str, stage: &str, percent: u8) {
        self.events
            .emit(CoreEvent::BenchmarkProgress(BenchmarkProgress {
                model_id: model_id.to_string(),
                stage: stage.to_string(),
                percent,
            }));
    }

    // ------------------------------------------------------------ diagnostics

    pub async fn export_diagnostics(&self, include_conversation: bool) -> Result<PathBuf> {
        let hardware = self.hardware();
        let settings = self.settings()?;
        let installed = self.db.installed_models()?;
        let downloads = self.db.downloads()?;
        let engine_log = self.engine.log_tail().await;
        let conversation = if include_conversation {
            self.db
                .list_conversations()?
                .first()
                .and_then(|summary| self.db.get_conversation(&summary.id).ok())
        } else {
            None
        };
        self.diagnostics.export(DiagnosticsContext {
            hardware: &hardware,
            settings: &settings,
            installed: &installed,
            downloads: &downloads,
            engine_log: &engine_log,
            conversation: conversation.as_ref(),
        })
    }

    pub fn record_error(&self, error: &AppError) {
        self.diagnostics.record_error(error);
    }

    pub fn app_data_dir(&self) -> PathBuf {
        self.paths.data_dir.clone()
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.paths.logs_dir.clone()
    }
}

fn render_markdown(conversation: &Conversation) -> String {
    let mut output = String::new();
    output.push_str(&format!("# {}\n\n", conversation.title));
    output.push_str(&format!("Model: `{}`\n\n", conversation.model_id));
    for message in &conversation.messages {
        match message.role {
            MessageRole::User => output.push_str("## You\n\n"),
            MessageRole::Assistant => output.push_str("## AI\n\n"),
            MessageRole::System => continue,
        }
        output.push_str(&message.content);
        output.push_str("\n\n");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventSink;
    use crate::storage::ModelSource;
    use std::sync::mpsc;

    fn core(dir: &std::path::Path) -> AppCore {
        let paths = Paths::new(
            dir.join("data"),
            dir.join("engine"),
            dir.join("engine-baseline"),
        )
        .unwrap();
        AppCore::new(paths, EventSink::noop(), "0.1.0").unwrap()
    }

    #[test]
    fn bootstrap_selects_recommendation_without_installed_models() {
        let dir = tempfile::tempdir().unwrap();
        let core = core(dir.path());
        let expected = core
            .catalog
            .recommended_id(&core.hardware())
            .map(str::to_string);
        let state = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(core.bootstrap())
            .unwrap();
        assert!(state.active_model_id.is_none());
        assert_eq!(state.recommended_model_id, expected);
        assert!(!state.onboarding_complete);
    }

    #[test]
    fn settings_patch_rejects_uninstalled_active_model() {
        let dir = tempfile::tempdir().unwrap();
        let core = core(dir.path());
        let error = core
            .update_settings(&serde_json::json!({"active_model_id": "bonsai2-2b-pq2"}))
            .unwrap_err();
        assert_eq!(error.code(), "model_not_installed");
    }

    #[test]
    fn conversation_requires_an_installed_model() {
        let dir = tempfile::tempdir().unwrap();
        let core = core(dir.path());
        let error = core.create_conversation(None).unwrap_err();
        assert_eq!(error.code(), "model_not_installed");
    }

    #[test]
    fn generation_errors_are_emitted_as_events() {
        let dir = tempfile::tempdir().unwrap();
        let (sender, receiver) = mpsc::channel();
        let paths = Paths::new(
            dir.path().join("data"),
            dir.path().join("engine"),
            dir.path().join("baseline"),
        )
        .unwrap();
        let sink = EventSink::new(Arc::new(move |event| {
            let _ = sender.send(event);
        }));
        let core = AppCore::new(paths, sink, "0.1.0").unwrap();

        // Install a fake model row pointing at a real file, then create a chat.
        let model_path = dir.path().join("fake.gguf");
        std::fs::write(&model_path, b"GGUF").unwrap();
        core.db
            .upsert_installed(&InstalledModel {
                id: "bonsai2-2b-pq2".into(),
                display_name: "Test".into(),
                file_name: "fake.gguf".into(),
                path: model_path.to_string_lossy().to_string(),
                sha256: "a".repeat(64),
                size_bytes: 4,
                installed_at: 0,
                last_used_at: None,
                source: ModelSource::Catalog,
            })
            .unwrap();
        let conversation = core.create_conversation(None).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _entered = runtime.enter();
        let response = core
            .send_message(&conversation.id, "hello", "req-1")
            .unwrap();
        assert_eq!(response.request_id, "req-1");

        let event = loop {
            let event = receiver
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            if matches!(event, CoreEvent::ChatError(_)) {
                break event;
            }
        };
        match event {
            CoreEvent::ChatError(event) => {
                assert_eq!(event.request_id, "req-1");
                assert_eq!(event.error.code, "engine_spawn_failed");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
