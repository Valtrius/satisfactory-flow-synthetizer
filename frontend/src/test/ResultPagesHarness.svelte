<script lang="ts">
  import { untrack } from 'svelte';
  import { createResultPages } from '../lib/resultPages.svelte';

  let {
    args,
    onReady,
  }: {
    args: Parameters<typeof createResultPages>;
    onReady: (results: ReturnType<typeof createResultPages>) => void;
  } = $props();

  untrack(() => onReady(createResultPages(...args)));
</script>
