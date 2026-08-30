<script lang="ts">
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';
  import Button from './ui/Button.svelte';
  import Input from './ui/Input.svelte';
  import SegmentedControl from './ui/SegmentedControl.svelte';
  import type { SolveMode, SolverEngine } from '../types';

  type Props = {
    beltRate: string;
    solveMode: SolveMode;
    engine: SolverEngine;
    hasRunning: boolean;
    class?: string;
    onSolveModeChange: (value: SolveMode) => void;
    onEngineChange: (value: SolverEngine) => void;
    onSolve: () => void;
  };

  let {
    beltRate = $bindable(),
    solveMode,
    engine,
    hasRunning,
    class: className = '',
    onSolveModeChange,
    onEngineChange,
    onSolve,
  }: Props = $props();

  const engineOptions: { value: SolverEngine; label: string }[] = [
    { value: 'custom', label: 'Custom' },
    { value: 'z3', label: 'Z3' },
  ];
  const solveModeOptions: { value: SolveMode; label: string }[] = [
    { value: 'optimal', label: 'One' },
    { value: 'all_at_minimum_nodes_and_minimum_links', label: 'All min L' },
    { value: 'all_at_minimum_nodes', label: 'All L' },
  ];
</script>

<section class={`flex flex-col px-4 pt-3 pb-4.5 md:pb-6 ${className}`}>
  <h2 class="m-0 flex items-center gap-2 text-lg font-bold tracking-tight">
    <SlidersHorizontal class="text-accent size-[1.05rem] shrink-0" strokeWidth={2.2} aria-hidden="true" />
    Constraint
  </h2>
  <label class="mt-5.5">
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

  <div class="border-line mt-5 border-t pt-4">
    <span class="text-muted mb-2 block text-xs font-bold tracking-wide">Solver engine</span>
    <SegmentedControl
      size="default"
      options={engineOptions}
      value={engine}
      onchange={onEngineChange}
      aria-label="Solver engine"
    />
    <span class="text-muted mt-1.5 block text-xs">
      {engine === 'custom'
        ? 'Deterministic exact search with incumbents and optional full-N enumeration.'
        : 'Portfolio SMT search with Z3 attempt telemetry.'}
    </span>
  </div>

  <div class="border-line mt-5 border-t pt-4">
    <span class="text-muted mb-2 block text-xs font-bold tracking-wide">Layouts</span>
    <SegmentedControl
      size="default"
      options={solveModeOptions}
      value={solveMode}
      onchange={onSolveModeChange}
      aria-label="Layouts to find"
    />
    <span class="text-muted mt-1.5 block text-xs">
      {solveMode === 'optimal'
        ? 'Return one layout at minimum N and minimum L.'
        : solveMode === 'all_at_minimum_nodes_and_minimum_links'
          ? 'Return every layout at minimum N and minimum L.'
          : 'Return every layout at minimum N across every feasible L.'}
    </span>
  </div>
  <div class="mt-auto flex items-center gap-2.5 pt-5.5">
    <Button variant="primary" class="flex-1" type="button" onclick={onSolve}>
      {hasRunning
        ? solveMode === 'optimal'
          ? 'Queue optimal layout'
          : 'Queue all layouts'
        : solveMode === 'optimal'
          ? 'Find optimal layout'
          : 'Find all layouts'}
    </Button>
  </div>
</section>
