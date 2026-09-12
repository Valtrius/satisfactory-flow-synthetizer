<script lang="ts">
  import CircleHelp from '@lucide/svelte/icons/circle-help';
  import Button from './Button.svelte';
  import Popup from './Popup.svelte';
  type Props = {
    label: string;
    sections: readonly { title: string; items: readonly string[]; tone?: 'default' | 'warning' }[];
    align?: 'left' | 'right';
    /** Parent anchoring uses the nearest positioned ancestor and constrains the popup to it. */
    anchor?: 'trigger' | 'parent';
    class?: string;
    popupClass?: string;
  };
  let { label, sections, align = 'left', anchor = 'trigger', class: className = '', popupClass = '' }: Props = $props();
  let open = $state(false);
</script>

<div class={`${anchor === 'parent' ? 'static' : 'relative'} ${className}`}>
  <Button
    size="tiny"
    square
    variant="quiet"
    title={label}
    aria-label={label}
    aria-haspopup="dialog"
    aria-expanded={open}
    onclick={(event) => {
      event.stopPropagation();
      event.currentTarget.focus();
      open = !open;
    }}
  >
    <CircleHelp size={16} strokeWidth={2.2} aria-hidden="true" />
  </Button>
  {#if open}
    <Popup
      {label}
      onclose={() => {
        open = false;
      }}
      class={`border-line bg-panel absolute top-[calc(100%+0.35rem)] z-30 max-h-[min(36rem,80dvh)] w-[min(28rem,calc(100vw-2rem))] overflow-y-auto rounded-lg border px-4 py-3.5 shadow-[0_14px_32px_rgb(0_0_0/45%)] ${align === 'right' ? 'right-0' : 'left-0'} ${anchor === 'parent' ? 'max-w-full' : ''} ${popupClass}`}
    >
      <div class="flex flex-col gap-3.5 text-xs leading-relaxed">
        {#each sections as section}
          <section>
            <h4
              class={`m-0 mb-1.5 text-xs font-bold tracking-wide uppercase ${section.tone === 'warning' ? 'text-warning' : 'text-ink'}`}
            >
              {section.title}
            </h4>
            <ul
              class={`m-0 list-disc space-y-1.5 pl-4 ${section.tone === 'warning' ? 'text-warning/90' : 'text-muted'}`}
            >
              {#each section.items as item}<li>{item}</li>{/each}
            </ul>
          </section>
        {/each}
      </div>
    </Popup>
  {/if}
</div>
