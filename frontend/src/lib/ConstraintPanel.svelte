<script lang="ts">
  import CircleHelp from '@lucide/svelte/icons/circle-help';
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';
  import Button from './ui/Button.svelte';
  import Input from './ui/Input.svelte';
  import SegmentedControl from './ui/SegmentedControl.svelte';
  import type { SolveMode } from '../types';

  type Props = {
    beltRate: string;
    solveMode: SolveMode;

    hasRunning: boolean;
    class?: string;
    onSolveModeChange: (value: SolveMode) => void;

    onSolve: () => void;
  };

  let {
    beltRate = $bindable(),
    solveMode,

    hasRunning,
    class: className = '',
    onSolveModeChange,

    onSolve,
  }: Props = $props();

  let helpOpen = $state(false);
  let helpRoot: HTMLDivElement | undefined = $state();

  $effect(() => {
    if (!helpOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (helpRoot && !helpRoot.contains(event.target as HTMLElement)) helpOpen = false;
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') helpOpen = false;
    };
    window.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('keydown', onKeyDown);
    };
  });

  const solveModeOptions: { value: SolveMode; label: string }[] = [
    { value: 'one_min_nl', label: 'One min N/L' },
    { value: 'all_min_nl', label: 'All min N/L' },
    { value: 'all_min_n', label: 'All min N' },
  ];

  const helpSections = [
    {
      title: 'Glossary',
      tone: 'default' as const,
      items: [
        'N — number of splitters and mergers (nodes) in the layout.',
        'L — operator-to-operator belt count; excludes supply/demand stubs and discard belts.',
        'Maximum belt rate — capacity of every physical belt (/min), including discard.',
      ],
    },
    {
      title: 'Layouts',
      tone: 'default' as const,
      items: [
        'One min N/L — return a single layout at minimum N and minimum L.',
        'All min N/L — return every distinct layout at minimum N and that proven minimum L.',
        'All min N — return every distinct layout at minimum N across every feasible L.',
      ],
    },
    {
      title: 'Runtime warnings',
      tone: 'warning' as const,
      items: [
        'Find-all modes (All min N/L and All min N) can take far longer than One min N/L; cost grows with the search space and how many layouts exist.',
        'All min N is the widest scope and can explode on harder problems (many layouts across several L values).',
        'Prefer One min N/L while exploring. Use All min N/L for every min-L alternative; reserve All min N only when you need every min-N layout. Jobs stay cancellable and queueable.',
      ],
    },
  ] as const;
</script>

<section class={`flex flex-col gap-3 px-4 py-3 ${className}`}>
  <div class="relative flex items-center gap-2" bind:this={helpRoot}>
    <h2 class="m-0 flex items-center gap-2 text-lg font-bold tracking-tight">
      <SlidersHorizontal class="text-accent size-[1.05rem] shrink-0" strokeWidth={2.2} aria-hidden="true" />
      Constraint
    </h2>
    <Button
      size="small"
      square
      variant="quiet"
      type="button"
      class="!min-h-7 !w-7"
      title="Constraint help"
      aria-label="Constraint help"
      aria-expanded={helpOpen}
      aria-haspopup="dialog"
      onclick={() => (helpOpen = !helpOpen)}
    >
      <CircleHelp size={16} strokeWidth={2.2} aria-hidden="true" />
    </Button>
    {#if helpOpen}
      <div
        class="border-line bg-panel absolute top-[calc(100%+0.35rem)] right-0 z-30 max-h-[min(36rem,80dvh)] w-[min(28rem,calc(100vw-2rem))] overflow-y-auto rounded-lg border px-4 py-3.5 shadow-[0_14px_32px_rgb(0_0_0/45%)]"
        role="dialog"
        aria-label="Constraint glossary and options"
      >
        <div class="flex flex-col gap-3.5 text-sm leading-relaxed">
          {#each helpSections as section}
            <div>
              <p
                class={`m-0 mb-1.5 text-xs font-bold tracking-wide uppercase ${
                  section.tone === 'warning' ? 'text-warning' : 'text-ink'
                }`}
              >
                {section.title}
              </p>
              <ul
                class={`m-0 list-disc space-y-1.5 pl-4 text-[0.8125rem] ${
                  section.tone === 'warning' ? 'text-warning/90' : 'text-muted'
                }`}
              >
                {#each section.items as item}
                  <li>{item}</li>
                {/each}
              </ul>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
  <label class="mt-3">
    <span class="text-muted mb-1.5 block text-xs font-bold tracking-wide">Maximum belt rate</span>
    <div class="flex items-center">
      <Input shape="attached-left" class="tabular-nums" bind:value={beltRate} inputmode="decimal" />
      <em
        class="border-field-border bg-well-hover text-muted grid h-9 place-items-center rounded-tr-xs rounded-br-lg border border-l-0 px-3 text-xs whitespace-nowrap not-italic"
      >
        / min
      </em>
    </div>
  </label>

  <div>
    <span class="text-muted mb-2 block text-xs font-bold tracking-wide">Layouts</span>
    <SegmentedControl
      size="small"
      options={solveModeOptions}
      value={solveMode}
      onchange={onSolveModeChange}
      aria-label="Layouts to find"
    />
  </div>
  <div class="mt-auto flex items-center gap-2.5 pt-2">
    <Button variant="primary" class="flex-1" type="button" onclick={onSolve}>
      {hasRunning ? 'Queue' : 'Find'}
      {solveModeOptions.find((option) => option.value === solveMode)?.label}
    </Button>
  </div>
</section>
