import type { CollectionRef, SolutionPage } from '../types';
import type { CollectionStore } from './platform/contracts';
import type { SortColumn } from './solutionSort';

/** One in-flight page read and one coalesced replacement, never a graph collection. */
export function createPagedResults(
  store: CollectionStore,
  onPage: (page: SolutionPage) => void,
  onError: (message: string) => void,
) {
  let revision = 0;
  let running = false;
  let pending: { revision: number; ref: CollectionRef; offset: number; size: number; columns: SortColumn[] } | null =
    null;
  async function drain(): Promise<void> {
    if (running) return;
    running = true;
    try {
      while (pending) {
        const request = pending;
        pending = null;
        try {
          const page = await store.page(request.ref, request.offset, request.size, request.columns);
          if (revision === request.revision) onPage(page);
        } catch (error) {
          if (revision === request.revision) onError(`Could not load solution page: ${String(error)}`);
        }
      }
    } finally {
      running = false;
    }
  }
  return {
    load(ref: CollectionRef, offset: number, size: number, columns: SortColumn[]) {
      pending = {
        revision: ++revision,
        ref: { ...ref },
        offset,
        size,
        columns: columns.map((column) => ({ ...column })),
      };
      void drain();
    },
    clear() {
      revision++;
      pending = null;
    },
  };
}
