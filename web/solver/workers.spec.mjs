import { test, expect } from '@playwright/test';

test('automatic uses the real client thread count for a completed browser solve', async ({ page }, testInfo) => {
  await page.goto('./');
  const threads = await page.evaluate(() => navigator.hardwareConcurrency);
  const select = page.getByLabel('Browser compute workers');
  await expect(select).toHaveValue('auto');
  await expect(select.locator(`option[value="${threads}"]`)).toHaveCount(1);
  await page.evaluate(async () => {
    const { getPlatform } = await import('/satisfactory-flow-synthetizer/src/lib/platform/index.ts');
    const jobs = getPlatform().jobs;
    const create = jobs.create;
    window.requestedWorkers = null;
    jobs.create = (request) => {
      window.requestedWorkers = request.browserWorkers;
      return create(request);
    };
  });
  await page.getByRole('button', { name: /^Find/ }).click();
  expect(await page.evaluate(() => window.requestedWorkers)).toBe(threads);
  await expect(page.locator('[data-history-band="history"]')).toContainText('Completed', { timeout: 60_000 });
  await page.reload();
  await expect(select).toHaveValue('auto');
  await testInfo.attach('client-thread-default', {
    body: JSON.stringify({ reportedThreads: threads, defaultMode: 'auto' }),
    contentType: 'application/json',
  });
});

for (const threads of [12, 24, 64, 192]) {
  test(`the selector includes all ${threads} reported threads and defaults to automatic`, async ({ page }) => {
    await page.addInitScript((threads) => {
      Object.defineProperty(navigator, 'hardwareConcurrency', { configurable: true, value: threads });
      localStorage.setItem('sfs.browser-workers.v1', '1');
    }, threads);
    await page.goto('./');
    const select = page.getByLabel('Browser compute workers');
    await expect(select).toHaveValue('auto');
    await expect(select.locator('option[value="auto"]')).toHaveText(`Automatic (${threads} workers)`);
    const counts = await select
      .locator('option:not([value="auto"])')
      .evaluateAll((options) => options.map((option) => Number(option.value)));
    expect(Math.max(...counts)).toBe(threads);
    expect(counts).toContain(1);
    await select.selectOption(String(threads));
    await page.reload();
    await expect(select).toHaveValue(String(threads));
    await select.selectOption('auto');
    await page.reload();
    await expect(select).toHaveValue('auto');
  });
}
