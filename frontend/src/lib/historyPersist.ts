import { invoke, isTauri } from '@tauri-apps/api/core';
import {
  emptyDocument,
  parseHistoryDocument,
  persistableEntries,
  HISTORY_DOCUMENT_VERSION,
  type HistoryDocument,
  type HistoryEntry,
} from './historyModel';

export async function loadHistoryDocument(): Promise<HistoryDocument> {
  if (!isTauri()) return emptyDocument();
  const raw = await invoke<unknown>('load_history');
  return parseHistoryDocument(raw);
}

export async function saveHistoryDocument(entries: HistoryEntry[], selectedEntryId: string | null): Promise<void> {
  if (!isTauri()) return;
  const persisted = persistableEntries(entries);
  const document: HistoryDocument = {
    version: HISTORY_DOCUMENT_VERSION,
    entries: persisted,
    selectedEntryId:
      selectedEntryId && persisted.some((entry) => entry.id === selectedEntryId)
        ? selectedEntryId
        : (persisted[0]?.id ?? null),
  };
  await invoke('save_history', { document });
}
