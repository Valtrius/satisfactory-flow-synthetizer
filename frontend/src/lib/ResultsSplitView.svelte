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
    solutions: Solution[];
    selectedIndex: number;
    sortColumns: SortColumn[];
    selectedSolution: Solution;
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
    solutions,
    selectedIndex,
    sortColumns,
    selectedSolution,
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
    onNodeDragStop
  }: Props = $props();

  const graphSubtitle = $derived.by(() => {
    const linkCount =
      selectedSolution.stats.linkCount ?? selectedSolution.stats.beltCount ?? '—';
    const peak = selectedSolution.stats.internalMaxThroughput?.exact;
    return peak != null
      ? `Selected: ${linkCount} belts · peak ${peak} · ${selectedSolution.stats.feedbackLoops} feedback${selectedSolution.stats.feedbackLoops === 1 ? '' : 's'}`
      : `Selected: ${linkCount} links · ${selectedSolution.stats.feedbackLoops} feedback${selectedSolution.stats.feedbackLoops === 1 ? '' : 's'}`;
  });
</script>

<section class="mt-4" aria-label="Search results">
  <Panel class="overflow-hidden">
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
      borderBottom
    />

    <div
      class="grid grid-cols-1 items-stretch xl:h-[68vh] xl:min-h-107.5 xl:grid-cols-[minmax(280px,360px)_minmax(0,1fr)]"
    >
      <div
        class="h-[68vh] min-h-107.5 border-b border-line xl:h-full xl:min-h-0 xl:border-r xl:border-b-0"
      >
        <SolutionsTable
          {solutions}
          {selectedIndex}
          columns={sortColumns}
          {onSelect}
          {onColumnsChange}
        />
      </div>
      <TopologyGraphPanel
        {nodes}
        {edges}
        {fitRevision}
        {fullscreen}
        subtitle={graphSubtitle}
        class={`h-[68vh] min-h-107.5 xl:h-full xl:min-h-0 ${
          fullscreen ? 'fixed inset-0 z-100 !h-dvh !min-h-0 bg-[#08141c]' : ''
        }`}
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
