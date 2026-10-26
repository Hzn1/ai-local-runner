//! Engine supervisor: locate, spawn, health-check, self-test, fall back, stop.

use std::collections::VecDeque;
use std::net::TcpListener;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rand::RngCore;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::chat::{ChatCompletionRequest, ChatMessage};
use crate::engine::args::{EngineArgsConfig, build_args};
use crate::engine::health::{self, LogTail, push_log};
use crate::engine::lifecycle::{self, ensure_executable, lower_priority};
use crate::engine::selftest;
use crate::engine::stream::{self, ChatOutcome};
use crate::errors::{AppError, Result};
use crate::events::{CoreEvent, EngineState, EngineStateEvent, EventSink};
use crate::hardware::HardwareProfile;
use crate::models::catalog::{ModelSpec, Sampling};
use crate::paths::Paths;
use crate::storage::{InstalledModel, Settings};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineInfo {
    pub model_id: String,
    pub tier: String,
    pub compatibility_mode: bool,
    pub context_size: u32,
    pub threads: u32,
    pub loaded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineStatus {
    pub state: EngineState,
    pub model_id: Option<String>,
    pub tier: Option<String>,
    pub compatibility_mode: bool,
    pub uptime_secs: Option<u64>,
    pub idle_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchmarkResult {
    pub model_id: String,
    pub prompt_tokens: u32,
    pub prompt_tps: f32,
    pub generation_tokens: u32,
    pub generation_tps: f32,
    pub elapsed_ms: u64,
    pub tier: String,
    pub compatibility_mode: bool,
}

/// Everything needed to launch or chat with a specific model on this machine.
#[derive(Debug, Clone)]
pub struct EngineLaunch {
    pub installed: InstalledModel,
    pub spec: Option<ModelSpec>,
    pub context_size: u32,
    pub max_tokens: u32,
    pub threads: usize,
    pub batch_size: u32,
    pub ubatch_size: u32,
    pub sampling: Sampling,
    pub enable_thinking: bool,
    pub tier: String,
}

impl EngineLaunch {
    pub fn create(
        installed: InstalledModel,
        spec: Option<&ModelSpec>,
        hardware: &HardwareProfile,
        settings: &Settings,
    ) -> Self {
        let sampling = spec.map(|model| model.sampling.clone()).unwrap_or_default();
        let context_max = spec.map(|model| model.context_max).unwrap_or(32_768);
        let default_context = if hardware.minimum_mode {
            spec.map(|model| model.context_default.min(2048))
                .unwrap_or(2048)
        } else {
            spec.map(|model| model.context_default)
                .unwrap_or_else(|| hardware.default_context())
        };
        let mut context_size = settings
            .resolved_context(default_context)
            .clamp(512, context_max);
        let mut threads = settings.resolved_threads(hardware.default_threads());
        let mut max_tokens = settings
            .resolved_max_tokens(
                spec.map(|model| model.max_tokens_default)
                    .unwrap_or_else(|| hardware.default_max_tokens()),
            )
            .min(context_size / 2);

        // Keep memory and CPU limits on low-end machines. A slow CPU alone
        // should not silently discard an explicit answer-length preference
        // when the machine has enough RAM.
        if hardware.minimum_mode {
            context_size = context_size.min(2048);
            threads = threads.min(hardware.physical_cores.max(1));
            let output_cap = if hardware.total_ram_bytes >= 8 * 1024_u64.pow(3)
                && settings.max_tokens.is_some()
            {
                context_size / 2
            } else {
                256
            };
            max_tokens = max_tokens.min(output_cap);
        }

        let (batch_size, ubatch_size) = hardware.default_batch();
        Self {
            installed,
            spec: spec.cloned(),
            context_size,
            max_tokens,
            threads,
            batch_size,
            ubatch_size,
            sampling,
            enable_thinking: false,
            tier: hardware.features.best_engine_tier().to_string(),
        }
    }

    pub fn model_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.installed.path)
    }
}

struct RunningEngine {
    child: Child,
    port: u16,
    api_key: String,
    model_id: String,
    tier: String,
    compatibility_mode: bool,
    started: Instant,
    last_used: Instant,
    logs: LogTail,
}

impl RunningEngine {
    fn info(&self, launch: &EngineLaunch) -> EngineInfo {
        EngineInfo {
            model_id: self.model_id.clone(),
            tier: self.tier.clone(),
            compatibility_mode: self.compatibility_mode,
            context_size: launch.context_size,
            threads: launch.threads as u32,
            loaded: true,
        }
    }
}

impl Drop for RunningEngine {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

#[derive(Default)]
struct ManagerState {
    running: Option<RunningEngine>,
}

pub struct EngineManager {
    paths: Paths,
    client: reqwest::Client,
    events: EventSink,
    state: tokio::sync::Mutex<ManagerState>,
}

impl EngineManager {
    pub fn new(paths: Paths, events: EventSink) -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|error| AppError::Network {
                detail: error.to_string(),
            })?;
        Ok(Self {
            paths,
            client,
            events,
            state: tokio::sync::Mutex::new(ManagerState::default()),
        })
    }

    /// Start the engine if needed. Repeated calls for the same model are cheap.
    pub async fn ensure(&self, launch: &EngineLaunch) -> Result<EngineInfo> {
        let mut state = self.state.lock().await;

        if let Some(running) = state.running.as_mut() {
            if running.model_id == launch.installed.id && running.child.try_wait()?.is_none() {
                running.last_used = Instant::now();
                return Ok(running.info(launch));
            }
        }
        self.stop_locked(&mut state).await;

        self.emit(
            EngineState::Starting,
            Some(&launch.installed.id),
            None,
            false,
            None,
        );

        let primary = self.start_in(launch, &self.paths.engine_dir, false).await;
        match primary {
            Ok(running) => {
                let info = running.info(launch);
                state.running = Some(running);
                self.emit(
                    EngineState::Ready,
                    Some(&info.model_id),
                    Some(&info.tier),
                    info.compatibility_mode,
                    None,
                );
                Ok(info)
            }
            Err(primary_error) => {
                tracing::warn!(error = %primary_error, "primary engine failed");
                let can_fallback = self.paths.baseline_engine_dir != self.paths.engine_dir
                    && self
                        .paths
                        .engine_executable(&self.paths.baseline_engine_dir)
                        .exists()
                    && matches!(
                        primary_error,
                        AppError::EngineIllegalInstruction
                            | AppError::EngineGarbageOutput
                            | AppError::EngineCrash { .. }
                            | AppError::EngineSpawnFailed { .. }
                    );

                if can_fallback {
                    tracing::warn!("retrying with the x64 baseline engine");
                    match self
                        .start_in(launch, &self.paths.baseline_engine_dir, true)
                        .await
                    {
                        Ok(running) => {
                            let info = running.info(launch);
                            state.running = Some(running);
                            self.emit(
                                EngineState::Ready,
                                Some(&info.model_id),
                                Some(&info.tier),
                                true,
                                None,
                            );
                            return Ok(info);
                        }
                        Err(error) => {
                            self.emit(
                                EngineState::Failed,
                                Some(&launch.installed.id),
                                None,
                                true,
                                Some(error.payload()),
                            );
                            return Err(error);
                        }
                    }
                }

                self.emit(
                    EngineState::Failed,
                    Some(&launch.installed.id),
                    None,
                    false,
                    Some(primary_error.payload()),
                );
                Err(primary_error)
            }
        }
    }

    async fn start_in(
        &self,
        launch: &EngineLaunch,
        dir: &Path,
        compatibility_mode: bool,
    ) -> Result<RunningEngine> {
        let exe = self.paths.engine_executable(dir);
        if !exe.exists() {
            return Err(AppError::EngineSpawnFailed {
                detail: format!(
                    "engine binary not found at {}. Run scripts/fetch-engine.sh (or .ps1) before launching.",
                    exe.display()
                ),
            });
        }
        ensure_executable(&exe)?;

        let port = free_loopback_port()?;
        let api_key = random_api_key();
        let args = build_args(&EngineArgsConfig {
            model_path: launch.model_path(),
            port,
            api_key: api_key.clone(),
            context_size: launch.context_size,
            threads: launch.threads,
            batch_size: launch.batch_size,
            ubatch_size: launch.ubatch_size,
        });

        let mut command = Command::new(&exe);
        command
            .args(&args)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Make the runtime dispatcher and shared libraries independent of the
        // app's working directory.
        #[cfg(unix)]
        {
            let mut library_path = dir.to_string_lossy().to_string();
            if let Ok(existing) = std::env::var("LD_LIBRARY_PATH") {
                if !existing.is_empty() {
                    library_path.push(':');
                    library_path.push_str(&existing);
                }
            }
            command.env("LD_LIBRARY_PATH", library_path);
            // Own process group so the whole group can be terminated.
            unsafe {
                command.pre_exec(|| {
                    libc::setpgid(0, 0);
                    Ok(())
                });
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command
            .spawn()
            .map_err(|error| AppError::EngineSpawnFailed {
                detail: format!("{}: {error}", exe.display()),
            })?;

        if let Some(pid) = child.id() {
            lower_priority(pid);
            let _ = lifecycle::write_pid_file(&self.paths, pid, port, &launch.installed.id);
        }

        let logs: LogTail = Arc::new(Mutex::new(VecDeque::with_capacity(health::LOG_TAIL_LINES)));
        if let Some(stdout) = child.stdout.take() {
            spawn_log_reader(stdout, logs.clone(), "stdout");
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_log_reader(stderr, logs.clone(), "stderr");
        }

        self.emit(
            EngineState::Loading,
            Some(&launch.installed.id),
            Some(if compatibility_mode {
                "x64"
            } else {
                launch.tier.as_str()
            }),
            compatibility_mode,
            None,
        );

        let base_url = format!("http://127.0.0.1:{port}");
        let startup = async {
            health::wait_healthy(
                &self.client,
                &base_url,
                &api_key,
                Duration::from_secs(120),
                &mut child,
                &logs,
            )
            .await?;
            selftest::run_selftest(&self.client, &base_url, &api_key).await
        }
        .await;

        if let Err(error) = startup {
            lifecycle::terminate(&mut child).await;
            lifecycle::remove_pid_file(&self.paths);
            return Err(error);
        }

        let tier = if compatibility_mode {
            "x64".to_string()
        } else {
            launch.tier.clone()
        };

        Ok(RunningEngine {
            child,
            port,
            api_key,
            model_id: launch.installed.id.clone(),
            tier,
            compatibility_mode,
            started: Instant::now(),
            last_used: Instant::now(),
            logs,
        })
    }

    /// Stream a completion from the currently loaded engine.
    pub async fn chat(
        &self,
        launch: &EngineLaunch,
        messages: Vec<ChatMessage>,
        request_id: &str,
        cancel: CancellationToken,
    ) -> Result<ChatOutcome> {
        self.ensure(launch).await?;

        let (port, api_key) = {
            let mut state = self.state.lock().await;
            let running = state
                .running
                .as_mut()
                .ok_or_else(|| AppError::EngineCrash {
                    detail: "engine state disappeared".into(),
                })?;
            if running.child.try_wait()?.is_some() {
                return Err(AppError::EngineCrash {
                    detail: "engine exited before the request was sent".into(),
                });
            }
            running.last_used = Instant::now();
            (running.port, running.api_key.clone())
        };

        let request = ChatCompletionRequest::new(
            messages,
            launch.max_tokens,
            &launch.sampling,
            launch.enable_thinking,
        );
        let outcome = stream::stream_chat(
            &self.client,
            &format!("http://127.0.0.1:{port}"),
            &api_key,
            &request,
            request_id,
            cancel,
            &self.events,
        )
        .await;

        if let Ok(mut state) = self.state.try_lock() {
            if let Some(running) = state.running.as_mut() {
                running.last_used = Instant::now();
            }
        }
        outcome
    }

    /// A short, deterministic speed check used by onboarding.
    pub async fn benchmark(&self, launch: &EngineLaunch) -> Result<BenchmarkResult> {
        let info = self.ensure(launch).await?;
        let (port, api_key) = {
            let mut state = self.state.lock().await;
            let running = state
                .running
                .as_mut()
                .ok_or_else(|| AppError::EngineCrash {
                    detail: "engine state disappeared".into(),
                })?;
            running.last_used = Instant::now();
            (running.port, running.api_key.clone())
        };

        let body = serde_json::json!({
            "model": "local",
            "messages": [{"role": "user", "content": "List the numbers from 1 to 5, separated by commas."}],
            "stream": false,
            "max_tokens": 16,
            "temperature": 0.0,
            "top_k": 1,
            "chat_template_kwargs": {"enable_thinking": false},
        });
        let started = Instant::now();
        let response = self
            .client
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .bearer_auth(&api_key)
            .json(&body)
            .timeout(Duration::from_secs(300))
            .send()
            .await
            .map_err(|error| AppError::EngineCrash {
                detail: format!("benchmark request failed: {error}"),
            })?;
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        if !status_is_success(status) {
            return Err(stream::map_http_error(status, &text));
        }
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| AppError::EngineGarbageOutput)?;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let timings = value.get("timings");
        let prompt_tokens = timings
            .and_then(|t| t.get("prompt_n"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        let generation_tokens = timings
            .and_then(|t| t.get("predicted_n"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        let prompt_tps = timings
            .and_then(|t| t.get("prompt_per_second"))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_else(|| {
                let usage = value.get("usage");
                let tokens = usage
                    .and_then(|u| u.get("prompt_tokens"))
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(prompt_tokens as f64);
                if elapsed_ms > 0 {
                    (tokens * 1000.0 / elapsed_ms as f64) as f64
                } else {
                    0.0
                }
            }) as f32;
        let generation_tps = timings
            .and_then(|t| t.get("predicted_per_second"))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_else(|| {
                let usage = value.get("usage");
                let tokens = usage
                    .and_then(|u| u.get("completion_tokens"))
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(generation_tokens as f64);
                if elapsed_ms > 0 {
                    (tokens * 1000.0 / elapsed_ms as f64) as f64
                } else {
                    0.0
                }
            }) as f32;

        Ok(BenchmarkResult {
            model_id: launch.installed.id.clone(),
            prompt_tokens,
            prompt_tps,
            generation_tokens,
            generation_tps,
            elapsed_ms,
            tier: info.tier,
            compatibility_mode: info.compatibility_mode,
        })
    }

    pub async fn stop(&self) {
        let mut state = self.state.lock().await;
        self.stop_locked(&mut state).await;
        self.emit(EngineState::Stopped, None, None, false, None);
    }

    async fn stop_locked(&self, state: &mut ManagerState) {
        if let Some(mut running) = state.running.take() {
            self.emit(
                EngineState::Unloading,
                Some(&running.model_id),
                Some(&running.tier),
                running.compatibility_mode,
                None,
            );
            lifecycle::terminate(&mut running.child).await;
            lifecycle::remove_pid_file(&self.paths);
            running.logs.lock().map(|mut logs| logs.clear()).ok();
        }
    }

    pub async fn status(&self) -> EngineStatus {
        let mut state = self.state.lock().await;
        let Some(running) = state.running.as_mut() else {
            return EngineStatus {
                state: EngineState::Stopped,
                model_id: None,
                tier: None,
                compatibility_mode: false,
                uptime_secs: None,
                idle_secs: None,
            };
        };
        let alive = running.child.try_wait().ok().flatten().is_none();
        EngineStatus {
            state: if alive {
                EngineState::Ready
            } else {
                EngineState::Crashed
            },
            model_id: Some(running.model_id.clone()),
            tier: Some(running.tier.clone()),
            compatibility_mode: running.compatibility_mode,
            uptime_secs: Some(running.started.elapsed().as_secs()),
            idle_secs: Some(running.last_used.elapsed().as_secs()),
        }
    }

    /// Return the last engine lines for diagnostics.
    pub async fn log_tail(&self) -> Vec<String> {
        let state = self.state.lock().await;
        state
            .running
            .as_ref()
            .map(|running| health::tail_snapshot(&running.logs))
            .unwrap_or_default()
    }

    pub async fn unload_if_idle(&self, minutes: u32) {
        if minutes == 0 {
            return;
        }
        let mut state = self.state.lock().await;
        let idle = state
            .running
            .as_ref()
            .map(|running| running.last_used.elapsed() >= Duration::from_secs(minutes as u64 * 60))
            .unwrap_or(false);
        if idle {
            self.stop_locked(&mut state).await;
            self.emit(EngineState::Stopped, None, None, false, None);
        }
    }

    pub async fn is_loaded(&self, model_id: &str) -> bool {
        let mut state = self.state.lock().await;
        state
            .running
            .as_mut()
            .map(|running| {
                running.model_id == model_id && running.child.try_wait().ok().flatten().is_none()
            })
            .unwrap_or(false)
    }

    fn emit(
        &self,
        state: EngineState,
        model_id: Option<&str>,
        tier: Option<&str>,
        compatibility_mode: bool,
        error: Option<crate::errors::ErrorPayload>,
    ) {
        self.events.emit(CoreEvent::EngineState(EngineStateEvent {
            state,
            model_id: model_id.map(str::to_string),
            tier: tier.map(str::to_string),
            compatibility_mode,
            error,
        }));
    }
}

fn status_is_success(status: u16) -> bool {
    (200..300).contains(&status)
}

fn free_loopback_port() -> Result<u16> {
    let listener =
        TcpListener::bind(("127.0.0.1", 0)).map_err(|error| AppError::EnginePortConflict {
            detail: error.to_string(),
        })?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| AppError::EnginePortConflict {
            detail: error.to_string(),
        })
}

fn random_api_key() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn spawn_log_reader<R>(reader: R, logs: LogTail, stream_name: &'static str)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            // Never log the API key; llama-server does not print it, but guard
            // against accidental future changes.
            let line = redact(&line);
            tracing::debug!(target: "tnn_engine", stream = stream_name, "{line}");
            push_log(&logs, line);
        }
    });
}

fn redact(line: &str) -> String {
    if line.to_ascii_lowercase().contains("api-key")
        || line.to_ascii_lowercase().contains("api_key")
    {
        // Keep the shape of the line but drop anything after the flag.
        if let Some(index) = line.to_ascii_lowercase().find("api") {
            return format!("{} [redacted]", &line[..index]);
        }
        return "[redacted]".to_string();
    }
    line.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::{CpuFeatures, HardwareProfile};
    use crate::models::catalog::Catalog;
    use crate::storage::ModelSource;

    fn hardware(ram_gib: u64, cores: usize) -> HardwareProfile {
        HardwareProfile::from_parts(
            "Linux".into(),
            "x86_64".into(),
            "Test".into(),
            cores,
            cores * 2,
            ram_gib * 1024_u64.pow(3),
            ram_gib * 1024_u64.pow(3),
            100 * 1024_u64.pow(3),
            CpuFeatures {
                sse2: true,
                ssse3: true,
                sse42: true,
                ..Default::default()
            },
        )
    }

    fn installed() -> InstalledModel {
        InstalledModel {
            id: "bonsai2-2b-pq2".into(),
            display_name: "Bonsai 2 · 2B".into(),
            file_name: "m.gguf".into(),
            path: "/tmp/m.gguf".into(),
            sha256: "a".repeat(64),
            size_bytes: 1,
            installed_at: 0,
            last_used_at: None,
            source: ModelSource::Catalog,
        }
    }

    #[test]
    fn minimum_mode_clamps_settings() {
        let hardware = hardware(4, 2);
        let settings = Settings {
            context_size: Some(8192),
            max_tokens: Some(1024),
            threads: Some(8),
            show_reasoning: true,
            ..Settings::default()
        };
        let launch = EngineLaunch::create(installed(), None, &hardware, &settings);
        assert_eq!(launch.context_size, 2048);
        assert_eq!(launch.max_tokens, 256);
        assert_eq!(launch.threads, 2);
        assert!(!launch.enable_thinking);
    }

    #[test]
    fn balanced_machine_uses_catalog_defaults() {
        let hardware = hardware(8, 4);
        let catalog = Catalog::embedded().unwrap();
        let spec = catalog.get("bonsai2-4b-pq2").unwrap();
        let launch = EngineLaunch::create(installed(), Some(spec), &hardware, &Settings::default());
        assert_eq!(launch.context_size, 4096);
        assert_eq!(launch.max_tokens, 512);
        assert_eq!(launch.threads, 4);
        assert_eq!(launch.batch_size, 512);
        assert!(!launch.enable_thinking);
    }

    #[test]
    fn api_keys_are_random_and_long() {
        let a = random_api_key();
        let b = random_api_key();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[test]
    fn balanced_machine_clamps_thread_override_to_physical_cores() {
        let hardware = hardware(8, 4);
        let settings = Settings {
            threads: Some(8),
            ..Settings::default()
        };
        let launch = EngineLaunch::create(installed(), None, &hardware, &settings);
        assert!(!hardware.minimum_mode);
        assert_eq!(launch.threads, 4);
    }

    #[test]
    fn two_core_machine_with_enough_ram_can_request_a_longer_answer() {
        let hardware = hardware(8, 2);
        let settings = Settings {
            max_tokens: Some(512),
            ..Settings::default()
        };
        let launch = EngineLaunch::create(installed(), None, &hardware, &settings);
        assert_eq!(launch.max_tokens, 512);
        assert_eq!(launch.context_size, 2048);
        assert_eq!(launch.threads, 2);
        let automatic = EngineLaunch::create(installed(), None, &hardware, &Settings::default());
        assert_eq!(automatic.max_tokens, 256);
    }

    #[test]
    fn redacts_api_key_lines() {
        assert_eq!(redact("--api-key deadbeef"), "-- [redacted]");
        assert_eq!(redact("normal log line"), "normal log line");
    }

    #[test]
    fn finds_a_free_port() {
        let port = free_loopback_port().unwrap();
        assert!(port > 0);
    }
}
