<script lang="ts">
  type Props = {
    message: string;
  };

  let { message }: Props = $props();

  const title = $derived(/\bgraph\b/i.test(message) ? 'Could not update the graph.' : 'Could not finish the search.');
</script>

<section
  class="border-danger/50 bg-danger-bg text-danger-fg flex gap-2 rounded-tl-xl rounded-tr-sm rounded-br-xl rounded-bl-sm border px-4.5 py-3.5 text-sm"
  role="alert"
>
  <strong>{title}</strong>
  <span>{message}</span>
</section>
