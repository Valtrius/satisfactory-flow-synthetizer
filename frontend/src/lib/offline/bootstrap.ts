/** Acquire a controlled document before the app can own jobs or unsaved edits. */
export async function prepareOfflineDocument(target: HTMLElement): Promise<boolean> {
  const buildId = document.querySelector<HTMLMetaElement>('meta[name="sfs-build"]')?.content;
  if (!buildId || !('serviceWorker' in navigator) || !window.isSecureContext) return true;
  const reload = () => {
    window.location.reload();
    return false;
  };
  try {
    const controller = navigator.serviceWorker.controller;
    if (controller) {
      // Also recognize workers installed before the bootstrap handshake existed.
      const matches = await new Promise<boolean>((resolve) => {
        const channel = new MessageChannel();
        const finish = (value: boolean) => {
          clearTimeout(timer);
          channel.port1.close();
          resolve(value);
        };
        const timer = setTimeout(() => finish(true), 10_000);
        channel.port1.onmessage = ({ data }) => {
          const activeBuild = data?.buildId ?? data?.status?.buildId;
          finish(activeBuild ? activeBuild === buildId : !String(data?.error).includes('belong to different builds'));
        };
        controller.postMessage({ kind: 'offline-status', buildId }, [channel.port2]);
      });
      return matches || reload();
    }
    const message = document.createElement('p');
    message.className = 'p-6';
    message.setAttribute('role', 'status');
    message.textContent = 'Preparing the app for offline use…';
    target.replaceChildren(message);
    const registration = await navigator.serviceWorker.register(new URL('service-worker.js', document.baseURI), {
      scope: new URL('./', document.baseURI).href,
      updateViaCache: 'none',
    });
    const worker = registration.active ?? registration.installing ?? registration.waiting;
    if (worker && !['activated', 'redundant'].includes(worker.state)) {
      await new Promise<void>((resolve) => {
        const changed = () => {
          if (!['activated', 'redundant'].includes(worker.state)) return;
          worker.removeEventListener('statechange', changed);
          resolve();
        };
        worker.addEventListener('statechange', changed);
        changed();
      });
    }
    // Navigation selects the active worker's complete index and runtime together.
    // A failed installation leaves online use available without claiming this tab later.
    return registration.active?.state === 'activated' ? reload() : true;
  } catch {
    return true;
  }
}
