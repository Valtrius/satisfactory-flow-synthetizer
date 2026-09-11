import { createQueuedEntry, type HistoryEntry } from '../historyModel';
import type { VerifiedShare } from './client';

/** A verified physical witness is saved without a search proof or enumeration claim. */
export function sharedHistoryEntry(verified: VerifiedShare): HistoryEntry {
  const endpoints = (items: VerifiedShare['share']['request']['inputs'], prefix: string) =>
    items.map((item, index) => ({ ...item, id: `${prefix}-${index}` }));
  const request = {
    inputs: endpoints(verified.share.request.inputs, 'input'),
    outputs: endpoints(verified.share.request.outputs, 'output'),
    beltRate: verified.share.request.beltRate,
    // Only a default for copying into the local input form. It was not shared or solved.
    solveMode: 'one_min_nl' as const,
  };
  const form = {
    ...request,
    inputs: request.inputs.map((row) => ({ ...row, multiplier: '1' })),
    outputs: request.outputs.map((row) => ({ ...row, multiplier: '1' })),
  };
  const solution = { ...verified.solution, status: 'best_known', proof: null };
  return {
    ...createQueuedEntry(form, request),
    title: 'Shared solution',
    status: 'completed',
    result: solution,
    results: [solution],
    enumerationComplete: false,
    proof: null,
  };
}
