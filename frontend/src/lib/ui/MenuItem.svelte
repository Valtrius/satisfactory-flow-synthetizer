<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import Button from './Button.svelte';
  type Props = HTMLButtonAttributes & { children: Snippet; danger?: boolean };
  let { children, danger = false, class: className = '', ...props }: Props = $props();
</script>

<Button {...props} variant="menu" role="menuitem" class={`${danger ? 'text-danger' : ''} ${className}`}>
  {@render children()}
</Button>
