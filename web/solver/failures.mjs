import { readFile } from 'node:fs/promises';

/** Replace only a test response, never production sources or public APIs. */
export async function injectSessionFailure(page, kind) {
  const original = await readFile(new URL('../cvc5/session.mjs', import.meta.url), 'utf8');
  const wrapper = `
export function createSessionApi(module) {
  const api = originalSessionApi(module);
  return { ...api, createSession(options) {
    const session = api.createSession(options);
    let modelRead = false;
    return {
      execute(commands) {
        if (modelRead && commands.includes('(check-sat)')) {
          if (${JSON.stringify(kind)} === 'unknown') return 'unknown (RESOURCEOUT)';
          if (${JSON.stringify(kind)} === 'trap') throw new WebAssembly.RuntimeError('Injected backend trap after witness');
          if (${JSON.stringify(kind)} === 'dispose') throw new Error('Injected query failure before failed disposal');
          const hard = api.createSession();
          const count = 40;
          const holes = count - 1;
          const statements = ['(set-logic QF_LRA)'];
          for (let p = 0; p < count; p++) {
            for (let h = 0; h < holes; h++) statements.push('(declare-fun p' + p + 'h' + h + ' () Bool)');
            statements.push('(assert (or ' + Array.from({ length: holes }, (_, h) => 'p' + p + 'h' + h).join(' ') + '))');
          }
          for (let h = 0; h < holes; h++) for (let a = 0; a < count; a++) for (let b = a + 1; b < count; b++) statements.push('(assert (not (and p' + a + 'h' + h + ' p' + b + 'h' + h + ')))');
          try {
            hard.execute(statements.join('\\n'));
            self.postMessage({ kind: 'test-checking' });
            const result = hard.execute('(check-sat)');
            self.postMessage({ kind: 'test-returned' });
            return result;
          } finally { hard.dispose(); }
        }
        const response = session.execute(commands);
        if (commands.includes('(get-value')) modelRead = true;
        return response;
      },
      dispose() {
        session.dispose();
        if (${JSON.stringify(kind)} === 'dispose') throw new Error('Injected backend disposal failure');
      }
    };
  }};
}
`;
  const route = '**/assets/solver-*/session.mjs';
  await page.route(route, (request) =>
    request.fulfill({
      contentType: 'text/javascript',
      body: original.replace('export function createSessionApi', 'function originalSessionApi') + wrapper,
    }),
  );
  return () => page.unroute(route);
}

export async function observeWorkers(page) {
  await page.addInitScript(() => {
    const OriginalWorker = window.Worker;
    window.computeTest = { starts: 0, terminated: 0, checking: false, returned: false, ticks: 0 };
    setInterval(() => {
      if (window.computeTest.checking) window.computeTest.ticks++;
    }, 20);
    window.Worker = class extends OriginalWorker {
      compute = false;
      constructor(...args) {
        super(...args);
        this.addEventListener('message', ({ data }) => {
          if (data.kind === 'test-checking') window.computeTest.checking = true;
          if (data.kind === 'test-returned') window.computeTest.returned = true;
        });
      }
      postMessage(...args) {
        if (args[0]?.kind === 'start' && args[0]?.solverBase) {
          this.compute = true;
          window.computeTest.starts++;
        }
        return super.postMessage(...args);
      }
      terminate() {
        if (this.compute) {
          this.compute = false;
          window.computeTest.terminated++;
        }
        return super.terminate();
      }
    };
  });
}
