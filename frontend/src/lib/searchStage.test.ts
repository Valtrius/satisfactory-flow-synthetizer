import { describe, expect, it } from 'vitest';
import type { JobSnapshot, SolverProgress } from '../types';
import {
  formatElapsed,
  nodeCountLabel,
  ruledOutRangeLabel,
  searchHeadline,
  searchStageView,
  searchSubline,
  sizeSearchBody
} from './searchStage';

function snapshot(
  partial: Partial<JobSnapshot> & Pick<JobSnapshot, 'status'>
): JobSnapshot {
  return {
    jobId: 'job-1',
    startedAtMs: 0,
    progress: null,
    result: null,
    results: [],
    enumerationComplete: false,
    error: null,
    ...partial
  };
}

describe('searchStage', () => {
  it('formats elapsed time with tenths under an hour', () => {
    expect(formatElapsed(65_200)).toBe('1:05.2');
    expect(formatElapsed(3_661_000)).toBe('1:01:01');
  });

  it('labels node counts', () => {
    expect(nodeCountLabel(1)).toBe('1 node');
    expect(nodeCountLabel(4)).toBe('4 nodes');
  });

  it('maps preparing and checking progress', () => {
    const preparing: SolverProgress = { engine: 'z3', kind: 'preparing', lowerBound: 3 };
    expect(searchStageView(preparing).lowerBound).toBe(3);
    expect(searchStageView(preparing).nodeCount).toBeNull();
    expect(searchStageView(preparing).engine).toBe('z3');

    const checking: SolverProgress = {
      engine: 'z3',
      kind: 'checking',
      nodeCount: 5,
      rejectedUnstableCandidates: 2,
      lowerBound: 3,
      profileCount: 10,
      attemptSlots: 4,
      threadsPerAttempt: 1
    };
    const view = searchStageView(checking);
    expect(view.ruledOut).toBe(2);
    expect(view.ruledOutThrough).toBe(4);
    expect(view.profilesTotal).toBe(10);
  });

  it('maps custom progress without inventing Z3 portfolio fields', () => {
    const progress: SolverProgress = {
      engine: 'custom',
      phase: 'searching',
      obligation: { nodeCount: 3, linkCount: 5 },
      completedProfiles: 2,
      totalProfiles: 8,
      instrumentation: {
        rawStructuralDecisions: 0,
        canonicalStatesRetained: 1,
        canonicalDuplicatesEliminated: 0,
        propagationContradictions: 0,
        capacityPrunes: 4,
        lowerBoundPrunes: 1,
        noGoodHits: 0,
        sccSolves: 0,
        sccCacheHits: 0,
        componentHits: 0,
        componentOptimizations: 0,
        componentDbLookups: 0,
        componentDbHits: 0,
        canonicalizationTimeNs: 0,
        algebraTimeNs: 0,
        peakStateCacheSize: 0,
        peakMemoryBytes: 0,
        wallTimeMs: 12
      }
    };
    const view = searchStageView(progress);
    expect(view.engine).toBe('custom');
    expect(view.nodeCount).toBe(3);
    expect(view.linkCount).toBe(5);
    expect(view.profilesCompleted).toBe(2);
    expect(view.launchedAttempts).toBe(0);
    expect(view.instrumentation?.capacityPrunes).toBe(4);
  });

  it('builds size search body and ruled-out ranges', () => {
    const view = searchStageView({
      engine: 'z3',
      kind: 'size_progress',
      nodeCount: 6,
      lowerBound: 4,
      profilesTotal: 8,
      profilesUnresolved: 2,
      profilesUnsat: 4,
      activeAttempts: 1,
      launchedAttempts: 5,
      abandonedAttempts: 0,
      rejectedUnstableCandidates: 1,
      attemptSlots: 4
    });
    expect(ruledOutRangeLabel(view)).toBe('4–5');
    expect(sizeSearchBody(snapshot({ status: 'running' }), view)).toContain(
      'Searching all 6-node'
    );
  });

  it('builds headline and subline from explicit context', () => {
    const running = snapshot({
      status: 'running',
      progress: { engine: 'z3', kind: 'preparing', lowerBound: 2 }
    });
    expect(
      searchHeadline(running, {
        solutionsLength: 0,
        searchEnumerate: false,
        firstNodeCount: null,
        engine: 'z3'
      })
    ).toBe('Preparing the exact model');
    expect(
      searchSubline(
        running,
        searchStageView(running.progress),
        { solutionsLength: 0, searchEnumerate: false, firstNodeCount: null, engine: 'z3' }
      )
    ).toContain('Lower bound');
  });

  it('uses Custom cancel copy', () => {
    const cancelling = snapshot({ status: 'cancelling' });
    expect(
      searchSubline(cancelling, searchStageView(null), {
        solutionsLength: 0,
        searchEnumerate: false,
        firstNodeCount: null,
        engine: 'custom'
      })
    ).toContain('Custom');
  });
});
