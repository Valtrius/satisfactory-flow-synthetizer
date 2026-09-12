import {
  BROWSER_SOLVER_PROTOCOL,
  type Ack,
  type BrowserModule,
  type BrowserRun,
  type Dispatch,
  type RunUpdate,
  type Session,
  type SessionApi,
  type Start,
  type WorkerMessage,
} from './protocol';

let start: Start | undefined;
let checkpoint = 0;
let waiting: { checkpoint: number; resolve(): void } | undefined;

function post(packet: WorkerMessage): void {
  if (!start) throw new Error('Browser worker has no job');
  self.postMessage({ ...packet, jobId: start.jobId, attempt: start.attempt, protocol: BROWSER_SOLVER_PROTOCOL });
}

async function publish(update: RunUpdate): Promise<void> {
  if (update.packets.length === 0) return;
  const receipt = new Promise<void>((resolve) => {
    waiting = { checkpoint: ++checkpoint, resolve };
  });
  post({ kind: 'update', checkpoint, packets: update.packets, recovery: update.recovery, done: update.done });
  // In particular, do not enter another synchronous check-sat until the page
  // owns the accepted witness and its Rust-projected interruption packet.
  await receipt;
}

async function loadBackend(source: Start): Promise<SessionApi> {
  const [{ default: createCvc5 }, { createSessionApi }] = await Promise.all([
    import(/* @vite-ignore */ new URL('cvc5.mjs', source.solverBase).href),
    import(/* @vite-ignore */ new URL('session.mjs', source.solverBase).href),
  ]);
  const api: SessionApi = createSessionApi(
    await createCvc5({
      locateFile: (name: string) => new URL(name, source.solverBase).href,
    }),
  );
  post({ kind: 'ready' });
  return api;
}

async function driveLeaf(
  run: BrowserRun,
  api: SessionApi,
  task: Dispatch,
  elapsed: () => bigint,
  resourceLimit?: number,
): Promise<void> {
  const id = JSON.stringify(task.id);
  let session: Session | undefined;
  let verdict = 'failed';
  let detail = '';
  try {
    if (task.impossible) {
      verdict = 'exhausted';
    } else {
      session = api.createSession({ resourceLimit });
      let action: { commands: string; witnessed: boolean; completion: string | null } = JSON.parse(run.advance(id));
      let lastProgress = performance.now();
      for (;;) {
        if (action.witnessed || performance.now() - lastProgress >= 100) {
          const update: RunUpdate = JSON.parse(run.poll(elapsed()));
          if (update.done || update.dispatch.length) throw new Error('Planner dispatched while a leaf was active');
          await publish(update);
          lastProgress = performance.now();
        }
        if (action.completion) {
          verdict = action.completion;
          break;
        }
        const reply = session.execute(action.commands);
        action = JSON.parse(run.advance(id, reply));
      }
    }
  } catch (error) {
    detail = String(error).slice(0, 2048);
  } finally {
    // If disposal throws, the page must retire the entire worker. Do not claim
    // that this leaf's backend has retired while its cleanup remains uncertain.
    session?.dispose();
    try {
      run.retire(id, verdict, detail);
    } catch (error) {
      // Planner failure retirement also returns the error it records. Continue
      // to poll its incomplete outcome; other rejected transitions are fatal.
      if (verdict !== 'failed' || String(error) !== detail) throw error;
    }
  }
}

async function execute(source: Start): Promise<void> {
  let run: BrowserRun | undefined;
  try {
    const module: BrowserModule = await import(/* @vite-ignore */ new URL('solver_browser.js', source.solverBase).href);
    await module.default({ module_or_path: new URL('solver_browser_bg.wasm', source.solverBase) });
    if (module.browser_solver_version() !== BROWSER_SOLVER_PROTOCOL)
      throw new Error('Incompatible browser solver assets. Reload the application.');
    await publish(JSON.parse(module.initial_job_json(source.jobId, BigInt(source.startedAtMs))));
    const began = performance.now();
    const elapsed = () => BigInt(Math.max(0, Math.floor(performance.now() - began)));
    run = new module.BrowserRun(
      JSON.stringify(source.request),
      source.jobId,
      BigInt(source.startedAtMs),
      source.maxNodes,
    );
    let api: SessionApi | undefined;
    for (;;) {
      const update: RunUpdate = JSON.parse(run.poll(elapsed()));
      if (update.done) {
        if (api && api.activeSessions() !== 0) throw new Error('Completed browser solve retained a cvc5 session');
        run.free();
        run = undefined;
        await publish(update);
        return;
      }
      await publish(update);
      for (const task of update.dispatch) {
        if (task.impossible) {
          run.retire(JSON.stringify(task.id), 'exhausted', '');
          continue;
        }
        // Trivial global contradictions need no cvc5 download.
        api ??= await loadBackend(source);
        await driveLeaf(run, api, task, elapsed, source.resourceLimit);
      }
    }
  } catch (error) {
    post({ kind: 'failed', error: String(error).slice(0, 2048) });
  } finally {
    run?.free();
  }
}

self.onmessage = ({ data }: MessageEvent<Start | Ack>) => {
  if (data.protocol !== BROWSER_SOLVER_PROTOCOL) return;
  if (data.kind === 'start' && !start) {
    start = data;
    void execute(data).catch((error) => post({ kind: 'failed', error: String(error).slice(0, 2048) }));
  } else if (
    data.kind === 'ack' &&
    start &&
    data.jobId === start.jobId &&
    data.attempt === start.attempt &&
    data.checkpoint === waiting?.checkpoint
  ) {
    const receipt = waiting;
    waiting = undefined;
    receipt.resolve();
  }
};
