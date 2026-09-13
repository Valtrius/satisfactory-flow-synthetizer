import type { JobSnapshot, SolveRequest } from '../../types';
import type { IndexedSolution } from '../platform/contracts';

export const BROWSER_SOLVER_PROTOCOL = 2;
export type Identity = { jobId: string; attempt: string; protocol: number };
export type Recovery = { cancelled: JobSnapshot; failed: JobSnapshot };
export type Dispatch = { id: string; branch: number; source: string };
export type Scheduling = {
  dispatched: number;
  secondOutputRoots: number;
  adaptiveGroups: number;
  adaptiveChildren: number;
  peakActive: number;
  active: number;
  identityBytes: number;
  proofOwner: number | null;
  budgets: number[];
};
export type RunUpdate = {
  dispatch: Dispatch[];
  stop: string[];
  packets: JobSnapshot[];
  append: IndexedSolution[];
  count: number;
  preferredIndex: number | null;
  recovery: Recovery;
  done: boolean;
  scheduling: Scheduling;
};
export type Receipt = { task: string; sequence: number; workerAttempt: string };
export type ComputeEvent =
  { kind: 'witness'; id: string; witness: unknown } | { kind: 'retired'; id: string; verdict: string; detail: string };
export type BrowserRunOptions = {
  workerCount: number;
  maxNodes?: number;
  maxLayouts?: number;
  maxIdentityBytes?: number;
  strategy?: 'portfolio' | 'boolean' | 'sparse';
};
export type Start = Identity & {
  kind: 'start';
  startedAtMs: number;
  request: SolveRequest;
  solverBase: string;
  options: BrowserRunOptions;
};
export type Ack = Identity & { kind: 'ack'; checkpoint: number };
export type Events = Identity & { kind: 'events'; events: ComputeEvent[]; receipts: Receipt[] };
export type WorkerPacket = Identity &
  (
    | { kind: 'update'; checkpoint: number; update: RunUpdate; receipts: Receipt[]; rustBytes: number }
    | { kind: 'ready'; rustBytes: number }
    | { kind: 'failed'; error: string }
  );
export type LeafStart = Identity & {
  kind: 'leaf';
  workerAttempt: string;
  task: string;
  source: string;
  solverBase: string;
  resourceLimit?: number;
};
export type LeafAck = Identity & { kind: 'leaf-ack'; workerAttempt: string; task: string; sequence: number };
export type LeafPacket = Identity & { workerAttempt: string; task: string } & (
    | { kind: 'ready'; rustBytes: number; heapBytes: number }
    | { kind: 'leaf-events'; sequence: number; events: ComputeEvent[]; rustBytes: number; heapBytes: number }
    | { kind: 'heartbeat'; sequence: number; rustBytes: number; heapBytes: number }
    | { kind: 'failed'; error: string }
  );
export type Session = { execute(commands: string): string; dispose(): void };
export type SessionApi = {
  createSession(options?: { resourceLimit?: number }): Session;
  activeSessions(): number;
  heapBytes(): number;
};
export interface CoordinatorRun {
  poll(elapsedMs: bigint): string;
  accept(source: string, elapsedMs: bigint): void;
  free(): void;
}
export interface LeafRun {
  advance(reply?: string): string;
  free(): void;
}
export type BrowserModule = {
  default(options: { module_or_path: URL }): Promise<{ memory: WebAssembly.Memory }>;
  browser_solver_version(): number;
  BrowserCoordinator: new (request: string, jobId: string, startedAtMs: bigint, options: string) => CoordinatorRun;
  BrowserLeaf: new (source: string) => LeafRun;
};
