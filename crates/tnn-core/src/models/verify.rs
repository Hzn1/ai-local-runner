//! Integrity and filesystem guards used before trusting a model file.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use reqwest::Url;
use sha2::{Digest, Sha256};

use crate::errors::{AppError, Result};

/// Hosts allowed to serve model weights. Checked on the initial URL *and* on
/// every redirect hop.
pub fn is_allowed_model_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "huggingface.co"
        || host.ends_with(".huggingface.co")
        || host == "hf.co"
        || host.ends_with(".hf.co")
        || host == "github.com"
        || host == "objects.githubusercontent.com"
        || host == "release-assets.githubusercontent.com"
}

pub fn is_allowed_model_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    parsed.host_str().is_some_and(is_allowed_model_host)
}

/// SHA-256 of a file, computed in 64 KiB chunks.
pub fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let actual = sha256_file(path)?;
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(AppError::DownloadChecksum)
    }
}

/// Free space on the volume containing `path`.
pub fn free_disk_bytes(path: &Path) -> u64 {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut best: Option<(usize, u64)> = None;
    for disk in disks.list() {
        let mount = disk.mount_point();
        if path.starts_with(mount) {
            let depth = mount.components().count();
            if best.is_none_or(|(d, _)| depth > d) {
                best = Some((depth, disk.available_space()));
            }
        }
    }
    best.map(|(_, free)| free).unwrap_or_else(|| {
        disks
            .list()
            .iter()
            .map(|disk| disk.available_space())
            .max()
            .unwrap_or(0)
    })
}

pub fn ensure_disk_space(path: &Path, required: u64) -> Result<()> {
    let available = free_disk_bytes(path);
    if available < required {
        return Err(AppError::DiskFull {
            required,
            available,
        });
    }
    Ok(())
}

/// Validate a model id before using it in a path or SQL statement.
pub fn validate_model_id(id: &str) -> Result<()> {
    let valid = !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if valid {
        Ok(())
    } else {
        Err(AppError::InvalidInput {
            detail: format!("unsafe model id {id:?}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_accepts_only_https_and_known_hosts() {
        assert!(is_allowed_model_url(
            "https://huggingface.co/x/y/resolve/abc/z.gguf"
        ));
        assert!(is_allowed_model_url(
            "https://cdn-lfs.huggingface.co/x/y/z.gguf"
        ));
        assert!(is_allowed_model_url(
            "https://cas-bridge.xethub.hf.co/xet-bridge-us/abc"
        ));
        assert!(!is_allowed_model_url("http://huggingface.co/x.gguf"));
        assert!(!is_allowed_model_url("https://evil.example/x.gguf"));
        assert!(!is_allowed_model_url(
            "https://huggingface.co.evil.example/x.gguf"
        ));
        assert!(!is_allowed_model_url("file:///etc/passwd"));
    }

    #[test]
    fn hashes_a_known_string() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello");
        std::fs::write(&path, b"hello").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert!(verify_sha256(&path, &"0".repeat(64)).is_err());
    }

    #[test]
    fn rejects_traversal_ids() {
        assert!(validate_model_id("qwen2-5-1-5b-instruct-q4-k-m").is_ok());
        assert!(validate_model_id("../etc").is_err());
        assert!(validate_model_id("a/b").is_err());
        assert!(validate_model_id("").is_err());
    }
}
