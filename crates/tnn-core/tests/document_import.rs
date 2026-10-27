//! Android document pickers grant file descriptors, not ordinary paths.
use std::fs::{self, File};
use std::io::{Seek, SeekFrom};

use tnn_core::events::EventSink;
use tnn_core::models::verify::sha256_file;
use tnn_core::{AppCore, Paths};

#[tokio::test]
async fn descriptor_import_preserves_the_header_hash_and_database_record() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("document-without-extension");
    let mut bytes = b"GGUF".to_vec();
    bytes.extend_from_slice(&3_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u64.to_le_bytes()); // nonempty tensor declaration
    bytes.extend_from_slice(&0_u64.to_le_bytes()); // no metadata needed here
    bytes.resize(2048, 0);
    fs::write(&source, &bytes).unwrap();
    let paths = Paths::new(
        root.path().join("data"),
        root.path().join("engine"),
        root.path().join("base"),
    )
    .unwrap();
    let core = AppCore::new(paths, EventSink::noop(), "test").unwrap();
    let mut file = File::open(&source).unwrap();
    file.seek(SeekFrom::Start(100)).unwrap();
    let installed = core.import_model_file(file).await.unwrap();
    assert_eq!(fs::read(&installed.path).unwrap(), bytes);
    assert_eq!(installed.sha256, sha256_file(&source).unwrap());
    assert_eq!(installed.size_bytes, 2048);
    assert_eq!(core.db.installed_models().unwrap().len(), 1);
    assert!(
        !std::path::Path::new(&installed.path)
            .with_extension("gguf.part")
            .exists()
    );
}

#[tokio::test]
async fn descriptor_import_rejects_non_gguf_without_installing_it() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("invalid");
    fs::write(&source, vec![0_u8; 2048]).unwrap();
    let paths = Paths::new(
        root.path().join("data"),
        root.path().join("engine"),
        root.path().join("base"),
    )
    .unwrap();
    let core = AppCore::new(paths.clone(), EventSink::noop(), "test").unwrap();
    assert!(
        core.import_model_file(File::open(source).unwrap())
            .await
            .is_err()
    );
    assert!(core.db.installed_models().unwrap().is_empty());
    assert_eq!(fs::read_dir(paths.models_dir).unwrap().count(), 0);
}
