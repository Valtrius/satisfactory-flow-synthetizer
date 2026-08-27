export type ReorderAxis = 'x' | 'y';

export type VisualSlot<T> = { kind: 'item'; item: T; index: number } | { kind: 'ghost' };

export function prefersReducedMotion(): boolean {
  try {
    return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  } catch {
    return false;
  }
}

/** Insert index in the list with the dragged item removed, from midpoints along an axis. */
export function insertIndexFromClient(
  elements: Array<Pick<HTMLElement, 'getBoundingClientRect'>>,
  client: number,
  axis: ReorderAxis,
): number {
  let insertAt = elements.length;
  for (let index = 0; index < elements.length; index += 1) {
    const rect = elements[index].getBoundingClientRect();
    const mid = axis === 'x' ? rect.left + rect.width / 2 : rect.top + rect.height / 2;
    if (client < mid) {
      insertAt = index;
      break;
    }
  }
  return insertAt;
}

/**
 * Build list slots while dragging: items without `fromIndex`, plus a ghost at `insertAt`.
 * `insertAt` is an index in the without-dragged list (0..length inclusive).
 */
export function visualReorderSlots<T>(
  items: T[],
  fromIndex: number,
  insertAt: number | null,
  active: boolean,
): VisualSlot<T>[] {
  if (!active || fromIndex < 0 || fromIndex >= items.length) {
    return items.map((item, index) => ({ kind: 'item', item, index }));
  }
  const without = items.map((item, index) => ({ item, index })).filter((entry) => entry.index !== fromIndex);
  const at = Math.max(0, Math.min(without.length, insertAt ?? fromIndex));
  return [
    ...without.slice(0, at).map(({ item, index }) => ({ kind: 'item' as const, item, index })),
    { kind: 'ghost' },
    ...without.slice(at).map(({ item, index }) => ({ kind: 'item' as const, item, index })),
  ];
}

/** Map without-list insert index to a `toIndex` for splice-based reorder helpers. */
export function toIndexFromInsertAt(fromIndex: number, insertAt: number): number {
  return insertAt;
}

export function setListDragging(active: boolean): void {
  document.body.classList.toggle('list-dragging', active);
}
