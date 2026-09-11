import { test, expect } from '@playwright/test';

async function runWorker(page, operation) {
  return page.evaluate(
    (operation) =>
      new Promise((resolve, reject) => {
        const worker = new Worker('./worker.mjs', { type: 'module' });
        const finish = (action, value) => {
          clearTimeout(timer);
          worker.terminate();
          action(value);
        };
        const timer = setTimeout(() => finish(reject, new Error('worker test timed out')), 90_000);
        worker.onerror = (event) => finish(reject, new Error(event.message));
        worker.onmessage = ({ data }) => {
          if (data.kind === 'error') finish(reject, new Error(data.error));
          else if (data.kind === 'result') finish(resolve, data.value);
        };
        worker.postMessage({ id: 1, operation });
      }),
    operation,
  );
}

test('exact Rust verification agrees with native fixtures without loading cvc5', async ({ page }) => {
  const loaded = [];
  page.on('request', (request) => loaded.push(request.url()));
  await page.goto('./');
  expect(await page.evaluate(() => crossOriginIsolated)).toBe(false);
  const fixtures = await runWorker(page, 'verify');
  expect(fixtures.length).toBeGreaterThanOrEqual(28);
  for (const fixture of fixtures) {
    expect(fixture.actual, fixture.name).toEqual(fixture.native);
    if (fixture.actual.kind === 'verified') {
      expect(fixture.actual.solution.status).toBe('best_known');
      expect(fixture.actual.solution.proof).toBeNull();
    }
  }
  expect(loaded.some((url) => /\/cvc5\.(wasm|mjs)/.test(url))).toBe(false);
});

test('cvc5 sessions preserve assertions, symbols and errors independently', async ({ page }, testInfo) => {
  await page.goto('./');
  const result = await runWorker(page, 'sessions');
  expect(result.unknown).toBe('unknown');
  expect(result.recreated).toBe(16);
  expect(result.enumeratedModels).toBe(2);
  await testInfo.attach('cvc5-session-checks', {
    body: JSON.stringify(result, null, 2),
    contentType: 'application/json',
  });
});

test('Rust and cvc5 modules coexist in one worker with independent memories', async ({ page }) => {
  await page.goto('./');
  const result = await runWorker(page, 'coexist');
  expect(result.verified).toBe('verified');
  expect(result.answer).toMatch(/^sat\s/);
});

test('a blocked cvc5 worker can be terminated and replaced without freezing the page', async ({ page }) => {
  await page.goto('./');
  const result = await page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const worker = new Worker('./worker.mjs', { type: 'module' });
        let checking = false;
        let returned = false;
        let ticks = 0;
        const heartbeat = setInterval(() => {
          if (checking) ticks++;
        }, 20);
        const cleanup = () => {
          clearTimeout(timeout);
          clearInterval(heartbeat);
          worker.terminate();
        };
        const timeout = setTimeout(() => {
          cleanup();
          reject(new Error('cvc5 did not start the termination test'));
        }, 90_000);
        worker.onerror = (event) => {
          cleanup();
          reject(new Error(event.message));
        };
        worker.onmessage = ({ data }) => {
          if (data.kind === 'error') {
            cleanup();
            reject(new Error(data.error));
          }
          if (data.kind === 'result') returned = true;
          if (data.kind === 'checking') {
            checking = true;
            setTimeout(() => {
              cleanup();
              resolve({ ticks, returned });
            }, 200);
          }
        };
        worker.postMessage({ id: 1, operation: 'busy' });
      }),
  );
  expect(result.returned, 'the test must terminate a still-running SMT query').toBe(false);
  expect(result.ticks, 'page events must run while cvc5 is blocked').toBeGreaterThan(0);
  expect(await runWorker(page, 'ping')).toBe('sat');
});
