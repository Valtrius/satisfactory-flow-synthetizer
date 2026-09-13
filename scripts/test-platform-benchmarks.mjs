import { test } from 'node:test';
import assert from 'node:assert/strict';
import { compareResults, summarize } from './platform-benchmark-report.mjs';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const result = (solutions = []) => ({
  status: 'completed',
  proof: { minimumNodeCount: 2, minimumLinkCount: 1 },
  enumeration_complete: true,
  result: { validation: { nodeCount: 2, linkCount: 1 } },
  solutions,
});
test('enumeration requires the full graph and exact flow objects, not just counts', () => {
  assert.equal(compareResults(result([{ flow: '1/3' }]), result([{ flow: '1/2' }]), 'all_min_n').comparable, false);
});
test('one optimum allows different witnesses but requires an equal proven objective', () => {
  assert.equal(compareResults(result(['a']), result(['b']), 'one_min_nl').comparable, true);
  const changed = result();
  changed.proof.minimumLinkCount = 2;
  assert.equal(compareResults(result(), changed, 'one_min_nl').comparable, false);
});
test('incomplete proofs never become speed ratios even when collections agree', () => {
  assert.equal(compareResults(result(), { ...result(), status: 'incomplete' }, 'all_min_nl').comparable, false);
  const schedule = [1, 2].flatMap((repeat) =>
    ['native_baseline', 'native_current', 'web_current'].map((variant) => ({
      case: 'x',
      mode: 'all_min_n',
      workers: 8,
      repeat,
      variant,
    })),
  );
  const rows = schedule.map((job) => ({
    job,
    verified: result(),
    raw: {
      wall_s: job.variant === 'native_baseline' ? 10 : 20,
      deadline_fired: job.repeat === 2 && job.variant === 'native_current',
    },
  }));
  const summary = summarize(rows, schedule);
  assert.equal(summary.comparisons[0].median_change_pct, null);
  assert.equal(summary.comparisons[0].regression_signal, false);
});

test('missing runs suppress the comparison rather than selecting only completed pairs', () => {
  const schedule = ['native_baseline', 'native_current'].map((variant) => ({
    case: 'x',
    mode: 'all_min_n',
    workers: 8,
    repeat: 1,
    variant,
  }));
  const summary = summarize([{ job: schedule[0], verified: result(), raw: { wall_s: 1 } }], schedule);
  assert.equal(summary.comparisons[0].median_change_pct, null);
  assert.equal(summary.comparisons[0].regression_signal, false);
});

test('CLI typos and repeated launches preserve existing evidence without starting a solver', () => {
  const root = mkdtempSync(join(tmpdir(), 'sfs-platform-guard-'));
  try {
    const runner = fileURLToPath(new URL('./run-platform-benchmarks.mjs', import.meta.url));
    const status = join(root, 'BENCHMARK-STATUS.txt');
    writeFileSync(status, 'FINISHED: original evidence');
    const backend = join(root, 'browser-placeholder');
    writeFileSync(backend, 'fixture');
    writeFileSync(
      join(root, 'metadata.json'),
      JSON.stringify({
        browser_executable: backend,
        browser_sha256: createHash('sha256').update('fixture').digest('hex'),
      }),
    );
    writeFileSync(join(root, 'frozen-hashes.json'), '{}');
    writeFileSync(join(root, 'schedule.json'), '[]');
    mkdirSync(join(root, 'results'));
    for (const args of [[], ['--typo', root], ['--run', root]]) {
      assert.throws(() =>
        execFileSync(process.execPath, [runner, ...args], { cwd: root, windowsHide: true, stdio: 'pipe' }),
      );
      assert.equal(readFileSync(status, 'utf8'), 'FINISHED: original evidence');
    }
  } finally {
    assert.ok(resolve(root).startsWith(resolve(tmpdir()) + sep));
    rmSync(root, { recursive: true });
  }
});
