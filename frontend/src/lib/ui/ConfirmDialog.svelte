<script lang="ts">
  import { onMount } from 'svelte';
  import Button from './Button.svelte';
  type Props = {
    title: string;
    description: string;
    confirmLabel?: string;
    onconfirm: () => void;
    onclose: () => void;
  };
  let { title, description, confirmLabel = 'Delete', onconfirm, onclose }: Props = $props();
  const id = $props.id();
  let dialog: HTMLDialogElement;
  onMount(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.showModal();
    dialog.querySelector<HTMLButtonElement>('button')?.focus();
    return () => {
      dialog.close();
      if (opener?.isConnected) opener.focus();
    };
  });
</script>

<dialog
  bind:this={dialog}
  aria-labelledby={id}
  aria-describedby={`${id}-description`}
  class="border-line bg-panel text-ink m-auto w-[calc(100%-2rem)] max-w-md rounded-xl border p-5 shadow-[0_24px_48px_rgb(0_0_0/55%)]"
  oncancel={(event) => {
    event.preventDefault();
    onclose();
  }}
>
  <h3 {id} class="m-0 text-base font-bold tracking-tight">{title}</h3>
  <p id={`${id}-description`} class="text-muted mt-2 mb-0 text-sm leading-relaxed">{description}</p>
  <div class="mt-5 flex justify-end gap-2">
    <Button size="small" onclick={onclose}>Cancel</Button>
    <Button size="small" variant="danger" onclick={onconfirm}>{confirmLabel}</Button>
  </div>
</dialog>

<style>
  dialog::backdrop {
    background: rgb(4 10 15 / 70%);
  }
</style>
