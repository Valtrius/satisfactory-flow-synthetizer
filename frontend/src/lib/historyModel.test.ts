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
  solveMode: 'all_min_n',
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
  solveMode: 'all_min_n',
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
    expect(entryOutcomeLine(entry)).toBe('All layouts · N=5 · 2');
  });

  it('labels single layout and cancelled', () => {
    const single = completed({
      id: 'b',
      enumerationComplete: true,
      request: { ...request, solveMode: 'one_min_nl' },
      result: {
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
    expect(entryOutcomeLine(single)).toBe('Optimal · N=4');

    const cancelled = completed({
      id: 'c',
      status: 'cancelled',
      enumerationComplete: false,
      results: [single.result!],
    });
    expect(entryOutcomeLine(cancelled)).toBe('Cancelled · 1 layout · N=4');
  });
});

describe('entryHistoryMetrics', () => {
  it('summarizes search, engine, nodes, and layouts', () => {
    const entry = completed({
      id: 'metrics',
      enumerationComplete: true,
      results: [
        {
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
      search: { value: 'All min N', tip: 'Search: All min N' },
      belts: { value: 'L=—', tip: 'Minimum operator belt count not yet proved' },
      nodes: { value: 'N=5', tip: 'Node count N = 5' },
      layouts: { value: '2', tip: '2 layouts found' },
    });
    expect(entryStatusCaption(entry)).toBe('Completed');
  });

  it('labels a finished All min N/L search Completed even when layouts stay best_known', () => {
    const layout = {
      engine: 'custom' as const,
      status: 'best_known' as const,
      modelVersion: 1,
      stats: {
        nodeCount: 4,
        splitters: 1,
        mergers: 1,
        feedbackLoops: 0,
        linkCount: 3,
        checkedThrough: 4,
      },
      totalInput: { exact: '120', decimal: '120' },
      totalOutput: { exact: '120', decimal: '120' },
      discardRate: { exact: '0', decimal: '0' },
      beltRate: { exact: '1200', decimal: '1200' },
      nodes: [],
      edges: [],
      buildSteps: [],
    };
    const entry = completed({
      id: 'min-l',
      request: { ...request, solveMode: 'all_min_nl' },
      form: { ...form, solveMode: 'all_min_nl' },
      enumerationComplete: true,
      result: layout,
      results: [layout, { ...layout, stats: { ...layout.stats, feedbackLoops: 1 } }],
    });
    expect(entryStatusCaption(entry)).toBe('Completed');
  });

  it('keeps Best known for a single-layout Opt that never proved optimality', () => {
    const entry = completed({
      id: 'opt-best',
      request: { ...request, solveMode: 'one_min_nl' },
      form: { ...form, solveMode: 'one_min_nl' },
      enumerationComplete: false,
      result: {
        status: 'best_known',
        modelVersion: 1,
        stats: {
          nodeCount: 4,
          splitters: 1,
          mergers: 1,
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
    expect(entryStatusCaption(entry)).toBe('Best known');
  });

  it('uses placeholders for queued jobs', () => {
    const entry = createQueuedEntry({ ...form, solveMode: 'one_min_nl' }, { ...request, solveMode: 'one_min_nl' });
    expect(entryHistoryMetrics(entry)).toEqual({
      search: { value: 'One min N/L', tip: 'Search: One min N/L' },
      belts: { value: 'L=—', tip: 'Minimum operator belt count not yet proved' },
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
    phase: 'searching',
    elapsedMs: 100,
    nodeCount: 2,
    linkConstraint: { kind: 'exact', value: 1 },
    nodeLowerBound: 2,
    bestNodeCount: 2,
    bestLinkCount: 2,
    solutionsFound: 0,
    custom: [
      {
        name: 'solver.profiles_exhausted',
        label: 'Profiles',
        value: { type: 'integer', value: '18446744073709551615' },
        unit: null,
      },
    ],
  };
  const proof = { minimumNodeCount: 2, minimumLinkCount: null };
  it('normalizes persisted diagnostic names without losing their exact values', () => {
    const entry = completed({
      id: 'diagnostic-import',
      progress: {
        ...progress,
        custom: [{ ...progress.custom[0], name: 'astra.profiles_exhausted', label: 'Completed Astra profiles' }],
      },
    });
    const [loaded] = parseHistoryDocument({ entries: [entry] }).entries;
    expect(loaded.progress?.custom[0]).toEqual({ ...progress.custom[0], label: 'Completed Solver profiles' });
    expect(JSON.stringify(persistableEntries([loaded]))).not.toMatch(/astra/i);
  });
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
  it('ignores unsupported or incomplete telemetry without discarding the saved graph', () => {
    const graph = {
      status: 'best_known',
      nodes: [{ id: 'kept' }],
      edges: [],
    };
    for (const unsupportedProgress of [
      { engine: 'custom', phase: 'searching', instrumentation: {} },
      { engine: 'z3', kind: 'checking', nodeCount: 2 },
      { phase: 'searching', custom: [] },
      { ...progress, custom: [{ name: 'broken', label: 'Missing value' }] },
    ]) {
      const [loaded] = parseHistoryDocument({
        entries: [{ ...completed({ id: 'unsupported' }), progress: unsupportedProgress, result: graph }],
      }).entries;
      expect(loaded.progress).toBeNull();
      expect(loaded.result).toEqual(graph);
    }
  });
});

it('migrates version 2 result scopes and solver fields while preserving edited graph positions', () => {
  for (const [storedMode, mode] of [
    ['optimal', 'one_min_nl'],
    ['all_at_minimum_nodes_and_minimum_links', 'all_min_nl'],
    ['all_at_minimum_nodes', 'all_min_n'],
  ]) {
    const original = completed({ id: 'stored' });
    const stored = {
      ...original,
      request: { ...original.request, solveMode: storedMode, engine: 'z3' },
      form: { ...original.form, solveMode: storedMode, engine: 'custom' },
      layouts: {
        '0': { layoutKey: 'z3::2::1', nodes: [{ id: 'node', position: { x: 144, y: 72 }, data: {} }], edges: [] },
      },
    };
    const [loaded] = parseHistoryDocument({ version: 2, entries: [stored] }).entries;
    expect(loaded.request.solveMode).toBe(mode);
    expect(loaded.form.solveMode).toBe(mode);
    expect(JSON.stringify(loaded)).not.toContain('"engine"');
    expect(loaded.layouts['0'].layoutKey).toBe('2::1');
    expect(loaded.layouts['0'].nodes[0].position).toEqual({ x: 144, y: 72 });
  }
});

it('shows a minimum belt count only when proved', () => {
  const entry = completed({ id: 'proved', proof: { minimumNodeCount: 4, minimumLinkCount: 3 } });
  expect(entryHistoryMetrics(entry).belts.value).toBe('L=3');
  expect(entryHistoryMetrics({ ...entry, proof: null, result: null, results: [] }).belts.value).toBe('L=—');
});
