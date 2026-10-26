# Testing

Targets: Linux x64 and Windows x64 hardware test builds. The Windows checks
below need native Windows evidence; the Linux suite cannot establish them.
See [WINDOWS.md](WINDOWS.md) for prerequisites, local installer builds and
PowerShell fixtures, and `docs/ENGINE.md` for the opt-in real-model
native/SSE2/SSSE3 inference test.

## Fast checks (run before every commit)

```bash
npm run check          # svelte-check: 0 errors, 0 warnings
npm run test           # Vitest: format, sanitizer, URL policy, locale parity
npm run build          # Vite/Rolldown production build
cargo fmt --all --check
cargo test --workspace
cargo check -p tnn-local-ai
```

`cargo test -p tnn-core --features ts` regenerates the TypeScript bindings and
fails if a `#[derive(TS)]` type cannot be exported. Run it after any IPC type
change and commit the regenerated files.

## Unit coverage (Rust)

- Hardware profile → tier, minimum mode, recommendation.
- Engine args: loopback, explicit context, thread/batch limits, no GPU, no UI.
- Engine lifecycle: PID file cleanup, executable permission fix.
- Health/crash classification including `SIGILL`.
- SSE parser: split UTF-8, CRLF, multi-line `data:`, comments, `[DONE]`,
  malformed JSON, usage and timings fallback.
- HTTP error mapping: OOM, context overflow, rejected key.
- Manifest validation, URL allowlist, SHA-256, GGUF header parsing.
- Download/install helpers: unique destinations, atomic rename behavior.
- DB migrations, cascade deletes, message tail deletion, downloads round-trip.
- Settings defaults, patches, null reset, range validation.
- Path validation and traversal rejection.
- Diagnostics redaction.

## Integration coverage

`crates/tnn-core/tests/streaming.rs` starts the mock engine on a random loopback
port and verifies:

- normal SSE streaming, usage, batched deltas and joined content;
- wrong API key → `engine_spawn_failed` with recovery actions;
- cancellation preserves a partial answer and is not an error;
- timing/speed reporting.
- preservation of a long last delta when `finish_reason = length` arrives;
- premature EOF is marked unfinished, rather than silently complete.

Continuation coverage includes the chat store's append/reload and failure
paths, context retention of unfinished text, and backend persistence when an
engine start fails. Opt-in real-model coverage in `tests/continuation.rs`
forces a length stop and continues the same answer using assistant prefill,
including a longer stored answer. Set `TNN_TEST_MODEL_ID` to test another
installed catalogue model. Streaming tests also cover cancellation before HTTP
headers arrive, prompt processing longer than the stream inactivity limit,
and an engine that stalls after producing output while sending keep-alives.

The mock engine can also be run manually:

```bash
cargo run -p mock-engine -- --host 127.0.0.1 --port 8080 --api-key dev
# fault modes
MOCK_ENGINE_BEHAVIOR=slow cargo run -p mock-engine -- --port 8080
```

### Missing but required before v1

- Full `EngineManager` process-lifecycle test (spawn → health → self-test →
  crash → fallback → restart). The manager currently needs a real or staged
  `llama-server`; add a test that installs the mock binary as `llama-server`.
- Download integration test covering resume, `Content-Range`, checksum failure
  and restart. The host allowlist blocks localhost, so this needs a test-only
  allowlist injection or a signed local fixture server.
- UI component tests for onboarding and streaming states.
- E2E smoke test with a real model on the reference machine.

## Manual matrix

| Scenario | Windows 10 | Linux |
|---|---|---|
| Clean install, 4 GB / 2 cores | required | required |
| Qwen 1.5B download, pause, resume, app restart | required | required |
| Chat, stop, regenerate, export | required | required |
| Qwen 1.5B on 8 GB | required | required |
| Qwen 7B on 16 GB | recommended | recommended |
| No AVX2 (Ivy Bridge or VM) | required | required |
| No network during chat (firewall/packet capture) | required | required |
| No orphan process after quit/crash | required | required |
| Engine self-test fallback (`SIGILL` simulation) | required | required |
| Keyboard-only and screen-reader smoke test | required | required |
| English and pt-BR completeness | required | required |

### Compatibility

```bash
./scripts/verify-no-avx2.sh
```

For a full VM gate:

```bash
qemu-system-x86_64 -m 4096 -smp 2 -cpu SandyBridge \
  -drive file=ubuntu-24.04.qcow2,format=qcow2
```

Install the app, download Qwen 1.5B, complete a short chat, and record the result.
If QEMU is not available, test on a legacy CPU machine and document it.

### Performance gates

See `docs/benchmarks.md` for Qwen compatibility measurements. Controlled
performance acceptance targets for the Qwen catalogue still need validation
on the reference machines; short compatibility samples do not establish them.

## Security tests

- Markdown XSS payloads: raw `<script>`, `<style>`, `onerror`, `onclick`,
  `javascript:` links, SVG payloads, forms and iframes. Covered by
  `src/lib/markdown.test.ts`; add SVG-specific cases when the renderer changes.
- URL policy: `javascript:`, `file:`, `data:`, `mailto:`, malformed and
  non-HTTP schemes are rejected (`src/lib/external.test.ts`).
- URL allowlist bypass: lookalike hosts, `http://`, redirects to unknown hosts.
- Engine is reachable only on `127.0.0.1`; verify with `ss -ltnp` while loaded.
- API key is absent from logs, SQLite, diagnostics and frontend bindings.
- Tauri capabilities contain no shell and no broad filesystem permissions.
- `npm audit`, `cargo audit` and `cargo deny` (add configs before release).

## Diagnostics

`export_diagnostics` writes a redacted bundle: app/engine version, OS, CPU
features, RAM, model catalogue state and the last log lines. Prompts and model
paths are excluded unless explicitly requested in code; keep it that way.
