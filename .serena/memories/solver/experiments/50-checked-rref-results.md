# 50 results: checked small-rational RREF

Verified 2026-09-03. User requested permanent commit of retained49, then analysis of this benchmark. Experiment49 is now committed as 4d711b1899ebca8cefe1b87cfb060596257d1049, perf(custom): omit derived inequalities from canonical keys. No push.

## Verification

Run target/parallelism-ladder/checked-rref-screen-20260903 launched08:53:29 and finished09:20:58 Paris, about27m29. All56 scheduled jobs present,52 optimal and4 expected stress caps, no failures. Frozen analyzer rerun is byte-identical to original summary. All200 run hashes verified. Full public solution objects, layout-key sets, preferred witness and outcome/proof identities match for every completed problem/scope. All15 audited structural counters match. Evidence results/summary-rechecked.json and checked-rref-audit.json, logs target/exp50-reverification.log and target/exp50-audit.log. Before profile_case16737f36ff769659ed2766b3d9788f9bc9e203d5680feda2d9b9a8d2c4a799e3; after41e8b65477e5a453d0a12bd9998960554295450928d020bdb058f219d567575a.

## Measured hotspot-OFF results

Paired median after/before wall deltas, with exploratory percentile-bootstrap95 intervals:

- 115 all,16workers CCD96,5pairs: -2.4815%, interval[-2.7829,-0.6505]. CPU -1.8839%. All5 wall/CPU pairs improve. Separate absolute medians43.2402s to42.2858s.
- 238 optimal,16CCD32,5pairs: -0.2203%, interval[-1.4938,+6.9090]. CPU +0.8329%, all5 CPU pairs worse. Separate medians6.5164s to6.5021s.
- 258 minimum_links,16CCD96,2pairs: -0.2075%, individual -0.5755% and+0.1604%. CPU -1.0524%. Separate medians172.8631s to172.5014s. Two pairs do not establish a reliable small wall gain.
- 36 optimal,32unrestricted,5pairs: +2.8015%, interval[-8.4388,+5.4806]. CPU +8.4861%, all5 CPU pairs worse and4/5 wall pairs worse. One -8.4388% wall pair remains in the data. Separate medians11.3946s to11.6162s.
  10/17 substantive wall pairs improve;7/17 CPU pairs improve. Do not conflate ratios of displayed medians with paired medians. Tiny and mixed-huge controls are about9ms with coarse/zero CPU samples, unsuitable for performance claims. Mixed-huge median wall +0.5709% is recorded but not decisive. Four capped diagnostic runs provide no completion-speed evidence.

## Diagnostic explanation and limits

No fallback on the ordinary completed115/238 profiles:715834 and108909 attempts, all successes. The mixed-huge diagnostic exercises60 attempts, all BigInt fallbacks. Its48us total small attempt includes43.5us conversion, correctness verified; this tiny case does not bound large intermediate-overflow cost.
115 instrumented RREF36.7260s to26.4346s;238 5.9384s to5.0898s. Conversion consumes14.4532/24.9919s =57.8% of small attempt time on115, and3.0725/4.8284s =63.6% on238. These are nested diagnostic accumulators across workers, not CPU shares or ordinary solve-time savings. Conversion is a candidate for redesign, not a proven cause of every wall regression. No case/affinity-based production switch justified.

## Decision and source state

Do not promote this unconditional checked64 implementation. Restored production source to committed49, verified against every frozen50-before source file. Preserve candidate as benchmarks/custom/variants/checked-rref.patch plus manifest and mixed-huge fixture. git apply --check succeeds against4d711b1. Existing detached sf50-checked source/target and frozen executables remain intact. Research artifacts and this decision are recorded separately from production commit49, in docs(custom): preserve checked-rref benchmark results. The candidate is absent from production source.

Candidate's final227/217 release tests,2ignored each, both strict Clippy configurations and16 CLI parity checks remain valid for the preserved candidate. Restored production is byte-equivalent after line-ending normalization to the already tested224/214-test49 baseline. No new solver behavior introduced during restoration; no redundant full benchmark or build launched.

Next work remains authorized by the original apply-all request. Revisit arithmetic only with a distinct conversion-aware/direct-row design or fraction-free algorithm, not this same candidate. Pruning-order instrumentation and deferred-cache-compatible tail scheduling remain unimplemented. At this user checkpoint no run is active.

Report audited and shared: https://copyparty.jakez.eu/agent-files/qy9J9274k.html?k=RFkE0tsrxi2_DdUd . No new benchmark or push. Benchmark artifacts plus Serena handoff are committed separately from permanent source49.
