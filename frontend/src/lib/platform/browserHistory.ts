import {
  HISTORY_DOCUMENT_VERSION,
  type CachedGraphLayout,
  type HistoryDocument,
  type HistoryEntry,
} from '../historyModel';
import type { HistoryOp } from '../historyOps';
import type { Solution } from '../../types';
import type { HistoryStore } from './contracts';

export const HISTORY_DATABASE = 'satisfactory-flow-synthetizer.history';
const SCHEMA_VERSION = 1;
const STORES = ['meta', 'entries', 'solutions', 'layouts'];

type Metadata = { schema: number; revision: number; order: string[]; selectedEntryId: string | null };
type EntryRecord = Omit<HistoryEntry, 'results' | 'layouts'>;
type SolutionRecord = { entryId: string; index: number; value: Solution };
type LayoutRecord = { entryId: string; key: string; value: CachedGraphLayout };

function request<T>(operation: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    operation.onsuccess = () => resolve(operation.result);
    operation.onerror = () => reject(operation.error ?? new Error('IndexedDB request failed.'));
  });
}

/** The body may await IndexedDB requests only, never a timer, network call or UI event. */
function transaction<T>(
  database: IDBDatabase,
  mode: IDBTransactionMode,
  body: (tx: IDBTransaction) => Promise<T>,
): Promise<T> {
  return new Promise((resolve, reject) => {
    const tx = database.transaction(STORES, mode);
    let value: T;
    let failure: unknown;
    tx.oncomplete = () => resolve(value);
    tx.onabort = () => reject(failure ?? tx.error ?? new Error('History transaction aborted.'));
    void body(tx).then(
      (result) => {
        value = result;
      },
      (error) => {
        failure = error;
        try {
          tx.abort();
        } catch {
          reject(error);
        }
      },
    );
  });
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
      if (abandoned || event.oldVersion !== 0) {
        operation.transaction?.abort();
        return;
      }
      const db = operation.result;
      const meta = db.createObjectStore('meta');
      db.createObjectStore('entries', { keyPath: 'id' });
      db.createObjectStore('solutions', { keyPath: ['entryId', 'index'] });
      db.createObjectStore('layouts', { keyPath: ['entryId', 'key'] });
      meta.put(
        { schema: SCHEMA_VERSION, revision: 0, order: [], selectedEntryId: null } satisfies Metadata,
        'document',
      );
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

async function applyOperation(tx: IDBTransaction, meta: Metadata, op: HistoryOp): Promise<void> {
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
      await request(entries.put(entry));
      await request(solutions.delete(children(entry.id)));
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
        await request(entries.delete(id));
        await request(solutions.delete(children(id)));
        await request(tx.objectStore('layouts').delete(children(id)));
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
  const solutions = (await request(tx.objectStore('solutions').getAll())) as SolutionRecord[];
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
  for (const row of solutions) {
    const entry = byId.get(row.entryId);
    if (
      !entry ||
      row.index !== entry.results.length ||
      !row.value ||
      !Array.isArray(row.value.nodes) ||
      !Array.isArray(row.value.edges)
    ) {
      throw new Error('Invalid browser history solution records. Stored data was preserved.');
    }
    entry.results.push(row.value);
  }
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

export function createBrowserHistoryStore(name = HISTORY_DATABASE): HistoryStore & { close(): void } {
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
  return {
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
        const nextRevision = await transaction(await database(), 'readwrite', async (tx) => {
          const meta = await metadata(tx);
          if (meta.revision !== revision) {
            throw new Error(
              'History changed in another tab. Export any unsaved changes, then reload. No stored history was overwritten.',
            );
          }
          for (const op of ops) await applyOperation(tx, meta, op);
          meta.revision++;
          if (!Number.isSafeInteger(meta.revision)) throw new Error('History revision overflow.');
          await request(tx.objectStore('meta').put(meta, 'document'));
          return meta.revision;
        });
        revision = nextRevision;
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
