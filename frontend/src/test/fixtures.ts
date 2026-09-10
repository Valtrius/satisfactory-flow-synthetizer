import type { Solution, SolveRequest } from '../types';
import { createQueuedEntry, type HistoryEntry } from '../lib/historyModel';

const rate = (exact: string) => ({ exact, decimal: exact });
export const solution: Solution = {
  status: 'proven_optimal',
  modelVersion: 1,
  stats: { nodeCount: 1, splitters: 1, mergers: 0, feedbackLoops: 0, linkCount: 0 },
  totalInput: rate('2'),
  totalOutput: rate('2'),
  discardRate: rate('0'),
  beltRate: rate('2'),
  nodes: [
    { id: 'i', kind: 'input', label: 'Input' },
    { id: 's', kind: 'splitter2', label: 'Splitter' },
    { id: 'a', kind: 'output', label: 'First' },
    { id: 'b', kind: 'output', label: 'Second' },
  ],
  edges: [
    {
      id: 'in',
      source: 'i',
      target: 's',
      sourcePort: 0,
      targetPort: 0,
      rate: rate('2'),
      feedback: false,
      discarded: false,
    },
    {
      id: 'a',
      source: 's',
      target: 'a',
      sourcePort: 0,
      targetPort: 0,
      rate: rate('1'),
      feedback: false,
      discarded: false,
    },
    {
      id: 'b',
      source: 's',
      target: 'b',
      sourcePort: 1,
      targetPort: 0,
      rate: rate('1'),
      feedback: false,
      discarded: false,
    },
  ],
  buildSteps: [],
};
export const request: SolveRequest = {
  inputs: [{ id: 'i', name: '', rate: '2' }],
  outputs: [
    { id: 'a', name: '', rate: '1' },
    { id: 'b', name: '', rate: '1' },
  ],
  beltRate: '2',
  solveMode: 'all_min_nl',
};
export function historyEntry(overrides: Partial<HistoryEntry> = {}): HistoryEntry {
  return {
    ...createQueuedEntry(
      {
        ...request,
        inputs: request.inputs.map((row) => ({ ...row, multiplier: '1' })),
        outputs: request.outputs.map((row) => ({ ...row, multiplier: '1' })),
      },
      request,
    ),
    id: 'entry',
    status: 'completed',
    enumerationComplete: true,
    result: solution,
    results: [solution],
    ...overrides,
  };
}
