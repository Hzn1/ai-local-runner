<script lang="ts">
  import { t } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import { Toaster, toast } from 'svelte-sonner';
  import { MessageSquare, RefreshCw, ShieldAlert } from '@lucide/svelte';
  import Chat from './routes/Chat.svelte';
  import Models from './routes/Models.svelte';
  import Settings from './routes/Settings.svelte';
  import Onboarding from './routes/Onboarding.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import ErrorCard from './components/ui/ErrorCard.svelte';
  import Spinner from './components/ui/Spinner.svelte';
  import { app } from './stores/app.svelte';
  import { chat } from './stores/chat.svelte';
  import { router } from './lib/router.svelte';
  import { listenEvent } from './lib/events';
  import { isTauri } from './lib/tauri';
  import type { ChatCancelled } from './lib/bindings/ChatCancelled';
  import type { ChatDelta } from './lib/bindings/ChatDelta';
  import type { ChatDone } from './lib/bindings/ChatDone';
  import type { ChatErrorEvent } from './lib/bindings/ChatErrorEvent';
  import type { RecoveryAction } from './lib/bindings/index';

  let cleanup: Array<() => void> = [];
  let lastUpdateVersion = $state('');

  onMount(() => {
    let disposed = false;
    const removeKeydown = attachKeydown();

    void (async () => {
      const unlistenCore = await app.attachEvents();
      const unlistenChat = await Promise.all([
        listenEvent<ChatDelta>('chat://delta', (event) => chat.handleDelta(event)),
        listenEvent<ChatDone>('chat://done', (event) => {
          void chat.handleDone(event);
        }),
        listenEvent<ChatCancelled>('chat://cancelled', (event) => {
          void chat.handleCancelled(event);
        }),
        listenEvent<ChatErrorEvent>('chat://error', (event) => {
          void chat.handleError(event);
        }),
      ]);
      if (disposed) {
        unlistenCore();
        for (const unlisten of unlistenChat) unlisten();
        return;
      }
      cleanup.push(unlistenCore, ...unlistenChat);
      await app.init();
      if (disposed) return;
      if (!app.bootstrap?.onboarding_complete) {
        router.navigate('onboarding');
      } else if (app.hasInstalledModel) {
        router.navigate('chat');
      } else {
        router.navigate('models');
      }
    })();

    return () => {
      disposed = true;
      removeKeydown();
      for (const unlisten of cleanup) unlisten();
      cleanup = [];
    };
  });

  $effect(() => {
    if (app.update && app.update.version !== lastUpdateVersion) {
      lastUpdateVersion = app.update.version;
      toast.success(
        $t('toast.updateFound', { values: { version: app.update.version } }),
      );
    }
  });

  function attachKeydown(): () => void {
    const handler = (event: KeyboardEvent) => {
      const modifier = event.ctrlKey || event.metaKey;
      if (!modifier) {
        if (event.key === 'Escape' && chat.sending && router.route === 'chat') {
          event.preventDefault();
          void chat.stop();
        }
        return;
      }
      if (event.key.toLowerCase() === 'n') {
        event.preventDefault();
        void chat.newChat();
        router.navigate('chat');
      } else if (event.key.toLowerCase() === 'k') {
        event.preventDefault();
        const search = document.querySelector<HTMLInputElement>('[data-chat-search]');
        search?.focus();
      } else if (event.key === ',') {
        event.preventDefault();
        router.navigate('settings');
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }

  function handleFatalAction(action: RecoveryAction): void {
    if (action === 'retry' || action === 'restart_app') {
      window.location.reload();
    }
  }
</script>

{#if !app.ready}
  <div class="flex h-full items-center justify-center">
    <div class="flex flex-col items-center gap-4 text-center">
      <div
        class="flex size-14 items-center justify-center rounded-2xl bg-brand-700 text-white shadow-lg dark:bg-brand-500 dark:text-brand-950"
      >
        <MessageSquare class="size-7" aria-hidden="true" />
      </div>
      <div>
        <p class="text-base font-semibold tracking-tight">{$t('app.name')}</p>
        <p class="mt-0.5 text-xs text-stone-500 dark:text-stone-400">{$t('app.tagline')}</p>
      </div>
      <Spinner />
    </div>
  </div>
{:else if app.fatalError}
  <div class="mx-auto flex min-h-full max-w-xl flex-col justify-center px-6 py-10">
    <div class="mb-5 flex items-center gap-3">
      <div class="flex size-11 items-center justify-center rounded-xl bg-red-100 text-red-700 dark:bg-red-950/60 dark:text-red-300">
        <ShieldAlert class="size-6" aria-hidden="true" />
      </div>
      <div>
        <h1 class="text-lg font-semibold">{$t('errors.title')}</h1>
        <p class="text-xs text-stone-500 dark:text-stone-400">{$t('app.name')}</p>
      </div>
    </div>
    <ErrorCard error={app.fatalError} onAction={handleFatalAction} />
    {#if !isTauri}
      <p class="mt-3 text-xs text-stone-500 dark:text-stone-400">
        {$t('app.desktopOnly')}
      </p>
    {/if}
    <button
      type="button"
      class="btn-secondary mt-4 self-start text-xs"
      onclick={() => window.location.reload()}
    >
      <RefreshCw class="size-3.5" aria-hidden="true" />
      {$t('common.retry')}
    </button>
  </div>
{:else if router.route === 'onboarding'}
  <Onboarding />
{:else}
  <div class="flex h-full min-h-0">
    <Sidebar />
    <main class="min-w-0 flex-1">
      {#if router.route === 'chat'}
        <Chat />
      {:else if router.route === 'models'}
        <Models />
      {:else}
        <Settings />
      {/if}
    </main>
  </div>
{/if}

<Toaster
  position="bottom-right"
  richColors
  closeButton
  theme={app.settings?.theme ?? 'system'}
  toastOptions={{ duration: 4000 }}
/>
