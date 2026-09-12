import { untrack } from 'svelte';
import type { CollectionRef, SolutionRow } from '../types';
import type { HistoryEntry } from './historyModel';
import type { CollectionStore } from './platform/contracts';
import { COLLECTION_PAGE_SIZE } from './platform/browserCollections';
import { createPagedResults } from './pagedResults';
import type { SortColumn } from './solutionSort';

/** Grow a sorted prefix of summaries as the user scrolls; load graphs only on selection. */
export function createResultPages(
  store: CollectionStore | undefined,
  selectedEntry: () => HistoryEntry | null,
  sortColumns: () => SortColumn[],
  onError: (message: string) => void,
) {
  let loading = $state(false);
  let failed = $state(false);
  let rows = $state<SolutionRow[]>([]);
  let viewKey = '';
  let active: { ref: CollectionRef; columns: SortColumn[]; limit: number; rows: SolutionRow[] } | null = null;

  function readNext(): void {
    if (!active || !pages) return;
    loading = true;
    failed = false;
    pages.load(active.ref, active.rows.length, COLLECTION_PAGE_SIZE, active.columns);
  }

  const pages = store
    ? createPagedResults(
        store,
        (page) => {
          if (!active) return;
          if (page.offset !== active.rows.length || (!page.rows.length && page.offset < active.ref.count)) {
            failed = true;
            loading = false;
            onError('Could not load more layouts: the result list is incomplete.');
            return;
          }
          active.rows.push(...page.rows);
          if (active.rows.length < active.limit) {
            readNext();
          } else {
            rows = active.rows;
            loading = false;
          }
        },
        (message) => {
          onError(message);
          loading = false;
          failed = true;
        },
      )
    : null;
  // Graph selection and telemetry do not change the summary list.
  const selectionKey = $derived(
    JSON.stringify([
      selectedEntry()?.id,
      selectedEntry()?.collection?.id,
      selectedEntry()?.collection?.legacyEntryId,
      sortColumns(),
    ]),
  );
  const requestKey = $derived(JSON.stringify([selectionKey, selectedEntry()?.collection?.count]));
  $effect(() => {
    void requestKey;
    const entry = untrack(selectedEntry);
    const columns = untrack(sortColumns);
    pages?.clear();
    active = null;
    failed = false;
    if (selectionKey !== viewKey) {
      viewKey = selectionKey;
      rows = [];
    }
    if (!entry?.collection?.count || !pages) {
      rows = [];
      loading = false;
      return;
    }
    // New results can sort ahead of existing rows. Refresh the visible prefix atomically
    // so an append never mixes offsets from different collection revisions.
    active = {
      ref: { ...entry.collection },
      columns: columns.map((column) => ({ ...column })),
      limit: Math.min(
        entry.collection.count,
        Math.max(
          COLLECTION_PAGE_SIZE,
          untrack(() => rows.length),
        ),
      ),
      rows: [],
    };
    loading = true;
    const timer = setTimeout(readNext, 20);
    return () => clearTimeout(timer);
  });
  $effect(() => () => pages?.clear());
  return {
    loadMore() {
      if (loading || failed || !active || rows.length >= active.ref.count) return;
      active = { ...active, rows: [...rows], limit: Math.min(active.ref.count, rows.length + COLLECTION_PAGE_SIZE) };
      readNext();
    },
    retry: readNext,
    get loading() {
      return loading;
    },
    get rows() {
      return rows;
    },
    get failed() {
      return failed;
    },
  };
}
