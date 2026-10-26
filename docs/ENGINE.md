# Engine

## Pin

| Field | Value |
|---|---|
| Repository | `PrismML-Eng/llama.cpp` |
| Release | `prism-b10826-c9c34bc` |
| Commit | `c9c34bc` |
| Linux archive | `llama-prism-b10826-c9c34bc-bin-ubuntu-x64.tar.gz` |
| Linux SHA-256 | `66a250f38c3f8e2064e2f913b3a26ff5bae10955b67e8ff38c72ab48cbecf28c` |
| Windows archive | `llama-prism-b10826-c9c34bc-bin-win-cpu-x64.zip` |
| Windows SHA-256 | `1e88ab9e858b5c2dbc30cf847947a9f923b53511ba0baf0b265353ebe1f4daa6` |

`engine.lock.json` is the machine-readable record. Do not bump the pin without
re-running the compatibility and performance gates. The engine is not updated
independently of the app.

## Layout

`scripts/fetch-engine.sh`/`.ps1` download the archive, verify its SHA-256,
extract it and keep only what `llama-server` needs:

- `llama-server` (`llama-server.exe`)
- `libllama-server-impl.so`, `libllama-common.so*`, `libllama.so*`,
  `libggml.so*`, `libggml-base.so*`, `libmtmd.so*`
- all `libggml-cpu-*.so` runtime CPU variants
- `LICENSE` (fetched from the same tag on Windows, whose zip omits it)

A second directory, `src-tauri/resources/engine-baseline`, contains the same
payload with only `libggml-cpu-x64` (`ggml-cpu-x64.dll`). This is the fallback
when a newer CPU variant crashes with `SIGILL`.

Binaries are intentionally ignored by git. The installer embeds the pruned
directories; model weights are never embedded.

## Spawn contract

`tokio::process::Command`, no shell:

```text
llama-server
  --model <absolute GGUF path>
  --host 127.0.0.1
  --port <free loopback port>
  --api-key <random per-session key>
  --ctx-size <explicit, never 0>
  --threads <physical cores or user setting>
  --threads-batch <same>
  --parallel 1
  --batch-size <256 low-end / hardware default>
  --ubatch-size <128 low-end / hardware default>
  --n-gpu-layers 0
  --load-mode mmap
  --no-ui
  --jinja
  --reasoning-budget 0
  --reasoning-format deepseek
  --flash-attn off
```

Additional process settings:

- `current_dir(engine_dir)` so `$ORIGIN`/DLL lookup works.
- Linux: `chmod 0o755` the executable if needed; own process group;
  `LD_LIBRARY_PATH` includes the engine directory.
- Windows: `CREATE_NO_WINDOW`.
- Lower-than-normal priority; stdout/stderr captured into a 200-line tail.
- `kill_on_drop(true)` plus explicit graceful-then-forced termination.
- A PID file in app data supports orphan cleanup after a crash.

## Lifecycle

```text
Stopped → Starting → Loading → Ready
                         ├─ Failed (health/self-test/panic)
                         └─ Crashed (exit during use)
Ready → Unloading → Stopped
```

1. `ensure()` reuses a live process for the same model.
2. It allocates a free port, generates a 32-byte key, spawns the process and
   polls `/health` (100 ms interval, 120 s deadline).
3. A tiny non-streaming completion (self-test) proves the model can answer and
   the API key works.
4. On `SIGILL`/illegal-instruction detection or crash-class startup failure,
   it retries once from `engine-baseline` and marks compatibility mode.
5. Idle unload runs in `src-tauri` (default 30 min, configurable, disabled by
   "keep model loaded"). It never unloads mid-generation.
6. On app exit, the process group is terminated and the PID file removed.

## API key

A fresh random key is generated for every process start. It is passed only as
a command-line argument to the child and used by the Rust HTTP client. It is
never logged, never serialized to the frontend and never written to SQLite.

## Health, self-test and errors

- `/health` must return `{"status":"ok"}` or `"ready"`.
- The self-test uses `/v1/chat/completions` with a one-word prompt.
- Crash classification checks the exit signal and log tail for “illegal
  instruction”, producing `engine_illegal_instruction` with a compatibility-mode
  recovery action.
- “Out of memory” and context-overflow HTTP bodies map to specific errors so
  the UI can suggest a smaller model or a new chat.
- A 401/403 response means the session key was rejected; treat the process as
  compromised and restart it.

## Compatibility check

```bash
./scripts/verify-no-avx2.sh
```

This runs `llama-server --version` under `qemu-x86_64 -cpu qemu64` (SSE2 only).
A pass proves the x64 baseline loads and the dispatcher does not execute AVX2.
It does not replace the full 4 GB VM chat test.

Known-good reference: Intel i3-3217U (Ivy Bridge, no AVX2), 2 threads,
2B PQ2_0 at 2.98 tok/s generation / 4.75 tok/s prompt.

### Real inference gate (Linux)

Fetch the pinned Linux engine, then provide the exact catalogue 2B model:

```bash
scripts/fetch-engine.sh
TNN_TEST_MODEL=/absolute/path/Ternary-Bonsai-2-2B-PQ2_0.gguf \
  cargo test -p tnn-core --test real_engine -- --ignored --nocapture
```

The opt-in test verifies the model SHA-256, size and GGUF architecture before
launching the production `EngineManager`. It exercises native dispatch, an
isolated x64/SSE2 directory, and an isolated SSSE3 directory. Each run performs
the startup self-test, streams a short answer with thinking disabled, and
checks shutdown/PID cleanup. There is no fallback between directories, so a
failed backend cannot be concealed by a successful fallback. It requires
SSSE3 on the host and never downloads weights itself; ordinary CI skips it.

The output heuristic rejects obvious garbage; human review of coherence and
a timed benchmark remain separate gates. Forced backends on an Ivy Bridge
host do not prove execution on an SSE2-only CPU. Use QEMU or a VM for that
claim, and a clean 4 GB Linux VM for the minimum-memory gate. Windows
validation is deferred while Linux is the active target.

To validate another catalogue entry, set `TNN_TEST_MODEL_ID` alongside the
absolute `TNN_TEST_MODEL` path. The opt-in gate also prints a short benchmark
using engine timings and a generated greeting for review. These tiny samples
are compatibility measurements, not release performance gates.

The real-model continuation regression uses a deliberately tiny 4-token limit
to force truncation, then verifies that continuation keeps the same message,
prefix and conversation history:

```bash
TNN_TEST_MODEL=/absolute/path/Ternary-Bonsai-2-2B-PQ2_0.gguf \
  cargo test -p tnn-core --test continuation -- --ignored --nocapture
```

## Changing the pin

1. Download the new archives and compute SHA-256.
2. Update `engine.lock.json` and the duplicate constants in both fetch scripts.
3. Run `scripts/prune-engine.sh`, then `scripts/verify-no-avx2.sh`.
4. Run `scripts/bench-engine.sh` on the reference machine and record results in
   `docs/benchmarks.md`.
5. Run the full test suite and a clean install on Windows and Linux.
6. Update this document and commit the pin in one focused change.
