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
    isOptimal ? 'Exact & optimal' : isBestKnown ? 'Best known · not proven optimal' : solution.status
  );
  const badgeLabel = $derived(
    isOptimal ? 'Verified optimal' : isBestKnown ? 'Validated witness' : 'Result'
  );
</script>

<div class="mb-5 flex flex-col items-start justify-between gap-4 md:flex-row md:items-center">
  <div class="flex flex-wrap items-center gap-3">
    <h2 class="m-0 text-3xl font-bold tracking-tighter md:text-4xl" id="result-title">
      {nodeCountLabel(solution.stats.nodeCount)}
    </h2>
    <span
      class="whitespace-nowrap rounded-full border border-[#4fc49a]/40 bg-[#20654b]/20 px-2.5 py-1.5 text-xs font-extrabold tracking-wider text-[#8bdeb8] uppercase"
    >
      {badgeLabel} · {engineLabel}
    </span>
  </div>
  <div
    class={`flex items-center gap-2 whitespace-nowrap rounded-full border px-3.5 py-2.5 text-sm font-extrabold ${
      isOptimal
        ? 'border-[#4fc49a]/40 bg-[#20654b]/20 text-[#8bdeb8]'
        : 'border-[#8a6a3a]/50 bg-[#3a2a12]/35 text-[#e6c27a]'
    }`}
  >
    <span>{isOptimal ? '✓' : '·'}</span>
    {statusLabel}
  </div>
</div>

<div
  class="mb-4 grid grid-cols-2 gap-px overflow-hidden rounded-tl-xl rounded-tr-sm rounded-br-xl rounded-bl-sm border border-line bg-line md:grid-cols-3 xl:grid-cols-6"
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
      {solution.engine === 'custom' ? 'Links (L)' : 'Belts'}
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

{#if solution.engine === 'custom' && (solution.stats.physicalLinkCount != null || solution.validation)}
  <div class="mb-4 grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line md:grid-cols-4">
    {#if solution.stats.physicalLinkCount != null}
      <div class="bg-[#0b1922] px-4 py-3">
        <span class="mb-1 block text-[0.65rem] font-bold tracking-wider text-dim uppercase"
          >Physical links</span
        >
        <strong class="text-[#dfe9ed] tabular-nums">{solution.stats.physicalLinkCount}</strong>
      </div>
    {/if}
    {#if solution.stats.discardLinkCount != null}
      <div class="bg-[#0b1922] px-4 py-3">
        <span class="mb-1 block text-[0.65rem] font-bold tracking-wider text-dim uppercase"
          >Discard links</span
        >
        <strong class="text-[#dfe9ed] tabular-nums">{solution.stats.discardLinkCount}</strong>
      </div>
    {/if}
    {#if solution.stats.checkedThrough != null}
      <div class="bg-[#0b1922] px-4 py-3">
        <span class="mb-1 block text-[0.65rem] font-bold tracking-wider text-dim uppercase"
          >Checked through</span
        >
        <strong class="text-[#dfe9ed] tabular-nums">N≤{solution.stats.checkedThrough}</strong>
      </div>
    {/if}
    {#if solution.validation}
      <div class="bg-[#0b1922] px-4 py-3">
        <span class="mb-1 block text-[0.65rem] font-bold tracking-wider text-dim uppercase"
          >Validator</span
        >
        <strong class="text-[#dfe9ed] tabular-nums">v{solution.validation.validatorVersion}</strong>
      </div>
    {/if}
  </div>
{/if}
