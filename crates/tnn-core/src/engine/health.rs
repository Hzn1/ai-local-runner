//! Health polling and early-crash detection.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::process::Child;

use crate::errors::{AppError, Result};

pub type LogTail = Arc<Mutex<VecDeque<String>>>;
pub const LOG_TAIL_LINES: usize = 200;

pub fn push_log(tail: &LogTail, line: String) {
    if let Ok(mut tail) = tail.lock() {
        if tail.len() >= LOG_TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(line);
    }
}

pub fn tail_snapshot(tail: &LogTail) -> Vec<String> {
    tail.lock()
        .map(|tail| tail.iter().cloned().collect())
        .unwrap_or_default()
}

pub fn tail_text(tail: &LogTail) -> String {
    tail_snapshot(tail).join("\n")
}

/// Poll `/health` until the engine answers or the deadline passes.
pub async fn wait_healthy(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    timeout: Duration,
    child: &mut Child,
    logs: &LogTail,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(crash_error(status, logs));
        }

        if let Ok(response) = client
            .get(format!("{base_url}/health"))
            .bearer_auth(api_key)
            .timeout(Duration::from_secs(3))
            .send()
            .await
        {
            if response.status().is_success() {
                let body: Value = response.json().await.unwrap_or(Value::Null);
                let status = body.get("status").and_then(Value::as_str).unwrap_or("ok");
                if status == "ok" || status == "ready" {
                    return Ok(());
                }
            }
        }

        if Instant::now() >= deadline {
            return Err(AppError::EngineTimeout);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Turn an early exit into the most specific error we can prove.
pub fn crash_error(status: std::process::ExitStatus, logs: &LogTail) -> AppError {
    let tail = tail_text(logs);
    let lower = tail.to_ascii_lowercase();
    let illegal_marker = lower.contains("illegal instruction")
        || lower.contains("undefined instruction")
        || lower.contains("invalid instruction");

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if status.signal() == Some(libc::SIGILL) || illegal_marker {
            return AppError::EngineIllegalInstruction;
        }
    }

    #[cfg(windows)]
    {
        // STATUS_ILLEGAL_INSTRUCTION / STATUS_PRIVILEGED_INSTRUCTION.
        if windows_instruction_exception(status.code()) || illegal_marker {
            return AppError::EngineIllegalInstruction;
        }
    }

    let detail = if tail.is_empty() {
        format!("engine exited with {status}")
    } else {
        let last: Vec<&str> = tail.lines().rev().take(8).collect();
        format!(
            "engine exited with {status}; last output: {}",
            last.into_iter().rev().collect::<Vec<_>>().join(" | ")
        )
    };
    AppError::EngineCrash { detail }
}

#[cfg(any(windows, test))]
fn windows_instruction_exception(code: Option<i32>) -> bool {
    // ExitStatus exposes NTSTATUS as signed i32, even when its high bit is set.
    matches!(
        code.map(|code| code as u32),
        Some(0xC000_001D) | Some(0xC000_0096)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_windows_instruction_codes_are_recognized() {
        for code in [0xC000_001D_u32, 0xC000_0096] {
            assert!(windows_instruction_exception(Some(code as i32)));
        }
        assert!(!windows_instruction_exception(Some(0xC000_0135_u32 as i32)));
        assert!(!windows_instruction_exception(Some(0xC000_001E_u32 as i32)));
        assert!(!windows_instruction_exception(Some(1)));
        assert!(!windows_instruction_exception(None));
    }

    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    #[test]
    #[cfg(windows)]
    fn windows_instruction_exceptions_trigger_compatibility_fallback() {
        let logs = Arc::new(Mutex::new(VecDeque::new()));
        for code in [0xC000_001D, 0xC000_0096] {
            let status = std::process::ExitStatus::from_raw(code);
            assert!(matches!(
                crash_error(status, &logs),
                AppError::EngineIllegalInstruction
            ));
        }
        let missing_dll = std::process::ExitStatus::from_raw(0xC000_0135);
        assert!(matches!(
            crash_error(missing_dll, &logs),
            AppError::EngineCrash { .. }
        ));
    }
}
