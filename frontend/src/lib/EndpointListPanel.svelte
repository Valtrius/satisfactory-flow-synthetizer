<script lang="ts">
  import type { Component } from 'svelte';
  import Plus from '@lucide/svelte/icons/plus';
  import X from '@lucide/svelte/icons/x';
  import type { EndpointRow } from '../types';
  import { MAX_ENDPOINTS } from './endpoints';
  import Button from './ui/Button.svelte';
  import Input from './ui/Input.svelte';

  type IconComponent = Component<{
    class?: string;
    size?: number;
    strokeWidth?: number;
    'aria-hidden'?: boolean | 'true';
  }>;

  type Props = {
    title: string;
    icon?: IconComponent;
    labelPrefix: string;
    endpoints: EndpointRow[];
    slots: number;
    minRows?: number;
    emptyTitle?: string;
    emptyBody?: string;
    class?: string;
    onAdd: () => void;
    onRemove: (index: number) => void;
    onUpdate: (index: number, field: 'rate' | 'multiplier', value: string) => void;
    onCommitMultiplier: (index: number) => void;
  };

  let {
    title,
    icon: TitleIcon,
    labelPrefix,
    endpoints,
    slots,
    minRows = 0,
    emptyTitle,
    emptyBody,
    class: className = '',
    onAdd,
    onRemove,
    onUpdate,
    onCommitMultiplier,
  }: Props = $props();
</script>

<section class={`flex flex-col px-4 pt-3 pb-4.5 md:pb-6 ${className}`}>
  <div class="flex items-center justify-between gap-4">
    <h2 class="m-0 flex items-center gap-2 text-lg font-bold tracking-tight">
      {#if TitleIcon}
        <TitleIcon class="text-accent size-[1.05rem] shrink-0" strokeWidth={2.2} aria-hidden="true" />
      {/if}
      {title}
    </h2>
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
          <span class="text-muted font-bold" aria-hidden="true">x</span>
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
        class="rounded-control grid min-h-17.5 content-center gap-1.5 border border-dashed border-[#375866] bg-[#0a1c25]/55 px-4 py-3 text-[#a9bec7]"
      >
        <strong class="text-sm text-[#d5e3e8]">{emptyTitle}</strong>
        <span class="text-xs leading-relaxed">{emptyBody}</span>
      </div>
    {/if}
  </div>
</section>
