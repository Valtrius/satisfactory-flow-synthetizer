import { isDeepStrictEqual } from 'node:util';

export function compareResults(a, b, mode) {
  if (!a || !b) return { comparable: false, errors: ['Missing validated result'] };
  if (a.status !== 'completed' || b.status !== 'completed') return { comparable: false, errors: [] };
  const errors = [];
  if (!isDeepStrictEqual(a.proof, b.proof)) errors.push('Optimum proof differs');
  const objective = (v) => [v.result?.validation.nodeCount, v.result?.validation.linkCount];
  if (!isDeepStrictEqual(objective(a), objective(b))) errors.push('Validated optimum differs');
  if (a.enumeration_complete !== b.enumeration_complete) errors.push('Enumeration proof differs');
  if (mode !== 'one_min_nl' && !isDeepStrictEqual(a.solutions, b.solutions))
    errors.push('Full canonical graphs or exact flows differ');
  return { comparable: errors.length === 0, errors };
}

const median = (values) => {
  const sorted = values.toSorted((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};

export function summarize(rows, schedule) {
  const comparisons = [];
  const groups = Map.groupBy(schedule, (j) => `${j.case}/${j.mode}/w${j.workers}`);
  for (const [scope, jobs] of groups) {
    const repeats = [...new Set(jobs.map((j) => j.repeat))];
    for (const [reference, candidate] of [
      ['native_baseline', 'native_current'],
      ['native_current', 'web_current'],
    ]) {
      const pairs = repeats.map((repeat) => {
        const find = (variant) =>
          rows.find(
            (r) =>
              r.job.case === jobs[0].case &&
              r.job.mode === jobs[0].mode &&
              r.job.workers === jobs[0].workers &&
              r.job.repeat === repeat &&
              r.job.variant === variant,
          );
        const a = find(reference),
          b = find(candidate);
        const checked = compareResults(a?.verified, b?.verified, jobs[0].mode);
        const timed = checked.comparable && !a.error && !b.error && !a.raw.deadline_fired && !b.raw.deadline_fired;
        return {
          repeat,
          comparable: timed,
          errors: checked.errors,
          reference_status: a?.verified?.status ?? (a?.error || 'missing'),
          candidate_status: b?.verified?.status ?? (b?.error || 'missing'),
          reference_s: a?.raw?.wall_s,
          candidate_s: b?.raw?.wall_s,
          ratio: timed ? b.raw.wall_s / a.raw.wall_s : null,
        };
      });
      const all = pairs.every((p) => p.comparable);
      const ratio = all ? median(pairs.map((p) => p.ratio)) : null;
      comparisons.push({
        scope,
        reference,
        candidate,
        pairs,
        all_comparable: all,
        reference_median_s: all ? median(pairs.map((p) => p.reference_s)) : null,
        candidate_median_s: all ? median(pairs.map((p) => p.candidate_s)) : null,
        median_change_pct: all ? 100 * (ratio - 1) : null,
        regression_signal:
          all &&
          pairs.length >= 2 &&
          reference === 'native_baseline' &&
          pairs.every((p) => p.ratio > 1.1 && p.candidate_s - p.reference_s > 0.5),
      });
    }
  }
  const failures = rows.filter((r) => r.error);
  const mismatches = comparisons.flatMap((c) =>
    c.pairs
      .filter((p) => p.errors.length && p.reference_status !== 'missing' && p.candidate_status !== 'missing')
      .map((p) => ({ scope: c.scope, ...p })),
  );
  return {
    planned: schedule.length,
    attempted: rows.length,
    completed: rows.filter((r) => r.verified?.status === 'completed' && !r.error).length,
    failures: failures.map((r) => ({ id: r.job.id, error: r.error })),
    mismatches,
    comparisons,
  };
}

export function markdown(summary) {
  const lines = [
    '# Native and browser solver benchmark',
    '',
    `${summary.attempted}/${summary.planned} runs attempted; ${summary.completed} completed. ${summary.failures.length} run failures; ${summary.mismatches.length} comparison failures.`,
    '',
    'Fresh release builds compare the selected native reference revision with the current native production solve API and the bundled production browser job/worker implementation; metadata.json identifies the exact revisions. Native timings include backend startup and cleanup. Browser timings include worker/module startup, accepted collection writes to IndexedDB and worker termination. Browser executable launch, page navigation, graph rendering, full history metadata persistence, and post-run canonical comparison are outside timing. Tauri IPC/UI and SQLite history are outside the native measurement.',
    '',
    'Each run uses a fresh browser context or native process. There is no warm-up solve; local asset fetch and module initialization are included in browser solve time. Both native variants use the same frozen cvc5 executable. Browser cvc5 uses the repository-pinned Wasm build. No solver jobs overlap. CPU affinity and power policy are recorded, not changed.',
    '',
    'Repetitions use reversed order. A regression signal requires at least two native pairs, all exceeding +10% and +0.5 s. A cap is censored; no ratio is reported for a group with missing, failed, capped or unequal results. One min N/L permits different validated witnesses at the same proven optimum; enumeration compares full canonical graphs and exact flows. Changes are medians of within-repeat ratios, which may differ from ratios of the displayed time medians.',
    '',
    '| Scope | Comparison | Reference median | Candidate median | Change |',
    '| --- | --- | ---: | ---: | ---: |',
  ];
  for (const c of summary.comparisons)
    lines.push(
      `| ${c.scope} | ${c.reference} → ${c.candidate} | ${c.all_comparable ? c.reference_median_s.toFixed(3) + ' s' : '—'} | ${c.all_comparable ? c.candidate_median_s.toFixed(3) + ' s' : '—'} | ${c.all_comparable ? c.median_change_pct.toFixed(1) + '%' + (c.regression_signal ? ' (regression signal)' : '') : 'inconclusive; see pair records'} |`,
    );
  return lines.join('\n') + '\n';
}
