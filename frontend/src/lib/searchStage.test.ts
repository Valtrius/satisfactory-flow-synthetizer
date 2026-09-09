import { describe, expect, it } from 'vitest';
import type { JobSnapshot, SolverProgress } from '../types';
import {
  diagnosticText,
  formatElapsed,
  formatTelemetryNumber,
  nodeCountLabel,
  ruledOutRangeLabel,
  searchHeadline,
  searchStageView,
  searchSubline,
  sizeSearchBody,
} from './searchStage';

function progress(part: Partial<SolverProgress> = {}): SolverProgress {
  return {
    phase: 'searching',
    elapsedMs: 10,
    nodeCount: null,
    linkConstraint: null,
    nodeLowerBound: null,
    bestNodeCount: null,
    bestLinkCount: null,
    solutionsFound: 0,
    custom: [],
    ...part,
  };
}
function snapshot(part: Partial<JobSnapshot> = {}): JobSnapshot {
  return {
    jobId: 'job-1',
    status: 'running',
    startedAtMs: 0,
    progress: null,
    result: null,
    results: [],
    enumerationComplete: false,
    error: null,
    ...part,
  };
}
const context = {
  solutionsLength: 0,
  searchEnumerate: false,
  firstNodeCount: null,
};

describe('shared search progress', () => {
  it('formats elapsed time and node counts', () => {
    expect(formatElapsed(65_200)).toBe('1:05.2');
    expect(formatElapsed(3_661_000)).toBe('1:01:01');
    expect(nodeCountLabel(1)).toBe('1 node');
    expect(nodeCountLabel(4)).toBe('4 nodes');
  });
  it('maps the same common facts regardless of diagnostic names', () => {
    for (const name of ['solver.profiles_exhausted', 'solver.branch', 'future.metric']) {
      const view = searchStageView(
        progress({
          nodeCount: 3,
          nodeLowerBound: 3,
          custom: [
            {
              name,
              label: 'Diagnostic',
              value: { type: 'integer', value: '42' },
              unit: null,
            },
          ],
        }),
      );
      expect(view.nodeCount).toBe(3);
      expect(view.ruledOutThrough).toBe(2);
      expect(view.custom[0].name).toBe(name);
    }
  });

  it('retains integer precision and displays unknown diagnostics', () => {
    expect(
      diagnosticText({
        name: 'future.counter',
        label: 'Counter',
        value: { type: 'integer', value: '18446744073709551615' },
        unit: 'items',
      }),
    ).toBe("18'446'744'073'709'551'615 items");
    expect(
      diagnosticText({
        name: 'future.flag',
        label: 'Flag',
        value: { type: 'boolean', value: false },
        unit: null,
      }),
    ).toBe('false');
    expect(
      diagnosticText({
        name: 'future.text',
        label: 'Text',
        value: { type: 'text', value: '123456' },
        unit: null,
      }),
    ).toBe('123456');
    expect(
      diagnosticText({
        name: 'future.rate',
        label: 'Rate',
        value: { type: 'rate', value: '12345/1000' },
        unit: 'items/min',
      }),
    ).toBe("12'345/1'000 items/min");
  });
  it('groups telemetry integers with apostrophes', () => {
    expect(formatTelemetryNumber(0)).toBe('0');
    expect(formatTelemetryNumber(999)).toBe('999');
    expect(formatTelemetryNumber(1000)).toBe("1'000");
    expect(formatTelemetryNumber(-1234567)).toBe("-1'234'567");
    expect(formatTelemetryNumber(null)).toBe('—');
  });
  it('describes only proved node bounds', () => {
    expect(ruledOutRangeLabel(searchStageView(progress({ nodeLowerBound: 6 })))).toBe('0–5');
    expect(ruledOutRangeLabel(searchStageView(progress()))).toBe('');
  });
  it('keeps minimum N proof distinct from unfinished optimization', () => {
    const stopped = snapshot({
      status: 'cancelled',
      proof: { minimumNodeCount: 3, minimumLinkCount: null },
    });
    expect(searchHeadline(stopped, context)).toBe('Search cancelled');
    expect(searchSubline(stopped, searchStageView(null), context)).toContain('Minimum N=3 proved');
  });
  it('does not call an incomplete enumeration complete', () => {
    const partial = snapshot({
      status: 'cancelled',
      enumerationComplete: false,
    });
    expect(
      searchHeadline(partial, {
        ...context,
        searchEnumerate: true,
        solutionsLength: 2,
        firstNodeCount: 1,
      }),
    ).toContain('cancelled');
  });
  it('keeps engine-specific cancellation copy outside the progress contract', () => {
    expect(
      searchSubline(snapshot({ status: 'cancelling' }), searchStageView(null), {
        ...context,
      }),
    ).toContain('solver workers');
  });
  it('describes lower-bound work while the bound is still unknown', () => {
    const view = searchStageView(progress({ phase: 'computing_lower_bound' }));
    expect(searchSubline(snapshot(), view, context)).toBe('Computing lower bound.');
  });
});
