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
    default: 'min-h-9 px-4 py-2 text-sm',
    small: 'min-h-8 px-3 py-1.5 text-xs'
  };

  const squareSizes: Record<Size, string> = {
    default: 'min-h-9 w-9 min-w-9',
    small: 'min-h-8 w-8 min-w-8'
  };
</script>

<button
  class={`cursor-pointer rounded-control border font-bold transition-[border-color,background-color,background-image,transform,box-shadow] duration-150 hover:-translate-y-px disabled:cursor-not-allowed disabled:opacity-50 ${variants[variant]} ${square ? `grid place-items-center p-0 ${squareSizes[size]}` : sizes[size]} ${className}`}
  {...props}
>
  {@render children()}
</button>
