# Third-party notices

This project is dual-licensed under MIT or Apache-2.0. The notices below cover
the main components distributed with or required by the app. Exact versions and
full licence texts are recorded in `Cargo.lock` and `package-lock.json`; before
a public release, generate a complete dependency report (`cargo about`,
`license-checker`/`npm-license-crawler`) and ship it with the installers.

## Inference engine

**PrismML llama.cpp** — <https://github.com/PrismML-Eng/llama.cpp>
Pinned release `prism-b10826-c9c34bc`. MIT licence, copyright the ggml authors.
The downloaded archive contains a `LICENSE` file that is copied into
`src-tauri/resources/engine/` and `engine-baseline/`.

The engine includes work from the llama.cpp project and its contributors.
It is distributed as a separate executable/sidecar, not linked into this app.

## Models

Model weights are downloaded separately and are **not** included in the
installer.

| Model family | Licence | Source |
|---|---|---|
| Ternary Bonsai 2 | Apache-2.0 | <https://huggingface.co/prism-ml> |
| BitNet b1.58 (experimental) | MIT | upstream model card |
| Falcon3 (experimental) | Apache-2.0 | <https://huggingface.co/tiiuae> |
| Llama 3 8B 1.58-bit (experimental) | Llama 3 Community Licence | requires acceptance |

Experimental entries are hidden and not downloadable until they pass the
validation gate. Check each model card before redistributing or mirroring
weights.

## Application stack

| Component | Licence |
|---|---|
| Tauri | MIT or Apache-2.0 |
| Svelte | MIT |
| Vite / Rolldown | MIT |
| Tailwind CSS | MIT |
| TypeScript | Apache-2.0 |
| `@tauri-apps/*` plugins | MIT or Apache-2.0 |
| `marked` | MIT |
| `DOMPurify` | MPL-2.0 or Apache-2.0 |
| `svelte-i18n` | MIT |
| `svelte-sonner` | MIT |
| `@lucide/svelte` (Lucide icons) | ISC |

## Rust crates

Direct dependencies include `tauri`, `tokio`, `reqwest` (rustls), `rusqlite`
(bundled SQLite), `serde`, `serde_json`, `thiserror`, `tracing`,
`tracing-subscriber`, `tracing-appender`, `sha2`, `hex`, `rand`, `raw-cpuid`,
`sysinfo`, `ts-rs`, `uuid`, `chrono`, `futures-util`, `tokio-util`, `zip`,
`rusqlite_migration`. Most are MIT or Apache-2.0; `rusqlite`'s bundled SQLite is
public domain. See `Cargo.lock` and each crate's `LICENSE` for the exact terms.

## Fonts and images

No remote fonts or images are loaded. The UI uses system font stacks and local
SVG icons. Generated app icons are original to this project.

## Reporting

If you believe a notice is missing or incorrect, open an issue before a release
is published. Do not ship an installer until the generated dependency report has
been reviewed.
