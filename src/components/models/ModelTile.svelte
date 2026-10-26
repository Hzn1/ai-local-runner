<script lang="ts">
  import { t } from 'svelte-i18n';
  import {
    BadgeCheck,
    Check,
    Cpu,
    Download,
    ExternalLink,
    Gauge,
    HardDrive,
    Pause,
    Play,
    Trash2,
    X,
  } from '@lucide/svelte';
  import type { DownloadProgress } from '../../lib/bindings/DownloadProgress';
  import type { ModelCard } from '../../lib/bindings/ModelCard';
  import { formatBytes, formatSpeed, percent } from '../../lib/format';
  import ProgressBar from '../ui/ProgressBar.svelte';

  let {
    card,
    progress = null,
    downloadState = 'idle',
    active = false,
    busy = false,
    ondownload,
    onpause,
    onresume,
    oncancel,
    onuse,
    ondelete,
    onbenchmark,
    onlicense,
  }: {
    card: ModelCard;
    progress?: DownloadProgress | null;
    downloadState?: string;
    active?: boolean;
    busy?: boolean;
    ondownload: () => void;
    onpause: () => void;
    onresume: () => void;
    oncancel: () => void;
    onuse: () => void;
    ondelete: () => void;
    onbenchmark: () => void;
    onlicense: () => void;
  } = $props();

  const downloadable = $derived(
    card.model.validation === 'validated' && Boolean(card.model.url),
  );
  const downloading = $derived(
    ['queued', 'downloading', 'paused', 'verifying'].includes(downloadState),
  );
  const downloadPercent = $derived(
    progress ? percent(progress.bytes_done, progress.bytes_total) : 0,
  );
  const tierLabel = $derived(
    card.model.tier === 'fast'
      ? $t('models.fastBadge')
      : card.model.tier === 'balanced'
        ? $t('models.balancedBadge')
        : card.model.tier === 'smart'
          ? $t('models.smartBadge')
          : $t('models.advancedBadge'),
  );
  const reason = $derived.by(() => {
    if (card.installed) return null;
    if (!downloadable) {
      return $t(
        card.model.experimental
          ? 'models.unsupported'
          : 'onboarding.model.notDownloadable',
      );
    }
    if (card.unavailable_reason === 'needs_more_ram') {
      return $t('models.needsRam', {
        values: { ram: formatBytes(card.model.min_ram_bytes) },
      });
    }
    if (card.unavailable_reason === 'needs_more_disk') {
      return $t('onboarding.model.needsDisk', {
        values: { disk: formatBytes(card.model.min_disk_bytes) },
      });
    }
    return null;
  });
</script>

<article
  class="card flex flex-col p-4"
  class:ring-2={active}
  class:ring-brand-500={active}
>
  <div class="flex items-start justify-between gap-3">
    <div class="min-w-0">
      <div class="flex flex-wrap items-center gap-1.5">
        <h3 class="text-base font-semibold">{card.model.display_name}</h3>
        {#if active}
          <span class="badge bg-brand-100 text-brand-800 dark:bg-brand-900/60 dark:text-brand-200">
            <Check class="size-3" aria-hidden="true" />
            {$t('common.active')}
          </span>
        {:else if card.recommended}
          <span class="badge bg-brand-100 text-brand-800 dark:bg-brand-900/60 dark:text-brand-200">
            {$t('models.recommendedBadge')}
          </span>
        {/if}
        {#if card.model.experimental}
          <span class="badge bg-amber-100 text-amber-800 dark:bg-amber-900/50 dark:text-amber-200">
            {$t('models.experimental')}
          </span>
        {/if}
      </div>
      <p class="mt-1 text-xs leading-relaxed text-stone-500 dark:text-stone-400">
        {card.model.description.startsWith('modelDescriptions.')
          ? $t(card.model.description)
          : card.model.description}
      </p>
    </div>
    <span class="badge shrink-0 bg-stone-100 text-stone-600 dark:bg-stone-800 dark:text-stone-300">
      {tierLabel}
    </span>
  </div>

  <dl class="mt-3 grid grid-cols-3 gap-2 text-[0.7rem]">
    <div class="rounded-lg bg-stone-50 px-2 py-1.5 dark:bg-stone-950/60">
      <dt class="text-stone-400 dark:text-stone-500">{$t('common.size')}</dt>
      <dd class="mt-0.5 font-medium text-stone-700 dark:text-stone-200">{formatBytes(card.model.size_bytes)}</dd>
    </div>
    <div class="rounded-lg bg-stone-50 px-2 py-1.5 dark:bg-stone-950/60">
      <dt class="flex items-center gap-1 text-stone-400 dark:text-stone-500">
        <Cpu class="size-2.5" aria-hidden="true" />RAM
      </dt>
      <dd class="mt-0.5 font-medium text-stone-700 dark:text-stone-200">{formatBytes(card.model.min_ram_bytes)}</dd>
    </div>
    <div class="rounded-lg bg-stone-50 px-2 py-1.5 dark:bg-stone-950/60">
      <dt class="text-stone-400 dark:text-stone-500">{$t('models.license')}</dt>
      <dd class="mt-0.5 truncate font-medium text-stone-700 dark:text-stone-200">{card.model.license}</dd>
    </div>
  </dl>

  {#if downloading}
    <div class="mt-3">
      <div class="mb-1 flex items-center justify-between text-[0.7rem] text-stone-500 dark:text-stone-400">
        <span>
          {$t(`download.${downloadState}`)}
          {#if progress}
            · {formatSpeed(progress.speed_bps)}
          {/if}
        </span>
        <span>
          {formatBytes(progress?.bytes_done ?? 0)} / {formatBytes(
            progress?.bytes_total ?? card.model.size_bytes,
          )}
        </span>
      </div>
      <ProgressBar
        value={downloadPercent}
        indeterminate={downloadState === 'queued' || downloadState === 'verifying'}
        label={card.model.display_name}
      />
      <div class="mt-2 flex gap-2">
        {#if downloadState === 'paused'}
          <button type="button" class="btn-secondary px-3 py-1.5 text-xs" onclick={onresume}>
            <Play class="size-3" aria-hidden="true" />
            {$t('models.resume')}
          </button>
        {:else}
          <button type="button" class="btn-secondary px-3 py-1.5 text-xs" onclick={onpause}>
            <Pause class="size-3" aria-hidden="true" />
            {$t('models.pause')}
          </button>
        {/if}
        <button type="button" class="btn-ghost px-3 py-1.5 text-xs" onclick={oncancel}>
          <X class="size-3" aria-hidden="true" />
          {$t('models.cancel')}
        </button>
      </div>
    </div>
  {:else if reason}
    <p class="mt-3 rounded-lg bg-amber-50 px-2.5 py-2 text-[0.7rem] text-amber-800 dark:bg-amber-950/40 dark:text-amber-200">
      {reason}
    </p>
  {/if}

  <div class="mt-auto flex flex-wrap items-center gap-2 pt-3">
    {#if active}
      <span class="inline-flex items-center gap-1.5 text-xs font-medium text-brand-700 dark:text-brand-300">
        <BadgeCheck class="size-4" aria-hidden="true" />
        {$t('models.activeModel')}
      </span>
    {:else if card.installed}
      <button type="button" class="btn-primary px-3 py-1.5 text-xs" disabled={busy} onclick={onuse}>
        {$t('models.setActive')}
      </button>
    {:else if downloadable && !reason}
      <button type="button" class="btn-primary px-3 py-1.5 text-xs" disabled={busy} onclick={ondownload}>
        <Download class="size-3.5" aria-hidden="true" />
        {$t('common.download')}
      </button>
    {/if}

    {#if card.installed}
      <button
        type="button"
        class="btn-ghost px-2 py-1.5 text-xs"
        disabled={busy}
        title={$t('models.benchmark')}
        onclick={onbenchmark}
      >
        <Gauge class="size-3.5" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="btn-ghost px-2 py-1.5 text-xs text-red-700 hover:bg-red-50 dark:text-red-300 dark:hover:bg-red-950/40"
        disabled={busy}
        title={$t('common.delete')}
        aria-label={$t('common.delete')}
        onclick={ondelete}
      >
        <Trash2 class="size-3.5" aria-hidden="true" />
      </button>
    {/if}

    <button
      type="button"
      class="ml-auto inline-flex items-center gap-1 text-[0.7rem] text-stone-400 hover:text-stone-700 dark:hover:text-stone-200"
      onclick={onlicense}
      title={$t('models.license')}
    >
      <HardDrive class="size-3" aria-hidden="true" />
      <ExternalLink class="size-3" aria-hidden="true" />
    </button>
  </div>
</article>
