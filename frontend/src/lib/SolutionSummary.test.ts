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
  it('places the node count and status in a compact card header directly above the stats', () => {
    const html = renderSummary();
    const header = html.match(/<header\b[^>]*>[\s\S]*?<\/header>/)?.[0];

    expect(header).toBeDefined();
    expect(header).toContain('3 nodes');
    expect(header).toContain('Proven optimal');
    expect(header).toContain('justify-between');
    expect(header).toContain('border-b');
    expect(header).toContain('border-line');
    expect(header).toContain('text-lg');
    expect(html.indexOf('</header>')).toBeLessThan(html.indexOf('Solve time'));
    expect(header).not.toMatch(/\bmb-\d|text-3xl|text-4xl/);
    expect(html).toContain('grid grid-cols-2 gap-px');
    expect(html).toContain('border-line bg-line');
    expect(html).not.toMatch(/rounded-tl-|rounded-br-/);
  });

  it('shows one optimal status pill after the node count', () => {
    const html = renderSummary();

    expect(html.match(/rounded-full/g)).toHaveLength(1);
    expect(html).toContain('Proven optimal');
    expect(html.indexOf('rounded-full')).toBeGreaterThan(html.indexOf('</h2>'));
    expect(html).toContain('text-[#8bdeb8]');
    expect(html).not.toContain('Verified optimal');
    expect(html).not.toContain('Exact &amp; optimal');
  });

  it('keeps best-known results distinct ', () => {
    const html = renderSummary({ status: 'best_known' });

    expect(html.match(/rounded-full/g)).toHaveLength(1);
    expect(html).toContain('Best known');
    expect(html).toContain('title="Not proven optimal"');
    expect(html).toContain('text-[#e6c27a]');
    expect(html).not.toContain('Proven optimal');
    expect(html).not.toContain('Validated witness');
  });

  it('keeps the six main stats and omits diagnostics even when supplied', () => {
    const html = renderSummary();
    const text = html
      .replace(/<[^>]*>/g, ' ')
      .replace(/\s+/g, ' ')
      .trim();

    expect(text).toContain('3 nodes');
    expect(text).toContain('Solve time 1.2s Splitters 2 Mergers 1 Feedback loops 0 Links (L) 2 Discard 20 /min');
    expect(html.match(/<strong\b/g)).toHaveLength(6);
    for (const label of ['Physical links', 'Discard links', 'Checked through', 'Validator']) {
      expect(text).not.toContain(label);
    }
  });
});
