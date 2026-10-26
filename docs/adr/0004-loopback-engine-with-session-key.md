# 0004 — Loopback engine with a per-session key

- Status: accepted
- Date: 2026-10-25

## Context

llama-server exposes an HTTP API. Binding it to a LAN interface would expose
the user's prompts and model to other devices. Disabling HTTP is not possible
without forking the engine.

## Decision

Bind to `127.0.0.1` on a random free port, generate a random API key per
process start, pass it via `--api-key`, and keep it inside the Rust layer. The
webview never receives the key and never calls the engine.

## Consequences

- Other local processes could still connect if they knew the port and key; the
  key is never persisted or logged, so the practical surface is small.
- Rust proxies SSE and owns all request shaping, cancellation and persistence.
- No CORS configuration is needed; there is no browser-to-engine path.
- A rejected key (401/403) is treated as a failed engine start.
