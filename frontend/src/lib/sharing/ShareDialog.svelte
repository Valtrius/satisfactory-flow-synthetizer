<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { get, writable } from 'svelte/store';
  import type { Edge, Node } from '@xyflow/svelte';
  import type { Solution, SolveRequest } from '../../types';
  import { layoutSolution } from '../graph';
  import TopologyGraphPanel from '../TopologyGraphPanel.svelte';
  import Button from '../ui/Button.svelte';
  import Copy from '@lucide/svelte/icons/copy';
  import Check from '@lucide/svelte/icons/check';
  import { getPlatform } from '../platform';
  import { createSelectedShare, runShareWorker, type VerifiedShare } from './client';
  import { MAX_SHARE_BYTES, boundedText, inlineLink, readShareLocation } from './codec';
  import { applyShareLayout, captureShareLayout, readShareLayout } from './layout';

  type Props = {
    target?: { request: SolveRequest; solution: Solution; nodes: Node[] };
    source?: string;
    canSave: boolean;
    onSave: (value: VerifiedShare) => Promise<void>;
    onclose: () => void;
  };
  let { target, source = '', canSave, onSave, onclose }: Props = $props();
  const platform = getPlatform();
  const id = $props.id();
  let dialog: HTMLDialogElement;
  let input = $state('');
  let verified = $state<VerifiedShare | null>(null);
  let link = $state('');
  let error = $state('');
  let notice = $state('');
  let busy = $state(false);
  let fitRevision = $state(0);
  const nodes = writable<Node[]>([]);
  const edges = writable<Edge[]>([]);
  let controller: AbortController | null = null;
  let disposed = false;
  const title = $derived(target ? 'Share solution' : 'Open shared solution');
  let backdropPressed = false;

  function isBackdrop(event: PointerEvent): boolean {
    const rect = dialog.getBoundingClientRect();
    return (
      event.target === dialog &&
      (event.clientX < rect.left ||
        event.clientX > rect.right ||
        event.clientY < rect.top ||
        event.clientY > rect.bottom)
    );
  }

  function fail(value: unknown): void {
    if (!disposed) error = value instanceof Error ? value.message : String(value);
  }
  async function show(value: VerifiedShare, current: AbortController): Promise<void> {
    if (current.signal.aborted || disposed) return;
    verified = value;
    link = '';
    if (platform.shareViewerUrl && value.token) {
      try {
        link = inlineLink(platform.shareViewerUrl, value.token);
      } catch (error) {
        fail(error);
      }
    }
    if (!target) {
      const graph = applyShareLayout(await layoutSolution(value.solution), value.layout);
      if (current.signal.aborted || disposed) return;
      nodes.set(graph.nodes);
      edges.set(graph.edges);
      fitRevision++;
    }
  }
  async function verifyInput(text: string): Promise<void> {
    controller?.abort();
    const current = new AbortController();
    controller = current;
    busy = true;
    error = '';
    notice = '';
    verified = null;
    link = '';
    nodes.set([]);
    edges.set([]);
    try {
      boundedText(text);
      let payload = text.trim();
      if (!payload.startsWith('{')) {
        const location = readShareLocation(payload);
        if (!location) throw new Error('Paste a selected-solution link or JSON file.');
        payload = location.token;
      }
      await show(await runShareWorker({ operation: 'open', source: payload }, current.signal), current);
    } catch (error) {
      if (!current.signal.aborted) fail(error);
    } finally {
      if (controller === current && !disposed) busy = false;
    }
  }
  async function initialize(): Promise<void> {
    try {
      if (target) {
        const current = new AbortController();
        controller = current;
        busy = true;
        const value = await createSelectedShare(target.request, target.solution, current.signal);
        if (current.signal.aborted || disposed) return;
        value.layout = readShareLayout(captureShareLayout(target.solution, target.nodes), value.solution);
        await show(value, current);
      } else if (source) await verifyInput(source);
    } catch (error) {
      fail(error);
    } finally {
      if (!disposed) busy = false;
    }
  }
  onMount(() => {
    input = source;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.showModal();
    void initialize();
    return () => {
      dialog.close();
      if (opener?.isConnected) opener.focus();
    };
  });
  onDestroy(() => {
    disposed = true;
    controller?.abort();
  });

  async function openFile(): Promise<void> {
    try {
      const text = await platform.files.openJsonText(MAX_SHARE_BYTES);
      if (text !== null && !disposed) {
        input = text;
        await verifyInput(text);
      }
    } catch (error) {
      fail(error);
    }
  }
  async function copy(value: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(value);
      notice = 'Link copied.';
    } catch {
      error = 'Clipboard access was denied. Select the link and copy it manually.';
    }
  }
  async function exportJson(): Promise<void> {
    if (!verified) return;
    try {
      const layout = target ? verified.layout : captureShareLayout(verified.solution, get(nodes));
      const contents = JSON.stringify({ ...verified.share, layout });
      boundedText(contents);
      await platform.files.saveText({
        contents,
        fileName: 'selected-solution.json',
        format: 'json',
      });
    } catch (error) {
      fail(error);
    }
  }
  async function save(): Promise<void> {
    if (!verified || !canSave || busy) return;
    busy = true;
    try {
      verified.layout = captureShareLayout(verified.solution, get(nodes));
      await onSave(verified);
      onclose();
    } catch (error) {
      fail(error);
    } finally {
      if (!disposed) busy = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby={id}
  class={`border-line bg-panel text-ink m-auto max-h-[94dvh] w-[calc(100%-2rem)] overflow-y-auto rounded-xl border p-5 backdrop:bg-[#040a0f]/75 ${target ? 'max-w-xl' : 'max-w-5xl'}`}
  onpointerdown={(event) => {
    backdropPressed = isBackdrop(event);
  }}
  onpointerup={(event) => {
    if (backdropPressed && isBackdrop(event)) onclose();
    backdropPressed = false;
  }}
  oncancel={(event) => {
    event.preventDefault();
    onclose();
  }}
>
  <div class="flex items-center justify-between gap-4">
    <h2 {id} class="m-0 text-lg font-bold">{title}</h2>
    <Button size="small" onclick={onclose}>Close</Button>
  </div>
  <p class="text-muted text-sm">
    {#if target}
      Links share the solution with automatic graph positioning. Export JSON to preserve your node positions and port
      orientations.
    {:else}
      Paste a share link or open a solution JSON file. Links use automatic graph positioning; JSON restores saved node
      positions and port orientations.
    {/if}
  </p>
  {#if !target}
    <label class="block text-sm">
      Selected-solution link or JSON
      <textarea
        aria-label="Selected-solution link or JSON"
        bind:value={input}
        class="border-line bg-well mt-2 block min-h-24 w-full rounded border p-2 font-mono text-xs"></textarea>
    </label>
    <div class="mt-2 flex gap-2">
      <Button size="small" onclick={() => void verifyInput(input)} disabled={busy}>Verify and view</Button><Button
        size="small"
        onclick={() => void openFile()}
        disabled={busy}
      >
        Open solution file
      </Button>
    </div>
  {/if}
  {#if busy}<p role="status" class="text-muted text-sm">Processing selected solution...</p>{/if}
  {#if verified}
    {#if !target}
      <p class="text-accent text-sm" role="status">
        Physical solution verified. Search proofs and history are not included.
      </p>
      <TopologyGraphPanel
        {nodes}
        {edges}
        {fitRevision}
        canvasClass="flow-wrap h-80 w-full"
        onFlowError={(_, message) => fail(message)}
      />
    {/if}
    {#if link}
      <label for={`${id}-link`} class="mt-3 block text-sm">Share link</label>
      <div
        class="border-field-border bg-well focus-within:outline-accent mt-1 flex min-w-0 items-center gap-1 rounded-md border p-1 focus-within:outline-2"
      >
        <input
          id={`${id}-link`}
          aria-label="Share link"
          readonly
          value={link}
          class="min-w-0 flex-1 truncate border-0 bg-transparent px-2 py-1 font-mono text-xs outline-none"
          onclick={(event) => event.currentTarget.select()}
        />
        <Button
          size="small"
          square
          variant="quiet"
          class="shrink-0"
          title="Copy link"
          aria-label="Copy link"
          onclick={() => void copy(link)}
        >
          {#if notice}<Check size={16} aria-hidden="true" />{:else}<Copy size={16} aria-hidden="true" />{/if}
        </Button>
      </div>
    {:else if !platform.shareViewerUrl}
      <p class="text-muted text-sm">Export JSON to share this solution.</p>
    {:else if !verified.token}
      <p class="text-muted text-sm">This solution is too large for a link. Export JSON instead.</p>
    {/if}
    <div class="mt-3 flex flex-wrap gap-2">
      <Button size="small" onclick={() => void exportJson()} disabled={busy}>Export solution JSON</Button>
    </div>
    {#if !target}<Button class="mt-4" variant="primary" onclick={() => void save()} disabled={!canSave || busy}>
        Save to history and view
      </Button>{/if}
  {/if}
  {#if error}<p role="alert" class="text-danger text-sm">{error}</p>{/if}
  {#if notice}<p role="status" class="text-muted text-sm">{notice}</p>{/if}
</dialog>
