import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { createQueuedEntry, type FormSnapshot, type HistoryEntry } from './historyModel';
import { applyHistoryChanges, rememberPersistedHistory, resetHistoryPersistForTests } from './historyPersist';
import type { SolveRequest } from '../types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  isTauri: () => true,
}));

const form: FormSnapshot = {
  inputs: [],
  outputs: [{ id: 'o1', name: '', rate: '60', multiplier: '1' }],
  beltRate: '1200',
  solveMode: 'optimal',
  engine: 'custom',
};

const request: SolveRequest = {
  inputs: [],
  outputs: [{ id: 'o1', name: '', rate: '60' }],
  beltRate: '1200',
  solveMode: 'optimal',
  engine: 'custom',
};

function completed(partial: Partial<HistoryEntry> & Pick<HistoryEntry, 'id'>): HistoryEntry {
  const base = createQueuedEntry(form, request);
  return {
    ...base,
    ...partial,
    status: partial.status ?? 'completed',
    jobId: null,
  };
}

function deferred(): { promise: Promise<void>; resolve: () => void; reject: (error: unknown) => void } {
  let resolve!: () => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<void>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

afterEach(() => {
  resetHistoryPersistForTests();
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockResolvedValue(undefined);
});

describe('applyHistoryChanges', () => {
  it('serializes overlapping applies so a stale batch cannot finish last', async () => {
    const first = completed({ id: 'a', title: 'one' });
    const second = { ...first, title: 'two', updatedAtMs: first.updatedAtMs + 1 };
    rememberPersistedHistory([], null);

    const blocked = deferred();
    vi.mocked(invoke).mockImplementationOnce(() => blocked.promise);

    const applyFirst = applyHistoryChanges([first], 'a');
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));

    const applySecond = applyHistoryChanges([second], 'a');
    expect(invoke).toHaveBeenCalledTimes(1);

    blocked.resolve();
    await Promise.all([applyFirst, applySecond]);
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(vi.mocked(invoke).mock.calls[1][1]).toEqual({
      ops: [
        {
          op: 'patchEntry',
          id: 'a',
          fields: expect.objectContaining({ title: 'two' }),
        },
      ],
    });

    vi.mocked(invoke).mockClear();
    await applyHistoryChanges([second], 'a');
    expect(invoke).not.toHaveBeenCalled();
  });

  it('retries the latest coalesced snapshot if an in-flight apply fails', async () => {
    const first = completed({ id: 'a', title: 'one' });
    const second = { ...first, title: 'two', updatedAtMs: first.updatedAtMs + 1 };
    rememberPersistedHistory([], null);

    const blocked = deferred();
    vi.mocked(invoke).mockImplementationOnce(() => blocked.promise);
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    const applyFirst = applyHistoryChanges([first], 'a');
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
    const applySecond = applyHistoryChanges([second], 'a');

    blocked.reject(new Error('ipc failed'));
    await expect(applyFirst).rejects.toThrow('ipc failed');
    await applySecond;

    expect(invoke).toHaveBeenCalledTimes(2);
    const retryOps = vi.mocked(invoke).mock.calls[1][1] as { ops: Array<{ op: string; entry?: HistoryEntry }> };
    expect(retryOps.ops.some((op) => op.op === 'upsertEntry' && op.entry?.title === 'two')).toBe(true);
  });
});
