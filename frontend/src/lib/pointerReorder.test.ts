import { describe, expect, it } from 'vitest';
import {
  insertIndexFromClient,
  toIndexFromInsertAt,
  visualReorderSlots
} from './pointerReorder';

function fakeEl(start: number, size: number, axis: 'x' | 'y') {
  return {
    getBoundingClientRect: () =>
      axis === 'x'
        ? ({ left: start, width: size, right: start + size, top: 0, height: 10, bottom: 10 } as DOMRect)
        : ({ top: start, height: size, bottom: start + size, left: 0, width: 10, right: 10 } as DOMRect)
  };
}

describe('insertIndexFromClient', () => {
  it('returns a without-list index from horizontal midpoints', () => {
    const els = [fakeEl(0, 100, 'x'), fakeEl(100, 100, 'x'), fakeEl(200, 100, 'x')];
    expect(insertIndexFromClient(els, 40, 'x')).toBe(0);
    expect(insertIndexFromClient(els, 120, 'x')).toBe(1);
    expect(insertIndexFromClient(els, 240, 'x')).toBe(2);
    expect(insertIndexFromClient(els, 400, 'x')).toBe(3);
  });
});

describe('visualReorderSlots', () => {
  it('keeps the original slot reachable after moving away', () => {
    const items = ['a', 'b', 'c'];
    expect(visualReorderSlots(items, 1, 1, true).map((slot) => slot.kind)).toEqual([
      'item',
      'ghost',
      'item'
    ]);
    expect(visualReorderSlots(items, 1, 3, true).map((slot) => slot.kind)).toEqual([
      'item',
      'item',
      'ghost'
    ]);
    expect(visualReorderSlots(items, 1, 1, true).map((slot) =>
      slot.kind === 'item' ? slot.item : 'ghost'
    )).toEqual(['a', 'ghost', 'c']);
  });
});

describe('toIndexFromInsertAt', () => {
  it('matches splice reorder semantics', () => {
    expect(toIndexFromInsertAt(0, 2)).toBe(2);
    expect(toIndexFromInsertAt(2, 0)).toBe(0);
    expect(toIndexFromInsertAt(1, 1)).toBe(1);
  });
});
