import { afterEach, describe, expect, it, vi } from 'vitest';
import type { JobSnapshot, Solution, SolveRequest } from '../types';
import { createQueuedEntry, type HistoryEntry } from './historyModel';
import { HistoryQueue } from './historyQueue';
import { cancelJob, createJob, getJob, releaseJob, shutdownJobs, resumeJobs, watchJob } from './api';

vi.mock('./api', () => ({
  createJob: vi.fn(),
  getJob: vi.fn(),
  watchJob: vi.fn(),
  cancelJob: vi.fn(),
  shutdownJobs: vi.fn(),
  resumeJobs: vi.fn(),
  releaseJob: vi.fn().mockResolvedValue(undefined),
}));

const request: SolveRequest = {
  inputs: [],
  outputs: [{ id: 'out', name: '', rate: '1' }],
  beltRate: '1200',
  solveMode: 'all_min_n',
};
const rate = { exact: '1', decimal: '1' };
const solution: Solution = {
  status: 'best_known',
  modelVersion: 1,
  stats: {
    nodeCount: 2,
    linkCount: 2,
    splitters: 1,
    mergers: 1,
    feedbackLoops: 0,
  },
  totalInput: rate,
  totalOutput: rate,
  discardRate: rate,
  beltRate: rate,
  nodes: [],
  edges: [],
  buildSteps: [],
};
let queue: HistoryQueue;
afterEach(() => {
  queue?.dispose();
  vi.resetAllMocks();
});

async function setup() {
  const queued = createQueuedEntry(
    {
      ...request,
      inputs: [],
      outputs: request.outputs.map((o) => ({ ...o, multiplier: '1' })),
      solveMode: 'all_min_n',
    },
    request,
  );
  let entries: HistoryEntry[] = [queued];
  let receive!: (snapshot: JobSnapshot) => void;
  vi.mocked(createJob).mockResolvedValue('job');
  vi.mocked(releaseJob).mockResolvedValue(undefined);
  vi.mocked(watchJob).mockImplementation(async (_id, onSnapshot) => {
    receive = onSnapshot;
    return { close: vi.fn() };
  });
  queue = new HistoryQueue({
    getEntries: () => entries,
    setEntries: (value) => {
      entries = value;
    },
    patchEntry: (id, patch) => {
      entries = entries.map((e) => (e.id === id ? { ...e, ...patch } : e));
    },
    getSelectedId: () => queued.id,
    flushSelectedChrome: vi.fn(),
    syncViewIfSelected: vi.fn(),
    setError: vi.fn(),
    setElapsedMs: vi.fn(),
  });
  await queue.pump();
  return { receive, entry: () => entries[0] };
}

function snapshot(overrides: Partial<JobSnapshot> = {}): JobSnapshot {
  return {
    jobId: 'job',
    status: 'running',
    startedAtMs: 1,
    progress: null,
    result: null,
    results: [],
    enumerationComplete: false,
    error: null,
    ...overrides,
  };
}

describe('common solver snapshot mapping', () => {
  it('keeps Opt incumbents out of the enumeration list', async () => {
    const state = await setup();
    state.receive(snapshot({ sequence: 1, result: solution }));
    expect(state.entry().result).toEqual(solution);
    expect(state.entry().results).toEqual([]);
    state.receive(snapshot({ sequence: 2, resultsOmitted: true }));
    expect(state.entry().result).toEqual(solution);
    expect(state.entry().results).toEqual([]);
  });

  it('recovers a missed append from a full snapshot at the same sequence', async () => {
    const state = await setup();
    const full = snapshot({
      sequence: 5,
      result: solution,
      results: [solution],
    });
    vi.mocked(getJob).mockResolvedValue(full);
    state.receive(snapshot({ sequence: 5, resultsOmitted: true, resultsLen: 1 }));
    await vi.waitFor(() => expect(state.entry().results).toEqual([solution]));
    expect(getJob).toHaveBeenCalledWith('job');
    state.receive(
      snapshot({
        sequence: 4,
        resultAppended: true,
        result: solution,
        resultsLen: 1,
      }),
    );
    expect(state.entry().sequence).toBe(5);
    expect(state.entry().results).toHaveLength(1);
  });

  it('retains a cancelled witness and minimum-N proof against late progress', async () => {
    const state = await setup();
    const proof = { minimumNodeCount: 2, minimumLinkCount: null };
    state.receive(snapshot({ sequence: 8, status: 'cancelled', result: solution, proof }));
    state.receive(snapshot({ sequence: 7, resultsOmitted: true }));
    expect(state.entry().status).toBe('cancelled');
    expect(state.entry().proof).toEqual(proof);
    expect(state.entry().result?.status).toBe('best_known');
    expect(state.entry().enumerationComplete).toBe(false);
  });
});

it('releases a full terminal snapshot once and ignores late duplicates', async () => {
  const state = await setup();
  state.receive(snapshot({ sequence: 9, status: 'completed', result: solution, results: [solution] }));
  state.receive(snapshot({ sequence: 9, status: 'completed', result: solution, results: [solution] }));
  expect(releaseJob).toHaveBeenCalledExactlyOnceWith('job');
  expect(state.entry().jobId).toBeNull();
  expect(state.entry().results).toEqual([solution]);
});

it('recovers final results during shutdown without starting queued work', async () => {
  const state = await setup();
  const final = snapshot({ sequence: 10, status: 'cancelled', result: solution, results: [solution] });
  vi.mocked(shutdownJobs).mockResolvedValue([final]);
  queue.pause();
  await queue.shutdown();
  expect(state.entry()).toMatchObject({ status: 'cancelled', results: [solution], jobId: null });
  expect(createJob).toHaveBeenCalledTimes(1);
  vi.mocked(resumeJobs).mockResolvedValue(undefined);
  await queue.resume();
  expect(resumeJobs).toHaveBeenCalledOnce();
});

it('discards a job collection when its queued entry is deleted during admission', async () => {
  const entry = createQueuedEntry({ ...request, inputs: [], outputs: [], solveMode: 'all_min_n' }, request);
  let entries = [entry];
  let admit!: (id: string) => void;
  vi.mocked(createJob).mockImplementation(
    () =>
      new Promise((resolve) => {
        admit = resolve;
      }),
  );
  const collection = { version: 1 as const, id: 'unattached-job', count: 1, preferredIndex: null };
  vi.mocked(getJob).mockResolvedValue(snapshot({ status: 'cancelled', collection }));
  const discard = vi.fn(async () => {
    expect(cancelJob).toHaveBeenCalledWith('job');
    expect(getJob).toHaveBeenCalledWith('job');
  });
  queue = new HistoryQueue({
    getEntries: () => entries,
    setEntries: (value) => {
      entries = value;
    },
    patchEntry: vi.fn(),
    getSelectedId: () => null,
    flushSelectedChrome: vi.fn(),
    syncViewIfSelected: vi.fn(),
    setError: vi.fn(),
    setElapsedMs: vi.fn(),
    discardCollection: discard,
  });
  const running = queue.pump();
  entries = [];
  admit('job');
  await running;
  expect(discard).toHaveBeenCalledExactlyOnceWith(collection);
  expect(releaseJob).toHaveBeenCalledExactlyOnceWith('job');
  expect(watchJob).not.toHaveBeenCalled();
  expect(entries).toEqual([]);
});
