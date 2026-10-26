# Windows hardware testing

Target: Windows 10/11 **x64** on Intel or AMD CPUs. AVX2 and a GPU are not
required. The same Qwen catalogue, non-thinking defaults, explicit context
sizes and physical-core thread limits apply on Linux and Windows.

Windows build configuration and local build scripts are available. A native
Windows build, installer launch and real-model hardware checks still need to
be run on a Windows PC; Linux checks do not establish Windows runtime support.

## Build on one Windows PC

Install these development prerequisites once:

- Node.js 22+ with npm.
- Current stable Rust with the MSVC toolchain (`x86_64-pc-windows-msvc`).
- Microsoft C++ Build Tools: the **Desktop development with C++** workload,
  including MSVC x64/x86 build tools and a Windows 10/11 SDK.
- Microsoft Edge WebView2 Runtime for desktop development.

See [Tauri's Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows)
for the official installers and component selection.

Open **64-bit PowerShell** in the repository and run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-windows.ps1
```

The execution-policy option applies to this process. The script installs
locked npm dependencies, fetches the checksum-pinned Windows engine, verifies
native and SSE2 engine startup with `--version`, runs local checks and builds
an NSIS installer. Visual C++ runtime errors at this step mean the runtime
prerequisite below needs installing. `-SkipChecks` skips source checks for a
repeat build; engine verification still runs.

The Windows configuration is merged automatically by Tauri. It selects NSIS,
embeds the full WebView2 offline installer, and disables updater-signature
artifacts for these local test builds. No updater private key is required.
The repository has no automated publishing or deployment.

With the default Cargo target directory, the output is:

```text
target\x86_64-pc-windows-msvc\release\bundle\nsis\*-setup.exe
```

The script prints the actual directory, including a custom `CARGO_TARGET_DIR`,
and writes a `.sha256` file beside each installer. Copy the setup executable
to each test PC; it includes the engine DLLs and its separate SSE2 fallback.
Model weights are downloaded or imported separately.

The test installer has no Authenticode signature. Windows may show an
unknown-publisher/SmartScreen prompt. Use your own locally built installer
and verify its SHA-256 after copying it:

```powershell
Get-FileHash -Algorithm SHA256 '.\TNN-Local-AI-setup.exe'
```

Replace the example filename with the generated setup filename and compare
with its `.sha256` file. Public distribution/signing remains a separate gate.

## Prepare each test PC

Install the **Microsoft Visual C++ v14 Redistributable, x64**, if it is not
already installed. The pinned engine imports `MSVCP140.dll`, `VCRUNTIME140.dll`
and `VCRUNTIME140_1.dll`, which are not included in its archive. Microsoft's
[redistributable download page](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)
links to the [x64 installer](https://aka.ms/vc14/vc_redist.x64.exe). You can
save that installer in advance and copy it alongside the app for offline setup.
The test PCs need the runtime, not the C++ development tools.

Run the app's setup executable and launch TNN Local AI. The installer includes
WebView2's offline installer; see [Tauri's WebView2 installation options](https://v2.tauri.app/distribute/windows-installer/#webview2-installation-options).
Download a Qwen model once, or import its existing GGUF file, then test chat
without a network connection. Application data is stored in
`%APPDATA%\ai.tnn.localai`, including conversations, models and logs.

## Compare machines

1. Start with Qwen 3 0.6B or Qwen 2.5 Instruct 1.5B. Try 7B on machines with
   enough RAM; 16 GB is recommended. Use the same model file on each machine.
2. Record Windows version, CPU, physical cores, RAM, selected engine tier,
   context size and thread count. Keep context at 2048 for comparison and
   leave thinking disabled. Thread overrides must stay within physical cores.
3. Run the model's benchmark action and record prompt/generation tokens per
   second. These are short samples; repeat under similar machine load.
4. Verify chat, Stop, Continue answer, saved conversations and exports.
5. Pause/resume a download and restart the app. Test a Windows user/profile
   path containing spaces and non-ASCII characters.
6. Quit the app and confirm its `llama-server.exe` process exits. Repeat
   after a failed model load. Export redacted diagnostics if something fails.

## Development and deeper checks

For desktop development after installing prerequisites:

```powershell
npm.cmd ci
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\fetch-engine.ps1
npm.cmd run tauri -- dev
```

Use `npm.cmd` to avoid PowerShell's `npm.ps1` execution-policy restrictions.
The fetch script reads `engine.lock.json`, stages and verifies both payloads,
and restores the previous pair if installation fails. Incomplete cached
payloads are repaired. It preserves all CPU dispatch DLLs in the main engine
and only `ggml-cpu-x64.dll` in the fallback.
Close the app before replacing engine resources with `-Force`, so Windows
can release the executable and DLL file handles.

Run the PowerShell fetch fixtures without downloading an engine or models:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\windows-scripts.ps1
```

After downloading the exact Qwen 1.5B catalogue file, test production-manager
inference on native, isolated SSE2 and isolated SSSE3 backends:

```powershell
$env:TNN_TEST_MODEL = Join-Path $env:APPDATA 'ai.tnn.localai\models\qwen2.5-1.5b-instruct-q4_k_m.gguf'
cargo test -p tnn-core --target x86_64-pc-windows-msvc --test real_engine -- --ignored --nocapture
cargo test -p tnn-core --target x86_64-pc-windows-msvc --test continuation -- --ignored --nocapture
Remove-Item Env:TNN_TEST_MODEL
```

The first test checks size, hash, architecture, streaming without reasoning,
short benchmark timings and PID cleanup. Set `TNN_TEST_MODEL_ID` as well to
test a different catalogue entry. Baseline backends can take several minutes.
Forced backends do not replace testing on a real older CPU or a full VM.
Record the results in `docs/benchmarks.md`, including hardware and limitations.
