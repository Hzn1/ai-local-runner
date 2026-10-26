<script lang="ts">
  import { t } from 'svelte-i18n';

  let { size = 'md' }: { size?: 'sm' | 'md' } = $props();
</script>

<span
  class="inline-flex items-center gap-2 text-stone-500 dark:text-stone-400"
  role="status"
>
  <span
    class="animate-spin rounded-full border-2 border-current border-t-transparent"
    class:size-4={size === 'sm'}
    class:size-5={size === 'md'}
  ></span>
  <span class={size === 'sm' ? 'text-xs' : 'text-sm'}>{$t('common.loading')}</span>
</span>
