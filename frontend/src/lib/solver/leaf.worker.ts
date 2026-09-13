import {
  BROWSER_SOLVER_PROTOCOL,
  type BrowserModule,
  type ComputeEvent,
  type LeafAck,
  type LeafRun,
  type LeafStart,
  type Session,
  type SessionApi,
} from './protocol';

let source: LeafStart | undefined;
let sequence = 0;
let waiting: { sequence: number; resolve(): void } | undefined;
let backend: Promise<{ module: BrowserModule; api: SessionApi; memory: WebAssembly.Memory }> | undefined;
let busy = false;

function post(packet: object): void {
  if (source)
    self.postMessage({
      ...packet,
      protocol: BROWSER_SOLVER_PROTOCOL,
      jobId: source.jobId,
      attempt: source.attempt,
      workerAttempt: source.workerAttempt,
      task: source.task,
    });
}

async function load(task: LeafStart) {
  const module: BrowserModule = await import(/* @vite-ignore */ new URL('solver_browser.js', task.solverBase).href);
  const { memory } = await module.default({ module_or_path: new URL('solver_browser_bg.wasm', task.solverBase) });
  if (module.browser_solver_version() !== BROWSER_SOLVER_PROTOCOL)
    throw new Error('Incompatible browser compute assets.');
  const [{ default: createCvc5 }, { createSessionApi }] = await Promise.all([
    import(/* @vite-ignore */ new URL('cvc5.mjs', task.solverBase).href),
    import(/* @vite-ignore */ new URL('session.mjs', task.solverBase).href),
  ]);
  const api: SessionApi = createSessionApi(
    await createCvc5({ locateFile: (name: string) => new URL(name, task.solverBase).href }),
  );
  return { module, api, memory };
}

async function publish(events: ComputeEvent[], api: SessionApi, memory: WebAssembly.Memory): Promise<void> {
  const receipt = new Promise<void>((resolve) => {
    waiting = { sequence: ++sequence, resolve };
  });
  post({
    kind: events.length ? 'leaf-events' : 'heartbeat',
    events,
    sequence,
    rustBytes: memory.buffer.byteLength,
    heapBytes: api.heapBytes(),
  });
  await receipt;
}

async function execute(task: LeafStart): Promise<void> {
  let leaf: LeafRun | undefined;
  let session: Session | undefined;
  try {
    const { module, api, memory } = await (backend ??= load(task));
    post({ kind: 'ready', rustBytes: memory.buffer.byteLength, heapBytes: api.heapBytes() });
    let verdict = 'failed';
    let detail = '';
    try {
      leaf = new module.BrowserLeaf(task.source);
      session = api.createSession({ resourceLimit: task.resourceLimit });
      let action = JSON.parse(leaf.advance());
      let last = performance.now();
      for (;;) {
        if (action.witness) {
          // Do not enter the next blocking check until Rust accepts this exact
          // witness and the page has committed or retained its collection batch.
          await publish([{ kind: 'witness', id: task.task, witness: action.witness }], api, memory);
          last = performance.now();
        } else if (performance.now() - last >= 250) {
          await publish([], api, memory);
          last = performance.now();
        }
        if (action.completion) {
          verdict = action.completion;
          break;
        }
        action = JSON.parse(leaf.advance(session.execute(action.commands)));
      }
    } catch (error) {
      detail = String(error).slice(0, 2048);
    }
    // A cleanup exception must force page-owned worker termination, rather than
    // falsely report that a live backend has retired.
    session?.dispose();
    session = undefined;
    leaf?.free();
    leaf = undefined;
    if (api.activeSessions() !== 0) throw new Error('Compute worker retained a backend session.');
    await publish([{ kind: 'retired', id: task.task, verdict, detail }], api, memory);
  } catch (error) {
    post({ kind: 'failed', error: String(error).slice(0, 2048) });
  } finally {
    leaf?.free();
    busy = false;
  }
}

self.onmessage = ({ data }: MessageEvent<LeafStart | LeafAck>) => {
  if (data.protocol !== BROWSER_SOLVER_PROTOCOL) return;
  if (data.kind === 'leaf' && !busy) {
    source = data;
    sequence = 0;
    busy = true;
    void execute(data).catch((error) => post({ kind: 'failed', error: String(error).slice(0, 2048) }));
  } else if (
    data.kind === 'leaf-ack' &&
    source &&
    data.jobId === source.jobId &&
    data.attempt === source.attempt &&
    data.workerAttempt === source.workerAttempt &&
    data.task === source.task &&
    data.sequence === waiting?.sequence
  ) {
    const receipt = waiting;
    waiting = undefined;
    receipt.resolve();
  }
};
