import { test, expect } from '@playwright/test';

const origin = 'http://127.0.0.1:4185';
const scope = `${origin}/satisfactory-flow-synthetizer/`;
const build = (page) => page.locator('meta[name="sfs-build"]').getAttribute('content');
const ready = (page) =>
  expect(page.getByText('Ready for offline solving and viewing.', { exact: true })).toBeVisible({ timeout: 60_000 });
async function prepare(page) {
  await ready(page);
}
async function configure(request, variant = 'a', failure = null) {
  await request.post(`${origin}/__control`, { data: { variant, failure } });
}
test.beforeEach(async ({ page, request }) => {
  await request.post(`${origin}/__control`, { data: { variant: 'a', hold: null } });
  await configure(request);
  await page.addInitScript(() => {
    localStorage.setItem('sfs.browser-workers.v2', '2');
  });
});

test('a second build opened during first installation acquires the active build before starting the app', async ({
  page,
  context,
  request,
}) => {
  await request.post(`${origin}/__control`, { data: { variant: 'a', hold: 'licenses.html' } });
  await page.goto('./');
  const firstBuild = await build(page);
  await expect.poll(async () => (await request.get(`${origin}/__held`)).json()).toBe(true);
  await configure(request, 'b');
  const second = await context.newPage();
  await second.goto(scope);
  expect(await build(second)).not.toBe(firstBuild);
  await expect(second.getByRole('status')).toContainText('Preparing the app');
  expect(await second.getByRole('button', { name: /^Find/ }).count()).toBe(0);
  await request.post(`${origin}/__control`, { data: { variant: 'b', hold: null } });
  await ready(page);
  await ready(second);
  expect(await build(second)).toBe(firstBuild);
  await context.setOffline(true);
  await second.getByRole('button', { name: /^Find/ }).click();
  await expect(second.locator('[data-history-band="history"]')).toContainText('Completed');
  await second.getByRole('button', { name: 'Share selected solution', exact: true }).click();
  await expect(second.getByRole('textbox', { name: 'Share link', exact: true })).toBeVisible();
});

test('the first visit caches automatically without starting compute, then every exact mode solves offline', async ({
  page,
  context,
}) => {
  const requests = [];
  const errors = [];
  page.on('request', (request) => requests.push(request.url()));
  page.on('pageerror', (error) => errors.push(error.message));
  await page.addInitScript(() => {
    const Original = Worker;
    window.testWorkers = 0;
    window.Worker = class extends Original {
      constructor(...args) {
        super(...args);
        window.testWorkers++;
      }
    };
  });
  await page.goto('./');
  await prepare(page);
  expect(await page.evaluate(() => window.testWorkers)).toBe(0);
  expect(await page.evaluate(() => Boolean(navigator.serviceWorker.controller))).toBe(true);
  await context.setOffline(true);
  await ready(page);
  for (const mode of ['One min N/L', 'All min N/L', 'All min N']) {
    await page.getByRole('radio', { name: mode, exact: true }).click();
    await page.getByRole('button', { name: `Find ${mode}`, exact: true }).click();
    await expect(page.locator('[data-history-band="history"]').filter({ hasText: 'Completed' })).toHaveCount(
      ['One min N/L', 'All min N/L', 'All min N'].indexOf(mode) + 1,
    );
  }
  await expect(page.locator('.svelte-flow__node')).toHaveCount(4);
  await page.getByRole('button', { name: 'Share selected solution', exact: true }).click();
  const shareLink = page.getByRole('textbox', { name: 'Share link', exact: true });
  await expect(shareLink).toBeVisible();
  const sharedUrl = await shareLink.inputValue();
  await page.getByRole('dialog').press('Escape');
  const shared = await context.newPage();
  await shared.goto(sharedUrl);
  const shareDialog = shared.getByRole('dialog', { name: 'Open shared solution' });
  await expect(shareDialog).toBeVisible();
  await expect(shareDialog.locator('.svelte-flow__node')).toHaveCount(4);
  await shared.close();
  // Wait for the normal metadata checkpoint, not a pagehide durability promise.
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const open = indexedDB.open('satisfactory-flow-synthetizer.history');
        const db = await new Promise((resolve, reject) => {
          open.onsuccess = () => resolve(open.result);
          open.onerror = () => reject(open.error);
        });
        try {
          return await new Promise((resolve) => {
            const read = db.transaction('entries').objectStore('entries').getAll();
            read.onsuccess = () => resolve(read.result.filter((entry) => entry.status === 'completed').length);
          });
        } finally {
          db.close();
        }
      }),
    )
    .toBe(3);
  await page.reload();
  await expect(page.locator('[data-history-band="history"]')).toHaveCount(3);
  expect(errors).toEqual([]);
  expect(requests.every((url) => url.startsWith(scope))).toBe(true);
});

for (const kind of ['missing', 'corrupt'])
  test(`a ${kind} solver asset cannot mark a partial download ready, and retry repairs it`, async ({
    page,
    request,
  }) => {
    await configure(request, 'a', { path: 'cvc5.wasm', kind });
    await page.goto('./');
    await expect(page.getByRole('alert')).toContainText('Could not save the complete offline version');
    await expect(page.getByText('Ready for offline solving and viewing.', { exact: true })).toHaveCount(0);
    await configure(request);
    await page.getByRole('button', { name: 'Retry offline download', exact: true }).click();
    await expect(
      page.getByText('Offline files are saved. Finish your work, then reopen the app to use them.'),
    ).toBeVisible();
    await page.reload();
    await prepare(page);
  });

test('an unused first-visit solver and verifier survive replacement of their files on the host', async ({
  page,
  context,
  request,
}) => {
  const oldRelease = await (await request.get(`${scope}release.json`)).json();
  const oldSolver = oldRelease.assets.find((asset) => asset.path.endsWith('/cvc5.wasm')).path;
  const oldVerifier = oldRelease.assets.find((asset) => asset.path.endsWith('/solver_web_bg.wasm')).path;
  await page.goto('./');
  await ready(page);
  const before = await build(page);
  await configure(request, 'b');
  expect((await request.get(`${scope}${oldSolver}`)).status()).toBe(404);
  expect((await request.get(`${scope}${oldVerifier}`)).status()).toBe(404);
  // Returning connectivity triggers the update check without a button click.
  await context.setOffline(true);
  await context.setOffline(false);
  await expect(page.getByText('An update is ready.', { exact: false })).toBeVisible({ timeout: 60_000 });
  expect(await build(page)).toBe(before);
  await context.setOffline(true);
  await page.getByRole('button', { name: /^Find/ }).click();
  await expect(page.locator('[data-history-band="history"]')).toContainText('Completed');
  await page.getByRole('button', { name: 'Share selected solution', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Share link', exact: true })).toBeVisible();
  await page.getByRole('dialog').press('Escape');
  await page.reload();
  expect(await build(page)).toBe(before);
  await ready(page);
});

test('a full offline update waits for every app tab and cannot terminate running compute ownership', async ({
  page,
  context,
  request,
}) => {
  await page.goto('./');
  await prepare(page);
  await page.reload();
  await ready(page);
  const before = await build(page);
  const second = await context.newPage();
  await second.goto(scope);
  await page.evaluate(() => {
    const post = Worker.prototype.postMessage;
    const terminate = Worker.prototype.terminate;
    window.testTerminations = 0;
    Worker.prototype.terminate = function (...args) {
      window.testTerminations++;
      return terminate.apply(this, args);
    };
    Worker.prototype.postMessage = function (message, ...args) {
      // Keep an actual job waiting for a leaf without editing production code.
      if (message?.kind === 'leaf') return;
      return post.call(this, message, ...args);
    };
  });
  await page.getByRole('button', { name: /^Find/ }).click();
  await expect(page.locator('[data-history-band="running"]')).toHaveCount(1);
  await configure(request, 'b');
  await page.getByRole('button', { name: 'Check for updates', exact: true }).click();
  await expect(page.getByText('An update is ready.', { exact: false })).toBeVisible({ timeout: 60_000 });
  expect(await build(page)).toBe(before);
  expect(await build(second)).toBe(before);
  expect(await page.evaluate(() => window.testTerminations)).toBe(0);
  await second.reload();
  expect(await build(second)).toBe(before);
  await page.getByRole('button', { name: 'Cancel running job', exact: true }).click();
  await expect(page.locator('[data-history-band="history"]')).toHaveCount(1);
  await expect(page.locator('[data-history-band="history"]')).toContainText('Cancelled');
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const db = await new Promise((resolve) => {
          const open = indexedDB.open('satisfactory-flow-synthetizer.history');
          open.onsuccess = () => resolve(open.result);
        });
        try {
          return await new Promise((resolve) => {
            const read = db.transaction('entries').objectStore('entries').getAll();
            read.onsuccess = () => resolve(read.result.filter((entry) => entry.status === 'cancelled').length);
          });
        } finally {
          db.close();
        }
      }),
    )
    .toBe(1);
  await second.close();
  await page.goto(`${origin}/__outside`);
  await expect
    .poll(() =>
      page.evaluate(async (scope) => {
        const registration = await navigator.serviceWorker.getRegistration(scope);
        return registration && !registration.waiting && !registration.installing;
      }, scope),
    )
    .toBe(true);
  await context.setOffline(true);
  await page.goto(scope);
  await ready(page);
  expect(await build(page)).not.toBe(before);
  await expect(page.locator('[data-history-band="history"]')).toHaveCount(1);
  await page.getByRole('button', { name: /^Find/ }).click();
  await expect(page.locator('[data-history-band="history"]').filter({ hasText: 'Completed' })).toHaveCount(1);
});

test('a failed update keeps the prior full offline build and removes only its failed candidate cache', async ({
  page,
  context,
  request,
}) => {
  await page.goto('./');
  await prepare(page);
  await page.reload();
  await ready(page);
  const before = await build(page);
  await page.evaluate(() =>
    caches.open('unrelated-user-cache').then((cache) => cache.put('/unrelated', new Response('keep'))),
  );
  await configure(request, 'b', { path: 'cvc5.wasm', kind: 'corrupt' });
  await page.getByRole('button', { name: 'Check for updates', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('current version was kept', { timeout: 60_000 });
  const keys = await page.evaluate(() => caches.keys());
  expect(keys.filter((key) => key.startsWith('sfs-offline-v1:'))).toHaveLength(1);
  expect(keys).toContain('unrelated-user-cache');
  await context.setOffline(true);
  await page.reload();
  await ready(page);
  expect(await build(page)).toBe(before);
});

test('cache eviction is visible, repair is explicit, and unknown URLs do not become app HTML', async ({
  page,
  context,
}) => {
  await page.goto('./');
  await prepare(page);
  await page.reload();
  await ready(page);
  await page.evaluate(async () => {
    const name = (await caches.keys()).find((name) => name.startsWith('sfs-offline-v1:'));
    const cache = await caches.open(name);
    const asset = (await cache.keys()).find((request) => request.url.endsWith('/cvc5.wasm'));
    await cache.delete(asset);
  });
  await context.setOffline(true);
  await expect(page.getByText('Some offline files are missing.', { exact: false })).toBeVisible();
  await context.setOffline(false);
  await page.getByRole('button', { name: 'Retry offline download', exact: true }).click();
  await prepare(page);
  const missing = await page.evaluate(() => fetch('./nonexistent').then((response) => response.status));
  expect(missing).toBe(404);
});

test('project paths have independent offline registrations and caches', async ({ page, context }) => {
  await page.goto('./');
  await prepare(page);
  await page.reload();
  await ready(page);
  const other = await context.newPage();
  await other.goto(`${origin}/second-app/`);
  await prepare(other);
  await other.reload();
  const scopes = await page.evaluate(async () =>
    (await navigator.serviceWorker.getRegistrations()).map((registration) => registration.scope).sort(),
  );
  expect(scopes).toEqual([scope, `${origin}/second-app/`].sort());
  await context.setOffline(true);
  await other.reload();
  await ready(other);
  await page.reload();
  await ready(page);
});
