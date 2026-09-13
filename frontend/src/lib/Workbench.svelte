<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import History from '@lucide/svelte/icons/history';
  import Layers from '@lucide/svelte/icons/layers';
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';
  import Button from './ui/Button.svelte';

  let {
    setup,
    history,
    layouts,
    children,
    graphFullscreen = false,
    setupOpen = $bindable(true),
    historyOpen = $bindable(true),
    layoutsOpen = $bindable(true),
  }: {
    setup: Snippet;
    history: Snippet;
    layouts: Snippet;
    children: Snippet;
    graphFullscreen?: boolean;
    setupOpen?: boolean;
    historyOpen?: boolean;
    layoutsOpen?: boolean;
  } = $props();

  const panelId = $props.id();
  let wide = $state(false);
  const collectionOpen = $derived(historyOpen || layoutsOpen);
  // Columns stay fixed; narrower screens combine History and Layouts or overlay the docks.
  const setupWidth = 340;
  const historyWidth = 280;
  const layoutsWidth = 256;
  const collectionWidth = 280;
  const collectionTotal = $derived(
    wide ? (historyOpen ? historyWidth : 0) + (layoutsOpen ? layoutsWidth : 0) : collectionOpen ? collectionWidth : 0,
  );

  onMount(() => {
    const media = window.matchMedia('(min-width: 1800px)');
    const update = () => (wide = media.matches);
    update();
    media.addEventListener('change', update);
    if (window.matchMedia('(max-width: 900px)').matches) {
      setupOpen = false;
      historyOpen = false;
      layoutsOpen = false;
    }
    return () => media.removeEventListener('change', update);
  });

  function toggleCollection(): void {
    const open = !collectionOpen;
    historyOpen = open;
    layoutsOpen = open;
  }

  function handlePanelShortcut(event: KeyboardEvent): void {
    if (
      graphFullscreen ||
      event.defaultPrevented ||
      event.repeat ||
      event.isComposing ||
      event.ctrlKey ||
      event.altKey ||
      event.metaKey ||
      event.location === KeyboardEvent.DOM_KEY_LOCATION_NUMPAD ||
      !['Digit1', 'Digit2', 'Digit3'].includes(event.code)
    )
      return;

    const target = event.target;
    if (
      target instanceof HTMLElement &&
      (target.isContentEditable ||
        target.closest(
          'input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"], [role="combobox"], [role="dialog"], [role="menu"], [popover], dialog',
        ))
    )
      return;

    // Physical number-row keys also cover AZERTY &, é and " without requiring Shift.
    event.preventDefault();
    if (event.code === 'Digit1') setupOpen = !setupOpen;
    else if (event.code === 'Digit2') historyOpen = !historyOpen;
    else layoutsOpen = !layoutsOpen;
  }
</script>

<svelte:window onkeydown={handlePanelShortcut} />

<main
  class="flex h-dvh w-full overflow-hidden"
  class:wide
  style={`--setup-width: ${setupWidth}px; --history-width: ${historyWidth}px; --layouts-width: ${layoutsWidth}px; --collection-width: ${collectionWidth}px; --collection-total: ${collectionTotal}px;`}
>
  <nav
    class="border-line bg-panel z-30 flex shrink-0 basis-13 flex-col items-center gap-2 border-r py-3"
    aria-label="Workspace panels"
  >
    <Button
      variant="rail"
      square
      title="Setup"
      aria-label="Toggle Setup"
      aria-pressed={setupOpen}
      aria-controls={`${panelId}-setup`}
      onclick={() => (setupOpen = !setupOpen)}
    >
      <SlidersHorizontal size={18} strokeWidth={2.2} aria-hidden="true" />
    </Button>
    {#if wide}
      <Button
        variant="rail"
        square
        title="History"
        aria-label="Toggle History"
        aria-pressed={historyOpen}
        aria-controls={`${panelId}-history`}
        onclick={() => (historyOpen = !historyOpen)}
      >
        <History size={18} strokeWidth={2.2} aria-hidden="true" />
      </Button>
      <Button
        variant="rail"
        square
        title="Layouts"
        aria-label="Toggle Layouts"
        aria-pressed={layoutsOpen}
        aria-controls={`${panelId}-layouts`}
        onclick={() => (layoutsOpen = !layoutsOpen)}
      >
        <Layers size={18} strokeWidth={2.2} aria-hidden="true" />
      </Button>
    {:else}
      <Button
        variant="rail"
        square
        title="History + Layouts"
        aria-label="Toggle History and Layouts"
        aria-pressed={collectionOpen}
        aria-controls={`${panelId}-history ${panelId}-layouts`}
        onclick={toggleCollection}
      >
        <History size={18} strokeWidth={2.2} aria-hidden="true" />
      </Button>
    {/if}
    <span
      class="text-dim mt-auto hidden rotate-180 py-2 text-[10px] font-bold tracking-[0.18em] whitespace-nowrap [writing-mode:vertical-rl] [@media(min-height:560px)]:block"
      aria-hidden="true"
    >
      SATISFACTORY FLOW SYNTHETIZER
    </span>
  </nav>

  <div class="panel-docks flex min-w-0 flex-none">
    <section
      id={`${panelId}-setup`}
      class="setup-column dock border-line relative min-h-0 min-w-0 overflow-hidden border-r"
      class:is-open={setupOpen}
      aria-label="Setup"
      aria-hidden={!setupOpen}
      inert={!setupOpen}
    >
      <div class="setup-content bg-panel h-full w-[calc(var(--setup-width)-1px)] overflow-auto">{@render setup()}</div>
    </section>
    <div
      class="collection-columns relative min-h-0 min-w-0 overflow-hidden"
      class:has-history={historyOpen}
      class:has-layouts={layoutsOpen}
    >
      <section
        id={`${panelId}-history`}
        class="history-column dock border-line relative min-h-0 min-w-0 overflow-hidden border-r border-b"
        class:is-open={historyOpen}
        aria-label="History"
        aria-hidden={!historyOpen}
        inert={!historyOpen}
      >
        <div class="panel-content h-full min-h-0 w-[calc(var(--collection-width)-1px)]">{@render history()}</div>
      </section>
      <section
        id={`${panelId}-layouts`}
        class="layouts-column dock border-line relative min-h-0 min-w-0 overflow-hidden border-r"
        class:is-open={layoutsOpen}
        aria-label="Layouts"
        aria-hidden={!layoutsOpen}
        inert={!layoutsOpen}
      >
        <div class="panel-content h-full min-h-0 w-[calc(var(--collection-width)-1px)]">{@render layouts()}</div>
      </section>
    </div>
  </div>
  <div class="graph-column flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">{@render children()}</div>
</main>

<style>
  /* Keep coupled dock sizing and transitions together; routine layout uses utilities. */
  .dock,
  .collection-columns {
    transition:
      width 240ms ease,
      flex-basis 240ms ease,
      grid-template-rows 240ms ease;
  }
  .setup-column {
    flex: 0 0 var(--setup-width);
    width: var(--setup-width);
  }
  .setup-content,
  .panel-content {
    transition:
      transform 240ms ease,
      opacity 180ms ease;
  }
  .collection-columns {
    display: grid;
    flex: 0 0 var(--collection-total);
    width: var(--collection-total);
    grid-template-rows: minmax(0, 2fr) minmax(0, 3fr);
  }
  .collection-columns:not(.has-layouts) {
    grid-template-rows: minmax(0, 1fr) minmax(0, 0fr);
  }
  .collection-columns:not(.has-history) {
    grid-template-rows: minmax(0, 0fr) minmax(0, 1fr);
  }
  .wide .collection-columns {
    display: flex;
  }
  .wide .history-column {
    flex: 0 0 var(--history-width);
    width: var(--history-width);
    border-bottom: 0;
  }
  .wide .layouts-column {
    flex: 0 0 var(--layouts-width);
    width: var(--layouts-width);
  }
  .wide .history-column .panel-content {
    width: calc(var(--history-width) - 1px);
  }
  .wide .layouts-column .panel-content {
    width: calc(var(--layouts-width) - 1px);
  }
  .dock:not(.is-open) {
    border-width: 0;
  }
  .setup-column:not(.is-open),
  .wide .dock:not(.is-open) {
    flex-basis: 0;
    width: 0;
  }
  .dock:not(.is-open) > .setup-content,
  .dock:not(.is-open) > .panel-content {
    transform: translateX(-100%);
    opacity: 0;
  }

  @media (max-width: 900px) {
    .panel-docks {
      position: absolute;
      inset: 0 0 0 52px;
      z-index: 20;
      width: max-content;
      max-width: calc(100% - 52px);
      overflow-x: auto;
      box-shadow: 12px 0 32px rgb(0 0 0 / 35%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .dock,
    .collection-columns,
    .setup-content,
    .panel-content {
      transition: none;
    }
  }
</style>
