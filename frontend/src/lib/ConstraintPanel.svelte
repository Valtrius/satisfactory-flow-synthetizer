<script lang="ts">
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';
  import Button from './ui/Button.svelte';
  import Input from './ui/Input.svelte';
  import SegmentedControl from './ui/SegmentedControl.svelte';
  import Switch from './ui/Switch.svelte';
  import type { SolverEngine } from '../types';

  type Props = {
    beltRate: string;
    enumerateAllAtN: boolean;
    engine: SolverEngine;
    hasRunning: boolean;
    class?: string;
    onEnumerateChange: (value: boolean) => void;
    onEngineChange: (value: SolverEngine) => void;
    onSolve: () => void;
  };

  let {
    beltRate = $bindable(),
    enumerateAllAtN,
    engine,
    hasRunning,
    class: className = '',
    onEnumerateChange,
    onEngineChange,
    onSolve
  }: Props = $props();

  const engineOptions: { value: SolverEngine; label: string }[] = [
    { value: 'custom', label: 'Custom' },
    { value: 'z3', label: 'Z3' }
  ];
</script>

<section class={`flex flex-col px-4 pt-3 pb-4.5 md:pb-6 ${className}`}>
  <h2 class="m-0 flex items-center gap-2 text-lg font-bold tracking-tight">
    <SlidersHorizontal class="size-[1.05rem] shrink-0 text-accent" strokeWidth={2.2} aria-hidden="true" />
    Constraint
  </h2>
  <label class="mt-5.5">
    <span class="mb-1.5 block text-xs font-bold tracking-wide text-muted">Maximum belt rate</span>
    <div class="flex items-center">
      <Input
        shape="attached-left"
        class="tabular-nums"
        bind:value={beltRate}
        inputmode="decimal"
      />
      <em
        class="grid h-10.5 place-items-center whitespace-nowrap rounded-tr-xs rounded-br-lg border border-l-0 border-field-border bg-well-hover px-3 text-xs not-italic text-muted"
        >/ min</em
      >
    </div>
  </label>

  <div class="mt-5 border-t border-line pt-4">
    <span class="mb-2 block text-xs font-bold tracking-wide text-muted">Solver engine</span>
    <SegmentedControl
      options={engineOptions}
      value={engine}
      onchange={onEngineChange}
      aria-label="Solver engine"
    />
    <span class="mt-1.5 block text-xs text-muted">
      {engine === 'custom'
        ? 'Deterministic exact search with incumbents and optional full-N enumeration.'
        : 'Portfolio SMT search with Z3 attempt telemetry.'}
    </span>
  </div>

  <div class="mt-5 flex items-start justify-between gap-3 border-t border-line pt-4">
    <div class="min-w-0">
      <span class="block text-sm font-bold">Find all layouts at N</span>
      <span class="mt-0.5 block text-xs text-muted">
        After the minimal node count is proven, keep searching until every distinct layout at that
        size is found.
      </span>
    </div>
    <Switch
      checked={enumerateAllAtN}
      label="Find all layouts at N"
      onclick={() => onEnumerateChange(!enumerateAllAtN)}
    />
  </div>
  <div class="mt-auto flex items-center gap-2.5 pt-5.5">
    <Button variant="primary" class="flex-1" type="button" onclick={onSolve}>
      {hasRunning
        ? enumerateAllAtN
          ? 'Queue all layouts'
          : 'Queue optimal layout'
        : enumerateAllAtN
          ? 'Find all layouts'
          : 'Find optimal layout'}
    </Button>
  </div>
</section>
