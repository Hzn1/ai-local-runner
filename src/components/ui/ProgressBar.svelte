<script lang="ts">
  let {
    value,
    indeterminate = false,
    label = undefined,
  }: { value: number; indeterminate?: boolean; label?: string } = $props();
</script>

<div
  class="h-2 w-full overflow-hidden rounded-full bg-stone-200 dark:bg-stone-800"
  role="progressbar"
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={indeterminate ? undefined : Math.round(value)}
  aria-label={label}
>
  <div
    class="h-full rounded-full bg-brand-600 transition-[width] duration-300 dark:bg-brand-400"
    class:animate-pulse={indeterminate}
    style:width={indeterminate ? '40%' : `${Math.min(100, Math.max(0, value))}%`}
  ></div>
</div>
