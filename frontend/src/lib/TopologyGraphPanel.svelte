<script lang="ts">
  import {
    Background,
    BackgroundVariant,
    Controls,
    MiniMap,
    SvelteFlow,
    type Edge,
    type Node
  } from '@xyflow/svelte';
  import Download from '@lucide/svelte/icons/download';
  import Maximize2 from '@lucide/svelte/icons/maximize-2';
  import Minimize2 from '@lucide/svelte/icons/minimize-2';
  import Redo2 from '@lucide/svelte/icons/redo-2';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import type { Writable } from 'svelte/store';
  import type { RotateDirection } from './graph';
  import FitViewController from './FitViewController.svelte';
  import Button from './ui/Button.svelte';
  import FactoryNode from './FactoryNode.svelte';

  type Props = {
    nodes: Writable<Node[]>;
    edges: Writable<Edge[]>;
    fitRevision: number;
    fullscreen: boolean;
    subtitle: string;
    canUndo: boolean;
    canRedo: boolean;
    class?: string;
    canvasClass?: string;
    onRotate: (direction: RotateDirection) => void;
    onUndo: () => void;
    onRedo: () => void;
    onReset: () => void;
    onExport: () => void;
    onToggleFullscreen: () => void;
    onFlowError: (id: string, message: string) => void;
    onNodeDragStart?: () => void;
    onNodeDragStop?: () => void;
  };

  let {
    nodes: nodesStore,
    edges: edgesStore,
    fitRevision,
    fullscreen,
    subtitle,
    canUndo,
    canRedo,
    class: className = '',
    canvasClass = 'flow-wrap min-h-0 w-full flex-1 bg-[#08141c]',
    onRotate,
    onUndo,
    onRedo,
    onReset,
    onExport,
    onToggleFullscreen,
    onFlowError,
    onNodeDragStart,
    onNodeDragStop
  }: Props = $props();

  // Svelte 5 does not auto-subscribe `$store` on props. Bridge writables → $state.raw
  // so SvelteFlow 1.x bind:nodes/edges can work.
  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);

  $effect(() => {
    const unsub = nodesStore.subscribe((value) => {
      nodes = value;
    });
    return unsub;
  });
  $effect(() => {
    const unsub = edgesStore.subscribe((value) => {
      edges = value;
    });
    return unsub;
  });

  function readNodes(): Node[] {
    return nodes;
  }
  function writeNodes(value: Node[]): void {
    nodes = value;
    nodesStore.set(value);
  }
  function readEdges(): Edge[] {
    return edges;
  }
  function writeEdges(value: Edge[]): void {
    edges = value;
    edgesStore.set(value);
  }

  const nodeTypes = { factory: FactoryNode };
</script>

<div class={`flex flex-col overflow-hidden ${className}`}>
  <div
    class="flex flex-col items-start justify-between gap-3 border-b border-line px-5 py-2 md:flex-row md:items-center"
  >
    <div>
      <h3 class="mb-1 text-lg font-bold">Topology graph</h3>
      <p class="m-0 text-xs text-muted">{subtitle}</p>
    </div>
    <div class="flex w-full items-start justify-between gap-4 md:w-auto md:items-center">
      <div class="flex flex-wrap gap-4.5 text-xs text-muted" aria-label="Graph legend">
        <span class="flex items-center gap-1.5"
          ><i class="block h-0.75 w-5.5 bg-flow"></i> Main flow</span
        >
        <span class="flex items-center gap-1.5"
          ><i class="legend-line-feedback block h-0.75 w-5.5"></i> Feedback</span
        >
        <span class="flex items-center gap-1.5"
          ><i class="legend-line-discard block h-0.75 w-5.5"></i> Discard</span
        >
      </div>
      <div class="flex flex-col items-end gap-1">
        <div class="flex gap-1.5" aria-label="Graph tools">
          <Button
            size="small"
            square
            type="button"
            title="Rotate graph 90° counter-clockwise"
            aria-label="Rotate graph 90 degrees counter-clockwise"
            onclick={() => onRotate('ccw')}
          >↺</Button>
          <Button
            size="small"
            square
            type="button"
            title="Rotate graph 90° clockwise"
            aria-label="Rotate graph 90 degrees clockwise"
            onclick={() => onRotate('cw')}
          >↻</Button>
          <Button
            size="small"
            square
            type="button"
            title="Export SVG"
            aria-label="Export SVG"
            onclick={onExport}
          >
            <Download size={16} strokeWidth={2.2} aria-hidden="true" />
          </Button>
          <Button
            size="small"
            square
            type="button"
            title={fullscreen ? 'Exit expanded graph' : 'Expand graph'}
            aria-label={fullscreen ? 'Exit expanded graph' : 'Expand graph'}
            onclick={onToggleFullscreen}
          >
            {#if fullscreen}
              <Minimize2 size={16} strokeWidth={2.2} aria-hidden="true" />
            {:else}
              <Maximize2 size={16} strokeWidth={2.2} aria-hidden="true" />
            {/if}
          </Button>
        </div>
        <div class="flex gap-1.5" aria-label="Graph edit history">
          <Button
            size="small"
            square
            type="button"
            title="Undo"
            aria-label="Undo last graph change"
            disabled={!canUndo}
            onclick={onUndo}
          >
            <Undo2 size={16} strokeWidth={2.2} aria-hidden="true" />
          </Button>
          <Button
            size="small"
            square
            type="button"
            title="Redo"
            aria-label="Redo last undone graph change"
            disabled={!canRedo}
            onclick={onRedo}
          >
            <Redo2 size={16} strokeWidth={2.2} aria-hidden="true" />
          </Button>
          <Button
            size="small"
            square
            type="button"
            title="Reset layout"
            aria-label="Reset graph to default positions"
            onclick={onReset}
          >
            <RotateCcw size={16} strokeWidth={2.2} aria-hidden="true" />
          </Button>
        </div>
      </div>
    </div>
  </div>
  <div class={canvasClass} aria-label="Interactive topology layout">
    <SvelteFlow
      bind:nodes={readNodes, writeNodes}
      bind:edges={readEdges, writeEdges}
      {nodeTypes}
      fitView
      minZoom={0.1}
      maxZoom={2}
      onflowerror={onFlowError}
      onnodedragstart={() => onNodeDragStart?.()}
      onnodedragstop={() => onNodeDragStop?.()}
    >
      <FitViewController revision={fitRevision} />
      <Background variant={BackgroundVariant.Dots} gap={24} size={1.5} patternColor="#314252" />
      <Controls position="bottom-right" />
      <MiniMap position="bottom-left" pannable zoomable />
    </SvelteFlow>
  </div>
</div>
