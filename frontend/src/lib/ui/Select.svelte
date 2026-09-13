<script lang="ts" generics="T extends string | number">
  import { onMount, tick } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Button from './Button.svelte';

  let {
    value = $bindable(),
    options,
    label,
  }: {
    value: T;
    options: { value: T; label: string }[];
    label: string;
  } = $props();
  const id = $props.id();
  let root: HTMLDivElement;
  let menu: HTMLDivElement;
  let trigger: HTMLButtonElement;
  let open = $state(false);
  let active = $state(0);
  let position = $state('');
  const selected = $derived(
    Math.max(
      0,
      options.findIndex((option) => option.value === value),
    ),
  );

  function place(): void {
    const rect = trigger.getBoundingClientRect();
    const below = window.innerHeight - rect.bottom - 8;
    const above = rect.top - 8;
    const upward = below < Math.min(options.length * 36 + 8, 280) && above > below;
    position = `left: ${rect.left}px; width: ${rect.width}px; max-height: ${Math.max(40, Math.min(280, upward ? above : below))}px; ${upward ? `bottom: ${window.innerHeight - rect.top + 4}px; top: auto` : `top: ${rect.bottom + 4}px; bottom: auto`};`;
  }
  function close(): void {
    open = false;
    menu.hidePopover?.();
  }
  function show(): void {
    active = selected;
    place();
    open = true;
    menu.showPopover?.();
    void tick().then(() => {
      if (open) menu.children[active]?.scrollIntoView?.({ block: 'nearest' });
    });
  }
  function choose(index: number): void {
    value = options[index].value;
    close();
    trigger.focus();
  }
  function keydown(event: KeyboardEvent): void {
    if (event.key === 'Tab') {
      close();
      return;
    }
    if (event.key === 'Escape') {
      if (open) {
        event.preventDefault();
        event.stopPropagation();
        close();
      }
      return;
    }
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault();
      if (!open) show();
      else if (event.key === 'ArrowDown') active = Math.min(options.length - 1, active + 1);
      else if (event.key === 'ArrowUp') active = Math.max(0, active - 1);
      if (event.key === 'Home') active = 0;
      if (event.key === 'End') active = options.length - 1;
    } else if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      if (open) choose(active);
      else show();
    } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      const match = options.findIndex((option) => option.label.toLowerCase().startsWith(event.key.toLowerCase()));
      if (match >= 0) {
        event.preventDefault();
        if (!open) show();
        active = match;
      }
    }
    if (open) menu.children[active]?.scrollIntoView?.({ block: 'nearest' });
  }
  onMount(() => {
    trigger = root.querySelector('button')!;
    const outside = (event: Event) => {
      if (open && event.target instanceof Node && !root.contains(event.target)) close();
    };
    const reposition = () => {
      if (open) place();
    };
    document.addEventListener('pointerdown', outside);
    document.addEventListener('focusin', outside);
    window.addEventListener('resize', reposition);
    window.addEventListener('scroll', reposition, true);
    return () => {
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('focusin', outside);
      window.removeEventListener('resize', reposition);
      window.removeEventListener('scroll', reposition, true);
    };
  });
</script>

<div bind:this={root} class="min-w-0">
  <Button
    variant="quiet"
    size="small"
    class="flex w-full items-center justify-between gap-3 font-normal!"
    role="combobox"
    aria-label={label}
    aria-expanded={open}
    aria-haspopup="listbox"
    aria-controls={id}
    aria-activedescendant={open ? `${id}-${active}` : undefined}
    onkeydown={keydown}
    onclick={() => (open ? close() : show())}
  >
    {options[selected]?.label}
    <ChevronDown size={14} aria-hidden="true" />
  </Button>
  <div
    bind:this={menu}
    {id}
    class={`border-control-border bg-panel text-ink fixed inset-auto z-200 m-0 overflow-y-auto rounded-md border p-1 shadow-[0_14px_32px_rgb(0_0_0/45%)] ${open ? 'block' : 'hidden'}`}
    style={position}
    role="listbox"
    aria-label={`${label} options`}
    popover="auto"
    ontoggle={(event) => {
      open = event.newState === 'open';
    }}
  >
    {#each options as option, index (option.value)}
      <button
        id={`${id}-${index}`}
        type="button"
        role="option"
        aria-selected={value === option.value}
        class={`hover:from-accent-bright hover:to-accent hover:text-on-accent focus-visible:from-accent-bright focus-visible:to-accent focus-visible:text-on-accent block w-full cursor-pointer rounded border-0 p-2 text-left text-xs hover:bg-linear-to-br focus-visible:bg-linear-to-br focus-visible:outline-none ${
          index === active
            ? 'from-accent-bright to-accent text-on-accent bg-linear-to-br'
            : value === option.value
              ? 'text-accent-bright bg-transparent'
              : 'text-muted bg-transparent'
        }`}
        tabindex="-1"
        onpointermove={() => (active = index)}
        onclick={() => choose(index)}
        onkeydown={keydown}
      >
        {option.label}
      </button>
    {/each}
  </div>
</div>
