<script lang="ts">
  let {
    checked,
    onchange,
    label,
    description = undefined,
    disabled = false,
  }: {
    checked: boolean;
    onchange: (value: boolean) => void;
    label: string;
    description?: string;
    disabled?: boolean;
  } = $props();
</script>

<label
  class="flex cursor-pointer items-start justify-between gap-4"
  class:opacity-60={disabled}
>
  <span class="min-w-0">
    <span class="block text-sm font-medium text-stone-800 dark:text-stone-100"
      >{label}</span
    >
    {#if description}
      <span class="mt-0.5 block text-xs leading-relaxed text-stone-500 dark:text-stone-400"
        >{description}</span
      >
    {/if}
  </span>
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    {disabled}
    class="relative mt-0.5 inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand-600 disabled:cursor-not-allowed {checked
      ? 'bg-brand-600 dark:bg-brand-400'
      : 'bg-stone-300 dark:bg-stone-700'}"
    onclick={() => onchange(!checked)}
  >
    <span
      class="inline-block size-4 rounded-full bg-white shadow transition-transform dark:bg-stone-950 {checked
        ? 'translate-x-6'
        : 'translate-x-1'}"
    ></span>
  </button>
</label>
