<script lang="ts">
  import HistoryEntryCard from './HistoryEntryCard.svelte';
  import History from '@lucide/svelte/icons/history';
  import Download from '@lucide/svelte/icons/download';
  import Upload from '@lucide/svelte/icons/upload';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import ArrowUpDown from '@lucide/svelte/icons/arrow-up-down';
  import ListFilter from '@lucide/svelte/icons/list-filter';
  import Button from './ui/Button.svelte';
  import MenuItem from './ui/MenuItem.svelte';
  import Input from './ui/Input.svelte';
  import Panel from './ui/Panel.svelte';
  import Popup from './ui/Popup.svelte';
  import ConfirmDialog from './ui/ConfirmDialog.svelte';
  import { displayTitle, entryLayoutCount, entryNodeCount, type HistoryEntry } from './historyModel';
  import {
    readUiPrefs,
    updateUiPrefs,
    type HistorySearchFilter,
    type HistorySortPref,
    type HistoryStatusFilter,
  } from './uiPrefs';
  import { flip } from 'svelte/animate';
  import { untrack, onDestroy } from 'svelte';
  import { createPointerDrag } from './pointerDrag';
  import { createHistoryEntrance, createHistoryOrder, historyMotionDuration } from './historyMotion';
  import { insertIndexFromClient, visualReorderSlots } from './pointerReorder';

  type StatusFilter = HistoryStatusFilter;
  type SearchFilter = HistorySearchFilter;
  type ToolbarPanel = 'sort' | 'filter';
  type HistorySort = HistorySortPref;
  type DragBand = 'queued' | 'history';
  type VisualItem = { kind: 'entry'; entry: HistoryEntry } | { kind: 'ghost'; key: string };

  type VisualRow =
    | {
        kind: 'entry';
        entry: HistoryEntry;
        band: 'queued' | 'running' | 'history';
        draggable: boolean;
      }
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
    onDeleteAll: () => void;
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
    onImport,
    onDeleteAll,
  }: Props = $props();

  let menuId = $state<string | null>(null);
  /** When true, the entry ⋯ menu opens above the trigger to stay in view. */
  let menuOpenUpward = $state(false);
  let menuMaxHeight = $state(168);
  let headerMenuOpen = $state(false);
  let confirmDeleteAll = $state(false);
  const savedToolbar = readUiPrefs().history;
  let query = $state(savedToolbar.query);
  let openPanel = $state<ToolbarPanel | null>(null);
  let statusFilters = $state<StatusFilter[]>([...savedToolbar.statusFilters]);
  let searchFilters = $state<SearchFilter[]>([...savedToolbar.searchFilters]);
  let sort = $state<HistorySort>(savedToolbar.sort);

  let dragBand = $state<DragBand | null>(null);
  let dragFromId = $state<string | null>(null);
  let dragInsertAt = $state<number | null>(null);
  let dragActive = $state(false);
  let dragContainer: HTMLElement | null = null;
  let dragWidth = $state(0);
  let dragHeight = $state(0);
  let dragFloatX = $state(0);
  let dragFloatY = $state(0);

  const statusOptions: { value: StatusFilter; label: string }[] = [
    { value: 'completed', label: 'Done' },
    { value: 'failed', label: 'Failed' },
    { value: 'cancelled', label: 'Cancelled' },
    { value: 'incomplete', label: 'Incomplete' },
    { value: 'unsat', label: 'Impossible' },
  ];

  const searchOptions: { value: SearchFilter; label: string }[] = [
    { value: 'one_min_nl', label: 'One min N/L' },
    { value: 'all_min_nl', label: 'All min N/L' },
    { value: 'all_min_n', label: 'All min N' },
  ];

  const sortOptions: { value: HistorySort; label: string; tip: string }[] = [
    { value: 'manual', label: 'Manual', tip: 'Drag rows to reorder' },
    { value: 'newest', label: 'Newest', tip: 'Most recently created first' },
    { value: 'oldest', label: 'Oldest', tip: 'Oldest created first' },
    { value: 'name-asc', label: 'Name A-Z', tip: 'Alphabetical by title' },
    { value: 'name-desc', label: 'Name Z-A', tip: 'Reverse alphabetical' },
    {
      value: 'layouts-desc',
      label: 'Most layouts',
      tip: 'Highest layout count first',
    },
    { value: 'nodes-asc', label: 'Fewest nodes', tip: 'Smallest N first' },
  ];

  const listEmpty = $derived(queued.length === 0 && !running && history.length === 0);
  const historyDraggable = $derived(sort === 'manual');
  const allEntries = $derived(running ? [...queued, running, ...history] : [...queued, ...history]);
  const filtersActive = $derived(statusFilters.length > 0 || searchFilters.length > 0);
  const sortActive = $derived(sort !== 'manual');

  $effect(() => {
    updateUiPrefs({
      history: {
        query,
        sort,
        statusFilters,
        searchFilters,
      },
    });
  });

  const enterHistoryRow = createHistoryEntrance(untrack(() => allEntries.map((entry) => entry.id)));

  function togglePanel(panel: ToolbarPanel, event: MouseEvent): void {
    event.stopPropagation();
    (event.currentTarget as HTMLElement).focus();
    menuId = null;
    headerMenuOpen = false;
    openPanel = openPanel === panel ? null : panel;
  }

  function toggleHeaderMenu(event: MouseEvent): void {
    event.stopPropagation();
    (event.currentTarget as HTMLElement).focus();
    menuId = null;
    openPanel = null;
    headerMenuOpen = !headerMenuOpen;
  }

  function closeConfirmDeleteAll(): void {
    confirmDeleteAll = false;
  }

  function requestDeleteAll(): void {
    headerMenuOpen = false;
    confirmDeleteAll = true;
  }

  function confirmDeleteAllHistory(): void {
    confirmDeleteAll = false;
    onDeleteAll();
  }

  function clearFilters(): void {
    statusFilters = [];
    searchFilters = [];
  }

  function toggleValue<T extends string>(list: T[], value: T): T[] {
    return list.includes(value) ? list.filter((item) => item !== value) : [...list, value];
  }

  function toggleStatus(value: StatusFilter): void {
    statusFilters = toggleValue(statusFilters, value);
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

  function entrySearch(entry: HistoryEntry): SearchFilter {
    return entry.request.solveMode;
  }

  function matchesStatusFilters(entry: HistoryEntry): boolean {
    return statusFilters.length === 0 || (statusFilters as string[]).includes(entry.status);
  }

  function matchesSearchFilters(entry: HistoryEntry): boolean {
    return searchFilters.length === 0 || searchFilters.includes(entrySearch(entry));
  }

  function matchesFilter(entry: HistoryEntry): boolean {
    return matchesStatusFilters(entry) && matchesSearchFilters(entry);
  }

  function matchesEntry(entry: HistoryEntry): boolean {
    return matchesQuery(entry) && matchesFilter(entry);
  }

  /** Facet counts ignore their own dimension so other chips stay meaningful. */
  function countStatus(value: StatusFilter): number {
    return allEntries.filter((entry) => matchesQuery(entry) && matchesSearchFilters(entry) && entry.status === value)
      .length;
  }

  function countSearch(value: SearchFilter): number {
    return allEntries.filter(
      (entry) => matchesQuery(entry) && matchesStatusFilters(entry) && entrySearch(entry) === value,
    ).length;
  }

  function compareHistory(a: HistoryEntry, b: HistoryEntry): number {
    switch (sort) {
      case 'newest':
        return b.createdAtMs - a.createdAtMs;
      case 'oldest':
        return a.createdAtMs - b.createdAtMs;
      case 'name-asc':
        return displayTitle(a).localeCompare(displayTitle(b), undefined, {
          sensitivity: 'base',
        });
      case 'name-desc':
        return displayTitle(b).localeCompare(displayTitle(a), undefined, {
          sensitivity: 'base',
        });
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
    sort === 'manual' ? history.filter(matchesEntry) : history.filter(matchesEntry).slice().sort(compareHistory),
  );
  const noMatches = $derived(
    !listEmpty && filteredQueued.length === 0 && filteredRunning == null && filteredHistory.length === 0,
  );

  const dragEntry = $derived(
    dragFromId
      ? (queued.find((entry) => entry.id === dragFromId) ?? history.find((entry) => entry.id === dragFromId) ?? null)
      : null,
  );

  function visualList(entries: HistoryEntry[], band: DragBand): VisualItem[] {
    if (!dragActive || dragBand !== band || !dragFromId) {
      return entries.map((entry) => ({ kind: 'entry', entry }));
    }
    const fromIndex = entries.findIndex((entry) => entry.id === dragFromId);
    return visualReorderSlots(entries, fromIndex, dragInsertAt, true).map((slot) =>
      slot.kind === 'ghost' ? { kind: 'ghost', key: `ghost-${band}` } : { kind: 'entry', entry: slot.item },
    );
  }

  const visualQueued = $derived(visualList(filteredQueued, 'queued'));
  const visualHistory = $derived(visualList(filteredHistory, 'history'));

  /** Single list so flip animates queued, running, and history together on insert. */
  const visualRows = $derived.by((): VisualRow[] => {
    const rows: VisualRow[] = [];
    for (const item of visualQueued) {
      if (item.kind === 'ghost') rows.push({ kind: 'ghost', key: item.key });
      else {
        rows.push({
          kind: 'entry',
          entry: item.entry,
          band: 'queued',
          draggable: true,
        });
      }
    }
    if (filteredRunning) {
      rows.push({
        kind: 'entry',
        entry: filteredRunning,
        band: 'running',
        draggable: false,
      });
    }
    for (const item of visualHistory) {
      if (item.kind === 'ghost') rows.push({ kind: 'ghost', key: item.key });
      else {
        rows.push({
          kind: 'entry',
          entry: item.entry,
          band: 'history',
          draggable: historyDraggable,
        });
      }
    }
    return rows;
  });

  // Svelte restarts FLIP on each list reconciliation. Job snapshots must update
  // the row contents without reconciling an unchanged order mid-animation.
  const visualRowsByKey = $derived(
    new Map(visualRows.map((item) => [item.kind === 'ghost' ? item.key : item.entry.id, item])),
  );
  const stabilizeOrder = createHistoryOrder();
  const visualOrder = $derived(stabilizeOrder([...visualRowsByKey.keys()]));

  function dropTargetId(entries: HistoryEntry[], fromId: string, insertAt: number): string | null {
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
    dragContainer = null;
    dragWidth = 0;
    dragHeight = 0;
  }

  const pointerDrag = createPointerDrag({
    move: (frame) => {
      dragActive = true;
      dragFloatX = frame.left;
      dragFloatY = frame.top;
      updateDropTarget(frame.clientY);
    },
    finish: finishDrag,
    cancel: clearDragState,
  });
  onDestroy(pointerDrag.dispose);

  function updateDropTarget(clientY: number): void {
    if (!dragBand || !dragFromId) return;
    const entryEls = [
      ...(dragContainer?.querySelectorAll<HTMLElement>(`[data-history-band="${dragBand}"][data-history-id]`) ?? []),
    ].filter((element) => element.dataset.historyId !== dragFromId);
    dragInsertAt = insertIndexFromClient(entryEls, clientY, 'y');
  }

  function onCardPointerDown(band: DragBand, id: string, event: PointerEvent): void {
    if (event.button !== 0 || isInteractiveTarget(event.target)) return;
    event.preventDefault();
    menuId = null;
    headerMenuOpen = false;
    openPanel = null;
    const card = event.currentTarget as HTMLElement;
    const rect = card.getBoundingClientRect();
    if (!pointerDrag.start(event, rect)) return;
    dragContainer = card.closest('[role="listbox"]');
    const source = band === 'queued' ? filteredQueued : filteredHistory;
    const fromIndex = source.findIndex((entry) => entry.id === id);
    dragBand = band;
    dragFromId = id;
    dragInsertAt = fromIndex >= 0 ? fromIndex : 0;
    dragActive = false;
    dragWidth = rect.width;
    dragHeight = rect.height;
    dragFloatX = rect.left;
    dragFloatY = rect.top;
  }

  function finishDrag(active: boolean): void {
    if (!dragFromId) return;
    const fromId = dragFromId;
    const band = dragBand;
    const insertAt = dragInsertAt;
    const source = band === 'queued' ? filteredQueued : band === 'history' ? filteredHistory : [];
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

  function iconBtnClass(active: boolean): string {
    return active ? '!border-accent/70 !bg-selected !text-accent' : '';
  }

  function toggleMenu(id: string, event: MouseEvent): void {
    event.stopPropagation();
    (event.currentTarget as HTMLElement).focus();
    openPanel = null;
    headerMenuOpen = false;
    if (menuId === id) {
      menuId = null;
      return;
    }
    const trigger = event.currentTarget as HTMLElement;
    const triggerRect = trigger.getBoundingClientRect();
    const list = trigger.closest('[role="listbox"]');
    const bounds = (list ?? document.documentElement).getBoundingClientRect();
    // Four menuitems + padding; keep a little slack so we don't clip the shadow.
    const menuHeight = 168;
    const spaceAbove = Math.max(0, triggerRect.top - bounds.top - 8);
    const spaceBelow = Math.max(0, bounds.bottom - triggerRect.bottom - 8);
    menuOpenUpward = spaceBelow < menuHeight && spaceAbove > spaceBelow;
    menuMaxHeight = menuOpenUpward ? spaceAbove : spaceBelow;
    menuId = id;
  }
</script>

<svelte:window
  onclick={() => {
    menuId = null;
    headerMenuOpen = false;
    openPanel = null;
  }}
  onkeydown={(event) => {
    if (event.key === 'Escape') {
      if (confirmDeleteAll) {
        event.preventDefault();
        closeConfirmDeleteAll();
        return;
      }
      if (dragFromId) {
        event.preventDefault();
        pointerDrag.cancel();
        return;
      }
      menuId = null;
      headerMenuOpen = false;
      openPanel = null;
    }
  }}
/>

<Panel variant="column" element="aside" class="[container-type:size] flex h-full min-h-0 flex-col overflow-hidden">
  <div class="border-line flex min-h-[57px] shrink-0 items-center justify-between gap-2 border-b px-4 py-3">
    <h2 class="m-0 flex items-center gap-2 text-lg font-bold tracking-tight">
      <History class="text-accent size-[1.05rem]" strokeWidth={2.2} aria-hidden="true" />
      History
    </h2>
    <div class="relative">
      <Button
        size="small"
        square
        type="button"
        title="History actions"
        aria-label="History actions"
        aria-expanded={headerMenuOpen}
        aria-haspopup="menu"
        class={iconBtnClass(headerMenuOpen)}
        onclick={toggleHeaderMenu}
      >
        ⋯
      </Button>
      {#if headerMenuOpen}
        <Popup
          label="History actions"
          onclose={() => {
            headerMenuOpen = false;
          }}
          class="border-line bg-panel absolute top-[calc(100%+0.25rem)] right-0 z-20 max-h-[calc(100cqh-4rem)] min-w-48 overflow-y-auto rounded-lg border py-1 shadow-[0_14px_32px_rgb(0_0_0/45%)]"
          role="menu"
        >
          <MenuItem
            type="button"
            onclick={() => {
              headerMenuOpen = false;
              onImport();
            }}
          >
            <Upload class="text-muted size-3.5" />
            Import history…
          </MenuItem>
          <MenuItem
            type="button"
            disabled={listEmpty}
            onclick={() => {
              headerMenuOpen = false;
              onExportAll();
            }}
          >
            <Download class="text-muted size-3.5" />
            Export all…
          </MenuItem>
          <div class="border-line my-1 border-t" role="separator"></div>
          <MenuItem type="button" danger disabled={listEmpty} onclick={() => requestDeleteAll()}>
            <Trash2 class="size-3.5" />
            Delete all history…
          </MenuItem>
        </Popup>
      {/if}
    </div>
  </div>

  <div class="border-line relative shrink-0 border-b px-3 py-2">
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
      <Popup
        label="Sort history"
        onclose={() => {
          openPanel = null;
        }}
        class="rounded-control border-line bg-panel-2 absolute inset-x-2 top-[calc(100%-0.15rem)] z-20 max-h-[calc(100cqh-7rem)] overflow-y-auto border py-1.5 shadow-[0_14px_32px_rgb(0_0_0/45%)]"
        role="dialog"
      >
        <p class="text-dim m-0 px-3 pt-1 pb-1.5 text-[0.65rem] font-bold tracking-[0.08em] uppercase">Sort by</p>
        <div class="flex flex-col gap-0.5 px-1.5 pb-1" role="listbox" aria-label="Sort options">
          {#each sortOptions as option (option.value)}
            <Button
              variant="plain"
              type="button"
              role="option"
              aria-selected={sort === option.value}
              class={`rounded-control border px-2.5 py-2 text-left transition-colors ${
                sort === option.value ? 'border-accent/70 bg-selected' : 'hover:bg-well-hover/70 border-transparent'
              }`}
              onclick={() => chooseSort(option.value)}
            >
              <span class="text-ink block text-xs font-bold">
                {option.label}
              </span>
              <span class="text-muted mt-0.5 block text-[0.68rem]">
                {option.tip}
              </span>
            </Button>
          {/each}
        </div>
      </Popup>
    {:else if openPanel === 'filter'}
      <Popup
        label="Filter history"
        onclose={() => {
          openPanel = null;
        }}
        class="rounded-control border-line bg-panel-2 absolute inset-x-2 top-[calc(100%-0.15rem)] z-20 max-h-[min(28rem,calc(100cqh-7rem))] overflow-y-auto border px-3 py-2.5 shadow-[0_14px_32px_rgb(0_0_0/45%)]"
        role="dialog"
      >
        <div class="mb-2.5 flex items-center justify-between gap-2">
          <p class="text-dim m-0 text-[0.65rem] font-bold tracking-[0.08em] uppercase">Filter</p>
          <Button
            variant="plain"
            type="button"
            class="text-accent hover:text-accent-bright disabled:text-dim cursor-pointer border-0 bg-transparent p-0 text-xs font-semibold disabled:cursor-default"
            disabled={!filtersActive}
            onclick={() => clearFilters()}
          >
            Clear
          </Button>
        </div>

        <div class="flex flex-col gap-3">
          <div>
            <p class="text-muted m-0 mb-1.5 text-[0.7rem] font-semibold">Status</p>
            <div class="flex flex-wrap gap-1.5" role="group" aria-label="Status filters">
              <Button
                variant="chip"
                type="button"
                aria-pressed={statusFilters.length === 0}
                onclick={() => {
                  statusFilters = [];
                }}
              >
                All
              </Button>
              {#each statusOptions as option (option.value)}
                {@const count = countStatus(option.value)}
                <Button
                  variant="chip"
                  type="button"
                  aria-pressed={statusFilters.includes(option.value)}
                  onclick={() => toggleStatus(option.value)}
                >
                  {option.label}
                  <span class="text-dim ml-1 font-medium tabular-nums">
                    {count}
                  </span>
                </Button>
              {/each}
            </div>
          </div>

          <div>
            <p class="text-muted m-0 mb-1.5 text-[0.7rem] font-semibold">Search</p>
            <div class="flex flex-wrap gap-1.5" role="group" aria-label="Search filters">
              <Button
                variant="chip"
                type="button"
                aria-pressed={searchFilters.length === 0}
                onclick={() => {
                  searchFilters = [];
                }}
              >
                Any
              </Button>
              {#each searchOptions as option (option.value)}
                {@const count = countSearch(option.value)}
                <Button
                  variant="chip"
                  type="button"
                  aria-pressed={searchFilters.includes(option.value)}
                  onclick={() => toggleSearch(option.value)}
                >
                  {option.label}
                  <span class="text-dim ml-1 font-medium tabular-nums">
                    {count}
                  </span>
                </Button>
              {/each}
            </div>
          </div>
        </div>
      </Popup>
    {/if}
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto" role="listbox" aria-label="Job history">
    {#if listEmpty}
      <p class="text-dim m-0 px-3 py-3 text-xs">Solve a problem to build history.</p>
    {:else if noMatches}
      <p class="text-dim m-0 px-3 py-3 text-xs">
        No entries match{query.trim() ? ` “${query.trim()}”` : ' these filters'}.
      </p>
    {:else}
      <div class="flex flex-col">
        {#each visualOrder as key (key)}
          {@const item = visualRowsByKey.get(key)!}
          <div
            class={`relative ${item.kind === 'entry' && menuId === item.entry.id ? 'z-20' : ''}`}
            animate:flip={{ duration: historyMotionDuration }}
          >
            {#if item.kind === 'ghost'}
              <div class="history-drag-ghost" style={`height: ${dragHeight}px`} aria-hidden="true"></div>
            {:else}
              <div use:enterHistoryRow={{ id: item.entry.id, band: item.band }}>
                {@render row(item.entry, item.band, item.draggable)}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</Panel>

{#if confirmDeleteAll}
  <ConfirmDialog
    title="Delete all history?"
    description="This permanently clears all saved results and graph edits. This cannot be undone. Any running job will be cancelled."
    confirmLabel="Delete all"
    onconfirm={confirmDeleteAllHistory}
    onclose={closeConfirmDeleteAll}
  />
{/if}

{#if dragActive && dragEntry && dragBand}
  <div
    class="pointer-events-none fixed top-0 left-0 z-80 will-change-transform"
    style={`width: ${dragWidth}px; transform: translate3d(${dragFloatX}px, ${dragFloatY}px, 0);`}
    aria-hidden="true"
  >
    {@render row(dragEntry, dragBand, false, true)}
  </div>
{/if}

{#snippet row(entry: HistoryEntry, band: 'queued' | 'running' | 'history', draggable: boolean, floating = false)}
  <HistoryEntryCard
    {entry}
    {band}
    {draggable}
    {floating}
    selected={entry.id === selectedEntryId}
    hasRunning={filteredRunning != null}
    {runningElapsedLabel}
    menuOpen={menuId === entry.id}
    {menuOpenUpward}
    {menuMaxHeight}
    {onSelect}
    {onRename}
    {onDelete}
    {onCancelRunning}
    {onCopyToNew}
    {onExportEntry}
    onPointerDown={(event) => {
      if (band !== 'running') onCardPointerDown(band, entry.id, event);
    }}
    onToggleMenu={(event) => toggleMenu(entry.id, event)}
    onCloseMenu={() => {
      menuId = null;
    }}
  />
{/snippet}
