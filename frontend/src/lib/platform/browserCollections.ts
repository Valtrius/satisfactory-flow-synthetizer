import type { CollectionRef, Solution, SolutionRow } from '../../types';
import type { CollectionStore, IndexedSolution } from './contracts';
import { compareSolutions } from '../solutionSort';
import { idbRequest as request, idbTransaction } from './idb';

export const COLLECTION_STORES = ['collections', 'collectionSolutions', 'collectionSummaries'];
export const COLLECTION_PAGE_SIZE = 64;
export const MAX_COLLECTION_PAGE = 128;
export type CollectionMetadata = { id: string; count: number };
type Pending = { base: CollectionRef; values: IndexedSolution[] };

export function checkCollectionRef(ref: CollectionRef): void {
  if (
    !ref ||
    ref.version !== 1 ||
    typeof ref.id !== 'string' ||
    !ref.id ||
    !Number.isSafeInteger(ref.count) ||
    ref.count < 0 ||
    !(
      ref.preferredIndex === null ||
      (Number.isSafeInteger(ref.preferredIndex) && ref.preferredIndex >= 0 && ref.preferredIndex < ref.count)
    ) ||
    (ref.legacyEntryId !== undefined && (typeof ref.legacyEntryId !== 'string' || !ref.legacyEntryId))
  ) {
    throw new Error('Invalid local solution collection reference.');
  }
}

function windowRange(ref: CollectionRef, offset: number, limit: number): number {
  checkCollectionRef(ref);
  if (
    !Number.isSafeInteger(offset) ||
    offset < 0 ||
    !Number.isSafeInteger(limit) ||
    limit < 1 ||
    limit > MAX_COLLECTION_PAGE
  ) {
    throw new Error(`Collection pages require a nonnegative offset and at most ${MAX_COLLECTION_PAGE} rows.`);
  }
  return Math.min(limit, Math.max(0, ref.count - offset));
}

function validateBatch(ref: CollectionRef, values: IndexedSolution[]): void {
  checkCollectionRef(ref);
  if (
    ref.legacyEntryId ||
    values.length > 16 ||
    values.some(
      (row, index) =>
        row.index !== ref.count + index ||
        !row.solution ||
        !Array.isArray(row.solution.nodes) ||
        !Array.isArray(row.solution.edges),
    )
  ) {
    throw new Error('Invalid or noncontiguous browser collection batch.');
  }
}

export function createBrowserCollections(database: () => Promise<IDBDatabase>): CollectionStore {
  // At most one failed bounded append per stopped job. No complete graph collection lives here.
  const pending = new Map<string, Pending>();
  let tail = Promise.resolve();
  const enqueue = <T>(work: () => Promise<T>): Promise<T> => {
    const result = tail.then(work);
    tail = result.then(
      () => {},
      () => {},
    );
    return result;
  };
  async function append(ref: CollectionRef, values: IndexedSolution[]): Promise<CollectionRef> {
    validateBatch(ref, values);
    await idbTransaction(await database(), COLLECTION_STORES, 'readwrite', async (tx) => {
      const metadata = tx.objectStore('collections');
      const old = (await request(metadata.get(ref.id))) as CollectionMetadata | undefined;
      // A committed append may have lost its acknowledgement. Verify that exact
      // immutable batch before accepting a retry, rather than overwriting it.
      if (old && values.length && old.count === ref.count + values.length) {
        for (const row of values) {
          const stored = await request(tx.objectStore('collectionSolutions').get([ref.id, row.index]));
          const summary = await request(tx.objectStore('collectionSummaries').get([ref.id, row.index]));
          if (
            JSON.stringify(stored?.value) !== JSON.stringify(row.solution) ||
            JSON.stringify(summary?.stats) !== JSON.stringify(row.solution.stats)
          ) {
            throw new Error('Collection retry does not match its stored immutable batch.');
          }
        }
        return;
      }
      if (!old || old.count !== ref.count)
        throw new Error('Collection append prefix changed. No solutions were overwritten.');
      for (const row of values) {
        await request(
          tx.objectStore('collectionSolutions').add({ collectionId: ref.id, index: row.index, value: row.solution }),
        );
        await request(
          tx
            .objectStore('collectionSummaries')
            .add({ collectionId: ref.id, index: row.index, stats: row.solution.stats }),
        );
      }
      await request(metadata.put({ id: ref.id, count: ref.count + values.length } satisfies CollectionMetadata));
    });
    return { ...ref, count: ref.count + values.length, pending: undefined };
  }
  async function flush(ref: CollectionRef): Promise<void> {
    const saved = pending.get(ref.id);
    if (!saved) return;
    await append(saved.base, saved.values);
    pending.delete(ref.id);
  }
  async function read(ref: CollectionRef, offset: number, limit: number): Promise<IndexedSolution[]> {
    const size = windowRange(ref, offset, limit);
    if (!size) return [];
    const saved = pending.get(ref.id);
    const storedEnd = Math.min(offset + size, saved?.base.count ?? ref.count);
    const rows = await idbTransaction(
      await database(),
      ref.legacyEntryId ? ['solutions'] : COLLECTION_STORES,
      'readonly',
      async (tx) => {
        if (offset >= storedEnd) return [] as IndexedSolution[];
        const id = ref.legacyEntryId ?? ref.id;
        const store = tx.objectStore(ref.legacyEntryId ? 'solutions' : 'collectionSolutions');
        const result = (await request(store.getAll(IDBKeyRange.bound([id, offset], [id, storedEnd - 1]), size))) as {
          index: number;
          value: Solution;
        }[];
        return result.map((row) => ({ index: row.index, solution: row.value }));
      },
    );
    for (const row of saved?.values ?? [])
      if (row.index >= offset && row.index < offset + size) rows.push(structuredClone(row));
    if (
      rows.length !== size ||
      rows.some((row, index) => row.index !== offset + index || !row.solution?.nodes || !row.solution?.edges)
    ) {
      throw new Error('Stored solution collection is incomplete or corrupt.');
    }
    return rows;
  }
  return {
    create: (id) =>
      enqueue(async () => {
        const ref: CollectionRef = { version: 1, id, count: 0, preferredIndex: null };
        checkCollectionRef(ref);
        await idbTransaction(await database(), ['collections'], 'readwrite', async (tx) => {
          await request(tx.objectStore('collections').add({ id, count: 0 }));
        });
        return ref;
      }),
    append: (ref, values) => {
      const savedRef = structuredClone(ref);
      const savedValues = structuredClone(values);
      return enqueue(() => append(savedRef, savedValues));
    },
    retain(ref, values) {
      validateBatch(ref, values);
      if (pending.has(ref.id)) throw new Error('A failed collection write is already retained.');
      pending.set(ref.id, { base: structuredClone(ref), values: structuredClone(values) });
      return { ...ref, count: ref.count + values.length, pending: true };
    },
    flush: (ref) => enqueue(() => flush(ref)),
    forget: (id) => {
      pending.delete(id);
    },
    discard: (ref) =>
      enqueue(async () => {
        checkCollectionRef(ref);
        if (ref.legacyEntryId) return;
        const removed = await idbTransaction(
          await database(),
          ['entries', ...COLLECTION_STORES],
          'readwrite',
          async (tx) => {
            // Check references in the same transaction as deletion, including
            // entries committed by another tab. Never collect unrelated work.
            const entries = await request(tx.objectStore('entries').getAll());
            if (entries.some((entry) => entry.collection?.id === ref.id)) return false;
            const range = IDBKeyRange.bound([ref.id], [ref.id, []]);
            await request(tx.objectStore('collections').delete(ref.id));
            await request(tx.objectStore('collectionSolutions').delete(range));
            await request(tx.objectStore('collectionSummaries').delete(range));
            return true;
          },
        );
        if (removed) pending.delete(ref.id);
      }),
    read,
    async get(ref, index) {
      const row = (await read(ref, index, 1))[0];
      if (!row) throw new Error('Solution index is outside this collection.');
      return row.solution;
    },
    async page(ref, offset, limit, columns) {
      windowRange(ref, offset, limit);
      const saved = pending.get(ref.id);
      const storedCount = Math.min(ref.count, saved?.base.count ?? ref.count);
      const rows = await idbTransaction(
        await database(),
        ref.legacyEntryId ? ['solutions'] : ['collectionSummaries'],
        'readonly',
        async (tx) => {
          if (storedCount === 0) return [] as SolutionRow[];
          const store = tx.objectStore(ref.legacyEntryId ? 'solutions' : 'collectionSummaries');
          const id = ref.legacyEntryId ?? ref.id;
          // A legacy cursor decodes one graph at a time. Only summaries enter the
          // sort index, and only a bounded row window enters the Svelte table.
          return new Promise<SolutionRow[]>((resolve, reject) => {
            const result: SolutionRow[] = [];
            const cursor = store.openCursor(IDBKeyRange.bound([id, 0], [id, storedCount - 1]));
            cursor.onerror = () => reject(cursor.error);
            cursor.onsuccess = () => {
              const row = cursor.result;
              if (!row) {
                resolve(result);
                return;
              }
              const value = row.value;
              if (value.index !== result.length || !(ref.legacyEntryId ? value.value?.stats : value.stats)) {
                reject(new Error('Corrupt collection summary index.'));
                return;
              }
              result.push({
                sourceIndex: value.index,
                solution: { stats: ref.legacyEntryId ? value.value.stats : value.stats },
              });
              row.continue();
            };
          });
        },
      );
      for (const row of saved?.values ?? [])
        if (row.index < ref.count)
          rows.push({ sourceIndex: row.index, solution: { stats: structuredClone(row.solution.stats) } });
      if (rows.length !== ref.count) throw new Error('Stored collection summary count is inconsistent.');
      rows.sort(
        (left, right) =>
          compareSolutions(left.solution, right.solution, columns) || left.sourceIndex - right.sourceIndex,
      );
      return { offset, total: ref.count, rows: rows.slice(offset, offset + limit) };
    },
  };
}
