<script lang="ts">
  import { t } from 'svelte-i18n';
  import { toast } from 'svelte-sonner';
  import {
    AlertTriangle,
    Boxes,
    Cpu,
    Download,
    FileArchive,
    FolderOpen,
    Gauge,
    Languages,
    MessageSquare,
    Monitor,
    Moon,
    RotateCcw,
    Save,
    Settings2,
    Sun,
    Trash2,
  } from '@lucide/svelte';
  import { app } from '../stores/app.svelte';
  import { chat } from '../stores/chat.svelte';
  import { router } from '../lib/router.svelte';
  import { api } from '../lib/tauri';
  import { normalizeError } from '../lib/errors';
  import {
    formatBytes,
    formatNumber,
    tokensToWordsPerSecond,
  } from '../lib/format';
  import type {
    ErrorPayload,
    Theme,
  } from '../lib/bindings/index';
  import Toggle from '../components/ui/Toggle.svelte';
  import ErrorCard from '../components/ui/ErrorCard.svelte';

  const DEFAULT_SYSTEM_PROMPT =
    'You are a helpful assistant. Answer clearly and concisely.';

  let tab = $state<'simple' | 'advanced'>(
    app.settings?.advanced_mode ? 'advanced' : 'simple',
  );
  let error = $state<ErrorPayload | null>(null);
  let busy = $state(false);
  let benchmarking = $state(false);
  let checkingUpdates = $state(false);
  let exporting = $state(false);

  let form = $state({
    threads: app.settings?.threads ?? null,
    context_size: app.settings?.context_size ?? null,
    max_tokens: app.settings?.max_tokens ?? null,
    temperature: app.settings?.temperature ?? null,
    top_p: app.settings?.top_p ?? null,
    top_k: app.settings?.top_k ?? null,
    presence_penalty: app.settings?.presence_penalty ?? null,
    system_prompt: app.settings?.system_prompt ?? DEFAULT_SYSTEM_PROMPT,
  });

  let fontScale = $state(app.settings?.font_scale ?? 1);

  const settings = $derived(app.settings);
  const installed = $derived(app.models.filter((card) => card.installed));
  const activeModel = $derived(app.activeModel);
  const tier = $derived(
    app.engine?.tier ?? app.hardware?.tier ?? $t('common.unknown'),
  );

  async function apply(patch: Record<string, unknown>): Promise<void> {
    error = null;
    try {
      await app.updateSettings(patch);
    } catch (caught) {
      error = normalizeError(caught);
    }
  }

  async function setTheme(theme: Theme): Promise<void> {
    await apply({ theme });
  }

  async function setLanguage(language: 'en' | 'pt-BR'): Promise<void> {
    await app.setLanguage(language);
  }

  async function setActiveModel(event: Event): Promise<void> {
    const modelId = (event.currentTarget as HTMLSelectElement).value;
    if (!modelId) return;
    busy = true;
    error = null;
    try {
      await app.setActiveModel(modelId);
      await app.startEngine(modelId);
    } catch (caught) {
      error = normalizeError(caught);
    } finally {
      busy = false;
    }
  }

  async function clearChats(): Promise<void> {
    if (!window.confirm($t('settings.clearConfirm'))) return;
    busy = true;
    error = null;
    try {
      for (const conversation of [...app.conversations]) {
        await api.deleteConversation(conversation.id);
      }
      await chat.newChat();
      await app.refreshConversations();
      toast.success($t('settings.chatsCleared'));
    } catch (caught) {
      error = normalizeError(caught);
    } finally {
      busy = false;
    }
  }

  async function checkUpdates(): Promise<void> {
    checkingUpdates = true;
    error = null;
    try {
      const update = await api.checkForUpdates();
      app.update = update;
      if (update) {
        toast.success($t('settings.updateAvailable', { values: { version: update.version } }));
      } else {
        toast.success($t('settings.upToDate'));
      }
    } catch (caught) {
      error = normalizeError(caught);
    } finally {
      checkingUpdates = false;
    }
  }

  async function exportDiagnostics(): Promise<void> {
    exporting = true;
    error = null;
    try {
      const path = await api.exportDiagnostics();
      toast.success($t('settings.diagnosticsExported', { values: { path } }));
    } catch (caught) {
      error = normalizeError(caught);
    } finally {
      exporting = false;
    }
  }

  async function runBenchmark(): Promise<void> {
    const modelId = app.bootstrap?.active_model_id;
    if (!modelId) return;
    benchmarking = true;
    error = null;
    try {
      const result = await api.runBenchmark(modelId);
      toast.success(
        $t('settings.benchmarkResult', {
          values: {
            speed: formatNumber(tokensToWordsPerSecond(result.generation_tps), 1),
          },
        }),
      );
    } catch (caught) {
      error = normalizeError(caught);
    } finally {
      benchmarking = false;
    }
  }

  async function saveAdvanced(): Promise<void> {
    busy = true;
    error = null;
    try {
      const patch = {
        threads: form.threads,
        context_size: form.context_size,
        max_tokens: form.max_tokens,
        temperature: form.temperature,
        top_p: form.top_p,
        top_k: form.top_k,
        presence_penalty: form.presence_penalty,
        system_prompt: form.system_prompt,
      };
      await app.updateSettings(patch);
      toast.success($t('settings.saved'));
    } catch (caught) {
      error = normalizeError(caught);
    } finally {
      busy = false;
    }
  }

  function resetAdvanced(): void {
    form = {
      threads: null,
      context_size: null,
      max_tokens: null,
      temperature: null,
      top_p: null,
      top_k: null,
      presence_penalty: null,
      system_prompt: DEFAULT_SYSTEM_PROMPT,
    };
    fontScale = 1;
  }

  async function updateFontScale(value: number): Promise<void> {
    fontScale = value;
    await apply({ font_scale: value });
  }
</script>

<section class="h-full overflow-y-auto">
  <div class="mx-auto max-w-3xl px-6 py-8">
    <h1 class="text-2xl font-semibold tracking-tight">{$t('settings.title')}</h1>
    <p class="mt-1 text-sm text-stone-500 dark:text-stone-400">
      {$t('settings.subtitle')}
    </p>

    <div class="mt-5 flex gap-1 rounded-xl bg-stone-200/60 p-1 dark:bg-stone-800/60">
      <button
        type="button"
        class="flex-1 rounded-lg px-3 py-2 text-sm font-medium {tab === 'simple'
          ? 'bg-white text-stone-900 shadow-sm dark:bg-stone-900 dark:text-stone-50'
          : 'text-stone-500 hover:text-stone-800 dark:text-stone-400 dark:hover:text-stone-200'}"
        onclick={() => (tab = 'simple')}
      >
        {$t('settings.simpleTab')}
      </button>
      <button
        type="button"
        class="flex-1 rounded-lg px-3 py-2 text-sm font-medium {tab === 'advanced'
          ? 'bg-white text-stone-900 shadow-sm dark:bg-stone-900 dark:text-stone-50'
          : 'text-stone-500 hover:text-stone-800 dark:text-stone-400 dark:hover:text-stone-200'}"
        onclick={() => (tab = 'advanced')}
      >
        {$t('settings.advancedTab')}
      </button>
    </div>

    {#if error}
      <div class="mt-4"><ErrorCard error={error} onDismiss={() => (error = null)} /></div>
    {/if}

    {#if tab === 'simple'}
      <div class="mt-5 space-y-4">
        <section class="card p-5">
          <h2 class="flex items-center gap-2 text-sm font-semibold">
            <Monitor class="size-4 text-brand-700 dark:text-brand-300" aria-hidden="true" />
            {$t('settings.appearance')}
          </h2>
          <div class="mt-4 space-y-4">
            <div>
              <span class="label">{$t('settings.theme')}</span>
              <div class="mt-1.5 grid grid-cols-3 gap-2">
                {#each [
                  { value: 'system' as Theme, label: $t('settings.themeSystem'), icon: Monitor },
                  { value: 'light' as Theme, label: $t('settings.themeLight'), icon: Sun },
                  { value: 'dark' as Theme, label: $t('settings.themeDark'), icon: Moon },
                ] as option (option.value)}
                  <button
                    type="button"
                    class="flex items-center justify-center gap-1.5 rounded-xl border px-2 py-2 text-xs font-medium {settings?.theme ===
                    option.value
                      ? 'border-brand-500 bg-brand-50 text-brand-900 dark:border-brand-400 dark:bg-brand-950/40 dark:text-brand-100'
                      : 'border-stone-200 bg-white text-stone-600 hover:bg-stone-50 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-300 dark:hover:bg-stone-800'}"
                    onclick={() => setTheme(option.value)}
                  >
                    <option.icon class="size-3.5" aria-hidden="true" />
                    {option.label}
                  </button>
                {/each}
              </div>
            </div>

            <div>
              <span class="label">{$t('settings.language')}</span>
              <div class="mt-1.5 flex gap-2">
                <button
                  type="button"
                  class="btn-secondary flex-1 text-xs {settings?.language === 'en'
                    ? 'border-brand-500 ring-1 ring-brand-500/30'
                    : ''}"
                  onclick={() => setLanguage('en')}
                >
                  <Languages class="size-3.5" aria-hidden="true" />
                  {$t('settings.languageEn')}
                </button>
                <button
                  type="button"
                  class="btn-secondary flex-1 text-xs {settings?.language === 'pt-BR'
                    ? 'border-brand-500 ring-1 ring-brand-500/30'
                    : ''}"
                  onclick={() => setLanguage('pt-BR')}
                >
                  <Languages class="size-3.5" aria-hidden="true" />
                  {$t('settings.languagePtBr')}
                </button>
              </div>
            </div>

            <div>
              <label class="label flex items-center justify-between" for="font-scale">
                <span>{$t('settings.fontScale')}</span>
                <span class="text-xs font-normal text-stone-500">{Math.round(fontScale * 100)}%</span>
              </label>
              <input
                id="font-scale"
                type="range"
                min="0.8"
                max="2"
                step="0.1"
                value={fontScale}
                class="mt-2 w-full accent-brand-700 dark:accent-brand-400"
                onchange={(event) => updateFontScale(Number(event.currentTarget.value))}
              />
            </div>
          </div>
        </section>

        <section class="card p-5">
          <h2 class="flex items-center gap-2 text-sm font-semibold">
            <Boxes class="size-4 text-brand-700 dark:text-brand-300" aria-hidden="true" />
            {$t('settings.modelSection')}
          </h2>
          <div class="mt-4 space-y-4">
            <div>
              <label class="label" for="active-model">{$t('settings.activeModel')}</label>
              <div class="mt-1.5 flex gap-2">
                <select
                  id="active-model"
                  class="input"
                  value={app.bootstrap?.active_model_id ?? ''}
                  disabled={busy || installed.length === 0}
                  onchange={setActiveModel}
                >
                  {#if installed.length === 0}
                    <option value="">{$t('models.noInstalled')}</option>
                  {/if}
                  {#each installed as card (card.model.id)}
                    <option value={card.model.id}>{card.model.display_name}</option>
                  {/each}
                </select>
                <button type="button" class="btn-secondary shrink-0 text-xs" onclick={() => router.navigate('models')}>
                  {$t('settings.manageModels')}
                </button>
              </div>
              {#if activeModel}
                <p class="mt-1 text-xs text-stone-500 dark:text-stone-400">
                  {formatBytes(activeModel.model.size_bytes)} · {activeModel.model.quant}
                </p>
              {/if}
            </div>

            <Toggle
              checked={settings?.keep_model_loaded ?? true}
              label={$t('settings.keepLoaded')}
              description={$t('settings.keepLoadedBody')}
              onchange={(value) => apply({ keep_model_loaded: value })}
            />

            <div class="flex items-center justify-between gap-4">
              <div>
                <p class="text-sm font-medium">{$t('settings.idleUnload')}</p>
                <p class="hint">{$t('settings.idleUnloadBody')}</p>
              </div>
              <select
                class="input w-32"
                value={String(settings?.idle_unload_minutes ?? 30)}
                onchange={(event) => apply({ idle_unload_minutes: Number(event.currentTarget.value) })}
              >
                <option value="0">{$t('common.none')}</option>
                <option value="5">5 {$t('common.minutes')}</option>
                <option value="15">15 {$t('common.minutes')}</option>
                <option value="30">30 {$t('common.minutes')}</option>
                <option value="60">60 {$t('common.minutes')}</option>
              </select>
            </div>
          </div>
        </section>

        <section class="card p-5">
          <h2 class="flex items-center gap-2 text-sm font-semibold">
            <MessageSquare class="size-4 text-brand-700 dark:text-brand-300" aria-hidden="true" />
            {$t('settings.chatsSection')}
          </h2>
          <div class="mt-3 flex items-center justify-between gap-4">
            <div>
              <p class="text-sm font-medium">{$t('settings.clearChats')}</p>
              <p class="hint">{$t('settings.clearChatsBody')}</p>
            </div>
            <button type="button" class="btn-secondary shrink-0 text-xs" disabled={busy} onclick={clearChats}>
              <Trash2 class="size-3.5" aria-hidden="true" />
              {$t('common.delete')}
            </button>
          </div>
        </section>

        <section class="card p-5">
          <h2 class="flex items-center gap-2 text-sm font-semibold">
            <Download class="size-4 text-brand-700 dark:text-brand-300" aria-hidden="true" />
            {$t('settings.updatesSection')}
          </h2>
          <div class="mt-3 flex items-center justify-between gap-4">
            <div>
              <p class="text-sm font-medium">{$t('settings.checkUpdates')}</p>
              {#if app.update}
                <p class="hint">{$t('settings.updateAvailable', { values: { version: app.update.version } })}</p>
              {/if}
            </div>
            <button type="button" class="btn-secondary shrink-0 text-xs" disabled={checkingUpdates} onclick={checkUpdates}>
              {checkingUpdates ? $t('settings.checkingUpdates') : $t('settings.checkUpdates')}
            </button>
          </div>
        </section>

        <section class="card p-5">
          <h2 class="flex items-center gap-2 text-sm font-semibold">
            <FileArchive class="size-4 text-brand-700 dark:text-brand-300" aria-hidden="true" />
            {$t('settings.diagnosticsSection')}
          </h2>
          <p class="hint mt-1">{$t('settings.diagnosticsHint')}</p>
          <div class="mt-3 flex flex-wrap gap-2">
            <button type="button" class="btn-secondary text-xs" disabled={exporting} onclick={exportDiagnostics}>
              {$t('settings.exportDiagnostics')}
            </button>
            <button type="button" class="btn-ghost text-xs" onclick={() => api.openDataDir()}>
              <FolderOpen class="size-3.5" aria-hidden="true" />
              {$t('settings.openData')}
            </button>
            <button type="button" class="btn-ghost text-xs" onclick={() => api.openLogs()}>
              <FolderOpen class="size-3.5" aria-hidden="true" />
              {$t('settings.openLogs')}
            </button>
          </div>
        </section>

        <section class="card p-5">
          <h2 class="flex items-center gap-2 text-sm font-semibold">
            <Gauge class="size-4 text-brand-700 dark:text-brand-300" aria-hidden="true" />
            {$t('settings.benchmarkSection')}
          </h2>
          <p class="hint mt-1">{$t('settings.benchmarkHint')}</p>
          <button
            type="button"
            class="btn-secondary mt-3 text-xs"
            disabled={benchmarking || !app.bootstrap?.active_model_id}
            onclick={runBenchmark}
          >
            <Gauge class="size-3.5" aria-hidden="true" />
            {$t('common.benchmark')}
          </button>
        </section>

        <section class="card p-5">
          <h2 class="flex items-center gap-2 text-sm font-semibold">
            <Cpu class="size-4 text-brand-700 dark:text-brand-300" aria-hidden="true" />
            {$t('hardware.processor')}
          </h2>
          <dl class="mt-3 grid grid-cols-2 gap-2 text-xs sm:grid-cols-4">
            <div>
              <dt class="text-stone-500 dark:text-stone-400">{$t('hardware.ram')}</dt>
              <dd class="font-medium">{formatBytes(app.hardware?.total_ram_bytes)}</dd>
            </div>
            <div>
              <dt class="text-stone-500 dark:text-stone-400">{$t('hardware.cores')}</dt>
              <dd class="font-medium">{app.hardware?.physical_cores ?? '—'}</dd>
            </div>
            <div>
              <dt class="text-stone-500 dark:text-stone-400">{$t('settings.engineTier')}</dt>
              <dd class="font-medium capitalize">{tier}</dd>
            </div>
            <div>
              <dt class="text-stone-500 dark:text-stone-400">{$t('app.version', { values: { version: app.bootstrap?.app_version ?? '—' } })}</dt>
              <dd class="font-medium">{app.bootstrap?.catalog_version ?? '—'}</dd>
            </div>
          </dl>
        </section>
      </div>
    {:else}
      <div class="mt-5 space-y-4">
        <div class="flex items-start gap-3 rounded-2xl border border-amber-200 bg-amber-50 p-4 dark:border-amber-900 dark:bg-amber-950/40">
          <AlertTriangle class="mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-300" aria-hidden="true" />
          <p class="text-xs leading-relaxed text-amber-900 dark:text-amber-100">
            {$t('settings.advancedWarning')}
          </p>
        </div>

        <section class="card p-5">
          <h2 class="flex items-center gap-2 text-sm font-semibold">
            <Settings2 class="size-4 text-brand-700 dark:text-brand-300" aria-hidden="true" />
            {$t('settings.performance')}
          </h2>
          <div class="mt-4 space-y-4">
            <div>
              <label class="label" for="threads">{$t('settings.threads')}</label>
              <select
                id="threads"
                class="input mt-1.5"
                value={form.threads === null ? 'auto' : String(form.threads)}
                onchange={(event) =>
                  (form.threads =
                    event.currentTarget.value === 'auto'
                      ? null
                      : Number(event.currentTarget.value))}
              >
                <option value="auto">Auto ({app.hardware?.physical_cores ?? '—'})</option>
                {#each Array(8) as _, index (index)}
                  <option value={String(index + 1)}>{index + 1}</option>
                {/each}
              </select>
              <p class="hint mt-1">{$t('settings.threadsHint')}</p>
            </div>

            <div>
              <label class="label" for="context">{$t('settings.context')}</label>
              <select
                id="context"
                class="input mt-1.5"
                value={form.context_size === null ? 'auto' : String(form.context_size)}
                onchange={(event) =>
                  (form.context_size =
                    event.currentTarget.value === 'auto'
                      ? null
                      : Number(event.currentTarget.value))}
              >
                <option value="auto">Auto</option>
                <option value="2048">{$t('settings.context2048')}</option>
                <option value="4096">{$t('settings.context4096')}</option>
                <option value="8192">{$t('settings.context8192')}</option>
              </select>
              <p class="hint mt-1">{$t('settings.contextHint')}</p>
            </div>

            <div>
              <label class="label" for="max-tokens">{$t('settings.maxTokens')}</label>
              <input
                id="max-tokens"
                class="input mt-1.5"
                type="number"
                min="1"
                max="8192"
                placeholder="Auto"
                value={form.max_tokens ?? ''}
                oninput={(event) =>
                  (form.max_tokens =
                    event.currentTarget.value === ''
                      ? null
                      : Number(event.currentTarget.value))}
              />
              <p class="hint mt-1">{$t('settings.maxTokensHint')}</p>
            </div>
          </div>
        </section>

        <section class="card p-5">
          <h2 class="text-sm font-semibold">{$t('settings.sampling')}</h2>
          <div class="mt-4 grid gap-3 sm:grid-cols-2">
            {#each [
              { key: 'temperature' as const, label: $t('settings.temperature'), min: 0, max: 2, step: 0.1 },
              { key: 'top_p' as const, label: $t('settings.topP'), min: 0, max: 1, step: 0.05 },
              { key: 'top_k' as const, label: $t('settings.topK'), min: 0, max: 500, step: 1 },
              { key: 'presence_penalty' as const, label: $t('settings.presencePenalty'), min: -2, max: 2, step: 0.1 },
            ] as field (field.key)}
              <div>
                <label class="label" for={field.key}>{field.label}</label>
                <input
                  id={field.key}
                  class="input mt-1.5"
                  type="number"
                  min={field.min}
                  max={field.max}
                  step={field.step}
                  placeholder="Auto"
                  value={form[field.key] ?? ''}
                  oninput={(event) =>
                    (form[field.key] =
                      event.currentTarget.value === ''
                        ? null
                        : Number(event.currentTarget.value))}
                />
              </div>
            {/each}
          </div>
        </section>

        <section class="card p-5">
          <h2 class="text-sm font-semibold">{$t('settings.systemPrompt')}</h2>
          <textarea
            class="input mt-2 min-h-24 resize-y"
            bind:value={form.system_prompt}
            maxlength="4000"
            aria-label={$t('settings.systemPrompt')}
          ></textarea>
          <p class="hint mt-1">{$t('settings.systemPromptHint')}</p>
        </section>

        <section class="card p-5 space-y-4">
          <Toggle
            checked={settings?.show_reasoning ?? false}
            label={$t('settings.showReasoning')}
            description={$t('settings.showReasoningHint')}
            onchange={(value) => apply({ show_reasoning: value })}
          />
          <Toggle
            checked={settings?.include_experimental ?? false}
            label={$t('models.showExperimental')}
            description={$t('models.experimentalBody')}
            onchange={(value) => apply({ include_experimental: value })}
          />
          <Toggle
            checked={settings?.advanced_mode ?? false}
            label={$t('settings.enableAdvanced')}
            description={$t('settings.advancedWarning')}
            onchange={(value) => apply({ advanced_mode: value })}
          />
        </section>

        <section class="card p-5">
          <h2 class="text-sm font-semibold">{$t('settings.engineTier')}</h2>
          <p class="mt-1 text-sm capitalize">{tier}</p>
          <p class="hint mt-1">
            {$t('hardware.tier')}: {app.hardware?.tier ?? '—'} · {$t('hardware.noAvx2')}
          </p>
          <div class="mt-3 flex flex-wrap gap-2">
            <button type="button" class="btn-ghost text-xs" onclick={() => api.openDataDir()}>
              <FolderOpen class="size-3.5" aria-hidden="true" />
              {$t('settings.openData')}
            </button>
            <button type="button" class="btn-ghost text-xs" onclick={() => api.openLogs()}>
              <FolderOpen class="size-3.5" aria-hidden="true" />
              {$t('settings.openLogs')}
            </button>
          </div>
        </section>

        <div class="flex flex-wrap items-center justify-between gap-2 pb-4">
          <button type="button" class="btn-ghost text-xs" onclick={resetAdvanced}>
            <RotateCcw class="size-3.5" aria-hidden="true" />
            {$t('settings.resetDefaults')}
          </button>
          <button type="button" class="btn-primary" disabled={busy} onclick={saveAdvanced}>
            <Save class="size-4" aria-hidden="true" />
            {$t('settings.saveAdvanced')}
          </button>
        </div>
      </div>
    {/if}
  </div>
</section>
