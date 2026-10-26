<script lang="ts">
  import { t } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import { toast } from 'svelte-sonner';
  import { open } from '@tauri-apps/plugin-dialog';
  import { openExternal } from '../lib/external';
  import {
    ChevronDown,
    Download,
    FolderOpen,
    HardDrive,
    Import,
    RefreshCw,
  } from '@lucide/svelte';
  import { app } from '../stores/app.svelte';
  import { api } from '../lib/tauri';
  import { normalizeError } from '../lib/errors';
  import { isTauri } from '../lib/tauri';
  import {
    formatBytes,
    formatNumber,
    tokensToWordsPerSecond,
  } from '../lib/format';
  import type {
    ErrorPayload,
    StorageSummary,
  } from '../lib/bindings/index';
  import ModelTile from '../components/models/ModelTile.svelte';
  import ErrorCard from '../components/ui/ErrorCard.svelte';

  let showMore = $state(false);
  let busyId = $state<string | null>(null);
  let importing = $state(false);
  let error = $state<ErrorPayload | null>(null);
  let storage = $state<StorageSummary | null>(null);

  const installed = $derived(app.models.filter((card) => card.installed));
  const visible = $derived(
    app.models
      .filter((card) => {
        if (card.installed) return false;
        if (card.model.validation !== 'validated') return false;
        if (card.model.experimental) return showMore && Boolean(app.settings?.include_experimental);
        if (card.model.params_b >= 20) return showMore;
        return true;
      })
      .sort((a, b) => {
        if (a.recommended !== b.recommended) return a.recommended ? -1 : 1;
        if (a.compatible !== b.compatible) return a.compatible ? -1 : 1;
        return a.model.params_b - b.model.params_b;
      }),
  );

  onMount(() => {
    void refreshStorage();
    void app.refreshModels();
  });

  async function refreshStorage(): Promise<void> {
    try {
      storage = await api.storageSummary();
    } catch {
      // Storage details are best-effort.
    }
  }

  async function download(modelId: string): Promise<void> {
    clearError();
    try {
      await api.downloadModel(modelId);
    } catch (caught) {
      error = normalizeError(caught);
    }
  }

  async function pause(modelId: string): Promise<void> {
    try {
      await api.pauseDownload(modelId);
    } catch (caught) {
      error = normalizeError(caught);
    }
  }

  async function resume(modelId: string): Promise<void> {
    clearError();
    try {
      await api.resumeDownload(modelId);
    } catch (caught) {
      error = normalizeError(caught);
    }
  }

  async function cancel(modelId: string): Promise<void> {
    try {
      await api.cancelDownload(modelId);
    } catch (caught) {
      error = normalizeError(caught);
    }
  }

  async function useModel(modelId: string): Promise<void> {
    busyId = modelId;
    clearError();
    try {
      await app.setActiveModel(modelId);
      await app.startEngine(modelId);
      toast.success($t('toast.modelReady'), {
        description: app.modelName(modelId),
      });
    } catch (caught) {
      error = normalizeError(caught);
    } finally {
      busyId = null;
    }
  }

  async function deleteModel(modelId: string): Promise<void> {
    const card = app.models.find((item) => item.model.id === modelId);
    if (!card) return;
    if (
      !window.confirm(
        $t('models.deleteBody', {
          values: { size: formatBytes(card.model.size_bytes) },
        }),
      )
    ) {
      return;
    }
    busyId = modelId;
    clearError();
    try {
      await api.deleteModel(modelId);
      await app.refreshAfterInstall();
      await refreshStorage();
      toast.success($t('toast.modelDeleted'));
    } catch (caught) {
      error = normalizeError(caught);
    } finally {
      busyId = null;
    }
  }

  async function benchmark(modelId: string): Promise<void> {
    busyId = modelId;
    clearError();
    try {
      const result = await api.runBenchmark(modelId);
      toast.success(
        $t('models.benchmarkResult', {
          values: {
            speed: formatNumber(
              tokensToWordsPerSecond(result.generation_tps),
              1,
            ),
          },
        }),
      );
    } catch (caught) {
      error = normalizeError(caught);
    } finally {
      busyId = null;
    }
  }

  async function importModel(): Promise<void> {
    if (!isTauri) return;
    importing = true;
    clearError();
    try {
      const path = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'GGUF model', extensions: ['gguf'] }],
      });
      if (typeof path !== 'string') return;
      await api.importModel(path);
      await app.refreshAfterInstall();
      await refreshStorage();
      toast.success($t('models.imported'));
    } catch (caught) {
      error = normalizeError(caught);
    } finally {
      importing = false;
    }
  }

  async function openLicense(url: string): Promise<void> {
    try {
      await openExternal(url);
    } catch (caught) {
      toast.error(normalizeError(caught).message);
    }
  }

  function clearError(): void {
    error = null;
  }
</script>

<section class="h-full overflow-y-auto">
  <div class="mx-auto max-w-5xl px-6 py-8">
    <div class="flex flex-wrap items-start justify-between gap-4">
      <div>
        <h1 class="text-2xl font-semibold tracking-tight">{$t('models.title')}</h1>
        <p class="mt-1 text-sm text-stone-500 dark:text-stone-400">
          {$t('models.subtitle')}
        </p>
      </div>
      <div class="flex flex-wrap gap-2">
        <button type="button" class="btn-secondary text-xs" disabled={importing} onclick={importModel}>
          <Import class="size-4" aria-hidden="true" />
          {$t('common.import')}
        </button>
        <button
          type="button"
          class="btn-ghost px-3 text-xs"
          title={$t('models.openFolder')}
          aria-label={$t('models.openFolder')}
          onclick={() => api.openDataDir()}
        >
          <FolderOpen class="size-4" aria-hidden="true" />
        </button>
      </div>
    </div>

    {#if error}
      <div class="mt-4">
        <ErrorCard error={error} onDismiss={clearError} />
      </div>
    {/if}

    {#if storage}
      <div class="card mt-5 flex flex-wrap items-center gap-4 p-4">
        <div class="flex size-9 items-center justify-center rounded-xl bg-stone-100 text-stone-600 dark:bg-stone-800 dark:text-stone-300">
          <HardDrive class="size-4" aria-hidden="true" />
        </div>
        <div class="min-w-0 flex-1">
          <p class="text-sm font-medium">
            {$t('models.storageUsed', {
              values: { used: formatBytes(storage.installed_bytes) },
            })}
          </p>
          <p class="mt-0.5 text-xs text-stone-500 dark:text-stone-400">
            {$t('models.storageFree', {
              values: { free: formatBytes(storage.free_disk_bytes) },
            })}
          </p>
          <p class="mt-0.5 truncate text-[0.7rem] text-stone-400 dark:text-stone-500">
            {$t('models.dataFolder', { values: { path: storage.models_dir } })}
          </p>
        </div>
      </div>
    {/if}

    <div class="mt-8">
      <div class="flex items-center justify-between">
        <h2 class="section-title">{$t('models.installedTitle')}</h2>
        <button
          type="button"
          class="btn-ghost px-2 py-1 text-xs"
          title={$t('models.openFolder')}
          onclick={() => api.openDataDir()}
        >
          <RefreshCw class="size-3.5" aria-hidden="true" />
        </button>
      </div>
      {#if installed.length === 0}
        <p class="mt-3 rounded-2xl border border-dashed border-stone-300 p-6 text-center text-sm text-stone-500 dark:border-stone-700 dark:text-stone-400">
          {$t('models.noInstalled')}
        </p>
      {:else}
        <div class="mt-3 grid gap-3 md:grid-cols-2">
          {#each installed as card (card.model.id)}
            <ModelTile
              {card}
              active={app.bootstrap?.active_model_id === card.model.id}
              busy={busyId === card.model.id}
              downloadState={app.downloadStateFor(card.model.id)}
              progress={app.downloadFor(card.model.id)}
              ondownload={() => download(card.model.id)}
              onpause={() => pause(card.model.id)}
              onresume={() => resume(card.model.id)}
              oncancel={() => cancel(card.model.id)}
              onuse={() => useModel(card.model.id)}
              ondelete={() => deleteModel(card.model.id)}
              onbenchmark={() => benchmark(card.model.id)}
              onlicense={() => openLicense(card.model.license_url)}
            />
          {/each}
        </div>
      {/if}
    </div>

    <div class="mt-8">
      <div class="flex items-center justify-between gap-3">
        <h2 class="section-title">{$t('models.availableTitle')}</h2>
        <button
          type="button"
          class="btn-ghost px-2 py-1 text-xs"
          onclick={() => (showMore = !showMore)}
        >
          {$t('models.moreModels')}
          <ChevronDown class="size-3.5 transition-transform {showMore ? 'rotate-180' : ''}" aria-hidden="true" />
        </button>
      </div>

      {#if showMore}
        <div class="mt-3 flex flex-wrap items-center gap-3 rounded-xl border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-900 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-100">
          <p class="flex-1">{$t('models.experimentalBody')}</p>
          <button
            type="button"
            class="rounded-lg border border-amber-300 px-2 py-1 font-medium hover:bg-amber-100 dark:border-amber-800 dark:hover:bg-amber-900/40"
            onclick={() =>
              app.updateSettings({
                include_experimental: !(app.settings?.include_experimental ?? false),
              })}
          >
            {$t(
              app.settings?.include_experimental
                ? 'models.hideExperimental'
                : 'models.showExperimental',
            )}
          </button>
        </div>
      {/if}

      {#if visible.length === 0}
        <p class="mt-3 rounded-2xl border border-dashed border-stone-300 p-6 text-center text-sm text-stone-500 dark:border-stone-700 dark:text-stone-400">
          {$t('models.noInstalled')}
        </p>
      {:else}
        <div class="mt-3 grid gap-3 md:grid-cols-2">
          {#each visible as card (card.model.id)}
            <ModelTile
              {card}
              busy={busyId === card.model.id}
              downloadState={app.downloadStateFor(card.model.id)}
              progress={app.downloadFor(card.model.id)}
              ondownload={() => download(card.model.id)}
              onpause={() => pause(card.model.id)}
              onresume={() => resume(card.model.id)}
              oncancel={() => cancel(card.model.id)}
              onuse={() => useModel(card.model.id)}
              ondelete={() => deleteModel(card.model.id)}
              onbenchmark={() => benchmark(card.model.id)}
              onlicense={() => openLicense(card.model.license_url)}
            />
          {/each}
        </div>
      {/if}
    </div>

    <div class="mt-10 flex items-center justify-center gap-2 text-xs text-stone-400 dark:text-stone-500">
      <Download class="size-3.5" aria-hidden="true" />
      {$t('app.offline')}
    </div>
  </div>
</section>
