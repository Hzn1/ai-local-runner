<script lang="ts">
  import { t } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import { toast } from 'svelte-sonner';
  import {
    ArrowDown,
    Boxes,
    Download,
    FileJson,
    FileText,
    Plus,
    Sparkles,
  } from '@lucide/svelte';
  import { app } from '../stores/app.svelte';
  import { chat } from '../stores/chat.svelte';
  import { router } from '../lib/router.svelte';
  import { api, isAndroid } from '../lib/tauri';
  import { normalizeError } from '../lib/errors';
  import { formatNumber } from '../lib/format';
  import type { RecoveryAction } from '../lib/bindings/index';
  import Composer from '../components/chat/Composer.svelte';
  import MessageBubble from '../components/chat/MessageBubble.svelte';
  import ErrorCard from '../components/ui/ErrorCard.svelte';

  let scroller = $state<HTMLDivElement | null>(null);
  let pinned = $state(true);
  let showJump = $state(false);
  let exporting = $state(false);

  const engineBusy = $derived(
    app.engine?.state === 'starting' || app.engine?.state === 'loading',
  );
  const stage = $derived(
    engineBusy ? $t('chat.starting') : $t('chat.reading'),
  );
  const suggestions = $derived([
    $t('chat.suggestion1'),
    $t('chat.suggestion2'),
    $t('chat.suggestion3'),
  ]);
  const stats = $derived(chat.lastStats);
  const wordsPerSecond = $derived(
    stats ? stats.tokensPerSec * 0.75 : 0,
  );
  const showThinking = $derived(
    chat.sending && !(chat.lastAssistant?.content ?? ''),
  );

  onMount(() => {
    if (!chat.current && app.hasInstalledModel) {
      // Start on a clean composer; a conversation is created on the first send.
      pinned = true;
    }
  });

  $effect(() => {
    // Track messages and the growing tail so the view follows streaming text.
    const count = chat.messages.length;
    const tail = chat.messages[count - 1];
    void count;
    void tail?.content;
    if (pinned) {
      requestAnimationFrame(() => scrollToBottom(false));
    }
  });

  $effect(() => {
    const id = chat.current?.id;
    void id;
    pinned = true;
    showJump = false;
    requestAnimationFrame(() => scrollToBottom(false));
  });

  function scrollToBottom(smooth = true): void {
    if (!scroller) return;
    scroller.scrollTo({
      top: scroller.scrollHeight,
      behavior: smooth ? 'smooth' : 'auto',
    });
  }

  function onScroll(): void {
    if (!scroller) return;
    const distance =
      scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight;
    pinned = distance < 80;
    showJump = !pinned;
  }

  async function sendSuggestion(text: string): Promise<void> {
    await chat.send(text);
  }

  async function startChatWithSuggestion(text: string): Promise<void> {
    await chat.send(text);
  }

  async function exportConversation(format: 'json' | 'markdown'): Promise<void> {
    exporting = true;
    try {
      const path = await chat.exportConversation(format);
      if (path) {
        toast.success(isAndroid ? $t('common.exportReady') : $t('chat.exported', { values: { path } }));
      }
    } catch (error) {
      toast.error(normalizeError(error).message);
    } finally {
      exporting = false;
    }
  }

  async function handleErrorAction(action: RecoveryAction): Promise<void> {
    switch (action) {
      case 'retry':
        await chat.regenerate();
        break;
      case 'restart_engine':
        try {
          await app.startEngine();
          await chat.regenerate();
        } catch (error) {
          toast.error(normalizeError(error).message);
        }
        break;
      case 'new_chat':
        await chat.newChat();
        break;
      case 'choose_smaller_model':
      case 'redownload':
      case 'import_file':
      case 'free_disk_space':
        router.navigate('models');
        break;
      case 'reduce_context':
      case 'free_memory':
      case 'compatibility_mode':
        router.navigate('settings');
        break;
      case 'export_diagnostics':
        try {
          const path = await api.exportDiagnostics();
          toast.success($t('toast.diagnosticsExported'), {
            description: path,
          });
        } catch (error) {
          toast.error(normalizeError(error).message);
        }
        break;
      default:
        break;
    }
  }
</script>

<section class="flex h-full min-h-0 flex-col">
  <header
    class="flex items-center gap-3 border-b border-stone-200 px-6 py-3.5 dark:border-stone-800"
  >
    <div class="min-w-0 flex-1">
      <h1 class="truncate text-sm font-semibold tracking-tight text-stone-900 dark:text-stone-50">
        {chat.current?.title ?? $t('nav.newChat')}
      </h1>
      <p class="mt-0.5 flex items-center gap-1.5 truncate text-[0.7rem] text-stone-500 dark:text-stone-400">
        <span class="size-1.5 rounded-full bg-emerald-500" aria-hidden="true"></span>
        {$t('app.local')}
        <span aria-hidden="true">·</span>
        {app.bootstrap?.active_model_id
          ? app.modelName(app.bootstrap.active_model_id)
          : $t('common.none')}
      </p>
    </div>

    {#if chat.current}
      <details class="relative">
        <summary
          class="btn-secondary list-none px-3 py-1.5 text-xs [&::-webkit-details-marker]:hidden"
        >
          <Download class="size-3.5" aria-hidden="true" />
          {$t('common.export')}
        </summary>
        <div
          class="absolute right-0 z-20 mt-1 w-48 overflow-hidden rounded-xl border border-stone-200 bg-white py-1 shadow-lg dark:border-stone-700 dark:bg-stone-900"
        >
          <button
            type="button"
            class="flex w-full items-center gap-2 px-3 py-2 text-left text-xs hover:bg-stone-100 dark:hover:bg-stone-800"
            disabled={exporting}
            onclick={() => exportConversation('markdown')}
          >
            <FileText class="size-3.5" aria-hidden="true" />
            {$t('chat.exportMarkdown')}
          </button>
          <button
            type="button"
            class="flex w-full items-center gap-2 px-3 py-2 text-left text-xs hover:bg-stone-100 dark:hover:bg-stone-800"
            disabled={exporting}
            onclick={() => exportConversation('json')}
          >
            <FileJson class="size-3.5" aria-hidden="true" />
            {$t('chat.exportJson')}
          </button>
        </div>
      </details>
    {/if}

    <button
      type="button"
      class="btn-ghost px-2.5 py-1.5"
      title={$t('nav.newChat')}
      aria-label={$t('nav.newChat')}
      onclick={() => {
        void chat.newChat();
        router.navigate('chat');
      }}
    >
      <Plus class="size-4" aria-hidden="true" />
    </button>
  </header>

  <div class="relative min-h-0 flex-1">
    <div
      bind:this={scroller}
      class="h-full overflow-y-auto px-4 py-6 sm:px-8"
      onscroll={onScroll}
      role="log"
      aria-label={$t('nav.chat')}
    >
      <div class="mx-auto flex max-w-3xl flex-col gap-5">
        {#if chat.messages.length === 0}
          <div class="flex min-h-[45vh] flex-col items-center justify-center text-center">
            <div
              class="mb-4 flex size-14 items-center justify-center rounded-2xl bg-brand-100 text-brand-800 dark:bg-brand-900/50 dark:text-brand-200"
            >
              <Sparkles class="size-7" aria-hidden="true" />
            </div>
            <h2 class="text-2xl font-semibold tracking-tight text-stone-900 dark:text-stone-50">
              {$t('chat.emptyTitle')}
            </h2>
            <p class="mt-2 max-w-md text-sm leading-relaxed text-stone-500 dark:text-stone-400">
              {$t('chat.emptySubtitle')}
            </p>

            {#if app.hasInstalledModel}
              <div class="mt-6 grid w-full max-w-xl gap-2 sm:grid-cols-3">
                {#each suggestions as suggestion (suggestion)}
                  <button
                    type="button"
                    class="rounded-xl border border-stone-200 bg-white px-3 py-3 text-left text-xs leading-relaxed text-stone-600 shadow-sm transition-colors hover:border-brand-300 hover:bg-brand-50/50 dark:border-stone-800 dark:bg-stone-900 dark:text-stone-300 dark:hover:border-brand-700 dark:hover:bg-brand-950/30"
                    onclick={() => startChatWithSuggestion(suggestion)}
                  >
                    {suggestion}
                  </button>
                {/each}
              </div>
            {:else}
              <button
                type="button"
                class="btn-primary mt-6"
                onclick={() => router.navigate('models')}
              >
                <Boxes class="size-4" aria-hidden="true" />
                {$t('nav.models')}
              </button>
            {/if}
          </div>
        {:else}
          {#each chat.messages as message (message.id)}
            <MessageBubble
              {message}
              streaming={message.status === 'streaming'}
              stage={chat.waitingForContinuation && message.id === chat.pendingAssistantId
                ? $t('chat.preparingContinuation')
                : showThinking ? stage : ''}
              showReasoning={app.settings?.show_reasoning ?? false}
              onregenerate={() => chat.regenerate()}
              oncontinue={!chat.sending && message.id === chat.messages[chat.messages.length - 1]?.id
                && ['complete', 'truncated', 'interrupted'].includes(message.status)
                ? () => { void chat.continueResponse(); }
                : undefined}
              oncopy={() => toast.success($t('toast.copied'))}
            />
          {/each}
        {/if}

        {#if chat.error}
          <ErrorCard
            error={chat.error}
            onAction={handleErrorAction}
            onDismiss={() => (chat.error = null)}
          />
        {/if}

        <div class="h-2"></div>
      </div>
    </div>

    {#if showJump}
      <button
        type="button"
        class="absolute bottom-4 left-1/2 flex -translate-x-1/2 items-center gap-1.5 rounded-full border border-stone-200 bg-white px-3 py-1.5 text-xs font-medium text-stone-700 shadow-lg hover:bg-stone-50 dark:border-stone-700 dark:bg-stone-900 dark:text-stone-200 dark:hover:bg-stone-800"
        onclick={() => {
          pinned = true;
          showJump = false;
          scrollToBottom();
        }}
      >
        <ArrowDown class="size-3.5" aria-hidden="true" />
        {$t('chat.jumpToLatest')}
      </button>
    {/if}
  </div>

  <div class="border-t border-stone-200 px-4 py-3 sm:px-8 dark:border-stone-800">
    <div class="mx-auto max-w-3xl">
      <Composer
        sending={chat.sending}
        disabled={!app.hasInstalledModel}
        onsend={(text: string) => sendSuggestion(text)}
        onstop={() => chat.stop()}
      />
      <div class="mt-1.5 flex items-center justify-between px-1">
        <p class="text-[0.68rem] text-stone-400 dark:text-stone-500">
          {$t('app.privacy')}
        </p>
        {#if stats}
          <p class="text-[0.68rem] font-medium text-stone-500 dark:text-stone-400">
            {$t('chat.wordsPerSecond', {
              values: { speed: formatNumber(wordsPerSecond, 1) },
            })}
          </p>
        {/if}
      </div>
    </div>
  </div>
</section>
