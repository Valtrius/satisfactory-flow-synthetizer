import { describe, expect, it } from 'vitest';
import type { SolveRequest } from '../types';
import {
  createQueuedEntry,
  defaultTitle,
  entryElapsedMs,
  entryOutcomeLine,
  mergeImportedPayload,
  partitionEntries,
  reorderWithinBand,
  type FormSnapshot,
  type HistoryEntry
} from './historyModel';

const form: FormSnapshot = {
  inputs: [{ id: 'in-1', name: '', rate: '60', multiplier: '2' }],
  outputs: [
    { id: 'out-1', name: '', rate: '40', multiplier: '1' },
    { id: 'out-2', name: '', rate: '40', multiplier: '1' },
    { id: 'out-3', name: '', rate: '40', multiplier: '1' }
  ],
  beltRate: '1200',
  enumerateAllAtN: true,
  engine: 'custom'
};

const request: SolveRequest = {
  inputs: [
    { id: 'in-1-1', name: '', rate: '60' },
    { id: 'in-1-2', name: '', rate: '60' }
  ],
  outputs: [
    { id: 'out-1', name: '', rate: '40' },
    { id: 'out-2', name: '', rate: '40' },
    { id: 'out-3', name: '', rate: '40' }
  ],
  beltRate: '1200',
  enumerateAllAtN: true,
  engine: 'custom'
};

function completed(partial: Partial<HistoryEntry> & Pick<HistoryEntry, 'id'>): HistoryEntry {
  const base = createQueuedEntry(form, request);
  return {
    ...base,
    ...partial,
    status: partial.status ?? 'completed',
    enumerationComplete: partial.enumerationComplete ?? true,
    result: partial.result ?? null,
    results: partial.results ?? []
  };
}

describe('defaultTitle', () => {
  it('joins expanded rates', () => {
    expect(defaultTitle(request)).toBe('60 + 60 = 40 + 40 + 40');
  });
});

describe('entryOutcomeLine', () => {
  it('labels full enumeration', () => {
    const entry = completed({
      id: 'a',
      enumerationComplete: true,
      results: [
        {
          engine: 'custom',
          status: 'best_known',
          modelVersion: 1,
          stats: {
            nodeCount: 5,
            splitters: 1,
            mergers: 1,
            feedbackLoops: 0,
            linkCount: 4,
            checkedThrough: 5
          },
          totalInput: { exact: '120', decimal: '120' },
          totalOutput: { exact: '120', decimal: '120' },
          discardRate: { exact: '0', decimal: '0' },
          beltRate: { exact: '1200', decimal: '1200' },
          nodes: [],
          edges: [],
          buildSteps: []
        },
        {
          engine: 'custom',
          status: 'best_known',
          modelVersion: 1,
          stats: {
            nodeCount: 5,
            splitters: 1,
            mergers: 1,
            feedbackLoops: 0,
            linkCount: 5,
            checkedThrough: 5
          },
          totalInput: { exact: '120', decimal: '120' },
          totalOutput: { exact: '120', decimal: '120' },
          discardRate: { exact: '0', decimal: '0' },
          beltRate: { exact: '1200', decimal: '1200' },
          nodes: [],
          edges: [],
          buildSteps: []
        }
      ]
    });
    expect(entryOutcomeLine(entry)).toBe('All layouts · N=5 · 2 · Custom');
  });

  it('labels single layout and cancelled', () => {
    const single = completed({
      id: 'b',
      enumerationComplete: true,
      request: { ...request, enumerateAllAtN: false },
      result: {
        engine: 'custom',
        status: 'proven_optimal',
        modelVersion: 1,
        stats: {
          nodeCount: 4,
          splitters: 1,
          mergers: 0,
          feedbackLoops: 0,
          linkCount: 3,
          checkedThrough: 4
        },
        totalInput: { exact: '60', decimal: '60' },
        totalOutput: { exact: '60', decimal: '60' },
        discardRate: { exact: '0', decimal: '0' },
        beltRate: { exact: '1200', decimal: '1200' },
        nodes: [],
        edges: [],
        buildSteps: []
      }
    });
    expect(entryOutcomeLine(single)).toBe('Optimal · N=4 · Custom');

    const cancelled = completed({
      id: 'c',
      status: 'cancelled',
      enumerationComplete: false,
      results: [single.result!]
    });
    expect(entryOutcomeLine(cancelled)).toBe('Cancelled · 1 layout · N=4');
  });

  it('persists engine on queued entries', () => {
    const z3Form = { ...form, engine: 'z3' as const };
    const z3Request = { ...request, engine: 'z3' as const };
    const entry = createQueuedEntry(z3Form, z3Request);
    expect(entry.form.engine).toBe('z3');
    expect(entry.request.engine).toBe('z3');
  });
});

describe('reorderWithinBand', () => {
  it('reorders queued without moving history', () => {
    const q1 = createQueuedEntry(form, request);
    const q2 = createQueuedEntry(form, request);
    const h1 = completed({ id: 'h1' });
    const entries = [q1, q2, h1];
    const next = reorderWithinBand(entries, 'queued', q2.id, q1.id);
    const parts = partitionEntries(next);
    expect(parts.queued.map((entry) => entry.id)).toEqual([q2.id, q1.id]);
    expect(parts.history.map((entry) => entry.id)).toEqual(['h1']);
  });
});

describe('mergeImportedPayload', () => {
  it('appends remapped ids', () => {
    const existing = completed({ id: 'keep' });
    const payload = {
      kind: 'history-entry',
      version: 1,
      entry: completed({ id: 'incoming', title: 'Friend share' })
    };
    const { entries, importedIds } = mergeImportedPayload([existing], payload);
    expect(entries).toHaveLength(2);
    expect(entries[0].id).toBe('keep');
    expect(importedIds).toHaveLength(1);
    expect(importedIds[0]).not.toBe('incoming');
    expect(entries[1].title).toBe('Friend share');
  });
});

describe('entryElapsedMs', () => {
  it('freezes on finishedAtMs for terminal entries', () => {
    const entry = completed({
      id: 'a',
      startedAtMs: 1000,
      finishedAtMs: 2500,
      updatedAtMs: 9000
    });
    expect(entryElapsedMs(entry)).toBe(1500);
  });

  it('uses live clock while running', () => {
    const entry = {
      ...createQueuedEntry(form, request),
      status: 'running' as const,
      startedAtMs: 1000,
      finishedAtMs: null
    };
    expect(entryElapsedMs(entry, 1600)).toBe(600);
  });
});
