<script lang="ts">
  import X from '@lucide/svelte/icons/x';
  import type { Solution } from '../types';
  import { formatTelemetryNumber } from './searchStage';
  import Panel from './ui/Panel.svelte';
  import Button from './ui/Button.svelte';

  let {
    solution,
    elapsedLabel,
    onclose,
  }: {
    solution: Solution;
    elapsedLabel?: string;
    onclose: () => void;
  } = $props();

  const devices = $derived(
    [
      { label: 'Splitters', value: solution.stats.splitters },
      { label: 'Mergers', value: solution.stats.mergers },
    ].filter((metric) => metric.value != null && metric.value > 0),
  );
  const links = $derived(solution.stats.linkCount ?? solution.stats.beltCount);
  const peakRate = $derived(solution.stats.internalMaxThroughput?.exact);
  const hasPeakRate = $derived(Boolean(peakRate && /[1-9]/.test(peakRate.split('/')[0])));
  const feedbacks = $derived(solution.stats.feedbackLoops);
  const discard = $derived(solution.discardRate?.exact);
  // Inspect the numerator so exact fractions remain exact, including very small rates.
  const hasDiscard = $derived(Boolean(discard && /[1-9]/.test(discard.split('/')[0])));
  const hasInfo = $derived(
    elapsedLabel ||
      solution.stats.nodeCount > 0 ||
      devices.length ||
      (links != null && links > 0) ||
      hasPeakRate ||
      feedbacks > 0 ||
      hasDiscard,
  );
</script>

{#snippet metric(label: string, value: string | number, unit = '')}
  <div class="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-3 px-3 py-1">
    <dt class="text-dim text-[0.65rem] font-bold tracking-wider whitespace-nowrap uppercase">{label}</dt>
    <dd class="text-ink m-0 text-right text-sm font-bold [overflow-wrap:anywhere] tabular-nums">
      {value}{#if unit}<small class="text-muted ml-1 text-xs font-normal">{unit}</small>{/if}
    </dd>
  </div>
{/snippet}

{#if hasInfo}
  <Panel element="aside" aria-label="Graph information" class="group relative w-48 max-w-full overflow-hidden">
    <Button
      variant="plain"
      class="text-muted hover:text-ink pointer-events-none absolute top-2 right-1 grid size-5 place-items-center rounded opacity-0 transition-opacity duration-150 group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100 motion-reduce:transition-none"
      title="Hide graph info"
      aria-label="Hide graph info"
      onclick={onclose}
    >
      <X size={14} aria-hidden="true" />
    </Button>
    <dl class="divide-line m-0 divide-y py-1">
      {#if elapsedLabel}{@render metric('Solve time', elapsedLabel)}{/if}
      {#if solution.stats.nodeCount > 0}{@render metric('Nodes', formatTelemetryNumber(solution.stats.nodeCount))}{/if}
      {#each devices as device}{@render metric(device.label, formatTelemetryNumber(device.value))}{/each}
      {#if links != null && links > 0}{@render metric('Links', formatTelemetryNumber(links))}{/if}
      {#if hasPeakRate && peakRate}{@render metric('Peak rate', peakRate, '/min')}{/if}
      {#if feedbacks > 0}{@render metric('Feedbacks', formatTelemetryNumber(feedbacks))}{/if}
      {#if hasDiscard && discard}{@render metric('Discard', discard, '/min')}{/if}
    </dl>
  </Panel>
{/if}
