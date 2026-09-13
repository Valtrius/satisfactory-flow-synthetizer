<script lang="ts">
  import {
    Background,
    BackgroundVariant,
    ControlButton,
    Controls,
    SvelteFlow,
    type Edge,
    type Node,
  } from '@xyflow/svelte';
  import HelpPopover from './ui/HelpPopover.svelte';
  import Lock from '@lucide/svelte/icons/lock';
  import LockOpen from '@lucide/svelte/icons/lock-open';
  import Network from '@lucide/svelte/icons/network';
  import Activity from '@lucide/svelte/icons/activity';
  import Info from '@lucide/svelte/icons/info';
  import X from '@lucide/svelte/icons/x';
  import type { Snippet } from 'svelte';
  import type { Solution } from '../types';
  import type { Writable } from 'svelte/store';
  import { GRAPH_SNAP_GRID } from './graph';
  import GraphToolbar, { type GraphEditingControls } from './GraphToolbar.svelte';
  import FitViewController from './FitViewController.svelte';
  import Button from './ui/Button.svelte';
  import Panel from './ui/Panel.svelte';
  import Popup from './ui/Popup.svelte';
  import SolutionSummary from './SolutionSummary.svelte';
  import FactoryNode from './FactoryNode.svelte';

  type Props = {
    nodes: Writable<Node[]>;
    edges: Writable<Edge[]>;
    fitRevision: number;
    fullscreen?: boolean;
    /** Omit editing controls for a graph preview. */
    editing?: GraphEditingControls;
    class?: string;
    canvasClass?: string;
    solution?: Solution | null;
    elapsedLabel?: string;
    telemetry?: Snippet;
    empty?: Snippet;
    onFlowError: (id: string, message: string) => void;
    onNodeDragStart?: () => void;
    onNodeDragStop?: () => void;
  };

  let {
    nodes: nodesStore,
    edges: edgesStore,
    fitRevision,
    fullscreen = false,
    editing,
    class: className = '',
    canvasClass = 'flow-wrap min-h-0 w-full flex-1 bg-[#08141c]',
    solution = null,
    elapsedLabel,
    telemetry,
    empty,
    onFlowError,
    onNodeDragStart,
    onNodeDragStop,
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
  let infoOpen = $state(true);
  let telemetryOpen = $state(false);
  let toolbarHeight = $state(0);
  const telemetryId = $props.id();

  const nodeTypes = { factory: FactoryNode };

  let canvasWidth = $state(0);
  let canvasHeight = $state(0);
  let resizeRevision = $state(0);
  $effect(() => {
    if (!canvasWidth || !canvasHeight) return;
    // Refit after panel toggles/resizing settle, without changing the saved node positions.
    const timer = setTimeout(() => (resizeRevision += 1), 120);
    return () => clearTimeout(timer);
  });

  const helpSections = [
    {
      title: 'Navigate',
      items: [
        'Drag the background to pan; scroll or pinch to zoom.',
        'Use the bottom-right controls for the same.',
        'Press F to expand or restore the graph.',
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

<div class={`@container/graph flex flex-col overflow-hidden ${className}`} class:relative={!fullscreen}>
  <div
    bind:clientHeight={toolbarHeight}
    class="border-line flex flex-col items-start justify-between gap-3 border-b px-5 py-2 @min-[800px]/graph:flex-row @min-[800px]/graph:items-center"
  >
    <div class="relative flex max-w-full flex-wrap items-center gap-2">
      <h3 class="m-0 flex items-center gap-2 text-lg font-bold tracking-tight">
        <Network class="text-accent size-[1.05rem] shrink-0" strokeWidth={2.2} aria-hidden="true" />
        Topology graph
      </h3>
      {#if editing}
        <HelpPopover
          label="Graph controls help"
          sections={helpSections}
          align="left"
          anchor="parent"
          class="@min-[800px]/graph:relative"
          popupClass="@min-[800px]/graph:max-w-none"
        />
      {/if}
      {#if telemetry}
        <Button
          variant="quiet"
          size="tiny"
          class="flex items-center gap-1.5"
          title="Telemetry"
          aria-label="Telemetry"
          aria-haspopup="dialog"
          aria-expanded={telemetryOpen}
          aria-controls={telemetryId}
          onclick={() => (telemetryOpen = !telemetryOpen)}
        >
          <Activity size={15} aria-hidden="true" />
          <span class="hidden @min-[480px]/graph:inline">Telemetry</span>
        </Button>
      {/if}
    </div>
    <div
      class="flex w-full flex-wrap items-start justify-between gap-4 @min-[800px]/graph:w-auto @min-[800px]/graph:items-center"
    >
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
      {#if editing}
        <GraphToolbar {editing} {fullscreen} hasNodes={nodes.length > 0} />
      {/if}
    </div>
  </div>
  <div
    class={`relative ${canvasClass}`}
    aria-label="Interactive topology layout"
    bind:clientWidth={canvasWidth}
    bind:clientHeight={canvasHeight}
  >
    {#if empty && !solution}
      <div class="flex h-full min-h-0 flex-col">{@render empty()}</div>
    {:else}
      <SvelteFlow
        bind:nodes={readNodes, writeNodes}
        bind:edges={readEdges, writeEdges}
        {nodeTypes}
        {snapGrid}
        nodesDraggable={interactive && Boolean(editing)}
        nodesConnectable={false}
        deleteKey={null}
        onbeforedelete={async () => false}
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
        <FitViewController revision={fitRevision + resizeRevision} />
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
      </SvelteFlow>
    {/if}
    {#if solution}
      <div
        class="absolute top-3 left-3 z-10 max-h-[calc(100%-1.5rem)] max-w-[calc(100%-1.5rem)] overflow-y-auto"
        class:rounded-panel={infoOpen}
      >
        {#if infoOpen}
          <SolutionSummary {solution} {elapsedLabel} onclose={() => (infoOpen = false)} />
        {:else}
          <Button
            variant="quiet"
            size="small"
            class="flex items-center gap-1.5"
            aria-label="Show graph info"
            title="Show graph info"
            onclick={() => (infoOpen = true)}
          >
            <Info size={15} aria-hidden="true" /> Graph info
          </Button>
        {/if}
      </div>
    {/if}
  </div>
  {#if telemetryOpen && telemetry}
    <div
      id={telemetryId}
      class="pointer-events-none absolute inset-x-3 bottom-3 z-40"
      style:top={`${toolbarHeight + 8}px`}
    >
      <Panel class="pointer-events-auto max-h-full w-[min(48rem,100%)] overflow-auto">
        <Popup label="Telemetry" onclose={() => (telemetryOpen = false)} class="@container/telemetry">
          <div class="border-line flex items-center justify-between border-b px-4 py-2">
            <h3 class="m-0 text-sm font-bold">Telemetry</h3>
            <Button
              variant="quiet"
              size="tiny"
              square
              aria-label="Close telemetry"
              title="Close telemetry"
              onclick={() => (telemetryOpen = false)}
            >
              <X size={14} aria-hidden="true" />
            </Button>
          </div>
          {@render telemetry()}
        </Popup>
      </Panel>
    </div>
  {/if}
</div>
