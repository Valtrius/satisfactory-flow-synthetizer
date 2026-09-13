import type { Edge, Node } from '@xyflow/svelte';
import type {
  EndpointRow,
  Solution,
  SolveRequest,
  SolverProgress,
  OptimalityProof,
  JobSnapshot,
  SolveMode,
  CollectionRef,
} from '../types';
import { enumeratesLayouts, parseSolveMode, SOLVE_MODE_LABELS } from '../types';
import { DEFAULT_SORT_COLUMNS, type SortColumn } from './solutionSort';

export const HISTORY_DOCUMENT_VERSION = 2;

export type HistoryBand = 'queued' | 'running' | 'history';

export type HistoryEntryStatus =
  'queued' | 'running' | 'cancelling' | 'completed' | 'cancelled' | 'incomplete' | 'unsat' | 'failed';

export interface FormSnapshot {
  inputs: EndpointRow[];
  outputs: EndpointRow[];
  beltRate: string;
  solveMode: SolveMode;
}

export interface CachedGraphLayout {
  layoutKey: string;
  nodes: Node[];
  edges: Edge[];
}

export interface HistoryEntry {
  id: string;
  /** Null means derive from the frozen request. */
  title: string | null;
  createdAtMs: number;
  updatedAtMs: number;
  status: HistoryEntryStatus;
  request: SolveRequest;
  form: FormSnapshot;
  jobId: string | null;
  startedAtMs: number | null;
  /** Set once when the job becomes terminal; used for frozen elapsed time. */
  finishedAtMs: number | null;
  progress: SolverProgress | null;
  proof?: OptimalityProof | null;
  sequence?: number;
  result: Solution | null;
  results: Solution[];
  collection?: CollectionRef;
  enumerationComplete: boolean;
  error: string | null;
  selectedSourceIndex: number;
  sortColumns: SortColumn[];
  /** Layout cache keyed by solution source index. */
  layouts: Record<string, CachedGraphLayout>;
}

export interface HistoryDocument {
  version: number;
  entries: HistoryEntry[];
  selectedEntryId: string | null;
}

export function entryToJobSnapshot(entry: HistoryEntry): JobSnapshot {
  const status = entry.status === 'queued' ? 'running' : entry.status;
  return {
    jobId: entry.jobId ?? entry.id,
    status,
    startedAtMs: entry.startedAtMs ?? entry.createdAtMs,
    progress: entry.progress,
    proof: entry.proof,
    sequence: entry.sequence,
    result: entry.result,
    results: entry.results,
    collection: entry.collection,
    enumerationComplete: entry.enumerationComplete,
    error: entry.error,
  };
}

export function isTerminalJobStatus(status: string | undefined): boolean {
  return (
    status === 'completed' ||
    status === 'cancelled' ||
    status === 'incomplete' ||
    status === 'unsat' ||
    status === 'failed'
  );
}

/** Frozen solve duration for terminal entries; live clock for running/cancelling. */
export function entryElapsedMs(entry: HistoryEntry, now = Date.now()): number {
  if (!entry.startedAtMs) return 0;
  if (entry.status === 'running' || entry.status === 'cancelling') {
    return Math.max(0, now - entry.startedAtMs);
  }
  const finishedAt = entry.finishedAtMs ?? entry.updatedAtMs;
  return Math.max(0, finishedAt - entry.startedAtMs);
}

export function snapshotForm(
  inputs: EndpointRow[],
  outputs: EndpointRow[],
  beltRate: string,
  solveMode: SolveMode,
): FormSnapshot {
  return {
    inputs: inputs.map((row) => ({ ...row })),
    outputs: outputs.map((row) => ({ ...row })),
    beltRate,
    solveMode,
  };
}

export function newEntryId(): string {
  return crypto.randomUUID();
}

export function cloneForm(form: FormSnapshot): FormSnapshot {
  return {
    inputs: form.inputs.map((row) => ({ ...row })),
    outputs: form.outputs.map((row) => ({ ...row })),
    beltRate: form.beltRate,
    solveMode: form.solveMode,
  };
}

export function createQueuedEntry(form: FormSnapshot, request: SolveRequest): HistoryEntry {
  const now = Date.now();
  return {
    id: newEntryId(),
    title: null,
    createdAtMs: now,
    updatedAtMs: now,
    status: 'queued',
    request,
    form: cloneForm(form),
    jobId: null,
    startedAtMs: null,
    finishedAtMs: null,
    progress: null,
    result: null,
    results: [],
    enumerationComplete: false,
    error: null,
    selectedSourceIndex: 0,
    sortColumns: [...DEFAULT_SORT_COLUMNS],
    layouts: {},
  };
}

export function entryBand(status: HistoryEntryStatus): HistoryBand {
  if (status === 'queued') return 'queued';
  if (status === 'running' || status === 'cancelling') return 'running';
  return 'history';
}

export function partitionEntries(entries: HistoryEntry[]): {
  queued: HistoryEntry[];
  running: HistoryEntry | null;
  history: HistoryEntry[];
} {
  const queued: HistoryEntry[] = [];
  let running: HistoryEntry | null = null;
  const history: HistoryEntry[] = [];
  for (const entry of entries) {
    const band = entryBand(entry.status);
    if (band === 'queued') queued.push(entry);
    else if (band === 'running') running = running ?? entry;
    else history.push(entry);
  }
  return { queued, running, history };
}

/**
 * Rebuild document order: queued → running → history.
 * Queued is newest-first (top of the list); the runner drains from the bottom.
 */
export function assembleEntries(
  queued: HistoryEntry[],
  running: HistoryEntry | null,
  history: HistoryEntry[],
): HistoryEntry[] {
  return running ? [...queued, running, ...history] : [...queued, ...history];
}

export function reorderWithinBand(
  entries: HistoryEntry[],
  band: 'queued' | 'history',
  fromId: string,
  toId: string,
): HistoryEntry[] {
  if (fromId === toId) return entries;
  const parts = partitionEntries(entries);
  const list = band === 'queued' ? [...parts.queued] : [...parts.history];
  const fromIndex = list.findIndex((entry) => entry.id === fromId);
  const toIndex = list.findIndex((entry) => entry.id === toId);
  if (fromIndex < 0 || toIndex < 0) return entries;
  const [moved] = list.splice(fromIndex, 1);
  list.splice(toIndex, 0, moved);
  return assembleEntries(
    band === 'queued' ? list : parts.queued,
    parts.running,
    band === 'history' ? list : parts.history,
  );
}

export function displayTitle(entry: HistoryEntry): string {
  const custom = entry.title?.trim();
  if (custom) return custom;
  return defaultTitle(entry.request);
}

export function defaultTitle(request: SolveRequest): string {
  const inputs = request.inputs.map((item) => item.rate.trim() || '?');
  const outputs = request.outputs.map((item) => item.rate.trim() || '?');
  const left = inputs.length > 0 ? inputs.join(' + ') : 'auto';
  const right = outputs.length > 0 ? outputs.join(' + ') : '?';
  return `${left} = ${right}`;
}

export function entryLayoutCount(entry: HistoryEntry): number {
  if (entry.collection) return entry.collection.count;
  if (entry.results.length > 0) return entry.results.length;
  return entry.result ? 1 : 0;
}

export function entryNodeCount(entry: HistoryEntry): number | null {
  const fromResult = entry.results[0]?.stats.nodeCount ?? entry.result?.stats.nodeCount ?? null;
  if (fromResult != null) return fromResult;
  const progress = entry.progress;
  if (!progress) return null;
  return progress.nodeCount ?? null;
}

export type HistoryMetricCell = {
  value: string;
  tip: string;
};

export type HistoryMetrics = {
  search: HistoryMetricCell;
  belts: HistoryMetricCell;
  nodes: HistoryMetricCell;
  layouts: HistoryMetricCell;
};

/** Compact icon-grid metrics for history cards. */
export function entryHistoryMetrics(entry: HistoryEntry): HistoryMetrics {
  const mode = entry.request.solveMode;
  const nodeCount = entryNodeCount(entry);
  const layoutCount = entryLayoutCount(entry);
  const solutions = [entry.result, ...entry.results];
  const proved = solutions.find((s) => s?.status === 'proven_optimal');
  const minimumL = entry.proof?.minimumLinkCount ?? proved?.stats.linkCount ?? null;
  // Found layouts arrive before the terminal proof. All min N can then search
  // larger L, so retain the best found count rather than the current constraint.
  let bestL = entry.progress?.bestNodeCount === nodeCount ? (entry.progress.bestLinkCount ?? null) : null;
  for (const solution of solutions) {
    if (solution?.stats.nodeCount === nodeCount) {
      bestL = bestL == null ? solution.stats.linkCount : Math.min(bestL, solution.stats.linkCount);
    }
  }
  const linkCount = minimumL ?? bestL;

  return {
    search: { value: SOLVE_MODE_LABELS[mode], tip: `Search: ${SOLVE_MODE_LABELS[mode]}` },
    belts: {
      value: linkCount != null ? `L=${linkCount}` : 'L=—',
      tip:
        minimumL != null
          ? `Minimum operator belt count L = ${minimumL}`
          : bestL != null
            ? `Best operator belt count found L = ${bestL}`
            : 'Minimum operator belt count not yet proved',
    },
    nodes: {
      value: nodeCount != null ? `N=${nodeCount}` : 'N=—',
      tip: nodeCount != null ? `Node count N = ${nodeCount}` : 'Node count unknown until solved',
    },
    layouts: {
      value: String(layoutCount),
      tip: layoutCount === 0 ? 'No layouts yet' : `${layoutCount} layout${layoutCount === 1 ? '' : 's'} found`,
    },
  };
}

/** Short status line under the title (metrics carry scope/L/N/layouts). */
export function entryStatusCaption(entry: HistoryEntry): string {
  switch (entry.status) {
    case 'queued':
      return 'Queued';
    case 'running':
      return '';
    case 'cancelling':
      return 'Stopping…';
    case 'failed':
      return 'Failed';
    case 'unsat':
      return 'Globally impossible';
    case 'incomplete':
      return 'Incomplete';
    case 'cancelled':
      return 'Cancelled';
    case 'completed': {
      // Enumeration stores every extra layout as `best_known`. Job completion is
      // independent of that: once the requested scope is exhausted, the entry is done.
      if (entry.enumerationComplete || enumeratesLayouts(entry.request.solveMode)) {
        return 'Completed';
      }
      const solutionStatus = entry.result?.status ?? entry.results[0]?.status;
      return solutionStatus === 'best_known' ? 'Best known' : 'Completed';
    }
    default:
      return '';
  }
}

/** Compact outcome line for history rows (no status pills). */
export function entryOutcomeLine(entry: HistoryEntry): string {
  const layoutCount = entryLayoutCount(entry);
  const nodeCount = entryNodeCount(entry);

  if (entry.status === 'queued') {
    const mode = entry.request.solveMode;
    return mode === 'one_min_nl' ? 'One min N/L' : mode === 'all_min_nl' ? 'All min N/L' : 'All min N';
  }
  if (entry.status === 'running' || entry.status === 'cancelling') {
    return entry.status === 'cancelling' ? 'Stopping…' : '';
  }
  if (entry.status === 'failed') {
    return layoutCount > 0 ? `Failed · ${layoutCount} layout${layoutCount === 1 ? '' : 's'} kept` : 'Failed';
  }
  if (entry.status === 'unsat') {
    return 'Globally impossible';
  }
  if (entry.status === 'incomplete') {
    return layoutCount > 0
      ? `Incomplete · ${layoutCount} layout${layoutCount === 1 ? '' : 's'} kept${
          nodeCount != null ? ` · N=${nodeCount}` : ''
        }`
      : 'Incomplete';
  }
  if (entry.status === 'cancelled') {
    if (layoutCount === 0) return 'Cancelled';
    return `Cancelled · ${layoutCount} layout${layoutCount === 1 ? '' : 's'}${
      nodeCount != null ? ` · N=${nodeCount}` : ''
    }`;
  }
  // completed
  if (entry.enumerationComplete && (enumeratesLayouts(entry.request.solveMode) || layoutCount > 1)) {
    const scope = entry.request.solveMode === 'all_min_nl' ? 'All min N/L' : 'All layouts';
    return `${scope}${nodeCount != null ? ` · N=${nodeCount}` : ''} · ${layoutCount}`;
  }
  if (layoutCount <= 1) {
    const status = entry.result?.status;
    if (status === 'best_known') {
      return `Best known${nodeCount != null ? ` · N=${nodeCount}` : ''}`;
    }
    return `Optimal${nodeCount != null ? ` · N=${nodeCount}` : ''}`;
  }
  return `${layoutCount} layouts${nodeCount != null ? ` · N=${nodeCount}` : ''}`;
}

export function emptyDocument(): HistoryDocument {
  return {
    version: HISTORY_DOCUMENT_VERSION,
    entries: [],
    selectedEntryId: null,
  };
}

/** Restore compatible telemetry and normalize its diagnostic names. */
function savedProgress(progress: SolverProgress | null | undefined): SolverProgress | null {
  return progress &&
    typeof progress.phase === 'string' &&
    Number.isFinite(progress.elapsedMs) &&
    Array.isArray(progress.custom) &&
    progress.custom.every(
      (entry) =>
        entry &&
        typeof entry.name === 'string' &&
        typeof entry.label === 'string' &&
        entry.value &&
        (entry.value.type === 'boolean'
          ? typeof entry.value.value === 'boolean'
          : ['integer', 'text', 'rate'].includes(entry.value.type) && typeof entry.value.value === 'string'),
    )
    ? {
        ...progress,
        phase: ['computing_lower_bound', 'searching', 'enumerating'].includes(progress.phase)
          ? progress.phase
          : 'searching',
        linkConstraint: progress.linkConstraint?.kind === 'exact' ? progress.linkConstraint : null,
        custom: progress.custom.flatMap((diagnostic) => {
          const name = diagnostic.name.replace(/^astra\./, 'solver.');
          return name.startsWith('solver.')
            ? [{ ...diagnostic, name, label: diagnostic.label.replace(/\bAstra\b/g, 'Solver') }]
            : [];
        }),
      }
    : null;
}

/** Checkpoint active work as incomplete, never as resumable or completed work. */
export function persistableEntries(entries: HistoryEntry[]): HistoryEntry[] {
  return entries
    .filter((entry) => entry.status !== 'queued')
    .map((entry) => {
      const normalized = normalizeEntry(entry);
      const interrupted = entry.status === 'running' || entry.status === 'cancelling';
      return {
        ...normalized,
        status: interrupted ? ('incomplete' as const) : normalized.status,
        enumerationComplete: interrupted ? false : normalized.enumerationComplete,
        finishedAtMs: interrupted ? entry.updatedAtMs : normalized.finishedAtMs,
        error: interrupted ? 'Interrupted before completion. Last saved checkpoint.' : normalized.error,
        jobId: null,
        sortColumns: normalized.sortColumns.map((column) => ({ ...column })),
        layouts: jsonClone(normalized.layouts),
      };
    });
}

function jsonClone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

export function parseHistoryDocument(raw: unknown): HistoryDocument {
  if (!raw || typeof raw !== 'object') return emptyDocument();
  const doc = raw as Partial<HistoryDocument>;
  if (!Array.isArray(doc.entries)) return emptyDocument();
  const entries = doc.entries
    .filter((entry): entry is HistoryEntry => Boolean(entry && typeof entry === 'object' && entry.id))
    .map((entry) => normalizeEntry(entry));
  return {
    version: HISTORY_DOCUMENT_VERSION,
    entries: persistableEntries(entries),
    selectedEntryId:
      typeof doc.selectedEntryId === 'string' && entries.some((entry) => entry.id === doc.selectedEntryId)
        ? doc.selectedEntryId
        : (entries[0]?.id ?? null),
  };
}

function readSolveMode(raw: unknown): SolveMode {
  const value = asRecord(raw);
  return parseSolveMode(value.solveMode, value.enumerateAllAtN === true ? 'all_min_n' : 'one_min_nl');
}

function asRecord(raw: unknown): Record<string, unknown> {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return {};
  return raw as Record<string, unknown>;
}

function normalizeEntry(entry: HistoryEntry): HistoryEntry {
  const requestMode = readSolveMode(entry.request);
  const formMode = entry.form ? readSolveMode(entry.form) : requestMode;
  const { engine: _ignoredRequestEngine, enumerateAllAtN: _ignoredRequestMode, ...request } = asRecord(entry.request);
  const { enumerateAllAtN: _ignoredFormMode, ...form } = asRecord(entry.form);
  return {
    id: entry.id,
    title: entry.title ?? null,
    createdAtMs: entry.createdAtMs ?? Date.now(),
    updatedAtMs: entry.updatedAtMs ?? Date.now(),
    status: entry.status ?? 'completed',
    request: {
      ...request,

      solveMode: requestMode,
    } as SolveRequest,
    form: entry.form
      ? cloneForm({ ...(form as unknown as FormSnapshot), solveMode: formMode })
      : {
          inputs: [],
          outputs: [],
          beltRate: entry.request?.beltRate ?? '1200',
          solveMode: requestMode,
        },
    jobId: null,
    startedAtMs: entry.startedAtMs ?? null,
    finishedAtMs:
      entry.finishedAtMs ??
      (entryBand(entry.status ?? 'completed') === 'history' && entry.startedAtMs
        ? (entry.updatedAtMs ?? Date.now())
        : null),
    progress: savedProgress(entry.progress),
    proof: entry.proof ?? null,
    sequence: entry.sequence,
    result: entry.result ? stripSolverType(entry.result) : null,
    results: Array.isArray(entry.results) ? entry.results.map(stripSolverType) : [],
    collection: entry.collection ? { ...entry.collection } : undefined,
    enumerationComplete: Boolean(entry.enumerationComplete),
    error: entry.error ?? null,
    selectedSourceIndex: entry.selectedSourceIndex ?? 0,
    sortColumns:
      Array.isArray(entry.sortColumns) && entry.sortColumns.length > 0 ? entry.sortColumns : [...DEFAULT_SORT_COLUMNS],
    layouts: Object.fromEntries(
      Object.entries(entry.layouts ?? {}).map(([index, layout]) => [
        index,
        { ...layout, layoutKey: layout.layoutKey.replace(/^(custom|z3|astra)::/, '') },
      ]),
    ),
  };
}

export type HistoryExportPayload =
  | { kind: 'history-entry'; version: number; entry: HistoryEntry }
  | { kind: 'history-bundle'; version: number; entries: HistoryEntry[] };

export function exportEntryPayload(entry: HistoryEntry): HistoryExportPayload {
  if (entry.collection) throw new Error('Paged history must be exported through the collection reader.');
  const cleaned = persistableEntries([entry])[0];
  return {
    kind: 'history-entry',
    version: HISTORY_DOCUMENT_VERSION,
    entry: cleaned,
  };
}

export function exportBundlePayload(entries: HistoryEntry[]): HistoryExportPayload {
  if (entries.some((entry) => entry.collection))
    throw new Error('Paged history must be exported through the collection reader.');
  return {
    kind: 'history-bundle',
    version: HISTORY_DOCUMENT_VERSION,
    entries: persistableEntries(entries),
  };
}

export function mergeImportedPayload(
  current: HistoryEntry[],
  payload: unknown,
): { entries: HistoryEntry[]; importedIds: string[] } {
  const imported = extractImportEntries(payload).map((entry) => remapEntry(entry));
  return {
    entries: [...current, ...imported],
    importedIds: imported.map((entry) => entry.id),
  };
}

function extractImportEntries(payload: unknown): HistoryEntry[] {
  if (!payload || typeof payload !== 'object') return [];
  const data = payload as Partial<HistoryExportPayload> & {
    entries?: HistoryEntry[];
    entry?: HistoryEntry;
  };
  if (data.kind === 'history-entry' && data.entry) return [normalizeEntry(data.entry)];
  if (data.kind === 'history-bundle' && Array.isArray(data.entries)) {
    return data.entries.map((entry) => normalizeEntry(entry));
  }
  // Bare entry or array fallbacks
  if (Array.isArray(data)) return (data as HistoryEntry[]).map((entry) => normalizeEntry(entry));
  if ('request' in data && 'id' in data) return [normalizeEntry(data as HistoryEntry)];
  if (Array.isArray(data.entries)) return data.entries.map((entry) => normalizeEntry(entry));
  return [];
}

function remapEntry(entry: HistoryEntry): HistoryEntry {
  if (entry.collection)
    throw new Error('Local collection references cannot be imported. Export the full history JSON first.');
  const now = Date.now();
  return {
    ...normalizeEntry(entry),
    id: newEntryId(),
    createdAtMs: now,
    updatedAtMs: now,
    jobId: null,
    status: entryBand(entry.status) === 'history' ? entry.status : 'completed',
  };
}

function stripSolverType(solution: Solution): Solution {
  const { engine: _ignored, ...rest } = solution as Solution & { engine?: unknown };
  return rest;
}
