import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createHistoryEntrance, createHistoryOrder, historyMotionDuration } from './historyMotion';

describe('history animation order', () => {
  it('preserves the list reference across startup and repeated content updates', () => {
    const stabilizeOrder = createHistoryOrder();
    stabilizeOrder(['earlier']);
    const inserted = stabilizeOrder(['new', 'earlier']);
    for (let snapshot = 0; snapshot < 30; snapshot++) {
      expect(stabilizeOrder(['new', 'earlier'])).toBe(inserted);
    }
  });

  it('updates the list for inserts, filters, sorts, and drag ghost moves', () => {
    const stabilizeOrder = createHistoryOrder();
    const orders = [
      ['a', 'b'],
      ['new', 'a', 'b'],
      ['a', 'b'],
      ['b', 'a'],
      ['ghost-history', 'a'],
      ['a', 'ghost-history'],
      ['a', 'b'],
      []
    ];
    let previous = stabilizeOrder([]);
    for (const keys of orders) {
      const next = stabilizeOrder(keys);
      expect(next).toEqual(keys);
      expect(next).not.toBe(previous);
      previous = next;
    }
  });
});

describe('history row motion', () => {
  let reduced = false;
  let preference: EventTarget & { readonly matches: boolean };
  let node: HTMLDivElement;
  let cancel: ReturnType<typeof vi.fn>;
  let animate: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    reduced = false;
    preference = new EventTarget() as typeof preference;
    Object.defineProperty(preference, 'matches', { get: () => reduced });
    vi.stubGlobal('matchMedia', () => preference);
    node = document.createElement('div');
    cancel = vi.fn();
    animate = vi.fn(() => ({ cancel }));
    Object.defineProperty(node, 'animate', { value: animate });
  });

  afterEach(() => vi.unstubAllGlobals());

  it('animates a new queued row only once, including after filtering or dragging', () => {
    const enter = createHistoryEntrance(['restored']);
    enter(node, { id: 'restored', band: 'queued' });
    expect(animate).not.toHaveBeenCalled();

    const mounted = enter(node, { id: 'new', band: 'queued' });
    expect(animate).toHaveBeenCalledTimes(1);
    mounted?.destroy?.();
    expect(cancel).toHaveBeenCalledTimes(1);

    enter(node, { id: 'new', band: 'queued' });
    expect(animate).toHaveBeenCalledTimes(1);
  });

  it('leaves the same entrance running across queued, running, and history updates', () => {
    const enter = createHistoryEntrance([]);
    const mounted = enter(node, { id: 'new', band: 'queued' });
    mounted?.update?.({ id: 'new', band: 'running' });
    mounted?.update?.({ id: 'new', band: 'history' });
    expect(animate).toHaveBeenCalledTimes(1);
    expect(cancel).not.toHaveBeenCalled();
    mounted?.destroy?.();
  });

  it('does not animate restored history or already running entries', () => {
    const enter = createHistoryEntrance([]);
    enter(node, { id: 'completed', band: 'history' });
    enter(node, { id: 'running', band: 'running' });
    expect(animate).not.toHaveBeenCalled();
  });

  it('keeps rapid entrances independent when one row is removed', () => {
    const enter = createHistoryEntrance([]);
    const first = enter(node, { id: 'first', band: 'queued' });
    const secondNode = document.createElement('div');
    const secondCancel = vi.fn();
    const secondAnimate = vi.fn(() => ({ cancel: secondCancel }));
    Object.defineProperty(secondNode, 'animate', { value: secondAnimate });
    const second = enter(secondNode, { id: 'second', band: 'queued' });

    expect(animate).toHaveBeenCalledTimes(1);
    expect(secondAnimate).toHaveBeenCalledTimes(1);
    first?.destroy?.();
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(secondCancel).not.toHaveBeenCalled();
    second?.destroy?.();
  });

  it('reads reduced motion for each entrance and each reorder', () => {
    const enter = createHistoryEntrance([]);
    expect(historyMotionDuration()).toBe(300);
    reduced = true;
    expect(historyMotionDuration()).toBe(0);
    enter(node, { id: 'quiet', band: 'queued' });
    expect(animate).not.toHaveBeenCalled();
    reduced = false;
    enter(node, { id: 'quiet', band: 'queued' });
    expect(animate).not.toHaveBeenCalled();
    expect(historyMotionDuration()).toBe(300);
  });

  it('cancels an entrance when reduced motion is enabled and cleans up on removal', () => {
    const enter = createHistoryEntrance([]);
    const remove = vi.spyOn(preference, 'removeEventListener');
    const mounted = enter(node, { id: 'new', band: 'queued' });
    reduced = true;
    preference.dispatchEvent(new Event('change'));
    expect(cancel).toHaveBeenCalledTimes(1);
    mounted?.destroy?.();
    expect(remove).toHaveBeenCalledWith('change', expect.any(Function));
    preference.dispatchEvent(new Event('change'));
    expect(cancel).toHaveBeenCalledTimes(2);
  });

  it('stops listening for preference changes after the entrance finishes', () => {
    const enter = createHistoryEntrance([]);
    const mounted = enter(node, { id: 'new', band: 'queued' });
    animate.mock.results[0].value.onfinish();
    reduced = true;
    preference.dispatchEvent(new Event('change'));
    expect(cancel).not.toHaveBeenCalled();
    mounted?.destroy?.();
  });
});
