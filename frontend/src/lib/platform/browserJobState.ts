import type { JobSnapshot } from '../../types';
import type { Recovery, Scheduling } from '../solver/protocol';
import type { CollectionStore } from './contracts';

export type Listener = { snapshot(value: JobSnapshot): void; error(): void };
export type Slot = {
  worker: Worker | null;
  attempt: string;
  task: string | null;
  sequence: number;
  waiting: boolean;
  reported: boolean;
  timer: ReturnType<typeof setTimeout> | null;
  heap: number;
};
export type Stop = { reason: 'cancelled' | 'failed'; detail?: string };
export type Job = {
  id: string;
  attempt: string;
  worker: Worker | null;
  slots: Slot[];
  tasks: Map<string, Slot>;
  retired: Set<string>;
  current: JobSnapshot;
  recovery: Recovery | null;
  checkpoint: number;
  sealed: boolean;
  requested: Stop | null;
  listeners: Set<Listener>;
  startupTimer: ReturnType<typeof setTimeout> | null;
  applying: Promise<void>;
  receiving: boolean;
  workerCount: number;
  solverBase: string;
  rustHeap: number;
  peakHeap: number;
  peakPending: number;
  scheduling: Scheduling | null;
};

export type BrowserJobOptions = {
  createWorker?: (role: 'coordinator' | 'compute', slot: number) => Worker;
  collections?: CollectionStore;
  solverBase?: string;
  startupTimeoutMs?: number;
  workerCount?: number;
  maxNodes?: number;
  maxLayouts?: number;
  maxIdentityBytes?: number;
  strategy?: 'portfolio' | 'boolean' | 'sparse';
  resourceLimit?: number;
  retention?: number;
};
