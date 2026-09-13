import init, { qualify_identity, qualify_primitives, PortableRun } from './solver_portable_tests.js';

function solve(api, request, interruption) {
  const run = new PortableRun(JSON.stringify(request));
  const events = [];
  const failures = [];
  try {
    for (;;) {
      const update = JSON.parse(run.poll());
      events.push(...update.events);
      if (update.done) {
        if (api.activeSessions() !== 0) throw new Error('A sealed result retained a backend session');
        if (run.cancel()) throw new Error('A sealed result accepted cancellation');
        return { ...update, events, failures, sessions: api.activeSessions() };
      }
      for (const task of update.dispatch) {
        const id = JSON.stringify(task.id);
        let session;
        let verdict = 'failed';
        let detail = '';
        try {
          if (task.impossible) {
            verdict = 'exhausted';
          } else {
            session = api.createSession({ resourceLimit: interruption === 'resource-limit' ? 1 : 0 });
            let action = JSON.parse(run.advance(id, undefined));
            for (;;) {
              // Retain and deliver accepted witnesses before the next synchronous
              // SMT call. That call can block until the worker is terminated.
              if (action.witnessed && interruption === 'cancel') {
                if (!run.cancel()) throw new Error('Cancellation was sealed too early');
                verdict = 'cancelled';
                break;
              }
              if (action.completion) {
                verdict = action.completion;
                break;
              }
              const reply =
                action.witnessed && interruption === 'unknown-after-witness'
                  ? 'unknown'
                  : session.execute(action.commands);
              action = JSON.parse(run.advance(id, reply));
            }
          }
        } catch (error) {
          detail = String(error);
          failures.push(detail);
        } finally {
          session?.dispose();
          try {
            run.retire(id, verdict, detail);
          } catch (error) {
            if (verdict !== 'failed') throw error;
          }
        }
      }
    }
  } finally {
    run.free();
  }
}

self.onmessage = async ({ data }) => {
  try {
    await init();
    if (data.operation === 'identity') {
      const response = await fetch('./identity.json');
      if (!response.ok) throw new Error(`Identity fixture HTTP ${response.status}`);
      const identity = JSON.parse(qualify_identity(await response.text()));
      const primitives = JSON.parse(qualify_primitives());
      self.postMessage({ kind: 'verified', identity, primitives });
    } else if (data.operation === 'search') {
      const [{ default: createCvc5 }, { createSessionApi }] = await Promise.all([
        import('./cvc5.mjs'),
        import('./session.mjs'),
      ]);
      const api = createSessionApi(await createCvc5());
      const response = await fetch('./solves.json');
      if (!response.ok) throw new Error(`Search fixture HTTP ${response.status}`);
      const cases = await response.json();
      const selected = data.interruption ? cases.filter((item) => item.name === 'mixed inputs AllMinNL') : cases;
      const results = selected.map((item) => ({
        name: item.name,
        expected: item.expected,
        actual: solve(api, item.request, data.interruption),
      }));
      self.postMessage({ kind: 'verified', results });
    } else {
      throw new Error('Unknown qualification operation');
    }
  } catch (error) {
    self.postMessage({ kind: 'failed', error: String(error) });
  }
};
