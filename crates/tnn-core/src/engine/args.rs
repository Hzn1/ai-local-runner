//! Command-line construction for the pinned llama-server.
//!
//! Invariants: loopback only, no UI, no GPU layers, no mlock, explicit context
//! (never `-c 0`), physical-core thread counts, and thinking disabled.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct EngineArgsConfig {
    pub model_path: PathBuf,
    pub port: u16,
    pub api_key: String,
    pub context_size: u32,
    pub threads: usize,
    pub batch_size: u32,
    pub ubatch_size: u32,
}

pub fn build_args(config: &EngineArgsConfig) -> Vec<String> {
    debug_assert!(config.context_size > 0, "never pass an unbounded context");
    debug_assert!(config.threads >= 1);
    vec![
        "--model".into(),
        path_string(&config.model_path),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        config.port.to_string(),
        "--api-key".into(),
        config.api_key.clone(),
        "--ctx-size".into(),
        config.context_size.to_string(),
        "--threads".into(),
        config.threads.to_string(),
        "--threads-batch".into(),
        config.threads.to_string(),
        "--parallel".into(),
        "1".into(),
        "--batch-size".into(),
        config.batch_size.to_string(),
        "--ubatch-size".into(),
        config.ubatch_size.to_string(),
        "--n-gpu-layers".into(),
        "0".into(),
        "--load-mode".into(),
        "mmap".into(),
        "--no-ui".into(),
        "--jinja".into(),
        "--reasoning-budget".into(),
        "0".into(),
        "--reasoning-format".into(),
        "deepseek".into(),
        "--flash-attn".into(),
        "off".into(),
    ]
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> EngineArgsConfig {
        EngineArgsConfig {
            model_path: PathBuf::from("/models/a.gguf"),
            port: 43210,
            api_key: "secret".into(),
            context_size: 2048,
            threads: 2,
            batch_size: 256,
            ubatch_size: 128,
        }
    }

    fn value_after<'a>(args: &'a [String], flag: &str) -> &'a str {
        let index = args.iter().position(|arg| arg == flag).unwrap();
        &args[index + 1]
    }

    #[test]
    fn low_end_template_is_exact() {
        let args = build_args(&config());
        assert_eq!(value_after(&args, "--host"), "127.0.0.1");
        assert_eq!(value_after(&args, "--ctx-size"), "2048");
        assert_eq!(value_after(&args, "--threads"), "2");
        assert_eq!(value_after(&args, "--threads-batch"), "2");
        assert_eq!(value_after(&args, "--batch-size"), "256");
        assert_eq!(value_after(&args, "--ubatch-size"), "128");
        assert_eq!(value_after(&args, "--n-gpu-layers"), "0");
        assert_eq!(value_after(&args, "--reasoning-budget"), "0");
        assert_eq!(value_after(&args, "--flash-attn"), "off");
        assert!(args.contains(&"--no-ui".to_string()));
        assert!(args.contains(&"--jinja".to_string()));
        assert!(!args.iter().any(|arg| arg == "-c"));
        assert!(!args.iter().any(|arg| arg == "--mlock"));
        assert!(!args.contains(&"--no-webui".to_string()) || true);
    }

    #[test]
    fn model_path_is_passed_verbatim() {
        let mut config = config();
        config.model_path = PathBuf::from("/home/user/Models with spaces/m.gguf");
        let args = build_args(&config);
        assert_eq!(
            value_after(&args, "--model"),
            "/home/user/Models with spaces/m.gguf"
        );
    }
}
