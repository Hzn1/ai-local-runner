# Release

Current release scope is Linux x64 only: `.deb` and AppImage. Windows
instructions below are retained for future work. The release and engine CI
matrices currently build Linux only; `scripts/release.sh` requires Linux.

## Prerequisites

- Pinned engine present in `src-tauri/resources/engine` and
  `src-tauri/resources/engine-baseline` (`scripts/fetch-engine.sh`/`.ps1`).
- A real updater signing keypair. The public key is in
  `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`); the private key must
  never enter the repository or CI logs.
- A real update endpoint. The checked-in endpoint
  (`updates.tnn.example.invalid`) is a deliberate placeholder and must be
  replaced with the signed-manifest URL before shipping.
- Platform signing identities for Windows (Authenticode) and, if applicable,
  Linux package signing.

### Updater key

Generate once and store the private key in a password manager/CI secret:

```bash
npx tauri signer generate -w ~/.tauri/tnn-local-ai.key
# Public key:
cat ~/.tauri/tnn-local-ai.key.pub
```

Paste the public key into `plugins.updater.pubkey`. For CI, set:

```text
TAURI_SIGNING_PRIVATE_KEY           path to key or literal key
TAURI_SIGNING_PRIVATE_KEY_PASSWORD  key password
```

> The development keypair used while scaffolding was generated with an empty
> password. Regenerate a password-protected key before the first public
> release and update the embedded public key. Losing the private key means
> existing installs cannot receive updates.

## Build

```bash
TAURI_SIGNING_PRIVATE_KEY=~/.tauri/tnn-local-ai.key \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD='…' \
./scripts/release.sh --base-url https://github.com/<owner>/<repo>/releases/download/v0.1.0
```

The script runs `npm ci`, Svelte checks, Vitest, `cargo fmt --check`,
`cargo test --workspace`, then `tauri build`. It collects installers and
updater signatures into `release-artifacts/`, writes `SHA256SUMS`, and, with
`--base-url`, writes `latest-<platform>.json`.

Targets from `tauri.conf.json`:

- Linux: AppImage and `.deb` (primary).
  Document `libwebkit2gtk-4.1-0` and `libgtk-3-0` dependencies.

`createUpdaterArtifacts` is enabled, so Tauri emits `.sig` files next to the
updater-capable bundles. Keep them with the artifacts.

## Update manifest

The updater expects a signed JSON document served at the configured endpoint:

```json
{
  "version": "0.1.1",
  "notes": "What changed",
  "pub_date": "2026-10-25T12:00:00Z",
  "platforms": {
    "linux-x86_64": {
      "signature": "<contents of .AppImage.sig>",
      "url": "https://…/TNN-Local-AI_0.1.1_amd64.AppImage"
    },
    "windows-x86_64": {
      "signature": "<contents of -setup.exe.sig>",
      "url": "https://…/TNN-Local-AI_0.1.1_x64-setup.exe"
    }
  }
}
```

The release workflow merges `latest-linux-x86_64.json` into `latest.json` and
publishes it with the signed artifacts. `.deb` users update through the
package manager or install a newer download; the Linux in-app updater targets
AppImage. Windows manifests are deferred.

Rules:

- Never update the engine or a model silently during a chat.
- Keep the previous app version recoverable if an update fails.
- Updates are opt-outable in Advanced settings; the startup check is
  non-intrusive (banner, not a modal).
- The model catalogue is embedded; do not fetch it remotely.

## Checksums

`SHA256SUMS-<platform>` covers every artifact. Publish it next to the release
and include the hashes in release notes. Verify before uploading:

```bash
cd release-artifacts
sha256sum -c SHA256SUMS-linux
```

## Release checklist

- [ ] Engine SHA-256 matches `engine.lock.json`; baseline directory present.
- [ ] `scripts/verify-no-avx2.sh` passes.
- [ ] `docs/benchmarks.md` updated on the reference machine.
- [ ] No secrets in the diff (`git grep -iE 'api[_-]?key|private key|password'`).
- [ ] Clean install on Linux 4 GB/2 cores (`.deb` and AppImage).
- [ ] 2B download, pause, resume, checksum, chat, stop, restart.
- [ ] 4B on 8 GB; 9B on 16 GB.
- [ ] No network during chat (firewall or packet capture).
- [ ] No orphan `llama-server` after quit or crash.
- [ ] Engine self-test fallback works.
- [ ] Update check and signed install work on Linux AppImage.
- [ ] Uninstall/cleanup documented (app data, models, logs).
- [ ] Licences and `THIRD-PARTY-NOTICES.md` present in the installer/About.
- [ ] Diagnostics export redacts prompts.
- [ ] Accessibility smoke test and keyboard-only flow.
- [ ] English and pt-BR strings complete.
- [ ] Checksums and signatures published.
- [ ] Update endpoint and public key are real, not placeholders.

## Uninstall and cleanup

Uninstalling the app leaves user data by default. Document the data directory:

- Windows: `%APPDATA%/ai.tnn.localai` (or the identifier's app-data path).
- Linux: `~/.local/share/ai.tnn.localai` (or the identifier's app-data path).

Removing it deletes conversations, models, downloads, logs and settings.

## Versioning

Semantic versioning for the app; the catalogue carries its own
`catalog_version`. Bump the app version in `tauri.conf.json`,
`package.json` and the workspace together. Engine changes are always a pin
change with a compatibility re-test, never an independent auto-update.
