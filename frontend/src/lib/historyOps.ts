import { persistableEntries, type CachedGraphLayout, type HistoryEntry, type HistoryEntryStatus } from './historyModel';
import type { Solution, SolverProgress, OptimalityProof } from '../types';
import type { SortColumn } from './solutionSort';

export type HistoryEntryPatch = {
  title: string | null;
  status: HistoryEntryStatus;
  createdAtMs: number;
  updatedAtMs: number;
  startedAtMs: number | null;
  finishedAtMs: number | null;
  enumerationComplete: boolean;
  error: string | null;
  selectedSourceIndex: number;
};

export type HistoryOp =
  | { op: 'upsertEntry'; entry: HistoryEntry }
  | { op: 'appendSolutions'; id: string; expectedCount: number; solutions: Solution[] }
  | { op: 'patchEntry'; id: string; fields: HistoryEntryPatch }
  | { op: 'saveLayouts'; id: string; layouts: Record<string, CachedGraphLayout> }
  | { op: 'saveSortColumns'; id: string; sortColumns: SortColumn[] }
  | {
      op: 'saveSolverState';
      id: string;
      progress: SolverProgress | null;
      proof: OptimalityProof | null;
      sequence: number;
    }
  | { op: 'deleteEntries'; ids: string[] }
  | { op: 'reorderEntries'; ids: string[] }
  | { op: 'setSelected'; id: string | null };

export type HistorySnapshot = {
  entries: HistoryEntry[];
  selectedEntryId: string | null;
};

export function persistableSelection(entries: HistoryEntry[], selectedEntryId: string | null): string | null {
  return selectedEntryId && entries.some((entry) => entry.id === selectedEntryId)
    ? selectedEntryId
    : (entries[0]?.id ?? null);
}

export function snapshotHistory(entries: HistoryEntry[], selectedEntryId: string | null): HistorySnapshot {
  const persisted = persistableEntries(entries);
  return {
    // UI objects may still change while an IPC write is in flight. Diff only
    // against detached snapshots of the payload that was actually committed.
    entries: JSON.parse(JSON.stringify(persisted)) as HistoryEntry[],
    selectedEntryId: persistableSelection(persisted, selectedEntryId),
  };
}

export function diffHistoryOps(previous: HistorySnapshot | null, next: HistorySnapshot): HistoryOp[] {
  const ops: HistoryOp[] = [];
  const previousEntries = previous?.entries ?? [];
  const previousById = new Map(previousEntries.map((entry) => [entry.id, entry]));
  const nextById = new Map(next.entries.map((entry) => [entry.id, entry]));

  const deletedIds = previousEntries.map((entry) => entry.id).filter((id) => !nextById.has(id));
  if (deletedIds.length > 0) {
    ops.push({ op: 'deleteEntries', ids: deletedIds });
  }

  for (const entry of next.entries) {
    const prior = previousById.get(entry.id);
    if (!prior) {
      ops.push({ op: 'upsertEntry', entry });
      continue;
    }
    ops.push(...classifyEntryDelta(prior, entry));
  }

  const previousOrder = previousEntries.map((entry) => entry.id);
  const nextOrder = next.entries.map((entry) => entry.id);
  if (!jsonEq(previousOrder, nextOrder)) {
    ops.push({ op: 'reorderEntries', ids: nextOrder });
  }

  const previousSelected = previous?.selectedEntryId ?? null;
  if (previousSelected !== next.selectedEntryId) {
    ops.push({ op: 'setSelected', id: next.selectedEntryId });
  }

  return ops;
}

function classifyEntryDelta(previous: HistoryEntry, next: HistoryEntry): HistoryOp[] {
  if (!jsonEq(coreFields(previous), coreFields(next))) {
    return [{ op: 'upsertEntry', entry: next }];
  }

  const ops: HistoryOp[] = [];
  if (!jsonEq(previous.results, next.results)) {
    const count = previous.results.length;
    const unchangedPrefix =
      count < next.results.length &&
      (count > 0 || previous.result == null) &&
      previous.results.every((solution, index) => jsonEq(solution, next.results[index]));
    if (!unchangedPrefix) return [{ op: 'upsertEntry', entry: next }];
    ops.push({ op: 'appendSolutions', id: next.id, expectedCount: count, solutions: next.results.slice(count) });
  }
  if (!jsonEq(scalarFields(previous), scalarFields(next))) {
    ops.push({ op: 'patchEntry', id: next.id, fields: scalarFields(next) });
  }
  if (!jsonEq(previous.layouts, next.layouts)) {
    ops.push({ op: 'saveLayouts', id: next.id, layouts: next.layouts });
  }
  if (!jsonEq(previous.sortColumns, next.sortColumns)) {
    ops.push({ op: 'saveSortColumns', id: next.id, sortColumns: next.sortColumns });
  }
  if (!jsonEq(solverState(previous), solverState(next))) {
    ops.push({
      op: 'saveSolverState',
      id: next.id,
      progress: next.progress,
      proof: next.proof ?? null,
      sequence: next.sequence ?? 0,
    });
  }
  return ops;
}

function coreFields(entry: HistoryEntry): unknown {
  return {
    request: entry.request,
    form: entry.form,
    result: entry.result,
  };
}

function scalarFields(entry: HistoryEntry): HistoryEntryPatch {
  return {
    title: entry.title,
    status: entry.status,
    createdAtMs: entry.createdAtMs,
    updatedAtMs: entry.updatedAtMs,
    startedAtMs: entry.startedAtMs,
    finishedAtMs: entry.finishedAtMs,
    enumerationComplete: entry.enumerationComplete,
    error: entry.error,
    selectedSourceIndex: entry.selectedSourceIndex,
  };
}

function solverState(entry: HistoryEntry): unknown {
  return {
    progress: entry.progress,
    proof: entry.proof ?? null,
    sequence: entry.sequence ?? 0,
  };
}

function jsonEq(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}
