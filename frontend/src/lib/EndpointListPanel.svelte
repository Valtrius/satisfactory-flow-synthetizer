<script lang="ts">
  import Plus from '@lucide/svelte/icons/plus';
  import X from '@lucide/svelte/icons/x';
  import type { EndpointRow } from '../types';
  import { MAX_ENDPOINTS } from './endpoints';
  import Button from './ui/Button.svelte';
  import Input from './ui/Input.svelte';
  import Panel from './ui/Panel.svelte';

  type Props = {
    title: string;
    labelPrefix: string;
    endpoints: EndpointRow[];
    slots: number;
    minRows?: number;
    emptyTitle?: string;
    emptyBody?: string;
    onAdd: () => void;
    onRemove: (index: number) => void;
    onUpdate: (index: number, field: 'rate' | 'multiplier', value: string) => void;
    onCommitMultiplier: (index: number) => void;
  };

  let {
    title,
    labelPrefix,
    endpoints,
    slots,
    minRows = 0,
    emptyTitle,
    emptyBody,
    onAdd,
    onRemove,
    onUpdate,
    onCommitMultiplier
  }: Props = $props();
</script>

<Panel class="p-4.5 md:p-6">
  <div class="flex items-start justify-between gap-4">
    <h2 class="m-0 text-lg font-bold tracking-tight">{title}</h2>
    <Button
      variant="primary"
      size="small"
      square
      type="button"
      aria-label={`Add ${labelPrefix.toLowerCase()}`}
      title={`Add ${labelPrefix.toLowerCase()}`}
      onclick={onAdd}
      disabled={slots >= MAX_ENDPOINTS}
    >
      <Plus size={16} strokeWidth={2.4} aria-hidden="true" />
    </Button>
  </div>
  <div class="mt-5.5 grid gap-3">
    {#each endpoints as endpoint, index (endpoint.id)}
      <div class="grid grid-cols-[auto_minmax(0,1fr)_2.125rem] items-center gap-2">
        <label class="flex items-center gap-1.5">
          <span class="sr-only">Count</span>
          <Input
            compact
            class="px-1.5 text-center tabular-nums"
            inputmode="numeric"
            value={endpoint.multiplier}
            oninput={(event) => onUpdate(index, 'multiplier', event.currentTarget.value)}
            onblur={() => onCommitMultiplier(index)}
            aria-label={`${labelPrefix} ${index + 1} count`}
          />
          <span class="font-bold text-muted" aria-hidden="true">x</span>
        </label>
        <label>
          <span class="sr-only">Items / min</span>
          <Input
            class="tabular-nums"
            inputmode="decimal"
            value={endpoint.rate}
            oninput={(event) => onUpdate(index, 'rate', event.currentTarget.value)}
            aria-label={`${labelPrefix} ${index + 1} items per minute`}
          />
        </label>
        <Button
          variant="danger"
          square
          class="h-10.5"
          type="button"
          aria-label={`Remove ${labelPrefix.toLowerCase()} ${index + 1}`}
          title={`Remove ${labelPrefix.toLowerCase()}`}
          onclick={() => onRemove(index)}
          disabled={endpoints.length <= minRows}
        >
          <X size={16} strokeWidth={2.4} aria-hidden="true" />
        </Button>
      </div>
    {/each}
    {#if endpoints.length === 0 && emptyTitle && emptyBody}
      <div
        class="grid min-h-17.5 content-center gap-1.5 rounded-tl-lg rounded-tr-xs rounded-br-lg rounded-bl-xs border border-dashed border-[#375866] bg-[#0a1c25]/55 px-4 py-3 text-[#a9bec7]"
      >
        <strong class="text-sm text-[#d5e3e8]">{emptyTitle}</strong>
        <span class="text-xs leading-relaxed">{emptyBody}</span>
      </div>
    {/if}
  </div>
</Panel>
