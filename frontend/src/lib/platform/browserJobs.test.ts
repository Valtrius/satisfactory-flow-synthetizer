import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createBrowserJobs, type BrowserJobOptions } from './browserJobs';
import { emptySnapshot } from '../solver/snapshots';
import { historyEntry } from '../../test/fixtures';
import type { CollectionRef, JobSnapshot, Solution } from '../../types';
import type { CollectionStore, IndexedSolution } from './contracts';
import type { Dispatch, Events, LeafStart, Receipt, RunUpdate, Start } from '../solver/protocol';

const best = (): Solution => ({ ...structuredClone(historyEntry().result!), status: 'best_known', proof: null });
class FakeWorker {
  onmessage: ((event: MessageEvent) => void) | null = null;
  onmessageerror: (() => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  messages: Record<string, unknown>[] = [];
  terminate = vi.fn();
  checkpoint = 0;
  sequence = 0;
  count = 0;
  preferred: Solution | null = null;
  postMessage(value: Record<string, unknown>) {
    this.messages.push(structuredClone(value));
  }
  get start(): Start {
    return this.messages.find((message) => message.kind === 'start') as unknown as Start;
  }
  get leaf(): LeafStart {
    return this.messages.findLast((message) => message.kind === 'leaf') as unknown as LeafStart;
  }
  send(value: object) {
    const identity = this.start ?? this.leaf;
    this.onmessage?.({
      data: { jobId: identity.jobId, attempt: identity.attempt, protocol: 2, ...value },
    } as MessageEvent);
  }
  update(
    options: {
      dispatch?: Dispatch[];
      append?: IndexedSolution[];
      receipts?: Receipt[];
      stop?: string[];
      done?: boolean;
      status?: JobSnapshot['status'];
      active?: number;
    } = {},
  ) {
    const append = options.append ?? [];
    this.count += append.length;
    if (append.length) this.preferred ??= append[0].solution;
    const snapshot = {
      ...emptySnapshot(this.start.jobId, this.start.startedAtMs),
      sequence: ++this.sequence,
      result: this.preferred,
      progress: null,
    };
    // Transport fixtures only. Rust tests own the proof/status decisions.
    const recovery = {
      ...snapshot,
      sequence: this.sequence + 1,
      result: null,
      results: [],
      resultsLen: 0,
      resultsOmitted: true,
      proof: { minimumNodeCount: null, minimumLinkCount: null },
    };
    const packets: JobSnapshot[] = [snapshot];
    if (options.done)
      packets.push({
        ...snapshot,
        sequence: ++this.sequence,
        status: options.status ?? 'completed',
        enumerationComplete: true,
      });
    const update: RunUpdate = {
      dispatch: options.dispatch ?? [],
      stop: options.stop ?? [],
      append,
      count: this.count,
      preferredIndex: this.count ? 0 : null,
      recovery: { cancelled: { ...recovery, status: 'cancelled' }, failed: { ...recovery, status: 'failed' } },
      packets,
      done: Boolean(options.done),
      scheduling: {
        dispatched: 0,
        secondOutputRoots: 0,
        adaptiveGroups: 0,
        adaptiveChildren: 0,
        peakActive: 0,
        active: options.active ?? 0,
        identityBytes: 0,
        proofOwner: options.done ? 0 : null,
        budgets: [this.start.options.workerCount],
      },
    };
    const packet = {
      kind: 'update',
      checkpoint: ++this.checkpoint,
      update,
      receipts: options.receipts ?? [],
      rustBytes: 65536,
    };
    this.send(packet);
    return packet;
  }
  event(kind: 'witness' | 'retired', sequence = 1) {
    const task = this.leaf;
    this.send({
      kind: 'leaf-events',
      workerAttempt: task.workerAttempt,
      task: task.task,
      sequence,
      rustBytes: 65536,
      heapBytes: 32 * 1024 * 1024,
      events: [
        kind === 'witness'
          ? { kind, id: task.task, witness: {} }
          : { kind, id: task.task, verdict: 'exhausted', detail: '' },
      ],
    });
  }
}

function memoryCollections() {
  const values = new Map<string, Solution[]>();
  const store: CollectionStore = {
    create: vi.fn(async (id: string): Promise<CollectionRef> => {
      values.set(id, []);
      return { version: 1, id, count: 0, preferredIndex: null };
    }),
    append: vi.fn(async (ref: CollectionRef, rows: IndexedSolution[]) => {
      values.get(ref.id)!.push(...structuredClone(rows.map((row) => row.solution)));
      return { ...ref, count: ref.count + rows.length };
    }),
    retain: vi.fn((ref: CollectionRef, rows: IndexedSolution[]) => {
      values.get(ref.id)!.push(...structuredClone(rows.map((row) => row.solution)));
      return { ...ref, count: ref.count + rows.length, pending: true };
    }),
    flush: vi.fn(async () => {}),
    forget: vi.fn(),
    discard: vi.fn(async () => {}),
    read: async (ref, offset, limit) =>
      values
        .get(ref.id)!
        .slice(offset, offset + limit)
        .map((solution, index) => ({ index: offset + index, solution })),
    get: async (ref, index) => values.get(ref.id)![index],
    page: async (ref, offset, limit) => ({
      offset,
      total: ref.count,
      rows: values
        .get(ref.id)!
        .slice(offset, offset + limit)
        .map((solution, index) => ({ sourceIndex: offset + index, solution: { stats: solution.stats } })),
    }),
  };
  return store;
}

const clients: ReturnType<typeof createBrowserJobs>[] = [];
beforeEach(() => {
  vi.spyOn(navigator, 'hardwareConcurrency', 'get').mockReturnValue(64);
});
const releases: (() => void)[] = [];
afterEach(async () => {
  for (const release of releases.splice(0)) release();
  for (const jobs of clients.splice(0)) await jobs.shutdown();
  vi.useRealTimers();
  vi.restoreAllMocks();
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
function setup(options: BrowserJobOptions = {}) {
  const workers: { role: string; worker: FakeWorker }[] = [];
  const collections = memoryCollections();
  const jobs = createBrowserJobs({
    collections,
    workerCount: 1,
    solverBase: 'https://example.test/app/solver/',
    createWorker: (role) => {
      const worker = new FakeWorker();
      workers.push({ role, worker });
      return worker as unknown as Worker;
    },
    ...options,
  });
  clients.push(jobs);
  return { jobs, workers, collections };
}
async function running(options: BrowserJobOptions = {}) {
  const state = setup(options);
  const id = await state.jobs.create(structuredClone(historyEntry().request));
  const coordinator = state.workers[0].worker;
  coordinator.update({ dispatch: [{ id: '0:0:0', branch: 0, source: '{}' }], active: 1 });
  await vi.waitFor(() => expect(state.workers).toHaveLength(2));
  return { ...state, id, coordinator, compute: state.workers[1].worker };
}
function receipts(coordinator: FakeWorker): Receipt[] {
  return (coordinator.messages.findLast((message) => message.kind === 'events') as unknown as Events).receipts;
}
async function accepted(state: Awaited<ReturnType<typeof running>>) {
  state.compute.event('witness');
  state.coordinator.update({
    append: [{ index: 0, solution: best() }],
    receipts: receipts(state.coordinator),
    active: 1,
  });
  await vi.waitFor(async () => expect((await state.jobs.get(state.id)).collection?.count).toBe(1));
}

describe('independent browser workers and paged job ownership', () => {
  it('defaults to the reported thread count and accepts a full 64-worker dispatch', async () => {
    const state = setup({ workerCount: undefined });
    const id = await state.jobs.create(historyEntry().request);
    const coordinator = state.workers[0].worker;
    expect(coordinator.start.options.workerCount).toBe(64);
    coordinator.update({
      dispatch: Array.from({ length: 64 }, (_, index) => ({ id: `task-${index}`, branch: index % 2, source: '{}' })),
      active: 64,
    });
    await vi.waitFor(() => expect(state.workers).toHaveLength(65));
    await state.jobs.cancel(id);
    for (const { worker } of state.workers) expect(worker.terminate).toHaveBeenCalledOnce();
  });
  it('seals with its accepted prefix when forwarding a compute event throws', async () => {
    const state = await running();
    await accepted(state);
    state.coordinator.postMessage = () => {
      throw new DOMException('Transport unavailable', 'DataCloneError');
    };
    state.compute.event('retired', 2);
    await vi.waitFor(async () => expect((await state.jobs.get(state.id)).status).toBe('failed'));
    const final = await state.jobs.get(state.id);
    expect(final.collection?.count).toBe(1);
    expect(final.result?.status).toBe('best_known');
    expect(final.error).toContain('Could not reach browser coordinator');
    expect(state.compute.terminate).toHaveBeenCalledOnce();
    expect(state.coordinator.terminate).toHaveBeenCalledOnce();
  });

  it.each(['failure', 'ready'] as const)(
    'rejects a malformed coordinator %s without leaving a live job',
    async (kind) => {
      const state = await running();
      await accepted(state);
      state.coordinator.send(kind === 'failure' ? { kind: 'failed', error: 17 } : { kind: 'ready', rustBytes: NaN });
      await vi.waitFor(async () => expect((await state.jobs.get(state.id)).status).toBe('failed'));
      expect((await state.jobs.get(state.id)).collection?.count).toBe(1);
    },
  );

  it('retires malformed compute failures instead of throwing from its event handler', async () => {
    const state = await running();
    const task = state.compute.leaf;
    state.compute.send({ kind: 'failed', task: task.task, workerAttempt: task.workerAttempt, error: 17 });
    expect(state.compute.terminate).toHaveBeenCalledOnce();
    expect(state.coordinator.messages.at(-1)).toMatchObject({
      kind: 'events',
      events: [{ kind: 'retired', verdict: 'failed' }],
    });
  });

  it('is lazy, preserves the request and admits one bounded job until all ownership ends', async () => {
    const state = setup({ workerCount: 4 });
    expect(state.workers).toHaveLength(0);
    const request = structuredClone(historyEntry().request);
    const id = await state.jobs.create(request);
    request.beltRate = 'mutated';
    expect(state.workers[0].worker.start.request.beltRate).not.toBe('mutated');
    expect(state.workers[0].worker.start.options.workerCount).toBe(4);
    await expect(state.jobs.create(request)).rejects.toThrow('already running');
    await expect(state.jobs.release(id)).rejects.toThrow('active');
    expect((await state.jobs.shutdown())[0].status).toBe('cancelled');
    await expect(state.jobs.create(request)).rejects.toThrow('admission is closed');
    await state.jobs.resume();
    expect(state.workers).toHaveLength(1);
    await state.jobs.create(structuredClone(historyEntry().request));
    expect(state.workers).toHaveLength(2);
  });

  it.each([0, -1, 1.5, 65])('rejects invalid worker budget %s before allocating a collection', async (workerCount) => {
    const state = setup({ workerCount });
    await expect(state.jobs.create(historyEntry().request)).rejects.toThrow('reported thread count');
    expect(state.collections.create).not.toHaveBeenCalled();
    expect(state.workers).toHaveLength(0);
  });

  it('does not acknowledge a witness until its bounded collection transaction commits', async () => {
    const state = await running();
    const pending = deferred<CollectionRef>();
    const ref = (await state.jobs.get(state.id)).collection!;
    releases.push(() => pending.resolve({ ...ref, count: 1 }));
    vi.mocked(state.collections.append).mockReturnValueOnce(pending.promise);
    state.compute.event('witness');
    state.coordinator.update({
      append: [{ index: 0, solution: best() }],
      receipts: receipts(state.coordinator),
      active: 1,
    });
    await vi.waitFor(() => expect(state.collections.append).toHaveBeenCalledOnce());
    expect(state.compute.messages.filter((message) => message.kind === 'leaf-ack')).toHaveLength(0);
    expect((await state.jobs.get(state.id)).collection?.count).toBe(0);
    pending.resolve({ ...ref, count: 1 });
    await vi.waitFor(() =>
      expect(state.compute.messages.filter((message) => message.kind === 'leaf-ack')).toHaveLength(1),
    );
    const full = await state.jobs.get(state.id);
    expect(full.results).toEqual([]);
    expect(full.collection?.count).toBe(1);
    full.collection!.count = 999;
    expect((await state.jobs.get(state.id)).collection?.count).toBe(1);
  });

  it('terminates blocked workers immediately but keeps a received batch when cancellation races its write', async () => {
    const state = await running();
    const pending = deferred<CollectionRef>();
    const ref = (await state.jobs.get(state.id)).collection!;
    releases.push(() => pending.resolve({ ...ref, count: 1 }));
    vi.mocked(state.collections.append).mockReturnValueOnce(pending.promise);
    state.compute.event('witness');
    state.coordinator.update({
      append: [{ index: 0, solution: best() }],
      receipts: receipts(state.coordinator),
      active: 1,
    });
    const cancellation = state.jobs.cancel(state.id);
    expect(state.compute.terminate).toHaveBeenCalledOnce();
    expect(state.coordinator.terminate).toHaveBeenCalledOnce();
    expect((await state.jobs.get(state.id)).status).toBe('cancelling');
    pending.resolve({ ...ref, count: 1 });
    const result = await cancellation;
    expect(result.status).toBe('cancelled');
    expect(result.collection?.count).toBe(1);
    expect(result.result?.status).toBe('best_known');
    expect(result.enumerationComplete).toBe(false);
    expect(result.proof?.minimumLinkCount).toBeNull();
    expect(state.compute.messages.filter((message) => message.kind === 'leaf-ack')).toHaveLength(0);
  });

  it('cancellation from a live subscriber prevents compute from entering its next blocking call', async () => {
    const state = await running();
    await state.jobs.watch(
      state.id,
      (snapshot) => {
        if (snapshot.status === 'running' && snapshot.collection?.count) void state.jobs.cancel(state.id);
      },
      vi.fn(),
    );
    await accepted(state);
    await vi.waitFor(async () => expect((await state.jobs.get(state.id)).status).toBe('cancelled'));
    expect(state.compute.messages.filter((message) => message.kind === 'leaf-ack')).toHaveLength(0);
    expect((await state.jobs.get(state.id)).collection?.count).toBe(1);
  });

  it('one failed compute branch retires without killing the independent healthy branch', async () => {
    const state = setup({ workerCount: 2 });
    const id = await state.jobs.create(historyEntry().request);
    const coordinator = state.workers[0].worker;
    coordinator.update({
      dispatch: [
        { id: 'a', branch: 0, source: '{}' },
        { id: 'b', branch: 1, source: '{}' },
      ],
      active: 2,
    });
    await vi.waitFor(() => expect(state.workers).toHaveLength(3));
    const first = state.workers[1].worker,
      second = state.workers[2].worker;
    first.onerror?.({ preventDefault() {}, message: 'Wasm trap' } as ErrorEvent);
    expect(first.terminate).toHaveBeenCalledOnce();
    expect(second.terminate).not.toHaveBeenCalled();
    expect(coordinator.messages.at(-1)).toMatchObject({
      kind: 'events',
      events: [{ kind: 'retired', id: 'a', verdict: 'failed' }],
    });
    expect((await state.jobs.get(id)).status).toBe('running');
  });

  it('ignores late packets and errors after replacing a retired compute worker', async () => {
    const state = await running();
    const oldListener = state.compute.onmessage!;
    const oldError = state.compute.onerror!;
    state.compute.onerror?.({ preventDefault() {}, message: 'first failure' } as ErrorEvent);
    state.coordinator.update({ dispatch: [{ id: '1:0:0', branch: 1, source: '{}' }], active: 1 });
    await vi.waitFor(() => expect(state.workers).toHaveLength(3));
    const replacement = state.workers[2].worker;
    oldListener({ data: { ...state.compute.leaf, kind: 'failed', error: 'late' } } as MessageEvent);
    oldError({ preventDefault() {}, message: 'late error' } as ErrorEvent);
    expect(replacement.terminate).not.toHaveBeenCalled();
    expect((await state.jobs.get(state.id)).status).toBe('running');
    replacement.send({ kind: 'failed', workerAttempt: 'foreign', task: replacement.leaf.task, error: 'foreign' });
    expect(replacement.terminate).not.toHaveBeenCalled();
  });

  it.each(['error', 'messageerror', 'failed'] as const)(
    'preserves a committed witness after coordinator %s',
    async (failure) => {
      const state = await running();
      await accepted(state);
      if (failure === 'error')
        state.coordinator.onerror?.({ preventDefault() {}, message: 'coordinator trap' } as ErrorEvent);
      else if (failure === 'messageerror') state.coordinator.onmessageerror?.();
      else state.coordinator.send({ kind: 'failed', error: 'out of memory' });
      await vi.waitFor(async () => expect((await state.jobs.get(state.id)).status).toBe('failed'));
      const final = await state.jobs.get(state.id);
      expect(final.collection?.count).toBe(1);
      expect(final.result?.status).toBe('best_known');
      expect(final.proof?.minimumLinkCount).toBeNull();
      expect(final.error).toBeTruthy();
      expect(state.compute.terminate).toHaveBeenCalledOnce();
    },
  );

  it('keeps the bounded failed write for export and retry without claiming completion', async () => {
    const state = await running();
    vi.mocked(state.collections.append).mockRejectedValueOnce(new DOMException('Storage full', 'QuotaExceededError'));
    state.compute.event('witness');
    state.coordinator.update({
      append: [{ index: 0, solution: best() }],
      receipts: receipts(state.coordinator),
      active: 1,
    });
    await vi.waitFor(async () => expect((await state.jobs.get(state.id)).status).toBe('failed'));
    const final = await state.jobs.get(state.id);
    expect(final.collection).toMatchObject({ count: 1, pending: true });
    expect(state.collections.retain).toHaveBeenCalledOnce();
    expect(final.error).toContain('retained in this tab');
    expect(final.enumerationComplete).toBe(false);
  });

  it('a post-seal cancellation cannot rewrite a completion or a newer job', async () => {
    const state = await running();
    await accepted(state);
    state.compute.event('retired', 2);
    const receive = state.coordinator.onmessage!;
    state.coordinator.update({ receipts: receipts(state.coordinator), done: true });
    await vi.waitFor(async () => expect((await state.jobs.get(state.id)).status).toBe('completed'));
    const sealed = await state.jobs.cancel(state.id);
    expect(sealed.status).toBe('completed');
    const next = await state.jobs.create(historyEntry().request);
    receive({ data: { ...state.coordinator.start, kind: 'failed', error: 'late' } } as MessageEvent);
    expect((await state.jobs.get(next)).status).toBe('running');
    expect(await state.jobs.get(state.id)).toEqual(sealed);
  });

  it('a received terminal proposal remains cancellable while its collection write is pending', async () => {
    const state = await running();
    state.compute.event('retired');
    const pending = deferred<CollectionRef>();
    const ref = (await state.jobs.get(state.id)).collection!;
    releases.push(() => pending.resolve({ ...ref, count: 1 }));
    vi.mocked(state.collections.append).mockReturnValueOnce(pending.promise);
    state.coordinator.update({
      append: [{ index: 0, solution: best() }],
      receipts: receipts(state.coordinator),
      done: true,
    });
    const cancellation = state.jobs.cancel(state.id);
    pending.resolve({ ...ref, count: 1 });
    const final = await cancellation;
    expect(final.status).toBe('cancelled');
    expect(final.enumerationComplete).toBe(false);
    expect(final.collection?.count).toBe(1);
  });

  it.each(['protocol', 'checkpoint', 'sequence', 'count', 'thin terminal', 'recovery', 'budget'] as const)(
    'rejects malformed %s without promoting evidence',
    async (failure) => {
      const state = await running();
      await accepted(state);
      const capture = state.coordinator.onmessage!;
      state.coordinator.onmessage = null;
      const packet = state.coordinator.update();
      state.coordinator.onmessage = capture;
      const value = { ...packet, protocol: 2 };
      if (failure === 'protocol') value.protocol = 9;
      else if (failure === 'checkpoint') value.checkpoint++;
      else if (failure === 'sequence') value.update.packets[0].sequence!++;
      else if (failure === 'count') value.update.count++;
      else if (failure === 'thin terminal') {
        value.update.done = true;
        value.update.packets[0].status = 'completed';
        value.update.packets[0].resultsOmitted = true;
      } else if (failure === 'recovery') value.update.recovery.cancelled.enumerationComplete = true;
      else value.update.scheduling.active = 9;
      state.coordinator.send(value);
      await vi.waitFor(async () => expect((await state.jobs.get(state.id)).status).toBe('failed'));
      const final = await state.jobs.get(state.id);
      expect(final.collection?.count).toBe(1);
      expect(final.proof?.minimumLinkCount).toBeNull();
    },
  );

  it('bounds retained job metadata, closes listeners and isolates consumer exceptions', async () => {
    const state = setup({ retention: 2 });
    const ids = [];
    for (let index = 0; index < 5; index++) {
      const id = await state.jobs.create(historyEntry().request);
      ids.push(id);
      const watch = await state.jobs.watch(
        id,
        () => {
          throw new Error('consumer');
        },
        () => {
          throw new Error('consumer error');
        },
      );
      watch.close();
      watch.close();
      await state.jobs.cancel(id);
    }
    await expect(state.jobs.get(ids[0])).rejects.toThrow('Unknown');
    expect((await state.jobs.get(ids[4])).status).toBe('cancelled');
    await state.jobs.release(ids[4]);
    await state.jobs.release(ids[4]);
    await expect(state.jobs.get(ids[4])).rejects.toThrow('Unknown');
  });

  it('bounds asset startup but has no solve timeout after both workers are ready', async () => {
    vi.useFakeTimers();
    const state = setup({ startupTimeoutMs: 100 });
    const first = await state.jobs.create(historyEntry().request);
    await vi.advanceTimersByTimeAsync(101);
    expect((await state.jobs.get(first)).status).toBe('failed');
    const next = await state.jobs.create(historyEntry().request);
    const coordinator = state.workers[1].worker;
    coordinator.send({ kind: 'ready', rustBytes: 65536 });
    await vi.advanceTimersByTimeAsync(1_000_000);
    expect((await state.jobs.get(next)).status).toBe('running');
  });
});
