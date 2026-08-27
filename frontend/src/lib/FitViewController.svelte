<script lang="ts">
  import { tick } from 'svelte';
  import { useSvelteFlow } from '@xyflow/svelte';

  let { revision }: { revision: number } = $props();
  const { fitView } = useSvelteFlow();
  let mounted = false;

  $effect(() => {
    const requestedRevision = revision;
    if (!mounted) {
      mounted = true;
      return;
    }
    void refit(requestedRevision);
  });

  async function refit(_revision: number): Promise<void> {
    await tick();
    requestAnimationFrame(() => {
      requestAnimationFrame(() => void fitView({ padding: 0.18, duration: 200 }));
    });
  }
</script>
