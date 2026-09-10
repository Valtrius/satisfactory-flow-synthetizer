import { afterEach, describe, expect, it, vi } from 'vitest';
import { get, writable } from 'svelte/store';
import type { Edge, Node } from '@xyflow/svelte';
import type { Solution } from '../types';
import { createGraphSession } from './graphSession';
import { createQueuedEntry, type HistoryEntry } from './historyModel';
import { layoutSolution, type FlowGraph } from './graph';

vi.mock('./graph', async (original) => ({ ...(await original<typeof import('./graph')>()), layoutSolution: vi.fn() }));
afterEach(() => vi.resetAllMocks());

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

const rate = { exact: '1', decimal: '1' };
const solution: Solution = {
  status: 'proven_optimal',
  modelVersion: 1,
  stats: { nodeCount: 0, linkCount: 0, splitters: 0, mergers: 0, feedbackLoops: 0 },
  totalInput: rate,
  totalOutput: rate,
  discardRate: { exact: '0', decimal: '0' },
  beltRate: rate,
  nodes: [],
  edges: [],
  buildSteps: [],
};
const layout = (id: string): FlowGraph => ({ nodes: [{ id, position: { x: 0, y: 0 }, data: {} }], edges: [] });

function setup() {
  const request = {
    inputs: [],
    outputs: [{ id: 'o', name: '', rate: '1' }],
    beltRate: '1',
    solveMode: 'one_min_nl' as const,
  };
  let entry: HistoryEntry = {
    ...createQueuedEntry({ ...request, outputs: [{ ...request.outputs[0], multiplier: '1' }] }, request),
    id: 'A',
    result: solution,
  };
  let selected = 0;
  let current: Solution | null = solution;
  let solutions: Solution[] = [];
  const nodes = writable<Node[]>([]),
    edges = writable<Edge[]>([]);
  const patchEntry = vi.fn();
  const setError = vi.fn();
  const session = createGraphSession({
    nodes,
    edges,
    getSelectedEntryId: () => entry.id,
    getSelectedEntry: () => entry,
    getSelectedSourceIndex: () => selected,
    setSelectedSourceIndex: (x) => {
      selected = x;
    },
    getSolution: () => current,
    setSolution: (x) => {
      current = x;
    },
    getSolutions: () => solutions,
    setSolutions: (x) => {
      solutions = x;
    },
    getSortColumns: () => [],
    setSortColumns: () => {},
    getInputs: () => [],
    getOutputs: () => [],
    patchEntry,
    setError,
  });
  return {
    session,
    nodes,
    patchEntry,
    setError,
    select: (id: string, result: Solution | null) => {
      entry = { ...entry, id, result };
      return session.hydrateFromEntry(entry);
    },
  };
}

describe('graph session ownership', () => {
  it('protects loaded nodes from topology changes while allowing movement', async () => {
    const state = setup();
    vi.mocked(layoutSolution).mockResolvedValue(layout('A'));
    await state.session.applySolution(solution);
    expect(get(state.nodes)[0]).toMatchObject({ deletable: false, connectable: false });
    const before = get(state.nodes)[0].position;
    state.session.onNodeDragStart();
    state.nodes.update((nodes) => nodes.map((node) => ({ ...node, position: { x: 24, y: 48 } })));
    state.session.onNodeDragStop();
    expect(state.session.canUndo).toBe(true);
    state.session.undo();
    expect(get(state.nodes)[0].position).toEqual(before);
  });
  it('discards a layout completed after selecting an empty history entry', async () => {
    const state = setup(),
      pending = deferred<FlowGraph>();
    vi.mocked(layoutSolution).mockReturnValueOnce(pending.promise);
    const apply = state.session.applySolution(solution);
    await state.select('B', null);
    pending.resolve(layout('A'));
    await apply;
    expect(get(state.nodes)).toEqual([]);
    expect(state.patchEntry).not.toHaveBeenCalled();
  });

  it.each(['resolve', 'reject'] as const)(
    'ignores a stale reset %s after another entry is selected',
    async (finish) => {
      const state = setup(),
        pending = deferred<FlowGraph>();
      vi.mocked(layoutSolution)
        .mockResolvedValueOnce(layout('A'))
        .mockReturnValueOnce(pending.promise)
        .mockResolvedValueOnce(layout('B'));
      await state.session.applySolution(solution);
      const reset = state.session.resetLayout();
      await state.select('B', solution);
      state.patchEntry.mockClear();
      if (finish === 'resolve') pending.resolve(layout('old-reset'));
      else pending.reject(new Error('old failure'));
      await reset;
      expect(get(state.nodes)[0].id).toBe('B');
      expect(state.patchEntry).not.toHaveBeenCalled();
      expect(state.setError).not.toHaveBeenCalled();
    },
  );

  it('clears undo when selecting an empty entry', async () => {
    const state = setup();
    vi.mocked(layoutSolution).mockResolvedValue(layout('A'));
    await state.session.applySolution(solution);
    state.session.rotate('cw');
    expect(state.session.canUndo).toBe(true);
    await state.select('B', null);
    state.session.undo();
    expect(state.session.canUndo).toBe(false);
    expect(get(state.nodes)).toEqual([]);
  });
});
