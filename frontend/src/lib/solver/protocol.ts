import type { JobSnapshot, SolveRequest } from '../../types';

export const BROWSER_SOLVER_PROTOCOL = 1;

export type Identity = { jobId: string; attempt: string; protocol: number };
export type Recovery = { cancelled: JobSnapshot; failed: JobSnapshot };
export type Dispatch = { id: [number, number]; impossible: boolean };
export type RunUpdate = {
  dispatch: Dispatch[];
  packets: JobSnapshot[];
  recovery: Recovery;
  done: boolean;
};
export type Start = Identity & {
  kind: 'start';
  startedAtMs: number;
  request: SolveRequest;
  solverBase: string;
  maxNodes?: number;
  resourceLimit?: number;
};
export type Ack = Identity & { kind: 'ack'; checkpoint: number };
export type WorkerMessage =
  | { kind: 'update'; checkpoint: number; packets: JobSnapshot[]; recovery: Recovery; done: boolean }
  | { kind: 'ready' }
  | { kind: 'failed'; error: string };
export type WorkerPacket = Identity & WorkerMessage;

export type Session = { execute(commands: string): string; dispose(): void };
export type SessionApi = {
  createSession(options?: { resourceLimit?: number }): Session;
  activeSessions(): number;
};

export interface BrowserRun {
  poll(elapsedMs: bigint): string;
  advance(id: string, reply?: string): string;
  retire(id: string, verdict: string, detail: string): void;
  free(): void;
}
export type BrowserModule = {
  default(options: { module_or_path: URL }): Promise<unknown>;
  browser_solver_version(): number;
  initial_job_json(jobId: string, startedAtMs: bigint): string;
  BrowserRun: new (request: string, jobId: string, startedAtMs: bigint, maxNodes?: number) => BrowserRun;
};
