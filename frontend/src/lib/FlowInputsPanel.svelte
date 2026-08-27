<script lang="ts">
  import Download from '@lucide/svelte/icons/download';
  import UploadCloud from '@lucide/svelte/icons/upload-cloud';
  import type { EndpointRow, SolverEngine } from '../types';
  import ConstraintPanel from './ConstraintPanel.svelte';
  import EndpointListPanel from './EndpointListPanel.svelte';
  import Panel from './ui/Panel.svelte';

  type Props = {
    inputs: EndpointRow[];
    outputs: EndpointRow[];
    inputSlots: number;
    outputSlots: number;
    beltRate: string;
    enumerateAllAtN: boolean;
    engine: SolverEngine;
    hasRunning: boolean;
    onAddInput: () => void;
    onRemoveInput: (index: number) => void;
    onUpdateInput: (index: number, field: 'rate' | 'multiplier', value: string) => void;
    onCommitInputMultiplier: (index: number) => void;
    onAddOutput: () => void;
    onRemoveOutput: (index: number) => void;
    onUpdateOutput: (index: number, field: 'rate' | 'multiplier', value: string) => void;
    onCommitOutputMultiplier: (index: number) => void;
    onEnumerateChange: (value: boolean) => void;
    onEngineChange: (value: SolverEngine) => void;
    onSolve: () => void;
  };

  let {
    inputs,
    outputs,
    inputSlots,
    outputSlots,
    beltRate = $bindable(),
    enumerateAllAtN,
    engine,
    hasRunning,
    onAddInput,
    onRemoveInput,
    onUpdateInput,
    onCommitInputMultiplier,
    onAddOutput,
    onRemoveOutput,
    onUpdateOutput,
    onCommitOutputMultiplier,
    onEnumerateChange,
    onEngineChange,
    onSolve,
  }: Props = $props();
</script>

<Panel
  element="section"
  class="divide-line grid shrink-0 grid-cols-1 items-stretch divide-y md:grid-cols-3 md:divide-x md:divide-y-0"
  aria-label="Flow inputs"
>
  <EndpointListPanel
    title="Supply"
    icon={Download}
    labelPrefix="Input"
    endpoints={inputs}
    slots={inputSlots}
    emptyTitle="Automatic supply"
    emptyBody="Automatic supply uses one belt. If total demand exceeds its capacity, add explicit inputs."
    onAdd={onAddInput}
    onRemove={onRemoveInput}
    onUpdate={onUpdateInput}
    onCommitMultiplier={onCommitInputMultiplier}
  />

  <EndpointListPanel
    title="Demand"
    icon={UploadCloud}
    labelPrefix="Output"
    endpoints={outputs}
    slots={outputSlots}
    minRows={1}
    onAdd={onAddOutput}
    onRemove={onRemoveOutput}
    onUpdate={onUpdateOutput}
    onCommitMultiplier={onCommitOutputMultiplier}
  />

  <ConstraintPanel
    bind:beltRate
    {enumerateAllAtN}
    {engine}
    {hasRunning}
    {onEnumerateChange}
    {onEngineChange}
    {onSolve}
  />
</Panel>
