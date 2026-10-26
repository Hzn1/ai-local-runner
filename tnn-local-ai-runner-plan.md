# TNN Local AI Runner — Implementation Plan

**Status:** In progress; Phase 0 validation pending human review  
**Date:** 2026-10-25  
**Target:** Linux x64 and Windows x64 hardware test builds (2026-10-26)
**Stack:** Rust + Tauri v2 + Svelte 5  
**Working title:** TNN Local AI  
**Primary audience:** non-technical users on modest, CPU-only machines without AVX2

> Current scope: Linux x64 plus Windows x64 builds for hardware testing, per
> product-owner instruction on 2026-10-26. See `docs/WINDOWS.md` for the local
> installer and testing workflow. Native Windows acceptance is pending; all
> guardrails and human review gates still apply before a public release.

---

> Model-scope update (2026-10-26): all Bonsai models were removed by product-owner
> instruction. The active catalogue and recommendations are documented in
> `docs/MODELS.md`. Bonsai sections below preserve the original plan and spike
> history; they no longer define the supported models or performance gates.

## 0. Confirmed decisions

These decisions were confirmed by the product owner and are not open for re-litigation during implementation unless a hard technical blocker is proven.

| Decision | Choice |
|---|---|
| Platforms | Windows 10/11 x64 and Linux x64. macOS is deferred. |
| Frontend | Svelte 5 + TypeScript + Vite + Tailwind CSS |
| Model scope | Qwen 2.5 Instruct 0.5B/1.5B/7B and Qwen 3 0.6B (Q4_K_M); experimental entries remain hidden |
| Minimum hardware | **Officially support 4 GB RAM / 2 physical cores at ~3 tok/s**, with clear warnings and a “Minimum mode” |
| V1 modality | Text-only chat |
| Deferred | Vision, tool calling, MCP, local API, RAG, voice, mobile, macOS |
| GPU | None. CPU-only, no CUDA/Metal/Vulkan backends shipped. |
| AVX2 | Not required. Engine must run on SSE2/SSSE3/SSE4.2/AVX CPUs. |
| Privacy | No telemetry, no account, no cloud inference. Network only for model download and update checks. |
| App name | `TNN Local AI` is a placeholder; branding can change without changing architecture. |

---

## 1. Executive summary

Build a consumer desktop application that lets a non-technical person download a ternary-weight language model and chat with it locally, entirely on the CPU, on machines that lack AVX2 and have no GPU.

The application is a **Tauri v2** desktop shell with a **Svelte 5** frontend and a **Rust core**. Inference is performed by a pinned **PrismML llama.cpp CPU build** running as a local child process, exposed only on `127.0.0.1` with a per-session API key. The Rust core owns model downloads, verification, storage, engine lifecycle, context building, streaming, cancellation, diagnostics, and error mapping. The frontend never talks to the engine directly.

The main model family is **Ternary Bonsai 2** (2B/4B/9B/27B, Apache 2.0), using the **PQ2_0** packing and the PrismML ternary kernels. BitNet and Falcon3 models are included as **experimental** entries behind a validation gate because their non-AVX2 performance and format support are weaker.

The product must be honest about speed. On the validated 2-core Ivy Bridge test machine, Bonsai 2 2B runs at roughly **3 tokens/second**. That is usable for short chat but not comparable to a GPU or a modern desktop. The app must benchmark the machine, default to the smallest suitable model, keep contexts short, disable reasoning mode by default, and explain performance in plain language.

---

## 2. Feasibility spike: what was validated

A real spike was run on the target-class machine:

- Intel Core i3-3217U, 2 physical cores / 4 threads, 1.8 GHz
- 9.6 GB RAM
- **AVX and F16C present, AVX2 absent**
- No GPU
- Linux x64
- Engine: PrismML llama.cpp `prism-b10826-c9c34bc`
- Model: `Ternary-Bonsai-2-2B-PQ2_0.gguf` (494 MB)

### 2.1 Measured results

| Configuration | Prompt t/s | Generation t/s | Notes |
|---|---:|---:|---|
| Bonsai 2 2B PQ2_0, 1 thread | 2.78 | 1.86 | Works |
| Bonsai 2 2B PQ2_0, 2 threads | **4.75** | **2.98** | Best; coherent non-thinking output |
| Bonsai 2 2B PQ2_0, 3 threads | 4.57 | 2.05 | Worse than 2 |
| Bonsai 2 2B PQ2_0, 4 threads | 4.32 | 1.78 | Worse than 2 |
| Bonsai 2 2B PQ2_0, forced `x64`/SSE2 backend | 2.94 (pp32) | 1.90 (tg16) | Baseline works |
| Bonsai 2 2B PQ2_0, forced `ssse3` backend | 4.06 (pp32) | 2.80 (tg16) | ~1.5× faster than SSE2 |
| Bonsai 2 2B PTQ1_0 | 1.96 | 1.48 | Slower than PQ2_0 here |
| Bonsai 2 2B Q2_0 | 0.99 | 0.69 | Slowest |
| BitNet 0.7B TQ2_0 | 3.89 | 3.73 | 0.2 GB; lower quality |
| BitNet 2B4T TQ2_0 | 1.24 | 1.09 | Scalar on non-AVX2; not suitable as default |

Benchmark numbers vary with background load and thermal state. Use them for relative decisions, not as universal promises.

### 2.2 Conclusions that drive the plan

1. **No AVX2 is not a blocker.** The PrismML CPU archive contains runtime-dispatch variants down to `x64` (SSE2), `ssse3`, `sse42`, `sandybridge`, `ivybridge`, `piledriver`, `haswell`, `zen4`, etc. On this machine it selected `libggml-cpu-ivybridge.so`. Forcing the `x64` and `ssse3` variants also produced correct output.
2. **Bonsai 2 2B PQ2_0 is the best low-end model tested.** It beats BitNet 2B4T TQ2_0 by roughly 3× on this hardware.
3. **Threads must be physical cores.** 2 threads beat 3–4 on a 2C/4T CPU. Never use logical core count.
4. **Thinking mode must be off by default.** Bonsai 2 is a reasoning model; a single answer can consume thousands of thinking tokens. At 3 tok/s that is minutes of waiting.
5. **Prompt processing is the hidden cost.** A 100-token prompt at ~4.5 tok/s takes ~22 seconds before the first token. Keep system prompts short, cap context, truncate history aggressively on low-end machines, and show a “Reading your message…” state.
6. **4 GB / 2-core is supportable but at the floor.** It must be labeled as the minimum, not the recommended experience.

---

## 3. Product scope

### 3.1 Target user

A person who:

- Uses Windows 10/11 or a common Linux desktop.
- Has a 2011–2018 laptop or desktop with 4–8 GB RAM and no discrete GPU.
- Does not know what AVX2, GGUF, quantization, or a token is.
- Wants a private ChatGPT-like assistant without accounts, subscriptions, or cloud uploads.
- Is willing to download a 0.5–2.5 GB model once.

### 3.2 V1 features

- First-run onboarding with automatic hardware scan and model recommendation.
- One-click model download with progress, pause/resume, checksum verification, and disk-space checks.
- Local chat with streaming responses, stop, regenerate, copy, and conversation history.
- Model management: install, switch, delete, import a local GGUF, view storage use.
- Plain-language performance check and “words per second” estimate.
- Settings in Simple and Advanced modes.
- Fully offline chat after the model is downloaded.
- English and Brazilian Portuguese UI.
- Export diagnostics and conversations.
- Auto-update for the app.

### 3.3 Non-goals for v1

- Image/vision input (Bonsai 2 supports it; deferred).
- Tool calling, MCP, web search, function calling.
- A local OpenAI-compatible API for power users.
- RAG/document chat.
- Voice input/output.
- Model training, fine-tuning, or quantization UI.
- macOS, mobile, 32-bit, ARM64 (architecture stays portable, but not shipped).
- A general “run any GGUF” power-user mode.

### 3.4 Supported platforms and minimum specs

| | Windows | Linux |
|---|---|---|
| OS | Windows 10 21H2+ / Windows 11, x64 | Ubuntu 22.04/24.04, Debian 12, x64 |
| CPU | 2 physical cores; SSE2 minimum for the engine; SSE4.2 recommended for the WebView | Same |
| AVX2 | Not required | Not required |
| RAM | **4 GB minimum**, 8 GB recommended | Same |
| Disk | 2 GB free + model size (0.5–7.2 GB) | Same |
| GPU | None | None |
| Runtime | WebView2 | WebKitGTK 4.1 |
| Arch | x64 | x64 |

Notes:

- The inference engine itself supports SSE2. The app’s WebView may require SSE4.2; officially support CPUs from roughly 2008 onward (Intel Penryn/Core 2 and AMD K10+).
- Windows ARM64, Linux ARM64, and macOS are explicitly deferred.
- 4 GB machines must use the 2B model, 2K context, 2 threads, and short outputs. The app should enter “Minimum mode” automatically.

### 3.5 Success metrics

- A non-technical tester can install, download a model, and send a first message without a terminal or documentation.
- On the validated 2-core Ivy Bridge machine: Bonsai 2 2B ≥ 2.5 tok/s, coherent output, UI responsive, app RSS < 1.2 GB.
- On a 4-core desktop without AVX2: Bonsai 2 2B ≥ 6 tok/s; 4B ≥ 3 tok/s.
- No network requests during chat (verified with a proxy/log).
- No AVX2 requirement; SSE2/SSSE3 fallback works.
- No orphan engine process after 10 app restarts.
- 100% of user-facing errors have a plain-language message and a recovery action.

---

## 4. Architecture

### 4.1 System diagram

```text
┌────────────────────────────────────────────────────────────┐
│ Svelte 5 UI (Tauri WebView)                                │
│ onboarding · chat · models · settings · diagnostics        │
└───────────────────────┬────────────────────────────────────┘
                        │ Tauri invoke / events
┌───────────────────────▼────────────────────────────────────┐
│ src-tauri: thin command and event layer                    │
│ window · updater · dialogs · single instance               │
└───────────────────────┬────────────────────────────────────┘
                        │
┌───────────────────────▼────────────────────────────────────┐
│ crates/tnn-core (pure Rust, testable without Tauri)        │
│ hardware · catalog · downloads · storage · engine manager  │
│ context builder · SSE proxy · diagnostics · errors         │
└───────────────────────┬────────────────────────────────────┘
                        │ spawn + HTTP/SSE on 127.0.0.1
┌───────────────────────▼────────────────────────────────────┐
│ llama-server (PrismML CPU build)                           │
│ runtime CPU variant dispatch: x64 → ssse3 → … → avx2       │
└────────────────────────────────────────────────────────────┘
```

### 4.2 Component responsibilities

| Component | Responsibility |
|---|---|
| `src-tauri` | Tauri setup, commands, event forwarding, window/updater, app lifecycle. No business logic. |
| `tnn-core` | Hardware detection, model catalog, download/verify, SQLite, engine lifecycle, chat context, SSE parsing, cancellation, diagnostics. |
| `llama-server` | Model loading, tokenization, chat template, inference, streaming OpenAI-compatible API. |
| Svelte UI | Presentation, local UI state, streaming display, accessibility, i18n. Never spawns processes or reads model files directly. |

### 4.3 Why a sidecar engine

- **Crash isolation:** an engine crash does not take down the UI or lose the conversation database.
- **Multi-ISA support:** the PrismML archive loads the best CPU backend at runtime; no Rust/C++ ABI work.
- **Mature inference:** chat templates, tokenization, KV cache, sampling, and SSE are already solved.
- **Packaging simplicity on Windows/Linux:** the engine is a relocatable directory in Tauri resources; no shared-library work in the Rust binary.
- **Security:** bind to loopback, disable the web UI, require a per-session API key, and proxy all requests through Rust.

### 4.4 Rejected alternatives (record as ADRs)

| Alternative | Why rejected |
|---|---|
| **bitnet.cpp** | Its i2_s x86 kernels have no non-AVX2 fallback; open issue #547 reports garbage output on such CPUs. Not safe for this product. |
| **Pure-Rust ternary engine** | Current Rust ternary runtimes are immature/pre-parity. Too risky for v1. Revisit in Phase 8 behind an engine trait. |
| **In-process `llama-cpp-2`** | Harder to patch the PrismML fork, harder to ship multi-ISA variants, and a crash kills the app. |
| **Ollama / LM Studio** | Not embeddable, heavier, and not designed around the no-AVX2 guarantee. |
| **Upstream llama.cpp only** | Upstream `Q2_0` support exists, but PQ2_0/PTQ1_0 and the tested SSE2/SSSE3 ternary kernels come from the PrismML build. |
| **macOS in v1** | Adds signing/notarization and dylib packaging work. Deferred until Windows/Linux are stable. |

### 4.5 Key data flows

**Chat:**

1. User sends a message.
2. Rust persists the user message and builds the context (system prompt + recent history within the token budget).
3. Rust ensures the engine is running for the selected model; starts and warms it if needed.
4. Rust POSTs to `http://127.0.0.1:<port>/v1/chat/completions` with `stream: true`, non-thinking kwargs, and sampling settings.
5. Rust parses SSE, batches deltas every ~50 ms, and emits `chat://delta` events.
6. On `[DONE]`, Rust persists the assistant message and usage, then emits `chat://done`.
7. On cancel, Rust aborts the HTTP request, persists the partial message as `interrupted`, and emits `chat://cancelled`.
8. On error, Rust maps the engine error to a friendly `AppError`, persists status `error`, and offers retry.

**Model download:**

1. User selects a model.
2. Rust checks license display, disk space, and URL allowlist.
3. Rust downloads to `<model>.part` with `Range` resume and streaming SHA-256.
4. On completion, Rust verifies SHA-256 and the GGUF header, then atomically renames to the final path.
5. Rust writes the installed-model row and emits `download://state = installed`.
6. On failure, Rust keeps the `.part` for resume unless the user cancels/deletes.

**Engine lifecycle:**

- Lazy start on first message, not app launch.
- Health poll `/health` every 100 ms, 60 s timeout.
- Warmup with a 1-token non-thinking request.
- Self-test with a short prompt; if output looks like garbage or the process dies with an illegal instruction, restart using the baseline engine directory.
- Idle unload after 30 minutes (configurable).
- Kill on app exit; orphan cleanup on startup via PID file + process-name check.

### 4.6 Security and privacy model

- Engine binds to `127.0.0.1` only; no LAN exposure.
- Per-session random API key; never logged or exposed to the frontend.
- `--no-ui` to disable llama-server’s built-in web UI.
- Frontend never calls the engine directly; Rust proxies SSE. No CORS surface.
- Strict CSP; no remote fonts, scripts, or images.
- Markdown is sanitized; raw HTML disabled; links open via the opener plugin after `http/https` validation.
- No telemetry. Diagnostics export is user-initiated and excludes prompts by default.
- Logs redact API keys, prompts, and model paths where appropriate.
- Model downloads use HTTPS, a host allowlist, pinned Hugging Face revisions, and pinned SHA-256.
- Tauri capabilities are least-privilege; no shell plugin, no broad filesystem plugin.
- App updates are signed; model manifest is embedded in the app, not fetched remotely.

---

## 5. Engine plan

### 5.1 Pinned engine

- Repository: `PrismML-Eng/llama.cpp`, branch `prism`.
- Tested release: `prism-b10826-c9c34bc`.
- Do not auto-update the engine independently in v1. Pin the release and archive SHA-256 in `engine.lock.json`.
- Re-run the compatibility spike before changing the pin.

Archives:

| Platform | Asset |
|---|---|
| Windows x64 | `llama-prism-b10826-c9c34bc-bin-win-cpu-x64.zip` |
| Linux x64 | `llama-prism-b10826-c9c34bc-bin-ubuntu-x64.tar.gz` |

The archive contains `llama-server`, its shared libraries, and CPU variant libraries (`x64`, `ssse3`, `sse42`, `sandybridge`, `ivybridge`, `piledriver`, `haswell`, `zen4`, and others). On Linux the wrapper has `RUNPATH=$ORIGIN`; on Windows the DLLs sit beside the executable. Both are relocatable.

### 5.2 Engine directory layout

Prune unused tools to reduce installer size. Keep only what `llama-server` needs.

**Linux:**

```text
resources/engine/
├─ llama-server
├─ libllama-server-impl.so
├─ libllama-common.so*
├─ libllama.so*
├─ libggml.so*
├─ libggml-base.so*
├─ libmtmd.so*                 # keep if linked by the server
├─ libggml-cpu-x64.so
├─ libggml-cpu-ssse3.so
├─ libggml-cpu-sse42.so
├─ libggml-cpu-sandybridge.so
├─ libggml-cpu-ivybridge.so
├─ libggml-cpu-piledriver.so
├─ libggml-cpu-haswell.so
├─ libggml-cpu-zen4.so
├─ ...other CPU variants...
└─ LICENSE
```

**Windows:**

```text
resources/engine/
├─ llama-server.exe
├─ llama-server-impl.dll
├─ llama-common.dll
├─ llama.dll
├─ ggml.dll
├─ ggml-base.dll
├─ mtmd.dll
├─ ggml-cpu-x64.dll
├─ ggml-cpu-ssse3.dll
├─ ggml-cpu-sse42.dll
├─ ggml-cpu-sandybridge.dll
├─ ggml-cpu-ivybridge.dll
├─ ggml-cpu-piledriver.dll
├─ ggml-cpu-haswell.dll
├─ ggml-cpu-zen4.dll
├─ ...other CPU variants...
└─ LICENSE
```

Keep a second **baseline** directory with only the `x64` CPU variant (and common libraries) for the fallback path.

### 5.3 Spawning the engine

Use `tokio::process::Command` directly; do not use a shell. This avoids shell-injection surface and avoids needing the Tauri shell plugin.

- `current_dir(engine_dir)` so `$ORIGIN`/DLL search works.
- On Linux, `chmod 0o755` the `llama-server` file at startup if needed.
- On Windows, set `CREATE_NO_WINDOW` (`0x08000000`) so no console flashes.
- On Unix, optionally set a process group so the whole group can be killed.
- Set process priority below normal so the UI stays responsive.
- Capture stdout/stderr, keep the last 200 lines in memory, and write to a rotating engine log.
- Never log the API key.

### 5.4 Command-line template

Low-end (4 GB / 2 cores, 2B model):

```text
llama-server
  --model <model.gguf>
  --host 127.0.0.1
  --port <random free port>
  --api-key <random 32-byte hex>
  --ctx-size 2048
  --threads 2
  --threads-batch 2
  --parallel 1
  --batch-size 256
  --ubatch-size 128
  --n-gpu-layers 0
  --load-mode mmap
  --no-ui
  --jinja
  --reasoning-budget 0
  --reasoning-format deepseek
  --flash-attn off
```

Balanced (8 GB / 4 cores, 4B model):

```text
--ctx-size 4096 --threads 4 --threads-batch 4
--batch-size 512 --ubatch-size 256
```

Smart (16 GB / 6–8 cores, 9B model):

```text
--ctx-size 8192 --threads 6 --threads-batch 6
--batch-size 1024 --ubatch-size 512
```

Rules:

- Never pass `-c 0`; Bonsai 2 would try its 262K training context and exhaust memory.
- `--no-ui` is the current flag; `--no-webui` is an alias.
- `--load-mode mmap` replaces the deprecated `--mmap`/`--mlock` flags.
- `--flash-attn off` on CPU; the spike showed `-fa on` slower here.
- `--parallel 1`; one generation at a time.
- `--n-gpu-layers 0`; no GPU backends are shipped.
- `--reasoning-budget 0` plus per-request `chat_template_kwargs` prevents thinking mode.
- Do not enable `--mlock` on 4 GB machines.
- For 9B on 8 GB, optionally use quantized KV cache (`--cache-type-k q8_0 --cache-type-v q8_0`) only after testing.

### 5.5 Hardware detection and backend selection

- Use `sysinfo` for total RAM, physical core count, OS version, and disk free space.
- Use `raw-cpuid` on x86 to detect SSE2, SSSE3, SSE4.2, AVX, AVX2, FMA, F16C.
- On ARM64 (future), use `std::arch::is_aarch64_feature_detected!`.
- Let the PrismML runtime choose the actual CPU backend. Report the chosen tier in diagnostics if the engine logs it.
- Map to a user-facing “Compatibility mode” only in Advanced/Diagnostics, not on the onboarding screen.

### 5.6 Self-test and fallback

After the engine is healthy and warm:

1. Send a short non-thinking prompt with `max_tokens=8`, `temperature=0`.
2. Accept output if it contains letters and is not obviously degenerate.
3. If output is garbage (e.g., one character repeated, no letters, `!!!!` flood) or the process exits with an illegal-instruction code:
   - Stop the engine.
   - Restart from the `engine-baseline` directory containing only the `x64` CPU variant.
   - Re-run the self-test.
   - If it still fails, show “Compatibility mode failed” with diagnostics export.
4. Log the fallback event and show “Compatibility mode” in About/Diagnostics.

This guards against the exact class of bug bitnet.cpp has on non-AVX2 CPUs.

### 5.7 Process lifecycle and orphan prevention

- Write a PID file in app data when the engine starts; remove it on clean shutdown.
- On startup, if a PID file exists, check whether that PID is alive and its process name matches `llama-server`; kill it only if it matches.
- On app exit, kill the child and wait briefly.
- On Windows, use a Job Object if available to ensure child termination; otherwise `taskkill /PID <pid> /T /F` as a fallback.
- On Linux, send `SIGTERM`, wait 2 s, then `SIGKILL`.
- Never auto-restart more than once after a crash; show the error instead of looping.

### 5.8 Performance tuning

- Threads = physical cores, capped at 8. On 2C/4T, use 2.
- Leave one core free on 4-core machines if the UI feels sluggish; make this an Advanced setting.
- Keep the system prompt short (< 100 tokens).
- Truncate history to fit `ctx - max_tokens - 128`.
- Use non-thinking mode and short outputs.
- Keep the model loaded while the app is open; unload after 30 minutes idle or on “Free memory.”
- Warm up after load so the first user prompt does not pay page-fault costs.
- Do not run downloads and generation simultaneously on 4 GB machines; pause downloads during chat if needed.

---

## 6. Model plan

### 6.1 Main catalog: Ternary Bonsai 2

| Tier | Display name | File | Size | Min RAM | Recommended RAM | Default context | Default max output |
|---|---|---|---:|---:|---:|---:|---:|
| Fast | Bonsai 2 · 2B | `Ternary-Bonsai-2-2B-PQ2_0.gguf` | 0.52 GB | 4 GB | 8 GB | 2048 (4 GB) / 4096 | 256 / 512 |
| Balanced | Bonsai 2 · 4B | `Ternary-Bonsai-2-4B-PQ2_0.gguf` | 1.15 GB | 6–8 GB | 12–16 GB | 4096 | 512 |
| Smart | Bonsai 2 · 9B | `Ternary-Bonsai-2-9B-PQ2_0.gguf` | 2.42 GB | 12 GB | 16 GB | 8192 | 1024 |
| Advanced | Bonsai 2 · 27B | `Ternary-Bonsai-2-27B-PQ2_0.gguf` or `PTQ1_0` | 7.21 / 5.95 GB | 24 GB | 32 GB | 8192 | 1024 |

All are Apache 2.0. Use PQ2_0 by default because the spike showed it is the fastest CPU packing here. Offer PTQ1_0 for the 27B only as a smaller-download alternative.

**Why Bonsai 2:** It is the current ternary family with usable sizes for 4–16 GB machines, Apache 2.0 licensing, and PrismML CPU kernels that work down to SSE2. It is a reasoning model, so non-thinking defaults are mandatory.

### 6.2 Experimental catalog

These are included because the product owner asked for all of them, but they are **not** default recommendations. Each must pass a validation gate before appearing in the UI.

| Model | Format | Approx. size | Status | Notes |
|---|---|---:|---|---|
| BitNet b1.58 2B4T | TQ2_0 (community conversion) | ~1.2 GB | Experimental | Scalar on non-AVX2; measured ~1.1 tok/s here. AVX2 recommended. |
| BitNet b1.58 0.7B (“large”) | TQ2_0 (community conversion) | ~0.2 GB | Experimental | Measured ~3.7 tok/s here; lower quality. Possible “Feather” model for 4 GB. |
| Falcon3 1B Instruct 1.58bit | TQ2_0/TQ1_0 (converted) | ~0.2 GB | Experimental | Official GGUF is i2_s only; conversion required. Apache 2.0. |
| Falcon3 3B Instruct 1.58bit | TQ2_0/TQ1_0 (converted) | ~0.6 GB | Experimental | Conversion required. |
| Falcon3 7B Instruct 1.58bit | TQ2_0/TQ1_0 (converted) | ~1.4 GB | Experimental | Conversion required; 8 GB+ recommended. |
| Falcon3 10B Instruct 1.58bit | TQ2_0/TQ1_0 (converted) | ~2.0 GB | Experimental | Conversion required; 16 GB recommended. |
| Llama3-8B-1.58 | TQ1_0 | ~1.7 GB | Optional experimental | Llama 3 license acceptance required; not recommended for 4 GB. |

**Validation gate for every experimental model:**

1. Legal: license permits download and local use; attribution shown.
2. Format: loads in the pinned engine and produces coherent output.
3. Compatibility: coherent on `x64` (SSE2), `ssse3`, and the best available tier.
4. Performance: ≥ 1.5 tok/s generation on a 4-core desktop; otherwise label “Advanced hardware only” or exclude.
5. Integrity: pinned Hugging Face revision and SHA-256.
6. UX: clear “Experimental” badge and a one-line reason (“slower on older CPUs”, “requires conversion”, etc.).

If an experimental model fails the gate, do not ship it in v1. The main Bonsai 2 catalog is not blocked by experimental work.

### 6.3 Non-thinking defaults

Bonsai 2 is a reasoning model. For this audience:

- Server: `--reasoning-budget 0`.
- Request: `chat_template_kwargs: {"enable_thinking": false}`.
- Sampling: `temperature 0.7`, `top_p 0.8`, `top_k 20`, `min_p 0`, `presence_penalty 1.5`, `repetition_penalty 1.0`, `repeat_last_n 64`.
- `max_tokens`: 256 (4 GB/2-core), 512 (8 GB/4-core), 1024 (16 GB/8-core).
- Advanced users can enable “Show reasoning” and a reasoning budget later; it is not part of the default flow.

### 6.4 Manifest schema

Embed the catalog in the app at build time. Do not fetch a remote manifest in v1.

```json
{
  "schema_version": 1,
  "catalog_version": "2026-10-25",
  "models": [
    {
      "id": "bonsai2-2b-pq2",
      "display_name": "Bonsai 2 · 2B",
      "tier": "fast",
      "family": "Bonsai 2",
      "params_b": 1.88,
      "quant": "PQ2_0",
      "file_name": "Ternary-Bonsai-2-2B-PQ2_0.gguf",
      "url": "https://huggingface.co/prism-ml/Ternary-Bonsai-2-2B-gguf/resolve/<pinned-commit>/Ternary-Bonsai-2-2B-PQ2_0.gguf",
      "sha256": "<pinned>",
      "size_bytes": 517000000,
      "license": "Apache-2.0",
      "license_url": "https://huggingface.co/prism-ml/Ternary-Bonsai-2-2B-gguf",
      "min_ram_bytes": 4294967296,
      "recommended_ram_bytes": 8589934592,
      "min_disk_bytes": 1073741824,
      "context_default": 4096,
      "context_max": 262144,
      "max_tokens_default": 512,
      "sampling": {
        "temperature": 0.7,
        "top_p": 0.8,
        "top_k": 20,
        "min_p": 0.0,
        "presence_penalty": 1.5,
        "repeat_penalty": 1.0,
        "repeat_last_n": 64
      },
      "chat_template_kwargs": { "enable_thinking": false },
      "engine_min_tier": "x64",
      "description": "Fast, private, and small enough for older PCs.",
      "badge": "recommended",
      "experimental": false
    }
  ]
}
```

### 6.5 Download, verification, and installation

- HTTPS only; host allowlist for `huggingface.co`, `cdn-lfs.huggingface.co`, and GitHub release hosts.
- Pin the Hugging Face revision commit, not `main`.
- Download to `<model>.part`; resume with `Range` and validate `Content-Range`.
- Stream SHA-256 while downloading; compare to the manifest.
- Verify GGUF magic, version, architecture, and quant type before install.
- Check disk space before start: `size_bytes + 512 MB`.
- Atomic install: write to a temp name, fsync, rename. On Windows, remove the destination first if it exists.
- One active download at a time; queue the rest.
- Pause/cancel/resume; resume after app restart.
- Local import: file picker → validate → copy to the managed models directory with progress. Allow “use in place” only in Advanced, with a warning if the file moves.

### 6.6 Licenses and attribution

- Show the model name, license, and source link before download.
- Do not require a checkbox for Apache-2.0/MIT; require explicit acceptance for Llama 3-licensed models.
- Include a Third-Party Notices page: llama.cpp (MIT), PrismML fork (MIT), each model’s license, Svelte/Tauri/Rust crates.
- Do not bundle model weights in the installer.
- Do not redistribute models from a private mirror without checking the license.

---

## 7. UX plan (Svelte)

### 7.1 Design principles

- **Calm and plain-spoken.** No jargon in the main flow.
- **One primary action per screen.**
- **Never make the user choose a technical parameter to get started.**
- **Show honest speed expectations** before and after download.
- **Keep the UI responsive** even when the CPU is saturated.
- **Accessible by default:** keyboard, screen reader, contrast, reduced motion.

Use “ultra-light AI models” in the UI. Mention “ternary” only in About/Technical details. Explain the value as “uses much less memory and runs without a graphics card.”

### 7.2 Onboarding

1. **Welcome**
   - “Your own AI. On your computer.”
   - “Nothing you type leaves this device. No account. No cloud.”
   - Primary: “Get started.”
2. **Hardware check**
   - Show RAM, cores, free disk in plain language.
   - “This PC can run: Fast (2B)” with a recommended badge.
   - If 4 GB/2-core: “Your PC is at the minimum. We’ll use the smallest model and short answers. Close other apps for the best experience.”
3. **Choose a model**
   - Cards for Fast / Balanced / Smart.
   - Show download size, memory fit, and “Recommended for your PC.”
   - Disable models that do not fit, with a plain reason (“Needs about 8 GB RAM”).
   - “More models” reveals 27B and experimental entries.
4. **Download**
   - Progress bar, downloaded/total, speed, ETA.
   - Pause/cancel.
   - “You can keep using your PC while this downloads.”
5. **Ready**
   - “Ready. Ask anything.”
   - Three suggested prompts.
   - Optional “Run a speed check” (runs automatically in the background).

### 7.3 Chat

- Sidebar: New chat, search, conversation list, active model badge, Settings.
- Main: messages, streaming cursor, Stop button, Regenerate, Copy.
- Composer: multiline; `Enter` sends, `Shift+Enter` newline; send button; character/token budget hint only in Advanced.
- During streaming: render plain text with a cursor. After completion: render sanitized Markdown with code blocks and copy buttons.
- Long conversations: virtualize the message list; keep the scroll pinned unless the user scrolls up; show “Jump to latest.”
- Footer: “Running locally” badge; after the first response, “About N words/sec” (Advanced: tok/s).
- Empty state: greeting + suggested prompts.
- Errors: inline card with a friendly message and actions (“Try again”, “Free memory”, “Choose a smaller model”, “Export diagnostics”).
- Stop: abort the request; keep the partial answer with an “Interrupted” label; allow regenerate.
- Regenerate: remove the last assistant message and resend the same user message.
- No editing/branching in v1.

### 7.4 Models page

- Installed models with size, last used, speed estimate, “Use this model,” delete.
- Available models with recommendation, size, RAM fit, license link.
- Storage summary and “Free up space.”
- Download queue with progress and pause/cancel.
- “Import a model file” with validation and clear errors.
- “More models” section for 27B and experimental entries, with badges and warnings.

### 7.5 Settings

**Simple tab:**

- Theme (system/light/dark)
- Language (English/Português do Brasil)
- Active model / Manage models
- “Keep AI ready” (keep model loaded)
- “Clear all chats”
- “Check for updates”
- “Export diagnostics”

**Advanced tab (behind a warning):**

- Threads (default: physical cores; cap 8)
- Context size (2048/4096/8192)
- Max output tokens
- Temperature, top-p, top-k, presence penalty
- System prompt
- Show reasoning (off by default)
- Engine tier (Auto; read-only unless debugging)
- Data folder, logs, benchmark, reset app

### 7.6 Error and state messages

| Cause | Message | Action |
|---|---|---|
| Engine crash | “The AI engine stopped unexpectedly.” | Restart / Export diagnostics |
| Out of memory | “This model needs more memory than is free.” | Try Fast model / Close apps |
| Corrupt model | “The model file is damaged.” | Re-download |
| No internet | “Connect to the internet to download a model.” | Retry / Import file |
| Slow PC | “This model is heavy for your PC.” | Switch to Fast / Reduce context |
| Unsupported CPU | “This PC is missing a required feature.” | Compatibility mode / Diagnostics |
| Disk full | “There isn’t enough free space for this model.” | Free space / Choose smaller model |
| Context overflow | “This conversation is too long for the model’s memory.” | Start a new chat / Reduce context |
| Download checksum | “The download was incomplete or damaged.” | Retry |
| Engine port conflict | “The AI engine could not start.” | Retry / Restart app |

### 7.7 Accessibility

- Keyboard: `Enter` send, `Shift+Enter` newline, `Esc` stop, `Ctrl/Cmd+N` new chat, `Ctrl/Cmd+K` search, `Ctrl/Cmd+,` settings.
- ARIA live region for streaming text; do not announce every token—announce sentence chunks or completion.
- Visible focus rings; focus management on route changes and dialogs.
- Contrast meets WCAG 2.1 AA; support 200% font scaling.
- Respect `prefers-reduced-motion`; disable non-essential animations.
- Screen-reader labels for icon-only buttons.
- Test with NVDA on Windows and Orca on Linux if available.

### 7.8 Internationalization

- `svelte-i18n` with `en.json` and `pt-BR.json`.
- All user-facing strings externalized; no hardcoded text in components.
- Dates/numbers via `Intl`.
- Model names and technical terms are not translated, but descriptions are.
- Add a language selector in onboarding and settings.

### 7.9 UI performance on weak machines

- Batch streamed deltas in Rust every ~50 ms; the frontend flushes on `requestAnimationFrame`.
- Virtualize the message list.
- Render plain text during streaming; parse Markdown only on completion.
- No syntax highlighting in v1; add lazy highlighting later.
- Avoid large images, blur effects, and continuous animations.
- Keep the sidebar list memoized; do not re-render it on every token.
- Use system fonts; no web font downloads.
- Target < 5% UI CPU during generation on the 2-core reference machine.

---

## 8. Technical stack and interfaces

### 8.1 Frontend

| Concern | Choice |
|---|---|
| Framework | Svelte 5 + TypeScript |
| Build | Vite + `@sveltejs/vite-plugin-svelte` |
| Styling | Tailwind CSS 4 |
| Headless UI | `bits-ui` (Svelte 5 compatible) |
| Icons | `lucide-svelte` |
| Toasts | `svelte-sonner` |
| Routing | Tiny custom route store for 4 screens; `svelte-spa-router` as fallback |
| State | Svelte 5 runes in `.svelte.ts` classes |
| Markdown | `marked` + `dompurify` (sanitize; no raw HTML) |
| Virtual list | `@tanstack/svelte-virtual`; fallback `svelte-virtual-list` |
| i18n | `svelte-i18n` |
| Tests | Vitest + `@testing-library/svelte`; Playwright for E2E |
| Lint/format | ESLint + Prettier + `svelte-check` |

Do not use SvelteKit; a plain Svelte + Vite SPA keeps the Tauri build simple and avoids SSR concerns.

### 8.2 Rust

| Concern | Crate |
|---|---|
| Desktop shell | `tauri` 2.x |
| Async | `tokio` (process, fs, io-util, sync, time) |
| HTTP/SSE | `reqwest` with `rustls-tls`, `stream`; `futures-util` |
| Serialization | `serde`, `serde_json` |
| Database | `rusqlite` with `bundled`; `rusqlite_migration` |
| Hashing | `sha2`, `hex` |
| IDs/time | `uuid` v7, `chrono` |
| Errors | `thiserror` (core), `anyhow` (app boundary) |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` |
| Hardware | `sysinfo`, `raw-cpuid` |
| Random | `rand` |
| Diagnostics | `zip` |
| Type export | `ts-rs` |
| Process priority | `libc` (Unix), `std::os::windows::process::CommandExt` (Windows) |
| Dev/test | `tempfile`, `wiremock`, `assert_cmd`, `serial_test` |

### 8.3 Tauri plugins and capabilities

Plugins:

- `tauri-plugin-dialog` (file picker)
- `tauri-plugin-opener` (external links, data/log folders)
- `tauri-plugin-clipboard-manager` (copy message/code)
- `tauri-plugin-single-instance`
- `tauri-plugin-window-state`
- `tauri-plugin-updater`
- `tauri-plugin-process` (relaunch after update)

No shell plugin. The Rust core spawns the engine with `tokio::process::Command`.

Capabilities: `core:default`, `dialog:default`, `opener:default`, `clipboard-manager:default`, `updater:default`, `process:allow-restart`, `window-state:default`. Do not grant broad filesystem access.

CSP baseline: `default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self' ipc: http://ipc.localhost; font-src 'self'`. Tighten after testing.

### 8.4 IPC commands

```text
get_bootstrap_state() -> BootstrapState
scan_hardware() -> HardwareProfile
list_models() -> Vec<ModelCard>
download_model(model_id) -> ()
pause_download(model_id) -> ()
resume_download(model_id) -> ()
cancel_download(model_id) -> ()
delete_model(model_id) -> ()
import_model(path) -> ModelCard
start_engine(model_id) -> EngineInfo
stop_engine() -> ()
engine_status() -> EngineState
send_message(request: SendMessageRequest) -> ()
cancel_generation(request_id) -> ()
regenerate(conversation_id) -> ()
list_conversations() -> Vec<ConversationSummary>
get_conversation(id) -> Conversation
create_conversation() -> Conversation
rename_conversation(id, title) -> ()
delete_conversation(id) -> ()
export_conversation(id, format) -> PathBuf
get_settings() -> Settings
update_settings(patch) -> Settings
run_benchmark(model_id) -> BenchmarkResult
export_diagnostics() -> PathBuf
open_data_dir() -> ()
open_logs() -> ()
check_for_updates() -> ()
```

### 8.5 Events

```text
download://progress   { model_id, bytes_done, bytes_total, speed_bps, eta_secs }
download://state      { model_id, state, error? }
engine://state        { state, model_id?, tier?, error? }
chat://delta          { request_id, delta, reasoning_delta?, index }
chat://done           { request_id, usage, elapsed_ms, tokens_per_sec }
chat://error          { request_id, error }
chat://cancelled      { request_id }
benchmark://progress  { model_id, stage, percent }
update://available    { version, notes }
```

### 8.6 SQLite schema

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA synchronous = NORMAL;

CREATE TABLE conversations (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  model_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  pinned INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE messages (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  reasoning TEXT,
  status TEXT NOT NULL,              -- complete | streaming | interrupted | error
  token_count INTEGER,
  created_at INTEGER NOT NULL,
  seq INTEGER NOT NULL
);

CREATE INDEX idx_messages_conversation ON messages(conversation_id, seq);

CREATE TABLE installed_models (
  id TEXT PRIMARY KEY,
  path TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  installed_at INTEGER NOT NULL,
  last_used_at INTEGER
);

CREATE TABLE downloads (
  model_id TEXT PRIMARY KEY,
  state TEXT NOT NULL,
  bytes_done INTEGER NOT NULL DEFAULT 0,
  bytes_total INTEGER NOT NULL DEFAULT 0,
  etag TEXT,
  updated_at INTEGER NOT NULL
);

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

On startup, mark any `streaming` messages as `interrupted`.

### 8.7 Settings schema

```json
{
  "schema_version": 1,
  "theme": "system",
  "language": "en",
  "active_model_id": "bonsai2-2b-pq2",
  "keep_model_loaded": true,
  "idle_unload_minutes": 30,
  "threads": null,
  "context_size": null,
  "max_tokens": null,
  "temperature": null,
  "top_p": null,
  "top_k": null,
  "presence_penalty": null,
  "system_prompt": "You are a helpful assistant. Answer clearly and concisely.",
  "show_reasoning": false,
  "telemetry_opt_in": false,
  "advanced_mode": false,
  "font_scale": 1.0
}
```

`null` means “use the model/hardware default.”

### 8.8 Repository layout

```text
tnn-local-ai-runner/
├─ Cargo.toml                         # workspace
├─ package.json
├─ vite.config.ts
├─ svelte.config.js
├─ tailwind.config.ts
├─ tsconfig.json
├─ AGENTS.md                          # conventions, commands, guardrails
├─ README.md
├─ crates/
│  ├─ tnn-core/
│  │  ├─ Cargo.toml
│  │  ├─ src/lib.rs
│  │  ├─ src/hardware.rs
│  │  ├─ src/engine/{mod,manager,args,health,stream,selftest,lifecycle}.rs
│  │  ├─ src/models/{mod,catalog,manifest,download,verify,gguf,install}.rs
│  │  ├─ src/chat/{mod,context,prompt,sse,events}.rs
│  │  ├─ src/storage/{mod,db,migrations,settings}.rs
│  │  ├─ src/paths.rs
│  │  ├─ src/errors.rs
│  │  ├─ src/diagnostics.rs
│  │  └─ tests/
│  └─ mock-engine/                    # deterministic fake llama-server
│     ├─ Cargo.toml
│     └─ src/main.rs
├─ src-tauri/
│  ├─ Cargo.toml
│  ├─ tauri.conf.json
│  ├─ capabilities/default.json
│  ├─ resources/engine/               # pinned PrismML archive, pruned
│  ├─ resources/engine-baseline/      # x64-only fallback
│  └─ src/{main,lib,commands,events,state}.rs
├─ src/                               # Svelte frontend
│  ├─ main.ts
│  ├─ App.svelte
│  ├─ routes/{Onboarding,Chat,Models,Settings}.svelte
│  ├─ components/{chat,models,settings,ui}/
│  ├─ stores/{chat,models,settings,engine}.svelte.ts
│  ├─ lib/{tauri,events,i18n,bindings,format,sanitize}.ts
│  ├─ locales/{en,pt-BR}.json
│  └─ styles/app.css
├─ scripts/
│  ├─ fetch-engine.sh
│  ├─ fetch-engine.ps1
│  ├─ prune-engine.sh
│  ├─ bench-engine.sh
│  ├─ verify-no-avx2.sh
│  └─ convert-experimental-model.sh
└─ docs/
   ├─ ARCHITECTURE.md
   ├─ ENGINE.md
   ├─ MODELS.md
   ├─ UX.md
   ├─ TESTING.md
   ├─ RELEASE.md
   └─ adr/
```

---

## 9. Implementation phases

Total estimate: **4–6 weeks** for one experienced developer. An AI agent can move faster but must stop at each gate for human review. Do not start Phase 3 until Phase 0 passes.

### Phase 0 — Feasibility and packaging spike (2–3 days)

**Tasks**

- Pin PrismML engine release and archive SHA-256.
- Download Windows x64 and Linux x64 CPU archives.
- Reproduce the spike on the Ivy Bridge machine; force `x64` and `ssse3` backends.
- Validate non-thinking mode and sampling with `llama-cli`/`llama-server`.
- Create a minimal Tauri app that ships the engine in resources and spawns it from the resource directory on Linux.
- Verify Windows DLL loading from the same directory in a Windows VM.
- Verify `chmod`/executable handling on Linux.
- Test a 4 GB Linux VM and a 4 GB Windows VM if available.
- Document results in `docs/ENGINE.md`.

**Exit criteria**

- Bonsai 2 2B produces coherent output on SSE2 and SSSE3.
- No illegal-instruction crash.
- Engine starts from a Tauri-managed resource directory on both platforms.
- 4 GB VM can load the 2B model and complete a short chat.
- If Windows/Linux resource spawning fails, switch to a static tiered engine build before proceeding.

### Phase 1 — Scaffold (2–3 days)

**Tasks**

- Create Cargo workspace and Tauri v2 app with Svelte 5 + TypeScript + Vite + Tailwind.
- Add Tauri plugins, capabilities, CSP, single-instance, window-state.
- Run local checks: `cargo fmt`, `cargo clippy -D warnings`, `cargo test`, `npm run check`, `npm run test`, production build.
- Build app shell with routes, theme, i18n scaffold, error boundary, logging.
- Build `mock-engine` with `/health`, `/props`, `/tokenize`, and streaming `/v1/chat/completions`.
- Add `AGENTS.md` with commands, conventions, and guardrails.

**Exit criteria**

- App launches on Windows and Linux.
- Mock chat streams end-to-end.
- Local checks pass.
- `ts-rs` type export works and drift is checked.

### Phase 2 — Core domain and storage (3–4 days)

**Tasks**

- Hardware detection and recommendation logic.
- Model manifest parsing/validation and GGUF header parser.
- SHA-256 utilities and URL allowlist.
- SQLite schema, migrations, settings, paths, typed errors.
- Context builder and token estimation.
- Unit tests for all of the above.

**Exit criteria**

- `cargo test` covers hardware mapping, manifest validation, GGUF parsing, DB migrations, context truncation, and settings.
- A synthetic 4 GB/2-core profile selects the 2B model and “Minimum mode.”

### Phase 3 — Engine manager and streaming (4–5 days)

**Tasks**

- Locate/spawn/health-check/stop the engine.
- Random port and API key.
- Capture logs; detect crash; kill on exit; orphan cleanup.
- Self-test, garbage detection, baseline fallback.
- SSE client, cancellation, usage/reasoning parsing, event batching.
- Process priority and idle unload.
- Integration tests with `mock-engine` and the real 2B model.

**Exit criteria**

- Real 2B chat works from an integration test.
- Cancel stops generation and persists a partial message.
- Killing the engine does not crash the app; restart works once.
- No orphan process after 10 restarts.
- Self-test fallback works when the primary engine directory is deliberately broken.

### Phase 4 — Model manager (3–4 days)

**Tasks**

- Download queue, resume, checksum, progress, pause/cancel, disk space.
- Atomic install, metadata, delete, storage summary.
- Local import and validation.
- License display and acceptance.
- Recommendation logic and model cards.
- Tests with `wiremock` for resume/checksum/disk-full.

**Exit criteria**

- 2B download survives a network drop and app restart.
- Corrupt file is rejected and deleted.
- Import and delete work.
- 4 GB machine can install and remove the 2B model without running out of space.

### Phase 5 — Svelte UX (5–7 days)

**Tasks**

- Onboarding flow.
- Chat UI with streaming, stop, regenerate, copy, virtualization.
- Models page and download queue.
- Settings (Simple/Advanced).
- Error states, empty states, accessibility, i18n.
- Performance batching and responsive layout.

**Exit criteria**

- A non-technical tester completes install → download → chat → stop → switch model without terminal or docs.
- UI remains responsive during generation on the 2-core reference machine.
- Keyboard-only and screen-reader smoke tests pass.
- English and pt-BR strings are complete.

### Phase 6 — Hardening, 4 GB mode, experimental models (4–6 days)

**Tasks**

- 4 GB/2-core “Minimum mode”: 2K context, 2 threads, 256 max tokens, batch 256/128, disable 4B/9B.
- Memory-pressure tests; OOM handling; context/batch auto-tuning.
- Crash recovery; partial message persistence; orphan cleanup.
- QEMU/VM compatibility tests on SSE2/SSSE3.
- Windows 10 no-AVX2 VM test.
- Experimental model conversion/validation (BitNet/Falcon) behind the gate.
- Security review: CSP, capabilities, URL allowlist, markdown sanitization, API key, localhost only.
- `cargo audit`, `cargo deny`, `npm audit`.

**Exit criteria**

- 4 GB Windows and Linux VMs complete a short chat with 2B.
- App RSS < 1.2 GB on the 4 GB reference VM; total system usage leaves headroom.
- No network requests during chat.
- Experimental models either pass the gate or are hidden.
- Security checklist is complete.

### Phase 7 — Packaging and release (3–5 days)

**Tasks**

- Windows NSIS per-user installer + optional MSI; WebView2 `embedBootstrapper`.
- Linux AppImage + `.deb`; document WebKitGTK dependencies.
- Sign Windows binaries/installer; checksum Linux artifacts.
- Tauri updater with signed manifest; Windows NSIS and Linux AppImage update paths.
- User docs, privacy page, licenses, third-party notices.
- Release checklist and clean-machine tests.

**Exit criteria**

- Clean install on Windows 10 4 GB/2-core and Linux 4 GB/2-core.
- Download, chat, stop, restart, update, uninstall.
- No orphan processes.
- Installer size and model download size are documented.
- Release artifacts and checksums published.

### Phase 8 — Optional future work

- macOS support (signing, notarization, dylib packaging).
- Vision (Bonsai 2 mmproj), tool calling, local API, RAG, speculative decoding.
- Pure-Rust ternary engine behind an `InferenceEngine` trait, with SSE2/SSSE3/AVX2/NEON/scalar parity tests.
- Do not let these into v1.

---

## 10. Testing and QA

### 10.1 Unit tests

- Hardware profile → model recommendation.
- Engine tier selection and fallback.
- Manifest parsing, URL allowlist, SHA-256.
- GGUF header parsing (magic, version, architecture, quant type).
- SSE parser: multi-line `data:`, `[DONE]`, usage, reasoning deltas, malformed JSON.
- Context truncation and token estimation.
- DB migrations and message status transitions.
- Settings validation and defaults.
- Path sanitization and model ID validation.

### 10.2 Integration tests

- `mock-engine`: start, health, chat, cancel, crash, restart.
- Real engine with 2B: start, warmup, chat, cancel, model switch.
- Download with `wiremock`: resume, checksum mismatch, disk full, HTTP 403/404/429.
- Import/delete model.
- Orphan cleanup and PID handling.
- Diagnostics zip contents and redaction.

### 10.3 E2E tests

- Playwright + `tauri-driver` on Linux and Windows.
- Onboarding with mock engine.
- Download progress and pause/resume with a local mock server.
- Chat streaming, stop, regenerate, copy.
- Model switch and settings persistence.
- Error states and recovery.
- Keyboard navigation and focus.

macOS is not tested in v1.

### 10.4 Compatibility matrix

| Environment | Purpose |
|---|---|
| Real Ivy Bridge i3-3217U, Linux, no AVX2 | Primary low-end reference; SSE2/SSSE3 forced backends |
| Windows 10 x64 VM, 4 GB RAM, 2 cores, no AVX2 CPU model | Minimum supported Windows |
| Linux x64 VM, 4 GB RAM, 2 cores, `-cpu qemu64`/Sandy Bridge | Minimum supported Linux and SSE2-only |
| Windows 10/11 x64, 8 GB, 4 cores | Recommended Windows |
| Linux x64, 8 GB, 4 cores | Recommended Linux |
| Windows 11 x64, 16 GB, 8 cores | 9B model |
| QEMU user-mode or full VM with `-cpu qemu64` | Prove no AVX/AVX2 instructions are required |

QEMU examples (adjust paths):

```bash
# User-mode check of the engine binary
qemu-x86_64 -cpu qemu64 -L /usr/x86_64-linux-gnu ./resources/engine/llama-server --version

# Full VM: boot a 4 GB Ubuntu image with a Sandy Bridge CPU model
qemu-system-x86_64 -m 4096 -smp 2 -cpu SandyBridge ...
```

If QEMU is not available locally, use a legacy CPU machine and document the manual gate.

### 10.5 Performance gates

| Scenario | Target |
|---|---|
| 2B on 2-core Ivy Bridge i3, 2 threads | ≥ 2.5 tok/s generation; ≥ 4 tok/s prompt; coherent |
| 2B on 4-core desktop, no AVX2 | ≥ 6 tok/s generation; ≥ 10 tok/s prompt |
| 4B on 4-core/8 GB | ≥ 3 tok/s generation |
| 9B on 8-core/16 GB | ≥ 2.5 tok/s generation |
| First token, 20-token prompt, 2B on i3 | < 8 s |
| App RSS, 2B on 4 GB VM | < 1.2 GB |
| Cold start | < 3 s |
| Model load, 2B on SSD | < 10 s |
| UI CPU during generation | < 5% on the 2-core reference |
| Memory growth over 20 turns | < 10% |

If a gate fails, fix the default (context, batch, threads, max tokens) before shipping.

### 10.6 Security tests

- Markdown XSS: raw HTML, `javascript:` links, image `onerror`, SVG payloads.
- URL allowlist bypass attempts.
- Path traversal in model IDs and import paths.
- API key never appears in logs, diagnostics, or frontend state.
- Engine not reachable from another machine.
- CSP violations.
- `cargo audit`, `cargo deny`, `npm audit`.
- Tauri capability review: no broad FS/shell permissions.

### 10.7 Local validation

- Run formatting, clippy, Rust tests, frontend checks, tests and builds locally.
- Fetch and verify the pinned engine locally before running compatibility checks.
- Run QEMU SSE2/SSSE3 checks or test on a legacy CPU machine manually.
- Build installers locally with `npm run tauri build`; see `docs/RELEASE.md`.

---

## 11. Packaging and release

### 11.1 Windows

- Targets: NSIS (primary, per-user, no admin) and MSI (optional enterprise).
- WebView2: `embedBootstrapper` so the installer can bootstrap the runtime; most Windows 10/11 machines already have it.
- Engine: `resources/engine/` with `llama-server.exe` and DLLs in the same directory.
- Sign the app executable, installer, and engine binaries with Authenticode.
- Test on Windows 10 21H2/22H2 and Windows 11, including a 4 GB VM.
- Long paths: models are under `%APPDATA%`; verify paths with spaces and non-ASCII usernames.
- Antivirus: avoid packing/UPX; sign binaries; document false-positive recovery.

### 11.2 Linux

- Targets: AppImage (portable) and `.deb` (recommended for Ubuntu/Debian).
- WebKitGTK 4.1 is required. The `.deb` declares dependencies; the AppImage documents the required system packages.
- Build on Ubuntu 22.04 for glibc 2.35 compatibility; test on Ubuntu 22.04 and 24.04.
- Test on a 4 GB/2-core VM and on the real Ivy Bridge machine.
- If AppImage dependency friction is too high for non-technical users, prioritize the `.deb` and consider Flatpak in a later phase.
- Provide checksums and a signed release manifest.

### 11.3 Updater

- Tauri updater with signed JSON manifest.
- Windows NSIS and Linux AppImage update in place; `.deb` users update via the package manager or a download link.
- Check after 5 seconds on startup; show a non-intrusive banner.
- Never update the engine or models silently during a chat.
- Keep the previous version recoverable if the update fails.

### 11.4 Installer and download sizes

| Item | Approx. size |
|---|---:|
| Tauri app + frontend | 10–15 MB |
| Engine (pruned) | 20–30 MB |
| Windows installer | 30–50 MB |
| Linux AppImage | 30–50 MB |
| Bonsai 2 2B model | 0.52 GB |
| Bonsai 2 4B model | 1.15 GB |
| Bonsai 2 9B model | 2.42 GB |
| Bonsai 2 27B model | 5.95–7.21 GB |

Models are downloaded separately; they are not bundled.

### 11.5 Release checklist

- [ ] Clean install on Windows 10 4 GB/2-core.
- [ ] Clean install on Linux 4 GB/2-core.
- [ ] 2B download, resume, checksum, chat, stop, restart.
- [ ] 4B on 8 GB; 9B on 16 GB.
- [ ] No network during chat.
- [ ] No orphan engine process.
- [ ] Engine self-test fallback works.
- [ ] Update check and install work.
- [ ] Uninstall/cleanup documented.
- [ ] Licenses and third-party notices present.
- [ ] Diagnostics export redacts prompts.
- [ ] Accessibility smoke test.
- [ ] English and pt-BR strings complete.
- [ ] Checksums and signatures published.

---

## 12. Diagnostics and observability

- Rotating app log: 5 MB × 3 files in app data.
- Engine stdout/stderr: last 200 lines in memory, plus a rotating engine log.
- Log levels: info in release, debug in dev; no prompts or API keys.
- Metrics stored locally: model load time, first-token latency, prompt/generation tok/s, peak RSS, engine tier, fallback events.
- “Export diagnostics” creates a zip with:
  - App version, engine version, OS, CPU features, RAM, disk free.
  - Model manifest and installed-model metadata.
  - Settings with secrets/prompts redacted.
  - Last 500 app log lines and engine log tail.
  - Last error and recent crash events.
  - Optionally the last conversation, only with explicit user consent.
- No automatic upload. “Report a problem” opens a GitHub issue with a user-reviewed summary.

---

## 13. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| 2-core ULV CPUs only reach ~3 tok/s | Poor UX | Officially support but label as minimum; default 2B/2K/2 threads/256 tokens; benchmark and warn; short answers; non-thinking |
| 4 GB RAM is tight with WebView + engine + model | OOM/crashes | Minimum mode; 2B only; 2K context; batch 256/128; no downloads during chat; test on 4 GB VMs |
| Prompt processing is slow on old CPUs | Long wait before first token | Short system prompt; history truncation; show “Reading…” state; recommend 2B; avoid long pasted text |
| PrismML fork/model churn | Build breaks, gibberish | Pin engine release + model revision + SHA; vendor archive; re-run spike before upgrades; baseline fallback |
| Windows DLL loading/AVX2 issues | Engine won’t start | Ship CPU archive with `x64`/`ssse3`; same-directory DLLs; baseline fallback; test no-AVX2 Windows VM |
| Linux WebKitGTK dependency | App won’t launch | `.deb` with declared deps; AppImage docs; test Ubuntu 22.04/24.04; consider Flatpak later |
| Experimental BitNet/Falcon models fail speed/quality | Bad user experience | Keep behind “Experimental”; validation gate; do not block Bonsai 2 launch |
| Bonsai 2 still thinks despite settings | Long waits | `--reasoning-budget 0` + per-request kwargs; test each revision; hide reasoning by default |
| Model/download supply chain | Corrupt or malicious file | Pinned HF revision, SHA-256, GGUF validation, HTTPS allowlist, no remote manifest |
| UI lag during generation | Feels broken | Batch deltas, virtualize, plain-text streaming, engine below-normal priority |
| Scope creep (vision/tools/API) | Delays v1 | Explicit non-goals; Phase 8 only |
| Users expect ChatGPT speed | Disappointment | Onboarding honesty; words/sec; “private and offline” value; model tiers |
| No signing certificate | SmartScreen warnings | Start unsigned for internal builds; obtain Authenticode before public release |
| Orphan engine processes | RAM leak | PID file, process-name check, kill on exit, Job Object/SIGTERM fallback |

---

## 14. Remaining decisions and assumptions

These are lower-risk and can be decided during Phase 0/1:

1. **Windows minimum version:** assume Windows 10 21H2+ (WebView2 and Tauri support). Confirm if 1809/1909 must be supported.
2. **Linux distro priority:** assume Ubuntu 22.04/24.04 and Debian 12; AppImage is best-effort, `.deb` is primary. Confirm if Fedora is required.
3. **Branding:** `TNN Local AI` is a placeholder. Final name/icon can change without architecture changes.
4. **Experimental model scope:** BitNet 0.7B/2B4T and Falcon3 1B/3B/7B/10B are included behind a validation gate. If any fails, hide it; do not delay v1.
5. **Llama3-8B-1.58:** optional, requires Llama 3 license acceptance; likely hidden on 4 GB machines.
6. **Telemetry:** assume none. If crash reporting is added later, it must be opt-in and exclude prompts.
7. **App license:** assume MIT or Apache-2.0 for the app; third-party notices included.
8. **Auto-update:** assume on by default with a banner; allow disabling in Advanced.

---

## 15. Handoff checklist for the implementing agent

Before starting:

- [ ] Read this plan, `docs/ARCHITECTURE.md`, `docs/ENGINE.md`, and `docs/MODELS.md`.
- [ ] Reproduce the Phase 0 spike on the Ivy Bridge machine.
- [ ] Confirm the pinned engine release and archive SHA-256.
- [ ] Confirm the model revision commits and SHA-256 values.
- [ ] Confirm Windows/Linux resource spawning works from Tauri.

Guardrails:

- Do not use bitnet.cpp as the engine.
- Do not build a custom inference engine in v1.
- Do not require AVX2.
- Do not enable thinking mode by default.
- Do not pass `-c 0`.
- Do not use more threads than physical cores.
- Do not expose the engine to the LAN or enable its web UI.
- Do not bundle model weights in the installer.
- Do not add a model without a license, pinned SHA-256, and benchmark.
- Do not add telemetry or cloud calls.
- Do not let vision/tools/API/RAG into v1.
- Do not ship without testing on a 4 GB/2-core machine.
- Stop at each phase gate for human review.

Definition of done for v1:

- [ ] Windows and Linux installers are available.
- [ ] A 4 GB/2-core machine can install, download 2B, and chat at ≥ 2.5 tok/s.
- [ ] An 8 GB/4-core machine can run 4B at ≥ 3 tok/s.
- [ ] No AVX2 requirement; SSE2/SSSE3 fallback verified.
- [ ] No network during chat.
- [ ] No orphan processes.
- [ ] All errors have friendly messages and recovery actions.
- [ ] Accessibility and i18n checks pass.
- [ ] Licenses, privacy, and diagnostics are complete.
- [ ] Release artifacts are signed/checksummed and the updater works.

---

## Appendix A — Engine command templates

### A.1 4 GB / 2-core / 2B

```bash
llama-server \
  --model "$MODEL" \
  --host 127.0.0.1 \
  --port "$PORT" \
  --api-key "$API_KEY" \
  --ctx-size 2048 \
  --threads 2 \
  --threads-batch 2 \
  --parallel 1 \
  --batch-size 256 \
  --ubatch-size 128 \
  --n-gpu-layers 0 \
  --load-mode mmap \
  --no-ui \
  --jinja \
  --reasoning-budget 0 \
  --reasoning-format deepseek \
  --flash-attn off
```

### A.2 8 GB / 4-core / 4B

```bash
llama-server \
  --model "$MODEL" \
  --host 127.0.0.1 \
  --port "$PORT" \
  --api-key "$API_KEY" \
  --ctx-size 4096 \
  --threads 4 \
  --threads-batch 4 \
  --parallel 1 \
  --batch-size 512 \
  --ubatch-size 256 \
  --n-gpu-layers 0 \
  --load-mode mmap \
  --no-ui \
  --jinja \
  --reasoning-budget 0 \
  --reasoning-format deepseek \
  --flash-attn off
```

### A.3 16 GB / 8-core / 9B

```bash
llama-server \
  --model "$MODEL" \
  --host 127.0.0.1 \
  --port "$PORT" \
  --api-key "$API_KEY" \
  --ctx-size 8192 \
  --threads 6 \
  --threads-batch 6 \
  --parallel 1 \
  --batch-size 1024 \
  --ubatch-size 512 \
  --n-gpu-layers 0 \
  --load-mode mmap \
  --no-ui \
  --jinja \
  --reasoning-budget 0 \
  --reasoning-format deepseek \
  --flash-attn off
```

### A.4 Chat request body

```json
{
  "model": "local",
  "messages": [
    { "role": "system", "content": "You are a helpful assistant. Answer clearly and concisely." },
    { "role": "user", "content": "Hello!" }
  ],
  "stream": true,
  "stream_options": { "include_usage": true },
  "max_tokens": 256,
  "temperature": 0.7,
  "top_p": 0.8,
  "top_k": 20,
  "min_p": 0.0,
  "presence_penalty": 1.5,
  "repetition_penalty": 1.0,
  "repeat_last_n": 64,
  "chat_template_kwargs": { "enable_thinking": false }
}
```

---

## Appendix B — Benchmark script

`scripts/bench-engine.sh` should:

1. Accept `--model`, `--threads`, `--runs`.
2. Run `llama-bench` with `-p 64 -n 32 -ngl 0 -r <runs>`.
3. Parse JSON output.
4. Print prompt t/s, generation t/s, model size, and thread count.
5. Append results to `docs/benchmarks.md` with date, CPU, RAM, OS, engine version, and model.

Example:

```bash
./scripts/bench-engine.sh \
  --model "$HOME/.local/share/tnn/models/Ternary-Bonsai-2-2B-PQ2_0.gguf" \
  --threads 2 \
  --runs 3
```

Record the validated baseline:

```text
Machine: Intel i3-3217U, 2C/4T, 1.8 GHz, 9.6 GB, no AVX2, Linux x64
Engine: PrismML prism-b10826-c9c34bc
Model: Ternary-Bonsai-2-2B-PQ2_0.gguf
Threads: 2
pp64: 4.75 t/s
tg32: 2.98 t/s
```

---

## Appendix C — Error codes

| Code | Meaning | User message | Recovery |
|---|---|---|---|
| `engine_spawn_failed` | Process could not start | “The AI engine could not start.” | Retry / Diagnostics |
| `engine_crash` | Process exited unexpectedly | “The AI engine stopped unexpectedly.” | Restart |
| `engine_timeout` | Health check timed out | “The AI engine is taking too long to start.” | Retry / Smaller model |
| `engine_illegal_instruction` | SIGILL / 0xC000001D | “This PC is missing a required CPU feature.” | Compatibility mode / Diagnostics |
| `engine_garbage_output` | Self-test failed | “The AI engine is not working correctly on this PC.” | Baseline fallback / Diagnostics |
| `model_not_found` | File missing | “The model file is missing.” | Re-download |
| `model_corrupt` | SHA/GGUF invalid | “The model file is damaged.” | Re-download |
| `model_unsupported` | Format/architecture unknown | “This model is not supported.” | Choose another model |
| `disk_full` | Not enough space | “There isn’t enough free space.” | Free space |
| `download_failed` | Network/HTTP error | “The download failed.” | Retry / Resume |
| `download_checksum` | SHA mismatch | “The download was incomplete or damaged.” | Retry |
| `out_of_memory` | Allocation/load failure | “This model needs more memory than is free.” | Smaller model / Close apps |
| `context_overflow` | Prompt too long | “This conversation is too long for the model’s memory.” | New chat / Reduce context |
| `canceled` | User canceled | “Generation stopped.” | Regenerate |
| `unsupported_platform` | OS/arch | “This version of the app does not support this PC.” | Documentation |

---

## Appendix D — Acceptance test script (manual)

1. Fresh Windows 10 VM, 4 GB RAM, 2 cores, no AVX2 CPU model.
2. Install the app; confirm no admin prompt.
3. Onboarding shows “Fast” recommendation and minimum-mode warning.
4. Download 2B; pause at 30%, close the app, reopen, resume.
5. Confirm SHA verification and model installed.
6. Send “Hello, what can you do?”; confirm streaming, coherent answer, Stop works.
7. Confirm UI remains responsive; measure RSS.
8. Switch to a non-installed 4B model; confirm it is disabled with a reason.
9. Export diagnostics; confirm no prompts/API key in the zip.
10. Restart the app 10 times; confirm no orphan `llama-server` process.
11. Disconnect the network; confirm chat still works.
12. Repeat on a fresh Linux 4 GB VM with the `.deb`.
13. Repeat on an 8 GB/4-core machine with 4B.
14. Repeat on a 16 GB/8-core machine with 9B.
15. Verify no network traffic during chat with a proxy/log.

---

## Appendix E — References

- PrismML llama.cpp fork: <https://github.com/PrismML-Eng/llama.cpp>
- PrismML Bonsai 2 documentation: <https://docs.prismml.com/>
- Bonsai 2 2B model card: <https://docs.prismml.com/models/bonsai-2-2b>
- Bonsai 2 formats and runtime support: <https://docs.prismml.com/download/formats>
- Bonsai 2 server guide: <https://docs.prismml.com/run/server>
- Tauri v2 documentation: <https://v2.tauri.app/>
- Tauri sidecar/embedding external binaries: <https://v2.tauri.app/develop/sidecar/>
- Svelte 5 documentation: <https://svelte.dev/docs/svelte/overview>
- bitnet.cpp non-AVX2 issue: <https://github.com/microsoft/BitNet/issues/547>
- BitNet b1.58 paper: <https://arxiv.org/abs/2402.17764>
- Bitnet.cpp technical report: <https://arxiv.org/abs/2502.11880>

---

*This plan is intentionally implementation-ready but not implementation-prescriptive beyond the hard constraints. The implementing agent may adjust internal APIs and component boundaries, but must preserve the engine pin, no-AVX2 guarantee, non-thinking defaults, 4 GB support, security model, and phase gates.*
