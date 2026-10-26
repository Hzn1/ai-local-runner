# Implementation status

Active scope: Linux x64, following the product-owner instruction on
2026-10-26. Windows delivery and validation are deferred. The original plan's
human review gates remain in effect; existing code across later phases does
not establish that those gates have passed.

## Phase 0 — Linux engine validation

- The pinned Linux archive was fetched and verified with `fetch-engine.sh`.
- The resource executable reports build 10826, commit `c9c34bc9c`.
- The 2B model was downloaded separately to temporary storage and its SHA-256
  matches the embedded catalogue. No weights are bundled or committed.
- The host is the reference Intel i3-3217U, 2 physical cores / 4 logical CPUs,
  with AVX and F16C and without AVX2; it has approximately 9.6 GiB RAM.
- `tests/real_engine.rs` provides an opt-in production-manager inference gate
  for native dispatch, isolated SSE2, and isolated SSSE3. It validates model
  integrity, non-thinking streaming and clean shutdown. Run instructions are
  in [ENGINE.md](ENGINE.md).
- The real-model gate passed on this Linux host for native dispatch, forced
  x64/SSE2, and forced SSSE3 (72.57 seconds total). Every backend passed the
  startup self-test, non-thinking streaming, and clean shutdown/PID cleanup.
  This is automated garbage detection, not a human coherence assessment or a
  throughput benchmark.
- QEMU user emulation is unavailable on this host. The SSE2-only CPU gate and
  clean 4 GB Linux VM gate remain pending. A forced SSE2 backend on Ivy Bridge
  does not replace those tests.
- Launching the packaged app and loading its resources on a clean supported
  Linux distribution remains a manual check.

Phase 0 remains open for review; do not claim a public release or silently
advance through the remaining gates.

## Existing implementation

The workspace already contains core/storage, engine supervision, downloads,
the Svelte interface, diagnostics and packaging scripts. Their presence is
separate from acceptance. Required remaining automated and manual coverage is
tracked in [TESTING.md](TESTING.md); release prerequisites and checks are in
[RELEASE.md](RELEASE.md).

Linux build configuration targets `.deb` and AppImage. Engine/release CI runs
on Linux, and Rust CI installs the GTK/WebKit dependencies needed by the
workspace's desktop crate. The release collector resolves Cargo's actual
workspace target directory rather than assuming `src-tauri/target`.

The effective thread limit now respects physical cores for every hardware
tier, including Advanced overrides. The benchmark script uses physical cores
and rejects oversubscription. Regression tests exercise the non-minimum
machine override as well as the existing minimum-mode clamps.

## Automated checks (2026-10-26)

- `npm run check`: zero errors and warnings.
- `npm run test`: 19 tests passed.
- `npm run build`: passed.
- `cargo fmt --all --check`: passed on stable Rust; the nightly-only formatting
  option was removed.
- `cargo test --workspace`: 114 core tests and 4 streaming integration tests
  passed; the real-model test is opt-in and skipped by this command.
- `cargo check -p tnn-local-ai`: passed.
- `TNN_TEST_MODEL=… cargo test -p tnn-core --test real_engine -- --ignored --nocapture`:
  passed across all three backends, with loopback permission.
- Modified shell scripts pass `bash -n`; workflow YAML parses successfully.
- A fake benchmark executable confirmed that the script selects 2 physical
  cores on this host and rejects zero/invalid values and a 4-thread override.
  These fixture numbers are not performance measurements.

Socket tests require loopback access. Their initial sandboxed run was blocked
by `Operation not permitted`; the full suite passed when rerun with that
permission. No clean-machine, packaged-app, or public-release claim is made
from these checks.

## Linux desktop startup fix (2026-10-26)

The entry point now initializes translations before mounting the UI. Previously
the first loading screen called the translator before a locale existed,
throwing an exception and leaving a blank window. A regression test reproduces
the old failure and passes with the fix. All 20 frontend tests, Svelte checking,
the production build, and the locked Linux Rust check pass.

The JavaScript Tauri packages and corresponding Rust dependencies are limited
to matching minor release lines. The npm lockfile has been updated to match
the existing Cargo lockfile, removing the reported version mismatches.

A fresh Linux desktop launch under Xvfb rendered the onboarding screen, with
real Tauri IPC and isolated temporary app data. A separate WebKitGTK preview
check rendered the desktop-only message without JavaScript startup errors.
This smoke check does not replace packaged-app or 4 GB VM acceptance tests.
