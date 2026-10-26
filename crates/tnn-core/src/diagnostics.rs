//! User-initiated diagnostics export.
//!
//! The archive never contains prompts, API keys, or full home-directory paths.
//! A conversation is included only when the caller explicitly passes one.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;

use crate::errors::{AppError, Result};
use crate::hardware::HardwareProfile;
use crate::paths::Paths;
use crate::storage::{Conversation, DownloadRecord, InstalledModel, Settings, now_millis};

const MAX_LOG_LINES: usize = 500;

#[derive(Debug, Clone, Serialize)]
struct ErrorRecord {
    at: i64,
    code: String,
    message: String,
}

pub struct Diagnostics {
    paths: Paths,
    app_version: String,
    engine_version: String,
    recent_errors: Mutex<VecDeque<ErrorRecord>>,
}

pub struct DiagnosticsContext<'a> {
    pub hardware: &'a HardwareProfile,
    pub settings: &'a Settings,
    pub installed: &'a [InstalledModel],
    pub downloads: &'a [DownloadRecord],
    pub engine_log: &'a [String],
    pub conversation: Option<&'a Conversation>,
}

impl Diagnostics {
    pub fn new(
        paths: Paths,
        app_version: impl Into<String>,
        engine_version: impl Into<String>,
    ) -> Self {
        Self {
            paths,
            app_version: app_version.into(),
            engine_version: engine_version.into(),
            recent_errors: Mutex::new(VecDeque::with_capacity(20)),
        }
    }

    pub fn record_error(&self, error: &AppError) {
        if let Ok(mut errors) = self.recent_errors.lock() {
            if errors.len() >= 20 {
                errors.pop_front();
            }
            errors.push_back(ErrorRecord {
                at: now_millis(),
                code: error.code().to_string(),
                message: error.to_string(),
            });
        }
    }

    pub fn export(&self, context: DiagnosticsContext<'_>) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.paths.exports_dir)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let path = self
            .paths
            .exports_dir
            .join(format!("tnn-diagnostics-{stamp}.zip"));
        let file = File::create(&path)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        let summary = serde_json::json!({
            "app_version": self.app_version,
            "engine_version": self.engine_version,
            "catalog_version": crate::CATALOG_VERSION,
            "exported_at": now_millis(),
            "os": context.hardware.os,
            "arch": context.hardware.arch,
            "cpu": context.hardware.cpu_brand,
            "physical_cores": context.hardware.physical_cores,
            "logical_cores": context.hardware.logical_cores,
            "total_ram_bytes": context.hardware.total_ram_bytes,
            "available_ram_bytes": context.hardware.available_ram_bytes,
            "free_disk_bytes": context.hardware.free_disk_bytes,
            "minimum_mode": context.hardware.minimum_mode,
            "hardware_tier": context.hardware.tier,
            "cpu_features": context.hardware.features,
            "engine_tier": context.hardware.features.best_engine_tier(),
            "notes": context.hardware.notes,
        });
        write_json(&mut zip, options, "summary.json", &summary)?;

        // System prompt and any free-text user setting are redacted.
        let mut settings = serde_json::to_value(context.settings)?;
        if let Some(object) = settings.as_object_mut() {
            if object.contains_key("system_prompt") {
                object.insert(
                    "system_prompt".into(),
                    serde_json::Value::String("[redacted]".into()),
                );
            }
        }
        write_json(&mut zip, options, "settings.json", &settings)?;

        let installed: Vec<_> = context
            .installed
            .iter()
            .map(|model| {
                let mut value = serde_json::to_value(model).unwrap_or_default();
                if let Some(object) = value.as_object_mut() {
                    object.insert(
                        "path".into(),
                        serde_json::Value::String(self.redact_path(&model.path)),
                    );
                }
                value
            })
            .collect();
        write_json(&mut zip, options, "models.json", &installed)?;
        write_json(&mut zip, options, "downloads.json", &context.downloads)?;

        let errors: Vec<ErrorRecord> = self
            .recent_errors
            .lock()
            .map(|errors| errors.iter().cloned().collect())
            .unwrap_or_default();
        write_json(&mut zip, options, "errors.json", &errors)?;

        let app_log = read_log_tail(&self.paths.logs_dir, MAX_LOG_LINES);
        write_text(&mut zip, options, "app-log.txt", &app_log)?;
        write_text(
            &mut zip,
            options,
            "engine.log",
            &context.engine_log.join("\n"),
        )?;

        if let Some(conversation) = context.conversation {
            write_json(&mut zip, options, "conversation.json", conversation)?;
        }

        write_text(
            &mut zip,
            options,
            "README.txt",
            "TNN Local AI diagnostics.\n\
             Contains no prompts or API keys. System prompt and file paths are redacted.\n\
             A conversation is included only when the user explicitly asked for it.\n",
        )?;

        zip.finish().map_err(|error| AppError::Storage {
            detail: error.to_string(),
        })?;
        Ok(path)
    }

    fn redact_path(&self, path: &str) -> String {
        let data_prefix = self.paths.data_dir.to_string_lossy().to_string();
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_default();
        if !data_prefix.is_empty() {
            if let Some(rest) = path.strip_prefix(&data_prefix) {
                return format!("<data>{rest}");
            }
        }
        if !home.is_empty() {
            if let Some(rest) = path.strip_prefix(&home) {
                return format!("~{rest}");
            }
        }
        path.to_string()
    }
}

fn write_json<T: Serialize>(
    zip: &mut zip::ZipWriter<File>,
    options: zip::write::SimpleFileOptions,
    name: &str,
    value: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    zip.start_file(name.to_string(), options)
        .map_err(|error| AppError::Storage {
            detail: error.to_string(),
        })?;
    zip.write_all(&bytes)?;
    Ok(())
}

fn write_text(
    zip: &mut zip::ZipWriter<File>,
    options: zip::write::SimpleFileOptions,
    name: &str,
    value: &str,
) -> Result<()> {
    zip.start_file(name.to_string(), options)
        .map_err(|error| AppError::Storage {
            detail: error.to_string(),
        })?;
    zip.write_all(value.as_bytes())?;
    Ok(())
}

/// Last `max_lines` lines across the rotating app log files.
fn read_log_tail(dir: &std::path::Path, max_lines: usize) -> String {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return String::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("tnn.log"))
        })
        .collect();
    files.sort();
    let mut lines: VecDeque<String> = VecDeque::with_capacity(max_lines);
    for path in files {
        let Ok(contents) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in contents.lines() {
            if lines.len() >= max_lines {
                lines.pop_front();
            }
            lines.push_back(line.to_string());
        }
    }
    lines.into_iter().collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::HardwareProfile;

    #[test]
    fn exports_redacted_archive() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path(), dir.path().join("e"), dir.path().join("b")).unwrap();
        let diagnostics = Diagnostics::new(paths.clone(), "0.1.0", "engine-pin");
        diagnostics.record_error(&AppError::EngineTimeout);

        let settings = Settings::default();
        let hardware = HardwareProfile::synthetic_minimum();
        let installed = vec![InstalledModel {
            id: "m".into(),
            display_name: "M".into(),
            file_name: "m.gguf".into(),
            path: paths
                .models_dir
                .join("m.gguf")
                .to_string_lossy()
                .to_string(),
            sha256: "a".repeat(64),
            size_bytes: 1,
            installed_at: 0,
            last_used_at: None,
            source: crate::storage::ModelSource::Catalog,
        }];
        let export = diagnostics
            .export(DiagnosticsContext {
                hardware: &hardware,
                settings: &settings,
                installed: &installed,
                downloads: &[],
                engine_log: &["line".to_string()],
                conversation: None,
            })
            .unwrap();
        assert!(export.exists());

        let file = File::open(&export).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut names = Vec::new();
        for index in 0..archive.len() {
            names.push(archive.by_index(index).unwrap().name().to_string());
        }
        assert!(names.contains(&"summary.json".to_string()));
        assert!(names.contains(&"app-log.txt".to_string()));
        assert!(!names.contains(&"conversation.json".to_string()));

        let mut settings_file = archive.by_name("settings.json").unwrap();
        let mut contents = String::new();
        std::io::Read::read_to_string(&mut settings_file, &mut contents).unwrap();
        assert!(contents.contains("[redacted]"));
        assert!(!contents.contains("You are a helpful assistant"));
    }
}
