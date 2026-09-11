<script lang="ts">
  import { Handle, type NodeProps, useUpdateNodeInternals } from '@xyflow/svelte';
  import ArrowLeftRight from '@lucide/svelte/icons/arrow-left-right';
  import ArrowUpDown from '@lucide/svelte/icons/arrow-up-down';
  import MoveDiagonal from '@lucide/svelte/icons/move-diagonal';
  import MoveDiagonal2 from '@lucide/svelte/icons/move-diagonal-2';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import RotateCw from '@lucide/svelte/icons/rotate-cw';
  import { sideToPosition, type PortSide, type RotateDirection } from './graph';
  import Button from './ui/Button.svelte';

  interface FactoryNodeData {
    label: string;
    inputPositions: PortSide[];
    outputPositions: PortSide[];
    onSwapSides?: (nodeId: string, first: PortSide, second: PortSide) => void;
    onRotatePorts?: (nodeId: string, direction: RotateDirection) => void;
  }

  let { id, data, selected }: NodeProps = $props();
  let node = $derived(data as unknown as FactoryNodeData);
  const updateNodeInternals = useUpdateNodeInternals();

  const controls = [
    {
      class: '-top-10.5 -left-10.5',
      label: 'Swap top and left ports',
      icon: MoveDiagonal,
      action: (event: MouseEvent) => swapPorts('top', 'left', event),
    },
    {
      class: '-top-10.5 -right-10.5',
      label: 'Swap top and right ports',
      icon: MoveDiagonal2,
      action: (event: MouseEvent) => swapPorts('top', 'right', event),
    },
    {
      class: '-bottom-10.5 -left-10.5',
      label: 'Swap bottom and left ports',
      icon: MoveDiagonal2,
      action: (event: MouseEvent) => swapPorts('bottom', 'left', event),
    },
    {
      class: '-right-10.5 -bottom-10.5',
      label: 'Swap bottom and right ports',
      icon: MoveDiagonal,
      action: (event: MouseEvent) => swapPorts('bottom', 'right', event),
    },
    {
      class: '-top-14 left-[calc(50%-2.5rem)]',
      label: 'Rotate ports counter-clockwise',
      icon: RotateCcw,
      action: (event: MouseEvent) => rotatePorts('ccw', event),
    },
    {
      class: '-top-14 left-[calc(50%+.25rem)]',
      label: 'Rotate ports clockwise',
      icon: RotateCw,
      action: (event: MouseEvent) => rotatePorts('cw', event),
    },
    {
      class: '-bottom-14 left-[calc(50%-2.5rem)]',
      label: 'Swap left and right ports',
      icon: ArrowLeftRight,
      action: (event: MouseEvent) => swapPorts('left', 'right', event),
    },
    {
      class: '-bottom-14 left-[calc(50%+.25rem)]',
      label: 'Swap top and bottom ports',
      icon: ArrowUpDown,
      action: (event: MouseEvent) => swapPorts('top', 'bottom', event),
    },
  ];

  $effect(() => {
    const key = `${node.inputPositions.join(',')}|${node.outputPositions.join(',')}`;
    void key;
    updateNodeInternals(id);
  });

  function swapPorts(first: PortSide, second: PortSide, event: MouseEvent): void {
    event.stopPropagation();
    node.onSwapSides?.(id, first, second);
  }

  function rotatePorts(direction: RotateDirection, event: MouseEvent): void {
    event.stopPropagation();
    node.onRotatePorts?.(id, direction);
  }
</script>

{#each node.inputPositions as side, port}
  <Handle id={`target-${port}`} type="target" position={sideToPosition(side)} />
{/each}

<span class="pointer-events-none">{node.label}</span>

{#each node.outputPositions as side, port}
  <Handle id={`source-${port}`} type="source" position={sideToPosition(side)} />
{/each}

{#if selected}
  <div class="nodrag nopan pointer-events-none absolute inset-0 z-8" aria-label={`Arrange ${node.label} ports`}>
    {#each controls as control}
      {@const Icon = control.icon}
      <Button
        square
        class={`hover:border-flow pointer-events-auto absolute !size-9 border-[#547083] bg-[#10232d] text-[#dce8ed] shadow-lg hover:bg-[#173542] ${control.class}`}
        type="button"
        title={control.label}
        aria-label={control.label}
        onclick={control.action}
      >
        <Icon size={20} strokeWidth={2.2} aria-hidden="true" />
      </Button>
    {/each}
  </div>
{/if}
