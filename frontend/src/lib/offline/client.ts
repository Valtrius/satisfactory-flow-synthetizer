type OfflineStatus = {
  buildId: string;
  cached: number;
  total: number;
  bytes: number;
  complete: boolean;
  shellComplete: boolean;
};

/** Offline assets are independent of IndexedDB history and solver lifetimes. */
export function createOfflineMaintenance() {
  let registration: ServiceWorkerRegistration | undefined;
  let disposed = false;
  const buildId = document.querySelector<HTMLMetaElement>('meta[name="sfs-build"]')?.content;
  const listeners: (() => void)[] = [];
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
  async function repairIfNeeded() {
    if (disposed || !registration?.active) return;
    try {
      const status = await request('offline-status');
      if (!status.complete && navigator.onLine) await request('offline-prepare');
    } catch {
      // Offline support is best-effort and intentionally has no visible UI.
    }
  }
  async function checkForUpdate() {
    if (disposed || !registration) return;
    try {
      await registration.update();
    } catch {
      // The active cached build remains usable when an update check fails.
    }
    await repairIfNeeded();
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
        observe(navigator.serviceWorker, 'controllerchange', () => {
          void repairIfNeeded();
        });
        observe(window, 'online', () => {
          void checkForUpdate();
        });
        await repairIfNeeded();
      } catch {
        // The app still works online if service-worker registration is unavailable.
      }
    },
    dispose() {
      disposed = true;
      for (const remove of listeners) remove();
    },
  };
}
