<script lang="ts">
  import {
    Background,
    BackgroundVariant,
    ControlButton,
    Controls,
    MiniMap,
    SvelteFlow,
    type Edge,
    type Node,
  } from '@xyflow/svelte';
  import CircleHelp from '@lucide/svelte/icons/circle-help';
  import Download from '@lucide/svelte/icons/download';
  import Lock from '@lucide/svelte/icons/lock';
  import LockOpen from '@lucide/svelte/icons/lock-open';
  import Maximize2 from '@lucide/svelte/icons/maximize-2';
  import Minimize2 from '@lucide/svelte/icons/minimize-2';
  import Network from '@lucide/svelte/icons/network';
  import Redo2 from '@lucide/svelte/icons/redo-2';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import type { Writable } from 'svelte/store';
  import { GRAPH_SNAP_GRID, type RotateDirection } from './graph';
  import FitViewController from './FitViewController.svelte';
  import Button from './ui/Button.svelte';
  import FactoryNode from './FactoryNode.svelte';

  type Props = {
    nodes: Writable<Node[]>;
    edges: Writable<Edge[]>;
    fitRevision: number;
    fullscreen: boolean;
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
    onNodeDragStop,
  }: Props = $props();

  // Svelte 5 does not auto-subscribe `$store` on props. Bridge writables → $state.raw
  // so SvelteFlow 1.x bind:nodes/edges can work.
  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);
  let helpOpen = $state(false);
  let helpRoot: HTMLDivElement | undefined = $state();

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

  $effect(() => {
    if (!helpOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (helpRoot && !helpRoot.contains(event.target as HTMLElement)) helpOpen = false;
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') helpOpen = false;
    };
    window.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('keydown', onKeyDown);
    };
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

  const SNAP_GRID: [number, number] = [GRAPH_SNAP_GRID, GRAPH_SNAP_GRID];

  /** Hold Alt while dragging to place freely; snap is on otherwise. */
  let freePlacement = $state(false);

  $effect(() => {
    const syncAlt = (event: KeyboardEvent) => {
      freePlacement = event.altKey;
    };
    const clearAlt = () => {
      freePlacement = false;
    };
    window.addEventListener('keydown', syncAlt);
    window.addEventListener('keyup', syncAlt);
    window.addEventListener('blur', clearAlt);
    return () => {
      window.removeEventListener('keydown', syncAlt);
      window.removeEventListener('keyup', syncAlt);
      window.removeEventListener('blur', clearAlt);
    };
  });

  const snapGrid = $derived<[number, number] | undefined>(freePlacement ? undefined : SNAP_GRID);

  /** Prop-driven lock: Controls' built-in write to store.nodesDraggable is ignored ($derived from props). */
  let interactive = $state(true);

  const nodeTypes = { factory: FactoryNode };

  const helpSections = [
    {
      title: 'Navigate',
      items: [
        'Drag the background to pan; scroll or pinch to zoom.',
        'Use the bottom-right controls or the minimap for the same.',
      ],
    },
    {
      title: 'Move nodes',
      items: [
        'Drag a node to move it; placement snaps to the grid.',
        'Hold Alt while dragging for free placement.',
        'Shift-drag on the background for a selection box.',
        'Ctrl-click to multi-select.',
      ],
    },
    {
      title: 'Edit',
      items: [
        'Select a splitter or merger to rearrange its ports (swap sides or rotate).',
        'Toolbar: rotate the whole graph, undo/redo, reset layout, export SVG, or expand.',
        'Use the lock control to disable selecting and dragging nodes (pan/zoom still work).',
        'Ctrl+Z undoes; Ctrl+Y or Ctrl+Shift+Z redoes.',
      ],
    },
  ] as const;
</script>

<div class={`flex flex-col overflow-hidden ${className}`}>
  <div
    class="border-line flex flex-col items-start justify-between gap-3 border-b px-5 py-2 md:flex-row md:items-center"
  >
    <div class="relative flex items-center gap-3" bind:this={helpRoot}>
      <h3 class="m-0 flex items-center gap-2 text-lg font-bold tracking-tight">
        <Network class="text-accent size-[1.05rem] shrink-0" strokeWidth={2.2} aria-hidden="true" />
        Topology graph
      </h3>
      <Button
        size="small"
        square
        variant="quiet"
        type="button"
        class="!min-h-7 !w-7"
        title="Graph controls help"
        aria-label="Graph controls help"
        aria-expanded={helpOpen}
        aria-haspopup="dialog"
        onclick={() => (helpOpen = !helpOpen)}
      >
        <CircleHelp size={16} strokeWidth={2.2} aria-hidden="true" />
      </Button>
      {#if helpOpen}
        <div
          class="border-line bg-panel absolute top-[calc(100%+0.35rem)] left-0 z-30 w-[min(22rem,calc(100vw-2.5rem))] rounded-lg border px-3.5 py-3 shadow-[0_14px_32px_rgb(0_0_0/45%)]"
          role="dialog"
          aria-label="Topology graph controls"
        >
          <div class="flex flex-col gap-3 text-xs leading-relaxed">
            {#each helpSections as section}
              <div>
                <p class="text-ink m-0 mb-1 font-bold tracking-wide uppercase">{section.title}</p>
                <ul class="text-muted m-0 list-disc space-y-1 pl-4">
                  {#each section.items as item}
                    <li>{item}</li>
                  {/each}
                </ul>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>
    <div class="flex w-full items-start justify-between gap-4 md:w-auto md:items-center">
      <div class="text-muted flex flex-wrap gap-4.5 text-xs" aria-label="Graph legend">
        <span class="flex items-center gap-1.5">
          <i class="bg-flow block h-0.75 w-5.5"></i>
          Main flow
        </span>
        <span class="flex items-center gap-1.5">
          <i class="legend-line-feedback block h-0.75 w-5.5"></i>
          Feedback
        </span>
        <span class="flex items-center gap-1.5">
          <i class="legend-line-discard block h-0.75 w-5.5"></i>
          Discard
        </span>
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
          >
            ↺
          </Button>
          <Button
            size="small"
            square
            type="button"
            title="Rotate graph 90° clockwise"
            aria-label="Rotate graph 90 degrees clockwise"
            onclick={() => onRotate('cw')}
          >
            ↻
          </Button>
          <Button size="small" square type="button" title="Export SVG" aria-label="Export SVG" onclick={onExport}>
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
      {snapGrid}
      nodesDraggable={interactive}
      nodesConnectable={interactive}
      elementsSelectable={interactive}
      proOptions={{ hideAttribution: true }}
      fitView
      minZoom={0.1}
      maxZoom={2}
      onflowerror={onFlowError}
      onnodedragstart={({ event }) => {
        freePlacement = event.altKey;
        onNodeDragStart?.();
      }}
      onnodedrag={({ event }) => {
        freePlacement = event.altKey;
      }}
      onnodedragstop={({ event }) => {
        freePlacement = event.altKey;
        onNodeDragStop?.();
      }}
    >
      <FitViewController revision={fitRevision} />
      <Background variant={BackgroundVariant.Dots} gap={GRAPH_SNAP_GRID} size={1.5} patternColor="#314252" />
      <Controls position="bottom-right" showLock={false}>
        <ControlButton
          class="svelte-flow__controls-interactive"
          onclick={() => (interactive = !interactive)}
          title={interactive ? 'Lock interactivity' : 'Unlock interactivity'}
          aria-label={interactive ? 'Lock interactivity' : 'Unlock interactivity'}
        >
          {#if interactive}
            <LockOpen size={16} strokeWidth={2.2} aria-hidden="true" />
          {:else}
            <Lock size={16} strokeWidth={2.2} aria-hidden="true" />
          {/if}
        </ControlButton>
      </Controls>
      <MiniMap position="bottom-left" pannable zoomable />
    </SvelteFlow>
  </div>
</div>
