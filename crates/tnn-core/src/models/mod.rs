pub mod catalog;
pub mod download;
pub mod gguf;
pub mod install;
pub mod verify;

pub use catalog::{
    Catalog, ModelCard, ModelSpec, ModelTier, Sampling, ValidationStatus, is_sha256_hex,
};
pub use download::DownloadManager;
pub use gguf::{GgufInfo, read_gguf_info, validate_gguf};
