# TNN Local AI

Private, CPU-only AI chat for ordinary computers. Download one small model
once, then talk to it entirely on your machine. No account, no cloud, no
telemetry, and no network connection during a chat.

- Tauri v2 desktop app (Linux x64; Windows x64 test builds available)
- Experimental Android ARM64 APK builds; see [Android setup and phone testing](docs/ANDROID.md)
- Svelte 5 + TypeScript frontend, Rust core, SQLite storage
- Pinned [PrismML llama.cpp](https://github.com/PrismML-Eng/llama.cpp) CPU build
  with runtime dispatch: SSE2/x64, SSSE3 and newer tiers, no AVX2 requirement
- Small Qwen 2.5 Instruct 0.5B and Qwen 3 0.6B Q4_K_M choices for basic chat
  on older CPUs, with thinking disabled
- Qwen 2.5 Instruct 1.5B and 7B Q4_K_M choices for larger local chats
- English and Brazilian Portuguese UI

> **Status:** working core and UI, not a public release. Engine binaries and
> model weights are not committed. Packaging, code signing and a clean 4 GB
> machine test are still required before v1.

Windows native build and runtime acceptance checks are pending. See
[Windows hardware testing](docs/WINDOWS.md) to build an installer and test on
your Windows PCs.

Current phase evidence and remaining review gates are recorded in
[docs/IMPLEMENTATION.md](docs/IMPLEMENTATION.md).

## Requirements for development

- Node.js 22+ and npm
- Rust 1.85+ (`rustup show` should select the workspace toolchain)
- Linux: WebKitGTK 4.1, GTK 3, and the usual Tauri build packages
- Windows: Microsoft C++ Build Tools, Windows SDK and WebView2;
  see [Windows setup](docs/WINDOWS.md).

## Quick start

### Linux

```bash
npm install
npm run check
npm run test

# Fetch the pinned engine into src-tauri/resources (verified SHA-256)
./scripts/fetch-engine.sh

# Desktop development
npm run tauri dev
```

The app starts even without the engine: onboarding reports that the engine is
missing and model downloads remain available. Chat needs the engine and a model.

### Windows

After installing the [Windows prerequisites](docs/WINDOWS.md), open 64-bit
PowerShell in the repository and build a local test installer:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-windows.ps1
```

The script prints the setup executable's path and writes its SHA-256. Copy
the installer to your other Windows x64 PCs. Each test PC also needs the
Microsoft Visual C++ x64 runtime described in the Windows guide.

### Frontend-only preview

```bash
npm run dev
```

The browser build renders the shell, but IPC calls show the “open the desktop
app” message. It is useful for styling, not for functional testing.

## Useful commands

| Command | Purpose |
|---|---|
| `npm run check` | Svelte + TypeScript diagnostics |
| `npm run test` | Vitest unit tests |
| `npm run build` | Production frontend build |
| `cargo test --workspace` | Rust unit and integration tests |
| `cargo fmt --all --check` | Formatting gate |
| `npm run tauri dev` | Run the desktop app |
| `scripts/fetch-engine.sh` | Download and prune the pinned engine |
| `scripts/fetch-engine.ps1` | Download and verify the Windows engine and SSE2 fallback |
| `scripts/build-windows.ps1` | Check and build a local Windows x64 test installer |
| `scripts/verify-no-avx2.sh` | Start the engine under an SSE2-only QEMU CPU |
| `scripts/bench-engine.sh` | Measure prompt/generation speed |
| `npm run tauri build` | Build desktop installers locally |

See [docs/TESTING.md](docs/TESTING.md) for the full test matrix and
[docs/RELEASE.md](docs/RELEASE.md) for signing and packaging.

## Repository layout

```text
crates/tnn-core/      domain, storage, downloads, engine supervisor, chat
crates/mock-engine/   deterministic fake llama-server for tests
src-tauri/            Tauri wiring, IPC commands, capabilities, resources
src/                  Svelte 5 UI, stores, types generated from Rust
scripts/              engine fetch/prune, benchmark, compatibility
docs/                 architecture, engine, models, UX, testing, release, ADRs
engine.lock.json      pinned engine version and archive hashes
```

## Privacy and security

- The engine binds to `127.0.0.1` on a random port with a per-session API key.
  The frontend never receives the key or talks to the engine directly.
- The CSP forbids remote scripts, fonts and images. There is no shell plugin
  and no broad filesystem access.
- Markdown output is sanitized; links are restricted to `http`/`https` before
  they reach the operating system opener.
- Downloads are HTTPS-only, host-allowlisted, revision-pinned and verified
  against a SHA-256 in the embedded catalogue before installation.
- Diagnostics are user-initiated and exclude prompts by default.
- No telemetry. The `telemetry_opt_in` setting exists only for forward
  compatibility and is off; nothing is sent anywhere.

## Models and licences

The catalogue is embedded at build time. Qwen entries are validated and
downloadable; BitNet, Falcon3 and Llama3 entries remain unavailable pending
validation or after rejection. Model weights are never
bundled in the installer.

Unfinished answers are labelled and can be extended with **Continue answer**.
The app keeps the original text and resumes in the same message. On a slow
PC with at least 8 GB RAM, Advanced settings can also raise the per-reply
limit; 4 GB machines keep the short default and can continue in chunks.

The application is dual-licensed under MIT or Apache-2.0. The pinned engine is
MIT-licensed; see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

## Known gaps before v1

- Generate and protect the updater keypair, then set a real update endpoint.
- Test clean installs on Linux and Windows with 4 GB RAM / 2 cores.
- Run the SSE2/SSSE3 compatibility gate on real hardware or a full VM.
- Finish the experimental model validation gate or keep those entries hidden.
- Add platform code signing and publish checksums/updater manifests.
