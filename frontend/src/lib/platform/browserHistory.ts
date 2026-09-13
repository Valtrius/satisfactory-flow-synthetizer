import {
  HISTORY_DOCUMENT_VERSION,
  type CachedGraphLayout,
  type HistoryDocument,
  type HistoryEntry,
} from '../historyModel';
import type { HistoryOp } from '../historyOps';
import type { CollectionRef, Solution } from '../../types';
import type { CollectionStore, HistoryStore } from './contracts';
import {
  COLLECTION_STORES,
  COLLECTION_PAGE_SIZE,
  checkCollectionRef,
  createBrowserCollections,
  type CollectionMetadata,
} from './browserCollections';
import { idbRequest as request, idbTransaction } from './idb';

export const HISTORY_DATABASE = 'satisfactory-flow-synthetizer.history';
const SCHEMA_VERSION = 2;
const LEGACY_STORES = ['meta', 'entries', 'solutions', 'layouts'];
const STORES = [...LEGACY_STORES, ...COLLECTION_STORES];

type Metadata = { schema: number; revision: number; order: string[]; selectedEntryId: string | null };
type EntryRecord = Omit<HistoryEntry, 'results' | 'layouts'>;
type SolutionRecord = { entryId: string; index: number; value: Solution };
type LayoutRecord = { entryId: string; key: string; value: CachedGraphLayout };

/** The body may await IndexedDB requests only, never a timer, network call or UI event. */
function transaction<T>(
  database: IDBDatabase,
  mode: IDBTransactionMode,
  body: (tx: IDBTransaction) => Promise<T>,
): Promise<T> {
  return idbTransaction(database, STORES, mode, body);
}

function openDatabase(name: string): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === 'undefined') {
      reject(new Error('This browser does not provide IndexedDB history storage.'));
      return;
    }
    const operation = indexedDB.open(name, SCHEMA_VERSION);
    let abandoned = false;
    operation.onblocked = () => {
      abandoned = true;
      reject(new Error('History storage is blocked by another tab. Close that tab, then reload.'));
    };
    operation.onupgradeneeded = (event) => {
      if (abandoned || ![0, 1].includes(event.oldVersion)) {
        operation.transaction?.abort();
        return;
      }
      const db = operation.result;
      if (event.oldVersion === 0) {
        const meta = db.createObjectStore('meta');
        db.createObjectStore('entries', { keyPath: 'id' });
        db.createObjectStore('solutions', { keyPath: ['entryId', 'index'] });
        db.createObjectStore('layouts', { keyPath: ['entryId', 'key'] });
        meta.put(
          { schema: SCHEMA_VERSION, revision: 0, order: [], selectedEntryId: null } satisfies Metadata,
          'document',
        );
      } else {
        if (
          db.objectStoreNames.length !== LEGACY_STORES.length ||
          LEGACY_STORES.some((name) => !db.objectStoreNames.contains(name))
        ) {
          operation.transaction?.abort();
          return;
        }
        const meta = operation.transaction!.objectStore('meta');
        const read = meta.get('document');
        read.onsuccess = () => {
          const value = read.result as Metadata | undefined;
          if (!value || value.schema !== 1 || !Number.isSafeInteger(value.revision) || !Array.isArray(value.order)) {
            operation.transaction?.abort();
            return;
          }
          meta.put({ ...value, schema: SCHEMA_VERSION }, 'document');
        };
      }
      db.createObjectStore('collections', { keyPath: 'id' });
      db.createObjectStore('collectionSolutions', { keyPath: ['collectionId', 'index'] });
      db.createObjectStore('collectionSummaries', { keyPath: ['collectionId', 'index'] });
    };
    operation.onerror = () => reject(operation.error ?? new Error('Could not open browser history.'));
    operation.onsuccess = () => {
      const db = operation.result;
      if (abandoned) {
        db.close();
        return;
      }
      if (db.objectStoreNames.length !== STORES.length || STORES.some((name) => !db.objectStoreNames.contains(name))) {
        db.close();
        reject(new Error('Unsupported browser history schema. Stored data was preserved.'));
        return;
      }
      resolve(db);
    };
  });
}

async function metadata(tx: IDBTransaction): Promise<Metadata> {
  const value = (await request(tx.objectStore('meta').get('document'))) as Metadata | undefined;
  if (
    !value ||
    value.schema !== SCHEMA_VERSION ||
    !Number.isSafeInteger(value.revision) ||
    value.revision < 0 ||
    !Array.isArray(value.order) ||
    value.order.some((id) => typeof id !== 'string') ||
    new Set(value.order).size !== value.order.length ||
    !(value.selectedEntryId === null || typeof value.selectedEntryId === 'string')
  ) {
    throw new Error('Invalid browser history metadata. Stored data was preserved.');
  }
  return value;
}

function children(id: string): IDBKeyRange {
  // Array keys sort after numbers and strings, so this covers this entry only.
  return IDBKeyRange.bound([id], [id, []]);
}

async function requireEntry(tx: IDBTransaction, id: string): Promise<EntryRecord> {
  const entry = (await request(tx.objectStore('entries').get(id))) as EntryRecord | undefined;
  if (!entry) throw new Error(`history entry not found: ${id}`);
  return entry;
}

async function replaceLayouts(
  tx: IDBTransaction,
  id: string,
  layouts: Record<string, CachedGraphLayout>,
): Promise<void> {
  const store = tx.objectStore('layouts');
  await request(store.delete(children(id)));
  for (const [key, value] of Object.entries(layouts)) {
    await request(store.add({ entryId: id, key, value } satisfies LayoutRecord));
  }
}

async function checkStoredCollection(tx: IDBTransaction, ref: CollectionRef, entryId: string): Promise<number> {
  checkCollectionRef(ref);
  if (ref.legacyEntryId) {
    if (ref.legacyEntryId !== entryId) throw new Error('A legacy collection cannot belong to another history entry.');
    const count = await request(tx.objectStore('solutions').count(children(entryId)));
    if (count < ref.count) throw new Error('Stored legacy collection is incomplete.');
    return count;
  }
  const meta = (await request(tx.objectStore('collections').get(ref.id))) as CollectionMetadata | undefined;
  if (!meta || !Number.isSafeInteger(meta.count) || meta.count < ref.count)
    throw new Error('Stored solution collection is missing or incomplete.');
  return meta.count;
}

async function applyOperation(
  tx: IDBTransaction,
  meta: Metadata,
  op: HistoryOp,
  releasedCollections: string[],
): Promise<void> {
  const entries = tx.objectStore('entries');
  const solutions = tx.objectStore('solutions');
  switch (op.op) {
    case 'upsertEntry': {
      const { results, layouts, ...entry } = op.entry;
      if (
        typeof entry.id !== 'string' ||
        !entry.id ||
        !Array.isArray(results) ||
        !layouts ||
        !entry.request ||
        !entry.form
      ) {
        throw new Error('Invalid history entry.');
      }
      if (!meta.order.includes(entry.id)) meta.order.push(entry.id);
      if (entry.collection) await checkStoredCollection(tx, entry.collection, entry.id);
      await request(entries.put(entry));
      if (!entry.collection?.legacyEntryId) await request(solutions.delete(children(entry.id)));
      for (const [index, value] of results.entries()) {
        await request(solutions.add({ entryId: entry.id, index, value } satisfies SolutionRecord));
      }
      await replaceLayouts(tx, entry.id, layouts);
      return;
    }
    case 'appendSolutions': {
      await requireEntry(tx, op.id);
      const keys = await request(solutions.getAllKeys(children(op.id)));
      if (
        !Number.isSafeInteger(op.expectedCount) ||
        op.expectedCount < 0 ||
        keys.length !== op.expectedCount ||
        keys.some((key, index) => !Array.isArray(key) || key[1] !== index)
      ) {
        throw new Error(`history_append_prefix_mismatch: ${op.id}`);
      }
      for (const [offset, value] of op.solutions.entries()) {
        const index = op.expectedCount + offset;
        if (!Number.isSafeInteger(index)) throw new Error('History result index overflow.');
        await request(solutions.add({ entryId: op.id, index, value } satisfies SolutionRecord));
      }
      return;
    }
    case 'patchEntry': {
      const entry = await requireEntry(tx, op.id);
      await request(entries.put({ ...entry, ...op.fields, id: op.id }));
      return;
    }
    case 'saveLayouts':
      await requireEntry(tx, op.id);
      await replaceLayouts(tx, op.id, op.layouts);
      return;
    case 'saveSortColumns': {
      const entry = await requireEntry(tx, op.id);
      await request(entries.put({ ...entry, sortColumns: op.sortColumns }));
      return;
    }
    case 'saveSolverState': {
      const entry = await requireEntry(tx, op.id);
      await request(entries.put({ ...entry, progress: op.progress, proof: op.proof, sequence: op.sequence }));
      return;
    }
    case 'deleteEntries':
      for (const id of op.ids) {
        const prior = (await request(entries.get(id))) as EntryRecord | undefined;
        await request(entries.delete(id));
        await request(solutions.delete(children(id)));
        await request(tx.objectStore('layouts').delete(children(id)));
        if (prior?.collection && !prior.collection.legacyEntryId) {
          const remaining = (await request(entries.getAll())) as EntryRecord[];
          if (!remaining.some((entry) => entry.collection?.id === prior.collection!.id)) {
            await request(tx.objectStore('collections').delete(prior.collection.id));
            await request(tx.objectStore('collectionSolutions').delete(children(prior.collection.id)));
            await request(tx.objectStore('collectionSummaries').delete(children(prior.collection.id)));
            releasedCollections.push(prior.collection.id);
          }
        }
      }
      meta.order = meta.order.filter((id) => !op.ids.includes(id));
      return;
    case 'reorderEntries':
      if (
        new Set(op.ids).size !== op.ids.length ||
        op.ids.length !== meta.order.length ||
        op.ids.some((id) => !meta.order.includes(id))
      ) {
        throw new Error('History order must contain every stored entry exactly once.');
      }
      meta.order = [...op.ids];
      return;
    case 'setSelected':
      meta.selectedEntryId = op.id;
      return;
    default:
      throw new Error('Unsupported history operation.');
  }
}

async function loadDocument(tx: IDBTransaction, meta: Metadata): Promise<HistoryDocument> {
  const records = (await request(tx.objectStore('entries').getAll())) as EntryRecord[];
  const layouts = (await request(tx.objectStore('layouts').getAll())) as LayoutRecord[];
  if (
    records.some(
      (entry) =>
        !entry ||
        typeof entry.id !== 'string' ||
        !entry.id ||
        typeof entry.status !== 'string' ||
        !entry.request ||
        !Array.isArray(entry.request.inputs) ||
        !Array.isArray(entry.request.outputs) ||
        typeof entry.request.beltRate !== 'string' ||
        !entry.form ||
        !Array.isArray(entry.form.inputs) ||
        !Array.isArray(entry.form.outputs) ||
        !Array.isArray(entry.sortColumns),
    )
  ) {
    throw new Error('Invalid browser history entry records. Stored data was preserved.');
  }
  const byId = new Map(
    records.map((entry) => [
      entry.id,
      { ...entry, results: [] as Solution[], layouts: {} as Record<string, CachedGraphLayout> },
    ]),
  );
  if (byId.size !== meta.order.length || meta.order.some((id) => !byId.has(id)))
    throw new Error('Invalid browser history order. Stored data was preserved.');
  let legacyCount = 0;
  for (const entry of byId.values()) {
    const count = await request(tx.objectStore('solutions').count(children(entry.id)));
    legacyCount += count;
    if (!entry.collection && count > COLLECTION_PAGE_SIZE) {
      entry.collection = { version: 1, id: `legacy:${entry.id}`, legacyEntryId: entry.id, count, preferredIndex: null };
    }
    if (entry.collection) {
      const stored = await checkStoredCollection(tx, entry.collection, entry.id);
      // Recover a newer durable prefix only for an interrupted checkpoint.
      // It cannot establish any new optimality or enumeration proof.
      entry.collection = {
        ...entry.collection,
        pending: undefined,
        count: entry.status === 'incomplete' ? stored : entry.collection.count,
      };
      if (!entry.result && entry.collection.count) {
        const store = tx.objectStore(entry.collection.legacyEntryId ? 'solutions' : 'collectionSolutions');
        const row = await request(store.get([entry.collection.legacyEntryId ?? entry.collection.id, 0]));
        if (!row?.value?.nodes || !row?.value?.edges) throw new Error('Missing preferred collection witness.');
        entry.result = row.value;
      }
      continue;
    }
    const solutions = (await request(
      tx.objectStore('solutions').getAll(children(entry.id), COLLECTION_PAGE_SIZE),
    )) as SolutionRecord[];
    for (const row of solutions) {
      if (
        row.index !== entry.results.length ||
        !row.value ||
        !Array.isArray(row.value.nodes) ||
        !Array.isArray(row.value.edges)
      ) {
        throw new Error('Invalid browser history solution records. Stored data was preserved.');
      }
      entry.results.push(row.value);
    }
  }
  if (legacyCount !== (await request(tx.objectStore('solutions').count())))
    throw new Error('Orphaned browser history solution records. Stored data was preserved.');
  for (const row of layouts) {
    const entry = byId.get(row.entryId);
    if (
      !entry ||
      !row.value ||
      typeof row.value.layoutKey !== 'string' ||
      !Array.isArray(row.value.nodes) ||
      !Array.isArray(row.value.edges)
    ) {
      throw new Error('Invalid browser history layout records. Stored data was preserved.');
    }
    Object.defineProperty(entry.layouts, row.key, {
      value: row.value,
      enumerable: true,
      writable: true,
      configurable: true,
    });
  }
  return {
    version: HISTORY_DOCUMENT_VERSION,
    selectedEntryId: meta.selectedEntryId,
    entries: meta.order.map((id) => byId.get(id)!),
  };
}

export function createBrowserHistoryStore(
  name = HISTORY_DATABASE,
): HistoryStore & { close(): void; collections: CollectionStore } {
  let connection: Promise<IDBDatabase> | undefined;
  let revision: number | null = null;
  let tail = Promise.resolve();
  function enqueue<T>(run: () => Promise<T>): Promise<T> {
    const result = tail.then(run);
    tail = result.then(
      () => {},
      () => {},
    );
    return result;
  }
  function database(): Promise<IDBDatabase> {
    connection ??= openDatabase(name)
      .then((db) => {
        db.onversionchange = () => {
          db.close();
          connection = undefined;
          revision = null;
        };
        db.onclose = () => {
          connection = undefined;
          revision = null;
        };
        return db;
      })
      .catch((error) => {
        connection = undefined;
        throw error;
      });
    return connection;
  }
  const collections = createBrowserCollections(database);
  return {
    collections,
    load: () =>
      enqueue(async () => {
        revision = null;
        const loaded = await transaction(await database(), 'readonly', async (tx) => {
          const meta = await metadata(tx);
          return { document: await loadDocument(tx, meta), revision: meta.revision };
        });
        revision = loaded.revision;
        return loaded.document;
      }),
    apply: (ops) =>
      enqueue(async () => {
        if (revision === null) throw new Error('Load browser history successfully before saving.');
        if (ops.length === 0) return;
        for (const op of ops)
          if (op.op === 'upsertEntry' && op.entry.collection) await collections.flush(op.entry.collection);
        const releasedCollections: string[] = [];
        const nextRevision = await transaction(await database(), 'readwrite', async (tx) => {
          const meta = await metadata(tx);
          if (meta.revision !== revision) {
            throw new Error(
              'History changed in another tab. Export any unsaved changes, then reload. No stored history was overwritten.',
            );
          }
          for (const op of ops) await applyOperation(tx, meta, op, releasedCollections);
          meta.revision++;
          if (!Number.isSafeInteger(meta.revision)) throw new Error('History revision overflow.');
          await request(tx.objectStore('meta').put(meta, 'document'));
          return meta.revision;
        });
        revision = nextRevision;
        for (const id of releasedCollections) collections.forget(id);
      }),
    close() {
      const closing = connection;
      connection = undefined;
      revision = null;
      void closing?.then(
        (db) => db.close(),
        () => {},
      );
    },
  };
}
