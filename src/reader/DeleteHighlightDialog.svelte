<!--
  O-10: confirms deleting a highlight (FR-03.3). Focus starts on the safe button.
  The text was approved in T-21.
-->
<script lang="ts">
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";

  interface Props {
    page: number;
    busy: boolean;
    onconfirm: () => void;
    onclose: () => void;
  }

  let { page, busy, onconfirm, onclose }: Props = $props();
</script>

<Dialog open title="Delete highlight?" {busy} {onclose}>
  <p>
    This highlight on page {page} and its note will be permanently deleted. This can't be undone.
  </p>
  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Cancel</Button>
    <Button variant="danger" loading={busy} onclick={onconfirm}>Delete highlight</Button>
  {/snippet}
</Dialog>

<style>
  p {
    margin: 0;
  }
</style>
