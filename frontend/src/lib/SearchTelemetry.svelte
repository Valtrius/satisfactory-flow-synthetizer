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
  const work = $derived(searchView.work);
  const progressVisible = $derived(work != null && (busy || detailsVisible));
</script>

<div
  class={`flex flex-col items-start justify-between gap-4 px-6 py-3 @min-[700px]/telemetry:flex-row @min-[700px]/telemetry:items-center ${
    !progressVisible && (borderBottom || detailsVisible) ? 'border-line border-b' : ''
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

{#if progressVisible && work}
  <div class={`bg-[#0a151d] px-6 pt-0.5 pb-4 ${borderBottom || detailsVisible ? 'border-line border-b' : ''}`}>
    <div class="grid gap-3 @min-[560px]/telemetry:grid-cols-2 @min-[560px]/telemetry:gap-6">
      {#each [{ label: `Link groups · N = ${formatTelemetryNumber(work.nodeCount)}`, count: work.linkGroups, color: 'groups' }, { label: `Profiles · L = ${formatTelemetryNumber(work.linkCount)}`, count: work.profiles, color: 'profiles' }] as scope}
        {#if scope.count.total > 0}
          <div class="min-w-0">
            <div class="mb-1.5 flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5 text-xs">
              <span class="text-[#dfe9ed]">{scope.label}</span>
              <span class="text-muted shrink-0 tabular-nums">
                <strong class="font-semibold text-[#dfe9ed]">{formatTelemetryNumber(scope.count.completed)}</strong>
                / {formatTelemetryNumber(scope.count.total)} completed
              </span>
            </div>
            <progress
              class={`search-progress ${scope.color}`}
              aria-label={scope.label}
              aria-valuetext={`${scope.count.completed} of ${scope.count.total} completed`}
              value={scope.count.completed}
              max={scope.count.total}
            ></progress>
          </div>
        {/if}
      {/each}
    </div>
    <p
      class="text-dim m-0 mt-2 text-[0.68rem]"
      title="Each bar counts exhausted scopes in the displayed independent search. The first resets at a new N; the second resets at a new L. Finding an optimum can finish a search before all scopes are exhausted."
    >
      Work completed · time per group varies
    </p>
  </div>
{/if}

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
        {#if work}
          <div class="flex justify-between gap-3">
            <span>Partitions completed at L = {formatTelemetryNumber(work.linkCount)}</span>
            <strong class="text-right text-[#dfe9ed] tabular-nums">
              {formatTelemetryNumber(work.partitions.completed)} / {formatTelemetryNumber(work.partitions.total)}
            </strong>
          </div>
          <div class="flex justify-between gap-3">
            <span>{busy ? 'Active workers in this search' : 'Active workers at last update'}</span>
            <strong class="text-right text-[#dfe9ed] tabular-nums">
              {formatTelemetryNumber(work.activeWorkers)}
            </strong>
          </div>
        {/if}
        {#each searchView.custom as entry (entry.name)}
          <div class="flex justify-between gap-3">
            <span>{entry.label}</span>
            <strong class="text-right text-[#dfe9ed] tabular-nums">
              {diagnosticText(entry)}
            </strong>
          </div>
        {:else}
          {#if !work}<span>No diagnostics reported yet.</span>{/if}
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .search-progress {
    display: block;
    width: 100%;
    height: 0.375rem;
    overflow: hidden;
    appearance: none;
    border: 0;
    border-radius: 999px;
    background: #253a45;
    color: #71cc91;
  }

  .search-progress.profiles {
    color: #69b8d4;
  }

  .search-progress::-webkit-progress-bar {
    background: #253a45;
    border-radius: 999px;
  }

  .search-progress::-webkit-progress-value {
    background: currentColor;
    border-radius: 999px;
  }

  .search-progress::-moz-progress-bar {
    background: currentColor;
    border-radius: 999px;
  }
</style>
