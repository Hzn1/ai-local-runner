# AGENTS.md

Conventions and guardrails for anyone (human or agent) changing this repository.

## Read first

- `README.md` for the quick start and current gaps.
- `docs/ARCHITECTURE.md` before changing IPC, events or storage.
- `docs/ENGINE.md` before touching process spawning or the engine pin.
- `docs/MODELS.md` before adding a catalogue entry.
- `docs/TESTING.md` and `docs/RELEASE.md` before claiming a release is ready.

## Commands

```bash
npm run check                 # svelte-check, zero warnings expected
npm run test                  # Vitest
npm run build                 # Vite production build
cargo fmt --all --check
cargo test --workspace
cargo check -p tnn-local-ai
```

Regenerate TypeScript IPC bindings after changing a `#[derive(TS)]` type:

```bash
cargo test -p tnn-core --features ts
```

Bindings land in `src/lib/bindings/`. Do not hand-edit them. `u64`/`i64` map to
`bigint` in TypeScript while JSON delivers numbers, so use `Number(...)` at the
boundary and `0n` only for local defaults.

## Architecture rules

- All behaviour lives in `crates/tnn-core`. `src-tauri` only wires plugins,
  forwards events and validates IPC input. The Svelte layer never calls the
  engine directly.
- Core types are transport-neutral. Emit `CoreEvent` through `EventSink`; the
  Tauri layer maps it to the `*://` event names.
- Keep `AppCore` testable without a window. The core already has a mock engine.

## Guardrails (from the product plan)

- Do not use bitnet.cpp or write a custom inference engine for v1.
- Do not require AVX2. Keep the `x64` baseline fallback working.
- Do not enable thinking/reasoning by default.
- Never pass `-c 0`; always send an explicit context size.
- Never use more threads than physical cores.
- Never expose the engine to the LAN or enable its web UI.
- Do not bundle model weights in the installer.
- Do not add a model without a licence, pinned revision, SHA-256 and benchmark.
- Do not add telemetry, cloud calls, RAG, tools or vision to v1.
- Do not add the Tauri shell plugin or broad filesystem permissions.
- Do not log prompts, model paths or the per-session API key.
- Stop at phase gates for human review; do not silently widen scope.

## Frontend conventions

- Svelte 5 runes (`$state`, `$derived`, `$effect`, `$props`), TypeScript strict.
- All user-facing strings live in `src/locales/en.json` and `pt-BR.json`.
  The two files must have identical keys and placeholders; a Vitest test
  enforces this.
- Use `openExternal()` from `src/lib/external.ts` for links. Never pass model
  output to `window.open` or the opener plugin unvalidated.
- Use the generated bindings for IPC types; keep `src/lib/tauri.ts` as the only
  place that calls `invoke`.

## Rust conventions

- Edition 2024, `rustfmt` defaults, `thiserror` for domain errors.
- Public core APIs return `tnn_core::Result<T>`; map failures to `AppError` so
  the UI gets a stable code and recovery actions.
- Filesystem paths must be validated (`validate_model_id`) before being joined.
- Download and engine code must remain cancellation-safe and must clean up
  partial files/PID files on failure.
- Tests: pure logic near the module, HTTP/SSE integration in
  `crates/tnn-core/tests/`. Do not require real model files in CI.

## Security checklist for changes

- New URL? Add it to the host allowlist and require HTTPS.
- New IPC command? Validate every argument; keep it in `commands.rs`.
- New capability? Prefer a narrow permission over a plugin default.
- New HTML rendering? Route it through `renderMarkdown`.
- New secret? Keep it out of logs, bindings and diagnostics.

## Commits

Small, focused commits with a conventional prefix (`feat:`, `fix:`, `docs:`,
`test:`, `chore:`). Do not commit engine binaries, models, signing keys or
`target/`.
