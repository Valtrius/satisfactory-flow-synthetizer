const progressId = 'sfs-web-load-progress';

type OfflineProgressMessage = {
  kind: 'offline-progress';
  loaded: number;
  total: number;
  state: 'progress' | 'complete' | 'failed';
};

function progressElement(): HTMLElement | null {
  return document.getElementById(progressId);
}

export function showWebLoadProgress(loaded?: number, total?: number): void {
  const element = progressElement();
  if (!element) return;
  element.hidden = false;

  if (loaded === undefined || total === undefined || total <= 0) {
    element.dataset.mode = 'indeterminate';
    element.removeAttribute('aria-valuenow');
    return;
  }

  const percent = Math.max(0, Math.min(100, (loaded / total) * 100));
  element.dataset.mode = 'determinate';
  element.style.setProperty('--sfs-load-progress', `${percent}%`);
  element.setAttribute('aria-valuenow', String(Math.round(percent)));
}

export function hideWebLoadProgress(): void {
  const element = progressElement();
  if (element) element.hidden = true;
}

export function installWebLoadProgressListener(): () => void {
  if (!('serviceWorker' in navigator)) return () => {};

  const listener = (event: MessageEvent<OfflineProgressMessage>) => {
    const message = event.data;
    if (message?.kind !== 'offline-progress') return;

    if (message.state === 'failed') {
      hideWebLoadProgress();
      return;
    }

    showWebLoadProgress(message.loaded, message.total);
    if (message.state === 'complete') window.setTimeout(hideWebLoadProgress, 120);
  };

  navigator.serviceWorker.addEventListener('message', listener);
  return () => navigator.serviceWorker.removeEventListener('message', listener);
}
