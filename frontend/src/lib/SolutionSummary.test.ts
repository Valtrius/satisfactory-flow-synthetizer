// @vitest-environment node
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import type { Solution } from '../types';
import SolutionSummary from './SolutionSummary.svelte';

const solution: Solution = {
  status: 'proven_optimal',
  modelVersion: 1,
  stats: {
    nodeCount: 3,
    splitters: 2,
    mergers: 1,
    feedbackLoops: 0,
    linkCount: 2,
    physicalLinkCount: 7,
    discardLinkCount: 1,
    checkedThrough: 3,
  },
  validation: {
    validatorVersion: 1,
    nodeCount: 3,
    linkCount: 2,
    physicalLinkCount: 7,
    discardLinkCount: 1,
    cyclicSccCount: 0,
  },
  totalInput: { exact: '120', decimal: '120' },
  totalOutput: { exact: '100', decimal: '100' },
  discardRate: { exact: '20', decimal: '20' },
  beltRate: { exact: '1200', decimal: '1200' },
  nodes: [],
  edges: [],
  buildSteps: [],
};

function renderSummary(overrides: Partial<Solution> = {}): string {
  return render(SolutionSummary, {
    props: { solution: { ...solution, ...overrides }, elapsedLabel: '1.2s' },
  }).body;
}

describe('SolutionSummary', () => {
  it.each([
    ['proven_optimal', 'Proven optimal', 'Best known'],
    ['best_known', 'Best known', 'Proven optimal'],
  ] as const)('distinguishes the %s result status', (status, label, other) => {
    const html = renderSummary({ status });
    expect(html).toContain('3 nodes');
    expect(html).toContain(label);
    expect(html).not.toContain(other);
    if (status === 'best_known') expect(html).toContain('Not proven optimal');
  });

  it('shows the user statistics without internal proof diagnostics', () => {
    const text = renderSummary()
      .replace(/<[^>]*>/g, ' ')
      .replace(/\s+/g, ' ')
      .trim();
    for (const statistic of [
      'Solve time 1.2s',
      'Splitters 2',
      'Mergers 1',
      'Feedback loops 0',
      'Links (L) 2',
      'Discard 20 /min',
    ])
      expect(text).toContain(statistic);
    for (const label of ['Physical links', 'Discard links', 'Checked through', 'Validator'])
      expect(text).not.toContain(label);
  });
});
