import {
  BROWSER_SOLVER_PROTOCOL,
  type Ack,
  type BrowserModule,
  type CoordinatorRun,
  type Events,
  type Receipt,
  type RunUpdate,
  type Start,
} from './protocol';
import { COORDINATOR_BATCH_SIZE, createCoordinatorInbox } from './inbox';

let start: Start | undefined;
let run: CoordinatorRun | undefined;
let memory: WebAssembly.Memory | undefined;
let checkpoint = 0;
let waiting: { checkpoint: number; resolve(): void } | undefined;
let busy = false;
let began = 0;
let queue = createCoordinatorInbox(1);
const elapsed = () => BigInt(Math.max(0, Math.floor(performance.now() - began)));

function post(packet: object): void {
  if (start)
    self.postMessage({ ...packet, protocol: BROWSER_SOLVER_PROTOCOL, jobId: start.jobId, attempt: start.attempt });
}

async function publish(update: RunUpdate, receipts: Receipt[]): Promise<void> {
  const received = new Promise<void>((resolve) => {
    waiting = { checkpoint: ++checkpoint, resolve };
  });
  post({ kind: 'update', checkpoint, update, receipts, rustBytes: memory?.buffer.byteLength ?? 0 });
  await received;
}

async function pump(): Promise<void> {
  if (busy || !run) return;
  busy = true;
  try {
    for (;;) {
      const batch = queue.take();
      const receipts: Receipt[] = [];
      for (const message of batch) {
        for (const event of message.events) run.accept(JSON.stringify(event), elapsed());
        receipts.push(...message.receipts);
      }
      const update: RunUpdate = JSON.parse(run.poll(elapsed()));
      if (update.append.length > COORDINATOR_BATCH_SIZE)
        throw new Error('Coordinator exceeded its collection batch budget.');
      if (update.done) {
        run.free();
        run = undefined;
      }
      await publish(update, receipts);
      if (!run) return;
      if (queue.length === 0 && update.scheduling.active > 0) return;
      // Impossible roots can retire without any compute worker. Yield between
      // polls so control messages are never starved by a run of empty groups.
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
  } catch (error) {
    post({ kind: 'failed', error: String(error).slice(0, 2048) });
    run?.free();
    run = undefined;
  } finally {
    busy = false;
  }
}

async function initialize(source: Start): Promise<void> {
  const module: BrowserModule = await import(/* @vite-ignore */ new URL('solver_browser.js', source.solverBase).href);
  ({ memory } = await module.default({ module_or_path: new URL('solver_browser_bg.wasm', source.solverBase) }));
  if (module.browser_solver_version() !== BROWSER_SOLVER_PROTOCOL)
    throw new Error('Incompatible browser solver assets. Reload the application.');
  post({ kind: 'ready', rustBytes: memory.buffer.byteLength });
  began = performance.now();
  run = new module.BrowserCoordinator(
    JSON.stringify(source.request),
    source.jobId,
    BigInt(source.startedAtMs),
    JSON.stringify(source.options),
  );
  await pump();
}

self.onmessage = ({ data }: MessageEvent<Start | Ack | Events>) => {
  if (data.protocol !== BROWSER_SOLVER_PROTOCOL) return;
  if (data.kind === 'start' && !start) {
    start = data;
    queue = createCoordinatorInbox(data.options.workerCount);
    void initialize(data).catch((error) => post({ kind: 'failed', error: String(error).slice(0, 2048) }));
    return;
  }
  if (!start || data.jobId !== start.jobId || data.attempt !== start.attempt) return;
  if (data.kind === 'ack' && data.checkpoint === waiting?.checkpoint) {
    const receipt = waiting;
    waiting = undefined;
    receipt.resolve();
  } else if (data.kind === 'events') {
    try {
      queue.push(data);
    } catch (error) {
      post({ kind: 'failed', error: String(error) });
      return;
    }
    void pump();
  }
};
