import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  // Do not mount App while constructing storage fixtures: its empty history
  // would otherwise be a second writer to the same document.
  await page.route('**/__collections', (route) =>
    route.fulfill({
      contentType: 'text/html',
      body: '<!doctype html><title>Collection storage tests</title>',
    }),
  );
  await page.goto('./__collections');
  await page.evaluate(async () => {
    const { createBrowserHistoryStore } =
      await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
    const { historyEntry } = await import('/satisfactory-flow-synthetizer/src/test/fixtures.ts');
    window.store = createBrowserHistoryStore();
    await window.store.load();
    window.example = historyEntry();
    window.example.result.status = 'best_known';
    window.example.result.proof = null;
    window.example.results = [];
  });
});
test.afterEach(async ({ page }) => {
  await page.evaluate(() => window.store?.close());
});

test('large collections keep graph objects out of history and summary pages, with exact stable sorting', async ({
  page,
}, testInfo) => {
  const result = await page.evaluate(async () => {
    const collections = window.store.collections;
    let ref = await collections.create('large-storage-fixture');
    const count = 2048;
    // Synthetic records exercise storage ownership, not solver enumeration.
    for (let offset = 0; offset < count; offset += 16) {
      const rows = Array.from({ length: 16 }, (_, i) => {
        const index = offset + i;
        const solution = structuredClone(window.example.result);
        const exact = String(900719925474099300000000000000000000n + BigInt(index));
        solution.stats.internalMaxThroughput = { exact, decimal: exact };
        solution.buildSteps = [`Stored fixture ${index}`];
        return { index, solution };
      });
      ref = await collections.append(ref, rows);
    }
    const selected = 1947;
    const entry = {
      ...window.example,
      collection: { ...ref, preferredIndex: 0 },
      selectedSourceIndex: selected,
      layouts: { [selected]: { layoutKey: 'source-1947', nodes: [], edges: [] } },
    };
    await window.store.apply([
      { op: 'upsertEntry', entry },
      { op: 'setSelected', id: entry.id },
    ]);
    const graphReads = [];
    const original = IDBObjectStore.prototype.getAll;
    IDBObjectStore.prototype.getAll = function (...args) {
      if (this.name === 'collectionSolutions') graphReads.push(args[1] ?? null);
      return original.apply(this, args);
    };
    try {
      const loaded = await window.store.load();
      const first = await collections.page(ref, 0, 64, [{ key: 'peak', dir: 'desc' }]);
      const second = await collections.page(ref, 64, 64, [{ key: 'peak', dir: 'desc' }]);
      const readsBeforeSelection = [...graphReads];
      const chosen = await collections.get(ref, selected);
      const slice = await collections.read(ref, 1920, 128);
      return {
        count,
        loaded,
        first,
        second,
        chosen,
        readsBeforeSelection,
        graphReads,
        slice: slice.map((row) => row.index),
        metadataBytes: JSON.stringify(loaded).length,
        repeatedGraphBytes: JSON.stringify(chosen).length * count,
      };
    } finally {
      IDBObjectStore.prototype.getAll = original;
    }
  });
  expect(result.loaded.entries[0].results).toEqual([]);
  expect(result.loaded.entries[0].collection.count).toBe(2048);
  expect(result.loaded.entries[0].selectedSourceIndex).toBe(1947);
  expect(result.loaded.entries[0].layouts['1947'].layoutKey).toBe('source-1947');
  expect(result.first.rows).toHaveLength(64);
  expect(result.first.rows[0].sourceIndex).toBe(2047);
  expect(result.second.rows[0].sourceIndex).toBe(1983);
  expect(result.first.rows.every((row) => Object.keys(row.solution).join() === 'stats')).toBe(true);
  expect(result.chosen.buildSteps).toEqual(['Stored fixture 1947']);
  expect(result.readsBeforeSelection).toEqual([]);
  expect(result.graphReads).toEqual([1, 128]);
  expect(result.slice).toHaveLength(128);
  expect(result.metadataBytes).toBeLessThan(result.repeatedGraphBytes / 50);
  await testInfo.attach('collection-ownership', {
    body: JSON.stringify({
      records: result.count,
      visibleRows: result.first.rows.length,
      graphReads: result.graphReads,
      metadataBytes: result.metadataBytes,
      repeatedGraphBytes: result.repeatedGraphBytes,
      scope: 'Structural ownership check, not a process-memory or speed benchmark. Sorting retains stats, not graphs.',
    }),
    contentType: 'application/json',
  });
});

test('collection transactions reject gaps, mismatched retries and late aborts without changing the prefix', async ({
  page,
}) => {
  const result = await page.evaluate(async () => {
    const c = window.store.collections;
    const ref = await c.create('prefix');
    const rows = [{ index: 0, solution: window.example.result }];
    const original = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (...args) {
      const operation = original.apply(this, args);
      if (this.name === 'collections')
        operation.addEventListener('success', () => this.transaction.abort(), { once: true });
      return operation;
    };
    let abort;
    try {
      await c.append(ref, rows);
    } catch (error) {
      abort = String(error);
    } finally {
      IDBObjectStore.prototype.put = original;
    }
    const accepted = await c.append(ref, rows);
    const retry = await c.append(ref, rows);
    const failures = [];
    for (const run of [
      () => c.append(ref, [{ index: 0, solution: { ...rows[0].solution, buildSteps: ['mismatch'] } }]),
      () => c.append(accepted, [{ index: 2, solution: rows[0].solution }]),
      () =>
        c.append(accepted, [
          { index: 1, solution: rows[0].solution },
          { index: 1, solution: rows[0].solution },
        ]),
      () => c.read(accepted, -1, 1),
      () => c.page(accepted, 0, 129, []),
    ]) {
      try {
        await run();
        failures.push(null);
      } catch (error) {
        failures.push(String(error));
      }
    }
    return { abort, accepted, retry, failures, rows: await c.read(accepted, 0, 64) };
  });
  expect(result.abort).toContain('abort');
  expect(result.accepted.count).toBe(1);
  expect(result.retry.count).toBe(1);
  expect(result.failures.every(Boolean)).toBe(true);
  expect(result.rows.map((row) => row.index)).toEqual([0]);
});

test('quota failures retain one bounded batch for viewing and retry, but do not save a false durable prefix', async ({
  page,
}) => {
  const result = await page.evaluate(async () => {
    const c = window.store.collections;
    const ref = await c.create('quota');
    const rows = [{ index: 0, solution: window.example.result }];
    const original = IDBObjectStore.prototype.add;
    IDBObjectStore.prototype.add = function (...args) {
      if (this.name === 'collectionSolutions') throw new DOMException('Full storage', 'QuotaExceededError');
      return original.apply(this, args);
    };
    let appendError, saveError, retained, before, visible;
    try {
      try {
        await c.append(ref, rows);
      } catch (error) {
        appendError = String(error);
        retained = c.retain(ref, rows);
      }
      visible = await c.get(retained, 0);
      try {
        await window.store.apply([{ op: 'upsertEntry', entry: { ...window.example, collection: retained } }]);
      } catch (error) {
        saveError = String(error);
      }
      before = await window.store.load();
    } finally {
      IDBObjectStore.prototype.add = original;
    }
    await window.store.apply([{ op: 'upsertEntry', entry: { ...window.example, collection: retained } }]);
    return {
      appendError,
      saveError,
      before,
      visible,
      loaded: await window.store.load(),
      page: await c.read(retained, 0, 64),
    };
  });
  expect(result.appendError).toContain('QuotaExceededError');
  expect(result.saveError).toContain('QuotaExceededError');
  expect(result.before.entries).toEqual([]);
  expect(result.visible.status).toBe('best_known');
  expect(result.loaded.entries[0].collection).toMatchObject({ count: 1 });
  expect(result.loaded.entries[0].collection.pending).toBeUndefined();
  expect(result.page).toHaveLength(1);
});

test('reload recovers newer durable witnesses only for an interrupted checkpoint and does not invent proof', async ({
  page,
}) => {
  const result = await page.evaluate(async () => {
    const c = window.store.collections;
    let ref = await c.create('interrupted');
    ref = await c.append(ref, [{ index: 0, solution: window.example.result }]);
    const proof = { minimumNodeCount: null, minimumLinkCount: null };
    await window.store.apply([
      {
        op: 'upsertEntry',
        entry: { ...window.example, collection: ref, status: 'incomplete', proof, enumerationComplete: false },
      },
    ]);
    await c.append(ref, [{ index: 1, solution: window.example.result }]);
    const recovered = (await window.store.load()).entries[0];
    await window.store.apply([
      {
        op: 'upsertEntry',
        entry: { ...window.example, collection: ref, status: 'completed', proof, enumerationComplete: false },
      },
    ]);
    const sealedPrefix = (await window.store.load()).entries[0];
    return { recovered, sealedPrefix };
  });
  expect(result.recovered.collection.count).toBe(2);
  expect(result.recovered.proof).toEqual({ minimumNodeCount: null, minimumLinkCount: null });
  expect(result.recovered.enumerationComplete).toBe(false);
  expect(result.sealedPrefix.collection.count).toBe(1);
});

test('schema-one large histories migrate without copying their graphs or renumbering edits', async ({ page }) => {
  const result = await page.evaluate(async () => {
    const name = 'legacy-collection';
    const entry = { ...window.example, result: null, results: undefined, layouts: undefined, selectedSourceIndex: 90 };
    const db = await new Promise((resolve, reject) => {
      const operation = indexedDB.open(name, 1);
      operation.onupgradeneeded = () => {
        const db = operation.result;
        db.createObjectStore('meta').put(
          { schema: 1, revision: 0, order: [entry.id], selectedEntryId: entry.id },
          'document',
        );
        db.createObjectStore('entries', { keyPath: 'id' }).put(entry);
        const solutions = db.createObjectStore('solutions', { keyPath: ['entryId', 'index'] });
        for (let index = 0; index < 130; index++)
          solutions.add({ entryId: entry.id, index, value: window.example.result });
        db.createObjectStore('layouts', { keyPath: ['entryId', 'key'] }).put({
          entryId: entry.id,
          key: '90',
          value: { layoutKey: 'saved-90', nodes: [], edges: [] },
        });
      };
      operation.onsuccess = () => resolve(operation.result);
      operation.onerror = () => reject(operation.error);
    });
    db.close();
    const { createBrowserHistoryStore } =
      await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
    const store = createBrowserHistoryStore(name);
    try {
      const loaded = await store.load();
      const restored = loaded.entries[0];
      const page = await store.collections.page(restored.collection, 64, 64, []);
      await store.apply([{ op: 'upsertEntry', entry: { ...restored, title: 'Migrated' } }]);
      return { loaded, page, reread: await store.load() };
    } finally {
      store.close();
    }
  });
  expect(result.loaded.entries[0].results).toEqual([]);
  expect(result.loaded.entries[0].collection).toMatchObject({ count: 130, legacyEntryId: 'entry' });
  expect(result.loaded.entries[0].selectedSourceIndex).toBe(90);
  expect(result.loaded.entries[0].layouts['90'].layoutKey).toBe('saved-90');
  expect(result.page.rows.map((row) => row.sourceIndex)).toEqual(Array.from({ length: 64 }, (_, index) => index + 64));
  expect(result.reread.entries[0].title).toBe('Migrated');
  expect(result.reread.entries[0].collection.count).toBe(130);
});

test('deleting the final referencing history entry deletes only its collection', async ({ page }) => {
  const result = await page.evaluate(async () => {
    const c = window.store.collections;
    let ref = await c.create('shared-local-prefix');
    ref = await c.append(ref, [{ index: 0, solution: window.example.result }]);
    await window.store.apply(
      ['a', 'b'].map((id) => ({ op: 'upsertEntry', entry: { ...window.example, id, collection: ref } })),
    );
    await window.store.apply([{ op: 'deleteEntries', ids: ['a'] }]);
    const survives = await c.get(ref, 0);
    await window.store.apply([{ op: 'deleteEntries', ids: ['b'] }]);
    let missing;
    try {
      await c.get(ref, 0);
    } catch (error) {
      missing = String(error);
    }
    return { survives, missing, loaded: await window.store.load() };
  });
  expect(result.survives.status).toBe('best_known');
  expect(result.missing).toContain('incomplete or corrupt');
  expect(result.loaded.entries).toEqual([]);
});

test('missing graph and summary records fail explicitly rather than silently returning a short page', async ({
  page,
}) => {
  const result = await page.evaluate(async () => {
    const c = window.store.collections;
    let ref = await c.create('corrupt');
    ref = await c.append(
      ref,
      [0, 1, 2].map((index) => ({ index, solution: window.example.result })),
    );
    const db = await new Promise((resolve) => {
      const open = indexedDB.open('satisfactory-flow-synthetizer.history');
      open.onsuccess = () => resolve(open.result);
    });
    await new Promise((resolve, reject) => {
      const tx = db.transaction(['collectionSolutions', 'collectionSummaries'], 'readwrite');
      tx.objectStore('collectionSolutions').delete([ref.id, 1]);
      tx.objectStore('collectionSummaries').delete([ref.id, 1]);
      tx.oncomplete = resolve;
      tx.onabort = () => reject(tx.error);
    });
    db.close();
    const errors = [];
    for (const run of [() => c.read(ref, 0, 64), () => c.page(ref, 0, 64, [])]) {
      try {
        await run();
        errors.push(null);
      } catch (error) {
        errors.push(String(error));
      }
    }
    return errors;
  });
  expect(result[0]).toContain('incomplete or corrupt');
  expect(result[1]).toContain('Corrupt collection summary');
});
