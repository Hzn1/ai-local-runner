# Android ARM64 hardware test build

Android support is experimental. The APK is for local phone testing, not a
public release. Android device acceptance is separate from the Linux tests;
successful compilation does not establish inference, lifecycle or thermal
behavior on a phone.

## Phone requirements

- Android 9 (API 28) or newer, with a **64-bit Android OS** supporting
  `arm64-v8a`. A 64-bit CPU with a 32-bit OS cannot install this APK.
- An updated Android System WebView/Chrome.
- At least 4 GB RAM and sufficient free internal storage for a downloaded
  model plus 512 MB headroom. Start with Qwen 2.5 0.5B or Qwen 3 0.6B.
- Internet access for the first model download; chat then runs locally.

The initial target phone is a Redmi 13 with 8 GB RAM and Helio G91-Ultra.
Android defaults to the existing Qwen 3 0.6B recommendation and up to four
physical-core threads, with thinking disabled. This is a conservative starting
point for testing, not a measured speed guarantee. Larger models stay
selectable. Xiaomi lists the device's Cortex-A55/A75 CPU on its
[specifications page](https://www.mi.com/pk/product/redmi-13/specs/).

## Build on Linux

Use Node 22+, the workspace Rust toolchain, Python 3, curl, tar, JDK 21,
and Android command-line tools. SDK packages used by this project:

```bash
sdkmanager 'platform-tools' 'platforms;android-36' 'build-tools;35.0.0' 'build-tools;36.0.0' \
  'ndk;28.2.13676358' 'cmake;3.22.1'
rustup target add aarch64-linux-android
```

Set `JAVA_HOME`, `ANDROID_HOME`, and `NDK_HOME` if using an existing SDK.
`scripts/android-env.sh` also recognizes the per-user tools installed under
`~/.local/share/tnn-android-tools/`. It caps Cargo jobs at two, disables
incremental compilation and omits Rust debug symbols to conserve disk space.
The Android project is checked in under `src-tauri/gen/android`; do not
regenerate it with `tauri android init`, which can overwrite the native
packaging, document-sharing and security configuration.

```bash
npm ci
npm run android:build
```

The script verifies the pinned engine source archive, cross-compiles the
existing PrismML engine for baseline ARMv8-A, runs frontend checks, builds a
signed debug APK with embedded frontend assets, verifies the actual APK,
and writes:

```text
artifacts/android/tnn-local-ai-0.1.0-android-arm64.apk
artifacts/android/SHA256SUMS
```

The APK does **not** require a development server. Gradle uses the local
Android debug signing key; do not commit that key. Updates to this test APK
must use the same signing key, or Android requires uninstalling first.
Uninstalling deletes the app's conversations, models and settings. There is
no Android in-app updater or Play Store release/signing setup.

## Install and test

Copy the APK to your phone, open it in the file manager and allow that app
to install unknown apps when Android asks. Or use USB debugging:

```bash
source scripts/android-env.sh
adb devices
adb install -r artifacts/android/tnn-local-ai-0.1.0-android-arm64.apk
```

1. Open TNN Local AI. Confirm onboarding and hardware detection succeed.
2. Download a small Qwen model. Pause/resume it, then restart the app and
   resume a partial download. Keep the app visible while downloading.
3. Start a new chat and ask a short question. Check streaming, Stop,
   regeneration and continuation. Switch English/Portuguese and rotate
   the phone; check navigation and the on-screen keyboard.
4. Enable airplane mode after the model is installed. Send another message
   to establish that inference works offline on this phone.
5. Leave the app during a response. Generation should stop, the partial
   answer should remain, and returning should allow another message.
6. Import a local GGUF using the Android document picker. Export a chat
   and diagnostics through Android's share sheet.
7. Force-stop the app while a model is loaded, then relaunch. Verify that
   no stale engine blocks the next model load.

Record phone model, Android/WebView version, RAM, chosen model, startup
time, speed and any failures. Do not include prompts, private model paths
or session API keys in diagnostic reports.

## Platform implementation

- The existing `tnn-core` owns downloads, SQLite, model validation, engine
  arguments, authentication and streaming. No cloud inference is added.
- Android forbids executing downloaded app-data binaries. The same pinned
  engine is built as a PIE executable named `libtnn_engine.so` and packaged
  under `lib/arm64-v8a/`. Native-library extraction is enabled. Rust resolves
  `ApplicationInfo.nativeLibraryDir` through JNI and launches it directly.
- Engine code uses ARMv8-A without optional dot-product, i8mm, SVE, GPU or
  OpenMP requirements. Model weights remain separate. The engine licence
  is embedded as an APK asset.
- NDK r28c's default app-library alignment and an explicit engine linker flag
  support 16 KB pages. The artifact verification
  checks both ELF files' ARM64 class and load-segment alignment.
- The engine binds only to `127.0.0.1`, uses a random port/session key, and
  disables its web UI and thinking. The webview never calls it directly.
- Android activity suspension cancels generation, unloads the model and
  pauses downloads. There is no background service, wake lock or background
  inference promise. Linux parent-death signaling kills the child if Android
  kills the main app process without a normal shutdown callback.
- App data is private under Android's `files/` directory; backups are disabled.
  The document picker grants read access to a selected file descriptor. The
  core validates and hashes it before atomic installation, without copying
  it to an intermediate full-size staging file.
- A non-exported FileProvider shares only `files/exports/`, using temporary
  URI grants. No broad storage, shell or filesystem plugin is added.
- Desktop-only single-instance, window-state, process and updater plugins
  are excluded from mobile builds. Android has a separate capability file.

References: [Tauri Android prerequisites](https://tauri.app/start/prerequisites/),
[Android execution restrictions](https://developer.android.com/about/versions/10/behavior-changes-10),
[Android 16 KB page sizes](https://developer.android.com/guide/practices/page-sizes),
and [AGP 8.11 compatibility](https://developer.android.com/build/releases/agp-8-11-0-release-notes).

## Local validation, 2026-10-26

- ARM64 Rust app and pinned ARMv8-A engine cross-compilation succeeded on Linux.
- `npm run android:build` produced a signed 31,415,514-byte debug APK with
  production frontend assets, the native engine and its licence.
- APK v2 signature, ARM64-only payload, both ELF load-segment alignments,
  zip alignment, min API 28, native-code extraction, disabled backups,
  packaged HTTPS frontend configuration and narrow permissions were verified.
- Frontend: 26 tests passed; Svelte check had zero errors and warnings.
- Core: 130 unit tests, 2 document-import tests and 7 streaming tests passed.
  The two real-model integration gates remain opt-in and were not run here.
- Full workspace: 140 Rust tests passed, including the Tauri IPC regression.
  `cargo check -p tnn-local-ai` and `cargo fmt --all --check` also passed.
- Browser checks at 360 px, 390 px and 1100 px passed navigation, sidebar
  visibility and horizontal-overflow checks using mocked IPC.
- Device startup was subsequently tested through ADB on the Redmi 13 running
  Android 16. Installation, visible onboarding and native bootstrap IPC passed;
  bootstrap reported ARM64, 8 physical cores and the Qwen 3 0.6B recommendation.
  A force-stop followed by a cold launch completed successfully in 1.246 s.
- Downloads, inference, keyboard/rotation, sharing and inference lifecycle
  behavior remain separate device-test gates.

### Startup icon crash fix

The initial APK aborted with `invalid icon`: a 32×32 16-bit RGBA PNG supplied
2,048 four-byte pixels where Tao expected 1,024. Tauri's pinned PNG decoder
preserves the channel depth, while the runtime icon expects eight-bit RGBA.
The Android configuration now selects the existing eight-bit RGBA launcher
PNG without changing its artwork. The regression test
`android_default_window_icon_is_eight_bit_rgba` validates the configured PNG's
channel depth, color type and interlacing. It passed alongside a rebuilt,
signature-verified APK, and the corrected APK was installed and cold-launched
on the connected phone without the former startup panic.
