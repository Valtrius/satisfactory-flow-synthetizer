<script lang="ts">
  let {
    title = 'Your topology graph will appear here.',
    description = 'Start with the sample 120 → 60 + 60 split, or enter your own exact rates.',
  }: { title?: string; description?: string } = $props();
</script>

<section class="text-muted bg-panel grid min-h-0 flex-1 place-items-center p-4 text-center">
  <span class="col-start-1 row-start-1 self-end text-5xl text-[#365565]" aria-hidden="true">⌁</span>
  <h2 class="col-start-1 row-start-2 m-0 self-center text-xl text-[#b7c7ce]">{title}</h2>
  <p class="col-start-1 row-start-3 m-0 max-w-125 self-start text-sm">
    {description}
  </p>
</section>
