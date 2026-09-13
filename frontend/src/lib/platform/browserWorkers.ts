export type BrowserWorkerPreference = 'auto' | number;
export const BROWSER_WORKER_PREFERENCE = 'sfs.browser-workers.v2';

/** The browser may report fewer logical processors than the machine has. */
export function browserThreadCount(
  value = typeof navigator === 'undefined' ? 1 : navigator.hardwareConcurrency,
): number {
  return Number.isSafeInteger(value) && value > 0 && value <= 0xffff_ffff ? value : 1;
}

export function readBrowserWorkers(threads: number): BrowserWorkerPreference {
  try {
    const saved = localStorage.getItem(BROWSER_WORKER_PREFERENCE);
    if (saved === null || saved === 'auto') return 'auto';
    const count = Number(saved);
    return Number.isSafeInteger(count) && count >= 1 && count <= threads ? count : 'auto';
  } catch {
    return 'auto';
  }
}

export function saveBrowserWorkers(value: BrowserWorkerPreference): void {
  try {
    localStorage.setItem(BROWSER_WORKER_PREFERENCE, String(value));
  } catch {
    // A missing preference must not prevent solving or history checkpoints.
  }
}

/** Powers of two keep the menu short, with the exact client limit always present. */
export function browserWorkerChoices(threads: number, preference: BrowserWorkerPreference): number[] {
  const values = new Set([1, threads]);
  for (let count = 2; count < threads; count *= 2) values.add(count);
  if (preference !== 'auto') values.add(preference);
  return [...values].sort((left, right) => left - right);
}
