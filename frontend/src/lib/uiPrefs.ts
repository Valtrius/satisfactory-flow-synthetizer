import type { EndpointRow, SolverEngine } from '../types';

export const UI_PREFS_STORAGE_KEY = 'sfs.ui-prefs.v1';
export const UI_PREFS_VERSION = 1 as const;

export type HistoryStatusFilter =
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'incomplete'
  | 'unsat';

export type HistoryEngineFilter = 'custom' | 'z3';
export type HistorySearchFilter = 'opt' | 'all';

export type HistorySortPref =
  | 'manual'
  | 'newest'
  | 'oldest'
  | 'name-asc'
  | 'name-desc'
  | 'layouts-desc'
  | 'nodes-asc';

export type HistoryToolbarPrefs = {
  query: string;
  sort: HistorySortPref;
  statusFilters: HistoryStatusFilter[];
  engineFilters: HistoryEngineFilter[];
  searchFilters: HistorySearchFilter[];
};

export type FormDraftPrefs = {
  inputs: EndpointRow[];
  outputs: EndpointRow[];
  beltRate: string;
  enumerateAllAtN: boolean;
  engine: SolverEngine;
  nextEndpointId: number;
};

export type UiPrefs = {
  version: typeof UI_PREFS_VERSION;
  history: HistoryToolbarPrefs;
  form: FormDraftPrefs;
};

const STATUS_FILTERS = new Set<HistoryStatusFilter>([
  'completed',
  'failed',
  'cancelled',
  'incomplete',
  'unsat'
]);

const ENGINE_FILTERS = new Set<HistoryEngineFilter>(['custom', 'z3']);
const SEARCH_FILTERS = new Set<HistorySearchFilter>(['opt', 'all']);
const SORT_PREFS = new Set<HistorySortPref>([
  'manual',
  'newest',
  'oldest',
  'name-asc',
  'name-desc',
  'layouts-desc',
  'nodes-asc'
]);

export const DEFAULT_HISTORY_TOOLBAR_PREFS: HistoryToolbarPrefs = {
  query: '',
  sort: 'manual',
  statusFilters: [],
  engineFilters: [],
  searchFilters: []
};

export const DEFAULT_FORM_DRAFT_PREFS: FormDraftPrefs = {
  inputs: [],
  outputs: [
    { id: 'output-1', name: '', rate: '60', multiplier: '1' },
    { id: 'output-2', name: '', rate: '60', multiplier: '1' }
  ],
  beltRate: '1200',
  enumerateAllAtN: true,
  engine: 'custom',
  nextEndpointId: 3
};

export function defaultUiPrefs(): UiPrefs {
  return {
    version: UI_PREFS_VERSION,
    history: { ...DEFAULT_HISTORY_TOOLBAR_PREFS, statusFilters: [], engineFilters: [], searchFilters: [] },
    form: {
      ...DEFAULT_FORM_DRAFT_PREFS,
      inputs: [],
      outputs: DEFAULT_FORM_DRAFT_PREFS.outputs.map((row) => ({ ...row }))
    }
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function parseEndpointRow(raw: unknown): EndpointRow | null {
  if (!isRecord(raw) || typeof raw.id !== 'string') return null;
  return {
    id: raw.id,
    name: typeof raw.name === 'string' ? raw.name : '',
    rate: typeof raw.rate === 'string' ? raw.rate : '60',
    multiplier: typeof raw.multiplier === 'string' ? raw.multiplier : '1'
  };
}

function parseEndpointRows(raw: unknown): EndpointRow[] | null {
  if (!Array.isArray(raw)) return null;
  const rows: EndpointRow[] = [];
  for (const item of raw) {
    const row = parseEndpointRow(item);
    if (!row) return null;
    rows.push(row);
  }
  return rows;
}

function filterKnown<T extends string>(raw: unknown, allowed: Set<T>): T[] {
  if (!Array.isArray(raw)) return [];
  const next: T[] = [];
  for (const item of raw) {
    if (typeof item === 'string' && allowed.has(item as T) && !next.includes(item as T)) {
      next.push(item as T);
    }
  }
  return next;
}

export function parseHistoryToolbarPrefs(raw: unknown): HistoryToolbarPrefs {
  if (!isRecord(raw)) return { ...DEFAULT_HISTORY_TOOLBAR_PREFS };
  const sort =
    typeof raw.sort === 'string' && SORT_PREFS.has(raw.sort as HistorySortPref)
      ? (raw.sort as HistorySortPref)
      : DEFAULT_HISTORY_TOOLBAR_PREFS.sort;
  return {
    query: typeof raw.query === 'string' ? raw.query : '',
    sort,
    statusFilters: filterKnown(raw.statusFilters, STATUS_FILTERS),
    engineFilters: filterKnown(raw.engineFilters, ENGINE_FILTERS),
    searchFilters: filterKnown(raw.searchFilters, SEARCH_FILTERS)
  };
}

export function parseFormDraftPrefs(raw: unknown): FormDraftPrefs {
  if (!isRecord(raw)) {
    return {
      ...DEFAULT_FORM_DRAFT_PREFS,
      outputs: DEFAULT_FORM_DRAFT_PREFS.outputs.map((row) => ({ ...row }))
    };
  }
  const inputs = parseEndpointRows(raw.inputs) ?? [];
  const outputs =
    parseEndpointRows(raw.outputs) ??
    DEFAULT_FORM_DRAFT_PREFS.outputs.map((row) => ({ ...row }));
  const engine: SolverEngine = raw.engine === 'z3' ? 'z3' : 'custom';
  const nextEndpointId =
    typeof raw.nextEndpointId === 'number' &&
    Number.isFinite(raw.nextEndpointId) &&
    raw.nextEndpointId >= 1
      ? Math.floor(raw.nextEndpointId)
      : DEFAULT_FORM_DRAFT_PREFS.nextEndpointId;
  return {
    inputs,
    outputs,
    beltRate: typeof raw.beltRate === 'string' ? raw.beltRate : DEFAULT_FORM_DRAFT_PREFS.beltRate,
    enumerateAllAtN:
      typeof raw.enumerateAllAtN === 'boolean'
        ? raw.enumerateAllAtN
        : DEFAULT_FORM_DRAFT_PREFS.enumerateAllAtN,
    engine,
    nextEndpointId
  };
}

export function parseUiPrefs(raw: unknown): UiPrefs {
  const defaults = defaultUiPrefs();
  if (!isRecord(raw)) return defaults;
  return {
    version: UI_PREFS_VERSION,
    history: parseHistoryToolbarPrefs(raw.history),
    form: parseFormDraftPrefs(raw.form)
  };
}

function storageAvailable(): boolean {
  try {
    return typeof localStorage !== 'undefined';
  } catch {
    return false;
  }
}

let cachedPrefs: UiPrefs | null = null;

export function readUiPrefs(): UiPrefs {
  if (cachedPrefs) {
    return {
      version: UI_PREFS_VERSION,
      history: {
        ...cachedPrefs.history,
        statusFilters: [...cachedPrefs.history.statusFilters],
        engineFilters: [...cachedPrefs.history.engineFilters],
        searchFilters: [...cachedPrefs.history.searchFilters]
      },
      form: {
        ...cachedPrefs.form,
        inputs: cachedPrefs.form.inputs.map((row) => ({ ...row })),
        outputs: cachedPrefs.form.outputs.map((row) => ({ ...row }))
      }
    };
  }
  if (!storageAvailable()) {
    cachedPrefs = defaultUiPrefs();
    return readUiPrefs();
  }
  try {
    const raw = localStorage.getItem(UI_PREFS_STORAGE_KEY);
    cachedPrefs = raw ? parseUiPrefs(JSON.parse(raw) as unknown) : defaultUiPrefs();
  } catch {
    cachedPrefs = defaultUiPrefs();
  }
  return readUiPrefs();
}

export function writeUiPrefs(prefs: UiPrefs): void {
  cachedPrefs = {
    version: UI_PREFS_VERSION,
    history: {
      ...prefs.history,
      statusFilters: [...prefs.history.statusFilters],
      engineFilters: [...prefs.history.engineFilters],
      searchFilters: [...prefs.history.searchFilters]
    },
    form: {
      ...prefs.form,
      inputs: prefs.form.inputs.map((row) => ({ ...row })),
      outputs: prefs.form.outputs.map((row) => ({ ...row }))
    }
  };
  if (!storageAvailable()) return;
  try {
    localStorage.setItem(UI_PREFS_STORAGE_KEY, JSON.stringify(cachedPrefs));
  } catch {
    /* quota / private mode */
  }
}

export function updateUiPrefs(patch: {
  history?: HistoryToolbarPrefs;
  form?: FormDraftPrefs;
}): UiPrefs {
  const current = readUiPrefs();
  const next: UiPrefs = {
    version: UI_PREFS_VERSION,
    history: patch.history
      ? {
          ...patch.history,
          statusFilters: [...patch.history.statusFilters],
          engineFilters: [...patch.history.engineFilters],
          searchFilters: [...patch.history.searchFilters]
        }
      : current.history,
    form: patch.form
      ? {
          ...patch.form,
          inputs: patch.form.inputs.map((row) => ({ ...row })),
          outputs: patch.form.outputs.map((row) => ({ ...row }))
        }
      : current.form
  };
  writeUiPrefs(next);
  return next;
}

/** Test helper: drop in-memory cache between cases. */
export function resetUiPrefsCache(): void {
  cachedPrefs = null;
}
