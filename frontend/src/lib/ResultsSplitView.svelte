<script lang="ts">
  import type { Edge, Node } from '@xyflow/svelte';
  import type { Writable } from 'svelte/store';
  import type { Solution } from '../types';
  import type { RotateDirection } from './graph';
  import type { SearchStageView } from './searchStage';
  import type { SortColumn } from './solutionSort';
  import SearchTelemetry from './SearchTelemetry.svelte';
  import SolutionsTable from './SolutionsTable.svelte';
  import TopologyGraphPanel from './TopologyGraphPanel.svelte';
  import Panel from './ui/Panel.svelte';

  type Props = {
    searchView: SearchStageView;
    searchStageMuted: boolean;
    busy: boolean;
    elapsedLabel: string;
    headline: string;
    subline: string;
    sizeBody: string;
    foundCount: number;
    showFound: boolean;
    showDetails: boolean;
    telemetryExpanded: boolean;
    onToggleTelemetry: () => void;
    solutions: Solution[];
    selectedIndex: number;
    sortColumns: SortColumn[];
    nodes: Writable<Node[]>;
    edges: Writable<Edge[]>;
    fitRevision: number;
    fullscreen: boolean;
    onSelect: (displayIndex: number) => void;
    onColumnsChange: (columns: SortColumn[]) => void;
    onRotate: (direction: RotateDirection) => void;
    canUndo: boolean;
    canRedo: boolean;
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
    searchView,
    searchStageMuted,
    busy,
    elapsedLabel,
    headline,
    subline,
    sizeBody,
    foundCount,
    showFound,
    showDetails,
    telemetryExpanded,
    onToggleTelemetry,
    solutions,
    selectedIndex,
    sortColumns,
    nodes,
    edges,
    fitRevision,
    fullscreen,
    onSelect,
    onColumnsChange,
    onRotate,
    canUndo,
    canRedo,
    onUndo,
    onRedo,
    onReset,
    onExport,
    onToggleFullscreen,
    onFlowError,
    onNodeDragStart,
    onNodeDragStop,
  }: Props = $props();
</script>

<section class="flex min-h-112 flex-1 flex-col overflow-hidden" aria-label="Search results">
  <Panel class="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
    <div class="shrink-0">
      <SearchTelemetry
        {searchView}
        muted={searchStageMuted}
        {busy}
        {elapsedLabel}
        {headline}
        {subline}
        {sizeBody}
        {foundCount}
        {showFound}
        {showDetails}
        collapsible
        detailsExpanded={telemetryExpanded}
        onToggleDetails={onToggleTelemetry}
        borderBottom
      />
    </div>

    <div
      class="grid min-h-0 flex-1 grid-cols-1 items-stretch max-xl:grid-rows-[minmax(16rem,50vh)_minmax(16rem,50vh)] xl:grid-cols-[minmax(280px,360px)_minmax(0,1fr)] xl:grid-rows-[minmax(0,1fr)]"
    >
      <div class="border-line h-full min-h-0 overflow-hidden border-b xl:border-r xl:border-b-0">
        <SolutionsTable {solutions} {selectedIndex} columns={sortColumns} {onSelect} {onColumnsChange} />
      </div>
      <TopologyGraphPanel
        {nodes}
        {edges}
        {fitRevision}
        {fullscreen}
        class={`h-full min-h-0 ${fullscreen ? 'fixed inset-0 z-100 !h-dvh !min-h-0 bg-[#08141c]' : ''}`}
        {onRotate}
        {canUndo}
        {canRedo}
        {onUndo}
        {onRedo}
        {onReset}
        {onExport}
        {onToggleFullscreen}
        {onFlowError}
        {onNodeDragStart}
        {onNodeDragStop}
      />
    </div>
  </Panel>
</section>
