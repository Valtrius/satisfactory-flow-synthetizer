<script lang="ts">
  import { onMount } from 'svelte';
  import Button from '../ui/Button.svelte';
  import { createOfflineClient, type OfflineState } from './client';
  let state = $state<OfflineState>({ available: false, busy: false, update: false, status: null, error: '' });
  const client = createOfflineClient((value) => {
    state = value;
  });
  onMount(() => {
    void client.start();
    return client.dispose;
  });
</script>

<section aria-label="Offline availability" class="border-line rounded-md border p-3 text-sm">
  <div class="flex flex-wrap items-center gap-3">
    <span role="status" aria-live="polite">
      {#if state.busy}Preparing offline files or checking for updates…
      {:else if state.status?.complete}Ready for offline solving and viewing.
      {:else if state.status?.shellComplete}Interface saved. Download solver files for full offline use.
      {:else}Offline files are not ready yet.{/if}
    </span>
    <Button size="small" disabled={!state.available || state.busy} onclick={() => void client.prepare()}>
      Download for offline use
    </Button>
    <Button size="small" disabled={!state.available || state.busy} onclick={() => void client.checkUpdate()}>
      Check for updates
    </Button>
    <a href="./licenses.html" class="text-flow underline" target="_blank" rel="noopener noreferrer">
      Licenses and source
    </a>
  </div>
  {#if state.update}<p class="mt-2 mb-0" role="status">
      An update is ready. Finish your work, close all tabs for this app, then reopen it. Running searches will not be
      interrupted.
    </p>{/if}
  {#if state.error}<p class="mt-2 mb-0" role="alert">{state.error}</p>{/if}
  <p class="text-muted mt-2 mb-0">
    Offline files and history use separate storage. The browser can evict either. Keep exported history backups.
  </p>
</section>
