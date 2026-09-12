import type { CollectionRef, JobSnapshot, Solution, SolutionPage, SolveRequest } from '../../types';
import type { SortColumn } from '../solutionSort';
import type { HistoryOp } from '../historyOps';

export interface JobWatch {
  close(): void;
}

export interface JobClient {
  create(request: SolveRequest): Promise<string>;
  get(jobId: string): Promise<JobSnapshot>;
  watch(jobId: string, onSnapshot: (snapshot: JobSnapshot) => void, onError: () => void): Promise<JobWatch>;
  cancel(jobId: string): Promise<JobSnapshot>;
  release(jobId: string): Promise<void>;
  shutdown(): Promise<JobSnapshot[]>;
  resume(): Promise<void>;
}

/** apply resolves only after the whole batch commits. A failed batch changes nothing. */
export interface HistoryStore {
  load(): Promise<unknown>;
  apply(ops: HistoryOp[]): Promise<void>;
}

export type IndexedSolution = { index: number; solution: Solution };
export interface CollectionStore {
  create(id: string): Promise<CollectionRef>;
  append(ref: CollectionRef, values: IndexedSolution[]): Promise<CollectionRef>;
  /** Retain a bounded failed write for viewing/export and a later save retry. */
  retain(ref: CollectionRef, values: IndexedSolution[]): CollectionRef;
  flush(ref: CollectionRef): Promise<void>;
  /** Drop a failed-write overlay only after its owning history entry was deleted. */
  forget(id: string): void;
  page(ref: CollectionRef, offset: number, limit: number, columns: SortColumn[]): Promise<SolutionPage>;
  read(ref: CollectionRef, offset: number, limit: number): Promise<IndexedSolution[]>;
  get(ref: CollectionRef, index: number): Promise<Solution>;
}

export type TextFile = {
  contents: string;
  fileName: string;
  format: 'json' | 'svg';
};

export interface FileActions {
  saveText(file: TextFile): Promise<void>;
  openJsonText(maxBytes?: number): Promise<string | null>;
  saveChunks?(file: Omit<TextFile, 'contents'>, chunks: AsyncIterable<string>): Promise<void>;
}

export type CloseFlushOptions = {
  flush: () => Promise<void>;
  onClosing?: () => void;
  prepare?: () => Promise<void>;
  onReopen?: () => Promise<void>;
  onError: (message: string) => void;
};

export interface PlatformServices {
  readonly capabilities: {
    readonly runtime: 'desktop' | 'browser';
    readonly solve: 'ready' | 'unavailable';
    readonly persistentStorage: 'native' | 'best-effort';
  };
  jobs: JobClient;
  history: HistoryStore;
  collections?: CollectionStore;
  files: FileActions;
  readonly shareViewerUrl: string | null;
  lifecycle: {
    installCloseFlush(options: CloseFlushOptions): Promise<() => void>;
  };
}
