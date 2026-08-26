<script lang="ts" generics="T extends string">
  type Option = {
    value: T;
    label: string;
  };

  type Props = {
    options: Option[];
    value: T;
    onchange: (value: T) => void;
    class?: string;
    disabled?: boolean;
    'aria-label'?: string;
  };

  let {
    options,
    value,
    onchange,
    class: className = '',
    disabled = false,
    'aria-label': ariaLabel
  }: Props = $props();

  const selectedIndex = $derived(
    Math.max(
      0,
      options.findIndex((option) => option.value === value)
    )
  );
  const count = $derived(Math.max(1, options.length));
</script>

<div
  class={`relative flex gap-1 rounded-lg border border-field-border bg-well p-1 ${className}`}
  role="radiogroup"
  aria-label={ariaLabel}
>
  <span
    class="pointer-events-none absolute top-1 bottom-1 rounded-md intent-primary !border-transparent transition-[left,width] duration-150 ease-out"
    style={`width: calc((100% - 0.5rem - ${(count - 1) * 0.25}rem) / ${count}); left: calc(0.25rem + ${selectedIndex} * ((100% - 0.5rem - ${(count - 1) * 0.25}rem) / ${count} + 0.25rem));`}
    aria-hidden="true"
  ></span>
  {#each options as option (option.value)}
    <button
      type="button"
      role="radio"
      aria-checked={value === option.value}
      class={`relative z-1 flex-1 rounded-md border border-transparent px-2.5 py-2 text-sm font-bold transition-colors duration-150 disabled:cursor-not-allowed disabled:opacity-50 ${
        value === option.value
          ? 'text-on-accent'
          : 'text-muted hover:text-control-fg'
      }`}
      disabled={disabled}
      onclick={() => onchange(option.value)}
    >
      {option.label}
    </button>
  {/each}
</div>
