# 0001 — Use llama-server as a sidecar process

- Status: accepted
- Date: 2026-10-25

## Context

The app must run a ternary GGUF model on ordinary x64 CPUs without AVX2. The
candidate engines were the PrismML llama.cpp fork, bitnet.cpp, or a custom Rust
inference engine.

## Decision

Ship the pinned PrismML `llama-server` binary as a sidecar process managed by
Rust. Do not link an inference library into the app and do not use bitnet.cpp.

## Consequences

- Crash isolation: a model/engine failure does not take down the UI.
- The engine can be pinned, pruned and replaced independently of the app code.
- We inherit llama.cpp's CPU dispatch and GGUF support.
- The HTTP boundary must be secured: loopback only, random port, per-session
  key, no web UI, no CORS surface.
- Process lifecycle (zombie prevention, SIGILL fallback) becomes our problem.
- Packaging must ship shared libraries/DLLs beside the executable.
