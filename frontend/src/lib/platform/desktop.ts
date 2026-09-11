import * as tauriCore from '@tauri-apps/api/core';
import type { JobSnapshot } from '../../types';
import type { CloseFlushOptions, JobClient, PlatformServices } from './contracts';

function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return tauriCore.invoke<T>(command, args);
}

async function invokeJob<T>(command: string, args: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw error instanceof Error ? error : new Error(String(error));
  }
}

function createDesktopJobs(): JobClient {
  const jobs: JobClient = {
    create: (request) => invokeJob('create_job', { request }),
    get: (jobId) => invokeJob('get_job', { jobId }),
    cancel: (jobId) => invokeJob('cancel_job', { jobId }),
    release: (jobId) => invoke('release_job', { jobId }),
    shutdown: () => invoke('shutdown_jobs'),
    resume: () => invoke('resume_jobs'),
    async watch(jobId, onSnapshot, onError) {
      let closed = false;
      let unlisten: (() => void) | undefined;
      try {
        const { listen } = await import('@tauri-apps/api/event');
        unlisten = await listen<JobSnapshot>('job-snapshot', (event) => {
          if (!closed && event.payload.jobId === jobId) onSnapshot(event.payload);
        });
        // Subscribe first, then reconcile. The queue rejects older snapshot sequences.
        onSnapshot(await jobs.get(jobId));
      } catch {
        onError();
      }
      return {
        close() {
          closed = true;
          unlisten?.();
          unlisten = undefined;
        },
      };
    },
  };
  return jobs;
}

async function installDesktopCloseFlush(options: CloseFlushOptions): Promise<() => void> {
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  const appWindow = getCurrentWindow();
  let closing = false;
  return appWindow.onCloseRequested(async (event) => {
    event.preventDefault();
    if (closing) return;
    closing = true;
    try {
      options.onClosing?.();
      await appWindow.hide();
      for (;;) {
        try {
          await options.prepare?.();
          break;
        } catch (error) {
          const { message } = await import('@tauri-apps/plugin-dialog');
          const choice = await message(`Solver cleanup failed.\n\n${String(error)}`, {
            title: 'Solver cleanup failed',
            kind: 'error',
            buttons: { ok: 'Retry cleanup', cancel: 'Return to app' },
          });
          if (choice !== 'Retry cleanup') throw error;
        }
      }
      for (;;) {
        try {
          await options.flush();
          break;
        } catch (error) {
          const { message } = await import('@tauri-apps/plugin-dialog');
          const choice = await message(`History could not be saved.\n\n${String(error)}`, {
            title: 'History save failed',
            kind: 'error',
            buttons: { ok: 'Exit without saving', cancel: 'Retry' },
          });
          // Dismissing the dialog is not permission to discard unsaved history.
          if (choice === 'Exit without saving') break;
        }
      }
      await appWindow.destroy();
    } catch (error) {
      closing = false;
      await options.onReopen?.().catch((resumeError) => options.onError(String(resumeError)));
      options.onError(`Could not close the application: ${String(error)}`);
      await appWindow.show().catch(() => {});
    }
  });
}

export function createDesktopPlatform(): PlatformServices {
  return {
    capabilities: { runtime: 'desktop', solve: 'ready', persistentStorage: 'native' },
    jobs: createDesktopJobs(),
    history: { load: () => invoke('load_history'), apply: (ops) => invoke('apply_history_ops', { ops }) },
    files: {
      async saveText({ contents, fileName, format }) {
        const { save } = await import('@tauri-apps/plugin-dialog');
        const { writeTextFile } = await import('@tauri-apps/plugin-fs');
        const path = await save({
          defaultPath: fileName,
          filters: [{ name: format.toUpperCase(), extensions: [format] }],
        });
        if (path) await writeTextFile(path, contents);
      },
      async openJsonText() {
        const { open } = await import('@tauri-apps/plugin-dialog');
        const { readTextFile } = await import('@tauri-apps/plugin-fs');
        const path = await open({ multiple: false, filters: [{ name: 'JSON', extensions: ['json'] }] });
        return !path || Array.isArray(path) ? null : readTextFile(path);
      },
    },
    lifecycle: { installCloseFlush: installDesktopCloseFlush },
  };
}
