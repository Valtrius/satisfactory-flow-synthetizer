import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { publicRequest, verifyJobs, runJob } from './helpers.mjs';

const corpus = JSON.parse(await readFile(new URL('../../target/web-portable/solves.json', import.meta.url), 'utf8'));

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    window.tauriCalls = 0;
    window.__TAURI_INTERNALS__ = {
      invoke() {
        window.tauriCalls++;
        throw new Error('Browser called native IPC');
      },
    };
  });
  await page.goto('./');
});
test.afterEach(async ({ page }) => {
  expect(await page.evaluate(() => window.tauriCalls)).toBe(0);
});

test('production browser jobs match native/reference outcomes and full exact collections in all three scopes', async ({
  page,
}, testInfo) => {
  test.setTimeout(180_000);
  const results = [];
  // The public preparer rejects out-of-capacity terminals before solver admission,
  // unlike the lower-level Problem corpus which classifies that contradiction.
  for (const fixture of corpus.filter((entry) => !entry.name.startsWith('external capacity'))) {
    const request = publicRequest(fixture);
    const result = await runJob(page, request, { maxNodes: fixture.request.options.maxNodes });
    results.push({
      name: fixture.name,
      request,
      expected: fixture.expected,
      problem: fixture.request.problem,
      ...result,
    });
  }
  const checked = verifyJobs(results);
  await testInfo.attach('native-reference-browser-comparisons', {
    body: JSON.stringify(checked),
    contentType: 'application/json',
  });
  expect(checked.length).toBe(corpus.length - 3);
  expect(results.some((value) => value.live.some((packet) => packet.resultsOmitted))).toBe(true);
  expect(results.some((value) => value.live.some((packet) => packet.resultAppended))).toBe(true);
  expect(
    results.some(
      (value) =>
        value.request.solveMode === 'all_min_n' &&
        value.snapshot.results.some((solution) => solution.stats.linkCount > value.snapshot.proof.minimumLinkCount),
    ),
  ).toBe(true);
  for (const result of results) {
    const sequences = result.live.map((packet) => packet.sequence);
    expect(sequences.every((sequence, index) => index === 0 || sequence >= sequences[index - 1])).toBe(true);
    expect(result.snapshot.resultsOmitted).toBe(false);
  }
  expect(await page.evaluate(() => crossOriginIsolated)).toBe(false);
});

test('cancellation after an acknowledged exact witness keeps it prooflessly and allows a fresh worker', async ({
  page,
}) => {
  for (const solveMode of ['one_min_nl', 'all_min_nl', 'all_min_n']) {
    const request = { inputs: [], outputs: [{ id: 'o', name: '', rate: '1/3' }], beltRate: '1', solveMode };
    const stopped = await runJob(page, request, {}, true);
    expect(stopped.snapshot.status).toBe('cancelled');
    expect(stopped.snapshot.result.status).toBe('best_known');
    expect(stopped.snapshot.result.proof).toBeNull();
    expect(stopped.snapshot.proof.minimumLinkCount).toBeNull();
    expect(stopped.snapshot.enumerationComplete).toBe(false);
    verifyJobs([{ name: `cancel ${solveMode}`, request, ...stopped }]);
    const restarted = await runJob(page, request);
    expect(restarted.snapshot.status).toBe('completed');
  }
});

test('resource exhaustion is not UNSAT and invalid application requests fail without cvc5', async ({ page }) => {
  const fixture = corpus.find(
    (entry) => entry.name.startsWith('multiple optimal layouts') && entry.request.options.mode === 'all_min_nl',
  );
  const limited = await runJob(page, publicRequest(fixture), { resourceLimit: 1 });
  expect(limited.snapshot.status).toBe('failed');
  expect(limited.snapshot.error).toContain('unknown');
  expect(limited.snapshot.enumerationComplete).toBe(false);
  expect(limited.snapshot.unsat).toBeUndefined();
  const fetched = [];
  page.on('request', (request) => fetched.push(request.url()));
  const invalid = await runJob(page, { ...publicRequest(fixture), beltRate: '0' });
  expect(invalid.snapshot.status).toBe('failed');
  expect(fetched.some((url) => /cvc5|session\.mjs/.test(url))).toBe(false);
  const normal = await runJob(page, {
    inputs: [],
    outputs: [{ id: 'o', name: '', rate: '1' }],
    beltRate: '1',
    solveMode: 'one_min_nl',
  });
  expect(normal.snapshot.status).toBe('completed');
});

for (const mode of ['One min N/L', 'All min N/L', 'All min N']) {
  test(`the actual Find UI solves ${mode}, saves and restores its terminal history`, async ({ page }) => {
    await page.getByLabel('Output 1 items per minute').fill('1/6');
    await page.getByLabel('Output 2 items per minute').fill('1/6');
    await page.getByRole('radio', { name: mode, exact: true }).click();
    await page.getByRole('button', { name: `Find ${mode}`, exact: true }).click();
    const card = page.locator('[data-history-band="history"]').first();
    await expect(card).toBeVisible({ timeout: 60_000 });
    await expect(card).toContainText('Completed');
    await expect(page.locator('.svelte-flow__node')).toHaveCount(4);
    await expect
      .poll(async () =>
        page.evaluate(async () => {
          const { createBrowserHistoryStore } =
            await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
          const store = createBrowserHistoryStore();
          try {
            return (await store.load()).entries[0]?.status;
          } finally {
            store.close();
          }
        }),
      )
      .toBe('completed');
    await page.reload();
    await expect(card).toBeVisible();
    await expect(page.locator('[data-history-band="running"]')).toHaveCount(0);
    const saved = await page.evaluate(async () => {
      const { createBrowserHistoryStore } =
        await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
      const store = createBrowserHistoryStore();
      try {
        return (await store.load()).entries[0];
      } finally {
        store.close();
      }
    });
    expect(saved.result.totalOutput.exact).toBe('1/3');
    expect(saved.result.stats.nodeCount).toBe(1);
    expect(saved.result.stats.linkCount).toBe(0);
    expect(saved.result.status).toBe('proven_optimal');
    expect(saved.proof).toEqual({ minimumNodeCount: 1, minimumLinkCount: 0 });
    expect(saved.enumerationComplete).toBe(mode !== 'One min N/L');
    verifyJobs([{ name: mode, request: saved.request, snapshot: saved }]);
  });
}

test('production assets solve under the project subpath and local-only CSP', async ({ page }) => {
  const requests = [];
  const errors = [];
  page.on('request', (request) => requests.push(request.url()));
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('http://127.0.0.1:4182/satisfactory-flow-synthetizer/');
  await expect(page.getByRole('button', { name: /^Find/ })).toBeEnabled();
  expect(requests.some((url) => /solver_browser|cvc5|session\.mjs/.test(url))).toBe(false);
  await page.getByRole('button', { name: /^Find/ }).click();
  await expect(page.locator('[data-history-band="history"]')).toContainText('Completed', { timeout: 60_000 });
  expect(requests.some((url) => /assets\/solver-[a-f0-9]+\/cvc5\.wasm$/.test(url))).toBe(true);
  expect(requests.every((url) => url.startsWith('http://127.0.0.1:4182/satisfactory-flow-synthetizer/'))).toBe(true);
  expect(errors).toEqual([]);
});
