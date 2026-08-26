<script lang="ts">
  import Download from '@lucide/svelte/icons/download';
  import Upload from '@lucide/svelte/icons/upload';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Copy from '@lucide/svelte/icons/copy';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import X from '@lucide/svelte/icons/x';
  import Button from './ui/Button.svelte';
  import Input from './ui/Input.svelte';
  import Panel from './ui/Panel.svelte';
  import {
    displayTitle,
    entryOutcomeLine,
    type HistoryEntry
  } from './historyModel';

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
  let query = $state('');

  let dragBand = $state<'queued' | 'history' | null>(null);
  let dragFromId = $state<string | null>(null);
  let dragOverId = $state<string | null>(null);
  let dragMoved = $state(false);
  let dragOriginX = 0;
  let dragOriginY = 0;

  const listEmpty = $derived(queued.length === 0 && !running && history.length === 0);

  function matchesQuery(entry: HistoryEntry): boolean {
    const needle = query.trim().toLowerCase();
    if (!needle) return true;
    return displayTitle(entry).toLowerCase().includes(needle);
  }

  const filteredQueued = $derived(queued.filter(matchesQuery));
  const filteredRunning = $derived(running && matchesQuery(running) ? running : null);
  const filteredHistory = $derived(history.filter(matchesQuery));
  const noMatches = $derived(
    !listEmpty &&
      filteredQueued.length === 0 &&
      filteredRunning == null &&
      filteredHistory.length === 0
  );

  function startRename(entry: HistoryEntry): void {
    menuId = null;
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

  function isInteractiveTarget(target: EventTarget | null): boolean {
    if (!(target instanceof Element)) return false;
    return Boolean(target.closest('button, input, textarea, a, [role="menuitem"]'));
  }

  function onCardPointerDown(
    band: 'queued' | 'history',
    id: string,
    event: PointerEvent
  ): void {
    if (event.button !== 0 || isInteractiveTarget(event.target) || renamingId === id) return;
    event.preventDefault();
    dragBand = band;
    dragFromId = id;
    dragOverId = id;
    dragMoved = false;
    dragOriginX = event.clientX;
    dragOriginY = event.clientY;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function onCardPointerMove(event: PointerEvent): void {
    if (!dragBand || !dragFromId) return;
    if (
      Math.abs(event.clientX - dragOriginX) > 3 ||
      Math.abs(event.clientY - dragOriginY) > 3
    ) {
      dragMoved = true;
    }
    const row = document
      .elementsFromPoint(event.clientX, event.clientY)
      .map((el) => (el instanceof Element ? el.closest<HTMLElement>('[data-history-id]') : null))
      .find(
        (el): el is HTMLElement =>
          Boolean(el && el.dataset.historyId && el.dataset.historyId !== dragFromId)
      );
    if (!row) return;
    const id = row.dataset.historyId;
    const band = row.dataset.historyBand;
    if (id && band === dragBand) dragOverId = id;
  }

  function finishCardPointer(event: PointerEvent, entryId: string): void {
    if (!dragBand || !dragFromId) {
      return;
    }
    const fromId = dragFromId;
    const toId = dragOverId;
    const band = dragBand;
    const moved = dragMoved;
    dragBand = null;
    dragFromId = null;
    dragOverId = null;
    dragMoved = false;
    try {
      (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
    } catch {
      /* already released */
    }
    if (moved && toId && toId !== fromId) {
      if (band === 'queued') onReorderQueued(fromId, toId);
      else onReorderHistory(fromId, toId);
      return;
    }
    if (!moved) onSelect(entryId);
  }

  function toggleMenu(id: string, event: MouseEvent): void {
    event.stopPropagation();
    menuId = menuId === id ? null : id;
  }

  function subtitle(entry: HistoryEntry, band: 'queued' | 'running' | 'history'): string {
    if (band === 'running') {
      if (entry.status === 'cancelling') return `Stopping… · ${runningElapsedLabel}`;
      return runningElapsedLabel || '0:00.0';
    }
    return entryOutcomeLine(entry);
  }
</script>

<svelte:window
  onclick={() => {
    menuId = null;
  }}
/>

<Panel
  element="aside"
  class="flex h-[calc(100dvh-2rem)] min-h-140 flex-col overflow-hidden"
>
  <div class="flex items-center justify-between gap-2 border-b border-line px-4 py-3">
    <h2 class="m-0 text-lg font-bold tracking-tight">History</h2>
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

  <div class="shrink-0 border-b border-line px-3 py-2">
    <Input
      type="search"
      class="h-9 text-xs"
      placeholder="Search by name…"
      aria-label="Search history by name"
      bind:value={query}
    />
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto px-2 py-2" role="listbox" aria-label="Job history">
    {#if listEmpty}
      <p class="m-0 px-2 py-3 text-xs text-dim">Solve a problem to build history.</p>
    {:else if noMatches}
      <p class="m-0 px-2 py-3 text-xs text-dim">No entries match “{query.trim()}”.</p>
    {:else}
      <div class="flex flex-col gap-1.5">
        {#each filteredQueued as entry (entry.id)}
          {@render row(entry, 'queued', true)}
        {/each}
        {#if filteredRunning}
          {@render row(filteredRunning, 'running', false)}
        {/if}
        {#each filteredHistory as entry (entry.id)}
          {@render row(entry, 'history', true)}
        {/each}
      </div>
    {/if}
  </div>
</Panel>

{#snippet row(entry: HistoryEntry, band: 'queued' | 'running' | 'history', draggable: boolean)}
  {@const selected = entry.id === selectedEntryId}
  {@const dragging = dragFromId === entry.id}
  {@const dropTarget = dragOverId === entry.id && dragFromId !== entry.id && dragBand === band}
  <div
    role="option"
    tabindex="0"
    aria-selected={selected}
    data-history-id={entry.id}
    data-history-band={band}
    class={`relative grid grid-cols-[minmax(0,1fr)_auto] items-start gap-2 rounded-lg px-2.5 py-2.5 ${
      band === 'queued'
        ? 'border border-dashed border-[#4a6574]'
        : band === 'history'
          ? 'border border-solid border-line'
          : 'border border-solid border-transparent'
    } ${
      selected && band !== 'running' ? 'bg-selected shadow-[inset_3px_0_0_var(--color-accent)]' : ''
    } ${selected && band === 'running' ? 'shadow-[inset_3px_0_0_var(--color-accent)]' : ''} ${
      !selected && band !== 'running' ? 'bg-transparent hover:bg-well-hover/55' : ''
    } ${band === 'running' ? 'history-entry-running cursor-pointer' : ''} ${
      selected && band === 'running' ? 'history-entry-running--selected' : ''
    } ${draggable ? 'cursor-grab active:cursor-grabbing' : ''} ${
      dragging ? 'history-entry-dragging z-2' : ''
    }`}
    onpointerdown={draggable && (band === 'queued' || band === 'history')
      ? (event) => onCardPointerDown(band, entry.id, event)
      : undefined}
    onpointermove={draggable ? onCardPointerMove : undefined}
    onpointerup={draggable
      ? (event) => finishCardPointer(event, entry.id)
      : undefined}
    onpointercancel={draggable
      ? (event) => finishCardPointer(event, entry.id)
      : undefined}
    onclick={() => {
      if (draggable) return;
      onSelect(entry.id);
    }}
    onkeydown={(event) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        onSelect(entry.id);
      }
    }}
  >
    {#if dropTarget && dragMoved}
      <div
        class="pointer-events-none absolute inset-x-1 -top-1 z-3 h-1 rounded-full bg-accent shadow-[0_0_10px_rgb(255_138_61/70%)]"
        aria-hidden="true"
      ></div>
    {/if}
    <div class="min-w-0">
      {#if renamingId === entry.id}
        <input
          class="w-full rounded border border-accent bg-[#08141c] px-1.5 py-0.5 text-xs font-bold text-ink"
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
        <p class="m-0 text-xs leading-snug font-bold wrap-break-word">{displayTitle(entry)}</p>
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
          onclick={(event) => {
            event.stopPropagation();
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
          onclick={(event) => toggleMenu(entry.id, event)}
        >
          ⋯
        </Button>
      {/if}
    </div>

    {#if menuId === entry.id}
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
