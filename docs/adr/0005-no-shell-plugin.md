# 0005 — No Tauri shell plugin

- Status: accepted
- Date: 2026-10-25

## Context

Packaging and engine control often tempt projects to add the shell plugin or
broad filesystem permissions. Both turn a UI compromise into arbitrary code
execution.

## Decision

Do not add `tauri-plugin-shell`. Spawn the engine with `tokio::process` from
Rust. Keep capabilities narrow: dialog, opener (HTTP/HTTPS), clipboard,
updater, process-restart, window-state and core defaults.

## Consequences

- The frontend cannot execute commands or read arbitrary files.
- All privileged operations are explicit IPC commands with typed arguments.
- The engine's stdout/stderr are captured by Rust, not exposed to the webview.
- Opening folders uses the opener plugin with app-owned paths only.
- Any future shell-like feature requires a new ADR and a security review.
