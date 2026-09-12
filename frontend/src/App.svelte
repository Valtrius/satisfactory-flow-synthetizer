<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte';
  import { get, writable, type Writable } from 'svelte/store';
  import type { Edge, Node } from '@xyflow/svelte';
  import EmptyGraphState from './lib/EmptyGraphState.svelte';
  import ErrorBanner from './lib/ErrorBanner.svelte';
  import FlowInputsPanel from './lib/FlowInputsPanel.svelte';
  import HistoryPanel from './lib/HistoryPanel.svelte';
  import SolutionsTable from './lib/SolutionsTable.svelte';
  import Workbench from './lib/Workbench.svelte';
  import Layers from '@lucide/svelte/icons/layers';
  import SearchTelemetry from './lib/SearchTelemetry.svelte';
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
  import { exportHistoryBundle, exportHistoryEntry, importHistoryPayload, pageImportedEntries } from './lib/historyIo';
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
  import ShareDialog from './lib/sharing/ShareDialog.svelte';
  import { createShareSession } from './lib/sharing/session.svelte';
  import Button from './lib/ui/Button.svelte';
  import Select from './lib/ui/Select.svelte';
  import appIcon from '../../src-tauri/icons/icon.ico?url';
  import { getPlatform } from './lib/platform';
  import OfflinePanel from './lib/offline/OfflinePanel.svelte';
  import { createResultPages } from './lib/resultPages.svelte';
  import {
    browserThreadCount,
    browserWorkerChoices,
    readBrowserWorkers,
    saveBrowserWorkers,
  } from './lib/platform/browserWorkers';
  import { formatElapsed, searchHeadline, searchStageView, searchSubline, sizeSearchBody } from './lib/searchStage';
  import { DEFAULT_SORT_COLUMNS, compareSolutions, type SortColumn } from './lib/solutionSort';
  import { readUiPrefs, updateUiPrefs } from './lib/uiPrefs';
  import { enumeratesLayouts, type EndpointRow, type Solution, type SolveRequest } from './types';

  const platform = getPlatform();
  const clientThreads = browserThreadCount();
  let browserWorkers = $state(readBrowserWorkers(clientThreads));
  const workerChoices = $derived(browserWorkerChoices(clientThreads, browserWorkers));
  const resultPages = createResultPages(
    platform.collections,
    () => selectedEntry,
    () => sortColumns,
    (message) => {
      errorMessage = message;
    },
  );
  const shares = createShareSession({
    canSave: () => historyReady && !closing,
    selectedEntry: () => selectedEntry,
    solution: () => solution,
    graphNodes: () => get(flowNodes),
    entries: () => historyEntries,
    add: (entry) => {
      historyEntries = [...historyEntries, entry];
    },
    select: async (id) => {
      selectedEntryId = id;
      await hydrateViewFromEntry(id);
    },
    flushChrome: () => graph.flushChrome(),
    flushHistory: () => flushHistoryToDisk(),
  });
  const savedForm = readUiPrefs().form;
  let nextEndpointId = $state(savedForm.nextEndpointId);
  let inputs = $state<EndpointRow[]>(savedForm.inputs.map((row) => ({ ...row })));
  let outputs = $state<EndpointRow[]>(savedForm.outputs.map((row) => ({ ...row })));
  let beltRate = $state(savedForm.beltRate);
  let solveMode = $state(savedForm.solveMode);

  let historyEntries = $state<HistoryEntry[]>([]);
  let selectedEntryId = $state<string | null>(null);

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
  let setupOpen = $state(true);
  let historyOpen = $state(true);
  let layoutsOpen = $state(true);
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
    patchEntry,
    setError: (message) => {
      errorMessage = message;
    },
    loadSolution: async (entry, index) => {
      if (!entry.collection || !platform.collections)
        throw new Error('This host cannot read that solution collection.');
      if (entry.collection.preferredIndex === index && entry.result) return entry.result;
      return platform.collections.get(entry.collection, index);
    },
    onChromeChange: (chrome) => {
      graphFitRevision = chrome.fitRevision;
      graphFullscreen = chrome.fullscreen;
      canUndoGraph = chrome.canUndo;
      canRedoGraph = chrome.canRedo;
    },
  });

  const queue = new HistoryQueue({
    discardCollection: async (ref) => {
      await platform.collections?.discard(ref);
    },
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
  const searchStageMuted = $derived(
    selectedEntry?.status === 'cancelled' ||
      selectedEntry?.status === 'failed' ||
      selectedEntry?.status === 'incomplete' ||
      selectedEntry?.status === 'unsat',
  );
  const searchView = $derived(searchStageView(viewJob?.progress ?? null));
  const inputSlots = $derived(endpointSlots(inputs));
  const outputSlots = $derived(endpointSlots(outputs));
  const foundCount = $derived(selectedEntry?.collection?.count ?? solutions.length);
  const showResultsTable = $derived(searchEnumerate && foundCount > 0);
  const searchCopyContext = $derived({
    solutionsLength: foundCount,
    searchEnumerate,
    firstNodeCount: selectedEntry?.result?.stats.nodeCount ?? solutions[0]?.stats.nodeCount ?? null,
  });
  const elapsedLabel = $derived(formatElapsed(elapsedMs));
  const runningElapsedLabel = $derived(
    bands.running?.startedAtMs ? formatElapsed(Math.max(0, runningTick - bands.running.startedAtMs)) : '',
  );
  const displayRows = $derived(
    selectedEntry?.collection
      ? resultPages.rows
      : solutions
          .map((item, sourceIndex) => ({ solution: item, sourceIndex }))
          .sort((left, right) => compareSolutions(left.solution, right.solution, sortColumns)),
  );
  const selectedDisplayIndex = $derived(displayRows.findIndex((row) => row.sourceIndex === selectedSourceIndex));
  $effect(() => {
    if (platform.runtime === 'browser') saveBrowserWorkers(browserWorkers);
  });

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
    if (platform.runtime === 'browser')
      request.browserWorkers = browserWorkers === 'auto' ? clientThreads : browserWorkers;
    if (request.outputs.length < 1) {
      errorMessage = 'Add at least one output before solving.';
      return;
    }
    historyOpen = true;
    if (enumeratesLayouts(request.solveMode)) layoutsOpen = true;
    const entry = createQueuedEntry(snapshotForm(inputs, outputs, beltRate, solveMode), request);
    graph.flushChrome();
    // Newest queued jobs stack on top; the runner drains from the bottom.
    const parts = partitionEntries(historyEntries);
    const jobAlreadyRunning = parts.running != null;
    historyEntries = assembleEntries([entry, ...parts.queued], parts.running, parts.history);
    if (!jobAlreadyRunning) {
      layoutsOpen = enumeratesLayouts(request.solveMode);
      selectedEntryId = entry.id;
      graph.clearView();
      sortColumns = [...DEFAULT_SORT_COLUMNS];
    }
    await queue.pump();
    if (!jobAlreadyRunning && selectedEntryId) await hydrateViewFromEntry(selectedEntryId);
  }

  async function selectHistoryEntry(id: string): Promise<void> {
    const entry = historyEntries.find((item) => item.id === id);
    if (!entry) return;
    historyOpen = true;
    layoutsOpen = enumeratesLayouts(entry.request.solveMode);
    if (id === selectedEntryId) return;
    graph.flushChrome();
    selectedEntryId = id;
    await hydrateViewFromEntry(id);
  }

  function renameEntry(id: string, title: string | null): void {
    patchEntry(id, { title });
  }

  async function deleteEntry(id: string): Promise<void> {
    if (!historyReady || closing) return;
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
    try {
      // Flush the removal before discarding storage, even if this entry never
      // reached the debounced checkpoint and produces no history operations.
      await persist.flushNow(historyEntries, selectedEntryId);
      if (entry.collection) await platform.collections?.discard(entry.collection);
    } catch (error) {
      errorMessage = `Could not delete history entry: ${error instanceof Error ? error.message : String(error)}`;
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
    const removed = historyEntries;
    historyEntries = [];
    selectedEntryId = null;
    graph.clearView();
    sortColumns = [...DEFAULT_SORT_COLUMNS];
    try {
      await persist.flushNow([], null);
      for (const entry of removed) if (entry.collection) await platform.collections?.discard(entry.collection);
    } catch (error) {
      errorMessage = `Could not clear history: ${error instanceof Error ? error.message : String(error)}`;
    }
  }

  function copyEntryToForm(id: string): void {
    const entry = historyEntries.find((item) => item.id === id);
    if (!entry) return;
    setupOpen = true;
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
      const imported = await pageImportedEntries(
        entries.filter((entry) => importedIds.includes(entry.id)),
        importedIds,
      );
      // Active jobs and edits can change while import writes collection pages.
      // Attach only the imported entries to the latest document.
      historyEntries = [...historyEntries, ...imported];
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
  <link rel="icon" type="image/x-icon" href={appIcon} />
  <meta
    name="description"
    content="Exact Satisfactory splitter and merger flow synthetizer with an exact cvc5 solver."
  />
</svelte:head>

<svg class="pointer-events-none absolute size-0" aria-hidden="true" focusable="false">
  <defs>
    <linearGradient id="sfs-accent-icon-gradient" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="24" y2="24">
      <stop offset="0%" stop-color="var(--color-accent-bright)" />
      <stop offset="100%" stop-color="var(--color-accent)" />
    </linearGradient>
  </defs>
</svg>

{#snippet telemetryContent()}
  {#if viewJob}
    <SearchTelemetry
      {searchView}
      muted={searchStageMuted}
      {busy}
      {elapsedLabel}
      headline={searchHeadline(viewJob, searchCopyContext)}
      subline={searchSubline(viewJob, searchView, searchCopyContext)}
      sizeBody={sizeSearchBody(viewJob, searchView)}
      {foundCount}
      showFound={searchEnumerate}
      showDetails
    />
  {/if}
{/snippet}

<Workbench {graphFullscreen} bind:setupOpen bind:historyOpen bind:layoutsOpen>
  {#snippet setup()}
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
    <div class="border-line grid gap-4 border-t p-4">
      {#if platform.runtime === 'browser'}
        <div class="text-muted flex flex-wrap items-center gap-3 text-sm">
          <span>Compute workers</span>
          <Select
            label="Browser compute workers"
            bind:value={browserWorkers}
            options={[
              {
                value: 'auto' as const,
                label: `Automatic (${clientThreads} ${clientThreads === 1 ? 'worker' : 'workers'})`,
              },
              ...workerChoices.map((count) => ({
                value: count,
                label: `${count} ${count === 1 ? 'worker' : 'workers'}`,
              })),
            ]}
          />
        </div>
        <p class="text-muted m-0 text-sm" role="status">
          Solving runs locally. Automatic uses the {clientThreads} logical {clientThreads === 1
            ? 'processor'
            : 'processors'} reported by this browser. Each compute worker has its own solver memory. Reduce the count to limit
          memory use. Closing this tab stops the search. Browser storage is best-effort. Export a backup before clearing site
          data.
        </p>
      {/if}

      {#if import.meta.env.PROD && platform.runtime === 'browser'}
        <OfflinePanel />
      {/if}

      <Button size="small" onclick={() => shares.open({})}>Open shared solution</Button>
    </div>
  {/snippet}
  {#snippet history()}
    <div class="h-full min-h-0" inert={!historyReady || closing} aria-busy={!historyReady}>
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
  {/snippet}
  {#snippet layouts()}
    <Panel variant="column" class="flex h-full min-h-0 flex-col overflow-hidden">
      <div class="border-line flex min-h-[57px] shrink-0 items-center gap-2 border-b px-4 py-3">
        <h2 class="m-0 flex items-center gap-2 text-lg font-bold tracking-tight">
          <Layers class="text-accent size-[1.05rem]" strokeWidth={2.2} aria-hidden="true" />
          Layouts
        </h2>
      </div>
      {#if showResultsTable && solution && viewJob}
        <div class="min-h-0 flex-1">
          <SolutionsTable
            solutions={displayRows.map((row) => row.solution)}
            selectedIndex={selectedDisplayIndex}
            resetKey={JSON.stringify([selectedEntryId, sortColumns])}
            onLoadMore={selectedEntry?.collection ? resultPages.loadMore : undefined}
            loading={resultPages.loading}
            loadFailed={resultPages.failed}
            onRetry={resultPages.retry}
            totalCount={foundCount}
            columns={sortColumns}
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
          />
        </div>
      {:else}
        <p class="text-muted m-0 p-4 text-sm">
          {solution ? 'The selected search returns one layout.' : 'Layouts from the selected search will appear here.'}
        </p>
      {/if}
    </Panel>
  {/snippet}

  {#if errorMessage}
    <div class="shrink-0"><ErrorBanner message={errorMessage} /></div>
  {/if}
  <section class="flex min-h-0 flex-1 flex-col" aria-label="Search results">
    <Panel variant="column" class="flex min-h-0 flex-1 flex-col overflow-hidden">
      <TopologyGraphPanel
        {solution}
        elapsedLabel={elapsedMs > 0 ? elapsedLabel : undefined}
        telemetry={viewJob ? telemetryContent : undefined}
        nodes={flowNodes}
        edges={flowEdges}
        fitRevision={graphFitRevision}
        fullscreen={graphFullscreen}
        class={`min-h-0 flex-1 ${graphFullscreen ? 'fixed inset-0 z-100 h-dvh w-full bg-[#08141c]' : ''}`}
        editing={{
          onRotate: graph.rotate,
          canUndo: canUndoGraph,
          canRedo: canRedoGraph,
          onUndo: graph.undo,
          onRedo: graph.redo,
          onReset: () => void graph.resetLayout(),
          onExport: () => void graph.exportSvg(),
          onShare: shares.shareSelected,
          onToggleFullscreen: graph.toggleFullscreen,
        }}
        onFlowError={handleFlowError}
        onNodeDragStart={graph.onNodeDragStart}
        onNodeDragStop={graph.onNodeDragStop}
      >
        {#snippet empty()}
          <EmptyGraphState
            title={viewJob ? searchHeadline(viewJob, searchCopyContext) : undefined}
            description={viewJob ? searchSubline(viewJob, searchView, searchCopyContext) : undefined}
          />
        {/snippet}
      </TopologyGraphPanel>
    </Panel>
  </section>
</Workbench>

{#if shares.dialog}
  {#key shares.revision}
    <ShareDialog {...shares.dialog} canSave={historyReady && !closing} onSave={shares.save} onclose={shares.close} />
  {/key}
{/if}
