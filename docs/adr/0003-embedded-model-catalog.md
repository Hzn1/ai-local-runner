# 0003 — Embed the model catalogue

- Status: accepted
- Date: 2026-10-25

## Context

A remote catalogue would let the project add models without an app update, but
it also lets a compromised server add unpinned or unlicensed weights.

## Decision

Compile `catalog.json` into `tnn-core`. Every downloadable entry carries a
pinned Hugging Face revision, exact SHA-256 and licence URL. There is no remote
manifest in v1.

## Consequences

- Adding or fixing a model requires an app release.
- Integrity is anchored in the signed app binary.
- Experimental entries can ship hidden (`validation: pending`, `url: null`) and
  be enabled only after a later release passes the gate.
- The catalogue must be validated at build/test time; a malformed entry fails
  the local checks rather than silently disappearing.
