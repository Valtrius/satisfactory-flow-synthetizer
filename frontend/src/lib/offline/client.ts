export type OfflineStatus = {
  buildId: string;
  cached: number;
  total: number;
  bytes: number;
  complete: boolean;
  shellComplete: boolean;
};
export type OfflineState = {
  available: boolean;
  busy: boolean;
  update: boolean;
  status: OfflineStatus | null;
  error: string;
};

/** Offline assets are independent of IndexedDB history and solver lifetimes. */
export function createOfflineClient(changed: (state: OfflineState) => void) {
  let registration: ServiceWorkerRegistration | undefined;
  let disposed = false;
  let state: OfflineState = { available: false, busy: false, update: false, status: null, error: '' };
  const buildId = document.querySelector<HTMLMetaElement>('meta[name="sfs-build"]')?.content;
  const listeners: (() => void)[] = [];
  const emit = (patch: Partial<OfflineState>) => {
    state = { ...state, ...patch };
    if (!disposed) changed({ ...state });
  };
  function observe(target: EventTarget, name: string, listener: EventListener) {
    target.addEventListener(name, listener);
    listeners.push(() => target.removeEventListener(name, listener));
  }
  function request(kind: 'offline-status' | 'offline-prepare'): Promise<OfflineStatus> {
    return new Promise((resolve, reject) => {
      const worker = navigator.serviceWorker.controller ?? registration?.active;
      if (!worker) {
        reject(new Error('Offline worker is still installing. Try again shortly.'));
        return;
      }
      const channel = new MessageChannel();
      const timeout = setTimeout(
        () => {
          channel.port1.close();
          reject(new Error('Offline storage did not respond. Check the connection and available storage, then retry.'));
        },
        kind === 'offline-prepare' ? 600_000 : 10_000,
      );
      channel.port1.onmessage = ({ data }) => {
        clearTimeout(timeout);
        channel.port1.close();
        if (data?.ok && data.status?.buildId === buildId) resolve(data.status);
        else reject(new Error(data?.error || 'Invalid offline worker response.'));
      };
      try {
        worker.postMessage({ kind, buildId }, [channel.port2]);
      } catch (error) {
        clearTimeout(timeout);
        channel.port1.close();
        channel.port2.close();
        reject(error);
      }
    });
  }
  async function refresh() {
    if (disposed || !registration?.active) return;
    try {
      emit({ status: await request('offline-status'), update: Boolean(registration.waiting) });
    } catch (error) {
      emit({ error: String(error) });
    }
  }
  return {
    async start() {
      if (!buildId || !('serviceWorker' in navigator) || !window.isSecureContext) return;
      try {
        registration = await navigator.serviceWorker.register(new URL('service-worker.js', document.baseURI), {
          scope: new URL('./', document.baseURI).href,
          updateViaCache: 'none',
        });
        if (disposed) return;
        emit({ available: Boolean(registration.active) });
        const track = () => {
          const worker = registration?.installing;
          if (!worker) return;
          observe(worker, 'statechange', () => {
            if (worker.state === 'installed') emit({ update: Boolean(registration?.waiting) });
            if (worker.state === 'activated') {
              emit({ available: true });
              void refresh();
            }
            if (worker.state === 'redundant')
              emit({
                error:
                  'Could not install the offline update. The current version was kept. Check the connection and storage, then retry.',
              });
          });
        };
        observe(registration, 'updatefound', track);
        observe(navigator.serviceWorker, 'controllerchange', () => {
          void refresh();
        });
        observe(window, 'online', () => {
          void refresh();
        });
        observe(window, 'offline', () => {
          void refresh();
        });
        track();
        await refresh();
      } catch (error) {
        emit({ error: `Offline caching is unavailable: ${String(error)}` });
      }
    },
    async prepare() {
      if (state.busy) return;
      emit({ busy: true, error: '' });
      try {
        emit({ status: await request('offline-prepare') });
      } catch (error) {
        emit({ error: `Offline download failed: ${String(error)}. Existing history was not changed.` });
      } finally {
        emit({ busy: false });
      }
    },
    async checkUpdate() {
      if (!registration || state.busy) return;
      emit({ busy: true, error: '' });
      try {
        await registration.update();
        await refresh();
      } catch (error) {
        emit({ error: `Could not check for updates: ${String(error)}` });
      } finally {
        emit({ busy: false });
      }
    },
    dispose() {
      disposed = true;
      for (const remove of listeners) remove();
    },
  };
}
