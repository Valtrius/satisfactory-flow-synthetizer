export type SortKey = 'belts' | 'peak' | 'feedbacks';
export type SortDir = 'asc' | 'desc';

export type SortColumn = {
  key: SortKey;
  dir: SortDir;
};

/** Default: belts ↑, peak ↑, feedbacks ↑ */
export const DEFAULT_SORT_COLUMNS: SortColumn[] = [
  { key: 'belts', dir: 'asc' },
  { key: 'peak', dir: 'asc' },
  { key: 'feedbacks', dir: 'asc' }
];

export const SORT_LABELS: Record<SortKey, string> = {
  belts: 'Belts',
  peak: 'Peak',
  feedbacks: 'Feedbacks'
};

function peakValue(exact: string): number {
  const asNumber = Number(exact);
  if (Number.isFinite(asNumber)) return asNumber;
  if (exact.includes('/')) {
    const [num, den] = exact.split('/').map((part) => Number(part));
    if (Number.isFinite(num) && Number.isFinite(den) && den !== 0) return num / den;
  }
  return 0;
}

export function metricValue(
  solution: {
    stats: {
      linkCount?: number | null;
      beltCount?: number | null;
      feedbackLoops: number;
      internalMaxThroughput?: { exact: string } | null;
    };
  },
  key: SortKey
): number {
  switch (key) {
    case 'belts':
      return solution.stats.linkCount ?? solution.stats.beltCount ?? 0;
    case 'peak':
      return peakValue(solution.stats.internalMaxThroughput?.exact ?? '0');
    case 'feedbacks':
      return solution.stats.feedbackLoops;
  }
}

export function compareSolutions(
  left: Parameters<typeof metricValue>[0],
  right: Parameters<typeof metricValue>[0],
  columns: SortColumn[]
): number {
  for (const column of columns) {
    const delta = metricValue(left, column.key) - metricValue(right, column.key);
    if (delta !== 0) return column.dir === 'asc' ? delta : -delta;
  }
  return 0;
}

export function sortSolutions<T extends Parameters<typeof metricValue>[0]>(
  solutions: T[],
  columns: SortColumn[]
): T[] {
  return [...solutions].sort((left, right) => compareSolutions(left, right, columns));
}

export function flipColumnDir(columns: SortColumn[], key: SortKey): SortColumn[] {
  return columns.map((column) =>
    column.key === key
      ? { ...column, dir: column.dir === 'asc' ? 'desc' : 'asc' }
      : column
  );
}

/** Move `fromIndex` column to `toIndex`; order is primary → tertiary. */
export function reorderColumns(
  columns: SortColumn[],
  fromIndex: number,
  toIndex: number
): SortColumn[] {
  if (
    fromIndex === toIndex ||
    fromIndex < 0 ||
    toIndex < 0 ||
    fromIndex >= columns.length ||
    toIndex >= columns.length
  ) {
    return columns;
  }
  const next = [...columns];
  const [moved] = next.splice(fromIndex, 1);
  next.splice(toIndex, 0, moved);
  return next;
}
