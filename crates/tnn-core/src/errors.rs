//! Typed application errors with stable machine codes and plain-language
//! recovery guidance.
//!
//! Every user-facing failure must map to one of these variants. The English
//! strings here are a fallback: the frontend localises by `code`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A concrete next step the UI can offer to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RecoveryAction {
    Retry,
    RestartEngine,
    RestartApp,
    FreeMemory,
    ChooseSmallerModel,
    Redownload,
    ImportFile,
    FreeDiskSpace,
    ExportDiagnostics,
    NewChat,
    ReduceContext,
    CompatibilityMode,
    OpenDocumentation,
}

/// Wire representation of an error. The frontend renders `message_key`
/// (`error.<code>`) and offers the recovery actions.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
    pub recovery: Vec<RecoveryAction>,
    pub retryable: bool,
}

/// The single error type used inside `tnn-core`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("the AI engine could not start: {detail}")]
    EngineSpawnFailed { detail: String },
    #[error("the AI engine stopped unexpectedly: {detail}")]
    EngineCrash { detail: String },
    #[error("the AI engine took too long to start")]
    EngineTimeout,
    #[error("this PC is missing a CPU feature required by the AI engine")]
    EngineIllegalInstruction,
    #[error("the AI engine produced invalid output")]
    EngineGarbageOutput,
    #[error("the AI engine could not bind a local port: {detail}")]
    EnginePortConflict { detail: String },
    #[error("the model is not installed")]
    ModelNotInstalled,
    #[error("the model file is missing")]
    ModelNotFound,
    #[error("the model file is damaged: {detail}")]
    ModelCorrupt { detail: String },
    #[error("this model is not supported: {detail}")]
    ModelUnsupported { detail: String },
    #[error("this model has not passed the validation gate")]
    ModelUnvalidated,
    #[error("not enough disk space: need {required} bytes, {available} available")]
    DiskFull { required: u64, available: u64 },
    #[error("download failed: {detail}")]
    DownloadFailed { detail: String },
    #[error("the download was incomplete or damaged")]
    DownloadChecksum,
    #[error("the download source is not allowed: {detail}")]
    DownloadBlocked { detail: String },
    #[error("the model needs more memory than is free")]
    OutOfMemory,
    #[error("this conversation is too long for the model's memory")]
    ContextOverflow,
    #[error("generation stopped")]
    Cancelled,
    #[error("this version of the app does not support this PC: {detail}")]
    UnsupportedPlatform { detail: String },
    #[error("storage error: {detail}")]
    Storage { detail: String },
    #[error("network error: {detail}")]
    Network { detail: String },
    #[error("invalid input: {detail}")]
    InvalidInput { detail: String },
    #[error("{detail}")]
    Other { detail: String },
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::EngineSpawnFailed { .. } => "engine_spawn_failed",
            Self::EngineCrash { .. } => "engine_crash",
            Self::EngineTimeout => "engine_timeout",
            Self::EngineIllegalInstruction => "engine_illegal_instruction",
            Self::EngineGarbageOutput => "engine_garbage_output",
            Self::EnginePortConflict { .. } => "engine_port_conflict",
            Self::ModelNotInstalled => "model_not_installed",
            Self::ModelNotFound => "model_not_found",
            Self::ModelCorrupt { .. } => "model_corrupt",
            Self::ModelUnsupported { .. } => "model_unsupported",
            Self::ModelUnvalidated => "model_unvalidated",
            Self::DiskFull { .. } => "disk_full",
            Self::DownloadFailed { .. } => "download_failed",
            Self::DownloadChecksum => "download_checksum",
            Self::DownloadBlocked { .. } => "download_blocked",
            Self::OutOfMemory => "out_of_memory",
            Self::ContextOverflow => "context_overflow",
            Self::Cancelled => "canceled",
            Self::UnsupportedPlatform { .. } => "unsupported_platform",
            Self::Storage { .. } => "storage_error",
            Self::Network { .. } => "network_error",
            Self::InvalidInput { .. } => "invalid_input",
            Self::Other { .. } => "unknown",
        }
    }

    pub fn recovery(&self) -> Vec<RecoveryAction> {
        use RecoveryAction as R;
        match self {
            Self::EngineSpawnFailed { .. } | Self::EnginePortConflict { .. } => {
                vec![R::Retry, R::RestartApp, R::ExportDiagnostics]
            }
            Self::EngineCrash { .. } => vec![R::RestartEngine, R::ExportDiagnostics],
            Self::EngineTimeout => vec![R::Retry, R::ChooseSmallerModel],
            Self::EngineIllegalInstruction | Self::EngineGarbageOutput => {
                vec![R::CompatibilityMode, R::ExportDiagnostics]
            }
            Self::ModelNotInstalled => vec![R::Redownload],
            Self::ModelNotFound => vec![R::Redownload],
            Self::ModelCorrupt { .. } => vec![R::Redownload],
            Self::ModelUnsupported { .. } => vec![R::ChooseSmallerModel],
            Self::ModelUnvalidated => vec![R::OpenDocumentation],
            Self::DiskFull { .. } => vec![R::FreeDiskSpace, R::ChooseSmallerModel],
            Self::DownloadFailed { .. } => vec![R::Retry],
            Self::DownloadChecksum => vec![R::Retry],
            Self::DownloadBlocked { .. } => vec![R::OpenDocumentation],
            Self::OutOfMemory => vec![R::ChooseSmallerModel, R::FreeMemory],
            Self::ContextOverflow => vec![R::NewChat, R::ReduceContext],
            Self::Cancelled => vec![R::Retry],
            Self::UnsupportedPlatform { .. } => vec![R::OpenDocumentation],
            Self::Storage { .. } => vec![R::ExportDiagnostics],
            Self::Network { .. } => vec![R::Retry, R::ImportFile],
            Self::InvalidInput { .. } => vec![R::Retry],
            Self::Other { .. } => vec![R::ExportDiagnostics],
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::EngineSpawnFailed { .. }
                | Self::EngineCrash { .. }
                | Self::EngineTimeout
                | Self::DownloadFailed { .. }
                | Self::DownloadChecksum
                | Self::Network { .. }
                | Self::Cancelled
        )
    }

    pub fn payload(&self) -> ErrorPayload {
        ErrorPayload {
            code: self.code().to_string(),
            message: self.to_string(),
            detail: self.technical_detail(),
            recovery: self.recovery(),
            retryable: self.retryable(),
        }
    }

    fn technical_detail(&self) -> Option<String> {
        match self {
            Self::EngineSpawnFailed { detail }
            | Self::EngineCrash { detail }
            | Self::EnginePortConflict { detail }
            | Self::ModelCorrupt { detail }
            | Self::ModelUnsupported { detail }
            | Self::DownloadFailed { detail }
            | Self::DownloadBlocked { detail }
            | Self::UnsupportedPlatform { detail }
            | Self::Storage { detail }
            | Self::Network { detail }
            | Self::InvalidInput { detail }
            | Self::Other { detail } => Some(detail.clone()),
            Self::DiskFull {
                required,
                available,
            } => Some(format!("required={required} available={available}")),
            _ => None,
        }
    }
}

impl From<AppError> for ErrorPayload {
    fn from(value: AppError) -> Self {
        value.payload()
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::Storage {
            detail: value.to_string(),
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Storage {
            detail: value.to_string(),
        }
    }
}

impl From<rusqlite_migration::Error> for AppError {
    fn from(value: rusqlite_migration::Error) -> Self {
        Self::Storage {
            detail: value.to_string(),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::Storage {
            detail: value.to_string(),
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(value: reqwest::Error) -> Self {
        if value.is_timeout() || value.is_connect() {
            Self::Network {
                detail: value.to_string(),
            }
        } else if value.is_decode() {
            Self::DownloadFailed {
                detail: value.to_string(),
            }
        } else {
            Self::Network {
                detail: value.to_string(),
            }
        }
    }
}

pub type Result<T, E = AppError> = std::result::Result<T, E>;
