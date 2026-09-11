import { invoke, isTauri } from '@tauri-apps/api/core';
import { emptyDocument, parseHistoryDocument, type HistoryDocument, type HistoryEntry } from './historyModel';
import { diffHistoryOps, snapshotHistory, type HistorySnapshot } from './historyOps';

let lastSnapshot: HistorySnapshot | null = null;
let pending: HistorySnapshot | null = null;
let tail: Promise<void> = Promise.resolve();

export function rememberPersistedHistory(entries: HistoryEntry[], selectedEntryId: string | null): void {
  lastSnapshot = snapshotHistory(entries, selectedEntryId);
}

export async function loadHistoryDocument(): Promise<HistoryDocument> {
  if (!isTauri()) {
    const document = emptyDocument();
    rememberPersistedHistory(document.entries, document.selectedEntryId);
    return document;
  }
  const raw = await invoke<unknown>('load_history');
  const document = parseHistoryDocument(raw);
  rememberPersistedHistory(document.entries, document.selectedEntryId);
  return document;
}

/** Diff against the last committed snapshot and apply. Overlapping calls coalesce to the latest UI snapshot and run one invoke at a time. */
export async function applyHistoryChanges(entries: HistoryEntry[], selectedEntryId: string | null): Promise<void> {
  pending = snapshotHistory(entries, selectedEntryId);
  const run = tail.then(flushPending);
  tail = run.then(
    () => undefined,
    () => undefined,
  );
  await run;
}

async function flushPending(): Promise<void> {
  while (pending) {
    const next = pending;
    pending = null;
    const ops = diffHistoryOps(lastSnapshot, next);
    if (ops.length === 0) continue;
    try {
      if (isTauri()) {
        await invoke('apply_history_ops', { ops });
      }
      lastSnapshot = next;
    } catch (error) {
      // A lost acknowledgement can leave the database ahead of our snapshot.
      // Prefix failures roll back the entire batch. Replace only entries whose
      // append could not be applied, and keep all other operations in the batch.
      if (typeof error === 'string' && error.startsWith('history_append_prefix_mismatch:')) {
        const replacements = new Map(next.entries.map((entry) => [entry.id, entry]));
        if (ops.some((op) => op.op === 'appendSolutions')) {
          try {
            await invoke('apply_history_ops', {
              ops: ops.map((op) =>
                op.op === 'appendSolutions' ? { op: 'upsertEntry', entry: replacements.get(op.id)! } : op,
              ),
            });
            lastSnapshot = next;
            continue;
          } catch (recoveryError) {
            pending ??= next;
            throw recoveryError;
          }
        }
      }
      pending ??= next;
      throw error;
    }
  }
}

export function resetHistoryPersistForTests(): void {
  lastSnapshot = null;
  pending = null;
  tail = Promise.resolve();
}
