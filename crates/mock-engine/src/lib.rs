//! Deterministic fake `llama-server`.
//!
//! It accepts the same subset of flags that `tnn-core` passes to the real
//! engine, binds to loopback, checks the API key, and streams OpenAI-compatible
//! SSE. Fault modes are selected with `MOCK_ENGINE_BEHAVIOR`:
//! `normal`, `garbage`, `hang`, `crash`, `slow`.

use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behavior {
    Normal,
    Garbage,
    Hang,
    Crash,
    Slow,
}

impl Behavior {
    pub fn parse(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "garbage" => Self::Garbage,
            "hang" => Self::Hang,
            "crash" => Self::Crash,
            "slow" => Self::Slow,
            _ => Self::Normal,
        }
    }
}

#[derive(Clone)]
pub struct MockConfig {
    pub host: String,
    pub port: u16,
    pub api_key: Option<String>,
    pub model_path: String,
    pub behavior: Behavior,
    pub context_size: u32,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 0,
            api_key: None,
            model_path: "mock.gguf".into(),
            behavior: Behavior::Normal,
            context_size: 2048,
        }
    }
}

impl MockConfig {
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Self {
        let args: Vec<String> = args.into_iter().collect();
        let get = |flag: &str| -> Option<String> {
            args.iter()
                .position(|arg| arg == flag)
                .and_then(|index| args.get(index + 1))
                .cloned()
        };
        Self {
            host: get("--host").unwrap_or_else(|| "127.0.0.1".into()),
            port: get("--port")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            api_key: get("--api-key"),
            model_path: get("--model").unwrap_or_else(|| "mock.gguf".into()),
            behavior: std::env::var("MOCK_ENGINE_BEHAVIOR")
                .map(|value| Behavior::parse(&value))
                .unwrap_or(Behavior::Normal),
            context_size: get("--ctx-size")
                .and_then(|value| value.parse().ok())
                .unwrap_or(2048),
        }
    }
}

#[derive(Clone)]
struct AppState {
    api_key: Option<String>,
    model_path: String,
    behavior: Behavior,
    context_size: u32,
}

impl AppState {
    fn from_config(config: &MockConfig) -> Self {
        Self {
            api_key: config.api_key.clone(),
            model_path: config.model_path.clone(),
            behavior: config.behavior,
            context_size: config.context_size,
        }
    }
}

pub struct TestServer {
    pub port: u16,
    pub api_key: Option<String>,
    shutdown: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl TestServer {
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
        self.handle.abort();
    }
}

pub async fn spawn_test_server(behavior: Behavior, api_key: Option<String>) -> TestServer {
    let config = MockConfig {
        behavior,
        api_key,
        ..MockConfig::default()
    };
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind mock engine");
    let port = listener.local_addr().expect("local addr").port();
    let shutdown = CancellationToken::new();
    let token = shutdown.clone();
    let app = router(&config);
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move { token.cancelled().await })
            .await;
    });
    // Give the listener a moment to be accepting connections.
    tokio::time::sleep(Duration::from_millis(30)).await;
    TestServer {
        port,
        api_key: config.api_key,
        shutdown,
        handle,
    }
}

pub async fn serve(config: MockConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let address = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;
    let actual = listener.local_addr()?;
    tracing::info!("mock engine listening on http://{actual}");
    axum::serve(listener, router(&config)).await?;
    Ok(())
}

fn router(config: &MockConfig) -> Router {
    let state = AppState::from_config(config);
    Router::new()
        .route("/health", get(health))
        .route("/props", get(props))
        .route("/tokenize", post(tokenize))
        .route("/v1/chat/completions", post(chat_completions))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
}

async fn auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let Some(expected) = state.api_key.as_deref() else {
        return next.run(request).await;
    };
    let supplied = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if supplied == format!("Bearer {expected}") {
        next.run(request).await
    } else {
        (StatusCode::UNAUTHORIZED, "invalid api key").into_response()
    }
}

async fn health(State(state): State<AppState>) -> Response {
    match state.behavior {
        Behavior::Hang => {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
        Behavior::Crash => {
            std::process::exit(1);
        }
        _ => Json(json!({"status": "ok"})).into_response(),
    }
}

async fn props(State(state): State<AppState>) -> Response {
    Json(json!({
        "model_path": state.model_path,
        "n_ctx": state.context_size,
        "chat_template": "{{ messages }}",
        "build_info": "mock-engine",
    }))
    .into_response()
}

async fn tokenize(Json(body): Json<Value>) -> Response {
    let content = body
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let count = content.chars().count().div_ceil(4).max(1);
    Json(json!({"tokens": (0..count).collect::<Vec<_>>()})).into_response()
}

async fn chat_completions(State(state): State<AppState>, Json(body): Json<Value>) -> Response {
    if state.behavior == Behavior::Hang {
        tokio::time::sleep(Duration::from_secs(3600)).await;
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if state.behavior == Behavior::Crash {
        std::process::exit(1);
    }

    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let prompt_tokens = body
        .get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| message.get("content").and_then(Value::as_str))
                .map(|content| content.chars().count().div_ceil(4).max(1) as u32)
                .sum()
        })
        .unwrap_or(2);
    let max_tokens = body.get("max_tokens").and_then(Value::as_u64).unwrap_or(64) as usize;

    let answer = match state.behavior {
        Behavior::Garbage => "!".repeat(max_tokens.clamp(4, 32)),
        _ => "Hello! This is the mock AI engine. It streams a short, deterministic answer so the interface can be tested offline.".to_string(),
    };

    if !stream {
        return Json(json!({
            "id": "mock",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": answer},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": prompt_tokens,
                "completion_tokens": answer.chars().count().div_ceil(4) as u32,
                "total_tokens": prompt_tokens + answer.chars().count().div_ceil(4) as u32
            },
            "timings": {
                "prompt_n": prompt_tokens,
                "predicted_n": answer.chars().count().div_ceil(4) as u32,
                "prompt_per_second": 12.5,
                "predicted_per_second": 4.0
            }
        }))
        .into_response();
    }

    let chunks: Vec<String> = answer
        .chars()
        .collect::<Vec<_>>()
        .chunks(6)
        .map(|chunk| chunk.iter().collect())
        .collect();
    let completion_tokens = answer.chars().count().div_ceil(4) as u32;
    let delay = if state.behavior == Behavior::Slow {
        Duration::from_millis(200)
    } else {
        Duration::from_millis(20)
    };

    let (sender, receiver) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(8);
    tokio::spawn(async move {
        for chunk in chunks {
            let payload = json!({
                "id": "mock",
                "choices": [{
                    "index": 0,
                    "delta": {"content": chunk},
                    "finish_reason": null
                }]
            });
            if sender
                .send(Ok(Event::default().data(payload.to_string())))
                .await
                .is_err()
            {
                return;
            }
            tokio::time::sleep(delay).await;
        }
        let usage = json!({
            "id": "mock",
            "choices": [],
            "usage": {
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "total_tokens": prompt_tokens + completion_tokens
            }
        });
        let _ = sender
            .send(Ok(Event::default().data(usage.to_string())))
            .await;
        let _ = sender.send(Ok(Event::default().data("[DONE]"))).await;
    });

    Sse::new(ReceiverStream::new(receiver))
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Convenience for integration tests that need a plain HTTP client.
pub fn spawn_blocking_server(config: MockConfig) -> Arc<Mutex<Option<TestServer>>> {
    let slot = Arc::new(Mutex::new(None));
    let output = Arc::clone(&slot);
    tokio::spawn(async move {
        let server = spawn_test_server(config.behavior, config.api_key).await;
        if let Ok(mut slot) = output.lock() {
            *slot = Some(server);
        }
    });
    slot
}

// Kept so the module compiles on all platforms without pulling extra crates.
#[allow(dead_code)]
fn _body_marker(_: Body) {}
