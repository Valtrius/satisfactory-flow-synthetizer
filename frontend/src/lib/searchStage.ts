import type {
  JobSnapshot,
  SearchInstrumentation,
  SolverProgress
} from '../types';
import { isCustomProgress, isZ3Progress } from '../types';

export type SearchStageView = {
  engine: 'custom' | 'z3' | null;
  phase: string | null;
  lowerBound: number | null;
  nodeCount: number | null;
  linkCount: number | null;
  rejected: number;
  profilesTotal: number | null;
  profilesUnsat: number;
  profilesCompleted: number;
  activeAttempts: number;
  attemptSlots: number | null;
  launchedAttempts: number;
  abandoned: number;
  ruledOut: number;
  /** Last node count fully ruled out, if any. */
  ruledOutThrough: number | null;
  portfolioPct: number;
  instrumentation: SearchInstrumentation | null;
};

export type SearchCopyContext = {
  solutionsLength: number;
  searchEnumerate: boolean;
  firstNodeCount: number | null;
  engine?: 'custom' | 'z3';
};

const EMPTY_VIEW: SearchStageView = {
  engine: null,
  phase: null,
  lowerBound: null,
  nodeCount: null,
  linkCount: null,
  rejected: 0,
  profilesTotal: null,
  profilesUnsat: 0,
  profilesCompleted: 0,
  activeAttempts: 0,
  attemptSlots: null,
  launchedAttempts: 0,
  abandoned: 0,
  ruledOut: 0,
  ruledOutThrough: null,
  portfolioPct: 0,
  instrumentation: null
};

export function formatElapsed(milliseconds: number): string {
  const totalSeconds = Math.floor(milliseconds / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const tenths = Math.floor((milliseconds % 1000) / 100);
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`
    : `${minutes}:${String(seconds).padStart(2, '0')}.${tenths}`;
}

export function nodeCountLabel(count: number): string {
  return `${count} ${count === 1 ? 'node' : 'nodes'}`;
}

function customPhaseLabel(phase: string): string {
  switch (phase) {
    case 'normalizing':
      return 'Normalizing rates';
    case 'global_checks':
      return 'Running global checks';
    case 'computing_lower_bound':
      return 'Computing lower bound';
    case 'searching':
      return 'Searching profiles';
    case 'validating_witness':
      return 'Validating witness';
    default:
      return phase;
  }
}

export function searchStageView(progress: SolverProgress | null): SearchStageView {
  if (!progress) return { ...EMPTY_VIEW };
  if (isCustomProgress(progress)) {
    const nodeCount = progress.obligation?.nodeCount ?? null;
    const linkCount = progress.obligation?.linkCount ?? null;
    const profilesTotal = progress.totalProfiles ?? null;
    const portfolioPct =
      profilesTotal != null && profilesTotal > 0
        ? Math.min(100, Math.round((100 * progress.completedProfiles) / profilesTotal))
        : 0;
    return {
      ...EMPTY_VIEW,
      engine: 'custom',
      phase: progress.phase,
      nodeCount,
      linkCount,
      profilesTotal,
      profilesCompleted: progress.completedProfiles,
      portfolioPct,
      instrumentation: progress.instrumentation
    };
  }
  if (!isZ3Progress(progress)) return { ...EMPTY_VIEW };
  if (progress.kind === 'preparing') {
    return {
      ...EMPTY_VIEW,
      engine: 'z3',
      lowerBound: progress.lowerBound
    };
  }
  if (progress.kind === 'checking') {
    return {
      ...EMPTY_VIEW,
      engine: 'z3',
      lowerBound: progress.lowerBound,
      nodeCount: progress.nodeCount,
      rejected: progress.rejectedUnstableCandidates,
      profilesTotal: progress.profileCount,
      attemptSlots: progress.attemptSlots,
      ruledOut: Math.max(0, progress.nodeCount - progress.lowerBound),
      ruledOutThrough:
        progress.nodeCount > progress.lowerBound ? progress.nodeCount - 1 : null
    };
  }
  if (progress.kind === 'candidate_rejected') {
    return {
      ...EMPTY_VIEW,
      engine: 'z3',
      lowerBound: progress.lowerBound,
      nodeCount: progress.nodeCount,
      rejected: progress.rejectedUnstableCandidates,
      ruledOut: Math.max(0, progress.nodeCount - progress.lowerBound + 1),
      ruledOutThrough: progress.nodeCount
    };
  }
  const portfolioPct =
    progress.profilesTotal > 0
      ? Math.min(100, Math.round((100 * progress.profilesUnsat) / progress.profilesTotal))
      : 0;
  return {
    ...EMPTY_VIEW,
    engine: 'z3',
    lowerBound: progress.lowerBound,
    nodeCount: progress.nodeCount,
    rejected: progress.rejectedUnstableCandidates,
    profilesTotal: progress.profilesTotal,
    profilesUnsat: progress.profilesUnsat,
    activeAttempts: progress.activeAttempts,
    attemptSlots: progress.attemptSlots,
    launchedAttempts: progress.launchedAttempts,
    abandoned: progress.abandonedAttempts,
    ruledOut: Math.max(0, progress.nodeCount - progress.lowerBound),
    ruledOutThrough:
      progress.nodeCount > progress.lowerBound ? progress.nodeCount - 1 : null,
    portfolioPct
  };
}

export function ruledOutRangeLabel(view: SearchStageView): string {
  if (view.lowerBound == null || view.ruledOutThrough == null) return '';
  if (view.ruledOutThrough === view.lowerBound) return String(view.lowerBound);
  return `${view.lowerBound}–${view.ruledOutThrough}`;
}

function isMutedStatus(status: JobSnapshot['status']): boolean {
  return (
    status === 'cancelled' ||
    status === 'failed' ||
    status === 'incomplete' ||
    status === 'unsat'
  );
}

export function sizeSearchBody(snapshot: JobSnapshot, view: SearchStageView): string {
  const muted = isMutedStatus(snapshot.status);
  if (view.engine === 'custom') {
    if (view.phase == null) {
      return muted ? 'Stopped before Custom progress started.' : 'Starting the Custom exact search.';
    }
    if (view.nodeCount == null) {
      return muted
        ? `Stopped during ${customPhaseLabel(view.phase).toLowerCase()}.`
        : `${customPhaseLabel(view.phase)}.`;
    }
    const link = view.linkCount != null ? ` · L=${view.linkCount}` : '';
    return muted
      ? `Stopped while searching N=${view.nodeCount}${link}.`
      : `Searching N=${view.nodeCount}${link} · ${view.profilesCompleted}${
          view.profilesTotal != null ? ` / ${view.profilesTotal}` : ''
        } profiles closed.`;
  }
  if (view.nodeCount == null) {
    return muted
      ? 'Stopped while preparing the exact model.'
      : 'Computing the smallest node count that could possibly work.';
  }
  if (view.ruledOutThrough == null) {
    return muted
      ? `Stopped while checking ${view.nodeCount}-node layouts.`
      : 'Searching the first feasible size from the lower bound.';
  }
  const range = ruledOutRangeLabel(view);
  if (muted) {
    return `Ruled out ${range} node layouts. Stopped while checking ${view.nodeCount}-node mixes.`;
  }
  if (view.ruledOutThrough === view.nodeCount) {
    return `No exact solution at ${range} nodes. Advancing to larger sizes.`;
  }
  return `No exact solution at ${range} nodes. Searching all ${view.nodeCount}-node splitter/merger mixes.`;
}

export function searchHeadline(
  snapshot: JobSnapshot,
  context: SearchCopyContext
): string {
  const { solutionsLength, searchEnumerate, firstNodeCount } = context;
  if (snapshot.status === 'cancelling') return 'Stopping the current proof…';
  if (snapshot.status === 'cancelled') {
    return solutionsLength > 0
      ? `Search cancelled · ${solutionsLength} layout${solutionsLength === 1 ? '' : 's'} kept`
      : 'Search cancelled';
  }
  if (snapshot.status === 'incomplete') {
    return solutionsLength > 0
      ? `Search incomplete · ${solutionsLength} layout${solutionsLength === 1 ? '' : 's'} kept`
      : 'Search incomplete';
  }
  if (snapshot.status === 'unsat') {
    return 'No exact network exists';
  }
  if (snapshot.status === 'failed') {
    return solutionsLength > 0
      ? `Search failed · ${solutionsLength} layout${solutionsLength === 1 ? '' : 's'} kept`
      : 'Search failed';
  }
  if (searchEnumerate && snapshot.status === 'completed' && solutionsLength > 0) {
    return firstNodeCount != null ? `All layouts at N = ${firstNodeCount}` : 'All layouts found';
  }
  if (searchEnumerate && solutionsLength > 0 && snapshot.status === 'running') {
    return firstNodeCount != null
      ? `Enumerating layouts at N = ${firstNodeCount}`
      : 'Enumerating layouts at proven N';
  }
  if (isCustomProgress(snapshot.progress)) {
    if (snapshot.progress.obligation?.nodeCount != null) {
      return `Checking ${nodeCountLabel(snapshot.progress.obligation.nodeCount)}`;
    }
    return customPhaseLabel(snapshot.progress.phase);
  }
  if (!snapshot.progress || (isZ3Progress(snapshot.progress) && snapshot.progress.kind === 'preparing')) {
    return 'Preparing the exact model';
  }
  if (isZ3Progress(snapshot.progress) && snapshot.progress.kind === 'candidate_rejected') {
    return `Advancing past ${nodeCountLabel(snapshot.progress.nodeCount)}`;
  }
  if (isZ3Progress(snapshot.progress) && 'nodeCount' in snapshot.progress) {
    const improving =
      'incumbentBeltCount' in snapshot.progress &&
      snapshot.progress.incumbentBeltCount != null &&
      snapshot.progress.maxOperatorBelts != null;
    if (improving) {
      return `Improving below ${snapshot.progress.incumbentBeltCount} belts at N=${snapshot.progress.nodeCount}`;
    }
    return `Checking ${snapshot.progress.nodeCount}-node layouts`;
  }
  return 'Searching';
}

export function searchSubline(
  snapshot: JobSnapshot,
  view: SearchStageView,
  context: SearchCopyContext
): string {
  const { solutionsLength, searchEnumerate, engine } = context;
  if (snapshot.status === 'cancelled') {
    return solutionsLength > 0
      ? 'Partial results kept below. Start a new search when ready.'
      : 'Last progress kept below. Start a new search when ready.';
  }
  if (snapshot.status === 'incomplete') {
    return solutionsLength > 0
      ? 'Resource-limited or timed out. Best-known layouts kept below — not proven optimal.'
      : 'Resource-limited or timed out before a witness was found.';
  }
  if (snapshot.status === 'unsat') {
    return 'Finite proof that no capacity-safe network satisfies these rates.';
  }
  if (snapshot.status === 'failed') {
    return solutionsLength > 0
      ? 'Partial results kept below — enumeration did not finish. Fix the issue and try again.'
      : 'Last progress kept below. Fix the issue and try again.';
  }
  if (snapshot.status === 'cancelling') {
    return engine === 'custom' || view.engine === 'custom'
      ? 'Waiting for the Custom search workers to stop.'
      : 'Waiting for the active Z3 attempts to stop.';
  }
  if (searchEnumerate && snapshot.status === 'completed' && solutionsLength > 0) {
    return `${solutionsLength} distinct topolog${solutionsLength === 1 ? 'y' : 'ies'} · exact · complete`;
  }
  if (searchEnumerate && solutionsLength > 0 && snapshot.status === 'running') {
    return `Minimal size proven. ${solutionsLength} distinct layout${solutionsLength === 1 ? '' : 's'} so far · still checking remaining profiles`;
  }
  if (view.engine === 'custom') {
    if (view.phase == null) return 'Exact Custom search is starting.';
    if (view.nodeCount == null) {
      return `${customPhaseLabel(view.phase)}.`;
    }
    return searchEnumerate
      ? `Custom search at N=${view.nodeCount} · will enumerate every distinct layout at the proven minimum`
      : `Custom search at N=${view.nodeCount} · proving lexicographic optimality`;
  }
  if (view.lowerBound == null) {
    return 'Normalizing rates and computing the combinatorial lower bound.';
  }
  if (view.nodeCount == null) {
    return `Lower bound is ${view.lowerBound} nodes. Starting the exact search.`;
  }
  return searchEnumerate
    ? `Started from lower bound ${view.lowerBound} · will enumerate every layout at the proven minimum`
    : `Started from lower bound ${view.lowerBound} · proving min nodes, then min operator belts`;
}
