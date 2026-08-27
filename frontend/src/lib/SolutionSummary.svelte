<script lang="ts">
  import type { Solution } from '../types';
  import { nodeCountLabel } from './searchStage';

  type Props = {
    solution: Solution;
    elapsedLabel: string;
  };

  let { solution, elapsedLabel }: Props = $props();

  const isOptimal = $derived(solution.status === 'proven_optimal');
  const isBestKnown = $derived(solution.status === 'best_known');
  const engineLabel = $derived(solution.engine === 'z3' ? 'Z3' : 'Custom');
  const statusLabel = $derived(
    isOptimal ? 'Proven optimal' : isBestKnown ? 'Best known' : solution.status
  );
</script>

<header class="flex flex-wrap items-center justify-between gap-3 border-b border-line px-5 py-3">
  <h2 class="m-0 text-lg font-bold tracking-tight" id="result-title">
    {nodeCountLabel(solution.stats.nodeCount)}
  </h2>
  <div
    title={isBestKnown ? 'Not proven optimal' : undefined}
    class={`flex items-center gap-2 whitespace-nowrap rounded-full border px-2.5 py-1.5 text-xs font-extrabold ${
      isOptimal
        ? 'border-[#4fc49a]/40 bg-[#20654b]/20 text-[#8bdeb8]'
        : 'border-[#8a6a3a]/50 bg-[#3a2a12]/35 text-[#e6c27a]'
    }`}
  >
    <span aria-hidden="true">{isOptimal ? '✓' : ''}</span>
    {statusLabel} · {engineLabel}
  </div>
</header>

<div
  class="grid grid-cols-2 gap-px border-b border-line bg-line md:grid-cols-3 xl:grid-cols-6"
>
  <div class="bg-[#0b1922] px-4.5 py-4">
    <span class="mb-2 block text-xs font-bold tracking-wider text-dim uppercase">Solve time</span>
    <strong class="block text-xl text-[#dfe9ed] tabular-nums">{elapsedLabel}</strong>
  </div>
  <div class="bg-[#0b1922] px-4.5 py-4">
    <span class="mb-2 block text-xs font-bold tracking-wider text-dim uppercase">Splitters</span>
    <strong class="block text-xl text-[#dfe9ed] tabular-nums">{solution.stats.splitters}</strong>
  </div>
  <div class="bg-[#0b1922] px-4.5 py-4">
    <span class="mb-2 block text-xs font-bold tracking-wider text-dim uppercase">Mergers</span>
    <strong class="block text-xl text-[#dfe9ed] tabular-nums">{solution.stats.mergers}</strong>
  </div>
  <div class="bg-[#0b1922] px-4.5 py-4">
    <span class="mb-2 block text-xs font-bold tracking-wider text-dim uppercase">Feedback loops</span>
    <strong class="block text-xl text-[#dfe9ed] tabular-nums">{solution.stats.feedbackLoops}</strong>
  </div>
  <div class="bg-[#0b1922] px-4.5 py-4">
    <span class="mb-2 block text-xs font-bold tracking-wider text-dim uppercase">
      Links (L)
    </span>
    <strong class="block text-xl text-[#dfe9ed] tabular-nums"
      >{solution.stats.linkCount ?? solution.stats.beltCount ?? '—'}</strong
    >
  </div>
  <div class="bg-[#0b1922] px-4.5 py-4">
    <span class="mb-2 block text-xs font-bold tracking-wider text-dim uppercase">Discard</span>
    <strong class="block text-xl text-[#dfe9ed] tabular-nums"
      >{solution.discardRate.exact}<small class="ml-1 text-xs text-dim">/min</small></strong
    >
  </div>
</div>
