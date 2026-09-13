import { createBrowserJobs } from '../../frontend/src/lib/platform/browserJobs';
import { createBrowserHistoryStore } from '../../frontend/src/lib/platform/browserHistory';
import type { JobSnapshot, SolveRequest } from '../../frontend/src/types';

// This entry is bundled only by the benchmark config, never by the application.
Object.assign(window, {
  benchmarkReady: true,
  async runBenchmark(request: SolveRequest, workers: number, maxNodes: number, seconds: number) {
    const history = createBrowserHistoryStore();
    const jobs = createBrowserJobs({ workerCount: workers, maxNodes, collections: history.collections });
    let first: number | null = null;
    let deadlineFired = false;
    let watch: Awaited<ReturnType<typeof jobs.watch>> | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const start = performance.now();
    const id = await jobs.create(request);
    try {
      const snapshot = await new Promise<JobSnapshot>((resolve, reject) => {
        timer = setTimeout(
          () => {
            deadlineFired = true;
            void jobs.cancel(id).catch(reject);
          },
          Math.max(0, seconds * 1000 - (performance.now() - start)),
        );
        void jobs
          .watch(
            id,
            (value) => {
              if (value.result && first === null) first = (performance.now() - start) / 1000;
              if (!['running', 'cancelling'].includes(value.status)) resolve(value);
            },
            () => reject(new Error('Browser job subscription failed')),
          )
          .then((value) => {
            watch = value;
          }, reject);
      });
      const wall = (performance.now() - start) / 1000;
      clearTimeout(timer);
      // Terminal snapshots are sealed after all workers have been terminated.
      // Collection materialization and cross-platform validation are outside timing.
      const solutions = [];
      for (let offset = 0; offset < (snapshot.collection?.count ?? 0); offset += 64)
        solutions.push(...(await history.collections.read(snapshot.collection!, offset, 64)).map((r) => r.solution));
      return {
        request,
        snapshot,
        solutions,
        wall_s: wall,
        first_valid_s: first,
        deadline_fired: deadlineFired,
        hardware_concurrency: navigator.hardwareConcurrency,
        cross_origin_isolated: crossOriginIsolated,
      };
    } finally {
      clearTimeout(timer);
      watch?.close();
      await jobs.shutdown();
      await jobs.release(id);
      history.close();
    }
  },
});
