export type SolverEngine = 'custom' | 'z3';

export interface EndpointInput {
  id: string;
  name: string;
  rate: string;
}

/** Frontend row; `multiplier` expands into duplicate `EndpointInput`s when solving. */
export interface EndpointRow extends EndpointInput {
  multiplier: string;
}

export interface SolveRequest {
  inputs: EndpointInput[];
  outputs: EndpointInput[];
  beltRate: string;
  enumerateAllAtN?: boolean;
  /** Defaults to `custom` when omitted. */
  engine?: SolverEngine;
}

export interface DisplayRate {
  exact: string;
  decimal: string;
}

export type GraphNodeKind =
  | 'input'
  | 'splitter2'
  | 'splitter3'
  | 'merger2'
  | 'merger3'
  | 'output'
  | 'discard';

export interface GraphNode {
  id: string;
  kind: GraphNodeKind;
  label: string;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  sourcePort: number;
  targetPort: number;
  rate: DisplayRate;
  feedback: boolean;
  discarded: boolean;
}

export interface ProofSummary {
  proofVersion: number;
  initialNodeLowerBound: number;
  nodeCountsExhaustedThrough?: number | null;
  linkGroupsExhausted: number;
  profilesExhausted: number;
  rootPartitionsExhausted: number;
}

export interface ValidationSummary {
  validatorVersion: number;
  nodeCount: number;
  linkCount: number;
  physicalLinkCount: number;
  discardLinkCount: number;
  cyclicSccCount: number;
}

export interface SolutionStats {
  nodeCount: number;
  splitters: number;
  mergers: number;
  feedbackLoops: number;
  /** Shared operator-to-operator belt count; excludes input/output stubs and discard lines. */
  linkCount: number;
  checkedThrough?: number | null;
  /** Compatibility alias of linkCount, supplied by both engines. */
  beltCount?: number | null;
  /** Peak throughput across operator-to-operator belts, supplied by both engines. */
  internalMaxThroughput?: DisplayRate | null;
  /** Custom-only. */
  physicalLinkCount?: number | null;
  /** Custom-only. */
  discardLinkCount?: number | null;
}

export interface Solution {
  engine: SolverEngine;
  status: string;
  modelVersion: number;
  proof?: ProofSummary | null;
  validation?: ValidationSummary | null;
  stats: SolutionStats;
  totalInput: DisplayRate;
  totalOutput: DisplayRate;
  discardRate: DisplayRate;
  beltRate: DisplayRate;
  nodes: GraphNode[];
  edges: GraphEdge[];
  buildSteps: string[];
}

export type CustomSolvePhase =
  | 'normalizing'
  | 'global_checks'
  | 'computing_lower_bound'
  | 'searching'
  | 'validating_witness'
  | 'prewarming_components';

export interface ProofObligation {
  nodeCount: number;
  linkCount?: number | null;
  profile?: unknown;
  rootPartition?: number | null;
}

export interface SearchInstrumentation {
  rawStructuralDecisions: number;
  canonicalStatesRetained: number;
  canonicalDuplicatesEliminated: number;
  propagationContradictions: number;
  capacityPrunes: number;
  lowerBoundPrunes: number;
  noGoodHits: number;
  sccSolves: number;
  sccCacheHits: number;
  componentHits: number;
  componentOptimizations: number;
  componentDbLookups: number;
  componentDbHits: number;
  canonicalizationTimeNs: number;
  algebraTimeNs: number;
  peakStateCacheSize: number;
  peakMemoryBytes: number;
  wallTimeMs: number;
}

export type Z3SolverProgress =
  | {
      kind: 'preparing';
      lowerBound: number;
    }
  | {
      kind: 'checking';
      nodeCount: number;
      rejectedUnstableCandidates: number;
      lowerBound: number;
      profileCount: number;
      attemptSlots: number;
      threadsPerAttempt: number;
    }
  | {
      kind: 'candidate_rejected';
      nodeCount: number;
      rejectedUnstableCandidates: number;
      reason: string;
      lowerBound: number;
    }
  | {
      kind: 'size_progress';
      nodeCount: number;
      lowerBound: number;
      profilesTotal: number;
      profilesUnresolved: number;
      profilesUnsat: number;
      activeAttempts: number;
      launchedAttempts: number;
      abandonedAttempts: number;
      rejectedUnstableCandidates: number;
      attemptSlots: number;
    };

export type SolverProgress =
  | ({ engine: 'z3' } & Z3SolverProgress)
  | {
      engine: 'custom';
      phase: CustomSolvePhase;
      obligation?: ProofObligation | null;
      completedProfiles: number;
      totalProfiles?: number | null;
      instrumentation: SearchInstrumentation;
    };

export type JobStatus =
  | 'running'
  | 'cancelling'
  | 'completed'
  | 'cancelled'
  | 'incomplete'
  | 'unsat'
  | 'failed';

export interface GlobalUnsatProof {
  reason: unknown;
  proof?: ProofSummary;
}

export interface JobSnapshot {
  jobId: string;
  status: JobStatus;
  startedAtMs: number;
  progress: SolverProgress | null;
  result: Solution | null;
  results: Solution[];
  enumerationComplete: boolean;
  /** Progress-only emit: result/results empty on purpose; keep local copies. */
  resultsOmitted?: boolean;
  /** Incremental full-N emit: `result` is the newly found layout; append locally. */
  resultAppended?: boolean;
  /** When `resultAppended`, server results length after the push (append seq). */
  resultsLen?: number;
  error: string | null;
  unsat?: GlobalUnsatProof | null;
}

export function isCustomProgress(
  progress: SolverProgress | null | undefined
): progress is Extract<SolverProgress, { engine: 'custom' }> {
  return progress?.engine === 'custom';
}

export function isZ3Progress(
  progress: SolverProgress | null | undefined
): progress is Extract<SolverProgress, { engine: 'z3' }> {
  return progress != null && progress.engine === 'z3';
}

/** Belt/link count used by tables and sort. */
export function solutionLinkCount(solution: Solution): number {
  return solution.stats.linkCount ?? solution.stats.beltCount ?? 0;
}
