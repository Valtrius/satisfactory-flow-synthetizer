import type { CollectionRef, SolutionPage, SolutionRow } from '../../types';
import { compareSolutions, type SortColumn } from '../solutionSort';
import type { IndexedSolution } from './contracts';
import { idbRequest as request, idbTransaction } from './idb';

export type PendingCollection = { base: CollectionRef; values: IndexedSolution[] };
type SortIndex = { columns: string; pending?: PendingCollection; rows: SolutionRow[] };
type SummaryIndex = { id: string; legacy?: string; rows: SolutionRow[]; sorted?: SortIndex };

/** Cache only the most recently viewed collection. Graphs never enter this index. */
export function createCollectionPages(database: () => Promise<IDBDatabase>, pending: Map<string, PendingCollection>) {
  let index: SummaryIndex | undefined;
  let tail = Promise.resolve();

  async function page(ref: CollectionRef, offset: number, limit: number, columns: SortColumn[]): Promise<SolutionPage> {
    if (index?.id !== ref.id || index.legacy !== ref.legacyEntryId)
      index = { id: ref.id, legacy: ref.legacyEntryId, rows: [] };
    const current = index;
    const saved = pending.get(ref.id);
    const storedCount = Math.min(ref.count, saved?.base.count ?? ref.count);
    const start = current.rows.length;
    const added = await idbTransaction(
      await database(),
      ref.legacyEntryId ? ['entries', 'solutions'] : ['collections', 'collectionSummaries'],
      'readonly',
      async (tx) => {
        // Check existence even on a cache hit: another tab may have deleted the collection.
        const metadata = await request(
          tx.objectStore(ref.legacyEntryId ? 'entries' : 'collections').get(ref.legacyEntryId ?? ref.id),
        );
        if (!metadata || (!ref.legacyEntryId && metadata.count < storedCount))
          throw new Error('Stored collection summary count is inconsistent.');
        if (start >= storedCount) return [] as SolutionRow[];
        const store = tx.objectStore(ref.legacyEntryId ? 'solutions' : 'collectionSummaries');
        const id = ref.legacyEntryId ?? ref.id;
        return new Promise<SolutionRow[]>((resolve, reject) => {
          const rows: SolutionRow[] = [];
          const cursor = store.openCursor(IDBKeyRange.bound([id, start], [id, storedCount - 1]));
          cursor.onerror = () => reject(cursor.error);
          cursor.onsuccess = () => {
            const value = cursor.result?.value;
            if (!cursor.result) {
              if (start + rows.length !== storedCount) reject(new Error('Corrupt collection summary index.'));
              else resolve(rows);
              return;
            }
            const stats = ref.legacyEntryId ? value.value?.stats : value.stats;
            if (value.index !== start + rows.length || !stats) {
              reject(new Error('Corrupt collection summary index.'));
              return;
            }
            rows.push({ sourceIndex: value.index, solution: { stats } });
            cursor.result.continue();
          };
        });
      },
    );
    for (const row of added) current.rows.push(row);
    const signature = JSON.stringify(columns);
    const prior = current.sorted;
    if (prior?.columns !== signature || prior.pending !== saved || prior.rows.length !== ref.count) {
      const compare = (left: SolutionRow, right: SolutionRow) =>
        compareSolutions(left.solution, right.solution, columns) || left.sourceIndex - right.sourceIndex;
      const reusable = prior?.columns === signature && prior.pending === saved && prior.rows.length <= ref.count;
      const from = reusable ? prior.rows.length : 0;
      const suffix = current.rows.slice(from, storedCount);
      for (const row of saved?.values ?? [])
        if (row.index >= from && row.index < ref.count)
          suffix.push({ sourceIndex: row.index, solution: { stats: row.solution.stats } });
      if (from + suffix.length !== ref.count) throw new Error('Stored collection summary count is inconsistent.');
      suffix.sort(compare);
      // Merge only the newly appended suffix into an already sorted immutable prefix.
      const rows: SolutionRow[] = [];
      const prefix = reusable ? prior.rows : [];
      let left = 0,
        right = 0;
      while (left < prefix.length && right < suffix.length)
        rows.push(compare(prefix[left], suffix[right]) <= 0 ? prefix[left++] : suffix[right++]);
      while (left < prefix.length) rows.push(prefix[left++]);
      while (right < suffix.length) rows.push(suffix[right++]);
      current.sorted = { columns: signature, pending: saved, rows };
    }
    return { offset, total: ref.count, rows: structuredClone(current.sorted!.rows.slice(offset, offset + limit)) };
  }

  return {
    page(ref: CollectionRef, offset: number, limit: number, columns: SortColumn[]) {
      const savedRef = structuredClone(ref),
        savedColumns = structuredClone(columns);
      const result = tail.then(() => page(savedRef, offset, limit, savedColumns));
      tail = result.then(
        () => {},
        () => {},
      );
      return result;
    },
    forget(id: string) {
      if (index?.id === id) index = undefined;
    },
  };
}
