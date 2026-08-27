<script lang="ts">
  type Props = {
    checked: boolean;
    disabled?: boolean;
    label: string;
    onclick: () => void;
  };

  let { checked, disabled = false, label, onclick }: Props = $props();
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  class={`relative mt-0.5 h-5 w-9 shrink-0 rounded-full border transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
    checked ? 'border-[#f07831] bg-accent' : 'border-[#456171] bg-[#314953]'
  }`}
  {disabled}
  {onclick}
>
  <span
    class={`pointer-events-none absolute top-1/2 size-3.5 -translate-y-1/2 rounded-full bg-[#eaf1f5] shadow-sm transition-[left] duration-150 ${
      checked ? 'left-[calc(100%-0.975rem)]' : 'left-0.5'
    }`}
  ></span>
</button>
