<script lang="ts">
  type Props = {
    message: string;
  };

  let { message }: Props = $props();

  const title = $derived(
    /\bgraph\b/i.test(message)
      ? 'Could not update the graph.'
      : 'Could not finish the search.'
  );
</script>

<section
  class="my-4 flex gap-2 rounded-tl-xl rounded-tr-sm rounded-br-xl rounded-bl-sm border border-danger/50 bg-[#5c1a17]/35 px-4.5 py-3.5 text-sm text-[#ffaaa4]"
  role="alert"
>
  <strong>{title}</strong>
  <span>{message}</span>
</section>
