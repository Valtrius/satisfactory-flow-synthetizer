import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { createQueuedEntry, type FormSnapshot, type HistoryEntry } from './historyModel';
import { applyHistoryChanges, rememberPersistedHistory, resetHistoryPersistForTests } from './historyPersist';
import type { SolveRequest } from '../types';
import { historyEntry, solution } from '../test/fixtures';
import type { HistoryOp } from './historyOps';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  isTauri: () => true,
}));

const form: FormSnapshot = {
  inputs: [],
  outputs: [{ id: 'o1', name: '', rate: '60', multiplier: '1' }],
  beltRate: '1200',
  solveMode: 'one_min_nl',
};

const request: SolveRequest = {
  inputs: [],
  outputs: [{ id: 'o1', name: '', rate: '60' }],
  beltRate: '1200',
  solveMode: 'one_min_nl',
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
  it('recovers a lost append acknowledgement with a full write of only the affected entry', async () => {
    const first = historyEntry();
    const other = historyEntry({ id: 'other' });
    const second = { ...first, results: [...first.results, { ...solution, buildSteps: ['second'] }] };
    rememberPersistedHistory([first, other], first.id);
    vi.mocked(invoke).mockRejectedValueOnce(`history_append_prefix_mismatch: ${first.id}`);
    await applyHistoryChanges([second, other], other.id);
    expect(invoke).toHaveBeenCalledTimes(2);
    const firstOps = (vi.mocked(invoke).mock.calls[0][1] as { ops: HistoryOp[] }).ops;
    const retryOps = (vi.mocked(invoke).mock.calls[1][1] as { ops: HistoryOp[] }).ops;
    expect(firstOps[0].op).toBe('appendSolutions');
    expect(retryOps.map((op) => op.op)).toEqual(['upsertEntry', 'setSelected']);
    expect(retryOps[0]).toMatchObject({ entry: { id: first.id, results: second.results } });
    vi.mocked(invoke).mockClear();
    await applyHistoryChanges([second, other], other.id);
    expect(invoke).not.toHaveBeenCalled();
  });

  it('keeps a failed recovery pending and retries without advancing the committed prefix', async () => {
    const first = historyEntry();
    const next = { ...first, results: [...first.results, { ...solution, buildSteps: ['second'] }] };
    rememberPersistedHistory([first], first.id);
    vi.mocked(invoke)
      .mockRejectedValueOnce(`history_append_prefix_mismatch: ${first.id}`)
      .mockRejectedValueOnce('disk full');
    await expect(applyHistoryChanges([next], next.id)).rejects.toBe('disk full');
    await applyHistoryChanges([next], next.id);
    const retryOps = (vi.mocked(invoke).mock.calls[2][1] as { ops: HistoryOp[] }).ops;
    expect(retryOps[0]).toMatchObject({ op: 'appendSolutions', expectedCount: 1, solutions: [next.results[1]] });
  });

  it('does not hide unrelated database errors behind a replacement', async () => {
    const first = historyEntry();
    rememberPersistedHistory([first], first.id);
    vi.mocked(invoke).mockRejectedValueOnce('database locked');
    await expect(applyHistoryChanges([{ ...first, results: [...first.results, solution] }], first.id)).rejects.toBe(
      'database locked',
    );
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('diffs a coalesced append against the last acknowledged prefix', async () => {
    const first = historyEntry();
    const second = { ...first, results: [...first.results, { ...solution, buildSteps: ['second'] }] };
    const third = { ...second, results: [...second.results, { ...solution, buildSteps: ['third'] }] };
    rememberPersistedHistory([first], first.id);
    const blocked = deferred();
    vi.mocked(invoke).mockImplementationOnce(() => blocked.promise);
    const a = applyHistoryChanges([second], first.id);
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
    const b = applyHistoryChanges([third], first.id);
    blocked.resolve();
    await Promise.all([a, b]);
    expect((vi.mocked(invoke).mock.calls[1][1] as { ops: HistoryOp[] }).ops[0]).toMatchObject({
      op: 'appendSolutions',
      expectedCount: 2,
      solutions: [third.results[2]],
    });
  });
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
