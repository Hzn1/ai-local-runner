#!/usr/bin/env bash
# Build, test and package a signed Linux release.
#
# The updater key must never be committed. Provide it through the standard
# Tauri variables:
#   TAURI_SIGNING_PRIVATE_KEY=/path/to/key (or the literal key)
#   TAURI_SIGNING_PRIVATE_KEY_PASSWORD=…
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/release-artifacts"
BASE_URL=""
SKIP_TESTS=0

usage() {
  cat <<'EOF'
Usage: scripts/release.sh [options]

Options:
  --base-url URL   Release asset base URL used to write latest-<platform>.json
  --out DIR        Output directory (default: release-artifacts)
  --skip-tests     Skip the test suite (not recommended)
  -h, --help       Show this help

Environment:
  TAURI_SIGNING_PRIVATE_KEY           required; path or literal key
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD  optional
EOF
}

while (($#)); do
  case "$1" in
    --base-url) BASE_URL="${2:?missing value}"; shift 2 ;;
    --out) OUT="${2:?missing value}"; shift 2 ;;
    --skip-tests) SKIP_TESTS=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

cd "$ROOT"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "Only Linux releases are currently supported." >&2
  exit 1
fi

if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  echo "TAURI_SIGNING_PRIVATE_KEY is not set; refusing to build unsigned updater artifacts." >&2
  exit 1
fi

if [[ ! -x src-tauri/resources/engine/llama-server && ! -x src-tauri/resources/engine/llama-server.exe ]]; then
  echo "engine binaries are missing. Run scripts/fetch-engine.sh (or .ps1) first." >&2
  exit 1
fi
if [[ ! -x src-tauri/resources/engine-baseline/llama-server && ! -x src-tauri/resources/engine-baseline/llama-server.exe ]]; then
  echo "the x64 baseline engine is missing. Re-run the fetch script." >&2
  exit 1
fi

command -v npm >/dev/null || { echo "npm is required" >&2; exit 1; }
command -v cargo >/dev/null || { echo "cargo is required" >&2; exit 1; }
command -v node >/dev/null || { echo "node is required" >&2; exit 1; }

echo "→ Installing locked frontend dependencies"
npm ci

if ((SKIP_TESTS == 0)); then
  echo "→ Type-checking Svelte"
  npm run check
  echo "→ Frontend tests"
  npm run test
  echo "→ Rust formatting"
  cargo fmt --all --check
  echo "→ Rust tests"
  cargo test --workspace
else
  echo "→ Skipping tests (--skip-tests)"
fi

echo "→ Building Tauri bundles and updater signatures"
npm run tauri build

rm -rf "$OUT"
mkdir -p "$OUT"

# Collect installer/update artifacts and their signatures.
BUNDLE_DIR="$(cargo metadata --offline --no-deps --format-version 1 | node -e 'let s=""; process.stdin.on("data", c => s += c); process.stdin.on("end", () => process.stdout.write(JSON.parse(s).target_directory));')/release/bundle"
find "$BUNDLE_DIR" -type f \
  \( -name '*.AppImage' -o -name '*.AppImage.sig' \
  -o -name '*.deb' -o -name '*.rpm' \
  -o -name '*-setup.exe' -o -name '*-setup.exe.sig' \
  -o -name '*.msi' -o -name '*.msi.sig' \) \
  -exec cp -t "$OUT" {} +

if [[ -z "$(ls -A "$OUT")" ]]; then
  echo "no bundle artifacts found under $BUNDLE_DIR" >&2
  exit 1
fi

echo "→ Writing checksums"
PLATFORM="$(uname -s | tr '[:upper:]' '[:lower:]')"
(
  cd "$OUT"
  sha256sum -- * > "SHA256SUMS-$PLATFORM"
)

if [[ "$PLATFORM" == "linux" ]]; then
  MANIFEST_PLATFORM="linux-x86_64"
  ARTIFACT_PATTERN='*.AppImage'
else
  MANIFEST_PLATFORM="windows-x86_64"
  ARTIFACT_PATTERN='*-setup.exe'
fi

if [[ -n "$BASE_URL" ]]; then
  echo "→ Writing latest-$MANIFEST_PLATFORM.json"
  node - "$ROOT" "$OUT" "$BASE_URL" "$MANIFEST_PLATFORM" "$ARTIFACT_PATTERN" <<'NODE'
const fs = require('fs');
const path = require('path');
const [root, out, baseUrl, platform, pattern] = process.argv.slice(2);
const config = JSON.parse(fs.readFileSync(path.join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
const files = fs.readdirSync(out);
const ext = pattern.replace('*', '');
const artifact = files.find((name) => name.endsWith(ext) && !name.endsWith('.sig'));
if (!artifact) {
  console.error(`no artifact matching ${pattern} in ${out}`);
  process.exit(1);
}
const signatureFile = path.join(out, `${artifact}.sig`);
if (!fs.existsSync(signatureFile)) {
  console.error(`missing updater signature: ${signatureFile}`);
  process.exit(1);
}
const manifest = {
  version: config.version,
  notes: process.env.RELEASE_NOTES || '',
  pub_date: new Date().toISOString(),
  platforms: {
    [platform]: {
      signature: fs.readFileSync(signatureFile, 'utf8').trim(),
      url: `${baseUrl.replace(/\/$/, '')}/${encodeURIComponent(artifact)}`,
    },
  },
};
fs.writeFileSync(
  path.join(out, `latest-${platform}.json`),
  JSON.stringify(manifest, null, 2) + '\n',
);
console.log(`wrote latest-${platform}.json for ${artifact}`);
NODE
fi

echo
echo "Release artifacts in $OUT:"
ls -lh "$OUT"
