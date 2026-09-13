<script lang="ts">
  import ArrowRightToLine from '@lucide/svelte/icons/arrow-right-to-line';
  import ArrowRightFromLine from '@lucide/svelte/icons/arrow-right-from-line';
  import type { EndpointRow, SolveMode } from '../types';
  import ConstraintPanel from './ConstraintPanel.svelte';
  import EndpointListPanel from './EndpointListPanel.svelte';
  import Panel from './ui/Panel.svelte';

  type Props = {
    inputs: EndpointRow[];
    outputs: EndpointRow[];
    inputSlots: number;
    outputSlots: number;
    beltRate: string;
    solveMode: SolveMode;

    hasRunning: boolean;
    ready?: boolean;
    onAddInput: () => void;
    onRemoveInput: (index: number) => void;
    onUpdateInput: (index: number, field: 'rate' | 'multiplier', value: string) => void;
    onCommitInputMultiplier: (index: number) => void;
    onAddOutput: () => void;
    onRemoveOutput: (index: number) => void;
    onUpdateOutput: (index: number, field: 'rate' | 'multiplier', value: string) => void;
    onCommitOutputMultiplier: (index: number) => void;
    onSolveModeChange: (value: SolveMode) => void;

    onSolve: () => void;
  };

  let {
    inputs,
    outputs,
    inputSlots,
    outputSlots,
    beltRate = $bindable(),
    solveMode,

    hasRunning,
    ready = true,
    onAddInput,
    onRemoveInput,
    onUpdateInput,
    onCommitInputMultiplier,
    onAddOutput,
    onRemoveOutput,
    onUpdateOutput,
    onCommitOutputMultiplier,
    onSolveModeChange,

    onSolve,
  }: Props = $props();
</script>

<Panel
  variant="column"
  element="section"
  class="divide-line grid shrink-0 grid-cols-1 items-stretch divide-y"
  aria-label="Flow inputs"
>
  <EndpointListPanel
    title="Inputs"
    icon={ArrowRightToLine}
    labelPrefix="Input"
    endpoints={inputs}
    slots={inputSlots}
    emptyTitle="Automatic input"
    emptyBody="Automatic input uses one belt. If total output exceeds its capacity, add explicit inputs."
    onAdd={onAddInput}
    onRemove={onRemoveInput}
    onUpdate={onUpdateInput}
    onCommitMultiplier={onCommitInputMultiplier}
  />

  <EndpointListPanel
    title="Outputs"
    icon={ArrowRightFromLine}
    labelPrefix="Output"
    endpoints={outputs}
    slots={outputSlots}
    minRows={1}
    onAdd={onAddOutput}
    onRemove={onRemoveOutput}
    onUpdate={onUpdateOutput}
    onCommitMultiplier={onCommitOutputMultiplier}
  />

  <ConstraintPanel bind:beltRate {solveMode} {hasRunning} {ready} {onSolveModeChange} {onSolve} />
</Panel>
