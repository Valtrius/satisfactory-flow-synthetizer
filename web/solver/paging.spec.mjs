import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { runJob } from './helpers.mjs';

async function savedEntry(page) {
  return page.evaluate(async () => {
    const { createBrowserHistoryStore } =
      await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
    const store = createBrowserHistoryStore();
    try {
      return (await store.load()).entries[0];
    } finally {
      store.close();
    }
  });
}

test('incrementally loaded UI preserves off-screen selection, graph edits, sharing and source-ordered history export', async ({
  page,
}) => {
  await page.addInitScript(() =>
    Object.defineProperty(navigator, 'hardwareConcurrency', { configurable: true, value: 4 }),
  );
  await page.goto('./');
  const request = {
    inputs: [],
    outputs: ['3', '2', '1'].map((rate, index) => ({ id: `o${index}`, name: '', rate })),
    beltRate: '60',
    solveMode: 'all_min_n',
  };
  const known = await runJob(page, request, { workerCount: 4 });
  expect(known.snapshot.status).toBe('completed');
  expect(known.solutions.length).toBeGreaterThan(1);
  const payload = await page.evaluate(
    async ({ request, solutions }) => {
      const { historyEntry } = await import('/satisfactory-flow-synthetizer/src/test/fixtures.ts');
      // Repeat known feasible layouts only to exercise paging, not as an
      // assertion that 145 distinct mathematical solutions were enumerated.
      return {
        kind: 'history-bundle',
        version: 2,
        entries: [
          historyEntry({
            title: 'Paged UI fixture',
            status: 'incomplete',
            jobId: null,
            request,
            form: { ...request, inputs: [], outputs: request.outputs.map((value) => ({ ...value, multiplier: '1' })) },
            result: solutions[0],
            results: Array.from({ length: 145 }, (_, index) => solutions[index % solutions.length]),
            enumerationComplete: false,
            proof: { minimumNodeCount: null, minimumLinkCount: null },
            selectedSourceIndex: 0,
            layouts: {},
          }),
        ],
      };
    },
    { request, solutions: known.solutions },
  );
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'History actions' }).click();
  await page.getByRole('menuitem', { name: /Import history/ }).click();
  await (
    await chooser
  ).setFiles({ name: 'paged.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(payload)) });
  await expect(page.locator('[data-history-band="history"]')).toContainText('Paged UI fixture');
  await expect(page.locator('[data-layout-select]')).toHaveCount(64);
  await expect.poll(async () => (await savedEntry(page))?.collection?.count).toBe(145);
  await page.locator('[data-layout-select]').last().scrollIntoViewIfNeeded();
  await expect(page.locator('[data-layout-select]')).toHaveCount(128);
  await page.locator('[data-layout-select]').last().scrollIntoViewIfNeeded();
  await expect(page.locator('[data-layout-select]')).toHaveCount(145);
  const expected = await page.evaluate(async () => {
    const { getPlatform } = await import('/satisfactory-flow-synthetizer/src/lib/platform/index.ts');
    const { createSelectedShare } = await import('/satisfactory-flow-synthetizer/src/lib/sharing/client.ts');
    const platform = getPlatform();
    const entry = (await platform.history.load()).entries[0];
    const rows = await platform.collections.page(entry.collection, 128, 64, entry.sortColumns);
    const index = rows.rows[0].sourceIndex;
    const solution = await platform.collections.get(entry.collection, index);
    const share = (await createSelectedShare(entry.request, solution)).share;
    window.pageReads = 0;
    const read = platform.collections.page;
    platform.collections.page = (...args) => {
      window.pageReads++;
      return read(...args);
    };
    return { index, share };
  });
  await page.locator('[data-layout-select="128"]').click();
  await expect.poll(async () => (await savedEntry(page))?.selectedSourceIndex).toBe(expected.index);
  await expect
    .poll(async () => (await savedEntry(page))?.layouts[expected.index]?.nodes.length ?? 0)
    .toBeGreaterThan(0);
  const before = (await savedEntry(page)).layouts[expected.index];
  await page.getByRole('button', { name: 'Rotate graph 90 degrees clockwise', exact: true }).click();
  await expect
    .poll(async () => JSON.stringify((await savedEntry(page))?.layouts[expected.index]?.nodes))
    .not.toBe(JSON.stringify(before.nodes));
  const edited = (await savedEntry(page)).layouts[expected.index];
  expect(await page.evaluate(() => window.pageReads)).toBe(0);
  await page.locator('[data-layout-select]').first().scrollIntoViewIfNeeded();
  await expect(page.locator('[data-layout-select]')).toHaveCount(145);
  expect((await savedEntry(page)).selectedSourceIndex).toBe(expected.index);

  await page.getByRole('button', { name: 'Share solution', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Share solution' });
  const link = dialog.getByRole('textbox', { name: 'Share link', exact: true });
  await expect(link).toBeVisible();
  const shared = await page.evaluate(
    async (link) => {
      const { decodeShare } = await import('/satisfactory-flow-synthetizer/src/lib/sharing/codec.ts');
      return JSON.parse(await decodeShare(new URL(link).hash.slice(1)));
    },
    await link.inputValue(),
  );
  expect(shared).toEqual(expected.share);
  const fetched = [];
  page.on('request', (request) => fetched.push(request.url()));
  await page.reload();
  await expect(page.locator('[data-layout-select]')).toHaveCount(64);
  await expect(page.locator('.svelte-flow__node').first()).toBeVisible();
  expect((await savedEntry(page)).selectedSourceIndex).toBe(expected.index);
  expect((await savedEntry(page)).layouts[expected.index].nodes).toEqual(edited.nodes);
  expect(fetched.some((url) => /solver_browser|cvc5|session\.mjs/.test(url))).toBe(false);

  const download = page.waitForEvent('download');
  await page.evaluate(async () => {
    const { getPlatform } = await import('/satisfactory-flow-synthetizer/src/lib/platform/index.ts');
    const { exportHistoryEntry } = await import('/satisfactory-flow-synthetizer/src/lib/historyIo.ts');
    await exportHistoryEntry((await getPlatform().history.load()).entries[0]);
  });
  const exported = JSON.parse(await readFile(await (await download).path(), 'utf8'));
  expect(exported.kind).toBe('history-entry');
  expect(exported.entry.collection).toBeUndefined();
  expect(exported.entry.results).toEqual(payload.entries[0].results);
  expect(exported.entry.selectedSourceIndex).toBe(expected.index);
  expect(exported.entry.layouts[expected.index].nodes).toEqual(edited.nodes);
});
