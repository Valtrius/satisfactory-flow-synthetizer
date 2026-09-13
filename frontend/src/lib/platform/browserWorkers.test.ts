import { afterEach, expect, it, vi } from 'vitest';
import {
  BROWSER_WORKER_PREFERENCE,
  browserThreadCount,
  browserWorkerChoices,
  readBrowserWorkers,
  saveBrowserWorkers,
} from './browserWorkers';

afterEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

it.each([1, 12, 24, 32, 64, 192])('uses all %s reported client threads without an eight-worker cap', (threads) => {
  expect(browserThreadCount(threads)).toBe(threads);
  expect(readBrowserWorkers(threads)).toBe('auto');
  const choices = browserWorkerChoices(threads, 'auto');
  expect(choices).toContain(threads);
  expect(choices[0]).toBe(1);
  expect(choices.every((count) => count <= threads)).toBe(true);
});

it.each([0, -1, 1.5, NaN, Infinity, 4_294_967_296])(
  'falls back to one thread for an invalid hardware report %s',
  (value) => {
    expect(browserThreadCount(value)).toBe(1);
  },
);

it('preserves explicit choices, follows new hardware in automatic mode and resets obsolete preview defaults', () => {
  localStorage.setItem('sfs.browser-workers.v1', '1');
  expect(readBrowserWorkers(32)).toBe('auto');
  saveBrowserWorkers(12);
  expect(readBrowserWorkers(32)).toBe(12);
  expect(browserWorkerChoices(32, 12)).toContain(12);
  expect(readBrowserWorkers(8)).toBe('auto');
  saveBrowserWorkers('auto');
  expect(localStorage.getItem(BROWSER_WORKER_PREFERENCE)).toBe('auto');
  expect(readBrowserWorkers(64)).toBe('auto');
});

it('keeps automatic solving available when preference storage fails', () => {
  vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
    throw new Error('storage unavailable');
  });
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new Error('storage unavailable');
  });
  expect(readBrowserWorkers(32)).toBe('auto');
  expect(() => saveBrowserWorkers(16)).not.toThrow();
});
