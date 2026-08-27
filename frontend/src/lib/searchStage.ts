import type { Diagnostic, JobSnapshot, SolverProgress } from '../types';

export type SearchStageView = {
  phase: string | null;
  lowerBound: number | null;
  nodeCount: number | null;
  linkCount: number | null;
  linkKind: 'exact' | 'at_most' | null;
  bestNodeCount: number | null;
  bestLinkCount: number | null;
  ruledOutThrough: number | null;
  custom: Diagnostic[];
};

export type SearchCopyContext = {
  solutionsLength: number;
  searchEnumerate: boolean;
  firstNodeCount: number | null;
  engine?: 'custom' | 'z3';
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

function phaseLabel(phase: string | null): string {
  switch (phase) {
    case 'normalizing': return 'Normalizing rates';
    case 'global_checks': return 'Running global checks';
    case 'computing_lower_bound': return 'Computing lower bound';
    case 'validating_witness': return 'Validating witness';
    case 'optimizing_links': return 'Improving operator belt count';
    case 'enumerating': return 'Enumerating layouts';
    case 'searching': return 'Searching layouts';
    default: return 'Preparing the exact model';
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
    custom: progress?.custom ?? []
  };
}

export function diagnosticText(entry: Diagnostic): string {
  return `${String(entry.value.value)}${entry.unit ? ` ${entry.unit}` : ''}`;
}

/** A local profile count, never an estimate of overall solve completion. */
export function diagnosticProgress(entries: Diagnostic[]): {
  label: string;
  closed: string;
  total: string;
  percent: number;
} | null {
  const counter = (name: string): bigint | null => {
    const value = entries.find(entry => entry.name === name)?.value;
    return value?.type === 'integer' && /^\d+$/.test(value.value) ? BigInt(value.value) : null;
  };
  for (const [closedName, totalName, label] of [
    ['custom.completed_profiles', 'custom.total_profiles', 'Profiles closed in this L group'],
    ['z3.profiles_unsat', 'z3.profiles_total', 'Profiles proved UNSAT in this search']
  ]) {
    const closed = counter(closedName);
    const total = counter(totalName);
    if (closed == null || total == null || total === 0n || closed > total) continue;
    return {
      label, closed: closed.toString(), total: total.toString(),
      percent: Number(closed * 10_000n / total) / 100
    };
  }
  return null;
}

export function ruledOutRangeLabel(view: SearchStageView): string {
  return view.ruledOutThrough == null ? '' : view.ruledOutThrough === 0 ? '0' : `0–${view.ruledOutThrough}`;
}

export function sizeSearchBody(snapshot: JobSnapshot, view: SearchStageView): string {
  const stopped = ['cancelled', 'failed', 'incomplete', 'unsat'].includes(snapshot.status);
  if (view.nodeCount == null) return `${stopped ? 'Stopped during' : ''} ${phaseLabel(view.phase).toLowerCase()}.`.trim();
  const link = view.linkCount == null ? '' : ` · L${view.linkKind === 'at_most' ? '≤' : '='}${view.linkCount}`;
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
  if (progress?.phase === 'optimizing_links' && progress.bestLinkCount != null) {
    return `Improving below ${progress.bestLinkCount} belts at N=${progress.nodeCount}`;
  }
  if (searchEnumerate && solutionsLength > 0) return `Enumerating layouts at N = ${firstNodeCount ?? progress?.nodeCount ?? '?'}`;
  if (progress?.nodeCount != null) return `Checking ${nodeCountLabel(progress.nodeCount)}`;
  return phaseLabel(progress?.phase ?? null);
}

export function searchSubline(snapshot: JobSnapshot, view: SearchStageView, context: SearchCopyContext): string {
  if (snapshot.status === 'cancelling') return `Waiting for ${context.engine === 'custom' ? 'Custom' : 'Z3'} workers to stop.`;
  if (snapshot.status === 'unsat') return 'Finite proof that no capacity-safe network satisfies these rates.';
  if (['cancelled', 'incomplete', 'failed'].includes(snapshot.status)) {
    return snapshot.proof?.minimumNodeCount != null
      ? `Minimum N=${snapshot.proof.minimumNodeCount} proved. Partial results kept; unfinished proofs remain incomplete.`
      : 'Last progress and any validated witnesses kept. The requested proof did not finish.';
  }
  if (context.searchEnumerate && snapshot.enumerationComplete) return `${context.solutionsLength} distinct layouts · exact · complete`;
  if (view.lowerBound == null) return `${phaseLabel(view.phase)}.`;
  return `Lower bound is ${view.lowerBound} nodes. ${context.searchEnumerate ? 'Collecting every layout at the minimum size.' : 'Proving minimum nodes, then minimum operator belts.'}`;
}
