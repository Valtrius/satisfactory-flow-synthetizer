<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  type Props = { children: Snippet; label: string; role?: 'menu' | 'dialog'; class?: string; onclose: () => void };
  let { children, label, role = 'dialog', class: className = '', onclose }: Props = $props();
  let root: HTMLDivElement;
  let opener: HTMLElement | null = null;
  onMount(() => {
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    (root.querySelector<HTMLElement>('button:not(:disabled), input:not(:disabled), [tabindex="0"]') ?? root).focus();
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.contains(event.target) && !opener?.contains(event.target)) onclose();
    };
    document.addEventListener('pointerdown', outside);
    return () => {
      document.removeEventListener('pointerdown', outside);
      if (opener?.isConnected && (root.contains(document.activeElement) || document.activeElement === document.body))
        opener.focus();
    };
  });
  function keydown(event: KeyboardEvent): void {
    event.stopPropagation();
    if (event.key === 'Escape') {
      event.preventDefault();
      onclose();
      return;
    }
    if (event.key === 'Tab' && role === 'menu') {
      onclose();
      return;
    }
    const menu = role === 'menu' ? root : (event.target as HTMLElement).closest('[role="listbox"]');
    if (!menu) return;
    const items = Array.from(menu.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
    if (items.length === 0) return;
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    let next: number;
    switch (event.key) {
      case 'ArrowDown':
        next = (index + 1) % items.length;
        break;
      case 'ArrowUp':
        next = (index - 1 + items.length) % items.length;
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = items.length - 1;
        break;
      default:
        return;
    }
    event.preventDefault();
    items[next].focus();
  }
</script>

<div
  bind:this={root}
  {role}
  aria-label={label}
  tabindex="-1"
  class={className}
  onkeydown={keydown}
  onclick={(event) => event.stopPropagation()}
  onpointerdown={(event) => event.stopPropagation()}
  onfocusout={(event) => {
    if (
      event.relatedTarget instanceof Node &&
      !root.contains(event.relatedTarget) &&
      !opener?.contains(event.relatedTarget)
    )
      onclose();
  }}
>
  {@render children()}
</div>
