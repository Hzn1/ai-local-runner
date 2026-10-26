<script lang="ts">
  import { t } from 'svelte-i18n';
  import {
    Settings,
    Boxes,
    Search,
    Plus,
    Trash2,
    Cpu,
    MessageSquare,
  } from '@lucide/svelte';
  import { app } from '../stores/app.svelte';
  import { chat } from '../stores/chat.svelte';
  import { router } from '../lib/router.svelte';
  import { relativeTime } from '../lib/format';

  let search = $state('');
  const filtered = $derived(
    app.conversations.filter((conversation) => {
      const query = search.trim().toLowerCase();
      if (!query) return true;
      return (
        conversation.title.toLowerCase().includes(query) ||
        (conversation.preview ?? '').toLowerCase().includes(query)
      );
    }),
  );

  const engineState = $derived(app.engine?.state ?? 'stopped');

  function newChat(): void {
    void chat.newChat();
    router.navigate('chat');
  }

  function openConversation(id: string): void {
    void chat.open(id);
    router.navigate('chat');
  }

  async function removeConversation(event: MouseEvent, id: string): Promise<void> {
    event.stopPropagation();
    if (!window.confirm($t('chat.deleteBody'))) return;
    try {
      await chat.remove(id);
    } catch {
      // The chat page shows the typed error on next action; keep the sidebar quiet.
    }
  }
</script>

<aside
  class="flex h-full min-h-0 w-[17.5rem] shrink-0 flex-col border-r border-stone-200 bg-stone-100/70 dark:border-stone-800 dark:bg-stone-900/40"
  aria-label={$t('nav.chatHistory')}
>
  <div class="flex items-center gap-2 px-4 pt-5 pb-3">
    <div
      class="flex size-9 items-center justify-center rounded-xl bg-brand-700 text-white shadow-sm dark:bg-brand-500 dark:text-brand-950"
    >
      <MessageSquare class="size-5" aria-hidden="true" />
    </div>
    <div class="min-w-0">
      <p class="truncate text-sm font-semibold tracking-tight">{$t('app.name')}</p>
      <p class="truncate text-[0.7rem] text-stone-500 dark:text-stone-400">
        {$t('app.local')}
      </p>
    </div>
  </div>

  <div class="px-3">
    <button type="button" class="btn-primary w-full justify-start" onclick={newChat}>
      <Plus class="size-4" aria-hidden="true" />
      {$t('nav.newChat')}
    </button>
  </div>

  <div class="relative mt-3 px-3">
    <Search
      class="pointer-events-none absolute top-1/2 left-6 size-4 -translate-y-1/2 text-stone-400"
      aria-hidden="true"
    />
    <input
      class="input py-2 pl-9"
      type="search"
      placeholder={$t('nav.search')}
      aria-label={$t('nav.search')}
      bind:value={search}
      data-chat-search
    />
  </div>

  <nav class="mt-2 min-h-0 flex-1 overflow-y-auto px-2 pb-2" aria-label={$t('nav.chatHistory')}>
    {#if filtered.length === 0}
      <p class="px-3 py-6 text-center text-xs text-stone-500 dark:text-stone-400">
        {$t('nav.noChats')}
      </p>
    {:else}
      <ul class="space-y-0.5">
        {#each filtered as conversation (conversation.id)}
          <li>
            <div
              class="group flex w-full items-start gap-2 rounded-xl px-3 py-2 text-left transition-colors {chat.current
                ?.id === conversation.id
                ? 'bg-stone-200/80 dark:bg-stone-800'
                : 'hover:bg-stone-200/50 dark:hover:bg-stone-800/60'}"
            >
              <button
                type="button"
                class="min-w-0 flex-1 text-left"
                onclick={() => openConversation(conversation.id)}
                aria-current={chat.current?.id === conversation.id ? 'page' : undefined}
              >
                <span class="block truncate text-sm font-medium text-stone-800 dark:text-stone-100"
                  >{conversation.title}</span
                >
                <span class="mt-0.5 block truncate text-[0.7rem] text-stone-500 dark:text-stone-400">
                  {relativeTime(conversation.updated_at)}
                </span>
              </button>
              <button
                type="button"
                class="mt-0.5 rounded-md p-1 text-stone-400 opacity-0 transition-opacity group-hover:opacity-100 hover:bg-stone-300 hover:text-red-700 focus-visible:opacity-100 dark:hover:bg-stone-700 dark:hover:text-red-300"
                aria-label={$t('common.delete')}
                title={$t('common.delete')}
                onclick={(event) => removeConversation(event, conversation.id)}
              >
                <Trash2 class="size-3.5" aria-hidden="true" />
              </button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </nav>

  <div class="border-t border-stone-200 px-3 py-3 dark:border-stone-800">
    <div class="mb-2 flex items-center gap-2 px-1">
      <span
        class="size-2 rounded-full {engineState === 'ready'
          ? 'bg-emerald-500'
          : engineState === 'starting' || engineState === 'loading'
            ? 'animate-pulse bg-amber-500'
            : 'bg-stone-400 dark:bg-stone-600'}"
        aria-hidden="true"
      ></span>
      <span class="min-w-0 flex-1 truncate text-[0.7rem] text-stone-500 dark:text-stone-400">
        {$t(`engine.${engineState}`)}
      </span>
      <Cpu class="size-3.5 shrink-0 text-stone-400" aria-hidden="true" />
    </div>
    <div class="mb-2 truncate rounded-lg bg-white px-2.5 py-1.5 text-[0.7rem] font-medium text-stone-600 shadow-sm dark:bg-stone-900 dark:text-stone-300">
      {$t('chat.modelBadge', {
        values: {
          model: app.bootstrap?.active_model_id
            ? app.modelName(app.bootstrap.active_model_id)
            : $t('common.none'),
        },
      })}
    </div>
    <div class="flex gap-1">
      <button
        type="button"
        class="sidebar-link {router.route === 'models'
          ? 'sidebar-link-active'
          : ''}"
        onclick={() => router.navigate('models')}
      >
        <Boxes class="size-4" aria-hidden="true" />
        {$t('nav.models')}
      </button>
      <button
        type="button"
        class="sidebar-link {router.route === 'settings'
          ? 'sidebar-link-active'
          : ''}"
        onclick={() => router.navigate('settings')}
      >
        <Settings class="size-4" aria-hidden="true" />
        {$t('nav.settings')}
      </button>
    </div>
  </div>
</aside>
