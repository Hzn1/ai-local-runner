//! Tauri entry point: wiring only.
//!
//! Responsibilities kept here: plugin registration, app paths, logging,
//! forwarding core events, idle engine unloading and orderly shutdown. All
//! behaviour lives in `tnn-core`.

mod commands;
mod events;
mod state;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tauri::{Manager, RunEvent};
use tnn_core::{AppCore, Paths};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::state::AppState;

pub fn run() {
    let builder = tauri::Builder::default()
        // Must be the first plugin so a second launch never builds a core.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let engine_dir = resolve_engine_dir(app, "engine");
            let baseline_dir = resolve_engine_dir(app, "engine-baseline");
            let paths = Paths::new(data_dir, engine_dir, baseline_dir)?;
            let log_guard = init_logging(&paths.logs_dir);

            let sink = events::tauri_sink(app.handle().clone());
            let core = Arc::new(AppCore::new(
                paths,
                sink,
                app.package_info().version.to_string(),
            )?);

            tracing::info!(
                version = %app.package_info().version,
                data_dir = %core.app_data_dir().display(),
                "TNN Local AI starting"
            );

            app.manage(AppState {
                core: Arc::clone(&core),
                _log_guard: log_guard,
            });

            spawn_idle_monitor(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap_state,
            commands::scan_hardware,
            commands::list_models,
            commands::download_model,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::delete_model,
            commands::import_model,
            commands::get_storage_summary,
            commands::start_engine,
            commands::stop_engine,
            commands::engine_status,
            commands::send_message,
            commands::cancel_generation,
            commands::regenerate,
            commands::list_conversations,
            commands::get_conversation,
            commands::create_conversation,
            commands::rename_conversation,
            commands::delete_conversation,
            commands::export_conversation,
            commands::get_settings,
            commands::update_settings,
            commands::run_benchmark,
            commands::export_diagnostics,
            commands::open_data_dir,
            commands::open_logs,
            commands::check_for_updates,
        ]);

    builder
        .build(tauri::generate_context!())
        .expect("failed to build the Tauri application")
        .run(|app, event| {
            if matches!(event, RunEvent::ExitRequested { .. }) {
                let core = app
                    .try_state::<AppState>()
                    .map(|state| Arc::clone(&state.core));
                if let Some(core) = core {
                    // Stop the child process and remove the pid file before
                    // the process exits. The engine also dies with us, but
                    // this keeps normal exits tidy.
                    tauri::async_runtime::block_on(core.stop_engine());
                }
                tracing::info!("TNN Local AI stopped");
            }
        });
}

/// Where the engine lives in development and in a bundled app.
fn resolve_engine_dir(app: &tauri::App, name: &str) -> PathBuf {
    let mut candidates = Vec::new();
    if let Ok(resources) = app.path().resource_dir() {
        candidates.push(resources.join("resources").join(name));
        candidates.push(resources.join(name));
    }
    // `tauri dev` runs from src-tauri before resources are staged.
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(name),
    );

    let executable = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    candidates
        .iter()
        .find(|dir| dir.join(executable).exists())
        .cloned()
        .unwrap_or_else(|| candidates[0].clone())
}

fn init_logging(logs_dir: &Path) -> WorkerGuard {
    let file_appender = tracing_appender::rolling::daily(logs_dir, "tnn.log");
    let (writer, guard) = tracing_appender::non_blocking(file_appender);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info,tnn_core=debug,tnn_local_ai_lib=debug,tnn_engine=info")
    });

    let registry = tracing_subscriber::registry().with(filter).with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(false),
    );
    #[cfg(debug_assertions)]
    let registry = registry.with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr));
    registry.init();

    guard
}

/// Unload the model after the configured idle period. Keeping this in the
/// shell avoids a background task in the core that tests would have to stop.
fn spawn_idle_monitor(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let Some(state) = app.try_state::<AppState>() else {
                break;
            };
            let core = Arc::clone(&state.core);
            drop(state);
            let minutes = match core.settings() {
                Ok(settings) => settings.idle_unload_minutes,
                Err(error) => {
                    tracing::warn!(%error, "could not read settings for idle unload");
                    continue;
                }
            };
            core.engine.unload_if_idle(minutes).await;
        }
    });
}
