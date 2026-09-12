import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

const fixture = JSON.parse(
  await readFile(new URL('../../src-tauri/history-fixtures/incremental.json', import.meta.url), 'utf8'),
);

async function storageOnlyPage(page) {
  await page.route('**/satisfactory-flow-synthetizer/__storage-test', (route) =>
    route.fulfill({ contentType: 'text/html', body: '<!doctype html><title>Storage fixture</title>' }),
  );
  await page.goto('./__storage-test');
}

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
  await expect(page.getByLabel('Browser compute workers')).toBeVisible();
});

test.afterEach(async ({ page }) => {
  expect(await page.evaluate(() => window.tauriCalls)).toBe(0);
});

test('the native incremental HistoryOp fixture matches replacement at each checkpoint', async ({ page }) => {
  const result = await page.evaluate(async (fixture) => {
    const { createBrowserHistoryStore } =
      await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
    const left = createBrowserHistoryStore('sfs-test-incremental');
    const right = createBrowserHistoryStore('sfs-test-replacement');
    try {
      await left.load();
      await right.load();
      let entry = fixture.initial;
      for (const store of [left, right])
        await store.apply([
          { op: 'upsertEntry', entry },
          { op: 'setSelected', id: entry.id },
        ]);
      const checkpoints = [];
      for (const checkpoint of fixture.checkpoints) {
        entry = { ...entry, ...checkpoint.patch };
        await left.apply(checkpoint.ops);
        await right.apply([{ op: 'upsertEntry', entry }]);
        checkpoints.push({ name: checkpoint.name, left: await left.load(), right: await right.load() });
      }
      return checkpoints;
    } finally {
      left.close();
      right.close();
    }
  }, fixture);
  expect(result.length).toBeGreaterThan(0);
  for (const checkpoint of result) expect(checkpoint.left, checkpoint.name).toEqual(checkpoint.right);
});

test('late transaction aborts roll back all writes and do not acknowledge a save', async ({ page }) => {
  const result = await page.evaluate(async () => {
    const { createBrowserHistoryStore } =
      await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
    const { historyEntry } = await import('/satisfactory-flow-synthetizer/src/test/fixtures.ts');
    const store = createBrowserHistoryStore('sfs-test-abort');
    const entry = historyEntry();
    await store.load();
    await store.apply([
      { op: 'upsertEntry', entry },
      { op: 'setSelected', id: entry.id },
    ]);
    const original = IDBObjectStore.prototype.put;
    let aborted = false;
    IDBObjectStore.prototype.put = function (...args) {
      const operation = original.apply(this, args);
      if (this.name === 'meta')
        operation.addEventListener(
          'success',
          () => {
            aborted = true;
            this.transaction.abort();
          },
          { once: true },
        );
      return operation;
    };
    let error;
    try {
      await store.apply([
        { op: 'deleteEntries', ids: [entry.id] },
        { op: 'setSelected', id: null },
      ]);
    } catch (failure) {
      error = String(failure);
    } finally {
      IDBObjectStore.prototype.put = original;
    }
    // A retry before reloading proves the store did not advance its acknowledged revision.
    await store.apply([{ op: 'appendSolutions', id: entry.id, expectedCount: 1, solutions: [entry.results[0]] }]);
    const loaded = await store.load();
    store.close();
    return { aborted, error, loaded };
  });
  expect(result.aborted).toBe(true);
  expect(result.error).toContain('abort');
  expect(result.loaded.selectedEntryId).toBe('entry');
  expect(result.loaded.entries[0].results).toHaveLength(2);
});

test('append prefix failures and quota errors leave the previous checkpoint intact', async ({ page }) => {
  const result = await page.evaluate(async () => {
    const { createBrowserHistoryStore } =
      await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
    const { historyEntry } = await import('/satisfactory-flow-synthetizer/src/test/fixtures.ts');
    const store = createBrowserHistoryStore('sfs-test-failures');
    await store.load();
    const entry = historyEntry();
    await store.apply([{ op: 'upsertEntry', entry }]);
    let prefix;
    try {
      await store.apply([
        { op: 'setSelected', id: 'wrong' },
        { op: 'appendSolutions', id: entry.id, expectedCount: 99, solutions: entry.results },
      ]);
    } catch (error) {
      prefix = String(error);
    }
    const original = IDBObjectStore.prototype.add;
    IDBObjectStore.prototype.add = function (...args) {
      if (this.name === 'solutions') throw new DOMException('Injected storage quota failure', 'QuotaExceededError');
      return original.apply(this, args);
    };
    let quota;
    try {
      await store.apply([
        { op: 'setSelected', id: 'wrong' },
        { op: 'appendSolutions', id: entry.id, expectedCount: 1, solutions: entry.results },
      ]);
    } catch (error) {
      quota = String(error);
    } finally {
      IDBObjectStore.prototype.add = original;
    }
    await store.apply([{ op: 'setSelected', id: entry.id }]);
    const loaded = await store.load();
    store.close();
    return { prefix, quota, loaded };
  });
  expect(result.prefix).toContain('history_append_prefix_mismatch:');
  expect(result.quota).toContain('QuotaExceededError');
  expect(result.loaded.entries[0].results).toHaveLength(1);
  expect(result.loaded.selectedEntryId).toBe('entry');
});

test('independent tabs reject stale revisions instead of overwriting history', async ({ page, context }) => {
  const other = await context.newPage();
  try {
    await other.goto('./');
    for (const tab of [page, other]) {
      await tab.evaluate(async () => {
        const { createBrowserHistoryStore } =
          await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
        window.testStore = createBrowserHistoryStore('sfs-test-conflict');
        await window.testStore.load();
      });
    }
    await page.evaluate(async () => {
      const { historyEntry } = await import('/satisfactory-flow-synthetizer/src/test/fixtures.ts');
      await window.testStore.apply([{ op: 'upsertEntry', entry: historyEntry({ title: 'First tab' }) }]);
    });
    const error = await other.evaluate(async () => {
      const { historyEntry } = await import('/satisfactory-flow-synthetizer/src/test/fixtures.ts');
      try {
        await window.testStore.apply([{ op: 'upsertEntry', entry: historyEntry({ title: 'Stale tab' }) }]);
      } catch (error) {
        return String(error);
      }
    });
    expect(error).toContain('History changed in another tab');
    const saved = await page.evaluate(() => window.testStore.load());
    expect(saved.entries[0].title).toBe('First tab');
  } finally {
    await page.evaluate(() => window.testStore?.close());
    await other.evaluate(() => window.testStore?.close());
    await other.close();
  }
});

test('history checkpoints keep interrupted solutions, exclude queued work and restore edits after reload', async ({
  page,
}) => {
  // Seed before mounting the UI so its live empty state cannot race the fixture write.
  await storageOnlyPage(page);
  await page.evaluate(async () => {
    const { applyHistoryChanges, loadHistoryDocument } =
      await import('/satisfactory-flow-synthetizer/src/lib/historyPersist.ts');
    const { historyEntry } = await import('/satisfactory-flow-synthetizer/src/test/fixtures.ts');
    await loadHistoryDocument();
    const running = historyEntry({
      id: 'running',
      title: 'Interrupted search',
      status: 'running',
      jobId: 'obsolete',
      enumerationComplete: true,
    });
    running.layouts = { 0: { layoutKey: 'saved-position', nodes: [], edges: [] } };
    await applyHistoryChanges([running, historyEntry({ id: 'queued', status: 'queued' })], running.id);
  });
  await page.goto('./');
  await expect(page.locator('[data-history-id="running"]')).toContainText('Incomplete');
  await expect(page.locator('[data-history-id="queued"]')).toHaveCount(0);
  const entry = await page.evaluate(async () => {
    const { getPlatform } = await import('/satisfactory-flow-synthetizer/src/lib/platform/index.ts');
    return (await getPlatform().history.load()).entries[0];
  });
  expect(entry.jobId).toBeNull();
  expect(entry.enumerationComplete).toBe(false);
  expect(entry.error).toContain('Interrupted before completion');
  expect(entry.results).toHaveLength(1);
  // Graph hydration may replace an incompatible cache key, but it never reruns the solver.
  await expect(page.getByRole('button', { name: /^Find/ })).toBeEnabled();
});

test('the Svelte UI imports, renames and reloads browser-local history without Tauri', async ({ page }) => {
  const payload = await page.evaluate(async () => {
    const { historyEntry } = await import('/satisfactory-flow-synthetizer/src/test/fixtures.ts');
    return { kind: 'history-bundle', version: 2, entries: [historyEntry({ title: 'Browser import' })] };
  });
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'History actions' }).click();
  await page.getByRole('menuitem', { name: /Import history/ }).click();
  await (
    await chooser
  ).setFiles({ name: 'history.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(payload)) });
  const card = page.locator('[data-history-band="history"]').first();
  await expect(card).toContainText('Browser import');
  await card.getByRole('button', { name: 'More actions' }).click();
  await page.getByRole('menuitem', { name: 'Rename', exact: true }).click();
  await page.getByRole('textbox', { name: 'Rename history entry' }).fill('Saved in this browser');
  await page.getByRole('textbox', { name: 'Rename history entry' }).press('Enter');
  const node = page.locator('.svelte-flow__node[data-id="s"]');
  await expect(node).toBeVisible();
  await node.scrollIntoViewIfNeeded();
  const originalPosition = await node.evaluate((element) => element.style.transform);
  const box = await node.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 70, box.y + box.height / 2 + 40, { steps: 12 });
  await page.mouse.up();
  const editedPosition = await node.evaluate((element) => element.style.transform);
  const savedPosition = await node.evaluate((element) => {
    const matrix = new DOMMatrixReadOnly(element.style.transform);
    return { x: matrix.m41, y: matrix.m42 };
  });
  expect(editedPosition).not.toBe(originalPosition);
  // Read with an independent connection so the test waits for the committed transaction.
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const { createBrowserHistoryStore } =
          await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
        const store = createBrowserHistoryStore();
        try {
          return (await store.load()).entries[0]?.title;
        } finally {
          store.close();
        }
      }),
    )
    .toBe('Saved in this browser');
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const { createBrowserHistoryStore } =
          await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
        const store = createBrowserHistoryStore();
        try {
          return (await store.load()).entries[0]?.layouts['0']?.nodes.find((node) => node.id === 's')?.position;
        } finally {
          store.close();
        }
      }),
    )
    .toEqual(savedPosition);
  await page.reload();
  await expect(page.locator('[data-history-band="history"]')).toContainText('Saved in this browser');
  await expect
    .poll(() => page.locator('.svelte-flow__node[data-id="s"]').evaluate((element) => element.style.transform))
    .toBe(editedPosition);
});

test('unknown database versions and damaged metadata are rejected without resetting storage', async ({ page }) => {
  const results = await page.evaluate(async () => {
    const { createBrowserHistoryStore } =
      await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
    const future = await new Promise((resolve, reject) => {
      const operation = indexedDB.open('sfs-test-future', 99);
      operation.onupgradeneeded = () => operation.result.createObjectStore('sentinel').put('keep', 'value');
      operation.onsuccess = () => resolve(operation.result);
      operation.onerror = () => reject(operation.error);
    });
    future.close();
    const store = createBrowserHistoryStore('sfs-test-future');
    let versionError;
    try {
      await store.load();
    } catch (error) {
      versionError = String(error);
    }
    store.close();
    const preserved = await new Promise((resolve, reject) => {
      const operation = indexedDB.open('sfs-test-future');
      operation.onsuccess = () => {
        const db = operation.result;
        const read = db.transaction('sentinel').objectStore('sentinel').get('value');
        read.onsuccess = () => {
          resolve(read.result);
          db.close();
        };
      };
      operation.onerror = () => reject(operation.error);
    });
    const broken = createBrowserHistoryStore('sfs-test-corrupt');
    await broken.load();
    broken.close();
    await new Promise((resolve, reject) => {
      const operation = indexedDB.open('sfs-test-corrupt');
      operation.onsuccess = () => {
        const db = operation.result;
        const tx = db.transaction('meta', 'readwrite');
        tx.objectStore('meta').delete('document');
        tx.oncomplete = () => {
          db.close();
          resolve();
        };
      };
      operation.onerror = () => reject(operation.error);
    });
    let corruptError;
    try {
      await broken.load();
    } catch (error) {
      corruptError = String(error);
    }
    broken.close();
    return { versionError, preserved, corruptError };
  });
  expect(results.versionError).toContain('VersionError');
  expect(results.preserved).toBe('keep');
  expect(results.corruptError).toContain('Invalid browser history metadata');
});

for (const action of ['Delete', 'Delete all'])
  test(`${action} removes a large imported collection before its first history checkpoint`, async ({ page }) => {
    const payload = await page.evaluate(async () => {
      const { historyEntry } = await import('/satisfactory-flow-synthetizer/src/test/fixtures.ts');
      // Hold only the normal checkpoint timers; explicit deletion flushes must
      // still run. This makes the pre-checkpoint race deterministic.
      const schedule = window.setTimeout;
      window.setTimeout = (callback, delay, ...args) =>
        schedule(callback, [400, 5000].includes(delay) ? 120_000 : delay, ...args);
      const entry = historyEntry({ title: 'Uncheckpointed import' });
      entry.results = Array.from({ length: 65 }, () => structuredClone(entry.result));
      return { kind: 'history-bundle', version: 2, entries: [entry] };
    });
    const counts = () =>
      page.evaluate(async () => {
        const db = await new Promise((resolve, reject) => {
          const open = indexedDB.open('satisfactory-flow-synthetizer.history');
          open.onsuccess = () => resolve(open.result);
          open.onerror = () => reject(open.error);
        });
        const values = await Promise.all(
          ['entries', 'collections', 'collectionSolutions', 'collectionSummaries'].map(
            (name) =>
              new Promise((resolve) => {
                const read = db.transaction(name).objectStore(name).count();
                read.onsuccess = () => resolve(read.result);
              }),
          ),
        );
        db.close();
        return values;
      });
    const chooser = page.waitForEvent('filechooser');
    await page.getByRole('button', { name: 'History actions', exact: true }).click();
    await page.getByRole('menuitem', { name: /Import history/ }).click();
    await (
      await chooser
    ).setFiles({ name: 'large.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(payload)) });
    const card = page.locator('[data-history-band="history"]');
    await expect(card).toContainText('Uncheckpointed import');
    expect(await counts()).toEqual([0, 1, 65, 65]);
    if (action === 'Delete') {
      await card.getByRole('button', { name: 'More actions' }).click();
      await page.getByRole('menuitem', { name: 'Delete', exact: true }).click();
    } else {
      await page.getByRole('button', { name: 'History actions', exact: true }).click();
      await page.getByRole('menuitem', { name: /Delete all history/ }).click();
      await page.getByRole('button', { name: 'Delete all', exact: true }).click();
    }
    await expect(card).toHaveCount(0);
    await expect.poll(counts).toEqual([0, 0, 0, 0]);
    await page.reload();
    await expect(card).toHaveCount(0);
    expect(await counts()).toEqual([0, 0, 0, 0]);
  });

test('the shared persister repairs a lost append acknowledgement without rewriting another entry', async ({ page }) => {
  await storageOnlyPage(page);
  const result = await page.evaluate(async () => {
    const { loadHistoryDocument, applyHistoryChanges } =
      await import('/satisfactory-flow-synthetizer/src/lib/historyPersist.ts');
    const { getPlatform } = await import('/satisfactory-flow-synthetizer/src/lib/platform/index.ts');
    const { historyEntry } = await import('/satisfactory-flow-synthetizer/src/test/fixtures.ts');
    await loadHistoryDocument();
    const first = historyEntry({ id: 'first' });
    const other = historyEntry({ id: 'other', title: 'Keep this entry' });
    await applyHistoryChanges([first, other], first.id);
    const next = { ...first, results: [...first.results, first.results[0]] };
    const store = getPlatform().history;
    const apply = store.apply;
    store.apply = async (ops) => {
      await apply(ops);
      throw new Error('Lost acknowledgement after commit');
    };
    let error;
    try {
      await applyHistoryChanges([next, other], first.id);
    } catch (failure) {
      error = String(failure);
    }
    store.apply = apply;
    const put = IDBObjectStore.prototype.put;
    const written = [];
    IDBObjectStore.prototype.put = function (...args) {
      if (this.name === 'entries') written.push(args[0].id);
      return put.apply(this, args);
    };
    try {
      await applyHistoryChanges([next, other], first.id);
    } finally {
      IDBObjectStore.prototype.put = put;
    }
    return { error, written, document: await store.load() };
  });
  expect(result.error).toContain('Lost acknowledgement');
  expect(result.written).toEqual(['first']);
  expect(result.document.entries[0].results).toHaveLength(2);
  expect(result.document.entries[1].title).toBe('Keep this entry');
});

test('ordering, selection and child records survive delete and replacement batches', async ({ page }) => {
  const result = await page.evaluate(async () => {
    const { createBrowserHistoryStore } =
      await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
    const { historyEntry } = await import('/satisfactory-flow-synthetizer/src/test/fixtures.ts');
    const store = createBrowserHistoryStore('sfs-test-order');
    await store.load();
    const entries = ['a', 'b', 'c'].map((id) =>
      historyEntry({ id, layouts: { 0: { layoutKey: id, nodes: [], edges: [] } } }),
    );
    await store.apply([
      ...entries.map((entry) => ({ op: 'upsertEntry', entry })),
      { op: 'reorderEntries', ids: ['c', 'b', 'a'] },
      { op: 'setSelected', id: 'b' },
    ]);
    const before = await store.load();
    await store.apply([
      { op: 'deleteEntries', ids: ['b'] },
      { op: 'reorderEntries', ids: ['a', 'c'] },
      { op: 'setSelected', id: 'c' },
    ]);
    const after = await store.load();
    store.close();
    return { before, after };
  });
  expect(result.before.entries.map((entry) => entry.id)).toEqual(['c', 'b', 'a']);
  expect(result.before.selectedEntryId).toBe('b');
  expect(result.after.entries.map((entry) => entry.id)).toEqual(['a', 'c']);
  expect(result.after.selectedEntryId).toBe('c');
  expect(result.after.entries.map((entry) => entry.layouts['0'].layoutKey)).toEqual(['a', 'c']);
});
