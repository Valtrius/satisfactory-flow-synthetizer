<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';

  type Shape = 'default' | 'attached-left';
  type Size = 'default' | 'small' | 'large';

  type Props = Omit<HTMLInputAttributes, 'value' | 'size'> & {
    value?: string;
    shape?: Shape;
    size?: Size;
    compact?: boolean;
  };

  let {
    value = $bindable(''),
    shape = 'default',
    size = 'default',
    compact = false,
    class: className = '',
    ...props
  }: Props = $props();

  const shapes: Record<Shape, string> = {
    default: 'rounded-control',
    'attached-left': 'rounded-tl-lg rounded-bl-xs',
  };

  const sizes: Record<Size, string> = {
    small: 'h-8 px-2.5 text-xs',
    default: 'h-9 px-3',
    large: 'h-10 px-3',
  };
</script>

<input
  bind:value
  class={`${sizes[size]} ${compact ? 'w-11.5' : 'w-full'} border-field-border bg-field text-ink hover:border-field-border-hover focus:outline-accent border transition-colors focus:z-10 focus:outline-2 focus:outline-offset-3 disabled:cursor-not-allowed disabled:opacity-50 ${shapes[shape]} ${className}`}
  {...props}
/>
