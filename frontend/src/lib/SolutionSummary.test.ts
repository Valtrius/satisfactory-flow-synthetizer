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
    internalMaxThroughput: { exact: '120/7', decimal: '17.142857' },
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
    props: { solution: { ...solution, ...overrides }, elapsedLabel: '1.2s', onclose: () => {} },
  }).body;
}

describe('SolutionSummary', () => {
  it.each(['proven_optimal', 'best_known'])('keeps %s statistics without a duplicate status header', (status) => {
    const html = renderSummary({ status });
    expect(html).toContain('Graph information');
    expect(html).not.toContain('Proven optimal');
    expect(html).not.toContain('Best known');
  });

  it('shows the user statistics without internal proof diagnostics', () => {
    const text = renderSummary()
      .replace(/<[^>]*>/g, ' ')
      .replace(/\s+/g, ' ')
      .trim();
    for (const statistic of [
      'Solve time 1.2s',
      'Nodes 3',
      'Splitters 2',
      'Mergers 1',
      'Links 2',
      'Peak rate 120/7 /min',
      'Discard 20 /min',
    ])
      expect(text).toContain(statistic);
    for (const label of ['Physical links', 'Discard links', 'Checked through', 'Validator'])
      expect(text).not.toContain(label);
  });

  it('omits zero metrics, while retaining exact nonzero discard rates', () => {
    const emptyStats = { nodeCount: 0, splitters: 0, mergers: 0, feedbackLoops: 0, linkCount: 0 };
    const html = renderSummary({ stats: emptyStats, discardRate: { exact: '0/7', decimal: '0' } });
    for (const label of ['Nodes', 'Splitters', 'Mergers', 'Feedbacks', 'Links', 'Peak rate', 'Discard'])
      expect(html).not.toContain(label);
    const fraction = renderSummary({
      stats: emptyStats,
      discardRate: { exact: '1/1000000000000000000000', decimal: '0' },
    });
    expect(fraction).toContain('Discard');
    expect(fraction).toContain('1/1000000000000000000000');
  });

  it('orders links, exact peak rate and feedbacks, and omits a zero peak rate', () => {
    const html = renderSummary({ stats: { ...solution.stats, feedbackLoops: 2 } });
    expect(html.indexOf('Links')).toBeLessThan(html.indexOf('Peak rate'));
    expect(html.indexOf('Peak rate')).toBeLessThan(html.indexOf('Feedbacks'));
    expect(
      renderSummary({
        stats: { ...solution.stats, internalMaxThroughput: { exact: '0/7', decimal: '0' } },
      }),
    ).not.toContain('Peak rate');
  });
});
