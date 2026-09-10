<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { writable, type Writable } from 'svelte/store';
  import type { Edge, Node } from '@xyflow/svelte';
  import EmptyGraphState from './lib/EmptyGraphState.svelte';
  import ErrorBanner from './lib/ErrorBanner.svelte';
  import FlowInputsPanel from './lib/FlowInputsPanel.svelte';
  import HistoryPanel from './lib/HistoryPanel.svelte';
  import ResultsSplitView from './lib/ResultsSplitView.svelte';
  import SearchTelemetry from './lib/SearchTelemetry.svelte';
  import SolutionSummary from './lib/SolutionSummary.svelte';
  import TopologyGraphPanel from './lib/TopologyGraphPanel.svelte';
  import Panel from './lib/ui/Panel.svelte';
  import {
    MAX_ENDPOINTS,
    buildSolveRequest,
    clampMultiplier,
    createEndpointRow,
    endpointSlots,
    type EndpointCollection,
  } from './lib/endpoints';
  import { createGraphSession } from './lib/graphSession';
  import { exportHistoryBundle, exportHistoryEntry, importHistoryPayload } from './lib/historyIo';
  import { createPersistController, installCloseFlush, loadHistoryOrEmpty } from './lib/historyLifecycle';
  import {
    assembleEntries,
    createQueuedEntry,
    entryElapsedMs,
    entryToJobSnapshot,
    mergeImportedPayload,
    partitionEntries,
    reorderWithinBand,
    snapshotForm,
    type HistoryEntry,
  } from './lib/historyModel';
  import { HistoryQueue } from './lib/historyQueue';
  import { formatElapsed, searchHeadline, searchStageView, searchSubline, sizeSearchBody } from './lib/searchStage';
  import { DEFAULT_SORT_COLUMNS, compareSolutions, type SortColumn } from './lib/solutionSort';
  import { readUiPrefs, updateUiPrefs } from './lib/uiPrefs';
  import { enumeratesLayouts, type EndpointRow, type Solution } from './types';

  const savedForm = readUiPrefs().form;
  let nextEndpointId = $state(savedForm.nextEndpointId);
  let inputs = $state<EndpointRow[]>(savedForm.inputs.map((row) => ({ ...row })));
  let outputs = $state<EndpointRow[]>(savedForm.outputs.map((row) => ({ ...row })));
  let beltRate = $state(savedForm.beltRate);
  let solveMode = $state(savedForm.solveMode);

  let historyEntries = $state<HistoryEntry[]>([]);
  let selectedEntryId = $state<string | null>(null);
  let expandedTelemetryEntries = $state<Record<string, boolean>>({});
  const telemetryExpanded = $derived(selectedEntryId != null && (expandedTelemetryEntries[selectedEntryId] ?? false));

  function toggleTelemetry(): void {
    if (selectedEntryId == null) return;
    expandedTelemetryEntries[selectedEntryId] = !telemetryExpanded;
  }

  let solution = $state<Solution | null>(null);
  let solutions = $state<Solution[]>([]);
  let selectedSourceIndex = $state(0);
  let sortColumns = $state<SortColumn[]>([...DEFAULT_SORT_COLUMNS]);
  let errorMessage = $state('');
  let elapsedMs = $state(0);
  let historyReady = $state(false);
  let closing = $state(false);
  let runningTick = $state(Date.now());
  let graphFitRevision = $state(0);
  let graphFullscreen = $state(false);
  let canUndoGraph = $state(false);
  let canRedoGraph = $state(false);

  const flowNodes: Writable<Node[]> = writable([]);
  const flowEdges: Writable<Edge[]> = writable([]);

  function patchEntry(id: string, patch: Partial<HistoryEntry>): void {
    historyEntries = historyEntries.map((entry) =>
      entry.id === id ? { ...entry, ...patch, updatedAtMs: Date.now() } : entry,
    );
  }

  const graph = createGraphSession({
    nodes: flowNodes,
    edges: flowEdges,
    getSelectedEntryId: () => selectedEntryId,
    getSelectedEntry: () => historyEntries.find((entry) => entry.id === selectedEntryId) ?? null,
    getSelectedSourceIndex: () => selectedSourceIndex,
    setSelectedSourceIndex: (index) => {
      selectedSourceIndex = index;
    },
    getSolution: () => solution,
    setSolution: (next) => {
      solution = next;
    },
    getSolutions: () => solutions,
    setSolutions: (next) => {
      solutions = next;
    },
    getSortColumns: () => sortColumns,
    setSortColumns: (next) => {
      sortColumns = next;
    },
    getInputs: () => inputs,
    getOutputs: () => outputs,
    patchEntry,
    setError: (message) => {
      errorMessage = message;
    },
    onChromeChange: (chrome) => {
      graphFitRevision = chrome.fitRevision;
      graphFullscreen = chrome.fullscreen;
      canUndoGraph = chrome.canUndo;
      canRedoGraph = chrome.canRedo;
    },
  });

  const queue = new HistoryQueue({
    getEntries: () => historyEntries,
    setEntries: (entries) => {
      historyEntries = entries;
    },
    patchEntry,
    getSelectedId: () => selectedEntryId,
    flushSelectedChrome: () => graph.flushChrome(),
    syncViewIfSelected: (entryId, entry) => {
      if (selectedEntryId !== entryId) return;
      graph.syncLiveResults(entry);
    },
    setError: (message) => {
      errorMessage = message;
    },
    setElapsedMs: (ms) => {
      elapsedMs = ms;
    },
  });

  const persist = createPersistController({
    isReady: () => historyReady,
    onError: (message) => {
      errorMessage = message;
    },
  });

  const bands = $derived(partitionEntries(historyEntries));
  const selectedEntry = $derived(historyEntries.find((entry) => entry.id === selectedEntryId) ?? null);
  const hasRunning = $derived(bands.running != null);
  const viewJob = $derived(selectedEntry ? entryToJobSnapshot(selectedEntry) : null);
  const searchEnumerate = $derived(Boolean(selectedEntry && enumeratesLayouts(selectedEntry.request.solveMode)));
  const busy = $derived(selectedEntry?.status === 'running' || selectedEntry?.status === 'cancelling');
  const showSearchStage = $derived(
    Boolean(viewJob) && selectedEntry != null && selectedEntry.status !== 'queued' && (searchEnumerate || !solution),
  );
  const searchStageMuted = $derived(
    selectedEntry?.status === 'cancelled' ||
      selectedEntry?.status === 'failed' ||
      selectedEntry?.status === 'incomplete' ||
      selectedEntry?.status === 'unsat',
  );
  const searchView = $derived(searchStageView(viewJob?.progress ?? null));
  const inputSlots = $derived(endpointSlots(inputs));
  const outputSlots = $derived(endpointSlots(outputs));
  const showResultsTable = $derived(searchEnumerate && solutions.length > 0);
  const searchCopyContext = $derived({
    solutionsLength: solutions.length,
    searchEnumerate,
    firstNodeCount: solutions[0]?.stats.nodeCount ?? null,
  });
  const elapsedLabel = $derived(formatElapsed(elapsedMs));
  const runningElapsedLabel = $derived(
    bands.running?.startedAtMs ? formatElapsed(Math.max(0, runningTick - bands.running.startedAtMs)) : '',
  );
  const displayRows = $derived(
    solutions
      .map((item, sourceIndex) => ({ solution: item, sourceIndex }))
      .sort((left, right) => compareSolutions(left.solution, right.solution, sortColumns)),
  );
  const selectedDisplayIndex = $derived(displayRows.findIndex((row) => row.sourceIndex === selectedSourceIndex));

  onMount(() => {
    let unlistenClose: (() => void) | undefined;
    let disposed = false;

    void (async () => {
      try {
        const unlisten = await installCloseFlush({
          onClosing: () => {
            closing = true;
            queue.pause();
            persist.dispose();
          },
          prepare: () => queue.shutdown(),
          onReopen: async () => {
            try {
              await queue.resume();
            } finally {
              closing = false;
            }
          },
          flush: () => flushHistoryToDisk(),
          onError: (message) => {
            errorMessage = message;
          },
        });
        if (disposed) {
          unlisten();
          return;
        }
        unlistenClose = unlisten;
        const loaded = await loadHistoryOrEmpty();
        if (disposed) return;
        if (!loaded.ok) {
          errorMessage = loaded.error;
          return;
        }
        historyEntries = loaded.document.entries;
        selectedEntryId = loaded.document.selectedEntryId;
        historyReady = true;
        if (selectedEntryId) await hydrateViewFromEntry(selectedEntryId);
        if (!disposed) void queue.pump();
      } catch (error) {
        if (!disposed) errorMessage = `Could not initialize history: ${String(error)}`;
      }
    })();

    return () => {
      disposed = true;
      unlistenClose?.();
    };
  });

  $effect(() => {
    if (!closing) persist.schedule(historyEntries, selectedEntryId);
  });

  $effect(() => {
    updateUiPrefs({
      form: {
        inputs,
        outputs,
        beltRate,
        solveMode,

        nextEndpointId,
      },
    });
  });

  $effect(() => {
    if (!bands.running?.startedAtMs) return;
    const id = setInterval(() => {
      runningTick = Date.now();
    }, 100);
    return () => clearInterval(id);
  });

  $effect(() => {
    const entry = selectedEntry;
    if (!entry) {
      elapsedMs = 0;
      return;
    }
    elapsedMs = entryElapsedMs(entry);
  });

  async function hydrateViewFromEntry(id: string): Promise<void> {
    const entry = historyEntries.find((item) => item.id === id);
    if (!entry) {
      graph.clearView();
      sortColumns = [...DEFAULT_SORT_COLUMNS];
      return;
    }
    await graph.hydrateFromEntry(entry);
  }

  async function solve(): Promise<void> {
    if (!historyReady || closing) return;
    graph.setFullscreen(false);
    errorMessage = '';
    const request = buildSolveRequest(inputs, outputs, beltRate, solveMode);
    if (request.outputs.length < 1) {
      errorMessage = 'Add at least one output before solving.';
      return;
    }
    const entry = createQueuedEntry(snapshotForm(inputs, outputs, beltRate, solveMode), request);
    graph.flushChrome();
    // Newest queued jobs stack on top; the runner drains from the bottom.
    const parts = partitionEntries(historyEntries);
    const jobAlreadyRunning = parts.running != null;
    historyEntries = assembleEntries([entry, ...parts.queued], parts.running, parts.history);
    if (!jobAlreadyRunning) {
      selectedEntryId = entry.id;
      graph.clearView();
      sortColumns = [...DEFAULT_SORT_COLUMNS];
    }
    await queue.pump();
    if (!jobAlreadyRunning && selectedEntryId) await hydrateViewFromEntry(selectedEntryId);
  }

  async function selectHistoryEntry(id: string): Promise<void> {
    if (id === selectedEntryId) return;
    graph.flushChrome();
    selectedEntryId = id;
    await hydrateViewFromEntry(id);
  }

  function renameEntry(id: string, title: string | null): void {
    patchEntry(id, { title });
  }

  function deleteEntry(id: string): void {
    const entry = historyEntries.find((item) => item.id === id);
    if (!entry) return;
    if (entry.status === 'running' || entry.status === 'cancelling') return;
    historyEntries = historyEntries.filter((item) => item.id !== id);
    if (selectedEntryId === id) {
      const parts = partitionEntries(historyEntries);
      selectedEntryId = parts.running?.id ?? parts.history[0]?.id ?? parts.queued[0]?.id ?? null;
      if (selectedEntryId) void hydrateViewFromEntry(selectedEntryId);
      else {
        graph.clearView();
        sortColumns = [...DEFAULT_SORT_COLUMNS];
      }
    }
  }

  async function deleteAllHistory(): Promise<void> {
    if (!historyReady || closing) return;
    graph.flushChrome();
    const running = partitionEntries(historyEntries).running;
    if (running?.jobId) {
      try {
        await queue.cancel();
      } catch {
        /* best-effort; still wipe local history */
      }
    }
    queue.reset();
    historyEntries = [];
    selectedEntryId = null;
    graph.clearView();
    sortColumns = [...DEFAULT_SORT_COLUMNS];
    try {
      await persist.flushNow([], null);
    } catch (error) {
      errorMessage = `Could not clear history: ${error instanceof Error ? error.message : String(error)}`;
    }
  }

  function copyEntryToForm(id: string): void {
    const entry = historyEntries.find((item) => item.id === id);
    if (!entry) return;
    inputs = entry.form.inputs.map((row) => ({ ...row }));
    outputs = entry.form.outputs.map((row) => ({ ...row }));
    beltRate = entry.form.beltRate;
    solveMode = entry.form.solveMode;

    const maxId = [...inputs, ...outputs]
      .map((row) => Number(String(row.id).replace(/\D+/g, '')) || 0)
      .reduce((max, value) => Math.max(max, value), nextEndpointId);
    nextEndpointId = maxId + 1;
  }

  async function exportEntry(id: string): Promise<void> {
    graph.flushChrome();
    const entry = historyEntries.find((item) => item.id === id);
    if (!entry) return;
    try {
      await exportHistoryEntry(entry);
    } catch (error) {
      errorMessage = `Export failed: ${error instanceof Error ? error.message : String(error)}`;
    }
  }

  async function exportAll(): Promise<void> {
    graph.flushChrome();
    try {
      await exportHistoryBundle(historyEntries);
    } catch (error) {
      errorMessage = `Export failed: ${error instanceof Error ? error.message : String(error)}`;
    }
  }

  async function importHistory(): Promise<void> {
    if (!historyReady || closing) return;
    try {
      const payload = await importHistoryPayload();
      if (payload == null) return;
      const { entries, importedIds } = mergeImportedPayload(historyEntries, payload);
      if (importedIds.length === 0) {
        errorMessage = 'No history entries found in that file.';
        return;
      }
      historyEntries = entries;
      selectedEntryId = importedIds[0] ?? selectedEntryId;
      if (selectedEntryId) await hydrateViewFromEntry(selectedEntryId);
    } catch (error) {
      errorMessage = `Import failed: ${error instanceof Error ? error.message : String(error)}`;
    }
  }

  function updateEndpoint(
    collection: EndpointCollection,
    index: number,
    field: 'rate' | 'multiplier',
    value: string,
  ): void {
    const source = collection === 'inputs' ? inputs : outputs;
    const updated = source.map((item, itemIndex) => (itemIndex === index ? { ...item, [field]: value } : item));
    if (collection === 'inputs') inputs = updated;
    else outputs = updated;
  }

  function commitMultiplier(collection: EndpointCollection, index: number): void {
    const source = collection === 'inputs' ? inputs : outputs;
    const current = source[index];
    if (!current) return;
    const next = clampMultiplier(current.multiplier);
    if (next === current.multiplier) return;
    updateEndpoint(collection, index, 'multiplier', next);
  }

  function addEndpoint(collection: EndpointCollection): void {
    const slots = collection === 'inputs' ? inputSlots : outputSlots;
    if (slots >= MAX_ENDPOINTS) return;
    const endpoint = createEndpointRow(collection, nextEndpointId++);
    if (collection === 'inputs') inputs = [...inputs, endpoint];
    else outputs = [...outputs, endpoint];
  }

  function removeEndpoint(collection: EndpointCollection, index: number): void {
    if (collection === 'inputs') inputs = inputs.filter((_, itemIndex) => itemIndex !== index);
    else outputs = outputs.filter((_, itemIndex) => itemIndex !== index);
  }

  function handleFlowError(id: string, message: string): void {
    errorMessage = `The factory graph could not render an edge (${id}): ${message}`;
  }

  async function flushHistoryToDisk(): Promise<void> {
    graph.flushChrome();
    await persist.flushNow(historyEntries, selectedEntryId);
  }

  onDestroy(() => {
    queue.dispose();
    persist.dispose();
    document.body.classList.remove('graph-expanded');
    if (historyReady && !closing) {
      graph.flushChrome();
      void persist.flushNow(historyEntries, selectedEntryId).catch(() => {
        /* close path already tried; avoid noisy teardown errors */
      });
    }
  });
</script>

<svelte:window onkeydown={graph.handleKeydown} />

<svelte:head>
  <title>Satisfactory Flow Synthetizer</title>
  <meta
    name="description"
    content="Exact Satisfactory splitter and merger flow synthetizer with an exact cvc5 solver."
  />
</svelte:head>

<div class="min-h-screen">
  <main class="mx-auto w-full max-w-[1680px] p-4">
    <div class="grid grid-cols-1 items-start gap-4 xl:grid-cols-[minmax(240px,280px)_minmax(0,1fr)]">
      <div
        class="xl:sticky xl:top-4 xl:h-[calc(100dvh-2rem)]"
        inert={!historyReady || closing}
        aria-busy={!historyReady}
      >
        <HistoryPanel
          queued={bands.queued}
          running={bands.running}
          history={bands.history}
          {selectedEntryId}
          {runningElapsedLabel}
          onSelect={(id) => void selectHistoryEntry(id)}
          onReorderQueued={(fromId, toId) => {
            historyEntries = reorderWithinBand(historyEntries, 'queued', fromId, toId);
          }}
          onReorderHistory={(fromId, toId) => {
            historyEntries = reorderWithinBand(historyEntries, 'history', fromId, toId);
          }}
          onRename={renameEntry}
          onDelete={deleteEntry}
          onCancelRunning={() => void queue.cancel()}
          onCopyToNew={copyEntryToForm}
          onExportEntry={(id) => void exportEntry(id)}
          onExportAll={() => void exportAll()}
          onImport={() => void importHistory()}
          onDeleteAll={() => void deleteAllHistory()}
        />
      </div>

      <div
        class={`flex min-w-0 flex-col gap-4 ${
          showResultsTable ? 'xl:h-[calc(100dvh-2rem)]' : solution ? 'xl:h-[calc(100dvh-2rem)]' : ''
        }`}
      >
        <FlowInputsPanel
          {inputs}
          {outputs}
          {inputSlots}
          {outputSlots}
          bind:beltRate
          {solveMode}
          {hasRunning}
          ready={historyReady && !closing}
          onAddInput={() => addEndpoint('inputs')}
          onRemoveInput={(index) => removeEndpoint('inputs', index)}
          onUpdateInput={(index, field, value) => updateEndpoint('inputs', index, field, value)}
          onCommitInputMultiplier={(index) => commitMultiplier('inputs', index)}
          onAddOutput={() => addEndpoint('outputs')}
          onRemoveOutput={(index) => removeEndpoint('outputs', index)}
          onUpdateOutput={(index, field, value) => updateEndpoint('outputs', index, field, value)}
          onCommitOutputMultiplier={(index) => commitMultiplier('outputs', index)}
          onSolveModeChange={(value) => {
            solveMode = value;
          }}
          onSolve={() => void solve()}
        />

        {#if errorMessage}
          <ErrorBanner message={errorMessage} />
        {/if}

        {#if showSearchStage && viewJob && !showResultsTable}
          <section class="shrink-0" aria-labelledby="search-stage-title">
            <Panel class="overflow-hidden">
              <SearchTelemetry
                {searchView}
                muted={searchStageMuted}
                {busy}
                {elapsedLabel}
                headline={searchHeadline(viewJob, searchCopyContext)}
                subline={searchSubline(viewJob, searchView, searchCopyContext)}
                sizeBody={sizeSearchBody(viewJob, searchView)}
                foundCount={solutions.length}
                showFound={searchEnumerate}
                showDetails={searchEnumerate || busy || Boolean(viewJob.progress)}
                collapsible={searchEnumerate}
                detailsExpanded={telemetryExpanded}
                onToggleDetails={toggleTelemetry}
              />
            </Panel>
          </section>
        {/if}

        {#if showResultsTable && solution && viewJob}
          <ResultsSplitView
            {searchView}
            {searchStageMuted}
            {busy}
            {elapsedLabel}
            headline={searchHeadline(viewJob, searchCopyContext)}
            subline={searchSubline(viewJob, searchView, searchCopyContext)}
            sizeBody={sizeSearchBody(viewJob, searchView)}
            foundCount={solutions.length}
            showFound={searchEnumerate}
            showDetails
            {telemetryExpanded}
            onToggleTelemetry={toggleTelemetry}
            solutions={displayRows.map((row) => row.solution)}
            selectedIndex={Math.max(0, selectedDisplayIndex)}
            {sortColumns}
            nodes={flowNodes}
            edges={flowEdges}
            fitRevision={graphFitRevision}
            fullscreen={graphFullscreen}
            onSelect={(displayIndex) => {
              const row = displayRows[displayIndex];
              if (row) void graph.selectSolution(row.sourceIndex);
            }}
            onColumnsChange={(next) => {
              sortColumns = next;
              if (selectedEntryId) {
                patchEntry(selectedEntryId, {
                  sortColumns: next.map((column) => ({ ...column })),
                });
              }
            }}
            onRotate={graph.rotate}
            canUndo={canUndoGraph}
            canRedo={canRedoGraph}
            onUndo={graph.undo}
            onRedo={graph.redo}
            onReset={() => void graph.resetLayout()}
            onExport={() => void graph.exportSvg()}
            onToggleFullscreen={graph.toggleFullscreen}
            onFlowError={handleFlowError}
            onNodeDragStart={graph.onNodeDragStart}
            onNodeDragStop={graph.onNodeDragStop}
          />
        {:else if solution}
          <section class="flex min-h-112 flex-1 flex-col" aria-labelledby="result-title">
            <Panel class="flex min-h-0 flex-1 flex-col overflow-hidden">
              <div class="shrink-0">
                <SolutionSummary {solution} {elapsedLabel} />
              </div>
              <TopologyGraphPanel
                nodes={flowNodes}
                edges={flowEdges}
                fitRevision={graphFitRevision}
                fullscreen={graphFullscreen}
                class={`min-h-0 flex-1 ${graphFullscreen ? 'fixed inset-0 z-100 h-dvh w-full bg-[#08141c]' : ''}`}
                canvasClass={`flow-wrap w-full bg-[#08141c] ${
                  graphFullscreen ? 'min-h-0 flex-1' : 'h-[68vh] min-h-107.5 xl:h-auto xl:min-h-0 xl:flex-1'
                }`}
                onRotate={graph.rotate}
                canUndo={canUndoGraph}
                canRedo={canRedoGraph}
                onUndo={graph.undo}
                onRedo={graph.redo}
                onReset={() => void graph.resetLayout()}
                onExport={() => void graph.exportSvg()}
                onToggleFullscreen={graph.toggleFullscreen}
                onFlowError={handleFlowError}
                onNodeDragStart={graph.onNodeDragStart}
                onNodeDragStop={graph.onNodeDragStop}
              />
            </Panel>
          </section>
        {:else if !selectedEntry || selectedEntry.status === 'queued'}
          <EmptyGraphState />
        {/if}
      </div>
    </div>
  </main>
</div>
