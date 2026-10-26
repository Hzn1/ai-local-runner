<script lang="ts">
  import { t } from 'svelte-i18n';
  import { Sparkles, Copy, RotateCcw, UserRound } from '@lucide/svelte';
  import type { Message } from '../../lib/bindings/Message';
  import { renderMarkdown } from '../../lib/markdown';
  import { writeText } from '@tauri-apps/plugin-clipboard-manager';
  import { isTauri } from '../../lib/tauri';
  import { openExternal } from '../../lib/external';

  let {
    message,
    streaming = false,
    stage = '',
    showReasoning = false,
    onregenerate = undefined,
    oncopy = undefined,
  }: {
    message: Message;
    streaming?: boolean;
    stage?: string;
    showReasoning?: boolean;
    onregenerate?: () => void;
    oncopy?: () => void;
  } = $props();

  const isUser = $derived(message.role === 'user');
  const html = $derived(
    !isUser && message.status === 'complete'
      ? renderMarkdown(message.content)
      : '',
  );
  const hasReasoning = $derived(
    !isUser && showReasoning && Boolean(message.reasoning?.trim()),
  );

  async function copy(): Promise<void> {
    if (!message.content) return;
    try {
      if (isTauri) {
        await writeText(message.content);
      } else {
        await navigator.clipboard.writeText(message.content);
      }
      oncopy?.();
    } catch {
      // Clipboard permission failures are non-fatal.
    }
  }

  async function handleMarkdownClick(event: MouseEvent): Promise<void> {
    const target = event.target as HTMLElement | null;
    const anchor = target?.closest('a');
    const href = anchor?.getAttribute('href');
    if (!href) return;
    // Never navigate the webview away from the app.
    event.preventDefault();
    try {
      await openExternal(href);
    } catch {
      // The OS refused to open the link; the answer is still readable.
    }
  }

  /**
   * Delegate link clicks on sanitized Markdown to the OS opener. An action is
   * used instead of a template `onclick` so the container stays semantically
   * plain content rather than a fake interactive control.
   */
  function externalLinks(node: HTMLElement) {
    const listener = (event: MouseEvent) => void handleMarkdownClick(event);
    node.addEventListener('click', listener);
    return {
      destroy: () => node.removeEventListener('click', listener),
    };
  }
</script>

{#if isUser}
  <div class="flex justify-end gap-2.5">
    <div
      class="max-w-[85%] rounded-2xl rounded-br-md bg-brand-700 px-4 py-2.5 text-[0.95rem] leading-7 whitespace-pre-wrap text-white shadow-sm dark:bg-brand-500 dark:text-brand-950"
    >
      {message.content}
    </div>
    <div
      class="mt-1 flex size-8 shrink-0 items-center justify-center rounded-full bg-stone-200 text-stone-600 dark:bg-stone-800 dark:text-stone-300"
      aria-hidden="true"
    >
      <UserRound class="size-4" />
    </div>
  </div>
{:else}
  <div class="flex gap-2.5">
    <div
      class="mt-1 flex size-8 shrink-0 items-center justify-center rounded-full bg-brand-100 text-brand-800 dark:bg-brand-900/60 dark:text-brand-200"
      aria-hidden="true"
    >
      <Sparkles class="size-4" />
    </div>
    <div class="group min-w-0 flex-1">
      {#if hasReasoning}
        <details class="mb-2 rounded-xl border border-stone-200 bg-stone-100/70 px-3 py-2 dark:border-stone-800 dark:bg-stone-900/60">
          <summary class="cursor-pointer text-xs font-medium text-stone-500 dark:text-stone-400">
            {$t('chat.reasoning')}
          </summary>
          <pre class="mt-2 max-h-64 overflow-y-auto whitespace-pre-wrap break-words font-sans text-xs leading-relaxed text-stone-600 dark:text-stone-300">{message.reasoning}</pre>
        </details>
      {/if}

      {#if message.status === 'error' && !message.content}
        <p class="text-sm text-red-700 dark:text-red-300">{$t('chat.error')}</p>
      {:else if streaming}
        {#if message.content}
          <div class="text-[0.95rem] leading-7 whitespace-pre-wrap text-stone-800 dark:text-stone-100"
            >{message.content}<span
              class="ml-0.5 inline-block h-4 w-1.5 animate-pulse rounded-sm bg-brand-600 align-middle dark:bg-brand-400"
            ></span
            ></div
          >
        {:else}
          <div class="flex items-center gap-2 py-1 text-sm text-stone-500 dark:text-stone-400" role="status">
            <span class="flex gap-1" aria-hidden="true">
              <span class="size-1.5 animate-bounce rounded-full bg-brand-500 [animation-delay:-0.3s]"></span>
              <span class="size-1.5 animate-bounce rounded-full bg-brand-500 [animation-delay:-0.15s]"></span>
              <span class="size-1.5 animate-bounce rounded-full bg-brand-500"></span>
            </span>
            {stage || $t('chat.reading')}
          </div>
        {/if}
      {:else}
        <div class="markdown" use:externalLinks>{@html html}</div>
      {/if}

      {#if message.status === 'interrupted'}
        <span class="badge mt-2 bg-amber-100 text-amber-800 dark:bg-amber-900/50 dark:text-amber-200">
          {$t('chat.interrupted')}
        </span>
      {/if}

      {#if !streaming && message.content}
        <div class="mt-1.5 flex gap-1 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
          <button
            type="button"
            class="rounded-lg p-1.5 text-stone-400 hover:bg-stone-200 hover:text-stone-700 dark:hover:bg-stone-800 dark:hover:text-stone-200"
            title={$t('common.copy')}
            aria-label={$t('common.copy')}
            onclick={copy}
          >
            <Copy class="size-3.5" aria-hidden="true" />
          </button>
          {#if onregenerate}
            <button
              type="button"
              class="rounded-lg p-1.5 text-stone-400 hover:bg-stone-200 hover:text-stone-700 dark:hover:bg-stone-800 dark:hover:text-stone-200"
              title={$t('common.regenerate')}
              aria-label={$t('common.regenerate')}
              onclick={onregenerate}
            >
              <RotateCcw class="size-3.5" aria-hidden="true" />
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}
