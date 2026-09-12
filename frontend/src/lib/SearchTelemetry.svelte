<script lang="ts">
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import { diagnosticText, formatTelemetryNumber, type SearchStageView } from './searchStage';
  import Button from './ui/Button.svelte';

  type Props = {
    searchView: SearchStageView;
    muted: boolean;
    busy: boolean;
    elapsedLabel: string;
    headline: string;
    subline: string;
    sizeBody: string;
    foundCount: number;
    showFound: boolean;
    showDetails: boolean;
    borderBottom?: boolean;
    collapsible?: boolean;
    detailsExpanded?: boolean;
    onToggleDetails?: () => void;
  };

  let {
    searchView,
    muted,
    busy,
    elapsedLabel,
    headline,
    subline,
    sizeBody,
    foundCount,
    showFound,
    showDetails,
    borderBottom = false,
    collapsible = false,
    detailsExpanded = false,
    onToggleDetails,
  }: Props = $props();
  const detailsId = $props.id();
  const detailsVisible = $derived(showDetails && (!collapsible || detailsExpanded));
</script>

<div
  class={`flex flex-col items-start justify-between gap-4 px-6 py-3 @min-[700px]/telemetry:flex-row @min-[700px]/telemetry:items-center ${
    borderBottom || detailsVisible ? 'border-line border-b' : ''
  } bg-[#0a151d]`}
>
  <div class="flex items-center gap-3">
    {#if busy}
      <span
        class="border-t-accent size-6 shrink-0 animate-spin rounded-full border-2 border-[#314953] bg-transparent"
        aria-hidden="true"
      ></span>
    {:else if muted}
      <span class="bg-dim size-2.5 shrink-0 rounded-full" aria-hidden="true"></span>
    {:else}
      <span class="size-2.5 shrink-0 rounded-full bg-[#71cc91]" aria-hidden="true"></span>
    {/if}
    <div aria-live="polite">
      <h2 class="m-0 text-lg font-bold tracking-tight" id="search-stage-title">
        {headline}
      </h2>
      <p class="text-muted m-0 mt-1 text-xs">{subline}</p>
    </div>
  </div>
  <div class="flex shrink-0 flex-wrap items-center gap-3">
    {#if collapsible && showFound && !detailsVisible}
      <span class="text-muted text-sm whitespace-nowrap tabular-nums">
        <strong class="text-[#dfe9ed]">
          {formatTelemetryNumber(foundCount)}
        </strong>
        found
      </span>
    {/if}
    <span class="text-muted text-sm whitespace-nowrap tabular-nums">
      {elapsedLabel}
    </span>
    {#if collapsible && showDetails}
      <Button
        type="button"
        variant="quiet"
        size="small"
        class="flex items-center gap-1.5 whitespace-nowrap"
        aria-expanded={detailsVisible}
        aria-controls={detailsId}
        onclick={onToggleDetails}
      >
        {detailsVisible ? 'Hide telemetry' : 'Show telemetry'}
        <ChevronDown size={14} class={detailsVisible ? 'rotate-180' : ''} aria-hidden="true" />
      </Button>
    {/if}
  </div>
</div>

{#if detailsVisible}
  <div
    id={detailsId}
    class={`grid gap-5 p-4.5 @min-[700px]/telemetry:grid-cols-[minmax(0,1fr)_auto_minmax(0,1.1fr)] @min-[700px]/telemetry:items-start @min-[700px]/telemetry:gap-0 @min-[700px]/telemetry:p-5 ${
      borderBottom ? 'border-line border-b' : ''
    }`}
  >
    <div class="@min-[700px]/telemetry:pr-5">
      <h3 class="text-dim m-0 mb-3 text-[0.68rem] font-bold tracking-wider uppercase">Exact search</h3>
      <div class="flex flex-wrap gap-2">
        {#each [{ label: 'N bound', value: searchView.lowerBound }, { label: 'N current', value: searchView.nodeCount }, { label: 'L exact', value: searchView.linkCount }, { label: 'Best L', value: searchView.bestLinkCount }] as metric}
          <div class="min-w-18 rounded-lg border border-[#253a45] bg-[#050f15]/55 px-2.5 py-2">
            <strong class="block text-lg text-[#dfe9ed] tabular-nums">
              {formatTelemetryNumber(metric.value)}
            </strong>
            <span class="text-dim text-[0.7rem]">{metric.label}</span>
          </div>
        {/each}
        {#if showFound}
          <div class="min-w-18 rounded-lg border border-[#253a45] bg-[#050f15]/55 px-2.5 py-2">
            <strong class="block text-lg text-[#dfe9ed] tabular-nums">
              {formatTelemetryNumber(foundCount)}
            </strong>
            <span class="text-dim text-[0.7rem]">found</span>
          </div>
        {/if}
      </div>
      <p class="text-muted m-0 mt-3 text-xs">{sizeBody}</p>
    </div>

    <div class="bg-line hidden w-px self-stretch @min-[700px]/telemetry:block" aria-hidden="true"></div>
    <div
      class="border-line border-t pt-5 @min-[700px]/telemetry:border-t-0 @min-[700px]/telemetry:pt-0 @min-[700px]/telemetry:pl-5"
    >
      <h3 class="text-dim m-0 mb-3 text-[0.68rem] font-bold tracking-wider uppercase">Solver diagnostics</h3>
      <div class="text-muted grid gap-1.5 text-xs">
        {#each searchView.custom as entry (entry.name)}
          <div class="flex justify-between gap-3">
            <span>{entry.label}</span>
            <strong class="text-right text-[#dfe9ed] tabular-nums">
              {diagnosticText(entry)}
            </strong>
          </div>
        {:else}
          <span>No diagnostics reported yet.</span>
        {/each}
      </div>
    </div>
  </div>
{/if}
