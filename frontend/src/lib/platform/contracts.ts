import type { JobSnapshot, SolveRequest } from '../../types';
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

export type TextFile = {
  contents: string;
  fileName: string;
  format: 'json' | 'svg';
};

export interface FileActions {
  saveText(file: TextFile): Promise<void>;
  openJsonText(maxBytes?: number): Promise<string | null>;
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
  files: FileActions;
  readonly shareViewerUrl: string | null;
  lifecycle: {
    installCloseFlush(options: CloseFlushOptions): Promise<() => void>;
  };
}
