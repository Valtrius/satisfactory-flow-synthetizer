import { afterEach, describe, expect, it, vi } from 'vitest';
import { createBrowserJobs, type BrowserJobOptions } from './browserJobs';
import { emptySnapshot } from '../solver/snapshots';
import { historyEntry } from '../../test/fixtures';
import type { JobSnapshot } from '../../types';
import type { Recovery, Start } from '../solver/protocol';

class FakeWorker {
  onmessage: ((event: MessageEvent) => void) | null = null;
  onmessageerror: (() => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  messages: unknown[] = [];
  terminate = vi.fn();
  postMessage(value: unknown) {
    this.messages.push(structuredClone(value));
  }
  get start(): Start {
    return this.messages[0] as Start;
  }
  send(value: object) {
    this.onmessage?.({
      data: { jobId: this.start.jobId, attempt: this.start.attempt, protocol: 1, ...value },
    } as MessageEvent);
  }
  update(checkpoint: number, packets: JobSnapshot[], done = false) {
    const last = packets.at(-1)!;
    // These are transport fixtures. Real proof/status projection is tested in Rust.
    const base = {
      ...last,
      sequence: last.sequence! + (done ? 0 : 1),
      result: null,
      results: [],
      resultsLen: last.resultAppended ? last.resultsLen : last.resultsOmitted ? last.resultsLen : last.results.length,
      resultsOmitted: true,
      resultAppended: false,
      enumerationComplete: false,
      proof: { minimumNodeCount: null, minimumLinkCount: null },
      error: null,
    };
    const recovery: Recovery = { cancelled: { ...base, status: 'cancelled' }, failed: { ...base, status: 'failed' } };
    this.send({ kind: 'update', checkpoint, packets, recovery, done });
  }
}

const clients: ReturnType<typeof createBrowserJobs>[] = [];
afterEach(async () => {
  for (const client of clients.splice(0)) await client.shutdown();
  vi.useRealTimers();
});

function setup(options: BrowserJobOptions = {}) {
  const workers: FakeWorker[] = [];
  const jobs = createBrowserJobs({
    solverBase: 'https://example.test/app/solver/',
    createWorker: () => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker as unknown as Worker;
    },
    ...options,
  });
  clients.push(jobs);
  return { jobs, workers };
}

function snapshot(worker: FakeWorker, patch: Partial<JobSnapshot> = {}): JobSnapshot {
  return { ...emptySnapshot(worker.start.jobId, worker.start.startedAtMs), sequence: 1, ...patch };
}

function append(worker: FakeWorker, sequence = 1, count = 1): JobSnapshot {
  return snapshot(worker, {
    sequence,
    result: { ...historyEntry().results[0], status: 'best_known', proof: null },
    resultAppended: true,
    resultsLen: count,
  });
}

describe('single-worker browser job ownership', () => {
  it('is lazy, admits one job, clones its request, and reopens admission without resuming old work', async () => {
    const { jobs, workers } = setup();
    expect(workers).toHaveLength(0);
    const request = structuredClone(historyEntry().request);
    const id = await jobs.create(request);
    const original = workers[0].start.request.beltRate;
    request.beltRate = 'invalid mutation';
    expect(workers[0].start.request.beltRate).toBe(original);
    await expect(jobs.create(historyEntry().request)).rejects.toThrow('already running');
    await expect(jobs.release(id)).rejects.toThrow('active');
    expect((await jobs.shutdown())[0].status).toBe('cancelled');
    expect(workers[0].terminate).toHaveBeenCalledOnce();
    await expect(jobs.create(historyEntry().request)).rejects.toThrow('admission is closed');
    await jobs.resume();
    expect(workers).toHaveLength(1);
    await jobs.create(historyEntry().request);
    expect(workers).toHaveLength(2);
  });

  it('retains append graphs and reconciles missed subscriptions with a full get/watch snapshot', async () => {
    const { jobs, workers } = setup();
    const id = await jobs.create(historyEntry().request);
    const worker = workers[0];
    worker.update(1, [append(worker)]);
    worker.update(2, [append(worker, 2, 2)]);
    worker.update(3, [snapshot(worker, { sequence: 3, resultsOmitted: true, resultsLen: 2 })]);
    const receive = vi.fn();
    const watch = await jobs.watch(id, receive, vi.fn());
    expect(receive.mock.calls[0][0].results).toHaveLength(2);
    const full = await jobs.get(id);
    expect(full.resultsOmitted).toBe(false);
    full.results.length = 0;
    receive.mock.calls[0][0].results.length = 0;
    expect((await jobs.get(id)).results).toHaveLength(2);
    watch.close();
    watch.close();
    worker.update(4, [snapshot(worker, { sequence: 4, resultsOmitted: true, resultsLen: 2 })]);
    expect(receive).toHaveBeenCalledOnce();
  });

  it('keeps accepted witnesses when a watcher cancels before acknowledging the next compute step', async () => {
    const { jobs, workers } = setup();
    const id = await jobs.create(historyEntry().request);
    const worker = workers[0];
    const receive = vi.fn((value: JobSnapshot) => {
      if (value.resultAppended) void jobs.cancel(id);
    });
    await jobs.watch(id, receive, vi.fn());
    worker.update(1, [append(worker)]);
    const final = await jobs.get(id);
    expect(final.status).toBe('cancelled');
    expect(final.result?.status).toBe('best_known');
    expect(final.results).toHaveLength(1);
    expect(final.enumerationComplete).toBe(false);
    expect(final.sequence).toBe(2);
    expect(worker.messages).toHaveLength(1); // No ACK enters the next blocking call.
    expect(worker.terminate).toHaveBeenCalledOnce();
    expect(receive.mock.calls.at(-1)![0].results).toHaveLength(1);
  });

  it('cancellation before completion wins, but cancellation after the host seal cannot rewrite it', async () => {
    for (const cancelFirst of [true, false]) {
      const { jobs, workers } = setup();
      const id = await jobs.create(historyEntry().request);
      const worker = workers[0];
      worker.update(1, [append(worker)]);
      const listener = worker.onmessage!;
      const complete = snapshot(worker, {
        sequence: 2,
        status: 'completed',
        result: historyEntry().results[0],
        results: [historyEntry().results[0]],
        enumerationComplete: true,
      });
      if (cancelFirst) await jobs.cancel(id);
      // Capture even packets already queued before terminate nulled onmessage.
      worker.onmessage = listener;
      worker.update(2, [complete], true);
      const result = await jobs.cancel(id);
      expect(result.status).toBe(cancelFirst ? 'cancelled' : 'completed');
      expect(result.enumerationComplete).toBe(!cancelFirst);
      expect(worker.terminate).toHaveBeenCalledOnce();
    }
  });

  it('ignores foreign attempts and packets queued by a retired worker after replacement', async () => {
    const { jobs, workers } = setup();
    const first = await jobs.create(historyEntry().request);
    const old = workers[0];
    const queued = old.onmessage!;
    old.send({ kind: 'failed', attempt: 'other-attempt', error: 'must not apply' });
    expect((await jobs.get(first)).status).toBe('running');
    await jobs.cancel(first);
    const second = await jobs.create(historyEntry().request);
    queued({ data: { ...old.start, kind: 'failed', error: 'late failure' } } as MessageEvent);
    expect((await jobs.get(second)).status).toBe('running');
    expect((await jobs.get(first)).status).toBe('cancelled');
    await jobs.release(first);
    await jobs.release(first);
    await expect(jobs.get(first)).rejects.toThrow('Unknown');
  });

  it.each(['error', 'messageerror', 'failed'] as const)(
    'preserves an accepted witness after worker %s',
    async (kind) => {
      const { jobs, workers } = setup();
      const id = await jobs.create(historyEntry().request);
      const worker = workers[0];
      worker.update(1, [append(worker)]);
      if (kind === 'error') worker.onerror?.({ preventDefault() {}, message: 'Wasm trap' } as ErrorEvent);
      else if (kind === 'messageerror') worker.onmessageerror?.();
      else worker.send({ kind: 'failed', error: 'Out of memory' });
      const final = await jobs.get(id);
      expect(final.status).toBe('failed');
      expect(final.results).toHaveLength(1);
      expect(final.enumerationComplete).toBe(false);
      expect(final.proof?.minimumLinkCount).toBeNull();
      expect(final.error).toBeTruthy();
      expect(worker.terminate).toHaveBeenCalledOnce();
      await jobs.create(historyEntry().request);
      expect(workers).toHaveLength(2);
    },
  );

  it.each(['protocol', 'checkpoint', 'sequence', 'append gap', 'thin terminal', 'recovery'] as const)(
    'rejects a bad %s without accepting new proof or losing earlier graphs',
    async (fault) => {
      const { jobs, workers } = setup();
      const id = await jobs.create(historyEntry().request);
      const worker = workers[0];
      worker.update(1, [append(worker)]);
      const next = snapshot(worker, { sequence: 2, resultsOmitted: true, resultsLen: 1 });
      if (fault === 'protocol') worker.send({ kind: 'ready', protocol: 9 });
      else if (fault === 'checkpoint') worker.update(3, [next]);
      else if (fault === 'sequence') worker.update(2, [{ ...next, sequence: 3 }]);
      else if (fault === 'append gap') worker.update(2, [append(worker, 2, 3)]);
      else if (fault === 'thin terminal') worker.update(2, [{ ...next, status: 'completed' }], true);
      else worker.send({ kind: 'update', checkpoint: 2, packets: [next], recovery: {}, done: false });
      const final = await jobs.get(id);
      expect(final.status).toBe('failed');
      expect(final.results).toHaveLength(1);
      expect(final.enumerationComplete).toBe(false);
      expect(final.sequence).toBe(2);
      expect(final.proof?.minimumLinkCount).toBeNull();
    },
  );

  it('bounds startup but imposes no solve timeout after the backend is ready', async () => {
    vi.useFakeTimers();
    const { jobs, workers } = setup({ startupTimeoutMs: 100 });
    const failed = await jobs.create(historyEntry().request);
    await vi.advanceTimersByTimeAsync(101);
    expect((await jobs.get(failed)).status).toBe('failed');
    const active = await jobs.create(historyEntry().request);
    workers[1].send({ kind: 'ready' });
    await vi.advanceTimersByTimeAsync(1_000_000);
    expect((await jobs.get(active)).status).toBe('running');
  });

  it('bounds retained terminal snapshots and removes listener ownership on release', async () => {
    const { jobs } = setup({ retention: 2 });
    const ids: string[] = [];
    for (let index = 0; index < 5; index++) {
      const id = await jobs.create(historyEntry().request);
      ids.push(id);
      await jobs.cancel(id);
    }
    await expect(jobs.get(ids[0])).rejects.toThrow('Unknown');
    expect((await jobs.get(ids[3])).status).toBe('cancelled');
    expect((await jobs.get(ids[4])).status).toBe('cancelled');
  });

  it('isolates throwing consumers and reports construction failure without keeping a worker slot', async () => {
    const { jobs, workers } = setup();
    const id = await jobs.create(historyEntry().request);
    await jobs.watch(
      id,
      () => {
        throw new Error('consumer');
      },
      () => {
        throw new Error('consumer error');
      },
    );
    workers[0].update(1, [append(workers[0])]);
    expect((await jobs.get(id)).results).toHaveLength(1);
    const failed = setup({
      createWorker: () => {
        throw new Error('Worker blocked by CSP');
      },
    }).jobs;
    const first = await failed.create(historyEntry().request);
    expect((await failed.get(first)).error).toContain('Worker blocked by CSP');
    await failed.create(historyEntry().request);
    await expect(failed.create({ ...historyEntry().request, beltRate: '1'.repeat(262_145) })).rejects.toThrow(
      '256 KiB',
    );
  });
});
