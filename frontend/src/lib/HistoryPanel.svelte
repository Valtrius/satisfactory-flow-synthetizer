<script lang="ts">
  import History from '@lucide/svelte/icons/history';
  import Download from '@lucide/svelte/icons/download';
  import Upload from '@lucide/svelte/icons/upload';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Copy from '@lucide/svelte/icons/copy';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import X from '@lucide/svelte/icons/x';
  import Target from '@lucide/svelte/icons/target';
  import LayoutGrid from '@lucide/svelte/icons/layout-grid';
  import Cpu from '@lucide/svelte/icons/cpu';
  import Zap from '@lucide/svelte/icons/zap';
  import Network from '@lucide/svelte/icons/network';
  import Table2 from '@lucide/svelte/icons/table-2';
  import ArrowUpDown from '@lucide/svelte/icons/arrow-up-down';
  import ListFilter from '@lucide/svelte/icons/list-filter';
  import Button from './ui/Button.svelte';
  import Input from './ui/Input.svelte';
  import Panel from './ui/Panel.svelte';
  import { formatElapsed } from './searchStage';
  import {
    displayTitle,
    entryElapsedMs,
    entryHistoryMetrics,
    entryLayoutCount,
    entryNodeCount,
    entryStatusCaption,
    type HistoryEntry
  } from './historyModel';
  import {
    readUiPrefs,
    updateUiPrefs,
    type HistoryEngineFilter,
    type HistorySearchFilter,
    type HistorySortPref,
    type HistoryStatusFilter
  } from './uiPrefs';
  import { flip } from 'svelte/animate';
  import {
    insertIndexFromClient,
    prefersReducedMotion,
    setListDragging,
    visualReorderSlots
  } from './pointerReorder';

  type StatusFilter = HistoryStatusFilter;
  type EngineFilter = HistoryEngineFilter;
  type SearchFilter = HistorySearchFilter;
  type ToolbarPanel = 'sort' | 'filter';
  type HistorySort = HistorySortPref;
  type DragBand = 'queued' | 'history';
  type VisualItem =
    | { kind: 'entry'; entry: HistoryEntry }
    | { kind: 'ghost'; key: string };

  type Props = {
    queued: HistoryEntry[];
    running: HistoryEntry | null;
    history: HistoryEntry[];
    selectedEntryId: string | null;
    runningElapsedLabel: string;
    onSelect: (id: string) => void;
    onReorderQueued: (fromId: string, toId: string) => void;
    onReorderHistory: (fromId: string, toId: string) => void;
    onRename: (id: string, title: string | null) => void;
    onDelete: (id: string) => void;
    onCancelRunning: () => void;
    onCopyToNew: (id: string) => void;
    onExportEntry: (id: string) => void;
    onExportAll: () => void;
    onImport: () => void;
  };

  let {
    queued,
    running,
    history,
    selectedEntryId,
    runningElapsedLabel,
    onSelect,
    onReorderQueued,
    onReorderHistory,
    onRename,
    onDelete,
    onCancelRunning,
    onCopyToNew,
    onExportEntry,
    onExportAll,
    onImport
  }: Props = $props();

  let renamingId = $state<string | null>(null);
  let renameDraft = $state('');
  let renameIgnoreBlur = false;
  let menuId = $state<string | null>(null);
  const savedToolbar = readUiPrefs().history;
  let query = $state(savedToolbar.query);
  let openPanel = $state<ToolbarPanel | null>(null);
  let statusFilters = $state<StatusFilter[]>([...savedToolbar.statusFilters]);
  let engineFilters = $state<EngineFilter[]>([...savedToolbar.engineFilters]);
  let searchFilters = $state<SearchFilter[]>([...savedToolbar.searchFilters]);
  let sort = $state<HistorySort>(savedToolbar.sort);

  let dragBand = $state<DragBand | null>(null);
  let dragFromId = $state<string | null>(null);
  let dragInsertAt = $state<number | null>(null);
  let dragActive = $state(false);
  let dragPointerId = $state<number | null>(null);
  let dragOriginX = 0;
  let dragOriginY = 0;
  let dragGrabX = 0;
  let dragGrabY = 0;
  let dragWidth = $state(0);
  let dragHeight = $state(0);
  let dragFloatX = $state(0);
  let dragFloatY = $state(0);
  let dragReduceMotion = false;

  const statusOptions: { value: StatusFilter; label: string }[] = [
    { value: 'completed', label: 'Done' },
    { value: 'failed', label: 'Failed' },
    { value: 'cancelled', label: 'Cancelled' },
    { value: 'incomplete', label: 'Incomplete' },
    { value: 'unsat', label: 'Impossible' }
  ];

  const engineOptions: { value: EngineFilter; label: string }[] = [
    { value: 'custom', label: 'Custom' },
    { value: 'z3', label: 'Z3' }
  ];

  const searchOptions: { value: SearchFilter; label: string }[] = [
    { value: 'opt', label: 'Optimal' },
    { value: 'all', label: 'All layouts' }
  ];

  const sortOptions: { value: HistorySort; label: string; tip: string }[] = [
    { value: 'manual', label: 'Manual', tip: 'Drag rows to reorder' },
    { value: 'newest', label: 'Newest', tip: 'Most recently created first' },
    { value: 'oldest', label: 'Oldest', tip: 'Oldest created first' },
    { value: 'name-asc', label: 'Name A-Z', tip: 'Alphabetical by title' },
    { value: 'name-desc', label: 'Name Z-A', tip: 'Reverse alphabetical' },
    { value: 'layouts-desc', label: 'Most layouts', tip: 'Highest layout count first' },
    { value: 'nodes-asc', label: 'Fewest nodes', tip: 'Smallest N first' }
  ];

  const listEmpty = $derived(queued.length === 0 && !running && history.length === 0);
  const historyDraggable = $derived(sort === 'manual');
  const allEntries = $derived(
    running ? [...queued, running, ...history] : [...queued, ...history]
  );
  const filtersActive = $derived(
    statusFilters.length > 0 || engineFilters.length > 0 || searchFilters.length > 0
  );
  const sortActive = $derived(sort !== 'manual');

  $effect(() => {
    updateUiPrefs({
      history: {
        query,
        sort,
        statusFilters,
        engineFilters,
        searchFilters
      }
    });
  });

  function togglePanel(panel: ToolbarPanel, event: MouseEvent): void {
    event.stopPropagation();
    menuId = null;
    openPanel = openPanel === panel ? null : panel;
  }

  function clearFilters(): void {
    statusFilters = [];
    engineFilters = [];
    searchFilters = [];
  }

  function toggleValue<T extends string>(list: T[], value: T): T[] {
    return list.includes(value) ? list.filter((item) => item !== value) : [...list, value];
  }

  function toggleStatus(value: StatusFilter): void {
    statusFilters = toggleValue(statusFilters, value);
  }

  function toggleEngine(value: EngineFilter): void {
    engineFilters = toggleValue(engineFilters, value);
  }

  function toggleSearch(value: SearchFilter): void {
    searchFilters = toggleValue(searchFilters, value);
  }

  function chooseSort(value: HistorySort): void {
    sort = value;
    openPanel = null;
  }

  function matchesQuery(entry: HistoryEntry): boolean {
    const needle = query.trim().toLowerCase();
    if (!needle) return true;
    return displayTitle(entry).toLowerCase().includes(needle);
  }

  function entryEngine(entry: HistoryEntry): EngineFilter {
    return entry.request.engine === 'z3' ? 'z3' : 'custom';
  }

  function entrySearch(entry: HistoryEntry): SearchFilter {
    return entry.request.enumerateAllAtN ? 'all' : 'opt';
  }

  function matchesStatusFilters(entry: HistoryEntry): boolean {
    return statusFilters.length === 0 || (statusFilters as string[]).includes(entry.status);
  }

  function matchesEngineFilters(entry: HistoryEntry): boolean {
    return engineFilters.length === 0 || engineFilters.includes(entryEngine(entry));
  }

  function matchesSearchFilters(entry: HistoryEntry): boolean {
    return searchFilters.length === 0 || searchFilters.includes(entrySearch(entry));
  }

  function matchesFilter(entry: HistoryEntry): boolean {
    return (
      matchesStatusFilters(entry) && matchesEngineFilters(entry) && matchesSearchFilters(entry)
    );
  }

  function matchesEntry(entry: HistoryEntry): boolean {
    return matchesQuery(entry) && matchesFilter(entry);
  }

  /** Facet counts ignore their own dimension so other chips stay meaningful. */
  function countStatus(value: StatusFilter): number {
    return allEntries.filter(
      (entry) =>
        matchesQuery(entry) &&
        matchesEngineFilters(entry) &&
        matchesSearchFilters(entry) &&
        entry.status === value
    ).length;
  }

  function countEngine(value: EngineFilter): number {
    return allEntries.filter(
      (entry) =>
        matchesQuery(entry) &&
        matchesStatusFilters(entry) &&
        matchesSearchFilters(entry) &&
        entryEngine(entry) === value
    ).length;
  }

  function countSearch(value: SearchFilter): number {
    return allEntries.filter(
      (entry) =>
        matchesQuery(entry) &&
        matchesStatusFilters(entry) &&
        matchesEngineFilters(entry) &&
        entrySearch(entry) === value
    ).length;
  }

  function compareHistory(a: HistoryEntry, b: HistoryEntry): number {
    switch (sort) {
      case 'newest':
        return b.createdAtMs - a.createdAtMs;
      case 'oldest':
        return a.createdAtMs - b.createdAtMs;
      case 'name-asc':
        return displayTitle(a).localeCompare(displayTitle(b), undefined, { sensitivity: 'base' });
      case 'name-desc':
        return displayTitle(b).localeCompare(displayTitle(a), undefined, { sensitivity: 'base' });
      case 'layouts-desc':
        return entryLayoutCount(b) - entryLayoutCount(a);
      case 'nodes-asc': {
        const aNodes = entryNodeCount(a);
        const bNodes = entryNodeCount(b);
        if (aNodes == null && bNodes == null) return 0;
        if (aNodes == null) return 1;
        if (bNodes == null) return -1;
        return aNodes - bNodes;
      }
      default:
        return 0;
    }
  }

  const filteredQueued = $derived(queued.filter(matchesEntry));
  const filteredRunning = $derived(running && matchesEntry(running) ? running : null);
  const filteredHistory = $derived(
    sort === 'manual'
      ? history.filter(matchesEntry)
      : history.filter(matchesEntry).slice().sort(compareHistory)
  );
  const noMatches = $derived(
    !listEmpty &&
      filteredQueued.length === 0 &&
      filteredRunning == null &&
      filteredHistory.length === 0
  );

  const dragEntry = $derived(
    dragFromId
      ? (queued.find((entry) => entry.id === dragFromId) ??
        history.find((entry) => entry.id === dragFromId) ??
        null)
      : null
  );

  const flipDuration = $derived(dragReduceMotion || !dragActive ? 0 : 220);

  function visualList(entries: HistoryEntry[], band: DragBand): VisualItem[] {
    if (!dragActive || dragBand !== band || !dragFromId) {
      return entries.map((entry) => ({ kind: 'entry', entry }));
    }
    const fromIndex = entries.findIndex((entry) => entry.id === dragFromId);
    return visualReorderSlots(entries, fromIndex, dragInsertAt, true).map((slot) =>
      slot.kind === 'ghost'
        ? { kind: 'ghost', key: `ghost-${band}` }
        : { kind: 'entry', entry: slot.item }
    );
  }

  const visualQueued = $derived(visualList(filteredQueued, 'queued'));
  const visualHistory = $derived(visualList(filteredHistory, 'history'));

  function dropTargetId(
    entries: HistoryEntry[],
    fromId: string,
    insertAt: number
  ): string | null {
    const fromIndex = entries.findIndex((entry) => entry.id === fromId);
    if (fromIndex < 0) return null;
    const without = entries.filter((entry) => entry.id !== fromId);
    const clamped = Math.max(0, Math.min(without.length, insertAt));
    if (clamped === fromIndex) return fromId;
    if (clamped < fromIndex) return without[clamped]?.id ?? null;
    return without[clamped - 1]?.id ?? null;
  }

  function isInteractiveTarget(target: EventTarget | null): boolean {
    if (!(target instanceof Element)) return false;
    return Boolean(target.closest('button, input, textarea, a, [role="menuitem"]'));
  }

  function clearDragState(): void {
    dragBand = null;
    dragFromId = null;
    dragInsertAt = null;
    dragActive = false;
    dragPointerId = null;
    dragWidth = 0;
    dragHeight = 0;
    setListDragging(false);
  }

  function cancelDrag(): void {
    if (!dragFromId) return;
    clearDragState();
  }

  function updateDropTarget(clientY: number): void {
    if (!dragBand || !dragFromId) return;
    const entryEls = [
      ...document.querySelectorAll<HTMLElement>(
        `[data-history-band="${dragBand}"][data-history-id]`
      )
    ];
    dragInsertAt = insertIndexFromClient(entryEls, clientY, 'y');
  }

  function onCardPointerDown(band: DragBand, id: string, event: PointerEvent): void {
    if (event.button !== 0 || isInteractiveTarget(event.target) || renamingId === id) return;
    event.preventDefault();
    menuId = null;
    openPanel = null;
    dragReduceMotion = prefersReducedMotion();
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const source = band === 'queued' ? filteredQueued : filteredHistory;
    const fromIndex = source.findIndex((entry) => entry.id === id);
    dragBand = band;
    dragFromId = id;
    dragInsertAt = fromIndex >= 0 ? fromIndex : 0;
    dragActive = false;
    dragPointerId = event.pointerId;
    dragOriginX = event.clientX;
    dragOriginY = event.clientY;
    dragGrabX = event.clientX - rect.left;
    dragGrabY = event.clientY - rect.top;
    dragWidth = rect.width;
    dragHeight = rect.height;
    dragFloatX = rect.left;
    dragFloatY = rect.top;
  }

  function onWindowPointerMove(event: PointerEvent): void {
    if (dragPointerId == null || event.pointerId !== dragPointerId || !dragFromId) return;
    if (!dragActive) {
      if (
        Math.abs(event.clientX - dragOriginX) <= 4 &&
        Math.abs(event.clientY - dragOriginY) <= 4
      ) {
        return;
      }
      dragActive = true;
      setListDragging(true);
    }
    dragFloatX = event.clientX - dragGrabX;
    dragFloatY = event.clientY - dragGrabY;
    updateDropTarget(event.clientY);
  }

  function onWindowPointerUp(event: PointerEvent): void {
    if (dragPointerId == null || event.pointerId !== dragPointerId || !dragFromId) return;
    const fromId = dragFromId;
    const band = dragBand;
    const insertAt = dragInsertAt;
    const active = dragActive;
    const source =
      band === 'queued'
        ? filteredQueued
        : band === 'history'
          ? filteredHistory
          : [];
    clearDragState();
    if (active && band && insertAt != null) {
      const toId = dropTargetId(source, fromId, insertAt);
      if (toId && toId !== fromId) {
        if (band === 'queued') onReorderQueued(fromId, toId);
        else onReorderHistory(fromId, toId);
      }
      return;
    }
    if (!active) onSelect(fromId);
  }

  $effect(() => {
    if (dragPointerId == null) return;
    const move = (event: PointerEvent) => onWindowPointerMove(event);
    const up = (event: PointerEvent) => onWindowPointerUp(event);
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    window.addEventListener('pointercancel', up);
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      window.removeEventListener('pointercancel', up);
      document.body.classList.remove('history-dragging');
      setListDragging(false);
    };
  });

  function chipClass(active: boolean): string {
    return `cursor-pointer rounded-control border px-2.5 py-1.5 text-left text-xs font-semibold transition-colors ${
      active
        ? 'border-accent/70 bg-selected text-ink'
        : 'border-control-border bg-control/60 text-muted hover:border-control-border-hover hover:text-control-fg'
    }`;
  }

  function iconBtnClass(active: boolean): string {
    return active ? '!border-accent/70 !bg-selected !text-accent' : '';
  }

  function startRename(entry: HistoryEntry): void {
    menuId = null;
    openPanel = null;
    renameIgnoreBlur = false;
    renamingId = entry.id;
    renameDraft = displayTitle(entry);
  }

  function commitRename(entry: HistoryEntry): void {
    if (renamingId !== entry.id) return;
    const next = renameDraft.trim();
    onRename(entry.id, next.length > 0 ? next : null);
    renamingId = null;
  }

  function cancelRename(): void {
    renamingId = null;
  }

  function toggleMenu(id: string, event: MouseEvent): void {
    event.stopPropagation();
    openPanel = null;
    menuId = menuId === id ? null : id;
  }

  function subtitle(entry: HistoryEntry, band: 'queued' | 'running' | 'history'): string {
    if (band === 'running') {
      if (entry.status === 'cancelling') return `Stopping… · ${runningElapsedLabel}`;
      return runningElapsedLabel || '0:00.0';
    }
    const status = entryStatusCaption(entry);
    if (band === 'queued') return status;
    const elapsed =
      entry.startedAtMs != null ? formatElapsed(entryElapsedMs(entry)) : '';
    if (elapsed && status) return `${elapsed} · ${status}`;
    return status || elapsed;
  }
</script>

<svelte:window
  onclick={() => {
    menuId = null;
    openPanel = null;
  }}
  onkeydown={(event) => {
    if (event.key === 'Escape') {
      if (dragFromId) {
        event.preventDefault();
        cancelDrag();
        return;
      }
      menuId = null;
      openPanel = null;
    }
  }}
/>

<Panel
  element="aside"
  class="flex h-[calc(100dvh-2rem)] min-h-140 flex-col overflow-hidden"
>
  <div class="flex items-center justify-between gap-2 border-b border-line px-4 py-3">
    <h2 class="m-0 flex items-center gap-2 text-lg font-bold tracking-tight">
      <History class="size-[1.05rem] text-accent" strokeWidth={2.2} aria-hidden="true" />
      History
    </h2>
    <div class="flex gap-1">
      <Button
        size="small"
        square
        type="button"
        title="Import history"
        aria-label="Import history"
        onclick={() => onImport()}
      >
        <Upload class="size-3.5" />
      </Button>
      <Button
        size="small"
        square
        type="button"
        title="Export all history"
        aria-label="Export all history"
        disabled={listEmpty}
        onclick={() => onExportAll()}
      >
        <Download class="size-3.5" />
      </Button>
    </div>
  </div>

  <div class="relative shrink-0 border-b border-line px-3 py-2">
    <div class="flex items-center gap-1.5">
      <Input
        type="search"
        size="small"
        class="min-w-0 flex-1"
        placeholder="Search…"
        aria-label="Search history by name"
        bind:value={query}
      />
      <Button
        size="small"
        square
        type="button"
        title="Sort history"
        aria-label="Sort history"
        aria-expanded={openPanel === 'sort'}
        aria-haspopup="dialog"
        class={iconBtnClass(openPanel === 'sort' || sortActive)}
        onclick={(event) => togglePanel('sort', event)}
      >
        <ArrowUpDown class="size-3.5" strokeWidth={2.2} />
      </Button>
      <Button
        size="small"
        square
        type="button"
        title="Filter history"
        aria-label="Filter history"
        aria-expanded={openPanel === 'filter'}
        aria-haspopup="dialog"
        class={iconBtnClass(openPanel === 'filter' || filtersActive)}
        onclick={(event) => togglePanel('filter', event)}
      >
        <ListFilter class="size-3.5" strokeWidth={2.2} />
      </Button>
    </div>

    {#if openPanel === 'sort'}
      <div
        class="absolute inset-x-2 top-[calc(100%-0.15rem)] z-20 rounded-control border border-line bg-panel-2 py-1.5 shadow-[0_14px_32px_rgb(0_0_0/45%)]"
        role="dialog"
        aria-label="Sort history"
        tabindex="-1"
        onclick={(event) => event.stopPropagation()}
        onpointerdown={(event) => event.stopPropagation()}
        onkeydown={(event) => event.stopPropagation()}
      >
        <p class="m-0 px-3 pt-1 pb-1.5 text-[0.65rem] font-bold tracking-[0.08em] text-dim uppercase">
          Sort by
        </p>
        <div class="flex flex-col gap-0.5 px-1.5 pb-1" role="listbox" aria-label="Sort options">
          {#each sortOptions as option (option.value)}
            <button
              type="button"
              role="option"
              aria-selected={sort === option.value}
              class={`rounded-control border px-2.5 py-2 text-left transition-colors ${
                sort === option.value
                  ? 'border-accent/70 bg-selected'
                  : 'border-transparent hover:bg-well-hover/70'
              }`}
              onclick={() => chooseSort(option.value)}
            >
              <span class="block text-xs font-bold text-ink">{option.label}</span>
              <span class="mt-0.5 block text-[0.68rem] text-muted">{option.tip}</span>
            </button>
          {/each}
        </div>
      </div>
    {:else if openPanel === 'filter'}
      <div
        class="absolute inset-x-2 top-[calc(100%-0.15rem)] z-20 max-h-[min(28rem,70dvh)] overflow-y-auto rounded-control border border-line bg-panel-2 px-3 py-2.5 shadow-[0_14px_32px_rgb(0_0_0/45%)]"
        role="dialog"
        aria-label="Filter history"
        tabindex="-1"
        onclick={(event) => event.stopPropagation()}
        onpointerdown={(event) => event.stopPropagation()}
        onkeydown={(event) => event.stopPropagation()}
      >
        <div class="mb-2.5 flex items-center justify-between gap-2">
          <p class="m-0 text-[0.65rem] font-bold tracking-[0.08em] text-dim uppercase">Filter</p>
          <button
            type="button"
            class="cursor-pointer border-0 bg-transparent p-0 text-xs font-semibold text-accent hover:text-accent-bright disabled:cursor-default disabled:text-dim"
            disabled={!filtersActive}
            onclick={() => clearFilters()}
          >
            Clear
          </button>
        </div>

        <div class="flex flex-col gap-3">
          <div>
            <p class="m-0 mb-1.5 text-[0.7rem] font-semibold text-muted">Status</p>
            <div class="flex flex-wrap gap-1.5" role="group" aria-label="Status filters">
              <button
                type="button"
                class={chipClass(statusFilters.length === 0)}
                aria-pressed={statusFilters.length === 0}
                onclick={() => {
                  statusFilters = [];
                }}
              >
                All
              </button>
              {#each statusOptions as option (option.value)}
                {@const count = countStatus(option.value)}
                <button
                  type="button"
                  class={chipClass(statusFilters.includes(option.value))}
                  aria-pressed={statusFilters.includes(option.value)}
                  onclick={() => toggleStatus(option.value)}
                >
                  {option.label}
                  <span class="ml-1 font-medium text-dim tabular-nums">{count}</span>
                </button>
              {/each}
            </div>
          </div>

          <div>
            <p class="m-0 mb-1.5 text-[0.7rem] font-semibold text-muted">Engine</p>
            <div class="flex flex-wrap gap-1.5" role="group" aria-label="Engine filters">
              <button
                type="button"
                class={chipClass(engineFilters.length === 0)}
                aria-pressed={engineFilters.length === 0}
                onclick={() => {
                  engineFilters = [];
                }}
              >
                Any
              </button>
              {#each engineOptions as option (option.value)}
                {@const count = countEngine(option.value)}
                <button
                  type="button"
                  class={chipClass(engineFilters.includes(option.value))}
                  aria-pressed={engineFilters.includes(option.value)}
                  onclick={() => toggleEngine(option.value)}
                >
                  {option.label}
                  <span class="ml-1 font-medium text-dim tabular-nums">{count}</span>
                </button>
              {/each}
            </div>
          </div>

          <div>
            <p class="m-0 mb-1.5 text-[0.7rem] font-semibold text-muted">Search</p>
            <div class="flex flex-wrap gap-1.5" role="group" aria-label="Search filters">
              <button
                type="button"
                class={chipClass(searchFilters.length === 0)}
                aria-pressed={searchFilters.length === 0}
                onclick={() => {
                  searchFilters = [];
                }}
              >
                Any
              </button>
              {#each searchOptions as option (option.value)}
                {@const count = countSearch(option.value)}
                <button
                  type="button"
                  class={chipClass(searchFilters.includes(option.value))}
                  aria-pressed={searchFilters.includes(option.value)}
                  onclick={() => toggleSearch(option.value)}
                >
                  {option.label}
                  <span class="ml-1 font-medium text-dim tabular-nums">{count}</span>
                </button>
              {/each}
            </div>
          </div>
        </div>
      </div>
    {/if}
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto" role="listbox" aria-label="Job history">
    {#if listEmpty}
      <p class="m-0 px-3 py-3 text-xs text-dim">Solve a problem to build history.</p>
    {:else if noMatches}
      <p class="m-0 px-3 py-3 text-xs text-dim">
        No entries match{query.trim() ? ` “${query.trim()}”` : ' these filters'}.
      </p>
    {:else}
      <div class="flex flex-col">
        {#each visualQueued as item (item.kind === 'ghost' ? item.key : item.entry.id)}
          <div class="history-list-item" animate:flip={{ duration: flipDuration }}>
            {#if item.kind === 'ghost'}
              <div
                class="history-drag-ghost"
                style={`height: ${dragHeight}px`}
                aria-hidden="true"
              ></div>
            {:else}
              {@render row(item.entry, 'queued', true)}
            {/if}
          </div>
        {/each}
        {#if filteredRunning}
          <div class="history-list-item">
            {@render row(filteredRunning, 'running', false)}
          </div>
        {/if}
        {#each visualHistory as item (item.kind === 'ghost' ? item.key : item.entry.id)}
          <div class="history-list-item" animate:flip={{ duration: flipDuration }}>
            {#if item.kind === 'ghost'}
              <div
                class="history-drag-ghost"
                style={`height: ${dragHeight}px`}
                aria-hidden="true"
              ></div>
            {:else}
              {@render row(item.entry, 'history', historyDraggable)}
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</Panel>

{#if dragActive && dragEntry && dragBand}
  <div
    class="history-drag-float"
    style={`width: ${dragWidth}px; transform: translate3d(${dragFloatX}px, ${dragFloatY}px, 0);`}
    aria-hidden="true"
  >
    {@render row(dragEntry, dragBand, false, true)}
  </div>
{/if}

{#snippet row(
  entry: HistoryEntry,
  band: 'queued' | 'running' | 'history',
  draggable: boolean,
  floating = false
)}
  {@const selected = entry.id === selectedEntryId}
  {@const metrics = entryHistoryMetrics(entry)}
  {@const allLayouts = Boolean(entry.request.enumerateAllAtN)}
  {@const engineZ3 = entry.request.engine === 'z3'}
  <div
    role="option"
    tabindex={floating ? -1 : 0}
    aria-selected={selected}
    data-history-id={floating ? undefined : entry.id}
    data-history-band={floating ? undefined : band}
    class={`relative grid grid-cols-[minmax(0,1fr)_auto] items-start gap-x-2 gap-y-1 border-b px-3 py-2.5 ${
      band === 'queued' ? 'border-dashed border-[#4a6574]' : 'border-solid border-line'
    } ${
      selected && band !== 'running' ? 'bg-selected shadow-[inset_3px_0_0_var(--color-accent)]' : ''
    } ${selected && band === 'running' ? 'shadow-[inset_3px_0_0_var(--color-accent)]' : ''} ${
      !selected && band !== 'running' && !floating ? 'bg-transparent hover:bg-well-hover/55' : ''
    } ${band === 'running' ? 'history-entry-running cursor-pointer' : ''} ${
      selected && band === 'running' ? 'history-entry-running--selected' : ''
    } ${draggable && !floating ? 'cursor-grab' : ''} ${
      floating ? 'history-drag-float-card border-solid' : ''
    }`}
    onpointerdown={draggable && !floating && (band === 'queued' || band === 'history')
      ? (event) => onCardPointerDown(band, entry.id, event)
      : undefined}
    onclick={() => {
      if (draggable || floating) return;
      onSelect(entry.id);
    }}
    onkeydown={(event) => {
      if (floating) return;
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        onSelect(entry.id);
      }
    }}
  >
    <div class="min-w-0">
      {#if renamingId === entry.id && !floating}
        <input
          class="w-full rounded border border-accent bg-[#08141c] px-1.5 py-0.5 text-sm font-bold text-ink"
          bind:value={renameDraft}
          aria-label="Rename history entry"
          onclick={(event) => event.stopPropagation()}
          onpointerdown={(event) => event.stopPropagation()}
          onkeydown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              commitRename(entry);
            } else if (event.key === 'Escape') {
              event.preventDefault();
              renameIgnoreBlur = true;
              cancelRename();
            }
          }}
          onblur={() => {
            if (renameIgnoreBlur) {
              renameIgnoreBlur = false;
              return;
            }
            commitRename(entry);
          }}
        />
      {:else}
        <p class="m-0 text-sm leading-snug font-bold wrap-break-word">{displayTitle(entry)}</p>
      {/if}
      <p class="mt-0.5 m-0 text-[0.7rem] tabular-nums text-muted">{subtitle(entry, band)}</p>
    </div>

    <div class="flex items-start">
      {#if band === 'running'}
        <Button
          size="small"
          square
          variant="danger"
          type="button"
          title="Cancel and keep any layouts found"
          aria-label="Cancel running job"
          class="!min-h-7 !w-7"
          onclick={(event) => {
            event.stopPropagation();
            onCancelRunning();
          }}
        >
          <X class="size-3.5" />
        </Button>
      {:else if band === 'queued'}
        <Button
          size="small"
          square
          variant="quiet"
          type="button"
          title="Remove from queue"
          aria-label="Remove from queue"
          class="!min-h-7 !w-7"
          tabindex={floating ? -1 : undefined}
          onclick={(event) => {
            event.stopPropagation();
            if (floating) return;
            onDelete(entry.id);
          }}
        >
          <X class="size-3.5" />
        </Button>
      {:else}
        <Button
          size="small"
          square
          variant="quiet"
          type="button"
          title="More actions"
          aria-label="More actions"
          class="!min-h-7 !w-7"
          tabindex={floating ? -1 : undefined}
          onclick={(event) => {
            if (floating) {
              event.stopPropagation();
              return;
            }
            toggleMenu(entry.id, event);
          }}
        >
          ⋯
        </Button>
      {/if}
    </div>

    <ul class="col-span-2 m-0 grid list-none grid-cols-2 gap-x-2 gap-y-0.5 p-0">
      <li
        class="inline-flex min-w-0 items-center gap-1 text-[0.68rem] font-semibold text-muted tabular-nums"
        title={metrics.search.tip}
        aria-label={metrics.search.tip}
      >
        {#if allLayouts}
          <LayoutGrid class="size-3 shrink-0 text-accent-bright" strokeWidth={2.2} aria-hidden="true" />
        {:else}
          <Target class="size-3 shrink-0 text-accent-bright" strokeWidth={2.2} aria-hidden="true" />
        {/if}
        <span class="truncate">{metrics.search.value}</span>
      </li>
      <li
        class="inline-flex min-w-0 items-center gap-1 text-[0.68rem] font-semibold text-muted tabular-nums"
        title={metrics.engine.tip}
        aria-label={metrics.engine.tip}
      >
        {#if engineZ3}
          <Zap class="size-3 shrink-0 text-flow" strokeWidth={2.2} aria-hidden="true" />
        {:else}
          <Cpu class="size-3 shrink-0 text-flow" strokeWidth={2.2} aria-hidden="true" />
        {/if}
        <span class="truncate">{metrics.engine.value}</span>
      </li>
      <li
        class="inline-flex min-w-0 items-center gap-1 text-[0.68rem] font-semibold text-muted tabular-nums"
        title={metrics.nodes.tip}
        aria-label={metrics.nodes.tip}
      >
        <Network class="size-3 shrink-0 text-[#9ec5d6]" strokeWidth={2.2} aria-hidden="true" />
        <span class="truncate">{metrics.nodes.value}</span>
      </li>
      <li
        class="inline-flex min-w-0 items-center gap-1 text-[0.68rem] font-semibold text-muted tabular-nums"
        title={metrics.layouts.tip}
        aria-label={metrics.layouts.tip}
      >
        <Table2 class="size-3 shrink-0 text-warning" strokeWidth={2.2} aria-hidden="true" />
        <span class="truncate">{metrics.layouts.value}</span>
      </li>
    </ul>

    {#if menuId === entry.id && !floating}
      <div
        class="absolute top-9 right-1 z-5 min-w-44 rounded-lg border border-line bg-panel py-1 shadow-[0_14px_32px_rgb(0_0_0/45%)]"
        role="menu"
        tabindex="-1"
        onkeydown={(event) => event.stopPropagation()}
        onclick={(event) => event.stopPropagation()}
        onpointerdown={(event) => event.stopPropagation()}
      >
        <button
          type="button"
          class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-[#152833]"
          role="menuitem"
          onclick={() => startRename(entry)}
        >
          <Pencil class="size-3.5 text-muted" />
          Rename
        </button>
        <button
          type="button"
          class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-[#152833]"
          role="menuitem"
          onclick={() => {
            menuId = null;
            onCopyToNew(entry.id);
          }}
        >
          <Copy class="size-3.5 text-muted" />
          Copy to New problem
        </button>
        <button
          type="button"
          class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-[#152833]"
          role="menuitem"
          onclick={() => {
            menuId = null;
            onExportEntry(entry.id);
          }}
        >
          <Download class="size-3.5 text-muted" />
          Export entry…
        </button>
        <button
          type="button"
          class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-danger hover:bg-[#152833]"
          role="menuitem"
          onclick={() => {
            menuId = null;
            onDelete(entry.id);
          }}
        >
          <Trash2 class="size-3.5" />
          Delete
        </button>
      </div>
    {/if}
  </div>
{/snippet}
