<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    children,
    element = 'article',
    class: className = ''
  }: {
    children: Snippet;
    element?: 'article' | 'aside' | 'section';
    class?: string;
  } = $props();
</script>

<svelte:element
  this={element}
  class={`rounded-tl-2xl rounded-tr-sm rounded-br-2xl rounded-bl-sm border border-line bg-linear-to-br from-panel-2/97 to-[#0a161e]/97 shadow-[0_18px_55px_rgb(0_0_0/17%)] ${className}`}
>
  {@render children()}
</svelte:element>
