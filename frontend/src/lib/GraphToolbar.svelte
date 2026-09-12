<script module lang="ts">
  import type { RotateDirection } from './graph';

  export type GraphEditingControls = {
    canUndo: boolean;
    canRedo: boolean;
    onRotate: (direction: RotateDirection) => void;
    onUndo: () => void;
    onRedo: () => void;
    onReset: () => void;
    onExport: () => void;
    onShare?: () => void;
    onToggleFullscreen: () => void;
  };
</script>

<script lang="ts">
  import Download from '@lucide/svelte/icons/download';
  import Share2 from '@lucide/svelte/icons/share-2';
  import Maximize2 from '@lucide/svelte/icons/maximize-2';
  import Minimize2 from '@lucide/svelte/icons/minimize-2';
  import Redo2 from '@lucide/svelte/icons/redo-2';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import Button from './ui/Button.svelte';

  let {
    editing,
    fullscreen,
    hasNodes,
  }: {
    editing: GraphEditingControls;
    fullscreen: boolean;
    hasNodes: boolean;
  } = $props();
</script>

<div class="flex flex-col items-end gap-1">
  <div class="flex gap-1.5" aria-label="Graph tools">
    <Button
      size="small"
      square
      type="button"
      title="Rotate graph 90° counter-clockwise"
      aria-label="Rotate graph 90 degrees counter-clockwise"
      onclick={() => editing.onRotate('ccw')}
      disabled={!hasNodes}
    >
      ↺
    </Button>
    <Button
      size="small"
      square
      type="button"
      title="Rotate graph 90° clockwise"
      aria-label="Rotate graph 90 degrees clockwise"
      onclick={() => editing.onRotate('cw')}
      disabled={!hasNodes}
    >
      ↻
    </Button>
    <Button
      size="small"
      square
      type="button"
      title="Export SVG"
      aria-label="Export SVG"
      onclick={editing.onExport}
      disabled={!hasNodes}
    >
      <Download size={16} strokeWidth={2.2} aria-hidden="true" />
    </Button>
    {#if editing.onShare}<Button
        size="small"
        square
        type="button"
        title="Share solution"
        aria-label="Share solution"
        disabled={!hasNodes}
        onclick={editing.onShare}
      >
        <Share2 size={16} strokeWidth={2.2} aria-hidden="true" />
      </Button>{/if}
    <Button
      size="small"
      square
      type="button"
      title={fullscreen ? 'Exit expanded graph (F)' : 'Expand graph (F)'}
      aria-label={fullscreen ? 'Exit expanded graph' : 'Expand graph'}
      onclick={editing.onToggleFullscreen}
      disabled={!hasNodes}
    >
      {#if fullscreen}
        <Minimize2 size={16} strokeWidth={2.2} aria-hidden="true" />
      {:else}
        <Maximize2 size={16} strokeWidth={2.2} aria-hidden="true" />
      {/if}
    </Button>
  </div>
  <div class="flex gap-1.5" aria-label="Graph edit history">
    <Button
      size="small"
      square
      type="button"
      title="Undo"
      aria-label="Undo last graph change"
      disabled={!editing.canUndo}
      onclick={editing.onUndo}
    >
      <Undo2 size={16} strokeWidth={2.2} aria-hidden="true" />
    </Button>
    <Button
      size="small"
      square
      type="button"
      title="Redo"
      aria-label="Redo last undone graph change"
      disabled={!editing.canRedo}
      onclick={editing.onRedo}
    >
      <Redo2 size={16} strokeWidth={2.2} aria-hidden="true" />
    </Button>
    <Button
      size="small"
      square
      type="button"
      title="Reset layout"
      aria-label="Reset graph to default positions"
      onclick={editing.onReset}
      disabled={!hasNodes}
    >
      <RotateCcw size={16} strokeWidth={2.2} aria-hidden="true" />
    </Button>
  </div>
</div>
