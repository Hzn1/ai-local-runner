# 0006 — Pin one engine build

- Status: accepted
- Date: 2026-10-25

## Context

llama.cpp changes quickly. CPU dispatch, GGUF support, flags and performance
can change between builds. Auto-updating the engine independently of the app
would make regressions and compatibility failures hard to reproduce.

## Decision

Pin `PrismML-Eng/llama.cpp` release `prism-b10826-c9c34bc` and both archive
SHA-256 values in `engine.lock.json`. Fetch scripts verify the hash. Changing
the pin requires a compatibility spike, a benchmark and a clean-machine test.

## Consequences

- Reproducible installs; a compromised release asset cannot be substituted
  silently.
- Engine fixes require an app release.
- The pruned payload and `x64` baseline are part of the release artifact and
  must be re-generated for each platform.
- The plan's “do not auto-update the engine” rule is enforced by process, not
  just intent.
