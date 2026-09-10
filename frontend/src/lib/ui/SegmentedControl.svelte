<script lang="ts" generics="T extends string">
  type Option = {
    value: T;
    label: string;
  };

  type Size = 'default' | 'small' | 'large';

  type Props = {
    options: Option[];
    value: T;
    onchange: (value: T) => void;
    size?: Size;
    class?: string;
    disabled?: boolean;
    'aria-label'?: string;
  };

  let {
    options,
    value,
    onchange,
    size = 'default',
    class: className = '',
    disabled = false,
    'aria-label': ariaLabel,
  }: Props = $props();

  function navigate(event: KeyboardEvent, index: number): void {
    if (disabled || options.length === 0) return;
    let next: number;
    switch (event.key) {
      case 'ArrowRight':
      case 'ArrowDown':
        next = (index + 1) % options.length;
        break;
      case 'ArrowLeft':
      case 'ArrowUp':
        next = (index - 1 + options.length) % options.length;
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = options.length - 1;
        break;
      default:
        return;
    }
    event.preventDefault();
    onchange(options[next].value);
    (event.currentTarget as HTMLElement).parentElement
      ?.querySelectorAll<HTMLButtonElement>('[role="radio"]')
      [next]?.focus();
  }

  const selectedIndex = $derived(
    Math.max(
      0,
      options.findIndex((option) => option.value === value),
    ),
  );
  const count = $derived(Math.max(1, options.length));

  const sizeStyles: Record<
    Size,
    {
      root: string;
      button: string;
      inset: string;
      padRem: number;
      gapRem: number;
    }
  > = {
    small: {
      root: 'h-8 gap-0.5 p-0.5',
      button: 'px-2 text-xs',
      inset: 'top-0.5 bottom-0.5',
      padRem: 0.25,
      gapRem: 0.125,
    },
    default: {
      root: 'h-9 gap-1 p-1',
      button: 'px-2.5 text-sm',
      inset: 'top-1 bottom-1',
      padRem: 0.5,
      gapRem: 0.25,
    },
    large: {
      root: 'h-10 gap-1 p-1',
      button: 'px-3 text-sm',
      inset: 'top-1 bottom-1',
      padRem: 0.5,
      gapRem: 0.25,
    },
  };

  const style = $derived(sizeStyles[size]);
  const segmentWidth = $derived(`calc((100% - ${style.padRem}rem - ${(count - 1) * style.gapRem}rem) / ${count})`);
  const segmentLeft = $derived(
    `calc(${style.padRem / 2}rem + ${selectedIndex} * (${segmentWidth} + ${style.gapRem}rem))`,
  );
</script>

<div
  class={`border-field-border bg-well relative flex rounded-lg border ${style.root} ${className}`}
  role="radiogroup"
  aria-label={ariaLabel}
>
  <span
    class={`intent-primary pointer-events-none absolute rounded-md !border-transparent transition-[left,width] duration-150 ease-out ${style.inset}`}
    style={`width: ${segmentWidth}; left: ${segmentLeft};`}
    aria-hidden="true"
  ></span>
  {#each options as option, index (option.value)}
    <button
      type="button"
      role="radio"
      aria-checked={value === option.value}
      tabindex={index === selectedIndex ? 0 : -1}
      onkeydown={(event) => navigate(event, index)}
      class={`relative z-1 flex h-full flex-1 items-center justify-center rounded-md border border-transparent font-bold whitespace-nowrap transition-colors duration-150 disabled:cursor-not-allowed disabled:opacity-50 ${style.button} ${
        value === option.value ? 'text-on-accent cursor-default' : 'text-muted hover:text-control-fg cursor-pointer'
      }`}
      {disabled}
      onclick={() => onchange(option.value)}
    >
      {option.label}
    </button>
  {/each}
</div>
