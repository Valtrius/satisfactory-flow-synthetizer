import type { JobSnapshot } from '../../types';
import { coordinatorQueueLimit } from '../solver/inbox';
import {
  BROWSER_SOLVER_PROTOCOL,
  type ComputeEvent,
  type Dispatch,
  type LeafPacket,
  type Receipt,
} from '../solver/protocol';
import type { BrowserJobOptions, Job, Slot, Stop } from './browserJobState';

type PoolOptions = Pick<BrowserJobOptions, 'startupTimeoutMs' | 'resourceLimit'> & {
  spawn: NonNullable<BrowserJobOptions['createWorker']>;
  observe(job: Job): void;
  stop(job: Job, reason: Stop['reason'], detail?: string): Promise<JobSnapshot>;
};

/** Own compute slots and receipts; terminal sealing remains with the job registry. */
export function createBrowserComputePool(options: PoolOptions) {
  const { spawn, observe: decorate, stop } = options;
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
  function acceptReceipts(job: Job, receipts: Receipt[]) {
    const acknowledgeLater: { slot: Slot; receipt: Receipt }[] = [];
    for (const receipt of receipts) {
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
    return acknowledgeLater;
  }
  function stopTasks(job: Job, tasks: string[]): void {
    for (const task of tasks) {
      const slot = job.tasks.get(task);
      if (!slot) continue;
      killSlot(slot);
      slot.task = null;
      job.tasks.delete(task);
      retired(job, task);
      if (!slot.reported) forward(job, [{ kind: 'retired', id: task, verdict: 'cancelled', detail: '' }]);
    }
  }
  function resume(
    job: Job,
    acknowledgeLater: { slot: Slot; receipt: Receipt }[],
    dispatches: Dispatch[],
    stopped: string[],
  ): void {
    for (const { slot, receipt } of acknowledgeLater) acknowledge(job, slot, receipt);
    for (const work of dispatches) {
      if (stopped.includes(work.id)) forward(job, [{ kind: 'retired', id: work.id, verdict: 'cancelled', detail: '' }]);
      else dispatch(job, work);
    }
  }
  return { killSlot, acceptReceipts, stopTasks, resume };
}
export type BrowserComputePool = ReturnType<typeof createBrowserComputePool>;
