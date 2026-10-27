#!/usr/bin/env bash
# Signed debug APK with embedded production assets, for local phone testing.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/android-env.sh
[[ -x "${JAVA_HOME:-}/bin/java" ]] || { echo 'Set JAVA_HOME to JDK 21; see docs/ANDROID.md.' >&2; exit 1; }
[[ -d src-tauri/gen/android ]] || { echo 'Initialize Android first; see docs/ANDROID.md.' >&2; exit 1; }
rustup target add aarch64-linux-android
bash scripts/build-engine-android.sh
npm run check
npm run test
node_modules/.bin/tauri android build --ci --debug --apk --target aarch64
tnn_apk=src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
[[ -f "$tnn_apk" ]] || { echo 'Expected ARM64 test APK was not produced.' >&2; exit 1; }
mkdir -p artifacts/android
cp "$tnn_apk" artifacts/android/tnn-local-ai-0.1.0-android-arm64.apk
python3 scripts/verify-android-apk.py artifacts/android/tnn-local-ai-0.1.0-android-arm64.apk
(cd artifacts/android && sha256sum tnn-local-ai-0.1.0-android-arm64.apk > SHA256SUMS && sha256sum -c SHA256SUMS)
