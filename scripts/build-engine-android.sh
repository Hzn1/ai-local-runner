#!/usr/bin/env bash
# Build the existing pinned engine for baseline Android ARM64, not the host CPU.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/android-env.sh
[[ -f "$NDK_HOME/build/cmake/android.toolchain.cmake" ]] || { echo 'Install the pinned Android NDK; see docs/ANDROID.md.' >&2; exit 1; }
command -v cmake >/dev/null || { echo 'Install Android SDK CMake 3.22.1.' >&2; exit 1; }
readarray -t tnn_pin < <(python3 - <<'PY'
import json, re
from urllib.parse import urlparse
lock=json.load(open('engine.lock.json'))
p=lock['android']
url=urlparse(p['source_url'])
assert url.scheme == 'https' and url.hostname == 'codeload.github.com', 'engine source URL is not allowlisted HTTPS'
for key in ('commit','source_url','source_sha256','api_level','cpu_arch'): print(p[key])
print(re.fullmatch(r'prism-b(\d+)-[0-9a-f]+', lock['release']).group(1))
PY
)
tnn_work="$PWD/target/android-engine"
tnn_source="$tnn_work/llama.cpp-${tnn_pin[0]}"
mkdir -p "$tnn_work"
tnn_extract=''
tnn_cleanup() {
  rm -f -- "$tnn_work/source.tar.gz.part"
  if [[ -n "$tnn_extract" ]]; then rm -rf -- "$tnn_extract"; fi
}
trap tnn_cleanup EXIT
if [[ ! -f "$tnn_work/source.tar.gz" ]]; then
  curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 --retry 3 "${tnn_pin[1]}" -o "$tnn_work/source.tar.gz.part"
  mv "$tnn_work/source.tar.gz.part" "$tnn_work/source.tar.gz"
fi
echo "${tnn_pin[2]}  $tnn_work/source.tar.gz" | sha256sum --check --status
if [[ ! -d "$tnn_source" ]]; then
  tnn_extract=$(mktemp -d "$tnn_work/source.XXXXXX")
  tar -xzf "$tnn_work/source.tar.gz" -C "$tnn_extract"
  mv "$tnn_extract/llama.cpp-${tnn_pin[0]}" "$tnn_source"
fi
cmake -S "$tnn_source" -B "$tnn_work/build" -G Ninja \
  -DCMAKE_TOOLCHAIN_FILE="$NDK_HOME/build/cmake/android.toolchain.cmake" \
  -DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM="android-${tnn_pin[3]}" \
  -DANDROID_STL=c++_static -DCMAKE_BUILD_TYPE=Release \
  -DLLAMA_BUILD_NUMBER="${tnn_pin[5]}" -DLLAMA_BUILD_COMMIT="${tnn_pin[0]}" \
  -DCMAKE_EXE_LINKER_FLAGS='-Wl,-z,max-page-size=16384' \
  -DBUILD_SHARED_LIBS=OFF -DGGML_BACKEND_DL=OFF -DGGML_NATIVE=OFF \
  -DGGML_CPU_ARM_ARCH="${tnn_pin[4]}" -DGGML_OPENMP=OFF \
  -DGGML_CPU_KLEIDIAI=OFF -DGGML_LLAMAFILE=OFF \
  -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF \
  -DLLAMA_BUILD_APP=OFF -DLLAMA_BUILD_UI=OFF -DLLAMA_USE_PREBUILT_UI=OFF -DLLAMA_OPENSSL=OFF \
  -DLLAMA_SUBPROCESS=OFF -DLLAMA_BUILD_SERVER=ON
# Remove only generated UI cache from earlier builds; upstream can otherwise
# embed stale downloaded assets even when both UI build options are disabled.
rm -rf -- "$tnn_work/build/tools/ui/dist"
cmake --build "$tnn_work/build" --target llama-server --parallel "$CARGO_BUILD_JOBS"
tnn_destination="$PWD/src-tauri/android/engine/arm64-v8a"
mkdir -p "$tnn_destination"
cp "$tnn_work/build/bin/llama-server" "$tnn_destination/libtnn_engine.so"
"$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip" "$tnn_destination/libtnn_engine.so"
mkdir -p src-tauri/android/licenses
cp "$tnn_source/LICENSE" src-tauri/android/licenses/llama.cpp-LICENSE.txt
sha256sum "$tnn_destination/libtnn_engine.so"
