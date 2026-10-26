#!/usr/bin/env bash
# Convert and quantize an experimental Hugging Face model with the pinned
# llama.cpp toolchain, then print a catalogue stub with the resulting SHA-256.
#
# This script does not make a model shippable. The experimental validation gate
# (license, coherence on x64/ssse3, ≥1.5 tok/s, pinned revision) still applies.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE=""
OUTPUT=""
LLAMA_CPP=""
QUANT="TQ2_0"
MODEL_ID=""
LICENSE_OK=0
WORK=""

usage() {
  cat <<'EOF'
Usage: scripts/convert-experimental-model.sh \
  --source DIR --output FILE --llama-cpp DIR --model-id ID [options]

Options:
  --source DIR       Local Hugging Face model directory (config.json, weights)
  --output FILE      Destination GGUF path
  --llama-cpp DIR    llama.cpp checkout matching the pinned engine commit
  --model-id ID      Catalogue id to print (e.g. falcon3-1b-158bit)
  --quant TYPE       Target quant (default: TQ2_0)
  --license-confirmed  Confirm the license allows redistribution/download
  --work DIR         Temporary directory (default: mktemp)
  -h, --help         Show this help

The pinned CPU archive does not ship Python conversion scripts, so a matching
checkout is required for convert_hf_to_gguf.py. llama-quantize is downloaded
from the pinned archive into a temporary directory.
EOF
}

while (($#)); do
  case "$1" in
    --source) SOURCE="${2:?missing value}"; shift 2 ;;
    --output) OUTPUT="${2:?missing value}"; shift 2 ;;
    --llama-cpp) LLAMA_CPP="${2:?missing value}"; shift 2 ;;
    --model-id) MODEL_ID="${2:?missing value}"; shift 2 ;;
    --quant) QUANT="${2:?missing value}"; shift 2 ;;
    --license-confirmed) LICENSE_OK=1; shift ;;
    --work) WORK="${2:?missing value}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

[[ -n "$SOURCE" && -d "$SOURCE" ]] || { echo "--source must be an existing directory" >&2; exit 2; }
[[ -n "$OUTPUT" ]] || { echo "--output is required" >&2; exit 2; }
[[ -n "$LLAMA_CPP" && -f "$LLAMA_CPP/convert_hf_to_gguf.py" ]] || {
  echo "--llama-cpp must point at a checkout containing convert_hf_to_gguf.py" >&2
  exit 2
}
[[ -n "$MODEL_ID" ]] || { echo "--model-id is required" >&2; exit 2; }
((LICENSE_OK == 1)) || {
  echo "Refusing to convert: pass --license-confirmed after checking the model license." >&2
  exit 2
}

command -v python3 >/dev/null || { echo "python3 is required" >&2; exit 1; }
command -v sha256sum >/dev/null || { echo "sha256sum is required" >&2; exit 1; }

if [[ -z "$WORK" ]]; then
  WORK="$(mktemp -d "${TMPDIR:-/tmp}/tnn-convert.XXXXXX")"
  trap 'rm -rf "$WORK"' EXIT
fi
mkdir -p "$WORK"

echo "→ Fetching pinned quantization tools"
"$ROOT/scripts/fetch-engine.sh" --platform linux --all-tools \
  --dest "$WORK/engine" --baseline "$WORK/baseline" >/dev/null

echo "→ Converting to an intermediate GGUF"
python3 "$LLAMA_CPP/convert_hf_to_gguf.py" "$SOURCE" \
  --outtype f16 --outfile "$WORK/model-f16.gguf"

mkdir -p "$(dirname "$OUTPUT")"
echo "→ Quantizing to $QUANT"
"$WORK/engine/llama-quantize" "$WORK/model-f16.gguf" "$OUTPUT" "$QUANT"

HASH="$(sha256sum "$OUTPUT" | awk '{print $1}')"
SIZE="$(stat -c '%s' "$OUTPUT")"
NAME="$(basename "$OUTPUT")"

cat <<EOF

Quantized file: $OUTPUT
SHA-256:        $HASH
Size (bytes):   $SIZE

Catalogue stub (fill in display metadata, RAM and validation after the gate):

{
  "id": "$MODEL_ID",
  "display_name": "REPLACE",
  "tier": "advanced",
  "family": "experimental",
  "params_b": 0.0,
  "quant": "$QUANT",
  "file_name": "$NAME",
  "url": null,
  "sha256": "$HASH",
  "size_bytes": $SIZE,
  "license": "REPLACE",
  "license_url": "REPLACE",
  "model_card_url": "REPLACE",
  "revision": "REPLACE",
  "experimental": true,
  "validation": "pending",
  "unavailable_reason": "experimental_model_not_released"
}
EOF
