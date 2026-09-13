import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

export function publicRequest(fixture) {
  const { problem, options } = fixture.request;
  const endpoints = (rates, prefix) =>
    rates.map((rate, index) => ({ id: `${prefix}-${index}`, name: `${prefix} ${index}`, rate }));
  return {
    inputs: endpoints(problem.inputs, 'input'),
    outputs: endpoints(problem.outputs, 'output'),
    beltRate: problem.maxLinkRate,
    solveMode: options.mode,
  };
}

export function verifyJobs(cases) {
  const binary = fileURLToPath(
    new URL(
      `../../target/debug/examples/verify_browser_jobs${process.platform === 'win32' ? '.exe' : ''}`,
      import.meta.url,
    ),
  );
  return JSON.parse(
    execFileSync(binary, [], {
      input: JSON.stringify(cases),
      encoding: 'utf8',
      maxBuffer: 32 * 1024 * 1024,
      windowsHide: true,
    }),
  );
}

export async function runJob(page, request, options = {}, cancelOnWitness = false) {
  return page.evaluate(
    async ({ request, options, cancelOnWitness }) => {
      const { createBrowserJobs } = await import('/satisfactory-flow-synthetizer/src/lib/platform/browserJobs.ts');
      const { createBrowserHistoryStore } =
        await import('/satisfactory-flow-synthetizer/src/lib/platform/browserHistory.ts');
      const history = createBrowserHistoryStore();
      const published = [];
      const collections = {
        ...history.collections,
        async append(ref, rows) {
          const result = await history.collections.append(ref, rows);
          published.push(...structuredClone(rows));
          return result;
        },
        retain(ref, rows) {
          const result = history.collections.retain(ref, rows);
          published.push(...structuredClone(rows));
          return result;
        },
      };
      const jobs = createBrowserJobs({ workerCount: 1, ...options, collections });
      const live = [];
      const id = await jobs.create(request);
      let watch;
      try {
        const snapshot = await new Promise(async (resolve, reject) => {
          const timer = setTimeout(() => {
            void jobs.shutdown();
            reject(new Error('Browser job test deadline'));
          }, 90_000);
          watch = await jobs.watch(
            id,
            (value) => {
              live.push(value);
              if (!['running', 'cancelling'].includes(value.status)) {
                clearTimeout(timer);
                resolve(value);
              } else if (cancelOnWitness && value.result) void jobs.cancel(id);
            },
            () => {
              clearTimeout(timer);
              reject(new Error('Browser job subscription failed'));
            },
          );
        });
        // Qualification deliberately materializes small fixtures. Production
        // get/watch retain only metadata and the preferred graph.
        const solutions = [];
        for (let offset = 0; offset < (snapshot.collection?.count ?? 0); offset += 64) {
          solutions.push(...(await collections.read(snapshot.collection, offset, 64)).map((row) => row.solution));
        }
        return { snapshot, live, solutions, published };
      } finally {
        watch?.close();
        await jobs.shutdown();
        await jobs.release(id);
        history.close();
      }
    },
    { request, options, cancelOnWitness },
  );
}

export async function storedEntries(page) {
  return page.evaluate(async () => {
    const db = await new Promise((resolve, reject) => {
      const open = indexedDB.open('satisfactory-flow-synthetizer.history');
      open.onsuccess = () => resolve(open.result);
      open.onerror = () => reject(open.error);
    });
    try {
      return await new Promise((resolve, reject) => {
        const tx = db.transaction(['entries', 'solutions', 'collectionSolutions']);
        const entries = tx.objectStore('entries').getAll();
        const solutions = tx.objectStore('solutions').getAll();
        const collected = tx.objectStore('collectionSolutions').getAll();
        tx.oncomplete = () =>
          resolve({
            entries: entries.result,
            solutions: [
              ...solutions.result,
              ...collected.result.filter((row) =>
                entries.result.some(
                  (entry) => entry.collection?.id === row.collectionId && row.index < entry.collection.count,
                ),
              ),
            ],
          });
        tx.onerror = () => reject(tx.error);
      });
    } finally {
      db.close();
    }
  });
}
