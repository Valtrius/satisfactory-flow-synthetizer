<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { get, writable } from 'svelte/store';
  import type { Edge, Node } from '@xyflow/svelte';
  import type { Solution, SolveRequest } from '../../types';
  import { layoutSolution, rotateFlowGraph, type RotateDirection } from '../graph';
  import TopologyGraphPanel from '../TopologyGraphPanel.svelte';
  import Button from '../ui/Button.svelte';
  import { getPlatform } from '../platform';
  import { createSelectedShare, runShareWorker, type VerifiedShare } from './client';
  import { MAX_SHARE_BYTES, boundedText, inlineLink, readShareLocation } from './codec';

  type Props = {
    target?: { request: SolveRequest; solution: Solution };
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
  let viewer = $state(platform.shareViewerUrl ?? '');
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
  const title = $derived(target ? 'Share selected solution' : 'Open shared solution');

  function fail(value: unknown): void {
    if (!disposed) error = value instanceof Error ? value.message : String(value);
  }
  async function show(value: VerifiedShare, current: AbortController): Promise<void> {
    if (current.signal.aborted || disposed) return;
    verified = value;
    link = '';
    if (target && viewer && value.token) {
      try {
        link = inlineLink(viewer, value.token);
      } catch (error) {
        fail(error);
      }
    }
    if (!target) {
      const graph = await layoutSolution(value.solution);
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
        await show(await createSelectedShare(target.request, target.solution, current.signal), current);
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
  function refreshLink(): void {
    error = '';
    link = '';
    try {
      if (verified?.token) link = inlineLink(viewer, verified.token);
      else throw new Error('Export JSON for this solution.');
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
      await platform.files.saveText({
        contents: JSON.stringify(verified.share),
        fileName: 'selected-solution.json',
        format: 'json',
      });
    } catch (error) {
      fail(error);
    }
  }
  function rotate(direction: RotateDirection): void {
    const rotated = rotateFlowGraph(get(nodes), get(edges), direction);
    nodes.set(rotated.nodes);
    edges.set(rotated.edges);
    fitRevision++;
  }
  async function save(): Promise<void> {
    if (!verified || !canSave || busy) return;
    busy = true;
    try {
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
  class="border-line bg-panel text-ink m-auto max-h-[94dvh] w-[calc(100%-2rem)] max-w-5xl overflow-y-auto rounded-xl border p-5"
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
    One physical solution with its rates and belt capacity. Graph positions, search mode, history and proof claims are
    not shared.
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
    <p class="text-accent text-sm" role="status">
      Physical solution verified with exact arithmetic. Optimality and complete enumeration are not established by this
      link.
    </p>
    <p class="text-muted text-sm">
      {verified.solution.stats.nodeCount} operators, {verified.solution.stats.linkCount} operator belts. Output {verified
        .solution.totalOutput.exact} /min.
    </p>
    {#if !target}
      <TopologyGraphPanel
        preview
        {nodes}
        {edges}
        {fitRevision}
        fullscreen={false}
        canUndo={false}
        canRedo={false}
        canvasClass="flow-wrap h-80 w-full"
        onRotate={rotate}
        onUndo={() => {}}
        onRedo={() => {}}
        onReset={() => {
          if (verified && controller) void show(verified, controller);
        }}
        onExport={() => void exportJson()}
        onToggleFullscreen={() => {}}
        onFlowError={(_, message) => fail(message)}
      />
    {/if}
    <label class="mt-3 block text-sm">
      Browser viewer address
      <input
        aria-label="Browser viewer address"
        class="border-line bg-well mt-1 block w-full rounded border p-2 text-sm"
        bind:value={viewer}
        placeholder="https://your-host/satisfactory-flow-synthetizer/"
      />
    </label>
    <div class="mt-3 flex flex-wrap gap-2">
      <Button size="small" onclick={refreshLink}>Generate inline link</Button><Button
        size="small"
        onclick={() => void exportJson()}
      >
        Export solution JSON
      </Button>
    </div>
    <p class="text-muted text-xs">The link contains the solution. No server stores it.</p>
    {#if link}<label class="mt-3 block text-sm">
        Share link
        <textarea
          aria-label="Share link"
          readonly
          value={link}
          class="border-line bg-well mt-1 block min-h-20 w-full rounded border p-2 font-mono text-xs"></textarea>
      </label>
      <Button size="small" class="mt-2" onclick={() => void copy(link)}>Copy link</Button>{/if}
    {#if !target}<Button class="mt-4" variant="primary" onclick={() => void save()} disabled={!canSave || busy}>
        Save to history and view
      </Button>{/if}
  {/if}
  {#if error}<p role="alert" class="text-danger text-sm">{error}</p>{/if}
  {#if notice}<p role="status" class="text-muted text-sm">{notice}</p>{/if}
</dialog>

<style>
  dialog::backdrop {
    background: rgb(4 10 15 / 75%);
  }
</style>
