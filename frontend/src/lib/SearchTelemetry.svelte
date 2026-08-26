<script lang="ts">
  import type { SearchStageView } from './searchStage';

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

  const isCustom = $derived(searchView.engine === 'custom');
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
        {isCustom ? 'Exact search' : 'Size search'}
      </h3>
      <div class="flex flex-wrap gap-2">
        {#if isCustom}
          <div class="min-w-18 rounded-lg border border-[#253a45] bg-[#050f15]/55 px-2.5 py-2">
            <strong class="block text-lg text-[#dfe9ed] tabular-nums"
              >{searchView.nodeCount ?? '—'}</strong
            >
            <span class="text-[0.7rem] text-dim">N</span>
          </div>
          <div class="min-w-18 rounded-lg border border-[#253a45] bg-[#050f15]/55 px-2.5 py-2">
            <strong class="block text-lg text-[#dfe9ed] tabular-nums"
              >{searchView.linkCount ?? '—'}</strong
            >
            <span class="text-[0.7rem] text-dim">L</span>
          </div>
          <div class="min-w-18 rounded-lg border border-[#253a45] bg-[#050f15]/55 px-2.5 py-2">
            <strong class="block text-lg text-[#dfe9ed] tabular-nums"
              >{searchView.profilesCompleted}</strong
            >
            <span class="text-[0.7rem] text-dim">profiles done</span>
          </div>
        {:else}
          <div class="min-w-18 rounded-lg border border-[#253a45] bg-[#050f15]/55 px-2.5 py-2">
            <strong class="block text-lg text-[#dfe9ed] tabular-nums"
              >{searchView.lowerBound ?? '—'}</strong
            >
            <span class="text-[0.7rem] text-dim">bound</span>
          </div>
          <div class="min-w-18 rounded-lg border border-[#253a45] bg-[#050f15]/55 px-2.5 py-2">
            <strong class="block text-lg text-[#dfe9ed] tabular-nums"
              >{searchView.nodeCount ?? '—'}</strong
            >
            <span class="text-[0.7rem] text-dim">current</span>
          </div>
          <div class="min-w-18 rounded-lg border border-[#253a45] bg-[#050f15]/55 px-2.5 py-2">
            <strong class="block text-lg text-[#dfe9ed] tabular-nums">{searchView.ruledOut}</strong>
            <span class="text-[0.7rem] text-dim">ruled out</span>
          </div>
        {/if}
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
      {#if isCustom}
        <h3 class="m-0 mb-3 text-[0.68rem] font-bold tracking-wider text-dim uppercase">
          Custom instrumentation
        </h3>
        <div class="mb-1.5 flex items-center justify-between gap-3 text-xs text-muted">
          <span>Profiles closed</span>
          <strong class="text-[#dfe9ed] tabular-nums">
            {#if searchView.profilesTotal == null}
              {searchView.profilesCompleted}
            {:else}
              {searchView.profilesCompleted} / {searchView.profilesTotal}
            {/if}
          </strong>
        </div>
        <div
          class="mb-3 h-2 overflow-hidden rounded-full bg-[#152530]"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={searchView.portfolioPct}
          aria-label="Profiles closed"
        >
          <div
            class={`h-full ${muted ? 'bg-[#3d5560]' : 'bg-linear-to-r from-[#2a8f6a] to-flow'}`}
            style={`width: ${searchView.portfolioPct}%`}
          ></div>
        </div>
        {#if searchView.instrumentation}
          <div class="grid gap-1.5 text-xs text-muted">
            <div class="flex justify-between gap-3">
              <span>Capacity prunes</span>
              <strong class="text-[#dfe9ed] tabular-nums"
                >{searchView.instrumentation.capacityPrunes}</strong
              >
            </div>
            <div class="flex justify-between gap-3">
              <span>Lower-bound prunes</span>
              <strong class="text-[#dfe9ed] tabular-nums"
                >{searchView.instrumentation.lowerBoundPrunes}</strong
              >
            </div>
            <div class="flex justify-between gap-3">
              <span>Canonical retained</span>
              <strong class="text-[#dfe9ed] tabular-nums"
                >{searchView.instrumentation.canonicalStatesRetained}</strong
              >
            </div>
            <div class="flex justify-between gap-3">
              <span>Wall time</span>
              <strong class="text-[#dfe9ed] tabular-nums"
                >{searchView.instrumentation.wallTimeMs} ms</strong
              >
            </div>
          </div>
        {/if}
      {:else}
        <h3 class="m-0 mb-3 text-[0.68rem] font-bold tracking-wider text-dim uppercase">
          {#if searchView.nodeCount != null}
            Portfolio at N = {searchView.nodeCount}
          {:else}
            Portfolio
          {/if}
        </h3>
        <div class="mb-1.5 flex items-center justify-between gap-3 text-xs text-muted">
          <span>Profiles closed</span>
          <strong class="text-[#dfe9ed] tabular-nums">
            {#if searchView.profilesTotal == null}
              —
            {:else}
              {searchView.profilesUnsat} / {searchView.profilesTotal}
            {/if}
          </strong>
        </div>
        <div
          class="mb-3 h-2 overflow-hidden rounded-full bg-[#152530]"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={searchView.portfolioPct}
          aria-label="Profiles closed at this size"
        >
          <div
            class={`h-full ${muted ? 'bg-[#3d5560]' : 'bg-linear-to-r from-[#2a8f6a] to-flow'}`}
            style={`width: ${searchView.portfolioPct}%`}
          ></div>
        </div>
        <div class="grid gap-1.5 text-xs text-muted">
          <div class="flex justify-between gap-3">
            <span>Attempts</span>
            <strong class="text-[#dfe9ed] tabular-nums">{searchView.launchedAttempts}</strong>
          </div>
          <div class="flex justify-between gap-3">
            <span>Currently running</span>
            <strong class="text-[#dfe9ed] tabular-nums"
              >{muted ? 0 : searchView.activeAttempts}</strong
            >
          </div>
          <div class="flex justify-between gap-3">
            <span>Unstable rejects</span>
            <strong class="text-[#dfe9ed] tabular-nums">{searchView.rejected}</strong>
          </div>
          <div class="flex justify-between gap-3">
            <span>Abandoned unknowns</span>
            <strong class="text-[#dfe9ed] tabular-nums">{searchView.abandoned}</strong>
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}
