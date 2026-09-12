import type { JobSnapshot, SolveRequest } from '../../types';
import type { JobClient } from './contracts';
import { BROWSER_SOLVER_PROTOCOL, type Recovery, type WorkerPacket } from '../solver/protocol';
import { assertSnapshot, emptySnapshot, materializeSnapshot, terminal } from '../solver/snapshots';

type Listener = { snapshot(value: JobSnapshot): void; error(): void };
type Job = {
  id: string;
  attempt: string;
  worker: Worker | null;
  current: JobSnapshot;
  recovery: Recovery | null;
  checkpoint: number;
  sealed: boolean;
  listeners: Set<Listener>;
  startupTimer: ReturnType<typeof setTimeout> | null;
};

export type BrowserJobOptions = {
  /** Dependency injection for transport tests, not a public SMT input API. */
  createWorker?: () => Worker;
  solverBase?: string;
  startupTimeoutMs?: number;
  maxNodes?: number;
  resourceLimit?: number;
  retention?: number;
};

export function createBrowserJobs(options: BrowserJobOptions = {}): JobClient {
  const jobs = new Map<string, Job>();
  let admission = true;
  let active: Job | null = null;
  const retention = Math.max(1, options.retention ?? 8);
  const spawn =
    options.createWorker ??
    (() => new Worker(new URL('../solver/solve.worker.ts', import.meta.url), { type: 'module' }));

  function lookup(id: string): Job {
    const job = jobs.get(id);
    if (!job) throw new Error('Unknown browser job');
    return job;
  }

  function emit(job: Job, packet: JobSnapshot): void {
    for (const listener of [...job.listeners]) {
      try {
        listener.snapshot(structuredClone(packet));
      } catch {
        try {
          listener.error();
        } catch {
          /* Consumer errors do not own search completion. */
        }
      }
    }
  }

  function dispose(job: Job): void {
    job.sealed = true;
    if (job.startupTimer !== null) clearTimeout(job.startupTimer);
    job.startupTimer = null;
    const worker = job.worker;
    job.worker = null;
    if (active === job) active = null;
    if (worker) {
      worker.onmessage = null;
      worker.onerror = null;
      worker.onmessageerror = null;
      worker.terminate();
    }
  }

  function stop(job: Job, reason: 'cancelled' | 'failed', detail?: string): JobSnapshot {
    if (job.sealed) return structuredClone(job.current);
    // Close host completion ownership and terminate synchronous Wasm before
    // exposing the pre-projected interruption. A late packet cannot resurrect it.
    dispose(job);
    if (job.recovery) {
      job.current = materializeSnapshot(job.current, job.recovery[reason]);
    } else {
      // No Rust message and therefore no accepted witness/proof exists yet.
      job.current = { ...job.current, status: reason, sequence: (job.current.sequence ?? 0) + 1 };
    }
    if (reason === 'failed')
      job.current.error = detail?.slice(0, 2048) || job.current.error || 'Browser compute worker failed.';
    const snapshot = structuredClone(job.current);
    emit(job, snapshot);
    return snapshot;
  }

  function accept(job: Job, value: unknown): void {
    if (job.sealed || active !== job || !job.worker) return;
    if (!value || typeof value !== 'object') {
      stop(job, 'failed', 'Unreadable browser worker packet.');
      return;
    }
    const packet = value as WorkerPacket;
    // Packets from retired attempts are ignored, not interpreted as new evidence.
    if (packet.jobId !== job.id || packet.attempt !== job.attempt) return;
    try {
      if (packet.protocol !== BROWSER_SOLVER_PROTOCOL) throw new Error('Incompatible browser worker protocol.');
      if (packet.kind === 'failed') {
        stop(job, 'failed', typeof packet.error === 'string' ? packet.error : 'Browser worker failed.');
        return;
      }
      if (packet.kind === 'ready') {
        if (job.startupTimer !== null) clearTimeout(job.startupTimer);
        job.startupTimer = null;
        return;
      }
      if (
        packet.kind !== 'update' ||
        packet.checkpoint !== job.checkpoint + 1 ||
        !Array.isArray(packet.packets) ||
        packet.packets.length === 0 ||
        typeof packet.done !== 'boolean' ||
        !packet.recovery
      )
        throw new Error('Invalid browser worker checkpoint.');
      // Validate and materialize the whole batch before any callback can cancel.
      // This also makes get() reconcile missed subscription packets immediately.
      let current = job.current;
      for (const snapshot of packet.packets) {
        assertSnapshot(snapshot, job.id);
        if (terminal(snapshot) && (snapshot.resultsOmitted || snapshot.resultAppended))
          throw new Error('Browser terminal snapshot must contain all results.');
        if (
          snapshot.sequence !== (current.sequence ?? 0) + 1 &&
          !(job.checkpoint === 0 && snapshot.sequence === 0 && current.sequence === 0)
        )
          throw new Error('Browser worker snapshot sequence is incomplete.');
        current = materializeSnapshot(current, snapshot);
      }
      if (terminal(current) !== packet.done || (packet.done && (current.resultsOmitted || current.resultAppended)))
        throw new Error('Invalid browser completion packet.');
      for (const reason of ['cancelled', 'failed'] as const) {
        const recovery = packet.recovery[reason];
        assertSnapshot(recovery, job.id);
        if (
          recovery.status !== reason ||
          !recovery.resultsOmitted ||
          recovery.resultAppended ||
          recovery.enumerationComplete ||
          (!packet.done && recovery.sequence !== current.sequence! + 1) ||
          recovery.resultsLen !== current.results.length
        )
          throw new Error('Invalid browser interruption packet.');
      }
      job.checkpoint = packet.checkpoint;
      job.current = current;
      job.recovery = packet.recovery;
      if (packet.done) {
        dispose(job);
        emit(job, job.current);
        return;
      }
      for (const snapshot of packet.packets) {
        if (job.sealed) return;
        emit(job, snapshot);
      }
      if (!job.sealed)
        job.worker?.postMessage({
          kind: 'ack',
          jobId: job.id,
          attempt: job.attempt,
          protocol: BROWSER_SOLVER_PROTOCOL,
          checkpoint: packet.checkpoint,
        });
    } catch (error) {
      stop(job, 'failed', String(error));
    }
  }

  return {
    async create(request: SolveRequest) {
      if (!admission) throw new Error('Browser job admission is closed.');
      if (active) throw new Error('A browser solve is already running.');
      const source = JSON.stringify(request);
      if (new TextEncoder().encode(source).length > 256 * 1024)
        throw new Error('Browser solve request exceeds 256 KiB.');
      const id = crypto.randomUUID();
      const job: Job = {
        id,
        attempt: crypto.randomUUID(),
        worker: null,
        current: emptySnapshot(id, Date.now()),
        recovery: null,
        checkpoint: 0,
        sealed: false,
        listeners: new Set(),
        startupTimer: null,
      };
      jobs.set(id, job);
      active = job;
      for (const previous of jobs.values()) {
        if (jobs.size <= retention) break;
        if (previous.sealed) {
          previous.listeners.clear();
          jobs.delete(previous.id);
        }
      }
      try {
        const worker = spawn();
        job.worker = worker;
        worker.onmessage = ({ data }) => accept(job, data);
        worker.onerror = (event) => {
          event.preventDefault();
          stop(job, 'failed', event.message || 'Browser compute worker crashed.');
        };
        worker.onmessageerror = () => stop(job, 'failed', 'Browser compute worker returned an unreadable message.');
        job.startupTimer = setTimeout(
          () =>
            stop(
              job,
              'failed',
              'Browser solver assets did not load before the startup limit. Check the connection and try again.',
            ),
          options.startupTimeoutMs ?? 120_000,
        );
        const solverBase =
          options.solverBase ?? new URL(`${import.meta.env.BASE_URL}${__SFS_SOLVER_DIR__}/`, document.baseURI).href;
        worker.postMessage({
          kind: 'start',
          jobId: id,
          attempt: job.attempt,
          protocol: BROWSER_SOLVER_PROTOCOL,
          startedAtMs: job.current.startedAtMs,
          request: JSON.parse(source),
          solverBase,
          maxNodes: options.maxNodes,
          resourceLimit: options.resourceLimit,
        });
      } catch (error) {
        stop(job, 'failed', String(error));
      }
      return id;
    },
    async get(id) {
      return structuredClone(lookup(id).current);
    },
    async watch(id, onSnapshot, onError) {
      const job = lookup(id);
      const listener = { snapshot: onSnapshot, error: onError };
      job.listeners.add(listener);
      try {
        onSnapshot(structuredClone(job.current));
      } catch {
        try {
          onError();
        } catch {
          /* Isolate consumer failure. */
        }
      }
      return {
        close: () => {
          job.listeners.delete(listener);
        },
      };
    },
    async cancel(id) {
      return stop(lookup(id), 'cancelled');
    },
    async release(id) {
      const job = jobs.get(id);
      if (!job) return;
      if (!job.sealed) throw new Error('Cannot release an active browser job.');
      job.listeners.clear();
      jobs.delete(id);
    },
    async shutdown() {
      admission = false;
      const owned = [...jobs.values()];
      return owned.map((job) => (job.sealed ? structuredClone(job.current) : stop(job, 'cancelled')));
    },
    async resume() {
      admission = true;
    },
  };
}
