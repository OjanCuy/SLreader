<!--
  O-06: confirms restoring a backup that replaces papers or highlights of the open
  library (FR-07.2). Focus starts on the safe button. Text approved in T-32.
-->
<script lang="ts">
  import type { RestorePreview } from "../lib/api";
  import { replaceMessage, RESTORE_CONFIRM, RESTORE_TITLE } from "../lib/backupTexts";
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";

  interface Props {
    preview: RestorePreview;
    busy: boolean;
    onconfirm: () => void;
    onclose: () => void;
  }

  let { preview, busy, onconfirm, onclose }: Props = $props();
</script>

<Dialog open title={RESTORE_TITLE} {busy} {onclose}>
  <p>{replaceMessage(preview)}</p>
  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Cancel</Button>
    <Button variant="primary" loading={busy} onclick={onconfirm}>{RESTORE_CONFIRM}</Button>
  {/snippet}
</Dialog>

<style>
  p {
    margin: 0;
  }
</style>
