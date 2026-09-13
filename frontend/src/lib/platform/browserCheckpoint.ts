import type { JobSnapshot } from '../../types';
import type { CollectionStore } from './contracts';
import type { Job } from './browserJobState';
import type { BrowserComputePool } from './browserComputePool';
import { checkCollectionRef } from './browserCollections';
import { BROWSER_SOLVER_PROTOCOL, type WorkerPacket } from '../solver/protocol';
import { assertSnapshot, materializeSnapshot, terminal } from '../solver/snapshots';

/** Validate a coordinator checkpoint and commit its graph batch before acknowledgements. */
export function createBrowserCheckpoint({
  collections,
  pool,
  emit,
  seal,
}: {
  collections(): CollectionStore;
  pool: BrowserComputePool;
  emit(job: Job): void;
  seal(job: Job): void;
}) {
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
    const acknowledgeLater = pool.acceptReceipts(job, packet.receipts);
    pool.stopTasks(job, update.stop);
    if (completed) {
      if (job.tasks.size || update.dispatch.length || update.scheduling.active !== 0)
        throw new Error('Browser completion retained active compute work.');
      job.current = { ...completed, collection: ref };
      seal(job);
      return;
    }
    emit(job);
    if (job.requested) return;
    pool.resume(job, acknowledgeLater, update.dispatch, update.stop);
    job.worker?.postMessage({
      kind: 'ack',
      protocol: BROWSER_SOLVER_PROTOCOL,
      jobId: job.id,
      attempt: job.attempt,
      checkpoint: packet.checkpoint,
    });
  }
  return apply;
}
