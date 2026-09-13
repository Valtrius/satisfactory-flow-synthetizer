import { test, expect } from '@playwright/test';

test('automatic uses the reported client thread count for a completed browser solve', async ({ page }, testInfo) => {
  await page.addInitScript(() =>
    Object.defineProperty(navigator, 'hardwareConcurrency', { configurable: true, value: 4 }),
  );
  await page.goto('./');
  const threads = await page.evaluate(() => navigator.hardwareConcurrency);
  const select = page.getByRole('combobox', { name: 'Browser compute workers', exact: true });
  await expect(select).toContainText(`Automatic (${threads} ${threads === 1 ? 'worker' : 'workers'})`);
  await select.click();
  await expect(
    page
      .getByRole('listbox', { name: 'Browser compute workers options' })
      .getByRole('option', { name: `${threads} ${threads === 1 ? 'worker' : 'workers'}`, exact: true }),
  ).toHaveCount(1);
  await select.press('Escape');
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
  await expect(select).toContainText('Automatic');
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
    const select = page.getByRole('combobox', { name: 'Browser compute workers', exact: true });
    await expect(select).toContainText(`Automatic (${threads} workers)`);
    await select.click();
    const options = page.getByRole('listbox', { name: 'Browser compute workers options' });
    const counts = await options
      .getByRole('option')
      .evaluateAll((options) =>
        options.map((option) => Number.parseInt(option.textContent, 10)).filter(Number.isFinite),
      );
    expect(Math.max(...counts)).toBe(threads);
    expect(counts).toContain(1);
    await options.getByRole('option', { name: `${threads} workers`, exact: true }).click();
    await page.reload();
    await expect(select).toContainText(`${threads} workers`);
    await select.click();
    await options.getByRole('option', { name: `Automatic (${threads} workers)`, exact: true }).click();
    await page.reload();
    await expect(select).toContainText('Automatic');
  });
}
