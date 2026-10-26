# UX

## Principles

- Calm, plain-spoken, one primary action per screen.
- Never require a technical parameter to start chatting.
- Show honest speed and memory expectations before download and after readiness.
- Keep the UI responsive while the CPU is saturated: batched deltas, no
  expensive work per token, no animation that forces layout.
- Accessible by default: semantic controls, visible focus, keyboard paths,
  contrast, reduced-motion support.

Use “ultra-light AI models” in normal copy. “Ternary” belongs in About or
technical details.

## Onboarding

Five steps, skippable only when a model is already installed:

1. **Welcome** — local/private promise, no account, works offline.
2. **Hardware check** — RAM, physical cores, free disk, detected CPU tier and
   the recommended model. Minimum-mode machines get a plain warning (“Your PC
   is at the minimum…”).
3. **Choose a model** — cards show download size, memory fit, speed expectation
   and a “Recommended for your PC” badge. Models that do not fit are disabled
   with a reason. “More models” reveals larger and experimental entries.
4. **Download** — progress, bytes, speed, ETA, pause/cancel, reassurance that
   the machine remains usable. Download progress survives navigation.
5. **Ready** — suggested prompts and an optional speed check.

## Chat

- Sidebar: new chat, search, conversation list, delete, model shortcut.
- Header: active model, engine state, model switch.
- Message stream: user bubbles, assistant Markdown, streaming cursor, stop,
  regenerate, copy, partial/interrupted badge, optional reasoning panel
  (hidden by default).
- Composer: multiline, Enter sends, Shift+Enter adds a line, disabled while
  sending with a visible stop action.
- Empty state: three local suggestion chips, no network.
- Generation errors become a friendly `ErrorCard` with recovery actions
  (retry, restart engine, choose a smaller model, new chat, diagnostics).

Keyboard shortcuts:

| Keys | Action |
|---|---|
| `Ctrl/Cmd+N` | New chat |
| `Ctrl/Cmd+K` | Focus conversation search |
| `Ctrl/Cmd+,` | Settings |
| `Esc` | Stop the current generation |
| `Enter` / `Shift+Enter` | Send / newline |

Links in assistant Markdown never navigate the webview. They are validated to
`http`/`https` and opened in the system browser.

## Models page

- Installed models with size, quant, last use and “Use this model”.
- Catalogue cards grouped by fit, with download/pause/resume/cancel actions.
- Storage summary and “Free up space”.
- Import a `.gguf` file; validation errors are shown before installation.
- Experimental entries remain hidden unless enabled after validation.
- Licence link and model card link are visible without downloading.

## Settings

Simple mode exposes theme, language, active model, keep-loaded/idle timeout,
font scale and onboarding replay. Advanced mode adds threads, context, max
tokens, sampling, system prompt, reasoning display and experimental visibility.
Values that are `null` follow hardware/model defaults; the UI shows the resolved
value and labels it as automatic.

Validation happens in Rust (`Settings::validate`); the UI surfaces the error and
keeps the previous value. Unknown keys are rejected.

## States and errors

Every `ErrorPayload` has a stable code and one or more recovery actions. The UI:

- maps the code to a localized message;
- shows technical detail only in Advanced;
- offers only actions that make sense (`free_memory`, `choose_smaller_model`,
  `redownload`, `restart_engine`, `export_diagnostics`, …);
- never blocks the rest of the app for a model-specific failure.

Specific states:

- Engine missing: models can still be downloaded; chat explains how to fetch
  the engine in development builds.
- Model missing/corrupt: offer re-download or import.
- Disk full: show required vs available and offer cleanup.
- Context overflow: offer a new chat or a smaller context.
- Interrupted answer: keep the partial text, label it, offer regenerate.
- Offline: already-downloaded models keep working; downloads show a retry.

## i18n

- Languages: English (`en`) and Brazilian Portuguese (`pt-BR`).
- All strings live in `src/locales/*.json`; no hardcoded UI copy.
- The files must have identical keys and interpolation placeholders (Vitest
  enforces this).
- Dates, numbers, bytes and relative times use the active locale.
- Language changes apply immediately and are persisted in settings.

## Accessibility

- Landmarks and headings for each route; sidebar is a labelled navigation.
- Icon-only buttons have `aria-label` and a tooltip.
- Focus rings are never removed without a replacement.
- Streaming regions use `role="status"`/`aria-live="polite"` sparingly; deltas
  are not announced token-by-token.
- Colour is never the only signal; contrast meets WCAG AA for body text.
- The app respects `prefers-reduced-motion` and uses short, non-essential
  animations only.
- Keyboard-only path: onboarding → download → chat → stop → switch model.

## Performance on weak machines

- Deltas batched at 50 ms; the store appends in one update per batch.
- Conversation list and long messages render without virtualization in v1 but
  avoid shadow DOM, per-token reactivity and layout thrash.
- The engine runs below normal priority; the UI thread is never blocked by
  filesystem hashing (moved to blocking tasks).
- Font scale and layout reflow are user-driven, not per token.
