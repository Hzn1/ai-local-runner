//! Filesystem layout owned by the core.
//!
//! Everything the app writes lives below a single data directory so that
//! "reset app" and diagnostics are simple, boring operations.

use std::path::{Path, PathBuf};

use crate::errors::Result;

/// Resolved, created-on-demand application paths.
#[derive(Debug, Clone)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub models_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub exports_dir: PathBuf,
    pub engine_dir: PathBuf,
    pub baseline_engine_dir: PathBuf,
    pub db_path: PathBuf,
    pub pid_file: PathBuf,
}

impl Paths {
    /// Build the layout and create the writable directories.
    ///
    /// `data_dir` should be the platform app-data directory. `engine_dir` and
    /// `baseline_engine_dir` are Tauri resources, not app data.
    pub fn new(
        data_dir: impl Into<PathBuf>,
        engine_dir: impl Into<PathBuf>,
        baseline_engine_dir: impl Into<PathBuf>,
    ) -> Result<Self> {
        let data_dir = data_dir.into();
        let paths = Self {
            models_dir: data_dir.join("models"),
            downloads_dir: data_dir.join("downloads"),
            logs_dir: data_dir.join("logs"),
            exports_dir: data_dir.join("exports"),
            db_path: data_dir.join("tnn.sqlite3"),
            pid_file: data_dir.join("engine.pid"),
            data_dir,
            engine_dir: engine_dir.into(),
            baseline_engine_dir: baseline_engine_dir.into(),
        };
        paths.create_dirs()?;
        Ok(paths)
    }

    pub fn create_dirs(&self) -> Result<()> {
        for dir in [
            &self.data_dir,
            &self.models_dir,
            &self.downloads_dir,
            &self.logs_dir,
            &self.exports_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    /// Path of the partially-downloaded file for a model.
    pub fn part_path(&self, file_name: &str) -> PathBuf {
        self.downloads_dir.join(format!("{file_name}.part"))
    }

    pub fn model_path(&self, file_name: &str) -> PathBuf {
        self.models_dir.join(file_name)
    }

    /// The engine executable for the current platform.
    pub fn engine_executable(&self, dir: &Path) -> PathBuf {
        if cfg!(windows) {
            dir.join("llama-server.exe")
        } else {
            dir.join("llama-server")
        }
    }

    pub fn app_log_glob(&self) -> PathBuf {
        self.logs_dir.join("tnn.log*")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::new(
            tmp.path(),
            tmp.path().join("engine"),
            tmp.path().join("base"),
        )
        .unwrap();
        assert!(paths.models_dir.is_dir());
        assert!(paths.downloads_dir.is_dir());
        assert!(paths.logs_dir.is_dir());
        assert_eq!(
            paths.model_path("a.gguf"),
            tmp.path().join("models").join("a.gguf")
        );
    }
}
