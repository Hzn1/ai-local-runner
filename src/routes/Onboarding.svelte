<script lang="ts">
  import { t } from 'svelte-i18n';
  import { toast } from 'svelte-sonner';
  import {
    ArrowLeft,
    ArrowRight,
    Check,
    Cpu,
    Download,
    Gauge,
    HardDrive,
    Languages,
    MemoryStick,
    MessageSquare,
    Pause,
    Play,
    Shield,
    Sparkles,
    X,
  } from '@lucide/svelte';
  import { app } from '../stores/app.svelte';
  import { chat } from '../stores/chat.svelte';
  import { router } from '../lib/router.svelte';
  import { api } from '../lib/tauri';
  import { normalizeError } from '../lib/errors';
  import {
    formatBytes,
    formatDuration,
    formatNumber,
    formatSpeed,
    percent,
    tokensToWordsPerSecond,
  } from '../lib/format';
  import type {
    BenchmarkResult,
    ErrorPayload,
    ModelCard,
  } from '../lib/bindings/index';
  import ProgressBar from '../components/ui/ProgressBar.svelte';
  import ErrorCard from '../components/ui/ErrorCard.svelte';

  let step = $state(0);
  let selectedId = $state<string | null>(null);
  let showMore = $state(false);
  let busy = $state(false);
  let downloadError = $state<ErrorPayload | null>(null);
  let benchmark = $state<BenchmarkResult | null>(null);
  let benchmarkBusy = $state(false);
  let finishing = $state(false);

  const totalSteps = 5;
  const hardware = $derived(app.hardware);
  const settings = $derived(app.settings);

  const visibleModels = $derived(
    app.models
      .filter((card) => {
        if (card.model.experimental) {
          return showMore && (settings?.include_experimental ?? false);
        }
        if (card.model.params_b >= 20 && !showMore) return false;
        return true;
      })
      .sort((a, b) => a.model.params_b - b.model.params_b),
  );

  const selectedCard = $derived(
    app.models.find((card) => card.model.id === selectedId) ?? null,
  );
  const downloadProgress = $derived(
    selectedId ? app.downloads[selectedId] : null,
  );
  const downloadState = $derived(
    selectedId ? app.downloadStateFor(selectedId) : 'idle',
  );
  const downloadPercent = $derived(
    downloadProgress
      ? percent(downloadProgress.bytes_done, downloadProgress.bytes_total)
      : 0,
  );

  $effect(() => {
    if (!selectedId) {
      selectedId =
        app.bootstrap?.recommended_model_id ??
        app.models.find((card) => card.installed)?.model.id ??
        app.models.find((card) => card.compatible)?.model.id ??
        null;
    }
  });

  $effect(() => {
    if (step === 3 && selectedId && downloadState === 'installed' && !finishing) {
      void completeSetup();
    }
  });

  function tierLabel(card: ModelCard): string {
    const key =
      card.model.tier === 'fast'
        ? 'models.fastBadge'
        : card.model.tier === 'balanced'
          ? 'models.balancedBadge'
          : card.model.tier === 'smart'
            ? 'models.smartBadge'
            : 'models.advancedBadge';
    return $t(key);
  }

  function unavailable(card: ModelCard): string | null {
    if (card.installed) return null;
    if (!card.compatible) {
      if (card.unavailable_reason === 'needs_more_ram') {
        return $t('onboarding.model.needsRam', {
          values: { ram: formatBytes(card.model.min_ram_bytes) },
        });
      }
      if (card.unavailable_reason === 'needs_more_disk') {
        return $t('onboarding.model.needsDisk', {
          values: { disk: formatBytes(card.model.min_disk_bytes) },
        });
      }
      return $t('onboarding.model.notDownloadable');
    }
    return null;
  }

  async function rescan(): Promise<void> {
    busy = true;
    try {
      const profile = await api.scanHardware();
      if (app.bootstrap) app.bootstrap.hardware = profile;
    } catch (error) {
      toast.error(normalizeError(error).message);
    } finally {
      busy = false;
    }
  }

  async function chooseModel(): Promise<void> {
    if (!selectedCard) return;
    if (selectedCard.installed) {
      await activateAndFinish();
      return;
    }
    step = 3;
    downloadError = null;
    await startDownload();
  }

  async function startDownload(): Promise<void> {
    if (!selectedId) return;
    try {
      await api.downloadModel(selectedId);
    } catch (error) {
      downloadError = normalizeError(error);
    }
  }

  async function pauseDownload(): Promise<void> {
    if (!selectedId) return;
    await api.pauseDownload(selectedId);
  }

  async function resumeDownload(): Promise<void> {
    if (!selectedId) return;
    downloadError = null;
    await api.resumeDownload(selectedId);
  }

  async function cancelDownload(): Promise<void> {
    if (!selectedId) return;
    await api.cancelDownload(selectedId);
    step = 2;
  }

  async function completeSetup(): Promise<void> {
    if (!selectedId || finishing) return;
    finishing = true;
    try {
      await app.updateSettings({
        active_model_id: selectedId,
        onboarding_complete: true,
      });
      await app.refreshAfterInstall();
      step = 4;
      void app.startEngine(selectedId).catch(() => {});
    } catch (error) {
      downloadError = normalizeError(error);
    } finally {
      finishing = false;
    }
  }

  async function activateAndFinish(): Promise<void> {
    if (!selectedCard) return;
    try {
      await app.setActiveModel(selectedCard.model.id);
      step = 4;
      void app.startEngine(selectedCard.model.id).catch(() => {});
    } catch (error) {
      downloadError = normalizeError(error);
    }
  }

  async function runBenchmark(): Promise<void> {
    if (!selectedId) return;
    benchmarkBusy = true;
    try {
      benchmark = await api.runBenchmark(selectedId);
    } catch (error) {
      toast.error(normalizeError(error).message);
    } finally {
      benchmarkBusy = false;
    }
  }

  async function startChatting(prompt?: string): Promise<void> {
    await chat.newChat();
    router.navigate('chat');
    if (prompt) await chat.send(prompt);
  }

  async function changeLanguage(language: 'en' | 'pt-BR'): Promise<void> {
    await app.setLanguage(language);
  }
</script>

<div class="min-h-full overflow-y-auto bg-gradient-to-b from-brand-50 to-stone-50 dark:from-stone-950 dark:to-stone-900">
  <div class="mx-auto flex min-h-full max-w-3xl flex-col px-6 py-8">
    <header class="flex items-center justify-between">
      <div class="flex items-center gap-2.5">
        <div
          class="flex size-9 items-center justify-center rounded-xl bg-brand-700 text-white dark:bg-brand-500 dark:text-brand-950"
        >
          <MessageSquare class="size-5" aria-hidden="true" />
        </div>
        <div>
          <p class="text-sm font-semibold tracking-tight">{$t('app.name')}</p>
          <p class="text-[0.7rem] text-stone-500 dark:text-stone-400">
            {$t('app.tagline')}
          </p>
        </div>
      </div>
      <div class="flex items-center gap-1 rounded-xl border border-stone-200 bg-white p-1 dark:border-stone-800 dark:bg-stone-900">
        <Languages class="ml-1.5 size-3.5 text-stone-400" aria-hidden="true" />
        <button
          type="button"
          class="rounded-lg px-2 py-1 text-[0.7rem] font-medium {settings?.language ===
          'en'
            ? 'bg-stone-200 text-stone-900 dark:bg-stone-700 dark:text-stone-50'
            : 'text-stone-500 hover:bg-stone-100 dark:text-stone-400 dark:hover:bg-stone-800'}"
          onclick={() => changeLanguage('en')}
        >
          EN
        </button>
        <button
          type="button"
          class="rounded-lg px-2 py-1 text-[0.7rem] font-medium {settings?.language ===
          'pt-BR'
            ? 'bg-stone-200 text-stone-900 dark:bg-stone-700 dark:text-stone-50'
            : 'text-stone-500 hover:bg-stone-100 dark:text-stone-400 dark:hover:bg-stone-800'}"
          onclick={() => changeLanguage('pt-BR')}
        >
          PT
        </button>
      </div>
    </header>

    <div class="mt-6 flex items-center gap-2" aria-hidden="true">
      {#each Array(totalSteps) as _, index (index)}
        <span
          class="h-1.5 flex-1 rounded-full transition-colors {index <= step
            ? 'bg-brand-600 dark:bg-brand-400'
            : 'bg-stone-200 dark:bg-stone-800'}"
        ></span>
      {/each}
    </div>
    <p class="mt-2 text-[0.7rem] font-medium tracking-wide text-stone-500 uppercase dark:text-stone-400">
      {$t('onboarding.step', {
        values: { current: step + 1, total: totalSteps },
      })}
    </p>

    <main class="mt-6 flex flex-1 flex-col">
      {#if step === 0}
        <div class="py-6">
          <h1 class="max-w-xl text-4xl leading-tight font-semibold tracking-tight text-stone-900 dark:text-stone-50">
            {$t('onboarding.welcome.title')}
          </h1>
          <p class="mt-3 max-w-xl text-base leading-relaxed text-stone-600 dark:text-stone-300">
            {$t('onboarding.welcome.subtitle')}
          </p>

          <div class="mt-8 grid gap-3 sm:grid-cols-3">
            {#each [
              { icon: Shield, title: $t('onboarding.welcome.feature1Title'), body: $t('onboarding.welcome.feature1Body') },
              { icon: Cpu, title: $t('onboarding.welcome.feature2Title'), body: $t('onboarding.welcome.feature2Body') },
              { icon: Gauge, title: $t('onboarding.welcome.feature3Title'), body: $t('onboarding.welcome.feature3Body') },
            ] as feature (feature.title)}
              <div class="card p-4">
                <feature.icon class="size-5 text-brand-700 dark:text-brand-300" aria-hidden="true" />
                <p class="mt-2 text-sm font-semibold">{feature.title}</p>
                <p class="mt-1 text-xs leading-relaxed text-stone-500 dark:text-stone-400">
                  {feature.body}
                </p>
              </div>
            {/each}
          </div>

          <div class="mt-8 flex flex-wrap items-center gap-3">
            <button type="button" class="btn-primary px-6" onclick={() => (step = 1)}>
              {$t('onboarding.welcome.getStarted')}
              <ArrowRight class="size-4" aria-hidden="true" />
            </button>
            <p class="text-xs text-stone-500 dark:text-stone-400">
              {$t('app.privacy')}
            </p>
          </div>
        </div>
      {:else if step === 1}
        <div class="py-4">
          <h1 class="text-3xl font-semibold tracking-tight">{$t('onboarding.hardware.title')}</h1>
          <p class="mt-2 text-sm text-stone-600 dark:text-stone-300">
            {$t('onboarding.hardware.subtitle')}
          </p>

          {#if hardware}
            <div class="mt-6 grid gap-3 sm:grid-cols-2">
              <div class="card flex items-center gap-3 p-4">
                <MemoryStick class="size-5 text-brand-700 dark:text-brand-300" aria-hidden="true" />
                <div>
                  <p class="text-xs text-stone-500 dark:text-stone-400">{$t('onboarding.hardware.ram')}</p>
                  <p class="text-lg font-semibold">{formatBytes(hardware.total_ram_bytes)}</p>
                </div>
              </div>
              <div class="card flex items-center gap-3 p-4">
                <Cpu class="size-5 text-brand-700 dark:text-brand-300" aria-hidden="true" />
                <div class="min-w-0">
                  <p class="text-xs text-stone-500 dark:text-stone-400">{$t('onboarding.hardware.cores')}</p>
                  <p class="text-lg font-semibold">{hardware.physical_cores}</p>
                  <p class="truncate text-[0.7rem] text-stone-500 dark:text-stone-400">{hardware.cpu_brand}</p>
                </div>
              </div>
              <div class="card flex items-center gap-3 p-4">
                <HardDrive class="size-5 text-brand-700 dark:text-brand-300" aria-hidden="true" />
                <div>
                  <p class="text-xs text-stone-500 dark:text-stone-400">{$t('onboarding.hardware.disk')}</p>
                  <p class="text-lg font-semibold">{formatBytes(hardware.free_disk_bytes)}</p>
                </div>
              </div>
              <div class="card flex items-center gap-3 p-4">
                <Sparkles class="size-5 text-brand-700 dark:text-brand-300" aria-hidden="true" />
                <div>
                  <p class="text-xs text-stone-500 dark:text-stone-400">{$t('hardware.tier')}</p>
                  <p class="text-lg font-semibold capitalize">{hardware.tier}</p>
                </div>
              </div>
            </div>

            <div class="mt-4 rounded-2xl border border-brand-200 bg-brand-50 p-4 dark:border-brand-800 dark:bg-brand-950/40">
              <p class="text-sm font-semibold text-brand-900 dark:text-brand-100">
                {$t('onboarding.hardware.recommendation', {
                  values: {
                    model: app.bootstrap?.recommended_model_id
                      ? app.modelName(app.bootstrap.recommended_model_id)
                      : '—',
                  },
                })}
              </p>
              <p class="mt-1 text-xs leading-relaxed text-brand-800/80 dark:text-brand-200/80">
                {$t('onboarding.hardware.recommendationBody')}
              </p>
            </div>

            {#if hardware.minimum_mode}
              <div class="mt-3 rounded-2xl border border-amber-200 bg-amber-50 p-4 dark:border-amber-800 dark:bg-amber-950/40">
                <p class="text-sm font-semibold text-amber-900 dark:text-amber-100">
                  {$t('hardware.minimum')}
                </p>
                <p class="mt-1 text-xs leading-relaxed text-amber-800 dark:text-amber-200">
                  {$t('onboarding.hardware.minimumWarning')}
                </p>
              </div>
            {/if}

            {#if hardware.notes.length}
              <div class="mt-4">
                <p class="text-xs font-semibold tracking-wide text-stone-500 uppercase dark:text-stone-400">
                  {$t('onboarding.hardware.notes')}
                </p>
                <ul class="mt-2 space-y-1">
                  {#each hardware.notes as note (note)}
                    <li class="flex gap-2 text-xs text-stone-500 dark:text-stone-400">
                      <Check class="mt-0.5 size-3.5 shrink-0 text-brand-600 dark:text-brand-400" aria-hidden="true" />
                      {note}
                    </li>
                  {/each}
                </ul>
              </div>
            {/if}
          {:else}
            <p class="mt-6 text-sm">{$t('common.loading')}</p>
          {/if}
        </div>

        <div class="mt-auto flex items-center justify-between pt-6">
          <div class="flex gap-2">
            <button type="button" class="btn-ghost" onclick={() => (step = 0)}>
              <ArrowLeft class="size-4" aria-hidden="true" />
              {$t('common.back')}
            </button>
            <button type="button" class="btn-ghost" disabled={busy} onclick={rescan}>
              {$t('onboarding.hardware.rescan')}
            </button>
          </div>
          <button type="button" class="btn-primary" onclick={() => (step = 2)}>
            {$t('onboarding.hardware.continue')}
            <ArrowRight class="size-4" aria-hidden="true" />
          </button>
        </div>
      {:else if step === 2}
        <div class="py-4">
          <h1 class="text-3xl font-semibold tracking-tight">{$t('onboarding.model.title')}</h1>
          <p class="mt-2 text-sm text-stone-600 dark:text-stone-300">
            {$t('onboarding.model.subtitle')}
          </p>

          <div class="mt-5 space-y-3">
            {#each visibleModels as card (card.model.id)}
              {@const reason = unavailable(card)}
              <button
                type="button"
                class="card w-full p-4 text-left transition-colors {selectedId ===
                card.model.id
                  ? 'border-brand-500 ring-2 ring-brand-500/20 dark:border-brand-400'
                  : ''} {reason
                  ? 'opacity-60'
                  : 'hover:border-brand-300 dark:hover:border-brand-700'}"
                disabled={Boolean(reason)}
                onclick={() => (selectedId = card.model.id)}
              >
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <div class="flex flex-wrap items-center gap-2">
                      <span class="text-base font-semibold">{card.model.display_name}</span>
                      <span class="badge bg-stone-100 text-stone-600 dark:bg-stone-800 dark:text-stone-300">
                        {tierLabel(card)}
                      </span>
                      {#if card.recommended}
                        <span class="badge bg-brand-100 text-brand-800 dark:bg-brand-900/60 dark:text-brand-200">
                          {$t('onboarding.model.recommendedForYou')}
                        </span>
                      {/if}
                      {#if card.model.experimental}
                        <span class="badge bg-amber-100 text-amber-800 dark:bg-amber-900/50 dark:text-amber-200">
                          {$t('models.experimental')}
                        </span>
                      {/if}
                      {#if card.installed}
                        <span class="badge bg-emerald-100 text-emerald-800 dark:bg-emerald-900/50 dark:text-emerald-200">
                          {$t('common.installed')}
                        </span>
                      {/if}
                    </div>
                    <p class="mt-1 text-xs leading-relaxed text-stone-500 dark:text-stone-400">
                      {card.model.description}
                    </p>
                    {#if reason}
                      <p class="mt-1 text-xs font-medium text-amber-700 dark:text-amber-300">{reason}</p>
                    {/if}
                  </div>
                  <div class="text-right">
                    <p class="text-sm font-semibold">{formatBytes(card.model.size_bytes)}</p>
                    <p class="mt-0.5 text-[0.7rem] text-stone-500 dark:text-stone-400">
                      {$t('onboarding.model.needsRam', {
                        values: { ram: formatBytes(card.model.min_ram_bytes) },
                      })}
                    </p>
                  </div>
                </div>
                {#if selectedId === card.model.id}
                  <div class="mt-2 flex items-center gap-1.5 text-xs font-medium text-brand-700 dark:text-brand-300">
                    <Check class="size-3.5" aria-hidden="true" />
                    {$t('onboarding.model.chosen')}
                  </div>
                {/if}
              </button>
            {/each}
          </div>

          <button
            type="button"
            class="btn-ghost mt-3 text-xs"
            onclick={() => (showMore = !showMore)}
          >
            {$t(showMore ? 'onboarding.model.lessModels' : 'onboarding.model.moreModels')}
          </button>

          {#if showMore && !settings?.include_experimental}
            <button
              type="button"
              class="mt-2 ml-3 text-xs font-medium text-brand-700 underline underline-offset-2 dark:text-brand-300"
              onclick={() => app.updateSettings({ include_experimental: true })}
            >
              {$t('models.showExperimental')}
            </button>
          {/if}
        </div>

        <div class="mt-auto flex items-center justify-between pt-6">
          <button type="button" class="btn-ghost" onclick={() => (step = 1)}>
            <ArrowLeft class="size-4" aria-hidden="true" />
            {$t('common.back')}
          </button>
          <button
            type="button"
            class="btn-primary"
            disabled={!selectedCard || (!selectedCard.compatible && !selectedCard.installed)}
            onclick={chooseModel}
          >
            {$t('common.continue')}
            <ArrowRight class="size-4" aria-hidden="true" />
          </button>
        </div>
      {:else if step === 3}
        <div class="flex flex-1 flex-col justify-center py-6">
          <div class="mx-auto w-full max-w-xl text-center">
            <div
              class="mx-auto flex size-16 items-center justify-center rounded-2xl bg-brand-100 text-brand-800 dark:bg-brand-900/50 dark:text-brand-200"
            >
              {#if downloadState === 'installed'}
                <Check class="size-8" aria-hidden="true" />
              {:else}
                <Download class="size-8" aria-hidden="true" />
              {/if}
            </div>
            <h1 class="mt-4 text-2xl font-semibold tracking-tight">
              {selectedCard?.model.display_name}
            </h1>
            <p class="mt-2 text-sm text-stone-500 dark:text-stone-400">
              {$t('onboarding.download.subtitle')}
            </p>

            {#if downloadError}
              <div class="mt-5 text-left">
                <ErrorCard
                  error={downloadError}
                  onAction={(action) => {
                    if (action === 'retry') resumeDownload();
                    if (action === 'free_disk_space') router.navigate('models');
                  }}
                  onDismiss={() => (downloadError = null)}
                />
              </div>
            {:else if downloadState === 'failed'}
              <div class="mt-5 rounded-2xl border border-red-200 bg-red-50 p-4 text-left dark:border-red-900 dark:bg-red-950/40">
                <p class="text-sm font-semibold text-red-900 dark:text-red-100">
                  {$t('onboarding.download.failedTitle')}
                </p>
                <p class="mt-1 text-xs text-red-800 dark:text-red-200">
                  {$t('onboarding.download.failedBody')}
                </p>
                <button type="button" class="btn-secondary mt-3" onclick={resumeDownload}>
                  {$t('common.retry')}
                </button>
              </div>
            {:else if downloadState === 'verifying'}
              <div class="mt-6 text-left">
                <p class="text-sm font-semibold">{$t('onboarding.download.verifyTitle')}</p>
                <p class="mt-1 text-xs text-stone-500 dark:text-stone-400">
                  {$t('onboarding.download.verifyBody')}
                </p>
                <div class="mt-3"><ProgressBar value={100} indeterminate /></div>
              </div>
            {:else}
              <div class="mt-6 text-left">
                <div class="mb-2 flex items-end justify-between text-xs text-stone-500 dark:text-stone-400">
                  <span>
                    {$t('onboarding.download.downloaded', {
                      values: {
                        done: formatBytes(downloadProgress?.bytes_done ?? 0),
                        total: formatBytes(
                          downloadProgress?.bytes_total ??
                            selectedCard?.model.size_bytes ??
                            0,
                        ),
                      },
                    })}
                  </span>
                  <span>
                    {formatSpeed(downloadProgress?.speed_bps ?? 0)}
                    {#if downloadProgress?.eta_secs}
                      · {formatDuration(downloadProgress.eta_secs)}
                    {/if}
                  </span>
                </div>
                <ProgressBar
                  value={downloadPercent}
                  indeterminate={downloadState === 'queued' || downloadState === 'idle'}
                  label={$t('onboarding.download.title')}
                />
                <div class="mt-3 flex items-center justify-between">
                  <p class="text-xs text-stone-500 dark:text-stone-400">
                    {$t(
                      downloadState === 'paused'
                        ? 'onboarding.download.paused'
                        : 'onboarding.download.keepUsing',
                    )}
                  </p>
                  <div class="flex gap-2">
                    {#if downloadState === 'paused'}
                      <button type="button" class="btn-secondary px-3 py-1.5 text-xs" onclick={resumeDownload}>
                        <Play class="size-3.5" aria-hidden="true" />
                        {$t('onboarding.download.resume')}
                      </button>
                    {:else if downloadState !== 'installed'}
                      <button type="button" class="btn-secondary px-3 py-1.5 text-xs" onclick={pauseDownload}>
                        <Pause class="size-3.5" aria-hidden="true" />
                        {$t('onboarding.download.pause')}
                      </button>
                    {/if}
                    <button type="button" class="btn-ghost px-3 py-1.5 text-xs" onclick={cancelDownload}>
                      <X class="size-3.5" aria-hidden="true" />
                      {$t('onboarding.download.cancel')}
                    </button>
                  </div>
                </div>
              </div>
            {/if}
          </div>
        </div>
      {:else}
        <div class="flex flex-1 flex-col justify-center py-6 text-center">
          <div
            class="mx-auto flex size-16 items-center justify-center rounded-2xl bg-emerald-100 text-emerald-700 dark:bg-emerald-900/50 dark:text-emerald-200"
          >
            <Check class="size-8" aria-hidden="true" />
          </div>
          <h1 class="mt-4 text-3xl font-semibold tracking-tight">{$t('onboarding.ready.title')}</h1>
          <p class="mt-2 text-sm text-stone-500 dark:text-stone-400">
            {$t('onboarding.ready.subtitle')}
          </p>

          <div class="mx-auto mt-6 grid w-full max-w-2xl gap-2 sm:grid-cols-3">
            {#each [
              $t('onboarding.ready.suggestion1'),
              $t('onboarding.ready.suggestion2'),
              $t('onboarding.ready.suggestion3'),
            ] as suggestion (suggestion)}
              <button
                type="button"
                class="rounded-xl border border-stone-200 bg-white px-3 py-3 text-left text-xs leading-relaxed text-stone-600 shadow-sm hover:border-brand-300 hover:bg-brand-50/50 dark:border-stone-800 dark:bg-stone-900 dark:text-stone-300 dark:hover:border-brand-700"
                onclick={() => startChatting(suggestion)}
              >
                {suggestion}
              </button>
            {/each}
          </div>

          <div class="mx-auto mt-6 w-full max-w-md rounded-2xl border border-stone-200 bg-white p-4 text-left dark:border-stone-800 dark:bg-stone-900">
            <div class="flex items-center gap-2">
              <Gauge class="size-4 text-brand-700 dark:text-brand-300" aria-hidden="true" />
              <p class="text-sm font-semibold">{$t('onboarding.ready.speedTitle')}</p>
            </div>
            {#if benchmark}
              <p class="mt-1 text-xs text-stone-500 dark:text-stone-400">
                {$t('onboarding.ready.speedBody', {
                  values: {
                    speed: formatNumber(
                      tokensToWordsPerSecond(benchmark.generation_tps),
                      1,
                    ),
                  },
                })}
              </p>
            {:else}
              <button
                type="button"
                class="btn-secondary mt-2 w-full text-xs"
                disabled={benchmarkBusy}
                onclick={runBenchmark}
              >
                <Gauge class="size-3.5" aria-hidden="true" />
                {$t('onboarding.ready.runBenchmark')}
              </button>
            {/if}
          </div>

          <button
            type="button"
            class="btn-primary mx-auto mt-6 px-6"
            onclick={() => startChatting()}
          >
            {$t('onboarding.ready.startChat')}
            <ArrowRight class="size-4" aria-hidden="true" />
          </button>
        </div>
      {/if}
    </main>
  </div>
</div>
