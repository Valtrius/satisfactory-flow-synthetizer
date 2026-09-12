import { getPlatform } from './platform';
import { exportBundlePayload, exportEntryPayload, persistableEntries, type HistoryEntry } from './historyModel';
import { COLLECTION_PAGE_SIZE } from './platform/browserCollections';
import type { CollectionRef } from '../types';

export async function exportHistoryEntry(entry: HistoryEntry): Promise<void> {
  if (entry.collection) {
    await savePagedHistory([entry], true);
    return;
  }
  await getPlatform().files.saveText({
    contents: JSON.stringify(exportEntryPayload(entry), null, 2),
    fileName: 'balancer-history-entry.json',
    format: 'json',
  });
}

export async function exportHistoryBundle(entries: HistoryEntry[]): Promise<void> {
  if (entries.some((entry) => entry.collection)) {
    await savePagedHistory(entries, false);
    return;
  }
  await getPlatform().files.saveText({
    contents: JSON.stringify(exportBundlePayload(entries), null, 2),
    fileName: 'balancer-history.json',
    format: 'json',
  });
}

export async function importHistoryPayload(): Promise<unknown | null> {
  const text = await getPlatform().files.openJsonText();
  return text === null ? null : JSON.parse(text);
}

async function* exportChunks(entries: HistoryEntry[], single: boolean): AsyncGenerator<string> {
  const store = getPlatform().collections;
  yield single ? '{"kind":"history-entry","version":2,"entry":' : '{"kind":"history-bundle","version":2,"entries":[';
  let firstEntry = true;
  for (const entry of entries) {
    if (entry.status === 'queued') continue;
    if (!firstEntry) yield ',';
    firstEntry = false;
    const { results, collection, ...metadata } = persistableEntries([entry])[0];
    yield JSON.stringify(metadata).slice(0, -1) + ',"results":[';
    let first = true;
    if (collection) {
      if (!store) throw new Error('This host cannot export the local solution collection.');
      for (let offset = 0; offset < collection.count; offset += COLLECTION_PAGE_SIZE) {
        const page = await store.read(collection, offset, COLLECTION_PAGE_SIZE);
        for (const row of page) {
          if (!first) yield ',';
          first = false;
          yield JSON.stringify(collection.preferredIndex === row.index && entry.result ? entry.result : row.solution);
        }
      }
    } else {
      for (const solution of results) {
        if (!first) yield ',';
        first = false;
        yield JSON.stringify(solution);
      }
    }
    yield ']}';
  }
  yield single ? '}' : ']}';
}

async function savePagedHistory(entries: HistoryEntry[], single: boolean): Promise<void> {
  const files = getPlatform().files;
  const file = { fileName: single ? 'balancer-history-entry.json' : 'balancer-history.json', format: 'json' as const };
  if (!files.saveChunks) throw new Error('This host does not support paged history export.');
  // JSON/Blob bytes accumulate for the download. At most one page of graph
  // objects is decoded at a time, and no local collection IDs enter the file.
  await files.saveChunks(file, exportChunks(entries, single));
}

/** Move large imported collections to local records before attaching them to Svelte. */
export async function pageImportedEntries(entries: HistoryEntry[], ids: string[]): Promise<HistoryEntry[]> {
  const store = getPlatform().collections;
  if (!store) return entries;
  const result: HistoryEntry[] = [];
  const staged: CollectionRef[] = [];
  try {
    for (const entry of entries) {
      if (!ids.includes(entry.id) || entry.results.length <= COLLECTION_PAGE_SIZE) {
        result.push(entry);
        continue;
      }
      let ref = await store.create(crypto.randomUUID());
      staged.push(ref);
      for (let offset = 0; offset < entry.results.length; offset += 16) {
        ref = await store.append(
          ref,
          entry.results.slice(offset, offset + 16).map((solution, index) => ({ index: offset + index, solution })),
        );
      }
      const selected = entry.result ? JSON.stringify([entry.result.nodes, entry.result.edges]) : '';
      const preferred = entry.results.findIndex(
        (solution) => JSON.stringify([solution.nodes, solution.edges]) === selected,
      );
      result.push({ ...entry, results: [], collection: { ...ref, preferredIndex: preferred < 0 ? null : preferred } });
    }
    return result;
  } catch (error) {
    // Nothing from this import has been attached to history yet. This also
    // discards earlier entries when a later entry fails partway through.
    const cleanup = await Promise.allSettled(staged.map((ref) => store.discard(ref)));
    const failure = cleanup.find((result) => result.status === 'rejected');
    if (failure?.status === 'rejected')
      throw new Error(`${String(error)}; could not discard imported collection: ${String(failure.reason)}`);
    throw error;
  }
}
