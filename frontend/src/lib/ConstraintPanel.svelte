<script lang="ts">
  import Button from './ui/Button.svelte';
  import Input from './ui/Input.svelte';
  import Panel from './ui/Panel.svelte';
  import Switch from './ui/Switch.svelte';
  import type { SolverEngine } from '../types';

  type Props = {
    beltRate: string;
    enumerateAllAtN: boolean;
    engine: SolverEngine;
    hasRunning: boolean;
    onEnumerateChange: (value: boolean) => void;
    onEngineChange: (value: SolverEngine) => void;
    onSolve: () => void;
  };

  let {
    beltRate = $bindable(),
    enumerateAllAtN,
    engine,
    hasRunning,
    onEnumerateChange,
    onEngineChange,
    onSolve
  }: Props = $props();
</script>

<Panel element="aside" class="flex flex-col p-4.5 md:col-span-2 md:p-6 xl:col-span-1">
  <h2 class="m-0 text-lg font-bold tracking-tight">Constraint</h2>
  <label class="mt-6">
    <span class="mb-1.5 block text-xs font-bold tracking-wide text-muted">Maximum belt rate</span>
    <div class="flex items-center">
      <Input
        shape="attached-left"
        class="tabular-nums"
        bind:value={beltRate}
        inputmode="decimal"
      />
      <em
        class="grid h-10.5 place-items-center whitespace-nowrap rounded-tr-xs rounded-br-lg border border-l-0 border-[#293f4b] bg-[#12222c] px-3 text-xs not-italic text-muted"
        >/ min</em
      >
    </div>
  </label>

  <div class="mt-5 border-t border-line pt-4">
    <span class="mb-2 block text-xs font-bold tracking-wide text-muted">Solver engine</span>
    <div class="grid grid-cols-2 gap-1 rounded-lg border border-[#293f4b] bg-[#0a151d] p-1">
      <button
        type="button"
        class={`rounded-md px-2.5 py-2 text-sm font-bold transition-colors ${
          engine === 'custom'
            ? 'bg-[#1d3a2e] text-[#8bdeb8]'
            : 'text-muted hover:bg-[#12222c] hover:text-[#dfe9ed]'
        }`}
        aria-pressed={engine === 'custom'}
        onclick={() => onEngineChange('custom')}
      >
        Custom
      </button>
      <button
        type="button"
        class={`rounded-md px-2.5 py-2 text-sm font-bold transition-colors ${
          engine === 'z3'
            ? 'bg-[#1d3a2e] text-[#8bdeb8]'
            : 'text-muted hover:bg-[#12222c] hover:text-[#dfe9ed]'
        }`}
        aria-pressed={engine === 'z3'}
        onclick={() => onEngineChange('z3')}
      >
        Z3
      </button>
    </div>
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
  <div class="mt-5.5 flex items-center gap-2.5">
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
</Panel>
