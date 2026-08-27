import { describe, expect, it } from 'vitest';
import type { SolveRequest, SolverProgress } from '../types';
import {
  createQueuedEntry,
  defaultTitle,
  entryElapsedMs,
  entryHistoryMetrics,
  entryOutcomeLine,
  entryStatusCaption,
  mergeImportedPayload,
  parseHistoryDocument,
  persistableEntries,
  partitionEntries,
  reorderWithinBand,
  type FormSnapshot,
  type HistoryEntry,
} from './historyModel';

const form: FormSnapshot = {
  inputs: [{ id: 'in-1', name: '', rate: '60', multiplier: '2' }],
  outputs: [
    { id: 'out-1', name: '', rate: '40', multiplier: '1' },
    { id: 'out-2', name: '', rate: '40', multiplier: '1' },
    { id: 'out-3', name: '', rate: '40', multiplier: '1' },
  ],
  beltRate: '1200',
  enumerateAllAtN: true,
  engine: 'custom',
};

const request: SolveRequest = {
  inputs: [
    { id: 'in-1-1', name: '', rate: '60' },
    { id: 'in-1-2', name: '', rate: '60' },
  ],
  outputs: [
    { id: 'out-1', name: '', rate: '40' },
    { id: 'out-2', name: '', rate: '40' },
    { id: 'out-3', name: '', rate: '40' },
  ],
  beltRate: '1200',
  enumerateAllAtN: true,
  engine: 'custom',
};

function completed(partial: Partial<HistoryEntry> & Pick<HistoryEntry, 'id'>): HistoryEntry {
  const base = createQueuedEntry(form, request);
  return {
    ...base,
    ...partial,
    status: partial.status ?? 'completed',
    enumerationComplete: partial.enumerationComplete ?? true,
    result: partial.result ?? null,
    results: partial.results ?? [],
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
            checkedThrough: 5,
          },
          totalInput: { exact: '120', decimal: '120' },
          totalOutput: { exact: '120', decimal: '120' },
          discardRate: { exact: '0', decimal: '0' },
          beltRate: { exact: '1200', decimal: '1200' },
          nodes: [],
          edges: [],
          buildSteps: [],
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
            checkedThrough: 5,
          },
          totalInput: { exact: '120', decimal: '120' },
          totalOutput: { exact: '120', decimal: '120' },
          discardRate: { exact: '0', decimal: '0' },
          beltRate: { exact: '1200', decimal: '1200' },
          nodes: [],
          edges: [],
          buildSteps: [],
        },
      ],
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
          checkedThrough: 4,
        },
        totalInput: { exact: '60', decimal: '60' },
        totalOutput: { exact: '60', decimal: '60' },
        discardRate: { exact: '0', decimal: '0' },
        beltRate: { exact: '1200', decimal: '1200' },
        nodes: [],
        edges: [],
        buildSteps: [],
      },
    });
    expect(entryOutcomeLine(single)).toBe('Optimal · N=4 · Custom');

    const cancelled = completed({
      id: 'c',
      status: 'cancelled',
      enumerationComplete: false,
      results: [single.result!],
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

describe('entryHistoryMetrics', () => {
  it('summarizes search, engine, nodes, and layouts', () => {
    const entry = completed({
      id: 'metrics',
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
            checkedThrough: 5,
          },
          totalInput: { exact: '120', decimal: '120' },
          totalOutput: { exact: '120', decimal: '120' },
          discardRate: { exact: '0', decimal: '0' },
          beltRate: { exact: '1200', decimal: '1200' },
          nodes: [],
          edges: [],
          buildSteps: [],
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
            checkedThrough: 5,
          },
          totalInput: { exact: '120', decimal: '120' },
          totalOutput: { exact: '120', decimal: '120' },
          discardRate: { exact: '0', decimal: '0' },
          beltRate: { exact: '1200', decimal: '1200' },
          nodes: [],
          edges: [],
          buildSteps: [],
        },
      ],
    });
    expect(entryHistoryMetrics(entry)).toEqual({
      search: { value: 'All', tip: 'Search: Find all layouts at N' },
      engine: { value: 'Custom', tip: 'Engine: Custom' },
      nodes: { value: 'N=5', tip: 'Node count N = 5' },
      layouts: { value: '2', tip: '2 layouts found' },
    });
    expect(entryStatusCaption(entry)).toBe('Best known');
  });

  it('uses placeholders for queued jobs', () => {
    const entry = createQueuedEntry(
      { ...form, enumerateAllAtN: false, engine: 'z3' },
      { ...request, enumerateAllAtN: false, engine: 'z3' },
    );
    expect(entryHistoryMetrics(entry)).toEqual({
      search: { value: 'Opt', tip: 'Search: Find optimal layout' },
      engine: { value: 'Z3', tip: 'Engine: Z3' },
      nodes: { value: 'N=—', tip: 'Node count unknown until solved' },
      layouts: { value: '0', tip: 'No layouts yet' },
    });
    expect(entryStatusCaption(entry)).toBe('Queued');
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
      entry: completed({ id: 'incoming', title: 'Friend share' }),
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
      updatedAtMs: 9000,
    });
    expect(entryElapsedMs(entry)).toBe(1500);
  });

  it('uses live clock while running', () => {
    const entry = {
      ...createQueuedEntry(form, request),
      status: 'running' as const,
      startedAtMs: 1000,
      finishedAtMs: null,
    };
    expect(entryElapsedMs(entry, 1600)).toBe(600);
  });
});

describe('saved common progress', () => {
  const progress: SolverProgress = {
    phase: 'optimizing_links',
    elapsedMs: 100,
    nodeCount: 2,
    linkConstraint: { kind: 'at_most', value: 1 },
    nodeLowerBound: 2,
    bestNodeCount: 2,
    bestLinkCount: 2,
    solutionsFound: 0,
    custom: [
      {
        name: 'z3.profiles_total',
        label: 'Profiles',
        value: { type: 'integer', value: '18446744073709551615' },
        unit: null,
      },
    ],
  };
  const proof = { minimumNodeCount: 2, minimumLinkCount: null };
  it('retains progress, proof, and sequence across save/load and import', () => {
    const entry = completed({
      id: 'entry',
      status: 'cancelled',
      progress,
      proof,
      sequence: 8,
    });
    const saved = persistableEntries([entry]);
    const [loaded] = parseHistoryDocument(JSON.parse(JSON.stringify({ entries: saved }))).entries;
    expect(loaded.progress).toEqual(progress);
    expect(loaded.proof).toEqual(proof);
    expect(loaded.sequence).toBe(8);
    const imported = mergeImportedPayload([], {
      kind: 'history-entry',
      entry: loaded,
    }).entries[0];
    expect(imported.progress).toEqual(progress);
    expect(imported.proof).toEqual(proof);
  });
  it('ignores old or incomplete telemetry without discarding the saved graph', () => {
    const graph = {
      engine: 'z3',
      status: 'best_known',
      nodes: [{ id: 'kept' }],
      edges: [],
    };
    for (const oldProgress of [
      { engine: 'custom', phase: 'searching', instrumentation: {} },
      { engine: 'z3', kind: 'checking', nodeCount: 2 },
      { phase: 'searching', custom: [] },
      { ...progress, custom: [{ name: 'broken', label: 'Missing value' }] },
    ]) {
      const [loaded] = parseHistoryDocument({
        entries: [{ ...completed({ id: 'old' }), progress: oldProgress, result: graph }],
      }).entries;
      expect(loaded.progress).toBeNull();
      expect(loaded.result).toEqual(graph);
    }
  });
});
