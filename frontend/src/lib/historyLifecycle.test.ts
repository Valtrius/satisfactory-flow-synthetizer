import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { createPersistController, installCloseFlush } from './historyLifecycle';
import { applyHistoryChanges } from './historyPersist';
import { message } from '@tauri-apps/plugin-dialog';

const windowMock = vi.hoisted(() => ({
  hide: vi.fn(),
  show: vi.fn(),
  destroy: vi.fn(),
  onCloseRequested: vi.fn(),
}));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => windowMock }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ message: vi.fn() }));
vi.mock('./historyPersist', () => ({ applyHistoryChanges: vi.fn(), loadHistoryDocument: vi.fn() }));

beforeEach(() => {
  vi.resetAllMocks();
  windowMock.hide.mockResolvedValue(undefined);
  windowMock.show.mockResolvedValue(undefined);
  windowMock.destroy.mockResolvedValue(undefined);
  vi.mocked(applyHistoryChanges).mockResolvedValue(undefined);
});
afterEach(() => vi.useRealTimers());

it('checkpoints the latest state within five seconds of continuous updates', async () => {
  vi.useFakeTimers();
  const persist = createPersistController({ isReady: () => true, onError: vi.fn() });
  for (let i = 0; i < 20; i++) {
    persist.schedule([], String(i));
    await vi.advanceTimersByTimeAsync(250);
  }
  expect(applyHistoryChanges).toHaveBeenCalledExactlyOnceWith([], '19');
  persist.dispose();
});

it('flushes immediately without leaving a stale checkpoint', async () => {
  vi.useFakeTimers();
  const persist = createPersistController({ isReady: () => true, onError: vi.fn() });
  persist.schedule([], 'old');
  await persist.flushNow([], 'new');
  await vi.advanceTimersByTimeAsync(5_000);
  expect(applyHistoryChanges).toHaveBeenCalledExactlyOnceWith([], 'new');
});

it.each(['Retry', 'Exit without saving'])('hides first and honours the %s save-failure choice', async (choice) => {
  const flush = vi.fn().mockRejectedValueOnce(new Error('disk full')).mockResolvedValue(undefined);
  vi.mocked(message).mockResolvedValue(choice);
  await installCloseFlush({ flush, onError: vi.fn() });
  await windowMock.onCloseRequested.mock.calls[0][0]({ preventDefault: vi.fn() });
  expect(windowMock.hide.mock.invocationCallOrder[0]).toBeLessThan(flush.mock.invocationCallOrder[0]);
  expect(flush).toHaveBeenCalledTimes(choice === 'Retry' ? 2 : 1);
  expect(windowMock.destroy).toHaveBeenCalledOnce();
  expect(message).toHaveBeenCalledWith(
    expect.stringContaining('disk full'),
    expect.objectContaining({
      buttons: { ok: 'Exit without saving', cancel: 'Retry' },
    }),
  );
});

it('restores the window instead of losing data when the failure dialog cannot open', async () => {
  const onError = vi.fn();
  vi.mocked(message).mockRejectedValue(new Error('dialog unavailable'));
  await installCloseFlush({ flush: vi.fn().mockRejectedValue('disk full'), onError });
  await windowMock.onCloseRequested.mock.calls[0][0]({ preventDefault: vi.fn() });
  expect(windowMock.destroy).not.toHaveBeenCalled();
  expect(windowMock.show).toHaveBeenCalledOnce();
  expect(onError).toHaveBeenCalledWith(expect.stringContaining('dialog unavailable'));
});

it('does not schedule or flush writes while history is still loading or has failed', async () => {
  vi.useFakeTimers();
  let ready = false;
  const persist = createPersistController({ isReady: () => ready, onError: vi.fn() });
  persist.schedule([], 'too-early');
  await persist.flushNow([], 'too-early');
  await vi.advanceTimersByTimeAsync(6_000);
  expect(applyHistoryChanges).not.toHaveBeenCalled();
  ready = true;
  persist.schedule([], 'loaded');
  await vi.advanceTimersByTimeAsync(400);
  expect(applyHistoryChanges).toHaveBeenCalledExactlyOnceWith([], 'loaded');
});

it('keeps cleanup hidden, retries failures, and flushes only after cleanup succeeds', async () => {
  const prepare = vi.fn().mockRejectedValueOnce('still stopping').mockResolvedValue(undefined);
  const flush = vi.fn().mockResolvedValue(undefined);
  vi.mocked(message).mockResolvedValue('Retry cleanup');
  await installCloseFlush({ prepare, flush, onError: vi.fn() });
  await windowMock.onCloseRequested.mock.calls[0][0]({ preventDefault: vi.fn() });
  expect(prepare).toHaveBeenCalledTimes(2);
  expect(windowMock.hide.mock.invocationCallOrder[0]).toBeLessThan(prepare.mock.invocationCallOrder[0]);
  expect(flush.mock.invocationCallOrder[0]).toBeGreaterThan(prepare.mock.invocationCallOrder[1]);
  expect(windowMock.show).not.toHaveBeenCalled();
  expect(windowMock.destroy).toHaveBeenCalledOnce();
});

it('returns to the app rather than abandoning an unfinished solver task', async () => {
  const onReopen = vi.fn().mockResolvedValue(undefined);
  vi.mocked(message).mockResolvedValue('Return to app');
  await installCloseFlush({
    prepare: vi.fn().mockRejectedValue('still stopping'),
    flush: vi.fn(),
    onReopen,
    onError: vi.fn(),
  });
  await windowMock.onCloseRequested.mock.calls[0][0]({ preventDefault: vi.fn() });
  expect(windowMock.destroy).not.toHaveBeenCalled();
  expect(onReopen).toHaveBeenCalledOnce();
  expect(windowMock.show).toHaveBeenCalledOnce();
});
