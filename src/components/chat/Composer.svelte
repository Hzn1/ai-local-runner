<script lang="ts">
  import { t } from 'svelte-i18n';
  import { ArrowUp, Square } from '@lucide/svelte';

  let {
    sending,
    disabled = false,
    onsend,
    onstop,
  }: {
    sending: boolean;
    disabled?: boolean;
    onsend: (text: string) => void;
    onstop: () => void;
  } = $props();

  let text = $state('');
  let textarea = $state<HTMLTextAreaElement | null>(null);
  let composing = false;

  $effect(() => {
    if (!textarea) return;
    textarea.style.height = 'auto';
    textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
  });

  function submit(): void {
    const value = text.trim();
    if (!value || sending || disabled) return;
    onsend(value);
    text = '';
    queueMicrotask(() => textarea?.focus());
  }

  function keydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && sending) {
      event.preventDefault();
      onstop();
      return;
    }
    if (event.key === 'Enter' && !event.shiftKey && !composing) {
      event.preventDefault();
      submit();
    }
  }
</script>

<form
  class="rounded-2xl border border-stone-300 bg-white p-2 shadow-sm focus-within:border-brand-600 focus-within:ring-2 focus-within:ring-brand-600/15 dark:border-stone-700 dark:bg-stone-900"
  onsubmit={(event) => {
    event.preventDefault();
    submit();
  }}
>
  <div class="flex items-end gap-2">
    <textarea
      bind:this={textarea}
      bind:value={text}
      class="max-h-52 min-h-[2.75rem] flex-1 resize-none bg-transparent px-2 py-2.5 text-[0.95rem] leading-6 text-stone-900 placeholder:text-stone-400 focus:outline-none dark:text-stone-100 dark:placeholder:text-stone-500"
      placeholder={$t('chat.placeholder')}
      aria-label={$t('chat.placeholder')}
      rows="1"
      {disabled}
      onkeydown={keydown}
      oncompositionstart={() => (composing = true)}
      oncompositionend={() => (composing = false)}
    ></textarea>
    {#if sending}
      <button
        type="button"
        class="mb-0.5 flex size-10 shrink-0 items-center justify-center rounded-xl bg-stone-900 text-white transition-colors hover:bg-stone-700 dark:bg-stone-100 dark:text-stone-900 dark:hover:bg-stone-300"
        onclick={onstop}
        aria-label={$t('chat.stop')}
        title={$t('chat.stop')}
      >
        <Square class="size-4" fill="currentColor" aria-hidden="true" />
      </button>
    {:else}
      <button
        type="submit"
        class="mb-0.5 flex size-10 shrink-0 items-center justify-center rounded-xl bg-brand-700 text-white transition-colors hover:bg-brand-800 disabled:cursor-not-allowed disabled:opacity-40 dark:bg-brand-500 dark:text-brand-950 dark:hover:bg-brand-400"
        disabled={!text.trim() || disabled}
        aria-label={$t('chat.send')}
        title={$t('chat.send')}
      >
        <ArrowUp class="size-4" aria-hidden="true" />
      </button>
    {/if}
  </div>
  <p class="px-2 pb-0.5 text-[0.68rem] text-stone-400 dark:text-stone-500">
    {$t('chat.hint')}
  </p>
</form>
