import { test, expect } from '@playwright/test';
import { injectSessionFailure, observeWorkers } from './failures.mjs';
import { runJob, storedEntries, verifyJobs } from './helpers.mjs';

const direct = { inputs: [], outputs: [{ id: 'o', name: '', rate: '1/3' }], beltRate: '1', solveMode: 'all_min_nl' };

test.beforeEach(async ({ page }) => {
  // These tests isolate one blocked worker. Parallel cancellation is covered separately.
  await page.addInitScript(() => localStorage.setItem('sfs.browser-workers.v2', '1'));
});

test('the actual cancel button retires a worker blocked inside cvc5 without losing its saved witness', async ({
  page,
}) => {
  await observeWorkers(page);
  const removeFault = await injectSessionFailure(page, 'busy');
  await page.goto('./');
  await page.getByRole('button', { name: /^Find/ }).click();
  await expect.poll(() => page.evaluate(() => window.computeTest.checking)).toBe(true);
  await expect.poll(() => page.evaluate(() => window.computeTest.ticks)).toBeGreaterThan(3);
  expect(await page.evaluate(() => window.computeTest.returned)).toBe(false);
  await page.getByRole('button', { name: 'Cancel running job', exact: true }).click();
  await expect(page.locator('[data-history-band="history"]')).toContainText('Cancelled');
  expect(await page.evaluate(() => window.computeTest.terminated)).toBe(1);
  await expect.poll(async () => (await storedEntries(page)).entries[0]?.status).toBe('cancelled');
  const saved = await storedEntries(page);
  expect(saved.entries[0].result.status).toBe('best_known');
  expect(saved.entries[0].enumerationComplete).toBe(false);
  expect(saved.entries[0].proof.minimumLinkCount).toBeNull();
  expect(saved.solutions).toHaveLength(1);
  verifyJobs([
    {
      name: 'blocked cancellation',
      request: saved.entries[0].request,
      snapshot: { ...saved.entries[0], results: [] },
      solutions: saved.solutions.map((row) => row.value),
    },
  ]);
  await removeFault();
  await page.getByRole('button', { name: /^Find/ }).click();
  await expect(page.locator('[data-history-band="history"]').filter({ hasText: 'Completed' })).toHaveCount(1, {
    timeout: 60_000,
  });
  expect(await page.evaluate(() => window.computeTest.starts)).toBe(2);
  expect(await page.evaluate(() => window.computeTest.terminated)).toBe(2);
});

test('reloading a running browser job restores the saved incomplete witness without restarting search', async ({
  page,
}) => {
  await observeWorkers(page);
  await injectSessionFailure(page, 'busy');
  await page.goto('./');
  await page.getByRole('button', { name: /^Find/ }).click();
  await expect.poll(() => page.evaluate(() => window.computeTest.checking)).toBe(true);
  await expect.poll(async () => (await storedEntries(page)).solutions.length).toBe(1);
  const saved = await storedEntries(page);
  // Checkpoint serialization deliberately records interrupted state even while
  // the live page still owns the running worker.
  await expect(page.locator('[data-history-band="running"]')).toHaveCount(1);
  expect(saved.entries[0].status).toBe('incomplete');
  const fetched = [];
  page.on('request', (request) => fetched.push(request.url()));
  await page.reload();
  await expect(page.locator('[data-history-band="history"]')).toContainText('Incomplete');
  await expect(page.locator('[data-history-band="running"]')).toHaveCount(0);
  const restored = await page.evaluate(async () => {
    const { createBrowserHistoryStore } =
      await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
    const store = createBrowserHistoryStore();
    try {
      return (await store.load()).entries[0];
    } finally {
      store.close();
    }
  });
  expect(restored.jobId).toBeNull();
  expect(restored.error).toContain('Interrupted before completion');
  expect(restored.results).toHaveLength(0);
  expect(restored.collection.count).toBe(1);
  expect(restored.result.status).toBe('best_known');
  expect(restored.enumerationComplete).toBe(false);
  expect(await page.evaluate(() => window.computeTest.starts)).toBe(0);
  expect(fetched.some((url) => /solver_browser|cvc5|session\.mjs/.test(url))).toBe(false);
});

for (const fault of ['unknown', 'trap', 'dispose']) {
  test(`a backend ${fault} after an accepted witness preserves it and never claims exhaustive completion`, async ({
    page,
  }) => {
    const removeFault = await injectSessionFailure(page, fault);
    await page.goto('./');
    const stopped = await runJob(page, direct);
    expect(stopped.snapshot.status).toBe('failed');
    expect(stopped.snapshot.result.status).toBe('best_known');
    expect(stopped.solutions).toHaveLength(1);
    expect(stopped.snapshot.results).toHaveLength(0);
    expect(stopped.snapshot.enumerationComplete).toBe(false);
    expect(stopped.snapshot.proof.minimumLinkCount).toBeNull();
    expect(stopped.snapshot.error).toContain(fault === 'dispose' ? 'disposal' : fault);
    verifyJobs([{ name: fault, request: direct, ...stopped }]);
    await removeFault();
    expect((await runJob(page, direct)).snapshot.status).toBe('completed');
  });
}

test('HistoryQueue recovers a missed collection update from the next immutable prefix', async ({ page }) => {
  await page.goto('./');
  await page.evaluate(async () => {
    const { getPlatform } = await import('/satisfactory-flow-synthetizer/src/lib/platform/index.ts');
    const jobs = getPlatform().jobs;
    const watch = jobs.watch;
    const get = jobs.get;
    window.missedAppend = false;
    window.fullFetches = 0;
    jobs.get = async (...args) => {
      window.fullFetches++;
      return get(...args);
    };
    jobs.watch = (id, onSnapshot, onError) =>
      watch(
        id,
        (snapshot) => {
          if (!window.missedAppend && snapshot.status === 'running' && snapshot.collection?.count) {
            window.missedAppend = true;
            return;
          }
          onSnapshot(snapshot);
        },
        onError,
      );
  });
  // This request continues through multiple profiles after its first layout.
  await page.getByLabel('Output 1 items per minute').fill('3');
  await page.getByLabel('Output 2 items per minute').fill('2');
  await page.getByRole('button', { name: 'Add output', exact: true }).click();
  await page.getByLabel('Output 3 items per minute').fill('1');
  await page.getByRole('radio', { name: 'All min N', exact: true }).click();
  await page.getByRole('button', { name: /^Find/ }).click();
  await expect(page.locator('[data-history-band="history"]')).toContainText('Completed', { timeout: 60_000 });
  expect(await page.evaluate(() => window.missedAppend)).toBe(true);
  // Every subsequent scalar snapshot carries the complete immutable prefix
  // reference, so no full-graph get is needed to repair a missed notification.
  expect(await page.evaluate(() => window.fullFetches)).toBe(0);
  await expect.poll(async () => (await storedEntries(page)).entries[0]?.status).toBe('completed');
  const saved = await storedEntries(page);
  expect(saved.solutions.length).toBeGreaterThan(1);
  expect(saved.entries[0].enumerationComplete).toBe(true);
});
