export type SolveMode = 'one_min_nl' | 'all_min_nl' | 'all_min_n';

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
  solveMode: SolveMode;
  /** Defaults to `custom` when omitted. */
}

export function enumeratesLayouts(mode: SolveMode): boolean {
  return mode !== 'one_min_nl';
}

export interface DisplayRate {
  exact: string;
  decimal: string;
}

export type GraphNodeKind = 'input' | 'splitter2' | 'splitter3' | 'merger2' | 'merger3' | 'output' | 'discard';

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
  /** Legacy history alias of linkCount. */
  beltCount?: number | null;
  /** Peak throughput across operator-to-operator belts, supplied by the solver. */
  internalMaxThroughput?: DisplayRate | null;
  /** Physical accounting supplied by the solver; optional in old history. */
  physicalLinkCount?: number | null;
  /** Physical accounting supplied by the solver; optional in old history. */
  discardLinkCount?: number | null;
}

export interface Solution {
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

export type SolvePhase = 'computing_lower_bound' | 'searching' | 'enumerating';

export type DiagnosticValue =
  { type: 'integer' | 'text' | 'rate'; value: string } | { type: 'boolean'; value: boolean };

export interface Diagnostic {
  name: string;
  label: string;
  value: DiagnosticValue;
  unit: string | null;
}

export interface SolverProgress {
  phase: SolvePhase;
  elapsedMs: number;
  nodeCount: number | null;
  linkConstraint: { kind: 'exact'; value: number } | null;
  nodeLowerBound: number | null;
  bestNodeCount: number | null;
  bestLinkCount: number | null;
  solutionsFound: number;
  custom: Diagnostic[];
}

export interface OptimalityProof {
  minimumNodeCount: number | null;
  minimumLinkCount: number | null;
}

export type JobStatus = 'running' | 'cancelling' | 'completed' | 'cancelled' | 'incomplete' | 'unsat' | 'failed';

export interface GlobalUnsatProof {
  reason: unknown;
  proof?: ProofSummary;
}

export interface JobSnapshot {
  jobId: string;
  status: JobStatus;
  startedAtMs: number;
  progress: SolverProgress | null;
  proof?: OptimalityProof | null;
  sequence?: number;
  result: Solution | null;
  results: Solution[];
  enumerationComplete: boolean;
  /** Progress-only emit: result/results empty on purpose; keep local copies. */
  resultsOmitted?: boolean;
  /** Incremental full-N emit: `result` is the newly found layout; append locally. */
  resultAppended?: boolean;
  /** Server result count, also included with progress to detect missed appends. */
  resultsLen?: number;
  error: string | null;
  unsat?: GlobalUnsatProof | null;
}

/** Belt/link count used by tables and sort. */
export function solutionLinkCount(solution: Solution): number {
  return solution.stats.linkCount ?? solution.stats.beltCount ?? 0;
}

export const SOLVE_MODE_LABELS: Record<SolveMode, string> = {
  one_min_nl: 'One min N/L',
  all_min_nl: 'All min N/L',
  all_min_n: 'All min N',
};

/** Old history and preferences remain readable; writes use the current names. */
export function parseSolveMode(value: unknown, fallback: SolveMode = 'one_min_nl'): SolveMode {
  switch (value) {
    case 'one_min_nl':
    case 'optimal':
      return 'one_min_nl';
    case 'all_min_nl':
    case 'all_at_minimum_nodes_and_minimum_links':
    case 'minimum_links':
      return 'all_min_nl';
    case 'all_min_n':
    case 'all_at_minimum_nodes':
    case 'all':
      return 'all_min_n';
    default:
      return fallback;
  }
}
