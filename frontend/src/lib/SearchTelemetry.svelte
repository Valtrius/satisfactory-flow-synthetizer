<script lang="ts">
  import { diagnosticProgress, diagnosticText, type SearchStageView } from './searchStage';

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
    borderBottom = false
  }: Props = $props();
  const profiles = $derived(diagnosticProgress(searchView.custom));
</script>

<div
  class={`flex flex-col items-start justify-between gap-4 px-6 py-3 md:flex-row md:items-center ${
    borderBottom || showDetails ? 'border-b border-line' : ''
  } bg-[#0a151d]`}
>
  <div class="flex items-center gap-3">
    {#if busy}
      <span
        class="size-6 shrink-0 animate-spin rounded-full border-2 border-[#314953] border-t-accent bg-transparent"
        aria-hidden="true"
      ></span>
    {:else if muted}
      <span class="size-2.5 shrink-0 rounded-full bg-dim" aria-hidden="true"></span>
    {:else}
      <span class="size-2.5 shrink-0 rounded-full bg-[#71cc91]" aria-hidden="true"></span>
    {/if}
    <div aria-live="polite">
      <h2 class="m-0 text-lg font-bold tracking-tight" id="search-stage-title">
        {headline}
      </h2>
      <p class="m-0 mt-1 text-xs text-muted">{subline}</p>
    </div>
  </div>
  <div class="whitespace-nowrap text-sm text-muted tabular-nums">
    {elapsedLabel}
  </div>
</div>

{#if showDetails}
  <div
    class={`grid gap-5 p-4.5 md:grid-cols-[minmax(0,1fr)_auto_minmax(0,1.1fr)] md:items-start md:gap-0 md:p-5 ${
      borderBottom ? 'border-b border-line' : ''
    }`}
  >
    <div class="md:pr-5">
      <h3 class="m-0 mb-3 text-[0.68rem] font-bold tracking-wider text-dim uppercase">
        Exact search
      </h3>
      <div class="flex flex-wrap gap-2">
        {#each [
          { label: 'N bound', value: searchView.lowerBound },
          { label: 'N current', value: searchView.nodeCount },
          { label: searchView.linkKind === 'at_most' ? 'L cap' : 'L exact', value: searchView.linkCount },
          { label: 'Best L', value: searchView.bestLinkCount }
        ] as metric}
          <div class="min-w-18 rounded-lg border border-[#253a45] bg-[#050f15]/55 px-2.5 py-2">
            <strong class="block text-lg text-[#dfe9ed] tabular-nums">{metric.value ?? '—'}</strong>
            <span class="text-[0.7rem] text-dim">{metric.label}</span>
          </div>
        {/each}
        {#if showFound}
          <div class="min-w-18 rounded-lg border border-[#253a45] bg-[#050f15]/55 px-2.5 py-2">
            <strong class="block text-lg text-[#dfe9ed] tabular-nums">{foundCount}</strong>
            <span class="text-[0.7rem] text-dim">found</span>
          </div>
        {/if}
      </div>
      <p class="m-0 mt-3 text-xs text-muted">{sizeBody}</p>
    </div>

    <div class="hidden w-px self-stretch bg-line md:block" aria-hidden="true"></div>
    <div class="border-t border-line pt-5 md:border-t-0 md:pt-0 md:pl-5">
      <h3 class="m-0 mb-3 text-[0.68rem] font-bold tracking-wider text-dim uppercase">Solver diagnostics</h3>
      {#if profiles}
        <div class="mb-3">
          <div class="mb-1.5 flex justify-between gap-3 text-xs text-muted">
            <span>{profiles.label}</span>
            <strong class="text-[#dfe9ed] tabular-nums">{profiles.closed} / {profiles.total}</strong>
          </div>
          <div
            role="progressbar"
            aria-label={profiles.label}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={profiles.percent}
            aria-valuetext={`${profiles.closed} of ${profiles.total} profiles`}
            class="h-1.5 overflow-hidden rounded-full bg-line"
          >
            <div class="h-full bg-accent" style:width={`${profiles.percent}%`}></div>
          </div>
        </div>
      {/if}
      <div class="grid gap-1.5 text-xs text-muted">
        {#each searchView.custom as entry (entry.name)}
          <div class="flex justify-between gap-3">
            <span>{entry.label}</span>
            <strong class="text-right text-[#dfe9ed] tabular-nums">{diagnosticText(entry)}</strong>
          </div>
        {:else}
          <span>No diagnostics reported yet.</span>
        {/each}
      </div>

    </div>
  </div>
{/if}
