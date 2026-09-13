import type { JobSnapshot, SolveRequest } from '../../types';
import type { JobClient } from './contracts';
import { createBrowserHistoryStore } from './browserHistory';
import { browserThreadCount } from './browserWorkers';
import { BROWSER_SOLVER_PROTOCOL, type WorkerPacket } from '../solver/protocol';
import { emptySnapshot, materializeSnapshot } from '../solver/snapshots';
import { createBrowserComputePool } from './browserComputePool';
import { createBrowserCheckpoint } from './browserCheckpoint';
import type { BrowserJobOptions, Job, Stop } from './browserJobState';
export type { BrowserJobOptions } from './browserJobState';

export function createBrowserJobs(options: BrowserJobOptions = {}): JobClient {
  const jobs = new Map<string, Job>();
  let admission = true;
  let active: Job | null = null;
  let ownedHistory: ReturnType<typeof createBrowserHistoryStore> | undefined;
  const collections = () => options.collections ?? (ownedHistory ??= createBrowserHistoryStore()).collections;
  const retention = Math.max(1, Math.min(32, options.retention ?? 8));
  const spawn =
    options.createWorker ??
    ((role: 'coordinator' | 'compute') =>
      role === 'coordinator'
        ? new Worker(new URL('../solver/solve.worker.ts', import.meta.url), { type: 'module' })
        : new Worker(new URL('../solver/leaf.worker.ts', import.meta.url), { type: 'module' }));

  const pool = createBrowserComputePool({
    spawn,
    observe: decorate,
    stop,
    startupTimeoutMs: options.startupTimeoutMs,
    resourceLimit: options.resourceLimit,
  });
  const apply = createBrowserCheckpoint({ collections, pool, emit, seal });

  function lookup(id: string): Job {
    const job = jobs.get(id);
    if (!job) throw new Error('Unknown browser job');
    return job;
  }
  function decorate(job: Job, value = job.current): JobSnapshot {
    const result = structuredClone(value);
    const heap = job.rustHeap + job.slots.reduce((total, slot) => total + slot.heap, 0);
    job.peakHeap = Math.max(job.peakHeap, heap);
    if (result.progress) {
      const values = {
        'solver.browser_heap_bytes': ['Allocated browser Wasm heaps', heap],
        'solver.browser_peak_heap_bytes': ['Peak allocated browser Wasm heaps', job.peakHeap],
        'solver.browser_peak_pending': ['Peak unacknowledged compute packets', job.peakPending],
        'solver.browser_dispatched': ['Dispatched browser leaves', job.scheduling?.dispatched ?? 0],
        'solver.browser_second_roots': ['Second-output partitions', job.scheduling?.secondOutputRoots ?? 0],
        'solver.browser_adaptive_children': ['Adaptive children queued', job.scheduling?.adaptiveChildren ?? 0],
        'solver.browser_identity_bytes': ['Coordinator canonical identity bytes', job.scheduling?.identityBytes ?? 0],
      };
      result.progress.custom.push(
        ...Object.entries(values).map(([name, [label, count]]) => ({
          name,
          label: String(label),
          value: { type: 'integer' as const, value: String(count) },
          unit: name.endsWith('_bytes') ? 'bytes' : null,
        })),
      );
      if (job.scheduling?.proofOwner != null)
        result.progress.custom.push({
          name: 'solver.portfolio_proof_owner',
          label: 'Returned independent proof owner',
          value: { type: 'integer', value: String(job.scheduling.proofOwner) },
          unit: null,
        });
    }
    return result;
  }
  function emit(job: Job): void {
    for (const listener of [...job.listeners])
      try {
        listener.snapshot(decorate(job));
      } catch {
        try {
          listener.error();
        } catch {
          /* Isolate consumer errors. */
        }
      }
  }
  function terminateWorkers(job: Job): void {
    decorate(job); // Sample all still-owned heaps before disposal.
    if (job.startupTimer !== null) clearTimeout(job.startupTimer);
    job.startupTimer = null;
    const worker = job.worker;
    job.worker = null;
    if (worker) {
      worker.onmessage = null;
      worker.onerror = null;
      worker.onmessageerror = null;
      worker.terminate();
    }
    for (const slot of job.slots) pool.killSlot(slot);
    job.rustHeap = 0;
  }
  function seal(job: Job): void {
    terminateWorkers(job);
    job.sealed = true;
    job.tasks.clear();
    job.retired.clear();
    if (active === job) active = null;
    emit(job);
  }
  function finishStop(job: Job): void {
    if (job.sealed || !job.requested) return;
    const ref = job.current.collection;
    const { reason, detail } = job.requested;
    job.current = job.recovery
      ? materializeSnapshot(job.current, job.recovery[reason])
      : { ...job.current, status: reason, sequence: (job.current.sequence ?? 0) + 1 };
    job.current.collection = ref;
    if (reason === 'failed')
      job.current.error = detail?.slice(0, 2048) || job.current.error || 'Browser compute failed.';
    seal(job);
  }
  function stop(job: Job, reason: Stop['reason'], detail?: string): Promise<JobSnapshot> {
    if (job.sealed) return Promise.resolve(decorate(job));
    if (!job.requested) {
      job.requested = { reason, detail };
      terminateWorkers(job);
      job.current = { ...job.current, status: 'cancelling' };
      emit(job);
    }
    // A received batch may already be committing. Keep it, then apply the Rust
    // interruption prepared before the terminal proposal. Cancellation wins.
    return job.applying.then(() => {
      finishStop(job);
      return decorate(job);
    });
  }
  function receive(job: Job, value: unknown): void {
    if (job.sealed || job.requested || !job.worker || active !== job) return;
    const packet = value as WorkerPacket;
    if (!packet || typeof packet !== 'object') {
      void stop(job, 'failed', 'Unreadable browser coordinator packet.');
      return;
    }
    if (packet.jobId !== job.id || packet.attempt !== job.attempt) return;
    if (packet.protocol !== BROWSER_SOLVER_PROTOCOL) {
      void stop(job, 'failed', 'Incompatible browser coordinator protocol.');
      return;
    }
    if (packet.kind === 'failed') {
      void stop(
        job,
        'failed',
        typeof packet.error === 'string' ? packet.error : 'Unreadable browser coordinator failure.',
      );
      return;
    }
    if (packet.kind === 'ready') {
      if (!Number.isSafeInteger(packet.rustBytes) || packet.rustBytes < 0) {
        void stop(job, 'failed', 'Invalid coordinator memory observation.');
        return;
      }
      if (job.startupTimer !== null) clearTimeout(job.startupTimer);
      job.startupTimer = null;
      job.rustHeap = packet.rustBytes;
      return;
    }
    if (packet.kind !== 'update' || job.receiving) {
      void stop(job, 'failed', 'Browser coordinator exceeded its in-flight checkpoint budget.');
      return;
    }
    job.receiving = true;
    job.applying = apply(job, packet)
      .catch((error) => {
        job.requested ??= { reason: 'failed', detail: String(error) };
        terminateWorkers(job);
      })
      .then(() => {
        job.receiving = false;
        finishStop(job);
      });
  }
  return {
    async create(request: SolveRequest) {
      if (!admission) throw new Error('Browser job admission is closed.');
      if (active) throw new Error('A browser solve is already running.');
      const source = JSON.stringify(request);
      if (new TextEncoder().encode(source).length > 256 * 1024)
        throw new Error('Browser solve request exceeds 256 KiB.');
      const threads = browserThreadCount();
      const workerCount = options.workerCount ?? request.browserWorkers ?? threads;
      if (!Number.isSafeInteger(workerCount) || workerCount < 1 || workerCount > threads)
        throw new Error(
          `Browser compute worker budget must be between 1 and ${threads}, the client's reported thread count.`,
        );
      const id = crypto.randomUUID();
      const job: Job = {
        id,
        attempt: crypto.randomUUID(),
        worker: null,
        slots: [],
        tasks: new Map(),
        retired: new Set(),
        current: emptySnapshot(id, Date.now()),
        recovery: null,
        checkpoint: 0,
        sealed: false,
        requested: null,
        listeners: new Set(),
        startupTimer: null,
        applying: Promise.resolve(),
        receiving: false,
        workerCount,
        solverBase:
          options.solverBase ?? new URL(`${import.meta.env.BASE_URL}${__SFS_SOLVER_DIR__}/`, document.baseURI).href,
        rustHeap: 0,
        peakHeap: 0,
        peakPending: 0,
        scheduling: null,
      };
      active = job;
      jobs.set(id, job);
      for (const previous of jobs.values()) {
        if (jobs.size <= retention) break;
        if (previous.sealed) {
          previous.listeners.clear();
          jobs.delete(previous.id);
        }
      }
      try {
        if (request.solveMode !== 'one_min_nl') {
          const preparation = collections()
            .create(id)
            .then((collection) => {
              job.current.collection = collection;
            });
          job.applying = preparation.catch(() => {});
          await preparation;
        }
        if (job.requested) {
          finishStop(job);
          return id;
        }
        const worker = spawn('coordinator', 0);
        job.worker = worker;
        worker.onmessage = ({ data }) => receive(job, data);
        worker.onerror = (event) => {
          event.preventDefault();
          void stop(job, 'failed', event.message || 'Browser coordinator crashed.');
        };
        worker.onmessageerror = () => {
          void stop(job, 'failed', 'Unreadable browser coordinator message.');
        };
        job.startupTimer = setTimeout(() => {
          void stop(job, 'failed', 'Browser solver assets exceeded the startup limit.');
        }, options.startupTimeoutMs ?? 120_000);
        worker.postMessage({
          kind: 'start',
          protocol: BROWSER_SOLVER_PROTOCOL,
          jobId: id,
          attempt: job.attempt,
          startedAtMs: job.current.startedAtMs,
          request: JSON.parse(source),
          solverBase: job.solverBase,
          options: {
            workerCount,
            maxNodes: options.maxNodes,
            maxLayouts: options.maxLayouts,
            maxIdentityBytes: options.maxIdentityBytes,
            strategy: options.strategy,
          },
        });
      } catch (error) {
        await stop(job, 'failed', String(error));
      }
      return id;
    },
    async get(id) {
      return decorate(lookup(id));
    },
    async watch(id, onSnapshot, onError) {
      const job = lookup(id);
      const listener = { snapshot: onSnapshot, error: onError };
      job.listeners.add(listener);
      try {
        onSnapshot(decorate(job));
      } catch {
        try {
          onError();
        } catch {
          /* Isolate consumer errors. */
        }
      }
      return {
        close() {
          job.listeners.delete(listener);
        },
      };
    },
    cancel: (id) => stop(lookup(id), 'cancelled'),
    async release(id) {
      const job = jobs.get(id);
      if (!job) return;
      if (!job.sealed) throw new Error('Cannot release an active browser job.');
      job.listeners.clear();
      jobs.delete(id);
    },
    async shutdown() {
      admission = false;
      const snapshots = await Promise.all(
        [...jobs.values()].map((job) => (job.sealed ? decorate(job) : stop(job, 'cancelled'))),
      );
      ownedHistory?.close();
      return snapshots;
    },
    async resume() {
      admission = true;
    },
  };
}
