<script lang="ts">
  import { onMount } from 'svelte';
  import type { HTMLInputAttributes } from 'svelte/elements';

  type Shape = 'default' | 'attached-left';
  type Size = 'default' | 'small' | 'large' | 'inline';

  type Props = Omit<HTMLInputAttributes, 'value' | 'size'> & {
    value?: string;
    shape?: Shape;
    size?: Size;
    compact?: boolean;
    focusOnMount?: boolean;
  };

  let {
    value = $bindable(''),
    shape = 'default',
    size = 'default',
    compact = false,
    focusOnMount = false,
    class: className = '',
    ...props
  }: Props = $props();

  let element: HTMLInputElement;
  onMount(() => {
    if (focusOnMount) {
      element.focus();
      element.select();
    }
  });

  const shapes: Record<Shape, string> = {
    default: 'rounded-control',
    'attached-left': 'rounded-tl-lg rounded-bl-xs',
  };

  const sizes: Record<Size, string> = {
    small: 'h-8 px-2.5 text-xs',
    default: 'h-9 px-3',
    large: 'h-10 px-3',
    inline: 'px-1.5 py-0.5 text-sm font-bold',
  };
</script>

<input
  bind:this={element}
  bind:value
  class={`${sizes[size]} ${compact ? 'w-11.5' : 'w-full'} border-field-border bg-field text-ink hover:border-field-border-hover focus:outline-accent border transition-colors focus:z-10 focus:outline-2 focus:outline-offset-3 disabled:cursor-not-allowed disabled:opacity-50 ${shapes[shape]} ${className}`}
  {...props}
/>
