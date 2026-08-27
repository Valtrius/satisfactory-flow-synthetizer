import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { loadHistoryDocument, saveHistoryDocument } from './historyPersist';
import type { HistoryDocument, HistoryEntry } from './historyModel';

export type CloseFlushOptions = {
  flush: () => Promise<void>;
};

/** Hide the window first, flush history, then destroy. Returns an unlisten callback. */
export async function installCloseFlush(options: CloseFlushOptions): Promise<() => void> {
  try {
    if (!isTauri()) return () => {};
    const appWindow = getCurrentWindow();
    let closing = false;
    let unlisten = await appWindow.onCloseRequested(async (event) => {
      event.preventDefault();
      if (closing) return;
      closing = true;
      try {
        unlisten();
      } catch {
        /* ignore */
      }
      try {
        await appWindow.hide();
      } catch {
        /* continue */
      }
      try {
        await options.flush();
      } catch {
        /* still exit */
      }
      try {
        await appWindow.destroy();
      } catch {
        try {
          await appWindow.close();
        } catch {
          /* give up */
        }
      }
    });
    return () => {
      try {
        unlisten();
      } catch {
        /* ignore */
      }
    };
  } catch {
    return () => {};
  }
}

export type PersistController = {
  /** Call when entries/selection change (debounced write). */
  schedule: (entries: HistoryEntry[], selectedEntryId: string | null) => void;
  flushNow: (entries: HistoryEntry[], selectedEntryId: string | null) => Promise<void>;
  dispose: () => void;
};

export type PersistOptions = {
  delayMs?: number;
  isReady: () => boolean;
  onError: (message: string) => void;
};

export function createPersistController(options: PersistOptions): PersistController {
  const delayMs = options.delayMs ?? 400;
  let timer: ReturnType<typeof setTimeout> | null = null;

  function clearTimer(): void {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  return {
    schedule(entries, selectedEntryId) {
      if (!options.isReady()) return;
      clearTimer();
      timer = setTimeout(() => {
        timer = null;
        void saveHistoryDocument(entries, selectedEntryId).catch((error) => {
          options.onError(`Could not save history: ${error instanceof Error ? error.message : String(error)}`);
        });
      }, delayMs);
    },
    async flushNow(entries, selectedEntryId) {
      if (!options.isReady()) return;
      clearTimer();
      await saveHistoryDocument(entries, selectedEntryId);
    },
    dispose() {
      clearTimer();
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
