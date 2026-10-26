//! End-to-end persistence check using an installed catalogue model, opt-in only.
use std::path::PathBuf;
use std::time::Duration;

use tnn_core::events::EventSink;
use tnn_core::models::{Catalog, verify};
use tnn_core::storage::{InstalledModel, MessageStatus, ModelSource, Settings};
use tnn_core::{AppCore, Conversation, Paths};

async fn finished(core: &AppCore, id: &str) -> Conversation {
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let conversation = core.db.get_conversation(id).unwrap();
            if conversation
                .messages
                .last()
                .is_some_and(|m| m.status != MessageStatus::Streaming)
            {
                return conversation;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("generation timed out")
}

#[tokio::test]
#[ignore = "requires TNN_TEST_MODEL (and optionally TNN_TEST_MODEL_ID) for an installed catalogue model"]
async fn length_limited_answer_continues_in_place() {
    let model = PathBuf::from(std::env::var_os("TNN_TEST_MODEL").expect("set TNN_TEST_MODEL"))
        .canonicalize()
        .unwrap();
    let catalog = Catalog::embedded().unwrap();
    let model_id = std::env::var("TNN_TEST_MODEL_ID").unwrap_or_else(|_| "bonsai2-2b-pq2".into());
    let spec = catalog.get(&model_id).unwrap();
    verify::verify_sha256(&model, spec.sha256.as_deref().unwrap()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src-tauri/resources")
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths::new(
        temp.path(),
        root.join("engine"),
        root.join("engine-baseline"),
    )
    .unwrap();
    let core = AppCore::new(paths, EventSink::noop(), "test").unwrap();
    core.db
        .upsert_installed(&InstalledModel {
            id: spec.id.clone(),
            display_name: spec.display_name.clone(),
            file_name: spec.file_name.clone(),
            path: model.to_string_lossy().into_owned(),
            sha256: spec.sha256.clone().unwrap(),
            size_bytes: spec.size_bytes,
            installed_at: 0,
            last_used_at: None,
            source: ModelSource::Catalog,
        })
        .unwrap();
    core.db
        .save_settings(&Settings {
            max_tokens: Some(4),
            context_size: Some(2048),
            ..Settings::default()
        })
        .unwrap();
    let conversation = core.create_conversation(Some(spec.id.clone())).unwrap();
    core.send_message(
        &conversation.id,
        "Explain how a computer works in several paragraphs.",
        "initial",
    )
    .unwrap();
    let first = finished(&core, &conversation.id).await;
    let answer = first.messages.last().unwrap();
    assert_eq!(answer.status, MessageStatus::Truncated);
    let prefix = answer.content.clone();
    core.continue_response(&conversation.id, "continuation")
        .unwrap();
    let continued = finished(&core, &conversation.id).await;
    assert_eq!(continued.messages.len(), 2);
    let last = continued.messages.last().unwrap();
    assert_eq!(last.id, answer.id);
    assert!(last.content.starts_with(&prefix));
    assert!(last.content.len() > prefix.len());
    assert!(
        !last.content[prefix.len()..].starts_with(&prefix),
        "the engine echoed the prefilled prefix"
    );
    assert!(!matches!(
        last.status,
        MessageStatus::Error | MessageStatus::Streaming
    ));
    assert!(last.token_count.unwrap() > answer.token_count.unwrap());

    // Exercise reprocessing a realistic existing answer, not only the tiny
    // four-token prefill above. This uses the same already-loaded engine.
    let long_prefix = format!(
        "{}{}",
        last.content,
        " A computer processes instructions using its processor and memory. ".repeat(16)
    );
    core.db
        .update_message(
            &last.id,
            &long_prefix,
            None,
            MessageStatus::Truncated,
            Some(256),
        )
        .unwrap();
    core.continue_response(&conversation.id, "long-continuation")
        .unwrap();
    let longer = finished(&core, &conversation.id).await;
    core.stop_engine().await;
    assert_eq!(longer.messages.len(), 2);
    let last = longer.messages.last().unwrap();
    assert_eq!(last.id, answer.id);
    assert!(last.content.starts_with(&long_prefix));
    assert!(last.content.len() > long_prefix.len());
    assert!(!matches!(
        last.status,
        MessageStatus::Error | MessageStatus::Streaming
    ));
}
