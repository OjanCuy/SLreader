<!--
  O-10: confirms deleting a paper (FR-01.7). Focus starts on the safe button.
  The text was approved in T-15.
-->
<script lang="ts">
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";

  interface Props {
    /** The paper's file name, or its title when it has none. */
    name: string;
    busy: boolean;
    onconfirm: () => void;
    onclose: () => void;
  }

  let { name, busy, onconfirm, onclose }: Props = $props();
</script>

<Dialog open title="Delete paper?" {busy} {onclose}>
  <p>
    {name} and its PDF file, decision, notes, highlights, chats, and AI suggestions will be
    permanently deleted. This can't be undone.
  </p>
  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Cancel</Button>
    <Button variant="danger" loading={busy} onclick={onconfirm}>Delete paper</Button>
  {/snippet}
</Dialog>

<style>
  p {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
