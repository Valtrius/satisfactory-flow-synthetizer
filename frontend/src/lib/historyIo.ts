import { getPlatform } from './platform';
import { exportBundlePayload, exportEntryPayload, type HistoryEntry } from './historyModel';

export async function exportHistoryEntry(entry: HistoryEntry): Promise<void> {
  await getPlatform().files.saveText({
    contents: JSON.stringify(exportEntryPayload(entry), null, 2),
    fileName: 'balancer-history-entry.json',
    format: 'json',
  });
}

export async function exportHistoryBundle(entries: HistoryEntry[]): Promise<void> {
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
