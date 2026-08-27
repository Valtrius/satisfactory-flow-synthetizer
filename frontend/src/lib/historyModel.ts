import type { Edge, Node } from '@xyflow/svelte';
import type {
  EndpointRow,
  Solution,
  SolveRequest,
  SolverEngine,
  SolverProgress,
  OptimalityProof,
  JobSnapshot,
} from '../types';
import { DEFAULT_SORT_COLUMNS, type SortColumn } from './solutionSort';

export const HISTORY_DOCUMENT_VERSION = 1;

export type HistoryBand = 'queued' | 'running' | 'history';

export type HistoryEntryStatus =
  'queued' | 'running' | 'cancelling' | 'completed' | 'cancelled' | 'incomplete' | 'unsat' | 'failed';

export interface FormSnapshot {
  inputs: EndpointRow[];
  outputs: EndpointRow[];
  beltRate: string;
  enumerateAllAtN: boolean;
  engine: SolverEngine;
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
  enumerateAllAtN: boolean,
  engine: SolverEngine = 'custom',
): FormSnapshot {
  return {
    inputs: inputs.map((row) => ({ ...row })),
    outputs: outputs.map((row) => ({ ...row })),
    beltRate,
    enumerateAllAtN,
    engine,
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
    enumerateAllAtN: form.enumerateAllAtN,
    engine: form.engine ?? 'custom',
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
  engine: HistoryMetricCell;
  nodes: HistoryMetricCell;
  layouts: HistoryMetricCell;
};

/** Compact icon-grid metrics for history cards. */
export function entryHistoryMetrics(entry: HistoryEntry): HistoryMetrics {
  const allLayouts = Boolean(entry.request.enumerateAllAtN);
  const engine = entry.request.engine === 'z3' ? 'Z3' : 'Custom';
  const nodeCount = entryNodeCount(entry);
  const layoutCount = entryLayoutCount(entry);

  return {
    search: {
      value: allLayouts ? 'All' : 'Opt',
      tip: allLayouts ? 'Search: Find all layouts at N' : 'Search: Find optimal layout',
    },
    engine: {
      value: engine,
      tip: `Engine: ${engine}`,
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

/** Short status line under the title (metrics carry search/engine/N/layouts). */
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
    return entry.request.enumerateAllAtN ? 'All layouts' : 'Optimal';
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
  const engineLabel = entry.request.engine === 'z3' ? 'Z3' : 'Custom';
  if (entry.enumerationComplete && (entry.request.enumerateAllAtN || layoutCount > 1)) {
    return `All layouts${nodeCount != null ? ` · N=${nodeCount}` : ''} · ${layoutCount} · ${engineLabel}`;
  }
  if (layoutCount <= 1) {
    const status = entry.result?.status;
    if (status === 'best_known') {
      return `Best known${nodeCount != null ? ` · N=${nodeCount}` : ''} · ${engineLabel}`;
    }
    return `Optimal${nodeCount != null ? ` · N=${nodeCount}` : ''} · ${engineLabel}`;
  }
  return `${layoutCount} layouts${nodeCount != null ? ` · N=${nodeCount}` : ''} · ${engineLabel}`;
}

export function emptyDocument(): HistoryDocument {
  return {
    version: HISTORY_DOCUMENT_VERSION,
    entries: [],
    selectedEntryId: null,
  };
}

/** Old engine-specific telemetry is not compatible with the common snapshot. */
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
    ? progress
    : null;
}

/** Entries safe to write across sessions (no live/queued work). */
export function persistableEntries(entries: HistoryEntry[]): HistoryEntry[] {
  return entries
    .filter((entry) => entryBand(entry.status) === 'history')
    .map((entry) => ({
      ...entry,
      jobId: null,
      progress: savedProgress(entry.progress),
      form: cloneForm(entry.form),
      sortColumns: entry.sortColumns.map((column) => ({ ...column })),
      // JSON round-trip: layouts must be IPC-serializable (no proxies / functions).
      layouts: jsonClone(entry.layouts),
    }));
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

function normalizeEntry(entry: HistoryEntry): HistoryEntry {
  const engine = entry.request?.engine ?? entry.form?.engine ?? 'custom';
  return {
    id: entry.id,
    title: entry.title ?? null,
    createdAtMs: entry.createdAtMs ?? Date.now(),
    updatedAtMs: entry.updatedAtMs ?? Date.now(),
    status: entry.status ?? 'completed',
    request: {
      ...entry.request,
      engine,
    },
    form: entry.form
      ? cloneForm({ ...entry.form, engine: entry.form.engine ?? engine })
      : {
          inputs: [],
          outputs: [],
          beltRate: entry.request?.beltRate ?? '1200',
          enumerateAllAtN: Boolean(entry.request?.enumerateAllAtN),
          engine,
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
    result: entry.result ?? null,
    results: Array.isArray(entry.results) ? entry.results : [],
    enumerationComplete: Boolean(entry.enumerationComplete),
    error: entry.error ?? null,
    selectedSourceIndex: entry.selectedSourceIndex ?? 0,
    sortColumns:
      Array.isArray(entry.sortColumns) && entry.sortColumns.length > 0 ? entry.sortColumns : [...DEFAULT_SORT_COLUMNS],
    layouts: entry.layouts && typeof entry.layouts === 'object' ? entry.layouts : {},
  };
}

export type HistoryExportPayload =
  | { kind: 'history-entry'; version: number; entry: HistoryEntry }
  | { kind: 'history-bundle'; version: number; entries: HistoryEntry[] };

export function exportEntryPayload(entry: HistoryEntry): HistoryExportPayload {
  const cleaned = persistableEntries([entry])[0];
  return {
    kind: 'history-entry',
    version: HISTORY_DOCUMENT_VERSION,
    entry: cleaned,
  };
}

export function exportBundlePayload(entries: HistoryEntry[]): HistoryExportPayload {
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
