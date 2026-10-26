//! Atomic installation and local import.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::errors::{AppError, Result};

/// Move a verified download into its final location.
///
/// The temp file is fsynced before the rename and the parent directory is
/// fsynced afterwards on Unix, so a crash cannot leave a truncated file with a
/// valid final name.
pub fn atomic_install(temp: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    // Flush the file contents to stable storage first.
    {
        // Windows FlushFileBuffers requires GENERIC_WRITE on the handle.
        let file = fs::OpenOptions::new().write(true).open(temp)?;
        file.sync_all()?;
    }

    // std::fs::rename replaces an existing file on both Unix and Windows.
    // Do not delete the previous model before the replacement succeeds.
    fs::rename(temp, destination)?;

    #[cfg(unix)]
    if let Some(parent) = destination.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

/// Pick a non-colliding destination for an imported file.
pub fn unique_destination(models_dir: &Path, file_name: &str) -> PathBuf {
    let safe_name = Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.to_ascii_lowercase().ends_with(".gguf"))
        .unwrap_or("imported-model.gguf");

    let stem = Path::new(safe_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("imported-model");
    let mut candidate = models_dir.join(safe_name);
    let mut counter = 1;
    while candidate.exists() {
        candidate = models_dir.join(format!("{stem}-{counter}.gguf"));
        counter += 1;
    }
    candidate
}

/// Copy a source GGUF to `destination`, hashing while copying and reporting
/// progress. The destination is written as `<destination>.part` and renamed on
/// success, so a cancelled import never looks installed.
pub fn copy_and_hash(
    source: &Path,
    destination: &Path,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<String> {
    let total = fs::metadata(source)?.len();
    let part = destination.with_extension("gguf.part");
    let result = (|| -> Result<String> {
        let input = File::open(source)?;
        let output = File::create(&part)?;
        let mut reader = BufReader::with_capacity(1024 * 1024, input);
        let mut writer = BufWriter::with_capacity(1024 * 1024, output);
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; 1024 * 1024];
        let mut done = 0_u64;
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            writer.write_all(&buffer[..read])?;
            hasher.update(&buffer[..read]);
            done += read as u64;
            on_progress(done, total);
        }
        writer.flush()?;
        writer.get_ref().sync_all()?;
        Ok(hex::encode(hasher.finalize()))
    })();

    match result {
        Ok(hash) => {
            atomic_install(&part, destination)?;
            Ok(hash)
        }
        Err(error) => {
            let _ = fs::remove_file(&part);
            Err(error)
        }
    }
}

/// Basic sanity checks before copying an arbitrary file chosen by the user.
pub fn validate_import_source(path: &Path) -> Result<u64> {
    let metadata = fs::metadata(path).map_err(|_| AppError::ModelNotFound)?;
    if !metadata.is_file() {
        return Err(AppError::InvalidInput {
            detail: "the selected path is not a file".into(),
        });
    }
    if metadata.len() < 1024 {
        return Err(AppError::ModelCorrupt {
            detail: "the selected file is too small to be a model".into(),
        });
    }
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("gguf"))
    {
        return Err(AppError::ModelUnsupported {
            detail: "only .gguf model files can be imported".into(),
        });
    }
    Ok(metadata.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_install_moves_file() {
        let dir = tempfile::tempdir().unwrap();
        let temp = dir.path().join("a.part");
        let dest = dir.path().join("sub").join("a.gguf");
        fs::write(&temp, b"payload").unwrap();
        atomic_install(&temp, &dest).unwrap();
        assert!(!temp.exists());
        assert_eq!(fs::read(&dest).unwrap(), b"payload");
    }

    #[test]
    fn atomic_install_replaces_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let temp = dir.path().join("a.part");
        let dest = dir.path().join("a.gguf");
        fs::write(&temp, b"verified replacement").unwrap();
        fs::write(&dest, b"previous model").unwrap();
        atomic_install(&temp, &dest).unwrap();
        assert!(!temp.exists());
        assert_eq!(fs::read(&dest).unwrap(), b"verified replacement");
    }

    #[test]
    fn copy_hashes_and_renames() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("in.gguf");
        fs::write(&source, b"model data").unwrap();
        let dest = dir.path().join("out.gguf");
        let mut seen = 0;
        let hash = copy_and_hash(&source, &dest, |done, _| seen = done).unwrap();
        assert_eq!(seen, 10);
        assert!(dest.exists());
        assert_eq!(hash.len(), 64);
        assert!(!dest.with_extension("gguf.part").exists());
    }

    #[test]
    fn unique_destination_avoids_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("model.gguf"), b"x").unwrap();
        let dest = unique_destination(dir.path(), "model.gguf");
        assert_eq!(dest.file_name().unwrap(), "model-1.gguf");
    }

    #[test]
    fn import_source_requires_gguf_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.bin");
        fs::write(&path, vec![0_u8; 2048]).unwrap();
        assert!(validate_import_source(&path).is_err());
    }
}
