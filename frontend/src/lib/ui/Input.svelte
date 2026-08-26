<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';

  type Shape = 'default' | 'attached-left';

  type Props = Omit<HTMLInputAttributes, 'value'> & {
    value?: string;
    shape?: Shape;
    compact?: boolean;
  };

  let {
    value = $bindable(''),
    shape = 'default',
    compact = false,
    class: className = '',
    ...props
  }: Props = $props();

  const shapes: Record<Shape, string> = {
    default: 'rounded-control',
    'attached-left': 'rounded-tl-lg rounded-bl-xs'
  };
</script>

<input
  bind:value
  class={`h-10.5 ${compact ? 'w-11.5' : 'w-full'} border border-field-border bg-field px-3 text-ink transition-colors hover:border-field-border-hover focus:z-10 focus:outline-2 focus:outline-offset-3 focus:outline-accent disabled:cursor-not-allowed disabled:opacity-50 ${shapes[shape]} ${className}`}
  {...props}
/>
