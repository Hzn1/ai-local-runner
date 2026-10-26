#!/usr/bin/env bash
# Prove the engine starts on a CPU without AVX2 by running it under QEMU's
# qemu64 model (SSE2 only). This is the compatibility gate from the plan.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENGINE="$ROOT/src-tauri/resources/engine"

usage() {
  cat <<'EOF'
Usage: scripts/verify-no-avx2.sh [--engine DIR]

Runs llama-server --version under qemu-x86_64 -cpu qemu64. A successful run
means the SSE2 baseline path loads without SIGILL. This is a user-mode check
of process startup, not a substitute for a full 4 GB VM chat test.
EOF
}

while (($#)); do
  case "$1" in
    --engine) ENGINE="${2:?missing value}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

EXE="$ENGINE/llama-server"
[[ -x "$EXE" ]] || { echo "engine not found: $EXE (run scripts/fetch-engine.sh)" >&2; exit 1; }
[[ -f "$ENGINE/libggml-cpu-x64.so" ]] || {
  echo "the x64 baseline CPU variant is missing from $ENGINE" >&2
  exit 1
}

if ! command -v qemu-x86_64 >/dev/null 2>&1; then
  cat >&2 <<'EOF'
qemu-x86_64 is not installed. On Debian/Ubuntu:
  sudo apt-get install qemu-user

If QEMU cannot be installed, run this manual gate instead:
  - Boot a 4 GB Ubuntu VM with -cpu SandyBridge (or a real Ivy Bridge host).
  - Install, download the 2B model, and complete a short chat.
Record the result in docs/TESTING.md.
EOF
  exit 1
fi

QEMU_ARGS=(-cpu qemu64)
for dir in /usr/x86_64-linux-gnu /usr/lib/x86_64-linux-gnu; do
  if [[ -d "$dir" ]]; then
    QEMU_ARGS+=(-L "$dir")
    break
  fi
done

echo "Running $EXE under qemu64 (no AVX/AVX2)…"
set +e
OUTPUT="$(cd "$ENGINE" && timeout 120 qemu-x86_64 "${QEMU_ARGS[@]}" ./llama-server --version 2>&1)"
STATUS=$?
set -e

printf '%s\n' "$OUTPUT" | tail -n 20

if ((STATUS == 124)); then
  echo "timed out waiting for llama-server --version" >&2
  exit 1
fi
if ((STATUS != 0)); then
  echo "engine failed on the SSE2-only CPU model (exit $STATUS)" >&2
  if printf '%s' "$OUTPUT" | grep -qiE 'illegal instruction|invalid instruction|undefined instruction'; then
    echo "SIGILL detected: the x64 fallback cannot run on this CPU model." >&2
  fi
  exit 1
fi

echo "OK: llama-server started on an SSE2-only CPU model."
