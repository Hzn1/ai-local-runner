//! Opt-in Phase 0 gate. Never downloads weights or requires them in the default suite.

#![cfg(any(target_os = "linux", target_os = "windows"))]

use std::path::{Path, PathBuf};
use std::time::Duration;

use tnn_core::Paths;
use tnn_core::chat::ChatMessage;
use tnn_core::engine::{EngineLaunch, EngineManager, selftest};
use tnn_core::events::{EngineState, EventSink};
use tnn_core::hardware::HardwareProfile;
use tnn_core::models::catalog::Catalog;
use tnn_core::models::{gguf, verify};
use tnn_core::storage::{InstalledModel, ModelSource, Settings};
use tokio_util::sync::CancellationToken;

/// Copy the resource payload into an isolated directory, keeping exactly the
/// requested CPU backend. Copying also resolves library symlinks on Linux.
fn stage_variant(source: &Path, destination: &Path, variant: &str) {
    std::fs::create_dir_all(destination).unwrap();
    let (prefix, extension) = if cfg!(windows) {
        ("ggml-cpu-", "dll")
    } else {
        ("libggml-cpu-", "so")
    };
    let backend = format!("{prefix}{variant}.{extension}");
    assert!(source.join(&backend).is_file(), "missing {variant} backend");
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let text = name.to_string_lossy();
        if entry.path().is_file() && (!text.starts_with(prefix) || text == backend) {
            std::fs::copy(entry.path(), destination.join(name)).unwrap();
        }
    }
}

#[tokio::test]
#[ignore = "requires TNN_TEST_MODEL and the pinned engine resources; see docs/ENGINE.md"]
async fn pinned_model_runs_on_native_sse2_and_ssse3() {
    let model = PathBuf::from(std::env::var_os("TNN_TEST_MODEL").expect("set TNN_TEST_MODEL"));
    let model = model.canonicalize().unwrap();
    let catalog = Catalog::embedded().unwrap();
    let model_id = std::env::var("TNN_TEST_MODEL_ID")
        .unwrap_or_else(|_| "qwen2-5-1-5b-instruct-q4-k-m".into());
    let spec = catalog.get(&model_id).unwrap();
    verify::verify_sha256(&model, spec.sha256.as_deref().unwrap()).unwrap();
    gguf::validate_gguf(&model, spec.architecture.as_deref()).unwrap();
    assert_eq!(std::fs::metadata(&model).unwrap().len(), spec.size_bytes);

    let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src-tauri/resources")
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let hardware = HardwareProfile::detect(temp.path()).unwrap();
    assert!(hardware.features.sse2, "SSE2 is required");
    assert!(hardware.features.ssse3, "this gate also exercises SSSE3");

    let installed = InstalledModel {
        id: spec.id.clone(),
        display_name: spec.display_name.clone(),
        file_name: spec.file_name.clone(),
        path: model.to_string_lossy().into_owned(),
        sha256: spec.sha256.clone().unwrap(),
        size_bytes: spec.size_bytes,
        installed_at: 0,
        last_used_at: None,
        source: ModelSource::Catalog,
    };
    let settings = Settings {
        context_size: Some(2048),
        max_tokens: Some(16),
        ..Settings::default()
    };
    let mut launch = EngineLaunch::create(installed, Some(spec), &hardware, &settings);

    for variant in ["native", "x64", "ssse3"] {
        let engine = if variant == "native" {
            resources.join("engine")
        } else {
            let dir = temp.path().join(variant);
            stage_variant(&resources.join("engine"), &dir, variant);
            dir
        };
        // No alternate directory: a fallback must not conceal failure of the
        // backend being tested.
        let paths = Paths::new(
            temp.path().join(format!("data-{variant}")),
            &engine,
            &engine,
        )
        .unwrap();
        launch.tier = if variant == "native" {
            hardware.features.best_engine_tier().to_string()
        } else {
            variant.to_string()
        };
        let manager = EngineManager::new(paths.clone(), EventSink::noop()).unwrap();
        // Allow startup, streaming and the benchmark to each complete on a
        // large Q4 model using a slow baseline backend.
        let result = tokio::time::timeout(Duration::from_secs(900), async {
            manager.ensure(&launch).await?;
            let outcome = manager
                .chat(
                    &launch,
                    vec![ChatMessage {
                        role: "user".into(),
                        content: "Say hello in one short sentence.".into(),
                    }],
                    "phase-0",
                    CancellationToken::new(),
                )
                .await?;
            let benchmark = manager.benchmark(&launch).await?;
            Ok::<_, tnn_core::AppError>((outcome, benchmark))
        })
        .await;
        // Always terminate/reap even when the request fails or times out.
        manager.stop().await;
        assert!(!paths.pid_file.exists(), "PID file survived shutdown");
        assert_eq!(manager.status().await.state, EngineState::Stopped);
        let (outcome, benchmark) = result
            .expect("engine gate timed out")
            .expect("engine gate failed");
        assert!(!outcome.cancelled);
        assert!(!selftest::looks_degenerate(&outcome.content));
        assert!(outcome.reasoning.is_empty(), "thinking must stay disabled");
        assert!(outcome.usage.completion_tokens > 0);
        println!(
            "VALIDATION {} {variant}: prompt={:.2} t/s, generation={:.2} t/s; sample={:?}",
            spec.id, benchmark.prompt_tps, benchmark.generation_tps, outcome.content
        );
        println!("PASS: {variant}, non-thinking streaming, clean shutdown");
    }
}
