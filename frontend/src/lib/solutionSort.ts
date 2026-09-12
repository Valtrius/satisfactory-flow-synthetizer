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
  { key: 'feedbacks', dir: 'asc' },
];

export const SORT_LABELS: Record<SortKey, string> = {
  belts: 'Belts',
  peak: 'Peak',
  feedbacks: 'Feedbacks',
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
  key: SortKey,
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
  columns: SortColumn[],
): number {
  for (const column of columns) {
    const delta =
      column.key === 'peak'
        ? compareExactRates(
            left.stats.internalMaxThroughput?.exact ?? '0',
            right.stats.internalMaxThroughput?.exact ?? '0',
          )
        : metricValue(left, column.key) - metricValue(right, column.key);
    if (delta !== 0) return column.dir === 'asc' ? delta : -delta;
  }
  return 0;
}

const rateCache = new Map<string, [bigint, bigint]>();
function exactRate(value: string): [bigint, bigint] | null {
  const cached = rateCache.get(value);
  if (cached) return cached;
  let result: [bigint, bigint];
  const fraction = /^([+-]?\d+)\/([+-]?\d+)$/.exec(value);
  const decimal = /^([+-]?)(\d+)(?:\.(\d*))?$/.exec(value);
  if (fraction) {
    const denominator = BigInt(fraction[2]);
    if (denominator === 0n) return null;
    result = [BigInt(fraction[1]) * (denominator < 0n ? -1n : 1n), denominator < 0n ? -denominator : denominator];
  } else if (decimal) {
    const digits = decimal[3] ?? '';
    result = [BigInt(decimal[2] + digits) * (decimal[1] === '-' ? -1n : 1n), 10n ** BigInt(digits.length)];
  } else return null;
  if (rateCache.size >= 512) rateCache.clear();
  rateCache.set(value, result);
  return result;
}

function compareExactRates(left: string, right: string): number {
  const a = exactRate(left);
  const b = exactRate(right);
  if (!a || !b) return peakValue(left) - peakValue(right);
  const delta = a[0] * b[1] - b[0] * a[1];
  return delta < 0n ? -1 : delta > 0n ? 1 : 0;
}

export function flipColumnDir(columns: SortColumn[], key: SortKey): SortColumn[] {
  return columns.map((column) =>
    column.key === key ? { ...column, dir: column.dir === 'asc' ? 'desc' : 'asc' } : column,
  );
}

/** Move `fromIndex` column to `toIndex`; order is primary → tertiary. */
export function reorderColumns(columns: SortColumn[], fromIndex: number, toIndex: number): SortColumn[] {
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
