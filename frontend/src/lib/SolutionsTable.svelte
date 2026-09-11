<script lang="ts">
  import { flip } from 'svelte/animate';
  import { onDestroy } from 'svelte';
  import { createPointerDrag } from './pointerDrag';
  import Button from './ui/Button.svelte';
  import type { Solution } from '../types';
  import { insertIndexFromClient, prefersReducedMotion, visualReorderSlots } from './pointerReorder';
  import { SORT_LABELS, type SortColumn, type SortKey, flipColumnDir, reorderColumns } from './solutionSort';

  type Props = {
    solutions: Solution[];
    selectedIndex: number;
    columns: SortColumn[];
    onSelect: (index: number) => void;
    onColumnsChange: (columns: SortColumn[]) => void;
  };

  let { solutions, selectedIndex, columns, onSelect, onColumnsChange }: Props = $props();

  let dragFrom = $state<number | null>(null);
  let dragInsertAt = $state<number | null>(null);
  let dragActive = $state(false);
  let dragContainer: HTMLElement | null = null;
  let dragWidth = $state(0);
  let dragHeight = $state(0);
  let dragFloatX = $state(0);
  let dragFloatY = $state(0);
  let dragReduceMotion = false;

  const dragColumn = $derived(
    dragFrom != null && dragFrom >= 0 && dragFrom < columns.length ? columns[dragFrom] : null,
  );
  const flipDuration = $derived(dragReduceMotion || !dragActive ? 0 : 220);
  const visualColumns = $derived(visualReorderSlots(columns, dragFrom ?? -1, dragInsertAt, dragActive));

  function clearDragState(): void {
    dragFrom = null;
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
      updateDropTarget(frame.clientX);
    },
    finish: finishDrag,
    cancel: clearDragState,
  });
  onDestroy(pointerDrag.dispose);

  function headerClick(key: SortKey): void {
    if (dragActive || dragFrom != null) return;
    onColumnsChange(flipColumnDir(columns, key));
  }

  function priorityClass(rank: number): string {
    if (rank === 0) return 'bg-accent text-[#1a1008]';
    if (rank === 1) return 'bg-flow text-root';
    return 'bg-[#5d7180] text-[#eaf1f5]';
  }

  function updateDropTarget(clientX: number): void {
    const headers = [...(dragContainer?.querySelectorAll<HTMLElement>('th[data-col-source-index]') ?? [])].filter(
      (element) => Number(element.dataset.colSourceIndex) !== dragFrom,
    );
    dragInsertAt = insertIndexFromClient(headers, clientX, 'x');
  }

  function onHandlePointerDown(index: number, event: PointerEvent): void {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    dragReduceMotion = prefersReducedMotion();
    const header = (event.currentTarget as HTMLElement).closest('th');
    const rect = (header ?? (event.currentTarget as HTMLElement)).getBoundingClientRect();
    if (!pointerDrag.start(event, rect)) return;
    dragContainer = (event.currentTarget as HTMLElement).closest('table');
    dragFrom = index;
    dragInsertAt = index;
    dragActive = false;
    dragWidth = rect.width;
    dragHeight = rect.height;
    dragFloatX = rect.left;
    dragFloatY = rect.top;
  }

  function finishDrag(active: boolean): void {
    if (dragFrom == null) return;
    const from = dragFrom;
    const insertAt = dragInsertAt;
    clearDragState();
    if (!active || insertAt == null) return;
    const to = insertAt;
    if (to === from) return;
    onColumnsChange(reorderColumns(columns, from, to));
  }

  function cellValue(solution: Solution, key: SortKey): string {
    if (key === 'belts') return String(solution.stats.linkCount ?? solution.stats.beltCount ?? '—');
    if (key === 'peak') return solution.stats.internalMaxThroughput?.exact ?? '—';
    return String(solution.stats.feedbackLoops);
  }
  function navigateLayouts(event: KeyboardEvent, index: number): void {
    const keys: Record<string, number> = {
      ArrowDown: Math.min(solutions.length - 1, index + 1),
      ArrowUp: Math.max(0, index - 1),
      Home: 0,
      End: solutions.length - 1,
    };
    const next = keys[event.key];
    if (next === undefined) return;
    event.preventDefault();
    (event.currentTarget as HTMLElement)
      .closest('table')
      ?.querySelector<HTMLButtonElement>(`[data-layout-select="${next}"]`)
      ?.focus();
    onSelect(next);
  }
</script>

<div class="flex h-full min-h-0 flex-col overflow-hidden bg-[#0d1922]">
  <div class="min-h-0 flex-1 overflow-auto">
    <table class="w-full border-collapse text-sm">
      <caption class="sr-only">
        Select layouts with their buttons or Up/Down, Home and End. Drag column handles to set sort priority; click
        labels to flip direction.
      </caption>
      <thead>
        <tr>
          {#each visualColumns as item, visualIndex (item.kind === 'ghost' ? 'ghost' : item.item.key)}
            <th
              scope="col"
              aria-sort={item.kind === 'item' && item.index === 0
                ? item.item.dir === 'asc'
                  ? 'ascending'
                  : 'descending'
                : undefined}
              data-col-source-index={item.kind === 'item' ? item.index : undefined}
              class={item.kind === 'ghost'
                ? 'list-drag-ghost list-drag-ghost--x sticky top-0 z-1'
                : 'text-muted sticky top-0 z-1 border-b border-[#1a2c36] bg-[#101f2a] px-2 py-2.5 text-left text-[0.7rem] font-bold tracking-wide uppercase select-none'}
              style={item.kind === 'ghost'
                ? `width: ${dragWidth}px; min-width: ${dragWidth}px; height: ${dragHeight}px`
                : undefined}
              aria-hidden={item.kind === 'ghost' ? true : undefined}
              animate:flip={{ duration: flipDuration }}
            >
              {#if item.kind === 'item'}
                {@const column = item.item}
                {@const index = item.index}
                <div class="flex items-center gap-1.5">
                  <Button
                    variant="plain"
                    type="button"
                    class="cursor-grab touch-none px-0.5 text-base leading-none text-[#5d7180]"
                    title="Drag to change sort priority"
                    aria-label={`Drag to reorder ${SORT_LABELS[column.key]} column`}
                    onpointerdown={(event) => onHandlePointerDown(index, event)}
                  >
                    ⠿
                  </Button>
                  <Button
                    variant="plain"
                    type="button"
                    class="text-muted inline-flex cursor-pointer items-center gap-1 border-0 bg-transparent p-0 font-bold tracking-wide uppercase"
                    title="Click to flip sort direction"
                    onclick={() => headerClick(column.key)}
                  >
                    <span
                      class={`inline-grid size-4 place-items-center rounded-sm text-[0.65rem] font-extrabold ${priorityClass(visualIndex)}`}
                    >
                      {visualIndex + 1}
                    </span>
                    {SORT_LABELS[column.key]}
                    <span class="text-accent">
                      {column.dir === 'asc' ? '↑' : '↓'}
                    </span>
                  </Button>
                </div>
              {/if}
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each solutions as solution, rowIndex (rowIndex)}
          <tr
            class={`cursor-pointer border-b border-[#1a2c36] tabular-nums hover:bg-[#122430] ${
              rowIndex === selectedIndex ? 'bg-selected shadow-[inset_3px_0_0_var(--color-accent)]' : ''
            }`}
            onclick={() => onSelect(rowIndex)}
          >
            {#each visualColumns as item, columnIndex (item.kind === 'ghost' ? `ghost-${rowIndex}` : `${item.item.key}-${rowIndex}`)}
              <td
                class={item.kind === 'ghost'
                  ? 'list-drag-ghost list-drag-ghost--x'
                  : `px-3 py-2.5 ${rowIndex === selectedIndex && columnIndex === 0 ? 'text-accent font-bold' : 'text-[#dfe9ed]'}`}
                style={item.kind === 'ghost' ? `width: ${dragWidth}px; min-width: ${dragWidth}px` : undefined}
                aria-hidden={item.kind === 'ghost' ? true : undefined}
              >
                {#if item.kind === 'item'}
                  {#if item.index === 0}
                    <Button
                      variant="plain"
                      class="w-full text-left tabular-nums"
                      data-layout-select={rowIndex}
                      aria-label={`Select layout ${rowIndex + 1}`}
                      aria-pressed={rowIndex === selectedIndex}
                      tabindex={rowIndex === selectedIndex ? 0 : -1}
                      onclick={(event) => {
                        event.stopPropagation();
                        onSelect(rowIndex);
                      }}
                      onkeydown={(event) => navigateLayouts(event, rowIndex)}
                    >
                      {cellValue(solution, item.item.key)}
                    </Button>
                  {:else}
                    {cellValue(solution, item.item.key)}
                  {/if}
                {/if}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

{#if dragActive && dragColumn}
  <div
    class="list-drag-float"
    style={`width: ${dragWidth}px; transform: translate3d(${dragFloatX}px, ${dragFloatY}px, 0);`}
    aria-hidden="true"
  >
    <div
      class="list-drag-float-card text-muted rounded-sm border border-[#1a2c36] bg-[#101f2a] px-2 py-2.5 text-left text-[0.7rem] font-bold tracking-wide uppercase"
    >
      <div class="flex items-center gap-1.5">
        <span class="px-0.5 text-base leading-none text-[#5d7180]">⠿</span>
        <span class="text-muted inline-flex items-center gap-1 font-bold tracking-wide uppercase">
          <span
            class={`inline-grid size-4 place-items-center rounded-sm text-[0.65rem] font-extrabold ${priorityClass(dragInsertAt ?? dragFrom ?? 0)}`}
          >
            {(dragInsertAt ?? dragFrom ?? 0) + 1}
          </span>
          {SORT_LABELS[dragColumn.key]}
          <span class="text-accent">
            {dragColumn.dir === 'asc' ? '↑' : '↓'}
          </span>
        </span>
      </div>
    </div>
  </div>
{/if}
