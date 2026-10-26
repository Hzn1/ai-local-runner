# Benchmarks

Measurements use the pinned engine and the catalogue's exact model files.
Run `scripts/bench-engine.sh --model <file> --threads <n>` on the target
machine and commit the appended result with the hardware details.

## Reference baseline (from the feasibility spike)

- Machine: Intel i3-3217U, 2C/4T, 1.8 GHz, 9.6 GB RAM, no AVX2, Linux x64
- Engine: PrismML `prism-b10826-c9c34bc`
- Model: `Ternary-Bonsai-2-2B-PQ2_0.gguf`
- Threads: 2
- pp64: 4.75 t/s
- tg32: 2.98 t/s

This is the recorded spike result, not a measurement taken by this
repository's CI. Re-run it on the reference machine before claiming a
performance gate.

## Gates

| Scenario | Target |
|---|---:|
| 2B, 2-core Ivy Bridge, 2 threads | ≥ 2.5 tok/s generation; ≥ 4 tok/s prompt |
| 2B, 4-core without AVX2 | ≥ 6 tok/s generation; ≥ 10 tok/s prompt |
| 4B, 4-core / 8 GB | ≥ 3 tok/s generation |
| 9B, 8-core / 16 GB | ≥ 2.5 tok/s generation |
| First token, 20-token prompt, 2B on i3 | < 8 s |
| App RSS, 2B on 4 GB VM | < 1.2 GB |
| Cold start | < 3 s |
| Model load, 2B on SSD | < 10 s |
