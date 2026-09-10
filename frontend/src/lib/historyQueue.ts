import { cancelJob, createJob, getJob, releaseJob, watchJob, type JobWatch } from './api';
import { assembleEntries, isTerminalJobStatus, partitionEntries, type HistoryEntry } from './historyModel';
import type { JobSnapshot, SolveRequest } from '../types';

export type HistoryQueueHost = {
  getEntries: () => HistoryEntry[];
  setEntries: (entries: HistoryEntry[]) => void;
  patchEntry: (id: string, patch: Partial<HistoryEntry>) => void;
  getSelectedId: () => string | null;
  flushSelectedChrome: () => void;
  syncViewIfSelected: (entryId: string, entry: HistoryEntry) => void;
  setError: (message: string) => void;
  setElapsedMs: (ms: number) => void;
};

function isAlreadyRunningError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return /already running/i.test(message);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function awaitJobTerminal(jobId: string, timeoutMs = 60_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const snapshot = await getJob(jobId);
      if (isTerminalJobStatus(snapshot.status)) return;
    } catch {
      return;
    }
    await sleep(50);
  }
}

async function createJobWhenIdle(request: SolveRequest): Promise<string> {
  const deadline = Date.now() + 60_000;
  for (;;) {
    try {
      return await createJob(request);
    } catch (error) {
      if (!isAlreadyRunningError(error) || Date.now() >= deadline) throw error;
      await sleep(100);
    }
  }
}

/** One-at-a-time solver queue runner over history entries. */
export class HistoryQueue {
  private startingJob = false;
  private jobWatch: JobWatch | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor(private readonly host: HistoryQueueHost) {}

  dispose(): void {
    this.resetTransport();
  }

  async cancel(): Promise<void> {
    const running = partitionEntries(this.host.getEntries()).running;
    if (!running?.jobId) return;
    try {
      await cancelJob(running.jobId);
    } catch (error) {
      this.host.setError(error instanceof Error ? error.message : String(error));
    }
  }

  async pump(): Promise<void> {
    if (this.startingJob) return;
    const parts = partitionEntries(this.host.getEntries());
    if (parts.running || parts.queued.length === 0) return;
    this.startingJob = true;
    try {
      // Queue stacks newest-first; always start the bottom (oldest) entry.
      const nextId = parts.queued[parts.queued.length - 1].id;
      await this.startQueuedEntry(nextId);
    } finally {
      this.startingJob = false;
    }
    // Re-check after startingJob clears: a fast job can finish while startup
    // still held the lock, so acceptSnapshot's pump no-oped.
    const next = partitionEntries(this.host.getEntries());
    if (!next.running && next.queued.length > 0) {
      queueMicrotask(() => {
        void this.pump();
      });
    }
  }

  private resetTransport(): void {
    this.jobWatch?.close();
    this.jobWatch = null;
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  private findEntryIdByJob(jobId: string): string | null {
    return this.host.getEntries().find((entry) => entry.jobId === jobId)?.id ?? null;
  }

  private acceptSnapshot(snapshot: JobSnapshot): void {
    const entryId = this.findEntryIdByJob(snapshot.jobId);
    if (!entryId) return;
    const previous = this.host.getEntries().find((entry) => entry.id === entryId);
    if (!previous) return;

    if (snapshot.sequence != null && previous.sequence != null && snapshot.sequence < previous.sequence) return;
    const omitted = Boolean(snapshot.resultsOmitted);
    if (omitted && isTerminalJobStatus(snapshot.status)) {
      void getJob(snapshot.jobId)
        .then((full) => this.acceptSnapshot(full))
        .catch(() => this.host.setError('Lost final solver results.'));
      return;
    }
    const appended = Boolean(snapshot.resultAppended);

    if (appended && snapshot.result) {
      if (isTerminalJobStatus(previous.status) || isTerminalJobStatus(snapshot.status)) {
        return;
      }
      const next = snapshot.result;
      const priorResults = previous.results;
      const seq = snapshot.resultsLen ?? 0;
      if (seq > 0 && priorResults.length === seq - 1) {
        const results = [...priorResults, next];
        this.host.patchEntry(entryId, {
          status: snapshot.status,
          progress: snapshot.progress,
          proof: snapshot.proof ?? previous.proof,
          sequence: snapshot.sequence,
          result: previous.result ?? next,
          results,
          enumerationComplete: snapshot.enumerationComplete,
          error: snapshot.error,
          startedAtMs: snapshot.startedAtMs,
        });
        const updated = this.host.getEntries().find((entry) => entry.id === entryId);
        if (updated) this.host.syncViewIfSelected(entryId, updated);
        return;
      }
      if (seq > 0 && priorResults.length < seq - 1) {
        void getJob(snapshot.jobId)
          .then((full) => this.acceptSnapshot(full))
          .catch(() => {
            const running = partitionEntries(this.host.getEntries()).running;
            if (running?.jobId === snapshot.jobId) {
              this.host.setError('Lost live updates from the solver.');
            }
          });
      }
      return;
    }

    if (omitted && (snapshot.resultsLen ?? 0) > previous.results.length) {
      void getJob(snapshot.jobId)
        .then((full) => this.acceptSnapshot(full))
        .catch(() => this.host.setError('Lost live updates from the solver.'));
    }
    const results = omitted ? previous.results : snapshot.results;
    const result = omitted ? (previous.result ?? snapshot.result) : snapshot.result;
    const terminal = isTerminalJobStatus(snapshot.status);
    this.host.patchEntry(entryId, {
      status: snapshot.status,
      progress: snapshot.progress ?? previous.progress,
      proof: snapshot.proof ?? previous.proof,
      sequence: snapshot.sequence,
      result,
      results,
      enumerationComplete: snapshot.enumerationComplete,
      error: snapshot.error,
      startedAtMs: snapshot.startedAtMs,
      finishedAtMs: terminal ? (previous.finishedAtMs ?? Date.now()) : previous.finishedAtMs,
      jobId: terminal ? null : snapshot.jobId,
    });

    if (!omitted) {
      const updated = this.host.getEntries().find((entry) => entry.id === entryId);
      if (updated) this.host.syncViewIfSelected(entryId, updated);
    }

    if ((snapshot.status === 'failed' || snapshot.status === 'unsat') && this.host.getSelectedId() === entryId) {
      this.host.setError(snapshot.error ?? 'The solver failed.');
    }

    if (terminal) {
      // Full terminal results are now owned by history. Late replies cannot change a new job.
      void releaseJob(snapshot.jobId).catch(() => {
        /* Bounded backend retention is the fallback. */
      });
      this.resetTransport();
      if (this.host.getSelectedId() === entryId) this.host.flushSelectedChrome();
      const parts = partitionEntries(this.host.getEntries());
      this.host.setEntries(assembleEntries(parts.queued, parts.running, parts.history));
      void this.pump();
    }
  }

  private async startQueuedEntry(entryId: string): Promise<boolean> {
    const entry = this.host.getEntries().find((item) => item.id === entryId);
    if (!entry || entry.status !== 'queued') return false;
    try {
      const jobId = await createJobWhenIdle(entry.request);
      if (!this.host.getEntries().some((item) => item.id === entryId)) {
        try {
          await cancelJob(jobId);
        } catch {
          /* best-effort */
        }
        await awaitJobTerminal(jobId);
        return false;
      }
      const startedAtMs = Date.now();
      const updatedAtMs = Date.now();
      const nextEntries = this.host.getEntries().map((item) =>
        item.id === entryId
          ? {
              ...item,
              status: 'running' as const,
              jobId,
              startedAtMs,
              finishedAtMs: null,
              progress: null,
              proof: null,
              sequence: undefined,
              error: null,
              updatedAtMs,
            }
          : item,
      );
      const parts = partitionEntries(nextEntries);
      this.host.setEntries(assembleEntries(parts.queued, parts.running, parts.history));

      this.resetTransport();
      this.timer = setInterval(() => {
        const running = partitionEntries(this.host.getEntries()).running;
        if (running?.startedAtMs && this.host.getSelectedId() === running.id) {
          this.host.setElapsedMs(Math.max(0, Date.now() - running.startedAtMs));
        }
      }, 100);

      const watch = await watchJob(
        jobId,
        (snapshot) => this.acceptSnapshot(snapshot),
        () => {
          const running = partitionEntries(this.host.getEntries()).running;
          if (running?.jobId === jobId) {
            this.host.setError('Lost live updates from the solver.');
          }
        },
      );
      const stillRunning = partitionEntries(this.host.getEntries()).running;
      if (stillRunning?.jobId === jobId) this.jobWatch = watch;
      else watch.close();
      return true;
    } catch (error) {
      if (this.host.getEntries().some((item) => item.id === entryId)) {
        this.host.patchEntry(entryId, {
          status: 'failed',
          finishedAtMs: Date.now(),
          error: error instanceof Error ? error.message : String(error),
        });
      }
      this.host.setError(error instanceof Error ? error.message : String(error));
      return false;
    }
  }
}
