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

## Catalogue compatibility measurements — 2026-10-26

Host: Intel i3-3217U, 2 physical cores / 4 logical CPUs, approximately 9.6 GiB
RAM, Linux x64, no AVX2. Engine: `prism-b10826-c9c34bc`. Threads: 2; context:
2048; thinking disabled. Files and revisions are pinned in the catalogue.

These are tiny `EngineManager::benchmark` samples (at most 16 generated tokens),
following the greeting self-test and a streamed greeting. Each variant uses
an isolated engine directory and cannot fall back to another variant. This
was a shared development machine with build/download/validation activity;
numbers vary and are not comparable to the controlled pp64/tg32 baseline or
the release performance gates. Generated greetings were reviewed for obvious
coherence; this is not a broad quality evaluation.

| Model / pack | Backend | Prompt t/s | Generation t/s |
|---|---|---:|---:|
| Ternary Bonsai 1.7B PQ2_0 | Native | 3.97 | 2.94 |
| Ternary Bonsai 1.7B PQ2_0 | x64/SSE2 | 2.46 | 1.79 |
| Ternary Bonsai 1.7B PQ2_0 | SSSE3 | 3.38 | 2.62 |
| Ternary Bonsai 4B PQ2_0 | Native | 0.87 | 1.23 |
| Ternary Bonsai 4B PQ2_0 | x64/SSE2 | 0.24 | 0.72 |
| Ternary Bonsai 4B PQ2_0 | SSSE3 | 1.32 | 1.00 |
| Bonsai 2 2B PTQ1_0 | Native | 2.65 | 1.82 |
| Bonsai 2 2B PTQ1_0 | x64/SSE2 | 2.63 | 1.75 |
| Bonsai 2 2B PTQ1_0 | SSSE3 | 2.85 | 1.91 |

All listed variants completed non-thinking streaming and clean shutdown/PID
cleanup. The Bonsai 1.7B gate completed in 114.00 seconds, the 4B gate in
518.67 seconds, and the compact Bonsai 2 gate in 168.09 seconds. An initial
1.7B benchmark attempt failed at the HTTP boundary; the full subsequent
native/SSE2/SSSE3 gate passed. Reproduce any intermittent failure before
claiming release stability.

Sources: the publisher's [1.7B](https://huggingface.co/prism-ml/Ternary-Bonsai-1.7B-gguf),
[4B](https://huggingface.co/prism-ml/Ternary-Bonsai-4B-gguf), and
[Bonsai 2 2B](https://huggingface.co/prism-ml/Ternary-Bonsai-2-2B-gguf) model cards.

### Rejected candidate: BitNet 2B4T TQ2_0

The [Synapticode conversion](https://huggingface.co/Synapticode/bitnet-b1.58-2B-4T-tq2_0-gguf),
revision `f965b4212a598c7a7d05c815135e214e2388a444`, loaded and streamed on the
native backend (prompt 1.32 t/s; generation 0.69 t/s). Its generated greeting
repeated a short word instead of providing a coherent answer. It failed
manual quality review despite passing the old character-only heuristic.
Validation was stopped by terminating that test's engine; SSE2/SSSE3 quality
was not established. The catalogue records it as `rejected`, never as a
downloadable or recommended model. The self-test now also rejects short-word
repetition floods. We did not change the engine pin or introduce bitnet.cpp.

## Small Qwen compatibility measurements — 2026-10-26

Same Intel i3-3217U host, 2 physical-core threads, 2048-token context and pinned
`prism-b10826-c9c34bc` engine. Thinking is disabled. Each backend is isolated,
with no fallback; file size, SHA-256 and GGUF architecture are checked before
loading. These are short `EngineManager::benchmark` samples (up to 16 output
tokens), not controlled pp64/tg32 or release performance measurements. The
machine was also downloading validation files during part of this run.

| Model / quant | Backend | Prompt t/s | Generation t/s |
|---|---|---:|---:|
| Qwen 2.5 Instruct 0.5B Q4_K_M | Native | 19.12 | 9.85 |
| Qwen 2.5 Instruct 0.5B Q4_K_M | x64/SSE2 | 2.25 | 2.33 |
| Qwen 2.5 Instruct 0.5B Q4_K_M | SSSE3 | 2.71 | 2.40 |
| Qwen 3 0.6B Q4_K_M | Native | 21.61 | 12.35 |
| Qwen 3 0.6B Q4_K_M | x64/SSE2 | 2.65 | 1.95 |
| Qwen 3 0.6B Q4_K_M | SSSE3 | 2.68 | 2.01 |

Qwen 2.5 completed the three-backend gate in 104.42 seconds; Qwen 3 took
99.74 seconds. The greetings were coherent (for example, “Hello! It's a
pleasure to meet you.” and “Hello! How can I assist you today?”), with no
reasoning output. Shutdown removed each PID file. This is a short compatibility
and obvious-garbage check, not a broad quality evaluation or a promise about
long prompts.

Sources: [Qwen's 0.5B GGUF](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF)
and [Unsloth's Qwen 3 0.6B Q4_K_M](https://huggingface.co/unsloth/Qwen3-0.6B-GGUF).
The catalogue records exact revisions and hashes. Reproduce with
`TNN_TEST_MODEL_ID=<catalogue-id> TNN_TEST_MODEL=<verified-file> cargo test -p tnn-core --test real_engine -- --ignored --nocapture`.
