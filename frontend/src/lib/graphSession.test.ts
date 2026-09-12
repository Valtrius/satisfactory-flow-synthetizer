import { afterEach, describe, expect, it, vi } from 'vitest';
import { get, writable } from 'svelte/store';
import type { Edge, Node } from '@xyflow/svelte';
import type { Solution, SolveRequest } from '../types';
import { saveSvgFile } from './exportSvg';
import { createGraphSession } from './graphSession';
import { createQueuedEntry, type HistoryEntry } from './historyModel';
import { layoutSolution, type FlowGraph } from './graph';

vi.mock('./graph', async (original) => ({ ...(await original<typeof import('./graph')>()), layoutSolution: vi.fn() }));
vi.mock('./exportSvg', async (original) => ({
  ...(await original<typeof import('./exportSvg')>()),
  graphToSvg: () => '<svg/>',
  saveSvgFile: vi.fn(),
}));
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

function setup(loadSolution?: (entry: HistoryEntry, index: number) => Promise<Solution>) {
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
  const patchEntry = vi.fn((id: string, patch: Partial<HistoryEntry>) => {
    if (id === entry.id) entry = { ...entry, ...patch };
  });
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
    patchEntry,
    setError,
    loadSolution,
  });
  return {
    session,
    nodes,
    patchEntry,
    setError,
    getEntry: () => entry,
    getSolution: () => current,
    getSelected: () => selected,
    setPaged: (count: number) => {
      entry = { ...entry, collection: count ? { version: 1, id: 'pages', count, preferredIndex: null } : undefined };
    },
    setRequest: (request: SolveRequest) => {
      entry = { ...entry, request };
    },
    select: (id: string, result: Solution | null) => {
      entry = { ...entry, id, result, layouts: id === entry.id ? entry.layouts : {} };
      return session.hydrateFromEntry(entry);
    },
  };
}

describe('graph session ownership', () => {
  it.each(['read', 'layout'] as const)(
    'keeps the committed selection and graph when a paged %s fails',
    async (failure) => {
      const pending = deferred<Solution>();
      const load = vi.fn().mockResolvedValueOnce(solution).mockReturnValueOnce(pending.promise);
      const state = setup(load);
      state.setPaged(2);
      vi.mocked(layoutSolution).mockResolvedValueOnce(layout('committed'));
      await state.session.selectSolution(0);
      state.session.rotate('cw');
      const before = get(state.nodes);
      const selection = state.session.selectSolution(1);
      expect(state.getSelected()).toBe(0);
      expect(state.getEntry().selectedSourceIndex).toBe(0);
      expect(state.getSolution()).toBe(solution);
      if (failure === 'read') pending.reject(new Error('unreadable collection'));
      else {
        vi.mocked(layoutSolution).mockRejectedValueOnce(new Error('layout failed'));
        pending.resolve({ ...solution, buildSteps: ['new selection'] });
      }
      await selection;
      expect(state.getSelected()).toBe(0);
      expect(state.getEntry().selectedSourceIndex).toBe(0);
      expect(state.getSolution()).toBe(solution);
      expect(get(state.nodes)).toEqual(before);
      expect(state.session.canUndo).toBe(true);
      expect(state.setError).toHaveBeenCalledOnce();
    },
  );

  it('reselects a committed paged row without losing edits and cancels a pending switch', async () => {
    const pending = deferred<Solution>();
    const load = vi.fn().mockResolvedValueOnce(solution).mockReturnValueOnce(pending.promise);
    const state = setup(load);
    state.setPaged(2);
    vi.mocked(layoutSolution).mockResolvedValueOnce(layout('committed'));
    await state.session.selectSolution(0);
    state.session.onNodeDragStart();
    state.nodes.update((nodes) => nodes.map((node) => ({ ...node, position: { x: 12, y: 34 } })));
    state.session.onNodeDragStop();
    await state.session.selectSolution(0);
    expect(load).toHaveBeenCalledOnce();
    expect(state.session.canUndo).toBe(true);
    const selection = state.session.selectSolution(1);
    await state.session.selectSolution(0);
    pending.resolve({ ...solution, buildSteps: ['abandoned'] });
    await selection;
    expect(state.getSelected()).toBe(0);
    expect(state.getSolution()).toBe(solution);
    expect(state.session.canUndo).toBe(true);
    state.session.undo();
    expect(get(state.nodes)[0].position).toEqual({ x: 0, y: 0 });
  });

  it('loads only the requested source index and restores its saved edit independently of page order', async () => {
    const load = vi.fn(async (_entry: HistoryEntry, _index: number) => solution);
    const state = setup(load);
    state.setPaged(200);
    vi.mocked(layoutSolution).mockResolvedValue(layout('paged'));
    await state.session.selectSolution(130);
    expect(load.mock.calls[0][1]).toBe(130);
    state.session.onNodeDragStart();
    state.nodes.update((nodes) => nodes.map((node) => ({ ...node, position: { x: 130, y: 42 } })));
    state.session.onNodeDragStop();
    await state.session.selectSolution(3);
    await state.session.selectSolution(130);
    expect(state.getSelected()).toBe(130);
    expect(get(state.nodes)[0].position).toEqual({ x: 130, y: 42 });
    expect(state.getEntry().layouts['130'].nodes[0].position).toEqual({ x: 130, y: 42 });
    expect(layoutSolution).toHaveBeenCalledTimes(2);
  });

  it('discards an out-of-order graph read after selecting another source index', async () => {
    const first = deferred<Solution>(),
      second = deferred<Solution>();
    const load = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const state = setup(load);
    state.setPaged(200);
    vi.mocked(layoutSolution).mockResolvedValue(layout('latest'));
    const oldRead = state.session.selectSolution(130);
    const newRead = state.session.selectSolution(3);
    second.resolve({ ...solution, buildSteps: ['new selection'] });
    await newRead;
    first.resolve({ ...solution, buildSteps: ['old selection'] });
    await oldRead;
    expect(state.getSelected()).toBe(3);
    expect(state.getSolution()?.buildSteps).toEqual(['new selection']);
    expect(layoutSolution).toHaveBeenCalledOnce();
  });

  it('does not duplicate a pending first graph read on repeated live collection updates', async () => {
    const pending = deferred<Solution>();
    const load = vi.fn(() => pending.promise);
    const state = setup(load);
    state.setPaged(200);
    state.session.clearView();
    state.session.syncLiveResults(state.getEntry());
    state.session.syncLiveResults(state.getEntry());
    expect(load).toHaveBeenCalledOnce();
    vi.mocked(layoutSolution).mockResolvedValue(layout('first-page'));
    pending.resolve(solution);
    await vi.waitFor(() => expect(get(state.nodes)[0]?.id).toBe('first-page'));
  });

  it('names the exported graph from its saved request rather than the editable form', async () => {
    const state = setup();
    vi.mocked(layoutSolution).mockResolvedValue(layout('A'));
    await state.session.applySolution(solution);
    state.setRequest({
      inputs: [{ id: 'i', name: '', rate: '60' }],
      outputs: [
        { id: 'a', name: '', rate: '40' },
        { id: 'b', name: '', rate: '20' },
      ],
      beltRate: '60',
      solveMode: 'one_min_nl',
    });
    await state.session.exportSvg();
    expect(saveSvgFile).toHaveBeenCalledWith('<svg/>', '60_to_40-20_n-0.svg');
  });
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
