import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { gzipSync } from 'node:zlib';

const fixtures = JSON.parse(await readFile(new URL('../../target/web-shares/fixtures.json', import.meta.url), 'utf8'));
const token = (value) =>
  `s1.${gzipSync(typeof value === 'string' || Buffer.isBuffer(value) ? value : JSON.stringify(value)).toString('base64url')}`;
const share = fixtures[0].native.share;
const app = 'http://127.0.0.1:4181/satisfactory-flow-synthetizer/';

test.beforeEach(async ({ context, page }) => {
  await context.addInitScript(() => {
    window.tauriCalls = 0;
    window.__TAURI_INTERNALS__ = {
      invoke() {
        window.tauriCalls++;
        throw new Error('Browser attempted native IPC');
      },
    };
  });
  await page.goto('./');
});
test.afterEach(async ({ page }) => {
  expect(await page.evaluate(() => window.tauriCalls)).toBe(0);
});

test('native selected presentations and topology reconstruction match the real Wasm worker', async ({ page }) => {
  const requests = [];
  page.on('request', (request) => requests.push(request.url()));
  for (const fixture of fixtures) {
    const migrated = await page.evaluate(async (source) => {
      const { runShareWorker } = await import('/satisfactory-flow-synthetizer/src/lib/sharing/client.ts');
      return runShareWorker({ operation: 'create', source });
    }, fixture.payload);
    expect(migrated.share, fixture.name).toEqual(fixture.native.share);
    expect(migrated.solution, fixture.name).toEqual(fixture.native.solution);
    const opened = await page.evaluate(async (source) => {
      const { runShareWorker } = await import('/satisfactory-flow-synthetizer/src/lib/sharing/client.ts');
      return runShareWorker({ operation: 'open', source });
    }, migrated.token);
    expect(opened.solution, fixture.name).toEqual(fixture.opened.solution);
    expect(opened.solution.status).toBe('best_known');
    expect(opened.solution.proof).toBeNull();
    expect(opened.share.request.solveMode).toBeUndefined();
  }
  expect(requests.some((url) => /cvc5|session\.mjs/.test(url))).toBe(false);
});

test('the codec bounds decompression and rejects damaged data, invalid UTF-8 and unsupported versions', async ({
  page,
}) => {
  const cases = [token(' '.repeat(262_145)), token(Buffer.from([255, 254])), 's2.abcd', 's1.A', 's1.a'.repeat(40_000)];
  const gzip = gzipSync(JSON.stringify(share));
  cases.push(`s1.${gzip.subarray(0, gzip.length - 3).toString('base64url')}`);
  for (const source of cases) {
    const rejected = await page.evaluate(async (source) => {
      const { decodeShare } = await import('/satisfactory-flow-synthetizer/src/lib/sharing/codec.ts');
      try {
        await decodeShare(source);
        return false;
      } catch {
        return true;
      }
    }, source);
    expect(rejected).toBe(true);
  }
  const invalid = { ...share, proof: { minimumNodeCount: 0 } };
  await page.goto(`${app}#${token(invalid)}`);
  await expect(page.getByRole('alert')).toContainText('unknown field');
  await expect(page.getByRole('button', { name: 'Save to history and view' })).toHaveCount(0);
});

test('inline links preview without saving and retain a proofless witness after explicit save and reload', async ({
  page,
}) => {
  await page.goto(`${app}#${token(share)}`);
  const dialog = page.getByRole('dialog', { name: 'Open shared solution' });
  await expect(dialog).toContainText('Physical solution verified');
  await expect(dialog.locator('.svelte-flow__node')).toHaveCount(2);
  await expect(page.locator('[data-history-band="history"]')).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Save to history and view' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator('[data-history-band="history"]')).toContainText('Best known');
  await page.reload();
  await expect(page.locator('[data-history-band="history"]')).toContainText('Shared solution');
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
  expect(saved.result.status).toBe('best_known');
  expect(saved.result.proof).toBeNull();
  expect(saved.proof).toBeNull();
  expect(saved.enumerationComplete).toBe(false);
  await expect(page.getByRole('button', { name: /^Find/ })).toBeDisabled();
});

test('untrusted endpoint text is rendered as text and never executes', async ({ page }) => {
  const value = structuredClone(share);
  value.request.outputs[0].name = '<img src=x onerror="window.shareInjected=true">';
  await page.goto(`${app}#${token(value)}`);
  await expect(page.getByRole('dialog')).toContainText('Physical solution verified');
  expect(await page.evaluate(() => window.shareInjected)).toBeUndefined();
  expect(await page.getByRole('dialog').locator('img').count()).toBe(0);
});

test('the graph toolbar shares the selected layout rather than the preferred result or collection', async ({
  page,
}) => {
  const examples = fixtures.slice(-2);
  const request = JSON.parse(examples[0].payload).request;
  const endpoints = (items, prefix) => items.map((item, index) => ({ ...item, id: `${prefix}-${index}` }));
  const savedRequest = {
    inputs: endpoints(request.inputs, 'input'),
    outputs: endpoints(request.outputs, 'output'),
    beltRate: request.beltRate,
    solveMode: 'all_min_nl',
  };
  const payload = await page.evaluate(
    async ({ savedRequest, solutions }) => {
      const { historyEntry } = await import('/satisfactory-flow-synthetizer/src/test/fixtures.ts');
      return {
        kind: 'history-bundle',
        version: 2,
        entries: [
          historyEntry({
            request: savedRequest,
            form: {
              ...savedRequest,
              inputs: savedRequest.inputs.map((item) => ({ ...item, multiplier: '1' })),
              outputs: savedRequest.outputs.map((item) => ({ ...item, multiplier: '1' })),
            },
            result: solutions[0],
            results: solutions,
            selectedSourceIndex: 1,
          }),
        ],
      };
    },
    { savedRequest, solutions: examples.map((example) => example.native.solution) },
  );
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'History actions' }).click();
  await page.getByRole('menuitem', { name: /Import history/ }).click();
  await (
    await chooser
  ).setFiles({ name: 'history.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(payload)) });
  await expect(page.getByRole('button', { name: 'Share selected solution', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Rotate graph 90 degrees clockwise' }).click();
  await page.getByRole('button', { name: 'Share selected solution', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Share selected solution' });
  await expect(dialog).toContainText('Physical solution verified');
  const value = await dialog.getByRole('textbox', { name: 'Share link', exact: true }).inputValue();
  const decoded = await page.evaluate(async (link) => {
    const { decodeShare } = await import('/satisfactory-flow-synthetizer/src/lib/sharing/codec.ts');
    return JSON.parse(await decodeShare(new URL(link).hash.slice(1)));
  }, value);
  expect(decoded).toEqual(examples[1].native.share);
  expect(decoded.topology).not.toEqual(examples[0].native.share.topology);
  expect(Object.keys(decoded).sort()).toEqual(['kind', 'request', 'topology', 'version']);
});

test('cancelling a verifier load retires its worker and a later attempt still works', async ({ page }) => {
  await page.route(
    '**/solver_web_bg.wasm',
    (route) =>
      new Promise((resolve) =>
        setTimeout(() => {
          void route
            .abort()
            .catch(() => {})
            .finally(resolve);
        }, 300),
      ),
  );
  const error = await page.evaluate(async (source) => {
    const { runShareWorker } = await import('/satisfactory-flow-synthetizer/src/lib/sharing/client.ts');
    const controller = new AbortController();
    const promise = runShareWorker({ operation: 'open', source }, controller.signal);
    setTimeout(() => controller.abort(), 30);
    try {
      await promise;
      return 'accepted';
    } catch (error) {
      return error.name;
    }
  }, token(share));
  expect(error).toBe('AbortError');
  await page.unroute('**/solver_web_bg.wasm');
  await page.goto(`${app}#${token(share)}`);
  await expect(page.getByRole('dialog')).toContainText('Physical solution verified');
});

test('the production build opens a native-generated link under its project path and CSP without cvc5', async ({
  page,
}) => {
  const requests = [];
  const errors = [];
  page.on('request', (request) => requests.push(request.url()));
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto(`http://127.0.0.1:4182/satisfactory-flow-synthetizer/#${token(fixtures.at(-1).native.share)}`);
  await expect(page.getByRole('dialog')).toContainText('Physical solution verified');
  await expect(page.getByRole('dialog').locator('.svelte-flow__node')).toHaveCount(13);
  expect(requests.filter((url) => /solver_web_bg\.wasm/.test(url))).toHaveLength(1);
  expect(requests.some((url) => /cvc5|session\.mjs/.test(url))).toBe(false);
  expect(requests.every((url) => url.startsWith('http://127.0.0.1:4182/'))).toBe(true);
  expect(errors).toEqual([]);
});

test('long links and JSON round trips need only static assets and have no link-management UI', async ({ page }) => {
  await expect(page.getByRole('button', { name: 'Manage short links', exact: true })).toHaveCount(0);
  const requests = [];
  page.on('request', (request) => requests.push({ url: request.url(), method: request.method() }));
  await page.goto(`${app}#${token(share)}`);
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Physical solution verified');
  await expect(dialog.getByRole('button', { name: /short link|revocation|revoke/i })).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Generate inline link' }).click();
  const link = await dialog.getByRole('textbox', { name: 'Share link', exact: true }).inputValue();
  expect(link).toMatch(/^http:\/\/127\.0\.0\.1:4181\/satisfactory-flow-synthetizer\/#s1\./);
  const downloading = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Export solution JSON' }).click();
  const contents = await readFile(await (await downloading).path());
  expect(JSON.parse(contents.toString())).toEqual(share);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await page.getByRole('button', { name: 'Open shared solution', exact: true }).click();
  const choosing = page.waitForEvent('filechooser');
  await dialog.getByRole('button', { name: 'Open solution file' }).click();
  await (await choosing).setFiles({ name: 'selected-solution.json', mimeType: 'application/json', buffer: contents });
  await expect(dialog).toContainText('Physical solution verified');
  await expect(page.locator('[data-history-band="history"]')).toHaveCount(0);
  expect(requests.every(({ url, method }) => method === 'GET' && new URL(url).origin === new URL(app).origin)).toBe(
    true,
  );
});

test('the verified preview fits a phone viewport and closes with Escape', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`${app}#${token(fixtures.at(-1).native.share)}`);
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Physical solution verified');
  await expect(dialog.locator('.svelte-flow__node')).toHaveCount(13);
  const size = await dialog.evaluate((element) => ({
    inner: element.clientWidth,
    content: element.scrollWidth,
    width: element.getBoundingClientRect().width,
    window: innerWidth,
  }));
  expect(size.content).toBeLessThanOrEqual(size.inner + 1);
  expect(size.width).toBeLessThanOrEqual(size.window);
  await testInfo.attach('mobile-verified-viewer', { body: await page.screenshot(), contentType: 'image/png' });
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
});
