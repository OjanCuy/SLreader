<!--
  O-10: offers to update the note links after a reference number changed (PRD Section
  8). Focus starts on the button that changes nothing. Text approved in T-27.
-->
<script lang="ts">
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";
  import { offerMessage } from "../lib/refLinks";

  interface Props {
    count: number;
    from: number;
    to: number;
    busy: boolean;
    onconfirm: () => void;
    onclose: () => void;
  }

  let { count, from, to, busy, onconfirm, onclose }: Props = $props();
</script>

<Dialog open title="Update note links?" {busy} {onclose}>
  <p>{offerMessage(count, from, to)}</p>
  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Keep as is</Button>
    <Button variant="primary" loading={busy} onclick={onconfirm}>Update links</Button>
  {/snippet}
</Dialog>

<style>
  p {
    margin: 0;
  }
</style>
