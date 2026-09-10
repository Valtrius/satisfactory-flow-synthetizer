<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  type Variant = 'default' | 'primary' | 'danger' | 'quiet' | 'plain' | 'menu' | 'chip';
  type Size = 'default' | 'small' | 'tiny';

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
    type = 'button',
    class: className = '',
    ...props
  }: Props = $props();

  const variants: Record<Variant, string> = {
    default: 'intent-default',
    primary: 'intent-primary intent-primary-glow',
    danger: 'intent-danger',
    quiet: 'intent-quiet',
    plain: '',
    menu: 'flex w-full items-center gap-2 border-0 px-3 py-1.5 text-left text-xs hover:bg-well-hover',
    chip: 'rounded-control border px-2.5 py-1.5 text-left text-xs font-semibold transition-colors border-control-border bg-control/60 text-muted hover:border-control-border-hover hover:text-control-fg aria-pressed:border-accent/70 aria-pressed:bg-selected aria-pressed:text-ink',
  };

  const sizes: Record<Size, string> = {
    default: 'min-h-9 px-4 py-2 text-sm',
    small: 'min-h-8 px-3 py-1.5 text-xs',
    tiny: 'min-h-7 px-2 py-1 text-xs',
  };

  const squareSizes: Record<Size, string> = {
    default: 'min-h-9 w-9 min-w-9',
    small: 'min-h-8 w-8 min-w-8',
    tiny: 'min-h-7 w-7 min-w-7',
  };
</script>

<button
  {type}
  class={`focus-visible:outline-accent cursor-pointer focus-visible:outline-2 focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50 ${variants[variant]} ${
    variant === 'plain' || variant === 'menu' || variant === 'chip'
      ? ''
      : `rounded-control border font-bold transition-[border-color,background-color,background-image,transform,box-shadow] duration-150 hover:-translate-y-px ${square ? `grid place-items-center p-0 ${squareSizes[size]}` : sizes[size]}`
  } ${className}`}
  {...props}
>
  {@render children()}
</button>
