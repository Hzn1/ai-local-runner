#!/usr/bin/env bash
# Measure prompt and generation speed with the pinned engine.
#
# The production engine is pruned and does not ship llama-bench, so when it is
# missing this script downloads a temporary full archive (still SHA-256 pinned)
# and uses that. Results are appended to docs/benchmarks.md.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL=""
# Count unique socket/core pairs, never SMT siblings. If topology is missing,
# use one thread rather than risk exceeding the physical-core count.
PHYSICAL_CORES="$(lscpu -p=SOCKET,CORE 2>/dev/null | awk -F, '!/^#/ && $1 ~ /^[0-9]+$/ && $2 ~ /^[0-9]+$/ { cores[$1 "," $2]=1 } END { print length(cores) }' || true)"
if [[ ! "$PHYSICAL_CORES" =~ ^[1-9][0-9]*$ ]]; then
  PHYSICAL_CORES=1
fi
THREADS=$((PHYSICAL_CORES < 8 ? PHYSICAL_CORES : 8))
RUNS=3
ENGINE="$ROOT/src-tauri/resources/engine"
OUT="$ROOT/docs/benchmarks.md"

usage() {
  cat <<'EOF'
Usage: scripts/bench-engine.sh --model PATH [options]

Options:
  --model PATH      GGUF file to benchmark (required)
  --threads N       Threads (default: physical cores, capped at 8)
  --runs N          Repetitions (default: 3)
  --engine DIR      Engine directory containing llama-bench
  --out FILE        Markdown file to append to (default: docs/benchmarks.md)
  --no-append       Print results only
  -h, --help        Show this help
EOF
}

APPEND=1
while (($#)); do
  case "$1" in
    --model) MODEL="${2:?missing value}"; shift 2 ;;
    --threads) THREADS="${2:?missing value}"; shift 2 ;;
    --runs) RUNS="${2:?missing value}"; shift 2 ;;
    --engine) ENGINE="${2:?missing value}"; shift 2 ;;
    --out) OUT="${2:?missing value}"; shift 2 ;;
    --no-append) APPEND=0; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

[[ "$THREADS" =~ ^[1-9][0-9]*$ && ${#THREADS} -le 3 ]] || {
  echo "--threads must be a positive integer" >&2; exit 2;
}
((THREADS <= PHYSICAL_CORES && THREADS <= 8)) || {
  echo "--threads must not exceed physical cores ($PHYSICAL_CORES) or 8" >&2; exit 2;
}
[[ "$RUNS" =~ ^[1-9][0-9]*$ && ${#RUNS} -le 4 ]] || {
  echo "--runs must be a positive integer of at most four digits" >&2; exit 2;
}

[[ -n "$MODEL" ]] || { echo "--model is required" >&2; usage >&2; exit 2; }
[[ -f "$MODEL" ]] || { echo "model not found: $MODEL" >&2; exit 1; }
command -v node >/dev/null 2>&1 || { echo "node is required to parse llama-bench JSON" >&2; exit 1; }

TMP=""
cleanup() {
  if [[ -n "$TMP" ]]; then
    rm -rf "$TMP"
  fi
}
trap cleanup EXIT

BENCH="$ENGINE/llama-bench"
if [[ ! -x "$BENCH" ]]; then
  echo "llama-bench not found in $ENGINE; fetching a temporary full build…" >&2
  TMP="$(mktemp -d "${TMPDIR:-/tmp}/tnn-bench.XXXXXX")"
  "$ROOT/scripts/fetch-engine.sh" --platform linux --all-tools \
    --dest "$TMP/engine" --baseline "$TMP/baseline" >/dev/null
  BENCH="$TMP/engine/llama-bench"
fi

if [[ -z "$TMP" ]]; then
  TMP="$(mktemp -d "${TMPDIR:-/tmp}/tnn-bench.XXXXXX")"
fi
RAW="$TMP/bench.json"

echo "Benchmarking $(basename "$MODEL") with $THREADS threads, $RUNS runs…"
"$BENCH" -m "$MODEL" -p 64 -n 32 -ngl 0 -t "$THREADS" -r "$RUNS" -o json > "$RAW"

CPU="$( (lscpu 2>/dev/null | awk -F: '/Model name/{gsub(/^ +| +$/, "", $2); print $2; exit}') || true)"
if [[ -z "$CPU" ]]; then
  CPU="$(uname -m)"
fi
RAM_GB=""
if [[ -r /proc/meminfo ]]; then
  RAM_GB="$(awk '/MemTotal/{printf "%.1f", $2/1024/1024}' /proc/meminfo)"
fi
OS="$(uname -srm)"
DATE="$(date -u +'%Y-%m-%d %H:%M UTC')"
VERSION="$(grep -o '"version": *"[^"]*"' "$ROOT/engine.lock.json" 2>/dev/null | head -n1 | sed 's/.*"\([^"]*\)"$/\1/' || true)"
VERSION="${VERSION:-unknown}"

SUMMARY="$(node - "$RAW" "$THREADS" <<'NODE'
const fs = require('fs');
const path = process.argv[2];
const threads = process.argv[3];
const rows = JSON.parse(fs.readFileSync(path, 'utf8'));
const out = [];
for (const row of rows) {
  const pp = Number(row.n_prompt || 0);
  const tg = Number(row.n_gen || 0);
  const ts = Number(row.avg_ts || 0);
  const label = pp > 0 && tg === 0 ? `pp${pp}` : tg > 0 ? `tg${tg}` : 'run';
  out.push(`${label}: ${ts.toFixed(2)} t/s`);
}
const size = rows[0] && rows[0].model_size ? (Number(rows[0].model_size) / 1024 ** 3).toFixed(2) + ' GiB' : 'unknown';
console.log(out.join('\n'));
console.log(`model_size: ${size}`);
console.log(`threads: ${threads}`);
NODE
)"

printf '%s\n' "$SUMMARY"

if ((APPEND == 1)); then
  mkdir -p "$(dirname "$OUT")"
  {
    echo
    echo "## $(basename "$MODEL")"
    echo
    echo "- Date: $DATE"
    echo "- Machine: $CPU${RAM_GB:+, ${RAM_GB} GB RAM}"
    echo "- OS: $OS"
    echo "- Engine: $VERSION"
    echo "- Model: $(basename "$MODEL")"
    echo '```text'
    printf '%s\n' "$SUMMARY"
    echo '```'
  } >> "$OUT"
  echo "Appended to $OUT"
fi
