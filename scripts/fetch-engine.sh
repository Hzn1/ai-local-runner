#!/usr/bin/env bash
# Fetch and prune the pinned PrismML llama.cpp CPU build.
#
# Values are duplicated in engine.lock.json (the machine-readable record used
# by CI and docs). Update both together when changing the engine pin.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="prism-b10826-c9c34bc"
REPO="PrismML-Eng/llama.cpp"
LINUX_FILE="llama-prism-b10826-c9c34bc-bin-ubuntu-x64.tar.gz"
LINUX_SHA="66a250f38c3f8e2064e2f913b3a26ff5bae10955b67e8ff38c72ab48cbecf28c"
WINDOWS_FILE="llama-prism-b10826-c9c34bc-bin-win-cpu-x64.zip"
WINDOWS_SHA="1e88ab9e858b5c2dbc30cf847947a9f923b53511ba0baf0b265353ebe1f4daa6"

PLATFORM="auto"
DEST=""
BASELINE=""
ALL_TOOLS=0
FORCE=0

usage() {
  cat <<'EOF'
Usage: scripts/fetch-engine.sh [options]

Options:
  --platform auto|linux|windows  Target build (default: auto for this host)
  --dest DIR                     Install directory (default: src-tauri/resources/engine)
  --baseline DIR                 Baseline directory (default: src-tauri/resources/engine-baseline)
  --all-tools                    Keep every tool in the archive (for benchmarking)
  --force                        Replace an existing install
  -h, --help                     Show this help

The private/session API key is generated at runtime; nothing secret is stored
here. Archives are verified against the SHA-256 pinned in engine.lock.json.
EOF
}

while (($#)); do
  case "$1" in
    --platform) PLATFORM="${2:?missing value}"; shift 2 ;;
    --dest) DEST="${2:?missing value}"; shift 2 ;;
    --baseline) BASELINE="${2:?missing value}"; shift 2 ;;
    --all-tools) ALL_TOOLS=1; shift ;;
    --force) FORCE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

case "$PLATFORM" in
  auto)
    case "$(uname -s)" in
      Linux) PLATFORM=linux ;;
      MINGW*|MSYS*|CYGWIN*|Windows_NT) PLATFORM=windows ;;
      *) echo "unsupported host $(uname -s); pass --platform linux or --platform windows" >&2; exit 2 ;;
    esac
    ;;
  linux|windows) ;;
  *) echo "--platform must be auto, linux or windows" >&2; exit 2 ;;
esac

if [[ -z "$DEST" ]]; then
  DEST="$ROOT/src-tauri/resources/engine"
fi
if [[ -z "$BASELINE" ]]; then
  BASELINE="$(dirname "$DEST")/engine-baseline"
fi

case "$PLATFORM" in
  linux)
    FILE="$LINUX_FILE"
    SHA="$LINUX_SHA"
    ;;
  windows)
    FILE="$WINDOWS_FILE"
    SHA="$WINDOWS_SHA"
    ;;
esac
URL="https://github.com/$REPO/releases/download/$VERSION/$FILE"

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "required command not found: $1" >&2
    exit 1
  }
}

check_sha() {
  local file="$1" expected="$2" actual=""
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$file" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  else
    echo "sha256sum or shasum is required" >&2
    exit 1
  fi
  if [[ "$actual" != "$expected" ]]; then
    echo "checksum mismatch for $file" >&2
    echo "  expected: $expected" >&2
    echo "  actual:   $actual" >&2
    exit 1
  fi
}

require curl
if [[ "$PLATFORM" == "linux" ]]; then
  require tar
else
  require unzip
fi

if [[ -x "$DEST/llama-server" || -x "$DEST/llama-server.exe" ]] && ((FORCE == 0)); then
  echo "engine already present in $DEST (use --force to replace)"
  exit 0
fi

TMP="$(mktemp -d "${TMPDIR:-/tmp}/tnn-engine.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading $VERSION ($PLATFORM)…"
curl --fail --location --retry 3 --retry-delay 2 \
  --output "$TMP/$FILE" "$URL"
check_sha "$TMP/$FILE" "$SHA"

mkdir -p "$TMP/extract"
if [[ "$PLATFORM" == "linux" ]]; then
  tar -xzf "$TMP/$FILE" -C "$TMP/extract"
  SRC="$TMP/extract/$VERSION"
  [[ -d "$SRC" ]] || SRC="$(find "$TMP/extract" -maxdepth 2 -type f -name llama-server -exec dirname {} \; | head -n 1)"
else
  unzip -q "$TMP/$FILE" -d "$TMP/extract"
  SRC="$TMP/extract"
fi

[[ -n "${SRC:-}" && -d "$SRC" ]] || {
  echo "could not find the extracted engine payload" >&2
  exit 1
}

rm -rf "$DEST"
mkdir -p "$DEST"

if ((ALL_TOOLS == 1)); then
  cp -a "$SRC"/. "$DEST"/
else
  if [[ "$PLATFORM" == "linux" ]]; then
    find "$SRC" -maxdepth 1 \( \
      -name 'llama-server' -o \
      -name 'LICENSE' -o \
      -name 'libllama-server-impl.so' -o \
      -name 'libllama-common.so*' -o \
      -name 'libllama.so*' -o \
      -name 'libggml.so*' -o \
      -name 'libggml-base.so*' -o \
      -name 'libmtmd.so*' -o \
      -name 'libggml-cpu-*.so' \
      \) -exec cp -a -t "$DEST" {} +
  else
    for name in \
      llama-server.exe \
      llama-server-impl.dll \
      llama-common.dll \
      llama.dll \
      ggml.dll \
      ggml-base.dll \
      mtmd.dll \
      libomp140.x86_64.dll \
      LICENSE; do
      if [[ -f "$SRC/$name" ]]; then
        cp -a "$SRC/$name" "$DEST/"
      fi
    done
    cp -a "$SRC"/ggml-cpu-*.dll "$DEST"/
  fi
fi

# The Windows archive does not ship the MIT text; fetch it from the same pin.
if [[ ! -f "$DEST/LICENSE" ]]; then
  curl --fail --location --retry 3 \
    --output "$DEST/LICENSE" \
    "https://raw.githubusercontent.com/$REPO/$VERSION/LICENSE"
fi

if [[ "$PLATFORM" == "linux" ]]; then
  BASELINE_VARIANT="$DEST/libggml-cpu-x64.so"
  EXE="$DEST/llama-server"
else
  BASELINE_VARIANT="$DEST/ggml-cpu-x64.dll"
  EXE="$DEST/llama-server.exe"
fi

[[ -e "$BASELINE_VARIANT" ]] || {
  echo "the archive no longer contains the x64 baseline CPU variant" >&2
  exit 1
}
[[ -e "$EXE" ]] || {
  echo "the archive no longer contains llama-server" >&2
  exit 1
}

chmod +x "$EXE" 2>/dev/null || true

# A second copy with only the SSE2 baseline CPU backend, used when a newer
# variant crashes with SIGILL on pre-AVX2 machines.
rm -rf "$BASELINE"
mkdir -p "$BASELINE"
cp -a "$DEST"/. "$BASELINE"/
if [[ "$PLATFORM" == "linux" ]]; then
  find "$BASELINE" -maxdepth 1 -type f -name 'libggml-cpu-*.so' \
    ! -name 'libggml-cpu-x64.so' -delete
  find "$BASELINE" -maxdepth 1 -type l -name 'libggml-cpu-*.so' \
    ! -name 'libggml-cpu-x64.so' -delete
  if command -v ldd >/dev/null 2>&1; then
    if ldd "$EXE" 2>&1 | grep -q 'not found'; then
      echo "the engine has unresolved shared libraries:" >&2
      ldd "$EXE" | grep 'not found' >&2
      exit 1
    fi
  fi
else
  find "$BASELINE" -maxdepth 1 -type f -name 'ggml-cpu-*.dll' \
    ! -name 'ggml-cpu-x64.dll' -delete
fi

SIZE="$(du -sh "$DEST" | awk '{print $1}')"
BASE_SIZE="$(du -sh "$BASELINE" | awk '{print $1}')"
echo "Engine installed: $DEST ($SIZE)"
echo "Baseline copy:    $BASELINE ($BASE_SIZE)"
echo "Version:          $VERSION"
echo "Tip: run scripts/verify-no-avx2.sh before packaging."
