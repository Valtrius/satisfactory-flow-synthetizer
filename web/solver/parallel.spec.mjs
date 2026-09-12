import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { publicRequest, runJob, verifyJobs } from './helpers.mjs';
import { injectSessionFailure, observeWorkers } from './failures.mjs';

const corpus = JSON.parse(await readFile(new URL('../../target/web-portable/solves.json', import.meta.url), 'utf8'));
const fixture = corpus.find(
  (entry) => entry.name.startsWith('multiple optimal layouts') && entry.request.options.mode === 'all_min_nl',
);
const metric = (snapshot, name) =>
  Number(snapshot.progress.custom.find((item) => item.name === name)?.value.value ?? 0);

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() =>
    Object.defineProperty(navigator, 'hardwareConcurrency', { configurable: true, value: 32 }),
  );
});

test('a failed independent branch does not replace the healthy complete proof', async ({ page }) => {
  await observeWorkers(page);
  await page.addInitScript(() => {
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (message, ...rest) {
      // Test-only exhaustion of every leaf belonging to the sparse branch.
      if (message?.kind === 'leaf' && message.task.startsWith('0:')) message = { ...message, resourceLimit: 1 };
      return post.call(this, message, ...rest);
    };
  });
  await page.goto('./');
  const request = publicRequest(fixture);
  const result = await runJob(page, request, { workerCount: 4 });
  verifyJobs([{ name: 'independent survivor', request, expected: fixture.expected, ...result }]);
  expect(metric(result.snapshot, 'solver.portfolio_proof_owner')).toBe(1);
  const workers = await page.evaluate(() => window.computeTest);
  expect(workers.peak).toBeGreaterThan(1);
  expect(workers.peak).toBeLessThanOrEqual(4);
  expect(workers.live).toBe(0);
  expect(workers.coordinators).toBe(0);
  expect(workers.terminated).toBe(workers.starts);
});

test('static and adaptive Boolean scheduling return the exact reference collection', async ({ page }, testInfo) => {
  await page.goto('./');
  const observed = [];
  for (const workerCount of [2, 8]) {
    const request = publicRequest(fixture);
    const result = await runJob(page, request, { workerCount, strategy: 'boolean' });
    verifyJobs([{ name: `Boolean ${workerCount}`, request, expected: fixture.expected, ...result }]);
    expect(metric(result.snapshot, 'solver.browser_peak_pending')).toBeLessThanOrEqual(workerCount);
    observed.push({
      workerCount,
      secondRoots: metric(result.snapshot, 'solver.browser_second_roots'),
      adaptiveChildren: metric(result.snapshot, 'solver.browser_adaptive_children'),
      peakHeapBytes: metric(result.snapshot, 'solver.browser_peak_heap_bytes'),
    });
  }
  expect(observed.some((value) => value.secondRoots > 0)).toBe(true);
  await testInfo.attach('parallel-scheduling', { body: JSON.stringify(observed), contentType: 'application/json' });
});

test('collection guards stop with exact witnesses and no enumeration exhaustion', async ({ page }) => {
  await page.goto('./');
  const request = publicRequest(fixture);
  for (const limit of [{ maxLayouts: 1 }, { maxIdentityBytes: 1 }]) {
    const result = await runJob(page, request, { workerCount: 4, ...limit });
    expect(result.snapshot.status).toBe('incomplete');
    expect(result.snapshot.enumerationComplete).toBe(false);
    expect(result.snapshot.proof.minimumLinkCount).toBeNull();
    expect(result.snapshot.error).toContain('Browser collection budget reached');
    expect(result.solutions.length).toBeGreaterThan(0);
    verifyJobs([{ name: 'collection guard', request, ...result }]);
  }
});

test('the four-worker UI cancels blocked compute heaps and preserves its worker preference', async ({ page }) => {
  await observeWorkers(page);
  await injectSessionFailure(page, 'busy');
  await page.goto('./');
  await page.getByLabel('Browser compute workers').selectOption('4');
  await page.getByRole('button', { name: /^Find/ }).click();
  await expect.poll(() => page.evaluate(() => window.computeTest.checking)).toBe(true);
  await expect.poll(() => page.evaluate(() => window.computeTest.ticks)).toBeGreaterThan(3);
  await page.getByRole('button', { name: 'Cancel running job', exact: true }).click();
  await expect(page.locator('[data-history-band="history"]')).toContainText('Cancelled');
  const workers = await page.evaluate(() => window.computeTest);
  expect(workers.starts).toBeGreaterThan(1);
  expect(workers.peak).toBeLessThanOrEqual(4);
  expect(workers.live).toBe(0);
  expect(workers.coordinators).toBe(0);
  expect(workers.terminated).toBe(workers.starts);
  await page.reload();
  await expect(page.getByLabel('Browser compute workers')).toHaveValue('4');
  expect(await page.evaluate(() => window.computeTest.starts)).toBe(0);
});
