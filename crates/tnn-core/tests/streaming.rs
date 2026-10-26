//! Integration tests for the HTTP/SSE boundary against the deterministic
//! mock engine (no real model or network access required).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mock_engine::{Behavior, spawn_test_server};
use tnn_core::chat::{ChatCompletionRequest, ChatMessage};
use tnn_core::engine::stream::stream_chat;
use tnn_core::events::{CoreEvent, EventSink};
use tnn_core::models::catalog::Sampling;
use tokio_util::sync::CancellationToken;

fn request() -> ChatCompletionRequest {
    ChatCompletionRequest::new(
        vec![ChatMessage {
            role: "user".into(),
            content: "Say hello".into(),
        }],
        64,
        &Sampling::default(),
        false,
    )
}

fn recording_sink() -> (EventSink, Arc<Mutex<Vec<CoreEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let store = Arc::clone(&events);
    let sink = EventSink::new(Arc::new(move |event| {
        store.lock().expect("event store").push(event);
    }));
    (sink, events)
}

#[tokio::test]
async fn streams_answer_and_batches_deltas() {
    let server = spawn_test_server(Behavior::Normal, Some("secret".into())).await;
    let (sink, events) = recording_sink();

    let outcome = stream_chat(
        &reqwest::Client::new(),
        &server.base_url(),
        "secret",
        &request(),
        "req-1",
        CancellationToken::new(),
        &sink,
    )
    .await
    .expect("stream succeeds");

    assert!(!outcome.cancelled);
    assert!(outcome.content.contains("mock AI engine"));
    assert!(outcome.usage.total_tokens > 0);
    assert!(outcome.usage.completion_tokens > 0);

    let events = events.lock().unwrap();
    let deltas: Vec<&CoreEvent> = events
        .iter()
        .filter(|event| matches!(event, CoreEvent::ChatDelta(_)))
        .collect();
    assert!(!deltas.is_empty(), "expected at least one batched delta");

    let joined: String = deltas
        .iter()
        .filter_map(|event| match event {
            CoreEvent::ChatDelta(delta) => Some(delta.delta.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(joined, outcome.content);
    for event in deltas {
        if let CoreEvent::ChatDelta(delta) = event {
            assert_eq!(delta.request_id, "req-1");
        }
    }
}

#[tokio::test]
async fn wrong_api_key_is_reported_as_a_spawn_failure() {
    let server = spawn_test_server(Behavior::Normal, Some("secret".into())).await;
    let (sink, _events) = recording_sink();

    let error = stream_chat(
        &reqwest::Client::new(),
        &server.base_url(),
        "not-the-key",
        &request(),
        "req-auth",
        CancellationToken::new(),
        &sink,
    )
    .await
    .expect_err("401 must fail");

    assert_eq!(error.code(), "engine_spawn_failed");
    let payload = error.payload();
    assert!(payload.retryable);
    assert!(
        payload
            .recovery
            .contains(&tnn_core::errors::RecoveryAction::Retry)
    );
}

#[tokio::test]
async fn cancellation_keeps_the_partial_answer() {
    let server = spawn_test_server(Behavior::Slow, Some("secret".into())).await;
    let (sink, events) = recording_sink();
    let cancel = CancellationToken::new();
    let cancel_after_first_delta = cancel.clone();
    let watch = Arc::clone(&events);

    let canceller = tokio::spawn(async move {
        for _ in 0..200 {
            let saw_delta = watch
                .lock()
                .map(|events| {
                    events
                        .iter()
                        .any(|event| matches!(event, CoreEvent::ChatDelta(_)))
                })
                .unwrap_or(false);
            if saw_delta {
                cancel_after_first_delta.cancel();
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        cancel_after_first_delta.cancel();
    });

    let outcome = stream_chat(
        &reqwest::Client::new(),
        &server.base_url(),
        "secret",
        &request(),
        "req-cancel",
        cancel,
        &sink,
    )
    .await
    .expect("cancellation is not an error");

    canceller.await.unwrap();
    assert!(outcome.cancelled);
    assert!(!outcome.content.is_empty());
    assert!(outcome.elapsed_ms < 5_000);
}

#[tokio::test]
async fn malformed_chunks_do_not_abort_a_good_answer() {
    let server = spawn_test_server(Behavior::Normal, Some("secret".into())).await;
    let (sink, _events) = recording_sink();

    let outcome = stream_chat(
        &reqwest::Client::new(),
        &server.base_url(),
        "secret",
        &request(),
        "req-2",
        CancellationToken::new(),
        &sink,
    )
    .await
    .expect("stream succeeds");

    assert!(outcome.tokens_per_sec > 0.0);
}
