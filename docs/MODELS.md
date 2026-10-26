# Models

The catalogue is compiled into `tnn-core` from `crates/tnn-core/assets/catalog.json`.
There is no remote manifest in v1: a compromised CDN cannot silently add a model.

## Validated catalogue

These entries are downloadable when their RAM requirements fit the machine.
Automatic recommendations select among the validated Qwen models by RAM and
minimum-mode limits.

| ID | Display name | Quant | Download | Minimum RAM | Notes |
|---|---|---:|---:|---:|---|
| `qwen2-5-0-5b-instruct-q4-k-m` | Qwen 2.5 Instruct · 0.5B | Q4_K_M | 491 MB | 4 GB | Small instruction-tuned chat model; official Qwen GGUF |
| `qwen3-0-6b-q4-k-m` | Qwen 3 · 0.6B | Q4_K_M | 397 MB | 4 GB | Unsloth conversion; thinking disabled |
| `qwen2-5-1-5b-instruct-q4-k-m` | Qwen 2.5 Instruct · 1.5B | Q4_K_M | 1.12 GB | 4 GB | Official Qwen GGUF; Apache-2.0 |
| `qwen2-5-7b-instruct-q4-k-m` | Qwen 2.5 Instruct · 7B | Q4_K_M | 4.68 GB | 8 GB | Bartowski single-file conversion; 16 GB RAM recommended |

All entries carry a pinned Hugging Face revision, exact SHA-256 and licence URL.
Recommendations prefer models whose recommended RAM fits the machine, then
choose the largest eligible model. Qwen 2.5 Instruct 1.5B is the default on
supported machines below 16 GB RAM; 7B is recommended with at least 16 GB RAM
and more than two physical cores. Minimum mode selects a model of at most
2B parameters, keeps context at 2K and threads within physical cores,
and defaults to 256-token replies. PCs with at least 8 GiB RAM can explicitly
raise the reply limit in Advanced settings, up to half the effective context;
minimum-memory machines retain the 256-token cap. **Continue answer** extends
an unfinished reply without increasing the per-request limit or discarding
the existing text.

The small Qwen choices use a 2K default context and 256-token replies to limit
prompt processing on older CPUs. They appear in the regular Models list and
use the existing download, verification and engine paths. Qwen 2.5 comes from
the publisher's [GGUF repository](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF).
Qwen 3 uses [Unsloth's Q4_K_M conversion](https://huggingface.co/unsloth/Qwen3-0.6B-GGUF)
to keep its download smaller than the publisher's Q8_0 file. Both are
Apache-2.0; the Qwen 3 licence link points to the pinned upstream licence.
Thinking remains disabled in the engine and chat template. These are options
for basic chat, with lower capacity than larger models. Runtime measurements are in
[benchmarks.md](benchmarks.md).

The larger Qwen choices are **Qwen 2.5 Instruct** in 1.5B and 7B sizes, with
Q4_K_M quantization. They retain a 2K default context, 256-token replies and
disabled thinking. The 1.5B file comes from Qwen's official GGUF repository.
The 7B uses
[Bartowski's conversion](https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF)
because Qwen's official Q4_K_M download is split across two files and the app
installs one GGUF per model. Allow 16 GB RAM for 7B for more headroom; larger
models can be very slow on the reference 2-core CPU.

The 1.5B and 7B models use Apache-2.0. Model weights are
downloaded from their source and are never bundled with the application.

## Unavailable experimental entries (hidden, not downloadable)

| ID | Reason |
|---|---|
| `bitnet-2b4t` | The pinned Synapticode TQ2_0 conversion loaded, but produced repetitive output during native validation. Rejected for this catalogue. |
| `bitnet-0-7b` | Pending; no pinned upstream GGUF URL yet. |
| `falcon3-1b-158bit` | Pending; upstream i2_s only, conversion required. |
| `falcon3-3b-158bit` | Pending; conversion required. |
| `falcon3-7b-158bit` | Pending; conversion required. |
| `falcon3-10b-158bit` | Pending; conversion required. |
| `llama3-8b-158` | Pending; Llama 3 licence acceptance required. |

`validation = pending` and/or `url = null` makes an entry non-downloadable.
`validation = rejected` also blocks downloads. The BitNet record now points
at an actual llama.cpp-compatible TQ2_0 conversion, not the incompatible
official i2_s file, but it stays rejected until it passes quality validation.
See the [conversion model card](https://huggingface.co/Synapticode/bitnet-b1.58-2B-4T-tq2_0-gguf)
and [validation measurements](benchmarks.md). This is a decision about this
specific conversion and pinned runtime, not about all BitNet models.
`settings.include_experimental` only reveals them in the UI after the gate is
passed; it does not bypass the download block. The main Qwen catalogue is
never blocked by experimental work.

### Validation gate

An experimental model may move to `validated` only when all of these hold:

1. Licence permits download and local use; attribution is shown.
2. It loads in the pinned engine and produces coherent output.
3. It is coherent on `x64` (SSE2), `ssse3` and the best available tier.
4. It reaches ≥ 1.5 tok/s generation on a 4-core desktop, or is labelled
   “Advanced hardware only” and excluded from recommendations.
5. The Hugging Face revision and SHA-256 are pinned.
6. The UI shows an “Experimental” badge and a one-line reason.

Use `scripts/convert-experimental-model.sh` to convert/quantize and emit a
catalogue stub with the resulting hash. It refuses to run without an explicit
licence confirmation.

## Manifest schema

```json
{
  "id": "qwen2-5-1-5b-instruct-q4-k-m",
  "display_name": "Qwen 2.5 Instruct · 1.5B",
  "tier": "fast",
  "family": "Qwen 2.5",
  "params_b": 1.54,
  "quant": "Q4_K_M",
  "file_name": "qwen2.5-1.5b-instruct-q4_k_m.gguf",
  "url": "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/91cad51170dc346986eccefdc2dd33a9da36ead9/qwen2.5-1.5b-instruct-q4_k_m.gguf",
  "sha256": "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
  "size_bytes": 1117320736,
  "license": "Apache-2.0",
  "license_url": "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/blob/91cad51170dc346986eccefdc2dd33a9da36ead9/LICENSE",
  "model_card_url": "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/blob/91cad51170dc346986eccefdc2dd33a9da36ead9/README.md",
  "revision": "91cad51170dc346986eccefdc2dd33a9da36ead9",
  "min_ram_bytes": 4294967296,
  "recommended_ram_bytes": 4294967296,
  "min_disk_bytes": 1654191648,
  "context_default": 2048,
  "context_max": 32768,
  "max_tokens_default": 256,
  "sampling": {
    "temperature": 0.7,
    "top_p": 0.8,
    "top_k": 20,
    "min_p": 0.0,
    "presence_penalty": 0.0,
    "repeat_penalty": 1.05,
    "repeat_last_n": 64
  },
  "enable_thinking": false,
  "engine_min_tier": "x64",
  "architecture": "qwen2",
  "description": "modelDescriptions.qwen25Instruct",
  "badge": null,
  "experimental": false,
  "validation": "validated",
  "unavailable_reason": null
}
```

Rules enforced by the catalogue validator and tests:

- IDs are `[a-z0-9_-]`, unique and safe to use in paths.
- Downloadable entries have a pinned HTTPS URL, 40-character revision, 64-hex
  SHA-256, positive size and a licence URL.
- `enable_thinking` is `false` for every default entry.
- Sizes and RAM fit are consistent with the tier.

## Download pipeline

1. `download_model(id)` validates the ID, catalogue entry, allowlist and free
   space (`size + 512 MB`).
2. One active download at a time; the rest queue on a semaphore.
3. Data streams to `downloads/<file>.part`, updating SHA-256 as it arrives.
4. Resume uses `Range`; `Content-Range` must start exactly where the `.part`
   file ends. A server that ignores `Range` restarts cleanly.
5. Pause keeps `.part`; cancel deletes it. Both survive app restarts.
6. On completion: compare SHA-256, parse the GGUF header (magic, version,
   architecture), fsync, then atomically rename into `models/`.
7. Persist `installed_models`; emit `Installed` (or `Failed`/`Paused`).
8. A checksum mismatch deletes the partial file and reports `download_checksum`
   with a retry action.

Local import copies the chosen file into `models/`, hashes it, validates the
GGUF header and assigns an `imported-<hash>` ID. Moving a file out of the
managed directory after import is not supported.

## Licences and attribution

- Qwen 2.5 Instruct 0.5B and Qwen 3 0.6B: Apache-2.0; Qwen 3 uses an
  attributed Unsloth GGUF conversion.
- Qwen 2.5 Instruct 1.5B and 7B: Apache-2.0; 7B uses an attributed
  Bartowski conversion.
- The UI shows the model name, licence and source link before download.
- Apache-2.0/MIT do not require a checkbox; Llama 3 does.
- Model weights are not bundled and are not mirrored without a licence check.
- See `THIRD-PARTY-NOTICES.md` for engine and dependency attribution.
