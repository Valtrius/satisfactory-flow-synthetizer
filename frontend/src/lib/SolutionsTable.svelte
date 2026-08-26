<script lang="ts">
  import type { Solution } from '../types';
  import {
    SORT_LABELS,
    type SortColumn,
    type SortKey,
    flipColumnDir,
    reorderColumns
  } from './solutionSort';

  type Props = {
    solutions: Solution[];
    selectedIndex: number;
    columns: SortColumn[];
    onSelect: (index: number) => void;
    onColumnsChange: (columns: SortColumn[]) => void;
  };

  let {
    solutions,
    selectedIndex,
    columns,
    onSelect,
    onColumnsChange
  }: Props = $props();

  let dragFrom = $state<number | null>(null);
  let dragOver = $state<number | null>(null);
  let headerRow: HTMLTableRowElement | null = $state(null);

  function headerClick(key: SortKey): void {
    if (dragFrom != null) return;
    onColumnsChange(flipColumnDir(columns, key));
  }

  function columnIndexAt(clientX: number): number | null {
    if (!headerRow) return null;
    const headers = [...headerRow.querySelectorAll('th[data-col-index]')];
    for (const header of headers) {
      const rect = header.getBoundingClientRect();
      if (clientX >= rect.left && clientX < rect.right) {
        return Number(header.getAttribute('data-col-index'));
      }
    }
    return null;
  }

  function onHandlePointerDown(index: number, event: PointerEvent): void {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    dragFrom = index;
    dragOver = index;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function onHandlePointerMove(event: PointerEvent): void {
    if (dragFrom == null) return;
    const over = columnIndexAt(event.clientX);
    if (over != null) dragOver = over;
  }

  function finishDrag(event: PointerEvent): void {
    if (dragFrom == null) return;
    const from = dragFrom;
    const to = dragOver ?? columnIndexAt(event.clientX);
    dragFrom = null;
    dragOver = null;
    try {
      (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
    } catch {
      /* already released */
    }
    if (to == null || to === from) return;
    onColumnsChange(reorderColumns(columns, from, to));
  }
</script>

<div class="flex h-full min-h-0 flex-col overflow-hidden bg-[#0d1922]">
  <div class="min-h-0 flex-1 overflow-auto">
    <table class="w-full border-collapse text-sm">
      <caption class="sr-only">
        Sortable layouts. Drag column handles to set sort priority; click labels to flip direction.
      </caption>
      <thead>
        <tr bind:this={headerRow}>
          {#each columns as column, index (column.key)}
            <th
              scope="col"
              data-col-index={index}
              class={`sticky top-0 z-1 select-none border-b border-[#1a2c36] bg-[#101f2a] px-2 py-2.5 text-left text-[0.7rem] font-bold tracking-wide text-muted uppercase ${
                dragOver === index && dragFrom != null && dragFrom !== index
                  ? 'bg-selected ring-1 ring-inset ring-accent/50'
                  : dragFrom === index
                    ? 'opacity-70'
                    : ''
              }`}
            >
              <div class="flex items-center gap-1.5">
                <button
                  type="button"
                  class="cursor-grab touch-none px-0.5 text-base leading-none text-[#5d7180] active:cursor-grabbing"
                  title="Drag to change sort priority"
                  aria-label={`Drag to reorder ${SORT_LABELS[column.key]} column`}
                  onpointerdown={(event) => onHandlePointerDown(index, event)}
                  onpointermove={onHandlePointerMove}
                  onpointerup={finishDrag}
                  onpointercancel={finishDrag}
                >
                  ⠿
                </button>
                <button
                  type="button"
                  class="inline-flex cursor-pointer items-center gap-1 border-0 bg-transparent p-0 font-bold tracking-wide text-muted uppercase"
                  title="Click to flip sort direction"
                  onclick={() => headerClick(column.key)}
                >
                  <span
                    class={`inline-grid size-4 place-items-center rounded-sm text-[0.65rem] font-extrabold ${
                      index === 0
                        ? 'bg-accent text-[#1a1008]'
                        : index === 1
                          ? 'bg-flow text-root'
                          : 'bg-[#5d7180] text-[#eaf1f5]'
                    }`}
                  >
                    {index + 1}
                  </span>
                  {SORT_LABELS[column.key]}
                  <span class="text-accent">{column.dir === 'asc' ? '↑' : '↓'}</span>
                </button>
              </div>
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each solutions as solution, index (index)}
          <tr
            class={`cursor-pointer border-b border-[#1a2c36] tabular-nums hover:bg-[#122430] ${
              index === selectedIndex ? 'bg-selected shadow-[inset_3px_0_0_var(--color-accent)]' : ''
            }`}
            onclick={() => onSelect(index)}
            aria-selected={index === selectedIndex}
          >
            {#each columns as column, columnIndex (column.key)}
              <td
                class={`px-3 py-2.5 ${index === selectedIndex && columnIndex === 0 ? 'font-bold text-accent' : 'text-[#dfe9ed]'}`}
              >
                {#if column.key === 'belts'}
                  {solution.stats.linkCount ?? solution.stats.beltCount ?? '—'}
                {:else if column.key === 'peak'}
                  {solution.stats.internalMaxThroughput?.exact ?? '—'}
                {:else}
                  {solution.stats.feedbackLoops}
                {/if}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>
