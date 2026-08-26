<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  type Variant = 'default' | 'primary' | 'danger' | 'quiet';
  type Size = 'default' | 'small';

  type Props = HTMLButtonAttributes & {
    children: Snippet;
    variant?: Variant;
    size?: Size;
    square?: boolean;
  };

  let {
    children,
    variant = 'default',
    size = 'default',
    square = false,
    class: className = '',
    ...props
  }: Props = $props();

  const variants: Record<Variant, string> = {
    default: 'intent-default',
    primary: 'intent-primary intent-primary-glow',
    danger: 'intent-danger',
    quiet: 'intent-quiet'
  };

  const sizes: Record<Size, string> = {
    default: 'min-h-10.5 px-4 py-2 text-sm',
    small: 'min-h-8.5 px-3 py-1.5 text-xs'
  };
</script>

<button
  class={`cursor-pointer rounded-control border font-bold transition-[border-color,background-color,background-image,transform,box-shadow] duration-150 hover:-translate-y-px disabled:cursor-not-allowed disabled:opacity-50 ${variants[variant]} ${square ? 'grid min-h-8.5 w-8.5 min-w-8.5 place-items-center p-0' : sizes[size]} ${className}`}
  {...props}
>
  {@render children()}
</button>
