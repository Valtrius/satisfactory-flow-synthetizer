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
    default: 'border-[#314856] bg-[#142630] text-[#d8e3e8] hover:border-[#527387] hover:bg-[#1a3340]',
    primary:
      'border-[#f07831] bg-linear-to-br from-accent-bright to-accent text-[#15191b] shadow-[0_8px_28px_rgb(255_128_52/18%)] hover:border-[#ffc49d] hover:from-[#ffc08f] hover:to-[#ff934e]',
    danger: 'border-danger/70 bg-[#69221f]/35 text-[#ffc0bb] hover:border-danger hover:bg-[#8c2a26]/55',
    quiet: 'border-[#314856] bg-transparent text-muted hover:border-[#527387] hover:bg-[#1a3340]'
  };

  const sizes: Record<Size, string> = {
    default: 'min-h-10.5 px-4 py-2 text-sm',
    small: 'min-h-8.5 px-3 py-1.5 text-xs'
  };
</script>

<button
  class={`cursor-pointer rounded-tl-lg rounded-tr-xs rounded-br-lg rounded-bl-xs border font-bold transition-[border-color,background-color,transform] duration-150 hover:-translate-y-px disabled:cursor-not-allowed disabled:opacity-50 ${variants[variant]} ${square ? 'grid min-h-8.5 w-8.5 min-w-8.5 place-items-center p-0' : sizes[size]} ${className}`}
  {...props}
>
  {@render children()}
</button>
