import { onDestroy, untrack } from 'svelte';
import type { SolutionRow } from '../types';
import type { HistoryEntry } from './historyModel';
import type { CollectionStore } from './platform/contracts';
import { COLLECTION_PAGE_SIZE } from './platform/browserCollections';
import { createPagedResults } from './pagedResults';
import type { SortColumn } from './solutionSort';

/** Own the selected collection's page window and pending reads. */
export function createResultPages(
  store: CollectionStore | undefined,
  selectedEntry: () => HistoryEntry | null,
  sortColumns: () => SortColumn[],
  onError: (message: string) => void,
) {
  let offset = $state(0);
  let entryId = '';
  let loading = $state(false);
  let rows = $state<SolutionRow[]>([]);
  const pages = store
    ? createPagedResults(
        store,
        (page) => {
          rows = page.rows;
          loading = false;
        },
        (message) => {
          onError(message);
          loading = false;
        },
      )
    : null;
  // Telemetry and graph edits do not change a page's data.
  const requestKey = $derived(JSON.stringify([selectedEntry()?.id, selectedEntry()?.collection, sortColumns()]));
  $effect(() => {
    void requestKey;
    const entry = untrack(selectedEntry);
    const columns = untrack(sortColumns);
    pages?.clear();
    if (entry?.id !== entryId) {
      entryId = entry?.id ?? '';
      offset = 0;
      rows = [];
    }
    if (!entry?.collection || !pages) {
      loading = false;
      return;
    }
    const next = Math.min(
      offset,
      Math.max(0, Math.floor((entry.collection.count - 1) / COLLECTION_PAGE_SIZE) * COLLECTION_PAGE_SIZE),
    );
    if (next !== offset) offset = next;
    loading = true;
    const timer = setTimeout(() => pages.load(entry.collection!, next, COLLECTION_PAGE_SIZE, columns), 20);
    return () => clearTimeout(timer);
  });
  onDestroy(() => pages?.clear());
  return {
    get offset() {
      return offset;
    },
    set offset(value: number) {
      offset = value;
    },
    get loading() {
      return loading;
    },
    get rows() {
      return rows;
    },
  };
}
