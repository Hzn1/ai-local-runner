<script lang="ts">
  import { t } from 'svelte-i18n';
  import { CircleAlert, ChevronDown, X } from '@lucide/svelte';
  import type { ErrorPayload, RecoveryAction } from '../../lib/bindings/index';

  let {
    error,
    onAction = () => {},
    onDismiss = undefined,
    compact = false,
  }: {
    error: ErrorPayload;
    onAction?: (action: RecoveryAction) => void;
    onDismiss?: () => void;
    compact?: boolean;
  } = $props();

  let showDetail = $state(false);
  const message = $derived(
    $t(`errors.codes.${error.code}`) === `errors.codes.${error.code}`
      ? $t('errors.unknown')
      : $t(`errors.codes.${error.code}`),
  );
</script>

<div
  class="rounded-2xl border border-red-200 bg-red-50 p-4 text-red-950 dark:border-red-900/70 dark:bg-red-950/40 dark:text-red-100"
  role="alert"
>
  <div class="flex items-start gap-3">
    <CircleAlert class="mt-0.5 size-5 shrink-0" aria-hidden="true" />
    <div class="min-w-0 flex-1">
      <p class="text-sm font-semibold">{message}</p>
      {#if !compact && error.detail}
        <button
          type="button"
          class="mt-1 inline-flex items-center gap-1 text-xs font-medium text-red-800/80 hover:text-red-950 dark:text-red-200/80 dark:hover:text-white"
          onclick={() => (showDetail = !showDetail)}
          aria-expanded={showDetail}
        >
          {$t(showDetail ? 'errors.hideDetail' : 'errors.showDetail')}
          <ChevronDown
            class="size-3.5 transition-transform {showDetail ? 'rotate-180' : ''}"
            aria-hidden="true"
          />
        </button>
        {#if showDetail}
          <pre
            class="mt-2 max-h-36 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-red-100/70 p-2 font-mono text-[0.7rem] leading-relaxed text-red-900 dark:bg-red-950/60 dark:text-red-200">{error.detail}</pre>
        {/if}
      {/if}
      {#if error.recovery.length}
        <div class="mt-3 flex flex-wrap gap-2">
          {#each error.recovery as action (action)}
            <button
              type="button"
              class="rounded-lg border border-red-300 bg-white px-3 py-1.5 text-xs font-medium text-red-900 hover:bg-red-100 dark:border-red-800 dark:bg-red-950/50 dark:text-red-100 dark:hover:bg-red-900/60"
              onclick={() => onAction(action)}
            >
              {$t(`errors.actions.${action}`)}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    {#if onDismiss}
      <button
        type="button"
        class="rounded-lg p-1 hover:bg-red-100 dark:hover:bg-red-900/60"
        onclick={onDismiss}
        aria-label={$t('common.close')}
      >
        <X class="size-4" aria-hidden="true" />
      </button>
    {/if}
  </div>
</div>
