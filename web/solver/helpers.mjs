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
      const jobs = createBrowserJobs(options);
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
        return { snapshot, live };
      } finally {
        watch?.close();
        await jobs.shutdown();
        await jobs.release(id);
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
        const tx = db.transaction(['entries', 'solutions']);
        const entries = tx.objectStore('entries').getAll();
        const solutions = tx.objectStore('solutions').getAll();
        tx.oncomplete = () => resolve({ entries: entries.result, solutions: solutions.result });
        tx.onerror = () => reject(tx.error);
      });
    } finally {
      db.close();
    }
  });
}
