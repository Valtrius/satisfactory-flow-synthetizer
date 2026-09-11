import { createBrowserHistoryStore } from './browserHistory';
import { publicViewerUrl } from '../sharing/viewer';
import type { CloseFlushOptions, FileActions, PlatformServices } from './contracts';

export const BROWSER_SOLVE_UNAVAILABLE =
  'Browser solving is not available in this development build. History can be imported, edited and saved locally.';

async function unavailable(): Promise<never> {
  throw new Error(BROWSER_SOLVE_UNAVAILABLE);
}

export const browserFiles: FileActions = {
  async saveText({ contents, fileName, format }) {
    const mime = format === 'json' ? 'application/json' : 'image/svg+xml';
    const url = URL.createObjectURL(new Blob([contents], { type: `${mime};charset=utf-8` }));
    const link = document.createElement('a');
    link.href = url;
    link.download = fileName;
    document.body.append(link);
    try {
      link.click();
    } finally {
      link.remove();
      // Give the browser a task to start consuming the download before revocation.
      setTimeout(() => URL.revokeObjectURL(url), 0);
    }
  },
  openJsonText(maxBytes) {
    return new Promise((resolve, reject) => {
      const input = document.createElement('input');
      input.type = 'file';
      input.accept = 'application/json,.json';
      input.hidden = true;
      document.body.append(input);
      const cleanup = () => {
        input.onchange = null;
        input.oncancel = null;
        input.remove();
      };
      input.oncancel = () => {
        cleanup();
        resolve(null);
      };
      input.onchange = () => {
        const file = input.files?.[0];
        cleanup();
        if (!file) {
          resolve(null);
          return;
        }
        if (maxBytes !== undefined && file.size > maxBytes) {
          reject(new Error(`Selected file exceeds ${maxBytes} bytes.`));
          return;
        }
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(reader.error ?? new Error('Could not read the selected file.'));
        reader.onabort = () => reject(new Error('Reading the selected file was interrupted.'));
        try {
          reader.readAsText(file);
        } catch (error) {
          reject(error);
        }
      };
      try {
        input.click();
      } catch (error) {
        cleanup();
        reject(error);
      }
    });
  },
};

export async function installBrowserCloseFlush(options: CloseFlushOptions): Promise<() => void> {
  const checkpoint = () => {
    // This supplements normal checkpoints, not a promise of durability at tab closure.
    void options.flush().catch((error) => options.onError(`Could not save history: ${String(error)}`));
  };
  const visibility = () => {
    if (document.visibilityState === 'hidden') checkpoint();
  };
  document.addEventListener('visibilitychange', visibility);
  window.addEventListener('pagehide', checkpoint);
  return () => {
    document.removeEventListener('visibilitychange', visibility);
    window.removeEventListener('pagehide', checkpoint);
  };
}

export function createBrowserPlatform(): PlatformServices {
  return {
    capabilities: { runtime: 'browser', solve: 'unavailable', persistentStorage: 'best-effort' },
    jobs: {
      create: unavailable,
      get: unavailable,
      watch: unavailable,
      cancel: unavailable,
      release: unavailable,
      shutdown: async () => [],
      resume: async () => {},
    },
    history: createBrowserHistoryStore(),
    files: browserFiles,
    shareViewerUrl: publicViewerUrl('browser'),
    lifecycle: { installCloseFlush: installBrowserCloseFlush },
  };
}
