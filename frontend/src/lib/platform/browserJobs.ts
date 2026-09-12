import type { JobSnapshot, SolveRequest } from '../../types';
import type { CollectionStore, JobClient } from './contracts';
import { createBrowserHistoryStore } from './browserHistory';
import { checkCollectionRef } from './browserCollections';
import { browserThreadCount } from './browserWorkers';
import { coordinatorQueueLimit } from '../solver/inbox';
import {
  BROWSER_SOLVER_PROTOCOL,
  type ComputeEvent,
  type Dispatch,
  type LeafPacket,
  type Receipt,
  type Recovery,
  type Scheduling,
  type WorkerPacket,
} from '../solver/protocol';
import { assertSnapshot, emptySnapshot, materializeSnapshot, terminal } from '../solver/snapshots';

type Listener = { snapshot(value: JobSnapshot): void; error(): void };
type Slot = {
  worker: Worker | null;
  attempt: string;
  task: string | null;
  sequence: number;
  waiting: boolean;
  reported: boolean;
  timer: ReturnType<typeof setTimeout> | null;
  heap: number;
};
type Stop = { reason: 'cancelled' | 'failed'; detail?: string };
type Job = {
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
  function killSlot(slot: Slot): void {
    if (slot.timer !== null) clearTimeout(slot.timer);
    slot.timer = null;
    const worker = slot.worker;
    slot.worker = null;
    if (worker) {
      worker.onmessage = null;
      worker.onerror = null;
      worker.onmessageerror = null;
      worker.terminate();
    }
    slot.heap = 0;
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
    for (const slot of job.slots) killSlot(slot);
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
  function forward(job: Job, events: ComputeEvent[], receipts: Receipt[] = []): void {
    if (job.requested || job.sealed) return;
    try {
      job.worker?.postMessage({
        kind: 'events',
        protocol: BROWSER_SOLVER_PROTOCOL,
        jobId: job.id,
        attempt: job.attempt,
        events,
        receipts,
      });
    } catch (error) {
      // Event-handler callers cannot await a transport exception. Retire every
      // worker and keep the last accepted prefix instead of leaving a live job.
      void stop(job, 'failed', `Could not reach browser coordinator: ${String(error)}`);
    }
  }
  function retired(job: Job, task: string): void {
    job.retired.add(task);
    if (job.retired.size > coordinatorQueueLimit(job.workerCount))
      job.retired.delete(job.retired.values().next().value!);
  }
  function failSlot(job: Job, slot: Slot, detail: string): void {
    if (!slot.worker || job.requested || job.sealed) return;
    const task = slot.task;
    killSlot(slot);
    slot.task = null;
    if (task) {
      job.tasks.delete(task);
      retired(job, task);
      if (!slot.reported)
        forward(job, [{ kind: 'retired', id: task, verdict: 'failed', detail: detail.slice(0, 2048) }]);
    }
  }
  function acknowledge(job: Job, slot: Slot, receipt: Receipt): void {
    slot.worker?.postMessage({
      kind: 'leaf-ack',
      protocol: BROWSER_SOLVER_PROTOCOL,
      jobId: job.id,
      attempt: job.attempt,
      ...receipt,
    });
  }
  function computeMessage(job: Job, slot: Slot, value: unknown): void {
    if (job.requested || job.sealed || !slot.worker) return;
    const packet = value as LeafPacket;
    if (!packet || typeof packet !== 'object') {
      failSlot(job, slot, 'Unreadable browser compute packet.');
      return;
    }
    if (
      packet.jobId !== job.id ||
      packet.attempt !== job.attempt ||
      packet.workerAttempt !== slot.attempt ||
      packet.task !== slot.task
    )
      return;
    try {
      if (packet.protocol !== BROWSER_SOLVER_PROTOCOL) throw new Error('Incompatible browser compute protocol.');
      if (packet.kind === 'failed') {
        if (typeof packet.error !== 'string') throw new Error('Unreadable browser compute failure.');
        failSlot(job, slot, packet.error);
        return;
      }
      if (
        !Number.isSafeInteger(packet.heapBytes) ||
        !Number.isSafeInteger(packet.rustBytes) ||
        packet.heapBytes < 0 ||
        packet.rustBytes < 0
      )
        throw new Error('Invalid worker memory observation.');
      slot.heap = packet.heapBytes + packet.rustBytes;
      decorate(job);
      if (packet.kind === 'ready') {
        if (slot.timer !== null) clearTimeout(slot.timer);
        slot.timer = null;
        return;
      }
      if (slot.waiting || packet.sequence !== slot.sequence + 1 || !['heartbeat', 'leaf-events'].includes(packet.kind))
        throw new Error('Noncontiguous compute packet.');
      slot.sequence = packet.sequence;
      const receipt = { task: packet.task, sequence: packet.sequence, workerAttempt: slot.attempt };
      if (packet.kind === 'heartbeat') {
        acknowledge(job, slot, receipt);
        return;
      }
      if (
        !Array.isArray(packet.events) ||
        packet.events.length !== 1 ||
        packet.events[0].id !== slot.task ||
        !['witness', 'retired'].includes(packet.events[0].kind)
      )
        throw new Error('Invalid browser leaf event.');
      slot.waiting = true;
      slot.reported = packet.events[0].kind === 'retired';
      job.peakPending = Math.max(job.peakPending, job.slots.filter((slot) => slot.waiting && slot.task).length);
      forward(job, packet.events, [receipt]);
    } catch (error) {
      failSlot(job, slot, String(error));
    }
  }
  function dispatch(job: Job, work: Dispatch): void {
    if (job.tasks.has(work.id)) throw new Error('Duplicate browser leaf dispatch.');
    let slot = job.slots.find((slot) => slot.task === null);
    if (!slot) {
      if (job.slots.length >= job.workerCount) throw new Error('Browser coordinator exceeded the compute budget.');
      slot = {
        worker: null,
        attempt: '',
        task: null,
        sequence: 0,
        waiting: false,
        reported: false,
        timer: null,
        heap: 0,
      };
      job.slots.push(slot);
    }
    const selected = slot;
    selected.task = work.id;
    selected.sequence = 0;
    selected.waiting = false;
    selected.reported = false;
    job.tasks.set(work.id, selected);
    try {
      if (!selected.worker) {
        selected.attempt = crypto.randomUUID();
        const worker = spawn('compute', job.slots.indexOf(selected));
        selected.worker = worker;
        worker.onmessage = ({ data }) => {
          if (selected.worker === worker) computeMessage(job, selected, data);
        };
        worker.onerror = (event) => {
          event.preventDefault();
          if (selected.worker === worker) failSlot(job, selected, event.message || 'Browser compute worker crashed.');
        };
        worker.onmessageerror = () => {
          if (selected.worker === worker) failSlot(job, selected, 'Unreadable browser compute message.');
        };
        selected.timer = setTimeout(
          () => failSlot(job, selected, 'Browser compute assets exceeded the startup limit.'),
          options.startupTimeoutMs ?? 120_000,
        );
      }
      selected.worker.postMessage({
        kind: 'leaf',
        protocol: BROWSER_SOLVER_PROTOCOL,
        jobId: job.id,
        attempt: job.attempt,
        workerAttempt: selected.attempt,
        task: work.id,
        source: work.source,
        solverBase: job.solverBase,
        resourceLimit: options.resourceLimit,
      });
    } catch (error) {
      // Construction can fail before a Worker exists; still retire the dispatched obligation.
      if (selected.worker) failSlot(job, selected, String(error));
      else {
        selected.task = null;
        job.tasks.delete(work.id);
        retired(job, work.id);
        forward(job, [{ kind: 'retired', id: work.id, verdict: 'failed', detail: String(error) }]);
      }
    }
  }
  async function apply(job: Job, packet: Extract<WorkerPacket, { kind: 'update' }>): Promise<void> {
    const update = packet.update;
    if (
      packet.checkpoint !== job.checkpoint + 1 ||
      !update ||
      !Array.isArray(update.packets) ||
      !update.packets.length ||
      update.packets.length > 4 ||
      !Array.isArray(update.append) ||
      update.append.length > 16 ||
      !Array.isArray(update.dispatch) ||
      update.dispatch.length > job.workerCount ||
      !Array.isArray(update.stop) ||
      !Array.isArray(packet.receipts) ||
      packet.receipts.length > 16 ||
      typeof update.done !== 'boolean' ||
      !update.recovery
    )
      throw new Error('Invalid browser coordinator checkpoint.');
    if (
      !update.scheduling ||
      !Array.isArray(update.scheduling.budgets) ||
      update.scheduling.budgets.reduce((a, b) => a + b, 0) !== job.workerCount ||
      !Number.isSafeInteger(update.scheduling.active) ||
      update.scheduling.active < 0 ||
      update.scheduling.active > job.workerCount ||
      !Number.isSafeInteger(packet.rustBytes) ||
      packet.rustBytes < 0 ||
      new Set(update.dispatch.map((work) => work.id)).size !== update.dispatch.length ||
      update.dispatch.some(
        (work) =>
          typeof work.id !== 'string' || typeof work.source !== 'string' || work.source.length > 16 * 1024 * 1024,
      ) ||
      update.stop.length > job.workerCount ||
      update.stop.some((id) => typeof id !== 'string')
    )
      throw new Error('Invalid browser scheduler observations.');
    let current = job.current;
    let completed: JobSnapshot | undefined;
    for (const [index, snapshot] of update.packets.entries()) {
      assertSnapshot(snapshot, job.id);
      if (snapshot.sequence !== (current.sequence ?? 0) + 1 || snapshot.resultAppended || snapshot.results.length)
        throw new Error('Invalid paged browser snapshot sequence.');
      if (terminal(snapshot)) {
        if (!update.done || index !== update.packets.length - 1 || snapshot.resultsOmitted)
          throw new Error('Invalid browser terminal proposal.');
        completed = materializeSnapshot(current, snapshot);
      } else {
        current = materializeSnapshot(current, snapshot);
      }
    }
    if (
      Boolean(completed) !== update.done ||
      !Number.isSafeInteger(update.count) ||
      update.count !== (job.current.collection?.count ?? 0) + update.append.length
    )
      throw new Error('Browser collection count or completion is inconsistent.');
    for (const reason of ['cancelled', 'failed'] as const) {
      const recovery = update.recovery[reason];
      assertSnapshot(recovery, job.id);
      if (
        recovery.status !== reason ||
        !recovery.resultsOmitted ||
        recovery.resultAppended ||
        recovery.resultsLen !== 0 ||
        recovery.enumerationComplete ||
        recovery.sequence !== current.sequence! + 1
      )
        throw new Error('Invalid browser interruption packet.');
    }
    let ref = job.current.collection;
    let storageError: unknown;
    if (update.append.length) {
      if (!ref) throw new Error('Enumeration batch without a collection.');
      try {
        ref = await collections().append(ref, update.append);
      } catch (error) {
        ref = collections().retain(ref, update.append);
        storageError = error;
      }
    }
    if (ref) {
      ref = { ...ref, preferredIndex: update.preferredIndex };
      checkCollectionRef(ref);
    }
    job.checkpoint = packet.checkpoint;
    job.current = { ...current, collection: ref };
    job.recovery = update.recovery;
    job.scheduling = update.scheduling;
    job.rustHeap = packet.rustBytes;
    if (storageError)
      throw new Error(
        `Could not store the accepted solution batch. It is retained in this tab for export or a save retry. ${String(storageError)}`,
      );
    if (job.requested) return;
    const acknowledgeLater: { slot: Slot; receipt: Receipt }[] = [];
    for (const receipt of packet.receipts) {
      const slot = job.tasks.get(receipt.task);
      if (!slot && job.retired.has(receipt.task)) continue;
      if (!slot || slot.attempt !== receipt.workerAttempt || !slot.waiting || slot.sequence !== receipt.sequence)
        throw new Error('Invalid browser compute receipt.');
      acknowledgeLater.push({ slot, receipt });
      slot.waiting = false;
      if (slot.reported) {
        slot.task = null;
        job.tasks.delete(receipt.task);
      }
    }
    for (const task of update.stop) {
      const slot = job.tasks.get(task);
      if (!slot) continue;
      killSlot(slot);
      slot.task = null;
      job.tasks.delete(task);
      retired(job, task);
      if (!slot.reported) forward(job, [{ kind: 'retired', id: task, verdict: 'cancelled', detail: '' }]);
    }
    if (completed) {
      if (job.tasks.size || update.dispatch.length || update.scheduling.active !== 0)
        throw new Error('Browser completion retained active compute work.');
      job.current = { ...completed, collection: ref };
      seal(job);
      return;
    }
    emit(job);
    if (job.requested) return;
    for (const { slot, receipt } of acknowledgeLater) acknowledge(job, slot, receipt);
    for (const work of update.dispatch) {
      if (update.stop.includes(work.id))
        forward(job, [{ kind: 'retired', id: work.id, verdict: 'cancelled', detail: '' }]);
      else dispatch(job, work);
    }
    job.worker?.postMessage({
      kind: 'ack',
      protocol: BROWSER_SOLVER_PROTOCOL,
      jobId: job.id,
      attempt: job.attempt,
      checkpoint: packet.checkpoint,
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
