# Models

The catalogue is compiled into `tnn-core` from `crates/tnn-core/assets/catalog.json`.
There is no remote manifest in v1: a compromised CDN cannot silently add a model.

## Validated catalogue

These entries are downloadable and recommended by hardware tier.

| ID | Display name | Quant | Download | Minimum RAM | Notes |
|---|---|---:|---:|---:|---|
| `bonsai2-2b-pq2` | Bonsai 2 · 2B | PQ2_0 | 517 MB | 4 GB | Default on minimum mode |
| `bonsai2-4b-pq2` | Bonsai 2 · 4B | PQ2_0 | 1.15 GB | 8 GB | Balanced |
| `bonsai2-9b-pq2` | Bonsai 2 · 9B | PQ2_0 | 2.42 GB | 12 GB | Smart |
| `bonsai2-27b-pq2` | Bonsai 2 · 27B | PQ2_0 | 7.21 GB | 24 GB | Advanced |
| `bonsai2-27b-ptq1` | Bonsai 2 · 27B (smaller download) | PTQ1_0 | 5.95 GB | 24 GB | Alternative 27B quant |

All entries carry a pinned Hugging Face revision, exact SHA-256 and licence URL.
The recommendation picks the largest validated model that fits available RAM;
`minimum_mode` (4 GB or ≤2 physical cores) hard-caps context at 2K, threads at
the physical core count and answers at 256 tokens.

## Experimental entries (hidden, not downloadable)

| ID | Reason |
|---|---|
| `bitnet-2b4t` | Pending conversion/validation. i2_s is slow on non-AVX2 CPUs. |
| `bitnet-0-7b` | Pending; no pinned upstream GGUF URL yet. |
| `falcon3-1b-158bit` | Pending; upstream i2_s only, conversion required. |
| `falcon3-3b-158bit` | Pending; conversion required. |
| `falcon3-7b-158bit` | Pending; conversion required. |
| `falcon3-10b-158bit` | Pending; conversion required. |
| `llama3-8b-158` | Pending; Llama 3 licence acceptance required. |

`validation = pending` and/or `url = null` makes an entry non-downloadable.
`settings.include_experimental` only reveals them in the UI after the gate is
passed; it does not bypass the download block. The main Bonsai catalogue is
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
  "id": "bonsai2-2b-pq2",
  "display_name": "Bonsai 2 · 2B",
  "tier": "fast",
  "family": "bonsai2",
  "params_b": 2.0,
  "quant": "PQ2_0",
  "file_name": "Ternary-Bonsai-2-2B-PQ2_0.gguf",
  "url": "https://huggingface.co/.../resolve/<revision>/<file>",
  "sha256": "<64 hex chars>",
  "size_bytes": 517312992,
  "license": "Apache-2.0",
  "license_url": "https://huggingface.co/prism-ml/Ternary-Bonsai-2-2B-gguf",
  "model_card_url": "https://huggingface.co/prism-ml/Ternary-Bonsai-2-2B-gguf",
  "revision": "<40 hex chars>",
  "min_ram_bytes": 4294967296,
  "recommended_ram_bytes": 8589934592,
  "min_disk_bytes": 1073741824,
  "context_default": 2048,
  "context_max": 32768,
  "max_tokens_default": 256,
  "sampling": {
    "temperature": 0.7,
    "top_p": 0.8,
    "top_k": 20,
    "min_p": 0.0,
    "presence_penalty": 1.5,
    "repeat_penalty": 1.0,
    "repeat_last_n": 64
  },
  "enable_thinking": false,
  "engine_min_tier": "x64",
  "architecture": "bonsai",
  "description": "...",
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

- Bonsai 2 entries: Apache-2.0.
- The UI shows the model name, licence and source link before download.
- Apache-2.0/MIT do not require a checkbox; Llama 3 does.
- Model weights are not bundled and are not mirrored without a licence check.
- See `THIRD-PARTY-NOTICES.md` for engine and dependency attribution.
