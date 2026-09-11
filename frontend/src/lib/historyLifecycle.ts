import { getPlatform } from './platform';
import type { CloseFlushOptions } from './platform/contracts';
import { loadHistoryDocument, applyHistoryChanges } from './historyPersist';
import type { HistoryDocument, HistoryEntry } from './historyModel';
export type { CloseFlushOptions } from './platform/contracts';

export const installCloseFlush = (options: CloseFlushOptions): Promise<() => void> =>
  getPlatform().lifecycle.installCloseFlush(options);

export type PersistController = {
  schedule: (entries: HistoryEntry[], selectedEntryId: string | null) => void;
  flushNow: (entries: HistoryEntry[], selectedEntryId: string | null) => Promise<void>;
  dispose: () => void;
};

export type PersistOptions = {
  delayMs?: number;
  maxWaitMs?: number;
  isReady: () => boolean;
  onError: (message: string) => void;
};

export function createPersistController(options: PersistOptions): PersistController {
  const delayMs = options.delayMs ?? 400;
  const maxWaitMs = options.maxWaitMs ?? 5_000;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let checkpoint: ReturnType<typeof setTimeout> | null = null;
  let pending: { entries: HistoryEntry[]; selectedEntryId: string | null } | null = null;

  function clearTimers(): void {
    if (timer != null) clearTimeout(timer);
    if (checkpoint != null) clearTimeout(checkpoint);
    timer = null;
    checkpoint = null;
  }

  function savePending(): void {
    const next = pending;
    pending = null;
    clearTimers();
    if (!next) return;
    void applyHistoryChanges(next.entries, next.selectedEntryId).catch((error) => {
      options.onError(`Could not save history: ${error instanceof Error ? error.message : String(error)}`);
    });
  }

  return {
    schedule(entries, selectedEntryId) {
      if (!options.isReady()) return;
      pending = { entries, selectedEntryId };
      if (timer != null) clearTimeout(timer);
      timer = setTimeout(savePending, delayMs);
      checkpoint ??= setTimeout(savePending, maxWaitMs);
    },
    async flushNow(entries, selectedEntryId) {
      if (!options.isReady()) return;
      clearTimers();
      pending = null;
      await applyHistoryChanges(entries, selectedEntryId);
    },
    dispose() {
      clearTimers();
      pending = null;
    },
  };
}

export async function loadHistoryOrEmpty(): Promise<
  { ok: true; document: HistoryDocument } | { ok: false; error: string }
> {
  try {
    const document = await loadHistoryDocument();
    return { ok: true, document };
  } catch (error) {
    return {
      ok: false,
      error: `Could not load history: ${error instanceof Error ? error.message : String(error)}`,
    };
  }
}
