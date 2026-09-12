import { describe, expect, it } from 'vitest';
import { DEFAULT_SORT_COLUMNS, compareSolutions, flipColumnDir, reorderColumns, sortSolutions } from './solutionSort';

function sol(belts: number, peak: string, feedbacks: number) {
  return {
    stats: {
      linkCount: belts,
      beltCount: belts,
      feedbackLoops: feedbacks,
      internalMaxThroughput: { exact: peak },
    },
  };
}

describe('solutionSort', () => {
  it('defaults to belts, then peak, then feedbacks ascending', () => {
    const sorted = sortSolutions([sol(12, '600', 0), sol(11, '600', 1), sol(12, '480', 1)], DEFAULT_SORT_COLUMNS);
    expect(sorted.map((item) => item.stats.linkCount)).toEqual([11, 12, 12]);
    expect(sorted[1]?.stats.internalMaxThroughput?.exact).toBe('480');
  });

  it('sorts layouts by linkCount when peak is absent', () => {
    const custom = (links: number, feedbacks: number) => ({
      stats: { linkCount: links, feedbackLoops: feedbacks },
    });
    const sorted = sortSolutions([custom(7, 0), custom(5, 2), custom(6, 1)], DEFAULT_SORT_COLUMNS);
    expect(sorted.map((item) => item.stats.linkCount)).toEqual([5, 6, 7]);
  });

  it('uses secondary keys only on ties', () => {
    expect(compareSolutions(sol(12, '400', 2), sol(12, '500', 0), DEFAULT_SORT_COLUMNS)).toBeLessThan(0);
  });

  it('flips direction for one column', () => {
    const flipped = flipColumnDir(DEFAULT_SORT_COLUMNS, 'belts');
    expect(flipped[0]).toEqual({ key: 'belts', dir: 'desc' });
    expect(flipped[1]?.dir).toBe('asc');
  });

  it('reorders columns for sort priority', () => {
    const reordered = reorderColumns(DEFAULT_SORT_COLUMNS, 2, 0);
    expect(reordered.map((column) => column.key)).toEqual(['feedbacks', 'belts', 'peak']);
  });
  it('orders close arbitrary rational peaks without floating-point rounding', () => {
    const larger = sol(1, '100000000000000000000000000000000000001/100000000000000000000000000000000000000', 0);
    const equal = sol(1, '1', 0);
    const smaller = sol(1, '99999999999999999999999999999999999999/100000000000000000000000000000000000000', 0);
    expect(sortSolutions([larger, equal, smaller], DEFAULT_SORT_COLUMNS)).toEqual([smaller, equal, larger]);
    expect(compareSolutions(sol(1, '-3/2', 0), sol(1, '-1.5', 0), DEFAULT_SORT_COLUMNS)).toBe(0);
    expect(compareSolutions(sol(1, '1/-2', 0), sol(1, '0', 0), DEFAULT_SORT_COLUMNS)).toBeLessThan(0);
  });
});
