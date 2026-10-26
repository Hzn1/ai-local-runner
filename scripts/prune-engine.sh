#!/usr/bin/env bash
# Prune an already-extracted engine tree down to what llama-server needs.
#
# Used by fetch-engine.sh and available on its own when inspecting a new
# upstream archive before updating the pin.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/src-tauri/resources/engine"
BASELINE="$ROOT/src-tauri/resources/engine-baseline"

usage() {
  cat <<'EOF'
Usage: scripts/prune-engine.sh [--engine DIR] [--baseline DIR]

Removes tools and libraries that llama-server does not load, then rebuilds the
x64-only baseline copy used for the SIGILL fallback. Safe to re-run.
EOF
}

while (($#)); do
  case "$1" in
    --engine) DEST="${2:?missing value}"; shift 2 ;;
    --baseline) BASELINE="${2:?missing value}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ ! -d "$DEST" ]]; then
  echo "engine directory not found: $DEST" >&2
  exit 1
fi

if [[ -f "$DEST/llama-server.exe" ]]; then
  # Windows layout.
  find "$DEST" -maxdepth 1 -type f \
    ! -name 'llama-server.exe' \
    ! -name 'llama-server-impl.dll' \
    ! -name 'llama-common.dll' \
    ! -name 'llama.dll' \
    ! -name 'ggml.dll' \
    ! -name 'ggml-base.dll' \
    ! -name 'mtmd.dll' \
    ! -name 'ggml-cpu-*.dll' \
    ! -name 'libomp140.x86_64.dll' \
    ! -name 'LICENSE' \
    -delete

  rm -rf "$BASELINE"
  mkdir -p "$BASELINE"
  cp -a "$DEST"/. "$BASELINE"/
  find "$BASELINE" -maxdepth 1 -type f -name 'ggml-cpu-*.dll' \
    ! -name 'ggml-cpu-x64.dll' -delete

  echo "Pruned $DEST and rebuilt $BASELINE (x64 baseline)."
  exit 0
fi

if [[ ! -f "$DEST/llama-server" ]]; then
  echo "no llama-server executable in $DEST" >&2
  exit 1
fi

# Linux layout: keep the server, its direct libraries and all CPU variants.
find "$DEST" -maxdepth 1 \( -type f -o -type l \) \( \
  ! -name 'llama-server' \
  ! -name 'LICENSE' \
  ! -name 'libllama-server-impl.so' \
  ! -name 'libllama-common.so*' \
  ! -name 'libllama.so*' \
  ! -name 'libggml.so*' \
  ! -name 'libggml-base.so*' \
  ! -name 'libmtmd.so*' \
  ! -name 'libggml-cpu-*.so' \
  \) -delete

rm -rf "$BASELINE"
mkdir -p "$BASELINE"
cp -a "$DEST"/. "$BASELINE"/
find "$BASELINE" -maxdepth 1 -name 'libggml-cpu-*.so' \
  ! -name 'libggml-cpu-x64.so' -delete

if command -v ldd >/dev/null 2>&1; then
  if ldd "$DEST/llama-server" 2>&1 | grep -q 'not found'; then
    echo "warning: llama-server has unresolved shared libraries" >&2
    ldd "$DEST/llama-server" | grep 'not found' >&2 || true
  fi
fi

echo "Pruned $DEST and rebuilt $BASELINE (x64 baseline)."
