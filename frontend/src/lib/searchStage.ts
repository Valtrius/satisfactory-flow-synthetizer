import type { Diagnostic, JobSnapshot, SolverProgress } from '../types';

export type SearchStageView = {
  phase: string | null;
  lowerBound: number | null;
  nodeCount: number | null;
  linkCount: number | null;
  linkKind: 'exact' | null;
  bestNodeCount: number | null;
  bestLinkCount: number | null;
  ruledOutThrough: number | null;
  custom: Diagnostic[];
};

export type SearchCopyContext = {
  solutionsLength: number;
  searchEnumerate: boolean;
  firstNodeCount: number | null;
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
  return `${formatTelemetryNumber(count)} ${count === 1 ? 'node' : 'nodes'}`;
}

/** Group integer strings directly so u64 diagnostics never lose precision. */
export function formatTelemetryNumber(value: number | string | null): string {
  if (value == null) return '—';
  const text = String(value);
  return /^-?\d+$/.test(text) ? text.replace(/\B(?=(\d{3})+(?!\d))/g, "'") : text;
}

function phaseLabel(phase: string | null): string {
  switch (phase) {
    case 'computing_lower_bound':
      return 'Computing lower bound';
    case 'enumerating':
      return 'Enumerating layouts';
    case 'searching':
      return 'Searching layouts';
    default:
      return 'Preparing the exact model';
  }
}

export function searchStageView(progress: SolverProgress | null): SearchStageView {
  const lowerBound = progress?.nodeLowerBound ?? null;
  return {
    phase: progress?.phase ?? null,
    lowerBound,
    nodeCount: progress?.nodeCount ?? null,
    linkCount: progress?.linkConstraint?.value ?? null,
    linkKind: progress?.linkConstraint?.kind ?? null,
    bestNodeCount: progress?.bestNodeCount ?? null,
    bestLinkCount: progress?.bestLinkCount ?? null,
    ruledOutThrough: lowerBound != null && lowerBound > 0 ? lowerBound - 1 : null,
    custom: progress?.custom ?? [],
  };
}

export function diagnosticText(entry: Diagnostic): string {
  const value =
    entry.value.type === 'integer'
      ? formatTelemetryNumber(entry.value.value)
      : entry.value.type === 'rate'
        ? entry.value.value.split('/').map(formatTelemetryNumber).join('/')
        : String(entry.value.value);
  return `${value}${entry.unit ? ` ${entry.unit}` : ''}`;
}

export function ruledOutRangeLabel(view: SearchStageView): string {
  return view.ruledOutThrough == null ? '' : view.ruledOutThrough === 0 ? '0' : `0–${view.ruledOutThrough}`;
}

export function sizeSearchBody(snapshot: JobSnapshot, view: SearchStageView): string {
  const stopped = ['cancelled', 'failed', 'incomplete', 'unsat'].includes(snapshot.status);
  if (view.nodeCount == null)
    return `${stopped ? 'Stopped during' : ''} ${phaseLabel(view.phase).toLowerCase()}.`.trim();
  const link = view.linkCount == null ? '' : ` · L=${view.linkCount}`;
  return `${stopped ? 'Stopped while searching' : 'Searching'} N=${view.nodeCount}${link}.`;
}

export function searchHeadline(snapshot: JobSnapshot, context: SearchCopyContext): string {
  const { solutionsLength, searchEnumerate, firstNodeCount } = context;
  if (snapshot.status === 'cancelling') return 'Stopping the current proof…';
  if (snapshot.status === 'unsat') return 'No exact network exists';
  if (['cancelled', 'incomplete', 'failed'].includes(snapshot.status)) {
    return `Search ${snapshot.status}${solutionsLength > 0 ? ` · ${solutionsLength} layout${solutionsLength === 1 ? '' : 's'} kept` : ''}`;
  }
  if (searchEnumerate && snapshot.enumerationComplete && snapshot.status === 'completed') {
    return firstNodeCount != null ? `All layouts at N = ${firstNodeCount}` : 'All layouts found';
  }
  const progress = snapshot.progress;
  if (searchEnumerate && solutionsLength > 0)
    return `Enumerating layouts at N = ${firstNodeCount ?? progress?.nodeCount ?? '?'}`;
  if (progress?.nodeCount != null) return `Checking ${nodeCountLabel(progress.nodeCount)}`;
  return phaseLabel(progress?.phase ?? null);
}

export function searchSubline(snapshot: JobSnapshot, view: SearchStageView, context: SearchCopyContext): string {
  if (snapshot.status === 'cancelling') return 'Waiting for solver workers to stop.';
  if (snapshot.status === 'unsat') return 'Finite proof that no capacity-safe network satisfies these rates.';
  if (['cancelled', 'incomplete', 'failed'].includes(snapshot.status)) {
    return snapshot.proof?.minimumNodeCount != null
      ? `Minimum N=${snapshot.proof.minimumNodeCount} proved. Partial results kept; unfinished proofs remain incomplete.`
      : 'Last progress and any validated witnesses kept. The requested proof did not finish.';
  }
  if (context.searchEnumerate && snapshot.enumerationComplete)
    return `${context.solutionsLength} distinct layouts · exact · complete`;
  if (view.lowerBound == null) return `${phaseLabel(view.phase)}.`;
  return `Lower bound is ${view.lowerBound} nodes. ${context.searchEnumerate ? 'Collecting every layout at the minimum size.' : 'Proving minimum nodes, then minimum operator belts.'}`;
}
