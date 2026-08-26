<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  type Props = HTMLAttributes<HTMLElement> & {
    children: Snippet;
    element?: 'article' | 'aside' | 'section';
  };

  let {
    children,
    element = 'article',
    class: className = '',
    ...rest
  }: Props = $props();
</script>

<svelte:element
  this={element}
  class={`rounded-panel border border-line bg-linear-to-br from-panel-2/97 to-[#0a161e]/97 shadow-[0_18px_55px_rgb(0_0_0/17%)] ${className}`}
  {...rest}
>
  {@render children()}
</svelte:element>
