import { flushSync, mount, unmount } from 'svelte';
import { fromStore, writable } from 'svelte/store';
import { expect, it, vi } from 'vitest';
import type { CollectionRef, SolutionPage, SolutionRow } from '../types';
import type { CollectionStore } from './platform/contracts';
import type { createResultPages } from './resultPages.svelte';
import { DEFAULT_SORT_COLUMNS } from './solutionSort';
import { historyEntry, solution } from '../test/fixtures';
import ResultPagesHarness from '../test/ResultPagesHarness.svelte';

function harness() {
  const collection: CollectionRef = { version: 1, id: 'first', count: 130, preferredIndex: 0 };
  const entry = writable(historyEntry({ collection }));
  const columns = writable([...DEFAULT_SORT_COLUMNS]);
  const selected = fromStore(entry);
  const sort = fromStore(columns);
  const requests: {
    ref: CollectionRef;
    offset: number;
    resolve: (value: SolutionPage) => void;
    reject: (error: Error) => void;
  }[] = [];
  const page = vi.fn(
    (ref: CollectionRef, offset: number) =>
      new Promise<SolutionPage>((resolve, reject) => {
        requests.push({ ref, offset, resolve, reject });
      }),
  );
  const onError = vi.fn();
  let results!: ReturnType<typeof createResultPages>;
  const component = mount(ResultPagesHarness, {
    target: document.body,
    props: {
      args: [{ page } as unknown as CollectionStore, () => selected.current, () => sort.current, onError],
      onReady: (value) => {
        results = value;
      },
    },
  });
  flushSync();
  const resolve = (requestIndex: number, sourceIndices?: number[]) => {
    const request = requests[requestIndex];
    const indices =
      sourceIndices ??
      Array.from({ length: Math.min(64, request.ref.count - request.offset) }, (_, i) => request.offset + i);
    request.resolve({
      offset: request.offset,
      total: request.ref.count,
      rows: indices.map((sourceIndex) => ({ sourceIndex, solution: { stats: solution.stats } }) satisfies SolutionRow),
    });
  };
  return { results, entry, columns, collection, requests, page, onError, resolve, dispose: () => unmount(component) };
}

it('appends bounded batches once and keeps selection changes from reloading the list', async () => {
  const h = harness();
  try {
    await vi.waitFor(() => expect(h.requests).toHaveLength(1));
    h.resolve(0);
    await vi.waitFor(() => expect(h.results.rows).toHaveLength(64));
    h.results.loadMore();
    h.results.loadMore();
    expect(h.requests).toHaveLength(2);
    expect(h.results.rows).toHaveLength(64);
    h.resolve(1);
    await vi.waitFor(() => expect(h.results.rows).toHaveLength(128));
    h.results.loadMore();
    h.resolve(2);
    await vi.waitFor(() => expect(h.results.rows).toHaveLength(130));
    expect(h.results.rows.map((row) => row.sourceIndex)).toEqual(Array.from({ length: 130 }, (_, i) => i));
    h.results.loadMore();
    h.entry.update((entry) => ({ ...entry, collection: { ...entry.collection!, preferredIndex: 100 } }));
    flushSync();
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(h.requests).toHaveLength(3);
    expect(h.page.mock.calls.every((call) => (call as unknown[])[2] === 64)).toBe(true);
  } finally {
    await h.dispose();
  }
});

it('ignores reads from old history or sort choices and retries a failed append without losing rows', async () => {
  const h = harness();
  try {
    await vi.waitFor(() => expect(h.requests).toHaveLength(1));
    h.entry.set(historyEntry({ id: 'second', collection: { ...h.collection, id: 'second' } }));
    flushSync();
    await new Promise((resolve) => setTimeout(resolve, 30));
    h.resolve(0);
    await vi.waitFor(() => expect(h.requests).toHaveLength(2));
    expect(h.results.rows).toEqual([]);
    expect(h.requests[1].ref.id).toBe('second');
    h.resolve(1);
    await vi.waitFor(() => expect(h.results.rows).toHaveLength(64));
    h.results.loadMore();
    h.requests[2].reject(new Error('Temporary read failure'));
    await vi.waitFor(() => expect(h.results.failed).toBe(true));
    expect(h.results.rows).toHaveLength(64);
    h.results.loadMore();
    expect(h.requests).toHaveLength(3);
    h.results.retry();
    expect(h.requests[3].offset).toBe(64);
    h.resolve(3);
    await vi.waitFor(() => expect(h.results.rows).toHaveLength(128));
    h.results.loadMore();
    h.columns.set(DEFAULT_SORT_COLUMNS.map((column) => ({ ...column, dir: 'desc' })));
    flushSync();
    await new Promise((resolve) => setTimeout(resolve, 30));
    h.resolve(4);
    await vi.waitFor(() => expect(h.requests).toHaveLength(6));
    expect(h.results.rows).toEqual([]);
    expect(h.requests[5].offset).toBe(0);
    const reversed = Array.from({ length: 64 }, (_, i) => 129 - i);
    h.resolve(5, reversed);
    await vi.waitFor(() => expect(h.results.rows.map((row) => row.sourceIndex)).toEqual(reversed));
    expect(h.onError).toHaveBeenCalledOnce();
  } finally {
    await h.dispose();
  }
});

it('refreshes the loaded prefix atomically when new layouts change the sorted order', async () => {
  const h = harness();
  try {
    await vi.waitFor(() => expect(h.requests).toHaveLength(1));
    h.resolve(0);
    await vi.waitFor(() => expect(h.results.rows).toHaveLength(64));
    h.results.loadMore();
    h.resolve(1);
    await vi.waitFor(() => expect(h.results.rows).toHaveLength(128));
    h.entry.update((entry) => ({ ...entry, collection: { ...entry.collection!, count: 131 } }));
    flushSync();
    await vi.waitFor(() => expect(h.requests).toHaveLength(3));
    const prefix = [130, ...Array.from({ length: 127 }, (_, i) => i)];
    h.resolve(2, prefix.slice(0, 64));
    await vi.waitFor(() => expect(h.requests).toHaveLength(4));
    expect(h.results.rows[0].sourceIndex).toBe(0);
    h.resolve(3, prefix.slice(64));
    await vi.waitFor(() => expect(h.results.rows.map((row) => row.sourceIndex)).toEqual(prefix));
    expect(h.results.loading).toBe(false);
  } finally {
    await h.dispose();
  }
});
