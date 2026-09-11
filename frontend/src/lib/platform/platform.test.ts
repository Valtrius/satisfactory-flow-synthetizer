import { afterEach, describe, expect, it, vi } from 'vitest';
import { createDesktopPlatform } from './desktop';
import { browserFiles, createBrowserPlatform, installBrowserCloseFlush } from './browser';
import { importHistoryPayload } from '../historyIo';
import { historyEntry } from '../../test/fixtures';
import { entryToJobSnapshot } from '../historyModel';

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke, isTauri: () => false }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));
afterEach(() => {
  vi.restoreAllMocks();
  vi.resetAllMocks();
  document.body.replaceChildren();
});

describe('platform job contracts', () => {
  it('subscribes before reconciling, filters job IDs and closes the listener once', async () => {
    const jobs = createDesktopPlatform().jobs;
    const current = { ...entryToJobSnapshot(historyEntry()), jobId: 'job', sequence: 1 };
    const receive = vi.fn();
    const onError = vi.fn();
    const unlisten = vi.fn();
    mocks.listen.mockResolvedValue(unlisten);
    mocks.invoke.mockResolvedValue(current);
    const watch = await jobs.watch('job', receive, onError);
    expect(mocks.listen).toHaveBeenCalledWith('job-snapshot', expect.any(Function));
    expect(mocks.listen.mock.invocationCallOrder[0]).toBeLessThan(mocks.invoke.mock.invocationCallOrder[0]);
    expect(mocks.invoke).toHaveBeenCalledWith('get_job', { jobId: 'job' });
    expect(receive).toHaveBeenCalledExactlyOnceWith(current);
    const event = mocks.listen.mock.calls[0][1];
    event({ payload: { ...current, jobId: 'other' } });
    expect(receive).toHaveBeenCalledOnce();
    event({ payload: { ...current, sequence: 2 } });
    expect(receive).toHaveBeenCalledTimes(2);
    watch.close();
    watch.close();
    event({ payload: { ...current, sequence: 3 } });
    expect(receive).toHaveBeenCalledTimes(2);
    expect(unlisten).toHaveBeenCalledOnce();
    expect(onError).not.toHaveBeenCalled();
  });

  it('keeps subscription recovery after a failed initial snapshot and normalizes command errors', async () => {
    const jobs = createDesktopPlatform().jobs;
    const receive = vi.fn();
    const onError = vi.fn();
    mocks.listen.mockResolvedValue(vi.fn());
    mocks.invoke.mockRejectedValue('native failure');
    const watch = await jobs.watch('job', receive, onError);
    expect(onError).toHaveBeenCalledOnce();
    const snapshot = { ...entryToJobSnapshot(historyEntry()), jobId: 'job' };
    mocks.listen.mock.calls[0][1]({ payload: snapshot });
    expect(receive).toHaveBeenCalledWith(snapshot);
    await expect(jobs.cancel('job')).rejects.toThrow('native failure');
    watch.close();
  });

  it('preserves create, release, shutdown and admission-reopen command shapes', async () => {
    const jobs = createDesktopPlatform().jobs;
    mocks.invoke.mockResolvedValue('id');
    const request = historyEntry().request;
    expect(await jobs.create(request)).toBe('id');
    await jobs.release('id');
    await jobs.shutdown();
    await jobs.resume();
    expect(mocks.invoke.mock.calls).toEqual([
      ['create_job', { request }],
      ['release_job', { jobId: 'id' }],
      ['shutdown_jobs', undefined],
      ['resume_jobs', undefined],
    ]);
  });

  it('never invokes native IPC or invents results in browser mode', async () => {
    const platform = createBrowserPlatform();
    expect(platform.capabilities).toEqual({
      runtime: 'browser',
      solve: 'unavailable',
      persistentStorage: 'best-effort',
    });
    await expect(platform.jobs.create(historyEntry().request)).rejects.toThrow('Browser solving is not available');
    await expect(platform.jobs.get('job')).rejects.toThrow('Browser solving is not available');
    await expect(platform.jobs.watch('job', vi.fn(), vi.fn())).rejects.toThrow('Browser solving is not available');
    await expect(platform.jobs.shutdown()).resolves.toEqual([]);
    await platform.jobs.resume();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});

it('browser lifecycle checkpoints do not shut down jobs or claim to delay tab closure', async () => {
  const flush = vi.fn().mockResolvedValue(undefined);
  const prepare = vi.fn();
  const onClosing = vi.fn();
  const onError = vi.fn();
  const dispose = await installBrowserCloseFlush({ flush, prepare, onClosing, onError });
  window.dispatchEvent(new Event('pagehide'));
  await vi.waitFor(() => expect(flush).toHaveBeenCalledOnce());
  flush.mockRejectedValueOnce(new Error('storage full'));
  vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
  document.dispatchEvent(new Event('visibilitychange'));
  await vi.waitFor(() => expect(onError).toHaveBeenCalledWith(expect.stringContaining('storage full')));
  expect(prepare).not.toHaveBeenCalled();
  expect(onClosing).not.toHaveBeenCalled();
  dispose();
  window.dispatchEvent(new Event('pagehide'));
  expect(flush).toHaveBeenCalledTimes(2);
});

function chooseFile(contents: string): void {
  const input = document.querySelector<HTMLInputElement>('input[type=file]')!;
  Object.defineProperty(input, 'files', {
    value: [new File([contents], 'history.json', { type: 'application/json' })],
  });
  input.dispatchEvent(new Event('change'));
}

it('browser file cancellation settles the operation and removes its input', async () => {
  vi.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(() => {});
  const result = browserFiles.openJsonText();
  document.querySelector('input')!.dispatchEvent(new Event('cancel'));
  await expect(result).resolves.toBeNull();
  expect(document.querySelector('input')).toBeNull();
});

it('browser import propagates malformed JSON rather than treating it as cancellation', async () => {
  vi.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(() => {});
  const result = importHistoryPayload();
  const rejected = expect(result).rejects.toThrow();
  chooseFile('not JSON');
  await rejected;
  expect(document.querySelector('input')).toBeNull();
});

it('browser import reports read failures and cleans up its picker', async () => {
  vi.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(() => {});
  vi.spyOn(FileReader.prototype, 'readAsText').mockImplementation(function (this: FileReader) {
    Object.defineProperty(this, 'error', { value: new DOMException('read denied', 'NotReadableError') });
    this.dispatchEvent(new Event('error'));
  });
  const result = browserFiles.openJsonText();
  const rejected = expect(result).rejects.toThrow('read denied');
  chooseFile('{}');
  await rejected;
  expect(document.querySelector('input')).toBeNull();
});
