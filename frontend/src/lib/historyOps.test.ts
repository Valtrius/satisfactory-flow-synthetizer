import { describe, expect, it } from 'vitest';
import { createQueuedEntry, type FormSnapshot, type HistoryEntry } from './historyModel';
import { diffHistoryOps, snapshotHistory } from './historyOps';
import type { SolveRequest } from '../types';
import { historyEntry, solution } from '../test/fixtures';
import contract from '../../../src-tauri/history-fixtures/incremental.json';

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

describe('diffHistoryOps', () => {
  it.each(contract.checkpoints.map((checkpoint, index) => ({ ...checkpoint, index })))(
    '$name matches the operations consumed by the SQLite contract test',
    ({ index, patch, ops }) => {
      let previous = contract.initial as unknown as HistoryEntry;
      for (const step of contract.checkpoints.slice(0, index)) {
        previous = { ...previous, ...step.patch } as HistoryEntry;
      }
      const next = { ...previous, ...patch } as HistoryEntry;
      expect(diffHistoryOps(snapshotHistory([previous], previous.id), snapshotHistory([next], next.id))).toEqual(ops);
    },
  );
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

  it('upserts when the request changes', () => {
    const entry = completed({ id: 'a' });
    const previous = snapshotHistory([entry], 'a');
    const next = snapshotHistory([{ ...entry, request: { ...request, beltRate: '780' } }], 'a');
    expect(diffHistoryOps(previous, next)).toEqual([{ op: 'upsertEntry', entry: next.entries[0] }]);
  });

  it('appends only the new suffix alongside scalar, layout, selection and proof changes', () => {
    const first = historyEntry({ status: 'incomplete', enumerationComplete: false });
    const appended = { ...solution, buildSteps: ['second stored layout'] };
    const next = {
      ...first,
      results: [...first.results, appended],
      updatedAtMs: first.updatedAtMs + 1,
      selectedSourceIndex: 1,
      layouts: { '1': { layoutKey: 'placed', nodes: [], edges: [] } },
      proof: { minimumNodeCount: 1, minimumLinkCount: null },
      sequence: 7,
    };
    const ops = diffHistoryOps(snapshotHistory([first], first.id), snapshotHistory([next], next.id));
    expect(ops.map((op) => op.op)).toEqual(['appendSolutions', 'patchEntry', 'saveLayouts', 'saveSolverState']);
    expect(ops[0]).toEqual({ op: 'appendSolutions', id: first.id, expectedCount: 1, solutions: [appended] });
    expect(ops).toContainEqual({ op: 'saveSolverState', id: first.id, progress: null, proof: next.proof, sequence: 7 });
  });

  it.each(['correction', 'removal', 'reorder', 'terminal proof', 'form'])(
    'uses a full replacement for a %s rather than an unsafe append',
    (kind) => {
      const second = { ...solution, buildSteps: ['second stored layout'] };
      const first = historyEntry({ results: [solution, second] });
      const next = structuredClone(first);
      if (kind === 'correction') next.results[0].stats.linkCount = 9;
      if (kind === 'removal') next.results.pop();
      if (kind === 'reorder') next.results.reverse();
      if (kind === 'terminal proof') next.result = { ...solution, status: 'best_known' };
      if (kind === 'form') next.form.beltRate = '1200';
      const snapshot = snapshotHistory([next], next.id);
      expect(diffHistoryOps(snapshotHistory([first], first.id), snapshot)).toEqual([
        { op: 'upsertEntry', entry: snapshot.entries[0] },
      ]);
    },
  );

  it('replaces a stored singleton before switching to an explicit result list', () => {
    const first = historyEntry({ results: [], result: solution });
    const next = snapshotHistory(
      [{ ...first, results: [solution, { ...solution, buildSteps: ['second'] }] }],
      first.id,
    );
    expect(diffHistoryOps(snapshotHistory([first], first.id), next)).toEqual([
      { op: 'upsertEntry', entry: next.entries[0] },
    ]);
  });

  it('detaches persisted requests and graphs from subsequent in-place UI edits', () => {
    const entry = structuredClone(historyEntry());
    const previous = snapshotHistory([entry], entry.id);
    entry.results[0].nodes[0].label = 'Changed after checkpoint';
    entry.request.outputs[0].rate = '12';
    expect(previous.entries[0].results[0].nodes[0].label).not.toBe(entry.results[0].nodes[0].label);
    expect(previous.entries[0].request.outputs[0].rate).toBe('1');
    expect(diffHistoryOps(previous, snapshotHistory([entry], entry.id))[0].op).toBe('upsertEntry');
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
