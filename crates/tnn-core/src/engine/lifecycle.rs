//! Process lifecycle: PID file, orphan cleanup, graceful termination.

use std::fs;
use std::path::Path;
use std::time::Duration;

use sysinfo::{Pid, System};
use tokio::process::Child;

use crate::errors::Result;
use crate::paths::Paths;

/// Persist the engine PID so an orphan can be found after an app crash.
pub fn write_pid_file(paths: &Paths, pid: u32, port: u16, model_id: &str) -> Result<()> {
    // PID files are advisory and never trusted for paths or commands.
    fs::write(&paths.pid_file, format!("{pid}\n{port}\n{model_id}\n"))?;
    Ok(())
}

pub fn remove_pid_file(paths: &Paths) {
    let _ = fs::remove_file(&paths.pid_file);
}

/// Kill a stale engine left behind by a previous run.
///
/// Only a PID whose process name actually contains `llama-server` is killed, so
/// PID reuse cannot take out an unrelated program.
pub fn cleanup_orphan(paths: &Paths) -> Result<bool> {
    let Ok(contents) = fs::read_to_string(&paths.pid_file) else {
        return Ok(false);
    };
    let pid = contents
        .lines()
        .next()
        .and_then(|line| line.trim().parse::<u32>().ok());
    let mut killed = false;
    if let Some(pid) = pid {
        let mut system = System::new_all();
        system.refresh_all();
        if let Some(process) = system.process(Pid::from_u32(pid)) {
            let name = process.name().to_string_lossy().to_ascii_lowercase();
            if name.contains("llama-server") {
                killed = process.kill();
            }
        }
    }
    remove_pid_file(paths);
    Ok(killed)
}

/// Send SIGTERM (Unix), wait up to two seconds, then force-kill.
pub async fn terminate(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // Safe: the PID is our direct child; failure simply means it already exited.
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }

    #[cfg(windows)]
    {
        let _ = child.start_kill();
    }

    match tokio::time::timeout(Duration::from_secs(2), child.wait()).await {
        Ok(_) => {}
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }
}

/// Lower CPU priority on Unix so the UI stays responsive during generation.
pub fn lower_priority(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, pid, 10);
    }
    #[cfg(not(unix))]
    let _ = pid;
}

/// Ensure the shipped engine binary is executable. Tauri resource copies can
/// lose the executable bit depending on how the app was packaged.
pub fn ensure_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path.exists() {
            let metadata = fs::metadata(path)?;
            let mut permissions = metadata.permissions();
            if permissions.mode() & 0o111 == 0 {
                permissions.set_mode(permissions.mode() | 0o755);
                fs::set_permissions(path, permissions)?;
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_file_round_trip_and_cleanup_is_safe() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path(), dir.path().join("e"), dir.path().join("b")).unwrap();
        write_pid_file(&paths, std::process::id(), 1234, "m").unwrap();
        assert!(paths.pid_file.exists());
        // The current process is not llama-server, so it must not be killed.
        cleanup_orphan(&paths).unwrap();
        assert!(!paths.pid_file.exists());
    }
}
