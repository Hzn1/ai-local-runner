# 0002 — CPU-only, no AVX2 requirement

- Status: accepted
- Date: 2026-10-25

## Context

The reference machine is an Ivy Bridge-era laptop (4 GB RAM, 2 cores, no AVX2).
The product promise is “runs on ordinary PCs”, not “runs on a gaming desktop”.

## Decision

Require only SSE2/SSSE3. Use the pinned engine's runtime CPU dispatch and ship an
`x64` baseline fallback directory. Never select a GPU backend in v1.

## Consequences

- Speed on old hardware is modest (2B ≈ 3 tok/s generation on the reference).
- The 4 GB minimum mode must cap context (2K), threads (2), and answers (256).
- Every engine pin must re-run `scripts/verify-no-avx2.sh` and a full VM chat.
- Newer CPU variants remain available and are selected at runtime; the baseline
  is only a fallback, not the default on capable machines.
- Vision, speculative decoding and large models stay outside v1 defaults.
