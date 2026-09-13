import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { createWriteStream } from 'node:fs';
import { resolve, extname, sep } from 'node:path';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { chromium } from 'playwright-core';
import { summarize, markdown } from './platform-benchmark-report.mjs';
import { isDeepStrictEqual } from 'node:util';
import { pathToFileURL } from 'node:url';

const root = resolve(process.argv[3] ?? '.');
const json = async (path) => JSON.parse(await readFile(path, 'utf8'));
const save = async (path, value) => writeFile(path, JSON.stringify(value, null, 2));
let ownsRun = false;

async function child(exe, args, { seconds = 60, input, log, env = process.env } = {}) {
  const p = spawn(exe, args, { cwd: root, windowsHide: true, env, stdio: ['pipe', 'pipe', 'pipe'] });
  const out = [],
    err = [];
  const logFile = log ? createWriteStream(log) : null;
  p.stdout.on('data', (b) => {
    if (logFile) logFile.write(b);
    else out.push(b);
  });
  p.stderr.on('data', (b) => {
    if (logFile) logFile.write(b);
    else err.push(b);
  });
  p.stdin.on('error', () => {});
  p.stdin.end(input);
  let expired = false;
  const timer = setTimeout(() => {
    expired = true;
    // Only this owned child and its descendants are terminated.
    const killer = spawn('taskkill.exe', ['/PID', String(p.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' });
    killer.on('error', () => p.kill());
  }, seconds * 1000);
  try {
    const code = await new Promise((resolve, reject) => {
      p.once('error', reject);
      p.once('close', resolve);
    });
    if (expired || code !== 0)
      throw new Error(
        `${expired ? 'Watchdog expired' : `Exit ${code}`}: ${exe}; ${Buffer.concat(err).toString().slice(-2000)}`,
      );
    return Buffer.concat(out).toString();
  } finally {
    clearTimeout(timer);
    if (logFile) await new Promise((resolve) => logFile.end(resolve));
  }
}

async function webJob() {
  const job = await json(process.argv[4]);
  const metadata = await json(resolve(root, 'metadata.json'));
  const browser = await chromium.launch({ executablePath: metadata.browser_executable, headless: true });
  try {
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    await page.goto(job.url);
    await page.waitForFunction(() => window.benchmarkReady === true);
    const endpoints = (rates, prefix) =>
      rates.map((rate, i) => ({ id: `${prefix}-${i}`, name: `${prefix} ${i}`, rate }));
    const problem = job.problem;
    const request = {
      inputs: endpoints(problem.inputs, 'input'),
      outputs: endpoints(problem.outputs, 'output'),
      beltRate: problem.maxLinkRate,
      solveMode: job.mode,
    };
    const raw = await page.evaluate(
      ({ request, job }) => window.runBenchmark(request, job.workers, job.max_nodes, job.seconds),
      { request, job },
    );
    if (errors.length) throw new Error(errors.join('\n'));
    if (raw.cross_origin_isolated || raw.hardware_concurrency < job.workers)
      throw new Error('Unexpected browser execution environment');
    await save(resolve(root, 'results', `${job.id}.raw.json`), {
      name: job.case,
      problem,
      browser_version: browser.version(),
      ...raw,
    });
  } finally {
    await browser.close();
  }
}

async function verifyFrozenArtifacts() {
  const hashes = await json(resolve(root, 'frozen-hashes.json'));
  for (const [path, expected] of Object.entries(hashes)) {
    const actual = createHash('sha256')
      .update(await readFile(resolve(root, path)))
      .digest('hex');
    if (actual !== expected) throw new Error(`Frozen artifact changed: ${path}`);
  }
  return Object.keys(hashes).length;
}

async function verifyResults() {
  const count = await verifyFrozenArtifacts();
  const schedule = await json(resolve(root, 'schedule.json'));
  const rows = [];
  for (const job of schedule) {
    let row;
    try {
      row = await json(resolve(root, 'results', `${job.id}.record.json`));
    } catch (error) {
      if (error.code === 'ENOENT') continue;
      throw error;
    }
    if (!isDeepStrictEqual(row.job, job)) throw new Error(`Changed job settings: ${job.id}`);
    if (!row.error) {
      const raw = await json(resolve(root, 'results', `${job.id}.raw.json`));
      if (!isDeepStrictEqual(row.raw, raw)) throw new Error(`Changed raw result: ${job.id}`);
      const verified = JSON.parse(
        await child(resolve(root, 'bin/verify_browser_jobs.exe'), [], {
          seconds: 30,
          input: JSON.stringify([raw]),
        }),
      )[0];
      if (!isDeepStrictEqual(verified, row.verified)) throw new Error(`Changed verification: ${job.id}`);
    }
    rows.push(row);
  }
  // Completed campaigns keep their original comparison policy and reports.
  const frozen = await import(pathToFileURL(resolve(root, 'platform-benchmark-report.mjs')).href);
  const summary = frozen.summarize(rows, schedule);
  if (!isDeepStrictEqual(summary, await json(resolve(root, 'summary.json'))))
    throw new Error('Frozen summary does not reproduce');
  console.log(
    JSON.stringify(
      {
        frozen_hashes_verified: count,
        records_checked: rows.length,
        planned: summary.planned,
        completed: summary.completed,
        failures: summary.failures.length,
        mismatches: summary.mismatches.length,
      },
      null,
      2,
    ),
  );
  if (summary.failures.length || summary.mismatches.length) process.exitCode = 1;
}

async function run() {
  const metadata = await json(resolve(root, 'metadata.json'));
  const schedule = await json(resolve(root, 'schedule.json'));
  await verifyFrozenArtifacts();
  const browserHash = createHash('sha256')
    .update(await readFile(metadata.browser_executable))
    .digest('hex');
  if (browserHash !== metadata.browser_sha256) throw new Error('Installed Chrome changed after preparation');
  // Creating results claims an unused suite. Never overwrite a previous run.
  await mkdir(resolve(root, 'results'));
  ownsRun = true;
  const status = (s) =>
    writeFile(resolve(root, 'BENCHMARK-STATUS.txt'), `${s}\nUpdated: ${new Date().toISOString()}\n`);
  const webRoot = resolve(root, 'web');
  const prefix = '/satisfactory-flow-synthetizer/';
  const server = createServer(async (req, res) => {
    try {
      const path = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
      if (!path.startsWith(prefix)) {
        res.writeHead(404).end();
        return;
      }
      const file = resolve(webRoot, path.slice(prefix.length) || 'index.html');
      if (!file.startsWith(webRoot + sep)) {
        res.writeHead(403).end();
        return;
      }
      const body = await readFile(file);
      const mime = {
        '.html': 'text/html',
        '.js': 'text/javascript',
        '.mjs': 'text/javascript',
        '.wasm': 'application/wasm',
        '.json': 'application/json',
      };
      res
        .writeHead(200, {
          'Content-Type': mime[extname(file)] || 'application/octet-stream',
          'Cache-Control': 'no-store',
        })
        .end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const url = `http://127.0.0.1:${server.address().port}${prefix}`;
  const started = performance.now();
  const rows = [];
  const env = { ...process.env, SOLVER_CVC5: resolve(root, 'bin/cvc5.exe') };
  for (const key of Object.keys(env)) if (key.startsWith('SOLVER_') && key !== 'SOLVER_CVC5') delete env[key];
  const report = async () => {
    const summary = summarize(rows, schedule);
    await save(resolve(root, 'summary.json'), summary);
    await writeFile(resolve(root, 'REPORT.md'), markdown(summary));
    return summary;
  };
  try {
    for (let index = 0; index < schedule.length; index++) {
      const job = schedule[index];
      if (index % 3 === 0) {
        const remaining = metadata.suite.session_seconds - (performance.now() - started) / 1000;
        const tripletBudget =
          3 * (job.seconds + metadata.suite.cleanup_seconds + 30) + metadata.suite.verification_reserve_seconds;
        if (remaining < tripletBudget) {
          await save(resolve(root, 'remaining-jobs.json'), schedule.slice(index));
          break;
        }
      }
      await status(`RUNNING ${index + 1}/${schedule.length}: ${job.id}\nCap: ${job.seconds}s; workers: ${job.workers}`);
      const row = { job, started_at: new Date().toISOString() };
      const rawFile = resolve(root, 'results', `${job.id}.raw.json`);
      try {
        const processStart = performance.now();
        if (job.variant === 'web_current') {
          const file = resolve(root, 'results', `${job.id}.job.json`);
          await save(file, { ...job, url });
          await child(process.execPath, [resolve(root, 'run-platform-benchmarks.mjs'), '--web-job', root, file], {
            seconds: job.seconds + metadata.suite.cleanup_seconds,
            log: resolve(root, 'results', `${job.id}.log`),
            env,
          });
        } else {
          await child(
            resolve(root, 'bin', `${job.variant}.exe`),
            [
              String(job.seconds),
              String(job.workers),
              String(job.max_nodes),
              rawFile,
              job.mode,
              resolve(root, 'cases', `${job.case}.json`),
            ],
            {
              seconds: job.seconds + metadata.suite.cleanup_seconds,
              log: resolve(root, 'results', `${job.id}.log`),
              env,
            },
          );
        }
        row.process_wall_s = (performance.now() - processStart) / 1000;
        row.raw = await json(rawFile);
        if (
          job.variant !== 'web_current' &&
          (row.raw.workers !== job.workers ||
            row.raw.mode !== job.mode ||
            row.raw.max_nodes !== job.max_nodes ||
            row.raw.timeout_s !== job.seconds ||
            row.raw.diagnostics_enabled ||
            row.raw.comparison_protocol !== 'layout-v1')
        )
          throw new Error('Native runner settings mismatch');
        const verified = JSON.parse(
          await child(resolve(root, 'bin/verify_browser_jobs.exe'), [], {
            seconds: 30,
            input: JSON.stringify([row.raw]),
            env,
          }),
        );
        row.verified = verified[0];
        if (!['completed', 'incomplete', 'cancelled', 'unsat'].includes(row.verified.status))
          throw new Error(`Job failed: ${row.raw.snapshot?.error ?? row.verified.status}`);
      } catch (error) {
        row.error = String(error);
      }
      rows.push(row);
      await save(resolve(root, 'results', `${job.id}.record.json`), row);
      const summary = await report();
      if (row.error || summary.mismatches.length)
        throw new Error('Run or exact comparison failed; remaining timings were not launched');
      if (
        index === 2 &&
        (summary.failures.length || summary.mismatches.length || rows.some((r) => r.verified?.status !== 'completed'))
      )
        throw new Error('Initial qualification triple failed; remaining timings were not launched');
    }
    const summary = await report();
    const headline =
      summary.failures.length || summary.mismatches.length
        ? 'FINISHED WITH FAILURES'
        : rows.length < schedule.length
          ? 'STOPPED AT SESSION BUDGET'
          : 'FINISHED';
    await status(
      `${headline}: ${rows.length}/${schedule.length} attempted; ${summary.completed} complete.\nReport: ${resolve(root, 'REPORT.md')}\nCapped runs remain incomplete. ${metadata.suite.repeats} repetitions per configuration.`,
    );
    await writeFile(resolve(root, 'BENCHMARK-FINISHED.txt'), await readFile(resolve(root, 'BENCHMARK-STATUS.txt')));
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}

if (process.argv[2] === '--watchdog-test') {
  const log = resolve(root, 'target/platform-watchdog-test.log');
  let caught = false;
  try {
    await child(
      process.execPath,
      [
        '-e',
        "const p=require('node:child_process').spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore',windowsHide:true}); console.log(p.pid); setInterval(()=>{},1000)",
      ],
      { seconds: 1, log },
    );
  } catch (error) {
    caught = String(error).includes('Watchdog expired');
  }
  if (!caught) throw new Error('Watchdog did not fire');
  const descendant = Number((await readFile(log, 'utf8')).trim());
  let alive = false;
  try {
    process.kill(descendant, 0);
    alive = true;
  } catch (error) {
    if (error.code !== 'ESRCH') throw error;
  }
  if (alive) throw new Error('Owned descendant survived watchdog cleanup');
  console.log('Watchdog terminated the synthetic child and its descendant. No solver was run.');
} else if (process.argv[2] === '--web-job') await webJob();
else if (process.argv[2] === '--preflight') {
  const browser = await chromium.launch({ executablePath: process.argv[4], headless: true });
  try {
    console.log(
      JSON.stringify({
        version: browser.version(),
        hardware_concurrency: await (await browser.newPage()).evaluate(() => navigator.hardwareConcurrency),
      }),
    );
  } finally {
    await browser.close();
  }
} else if (process.argv[2] === '--verify' && process.argv[3]) {
  await verifyResults();
} else if (process.argv[2] === '--run' && process.argv[3]) {
  try {
    await run();
  } catch (error) {
    if (ownsRun) await writeFile(resolve(root, 'BENCHMARK-STATUS.txt'), `FAILED: ${error.stack}\n`);
    else console.error(String(error));
    process.exitCode = 1;
  }
} else {
  console.error(
    'Usage: node scripts/run-platform-benchmarks.mjs --verify PREPARED_DIRECTORY\nLaunch timing with scripts/start-platform-benchmarks.ps1.',
  );
  process.exitCode = 1;
}
