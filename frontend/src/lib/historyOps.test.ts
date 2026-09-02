import { describe, expect, it } from 'vitest';
import { createQueuedEntry, type FormSnapshot, type HistoryEntry } from './historyModel';
import { diffHistoryOps, snapshotHistory } from './historyOps';
import type { SolveRequest } from '../types';

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

describe('diffHistoryOps', () => {
  it('inserts a new persistable entry and selects it', () => {
    const entry = completed({ id: 'a' });
    const next = snapshotHistory([entry], 'a');
    expect(diffHistoryOps(null, next)).toEqual([
      { op: 'upsertEntry', entry: next.entries[0] },
      { op: 'reorderEntries', ids: ['a'] },
      { op: 'setSelected', id: 'a' },
    ]);
  });

  it('returns no ops when the snapshot is unchanged', () => {
    const snapshot = snapshotHistory([completed({ id: 'a' })], 'a');
    expect(diffHistoryOps(snapshot, snapshot)).toEqual([]);
  });

  it('patches scalars without rewriting the graph', () => {
    const entry = completed({ id: 'a', title: null });
    const previous = snapshotHistory([entry], 'a');
    const next = snapshotHistory([{ ...entry, title: 'Named', updatedAtMs: 9 }], 'a');
    expect(diffHistoryOps(previous, next)).toEqual([
      {
        op: 'patchEntry',
        id: 'a',
        fields: {
          title: 'Named',
          status: 'completed',
          createdAtMs: entry.createdAtMs,
          updatedAtMs: 9,
          startedAtMs: null,
          finishedAtMs: null,
          enumerationComplete: false,
          error: null,
          selectedSourceIndex: 0,
        },
      },
    ]);
  });

  it('saves layout-only changes', () => {
    const entry = completed({ id: 'a', layouts: {} });
    const previous = snapshotHistory([entry], 'a');
    const layouts = { '0': { layoutKey: 'k', nodes: [], edges: [] } };
    const next = snapshotHistory([{ ...entry, layouts }], 'a');
    expect(diffHistoryOps(previous, next)).toEqual([{ op: 'saveLayouts', id: 'a', layouts }]);
  });

  it('deletes missing ids and reorders the rest', () => {
    const a = completed({ id: 'a' });
    const b = completed({ id: 'b' });
    const c = completed({ id: 'c' });
    const previous = snapshotHistory([a, b, c], 'a');
    const next = snapshotHistory([c, b], 'b');
    expect(diffHistoryOps(previous, next)).toEqual([
      { op: 'deleteEntries', ids: ['a'] },
      { op: 'reorderEntries', ids: ['c', 'b'] },
      { op: 'setSelected', id: 'b' },
    ]);
  });

  it('upserts when request or results change', () => {
    const entry = completed({ id: 'a' });
    const previous = snapshotHistory([entry], 'a');
    const next = snapshotHistory([{ ...entry, request: { ...request, beltRate: '780' } }], 'a');
    expect(diffHistoryOps(previous, next)).toEqual([{ op: 'upsertEntry', entry: next.entries[0] }]);
  });

  it('does not persist queued jobs until they become history', () => {
    const queued = createQueuedEntry(form, request);
    const idle = snapshotHistory([queued], queued.id);
    expect(idle.entries).toEqual([]);
    const terminal = completed({ id: queued.id, status: 'cancelled' });
    const next = snapshotHistory([queued, terminal], queued.id);
    expect(diffHistoryOps(idle, next)).toEqual([
      { op: 'upsertEntry', entry: next.entries[0] },
      { op: 'reorderEntries', ids: [queued.id] },
      { op: 'setSelected', id: queued.id },
    ]);
  });
});
