import { test, expect } from '@playwright/test';

async function inWorker(page, data) {
  await page.goto('./');
  return page.evaluate(
    (data) =>
      new Promise((resolve, reject) => {
        const worker = new Worker(new URL('worker.mjs', location.href), { type: 'module' });
        const timer = setTimeout(() => {
          worker.terminate();
          reject(new Error('Qualification worker timed out'));
        }, 150_000);
        worker.onmessage = ({ data }) => {
          clearTimeout(timer);
          worker.terminate();
          resolve(data);
        };
        worker.onerror = (error) => {
          clearTimeout(timer);
          worker.terminate();
          reject(new Error(error.message));
        };
        worker.postMessage(data);
      }),
    data,
  );
}

test('32-bit Canonaut preserves pre-port native golden identities and primitive word boundaries', async ({ page }) => {
  const result = await inWorker(page, { operation: 'identity' });
  expect(result.kind, result.error).toBe('verified');
  expect(result.identity.cases).toHaveLength(42);
  expect(result.identity.permutationsPerCase).toBe(3);
  expect(result.primitives.wordBits).toBe(32);
  expect(result.primitives.sizes).toEqual([31, 32, 33, 63, 64, 65, 127, 128, 129]);
  expect(await page.evaluate(() => crossOriginIsolated)).toBe(false);
});

test('the portable planner and leaf driver match native/reference scopes through real cvc5 Wasm', async ({ page }) => {
  const result = await inWorker(page, { operation: 'search' });
  expect(result.kind, result.error).toBe('verified');
  expect(result.results).toHaveLength(36);
  for (const item of result.results) {
    expect(item.actual.done, item.name).toEqual(item.expected);
    expect(item.actual.failures, item.name).toEqual([]);
    expect(item.actual.sessions, item.name).toBe(0);
  }
});

for (const interruption of ['cancel', 'unknown-after-witness', 'resource-limit']) {
  test(`portable ${interruption} stays incomplete and retires its backend`, async ({ page }) => {
    const result = await inWorker(page, { operation: 'search', interruption });
    expect(result.kind, result.error).toBe('verified');
    expect(result.results).toHaveLength(1);
    const { actual } = result.results[0];
    expect(actual.done.kind).toBe('incomplete');
    expect(actual.done.enumeration.complete).toBe(false);
    expect(actual.done.proof.minimumLinkCount).toBeNull();
    expect(actual.sessions).toBe(0);
    if (interruption !== 'resource-limit') {
      expect(actual.done.solutions.length).toBeGreaterThan(0);
      expect(actual.done.objective).not.toBeNull();
    }
    if (interruption !== 'cancel') expect(actual.failures.join(' ')).toContain('unknown');
  });
}
