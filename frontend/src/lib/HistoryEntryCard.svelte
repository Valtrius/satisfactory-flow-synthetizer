<script lang="ts">
  import Route from '@lucide/svelte/icons/route';
  import Download from '@lucide/svelte/icons/download';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Copy from '@lucide/svelte/icons/copy';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import X from '@lucide/svelte/icons/x';
  import Target from '@lucide/svelte/icons/target';
  import LayoutGrid from '@lucide/svelte/icons/layout-grid';
  import Network from '@lucide/svelte/icons/network';
  import Table2 from '@lucide/svelte/icons/table-2';
  import Button from './ui/Button.svelte';
  import Input from './ui/Input.svelte';
  import MenuItem from './ui/MenuItem.svelte';
  import Popup from './ui/Popup.svelte';
  import {
    displayTitle,
    entryElapsedMs,
    entryHistoryMetrics,
    entryStatusCaption,
    type HistoryEntry,
  } from './historyModel';
  import { formatElapsed } from './searchStage';
  import { enumeratesLayouts } from '../types';

  type Props = {
    entry: HistoryEntry;
    band: 'queued' | 'running' | 'history';
    draggable: boolean;
    floating?: boolean;
    selected: boolean;
    hasRunning: boolean;
    runningElapsedLabel: string;
    menuOpen: boolean;
    menuOpenUpward: boolean;
    menuMaxHeight?: number;
    onSelect: (id: string) => void;
    onRename: (id: string, title: string | null) => void;
    onDelete: (id: string) => void;
    onCancelRunning: () => void;
    onCopyToNew: (id: string) => void;
    onExportEntry: (id: string) => void;
    onPointerDown?: (event: PointerEvent) => void;
    onToggleMenu: (event: MouseEvent) => void;
    onCloseMenu: () => void;
  };
  let {
    entry,
    band,
    draggable,
    floating = false,
    selected,
    hasRunning,
    runningElapsedLabel,
    menuOpen,
    menuOpenUpward,
    menuMaxHeight = 168,
    onSelect,
    onRename,
    onDelete,
    onCancelRunning,
    onCopyToNew,
    onExportEntry,
    onPointerDown,
    onToggleMenu,
    onCloseMenu,
  }: Props = $props();
  let renaming = $state(false);
  let renameDraft = $state('');
  let renameIgnoreBlur = false;
  const metrics = $derived(entryHistoryMetrics(entry));
  const allLayouts = $derived(enumeratesLayouts(entry.request.solveMode));

  function startRename(entry: HistoryEntry): void {
    onCloseMenu();
    renameIgnoreBlur = false;
    renameDraft = displayTitle(entry);
    renaming = true;
  }
  function commitRename(entry: HistoryEntry): void {
    if (!renaming) return;
    onRename(entry.id, renameDraft.trim() || null);
    renaming = false;
  }
  function cancelRename(): void {
    renaming = false;
  }
  function subtitle(entry: HistoryEntry, band: 'queued' | 'running' | 'history'): string {
    if (band === 'running') {
      if (entry.status === 'cancelling') return `Stopping… · ${runningElapsedLabel}`;
      return runningElapsedLabel || '0:00.0';
    }
    const status = entryStatusCaption(entry);
    if (band === 'queued') return status;
    const elapsed = entry.startedAtMs != null ? formatElapsed(entryElapsedMs(entry)) : '';
    if (elapsed && status) return `${elapsed} · ${status}`;
    return status || elapsed;
  }
</script>

<div
  role="option"
  tabindex={floating ? -1 : 0}
  aria-selected={selected}
  data-history-id={floating ? undefined : entry.id}
  data-history-band={floating ? undefined : band}
  style:--entry-menu-height={`${menuMaxHeight}px`}
  class={`border-b-line relative grid grid-cols-[minmax(0,1fr)_auto] items-start gap-x-2 gap-y-1 border-y border-solid border-t-transparent px-3 py-2.5 transition-opacity duration-150 motion-reduce:transition-none ${
    menuOpen && !floating ? 'z-20' : ''
  } ${
    band === 'queued' && !floating && hasRunning
      ? 'opacity-[0.52] hover:opacity-[0.88] aria-selected:opacity-[0.88]'
      : ''
  } ${
    selected && band !== 'running' ? 'bg-selected shadow-[inset_3px_0_0_var(--color-accent)]' : ''
  } ${selected && band === 'running' ? 'shadow-[inset_3px_0_0_var(--color-accent)]' : ''} ${
    !selected && band !== 'running' && !floating ? 'hover:bg-well-hover/55 bg-transparent' : ''
  } ${band === 'running' ? 'history-entry-running cursor-pointer' : ''} ${
    selected && band === 'running' ? 'history-entry-running--selected' : ''
  } ${draggable && !floating ? 'cursor-grab' : ''} ${floating ? 'history-drag-float-card border-solid' : ''}`}
  onpointerdown={draggable && !floating && !renaming ? onPointerDown : undefined}
  onclick={() => {
    if (draggable || floating) return;
    onSelect(entry.id);
  }}
  onkeydown={(event) => {
    if (floating || event.target !== event.currentTarget) return;
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onSelect(entry.id);
    }
  }}
>
  <div class="min-w-0">
    {#if renaming && !floating}
      <Input
        size="inline"
        focusOnMount
        class="border-accent bg-[#08141c]"
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
      <p class="m-0 text-sm leading-snug font-bold wrap-break-word">
        {displayTitle(entry)}
      </p>
    {/if}
    <p class="text-muted m-0 mt-0.5 text-[0.7rem] tabular-nums">
      {subtitle(entry, band)}
    </p>
  </div>

  <div class="relative flex items-start">
    {#if band === 'running'}
      <Button
        size="tiny"
        square
        variant="danger"
        type="button"
        title="Cancel and keep any layouts found"
        aria-label="Cancel running job"
        onclick={(event) => {
          event.stopPropagation();
          onCancelRunning();
        }}
      >
        <X class="size-3.5" />
      </Button>
    {:else if band === 'queued'}
      <Button
        size="tiny"
        square
        variant="quiet"
        type="button"
        title="Remove from queue"
        aria-label="Remove from queue"
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
        size="tiny"
        square
        variant="quiet"
        type="button"
        title="More actions"
        aria-label="More actions"
        tabindex={floating ? -1 : undefined}
        aria-expanded={menuOpen}
        aria-haspopup="menu"
        onclick={(event) => {
          if (floating) {
            event.stopPropagation();
            return;
          }
          onToggleMenu(event);
        }}
      >
        ⋯
      </Button>
      {#if menuOpen && !floating}
        <Popup
          label="Entry actions"
          onclose={() => {
            onCloseMenu();
          }}
          class={`border-line bg-panel absolute right-0 z-20 max-h-(--entry-menu-height) min-w-44 overflow-y-auto rounded-lg border py-1 shadow-[0_14px_32px_rgb(0_0_0/45%)] ${
            menuOpenUpward ? 'bottom-full mb-1' : 'top-full mt-1'
          }`}
          role="menu"
        >
          <MenuItem type="button" onclick={() => startRename(entry)}>
            <Pencil class="text-muted size-3.5" />
            Rename
          </MenuItem>
          <MenuItem
            type="button"
            onclick={() => {
              onCloseMenu();
              onCopyToNew(entry.id);
            }}
          >
            <Copy class="text-muted size-3.5" />
            Copy to New problem
          </MenuItem>
          <MenuItem
            type="button"
            onclick={() => {
              onCloseMenu();
              onExportEntry(entry.id);
            }}
          >
            <Download class="text-muted size-3.5" />
            Export entry…
          </MenuItem>
          <MenuItem
            type="button"
            danger
            onclick={() => {
              onCloseMenu();
              onDelete(entry.id);
            }}
          >
            <Trash2 class="size-3.5" />
            Delete
          </MenuItem>
        </Popup>
      {/if}
    {/if}
  </div>

  <ul class="col-span-2 m-0 grid list-none grid-cols-2 gap-x-2 gap-y-0.5 p-0">
    <li
      class="text-muted inline-flex min-w-0 items-center gap-1 text-[0.68rem] font-semibold tabular-nums"
      title={metrics.search.tip}
      aria-label={metrics.search.tip}
    >
      {#if allLayouts}
        <LayoutGrid class="text-accent-bright size-3 shrink-0" strokeWidth={2.2} aria-hidden="true" />
      {:else}
        <Target class="text-accent-bright size-3 shrink-0" strokeWidth={2.2} aria-hidden="true" />
      {/if}
      <span class="truncate">{metrics.search.value}</span>
    </li>
    <li
      class="text-muted inline-flex min-w-0 items-center gap-1 text-[0.68rem] font-semibold tabular-nums"
      title={metrics.layouts.tip}
      aria-label={metrics.layouts.tip}
    >
      <Table2 class="text-warning size-3 shrink-0" strokeWidth={2.2} aria-hidden="true" />
      <span class="truncate">{metrics.layouts.value}</span>
    </li>
    <li
      class="text-muted inline-flex min-w-0 items-center gap-1 text-[0.68rem] font-semibold tabular-nums"
      title={metrics.nodes.tip}
      aria-label={metrics.nodes.tip}
    >
      <Network class="size-3 shrink-0 text-[#9ec5d6]" strokeWidth={2.2} aria-hidden="true" />
      <span class="truncate">{metrics.nodes.value}</span>
    </li>
    <li
      class="text-muted inline-flex min-w-0 items-center gap-1 text-[0.68rem] font-semibold tabular-nums"
      title={metrics.belts.tip}
      aria-label={metrics.belts.tip}
    >
      <Route class="text-flow size-3 shrink-0" strokeWidth={2.2} aria-hidden="true" />
      <span class="truncate">{metrics.belts.value}</span>
    </li>
  </ul>
</div>
