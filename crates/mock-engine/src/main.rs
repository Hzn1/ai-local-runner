use mock_engine::{MockConfig, serve};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Accept (and ignore) the flags tnn-core passes to llama-server. Unknown
    // flags are fine: the mock only cares about host/port/key/model/context.
    let config = MockConfig::from_args(std::env::args().skip(1));
    if let Err(error) = serve(config).await {
        eprintln!("mock-engine: {error}");
        std::process::exit(1);
    }
}
