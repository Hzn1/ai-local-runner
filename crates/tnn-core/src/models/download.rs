//! Resumable, verified model downloads.
//!
//! One download runs at a time; the rest are queued behind a semaphore. Partial
//! data lives in `<file>.part` and survives app restarts. The SHA-256 is
//! computed while streaming, then the GGUF header is validated, and only then
//! is the file atomically renamed into the models directory.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::errors::{AppError, Result};
use crate::events::{CoreEvent, DownloadProgress, DownloadState, DownloadStateEvent, EventSink};
use crate::models::catalog::{Catalog, ModelSpec};
use crate::models::gguf;
use crate::models::install::{
    atomic_install, copy_file_and_hash, unique_destination, validate_import_source,
};
use crate::models::verify::{ensure_disk_space, is_allowed_model_url, validate_model_id};
use crate::paths::Paths;
use crate::storage::{Db, DownloadRecord, InstalledModel, ModelSource, now_millis};

const INTENT_RUNNING: u8 = 0;
const INTENT_PAUSE: u8 = 1;
const INTENT_CANCEL: u8 = 2;

struct TaskControl {
    cancel: CancellationToken,
    intent: Arc<AtomicU8>,
}

struct Inner {
    client: Client,
    paths: Paths,
    catalog: Catalog,
    db: Db,
    events: EventSink,
    tasks: Mutex<HashMap<String, TaskControl>>,
    gate: Semaphore,
}

/// Cloneable handle to the download subsystem.
#[derive(Clone)]
pub struct DownloadManager {
    inner: Arc<Inner>,
}

impl DownloadManager {
    pub fn new(paths: Paths, catalog: Catalog, db: Db, events: EventSink) -> Result<Self> {
        let client = Client::builder()
            .user_agent(concat!("TNN-Local-AI/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() > 10 {
                    return attempt.error("too many redirects");
                }
                let allowed = attempt
                    .url()
                    .host_str()
                    .is_some_and(crate::models::verify::is_allowed_model_host);
                if allowed {
                    attempt.follow()
                } else {
                    attempt.error("redirect to a host outside the download allowlist")
                }
            }))
            .build()
            .map_err(|error| AppError::Network {
                detail: error.to_string(),
            })?;

        Ok(Self {
            inner: Arc::new(Inner {
                client,
                paths,
                catalog,
                db,
                events,
                tasks: Mutex::new(HashMap::new()),
                gate: Semaphore::new(1),
            }),
        })
    }

    pub fn set_events(&mut self, events: EventSink) {
        // Rebuild the inner handle so existing clones are unaffected.
        let inner = Arc::get_mut(&mut self.inner);
        if let Some(inner) = inner {
            inner.events = events;
        }
    }

    /// Queue a model download. Returns immediately; progress arrives as events.
    pub async fn start(&self, model_id: &str) -> Result<()> {
        validate_model_id(model_id)?;
        let spec = self
            .inner
            .catalog
            .get(model_id)
            .ok_or(AppError::ModelNotFound)?
            .clone();

        if !spec.is_downloadable() {
            return Err(AppError::ModelUnvalidated);
        }
        let url = spec.url.clone().ok_or(AppError::ModelUnvalidated)?;
        if !is_allowed_model_url(&url) {
            return Err(AppError::DownloadBlocked { detail: url });
        }
        ensure_disk_space(&self.inner.paths.downloads_dir, spec.required_disk_bytes())?;

        {
            let tasks = self.inner.tasks.lock().map_err(lock_error)?;
            if tasks.contains_key(model_id) {
                return Err(AppError::InvalidInput {
                    detail: "this download is already running".into(),
                });
            }
        }

        // If a previous attempt finished on disk but the DB row was lost, do
        // not download again.
        if let Some(installed) = self.inner.db.installed_model(model_id)? {
            if std::path::Path::new(&installed.path).exists() {
                self.emit_state(model_id, DownloadState::Installed, None);
                return Ok(());
            }
        }

        let control = TaskControl {
            cancel: CancellationToken::new(),
            intent: Arc::new(AtomicU8::new(INTENT_RUNNING)),
        };
        {
            let mut tasks = self.inner.tasks.lock().map_err(lock_error)?;
            tasks.insert(model_id.to_string(), control);
        }

        self.record_state(model_id, DownloadState::Queued, 0, spec.size_bytes, None);
        self.emit_state(model_id, DownloadState::Queued, None);

        let manager = self.clone();
        let id = model_id.to_string();
        tokio::spawn(async move {
            manager.run(spec, id).await;
        });
        Ok(())
    }

    /// Stop writing but keep the `.part` file for resume.
    pub fn pause(&self, model_id: &str) -> Result<()> {
        self.signal(model_id, INTENT_PAUSE)
    }

    /// Stop and delete partial data.
    pub fn cancel(&self, model_id: &str) -> Result<()> {
        if self.signal(model_id, INTENT_CANCEL).is_err() {
            // Nothing is running: remove whatever is on disk.
            self.remove_part(model_id)?;
            self.inner.db.delete_download(model_id)?;
            self.emit_state(model_id, DownloadState::Cancelled, None);
        }
        Ok(())
    }

    fn signal(&self, model_id: &str, intent: u8) -> Result<()> {
        let tasks = self.inner.tasks.lock().map_err(lock_error)?;
        let control = tasks.get(model_id).ok_or(AppError::InvalidInput {
            detail: "no active download for this model".into(),
        })?;
        control.intent.store(intent, Ordering::SeqCst);
        control.cancel.cancel();
        Ok(())
    }

    pub fn is_active(&self, model_id: &str) -> bool {
        self.inner
            .tasks
            .lock()
            .map(|tasks| tasks.contains_key(model_id))
            .unwrap_or(false)
    }

    async fn run(&self, spec: ModelSpec, model_id: String) {
        let control = {
            let tasks = self.inner.tasks.lock();
            match tasks {
                Ok(tasks) => match tasks.get(&model_id) {
                    Some(control) => TaskControl {
                        cancel: control.cancel.clone(),
                        intent: control.intent.clone(),
                    },
                    None => return,
                },
                Err(_) => return,
            }
        };

        let _permit = match self.inner.gate.acquire().await {
            Ok(permit) => permit,
            Err(_) => return,
        };

        let result = self.download(&spec, &control).await;
        match result {
            Ok(()) => self.emit_state(&model_id, DownloadState::Installed, None),
            Err(error) => {
                let intent = control.intent.load(Ordering::SeqCst);
                if error.code() == "canceled" {
                    if intent == INTENT_PAUSE {
                        let done = self.part_size(&spec);
                        self.record_state(
                            &model_id,
                            DownloadState::Paused,
                            done,
                            spec.size_bytes,
                            None,
                        );
                        self.emit_state(&model_id, DownloadState::Paused, None);
                    } else {
                        let _ = self.remove_part(&model_id);
                        let _ = self.inner.db.delete_download(&model_id);
                        self.emit_state(&model_id, DownloadState::Cancelled, None);
                    }
                } else {
                    tracing::warn!(model_id = %model_id, error = %error, "download failed");
                    let done = self.part_size(&spec);
                    self.record_state(
                        &model_id,
                        DownloadState::Failed,
                        done,
                        spec.size_bytes,
                        None,
                    );
                    self.emit_state(&model_id, DownloadState::Failed, Some(error.payload()));
                }
            }
        }

        if let Ok(mut tasks) = self.inner.tasks.lock() {
            tasks.remove(&model_id);
        }
    }

    async fn download(&self, spec: &ModelSpec, control: &TaskControl) -> Result<()> {
        let url = spec.url.clone().ok_or(AppError::ModelUnvalidated)?;
        let part = self.inner.paths.part_path(&spec.file_name);

        self.emit_state(&spec.id, DownloadState::Downloading, None);

        let existing = std::fs::metadata(&part).map(|meta| meta.len()).unwrap_or(0);
        let mut hasher = if existing > 0 {
            let path = part.clone();
            tokio::task::spawn_blocking(move || sha256_state_of_file(&path))
                .await
                .map_err(|error| AppError::Storage {
                    detail: error.to_string(),
                })??
        } else {
            Sha256::new()
        };

        let mut request = self.inner.client.get(&url);
        if existing > 0 {
            request = request.header(RANGE, format!("bytes={existing}-"));
        }
        let response = request.send().await.map_err(|error| {
            if error.to_string().contains("allowlist") {
                AppError::DownloadBlocked {
                    detail: error.to_string(),
                }
            } else {
                AppError::from(error)
            }
        })?;

        let status = response.status();
        let content_length = response.content_length().unwrap_or(0);
        let mut done = existing;
        let total = spec.size_bytes.max(existing + content_length);

        let mut file = match status {
            StatusCode::PARTIAL_CONTENT => {
                verify_content_range(&response, existing)?;
                tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&part)
                    .await?
            }
            StatusCode::OK => {
                // The server ignored our Range header: restart cleanly.
                done = 0;
                hasher = Sha256::new();
                tokio::fs::File::create(&part).await?
            }
            StatusCode::RANGE_NOT_SATISFIABLE => {
                if existing == spec.size_bytes {
                    return self.finish(spec, &part, sha256_of_hasher(hasher)).await;
                }
                let _ = tokio::fs::remove_file(&part).await;
                return Err(AppError::DownloadFailed {
                    detail: "the download server rejected the resume request".into(),
                });
            }
            status => {
                return Err(map_download_status(status, &url));
            }
        };

        self.record_state(&spec.id, DownloadState::Downloading, done, total, None);
        self.emit_progress(&spec.id, done, total, 0, Some(estimate_eta(total, done, 0)));

        let started = Instant::now();
        let mut last_progress = Instant::now() - Duration::from_secs(1);
        let mut speed = 0_u64;
        let mut stream = response.bytes_stream();

        loop {
            tokio::select! {
                _ = control.cancel.cancelled() => {
                    file.flush().await?;
                    file.sync_all().await?;
                    return Err(AppError::Cancelled);
                }
                item = stream.next() => {
                    match item {
                        Some(Ok(chunk)) => {
                            file.write_all(&chunk).await?;
                            hasher.update(&chunk);
                            done += chunk.len() as u64;
                            let elapsed = started.elapsed().as_secs_f64();
                            if elapsed > 0.2 {
                                speed = ((done.saturating_sub(existing)) as f64 / elapsed) as u64;
                            }
                            if last_progress.elapsed() >= Duration::from_millis(250) {
                                last_progress = Instant::now();
                                self.record_state(&spec.id, DownloadState::Downloading, done, total, None);
                                self.emit_progress(
                                    &spec.id,
                                    done,
                                    total,
                                    speed,
                                    Some(estimate_eta(total, done, speed)),
                                );
                            }
                        }
                        Some(Err(error)) => {
                            file.flush().await?;
                            return Err(AppError::DownloadFailed {
                                detail: error.to_string(),
                            });
                        }
                        None => break,
                    }
                }
            }
        }

        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        self.emit_state(&spec.id, DownloadState::Verifying, None);

        if total > 0 && done < total {
            return Err(AppError::DownloadFailed {
                detail: format!("connection closed early ({done}/{total} bytes)"),
            });
        }

        self.finish(spec, &part, sha256_of_hasher(hasher)).await
    }

    async fn finish(&self, spec: &ModelSpec, part: &PathBuf, actual: String) -> Result<()> {
        let expected = spec
            .sha256
            .as_deref()
            .ok_or(AppError::ModelUnvalidated)?
            .to_ascii_lowercase();
        if actual != expected {
            let _ = tokio::fs::remove_file(part).await;
            return Err(AppError::DownloadChecksum);
        }

        let part_clone = part.clone();
        let expected_arch = spec.architecture.clone();
        tokio::task::spawn_blocking(move || {
            gguf::validate_gguf(&part_clone, expected_arch.as_deref())
        })
        .await
        .map_err(|error| AppError::Storage {
            detail: error.to_string(),
        })??;

        let destination = self.inner.paths.model_path(&spec.file_name);
        let part_clone = part.clone();
        let destination_clone = destination.clone();
        tokio::task::spawn_blocking(move || atomic_install(&part_clone, &destination_clone))
            .await
            .map_err(|error| AppError::Storage {
                detail: error.to_string(),
            })??;

        let installed = InstalledModel {
            id: spec.id.clone(),
            display_name: spec.display_name.clone(),
            file_name: spec.file_name.clone(),
            path: destination.to_string_lossy().to_string(),
            sha256: expected,
            size_bytes: spec.size_bytes,
            installed_at: now_millis(),
            last_used_at: None,
            source: ModelSource::Catalog,
        };
        self.inner.db.upsert_installed(&installed)?;
        self.inner.db.delete_download(&spec.id)?;
        self.emit_progress(&spec.id, spec.size_bytes, spec.size_bytes, 0, Some(0));
        Ok(())
    }

    /// Copy a user-selected GGUF into the managed models directory.
    pub async fn import(&self, source: PathBuf) -> Result<InstalledModel> {
        tokio::task::spawn_blocking({
            let source = source.clone();
            move || validate_import_source(&source)
        })
        .await
        .map_err(|error| AppError::Storage {
            detail: error.to_string(),
        })??;

        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("imported-model.gguf")
            .to_string();
        self.import_file(std::fs::File::open(source)?, file_name)
            .await
    }

    pub async fn import_file(
        &self,
        mut source: std::fs::File,
        file_name: String,
    ) -> Result<InstalledModel> {
        use std::io::{Seek, SeekFrom};

        let metadata = source.metadata()?;
        let size = metadata.len();
        if !metadata.is_file() || size < 1024 {
            return Err(AppError::ModelCorrupt {
                detail: "select a local GGUF model file".into(),
            });
        }
        source = tokio::task::spawn_blocking(move || {
            // Duplicated file descriptors share their cursor. Rewind before
            // the full copy so the installed model retains its GGUF header.
            source.seek(SeekFrom::Start(0))?;
            gguf::read_gguf_file(source.try_clone()?)?;
            source.seek(SeekFrom::Start(0))?;
            Ok::<_, AppError>(source)
        })
        .await
        .map_err(|error| AppError::Storage {
            detail: error.to_string(),
        })??;

        ensure_disk_space(&self.inner.paths.models_dir, size + 512 * 1024 * 1024)?;

        let destination = unique_destination(&self.inner.paths.models_dir, &file_name);

        // Once copying starts, finish the install and DB registration in the
        // same blocking task even if the IPC future is dropped. A cancelled
        // waiter must not leave a complete but unregistered model on disk.
        let db = self.inner.db.clone();
        tokio::task::spawn_blocking(move || {
            let hash = copy_file_and_hash(source, &destination, |_, _| {})?;
            let stem = destination
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Imported model")
                .to_string();
            let installed = InstalledModel {
                id: format!("imported-{}", &hash[..16]),
                display_name: stem,
                file_name: destination
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("imported-model.gguf")
                    .to_string(),
                path: destination.to_string_lossy().to_string(),
                sha256: hash,
                size_bytes: size,
                installed_at: now_millis(),
                last_used_at: None,
                source: ModelSource::Imported,
            };
            if let Err(error) = db.upsert_installed(&installed) {
                let _ = std::fs::remove_file(&destination);
                return Err(error);
            }
            Ok(installed)
        })
        .await
        .map_err(|error| AppError::Storage {
            detail: error.to_string(),
        })?
    }

    fn part_size(&self, spec: &ModelSpec) -> u64 {
        std::fs::metadata(self.inner.paths.part_path(&spec.file_name))
            .map(|meta| meta.len())
            .unwrap_or(0)
    }

    fn remove_part(&self, model_id: &str) -> Result<()> {
        if let Some(spec) = self.inner.catalog.get(model_id) {
            let _ = std::fs::remove_file(self.inner.paths.part_path(&spec.file_name));
        }
        Ok(())
    }

    fn record_state(
        &self,
        model_id: &str,
        state: DownloadState,
        bytes_done: u64,
        bytes_total: u64,
        etag: Option<String>,
    ) {
        let record = DownloadRecord {
            model_id: model_id.to_string(),
            state: state.as_str().to_string(),
            bytes_done,
            bytes_total,
            etag,
            updated_at: now_millis(),
        };
        if let Err(error) = self.inner.db.save_download(&record) {
            tracing::warn!(%error, "could not persist download state");
        }
    }

    fn emit_state(
        &self,
        model_id: &str,
        state: DownloadState,
        error: Option<crate::errors::ErrorPayload>,
    ) {
        self.inner
            .events
            .emit(CoreEvent::DownloadState(DownloadStateEvent {
                model_id: model_id.to_string(),
                state,
                error,
            }));
    }

    fn emit_progress(
        &self,
        model_id: &str,
        bytes_done: u64,
        bytes_total: u64,
        speed_bps: u64,
        eta_secs: Option<u64>,
    ) {
        self.inner
            .events
            .emit(CoreEvent::DownloadProgress(DownloadProgress {
                model_id: model_id.to_string(),
                bytes_done,
                bytes_total,
                speed_bps,
                eta_secs,
            }));
    }
}

fn lock_error<T>(_: T) -> AppError {
    AppError::Storage {
        detail: "download task map is poisoned".into(),
    }
}

fn sha256_of_hasher(hasher: Sha256) -> String {
    hex::encode(hasher.finalize())
}

fn estimate_eta(total: u64, done: u64, speed: u64) -> u64 {
    if speed == 0 || done >= total {
        0
    } else {
        (total - done) / speed
    }
}

fn verify_content_range(response: &reqwest::Response, expected_start: u64) -> Result<()> {
    let value = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::DownloadFailed {
            detail: "resume response is missing Content-Range".into(),
        })?;
    // bytes <start>-<end>/<total>
    let start = value
        .strip_prefix("bytes ")
        .and_then(|rest| rest.split('-').next())
        .and_then(|start| start.parse::<u64>().ok())
        .ok_or_else(|| AppError::DownloadFailed {
            detail: format!("invalid Content-Range: {value}"),
        })?;
    if start != expected_start {
        return Err(AppError::DownloadFailed {
            detail: format!("server resumed at {start}, expected {expected_start}"),
        });
    }
    Ok(())
}

fn map_download_status(status: StatusCode, url: &str) -> AppError {
    let detail = match status {
        StatusCode::FORBIDDEN => "the download server refused access (HTTP 403)".to_string(),
        StatusCode::NOT_FOUND => format!("the model file was not found (HTTP 404): {url}"),
        StatusCode::TOO_MANY_REQUESTS => "the download server is busy (HTTP 429)".to_string(),
        StatusCode::RANGE_NOT_SATISFIABLE => "invalid resume range (HTTP 416)".to_string(),
        other => format!("download server returned HTTP {other}"),
    };
    AppError::DownloadFailed { detail }
}

/// Rebuild the SHA-256 state of an existing partial file. Resuming must hash
/// the bytes already on disk, not seed the hasher with a previous digest.
fn sha256_state_of_file(path: &std::path::Path) -> Result<Sha256> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::with_capacity(256 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher)
}
