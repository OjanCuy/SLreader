<!--
  Asks for the password of a password-protected PDF (PRD Section 8, T-19).
  The password lives only in this field and is handed to the reader on "Open";
  it is never stored. The text was approved in T-19.
-->
<script lang="ts">
  import { tick } from "svelte";
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";
  import TextField from "../lib/components/TextField.svelte";

  interface Props {
    /** The PDF is being opened with the password. */
    busy: boolean;
    /** The last password did not open the PDF. */
    wrong: boolean;
    onopen: (password: string) => void;
    /** "Skip", or Esc. */
    onskip: () => void;
  }

  let { busy, wrong, onopen, onskip }: Props = $props();

  let password = $state("");
  let body = $state<HTMLElement>();

  function open() {
    if (password === "" || busy) return;
    onopen(password);
  }

  // A wrong password is cleared, and the field gets focus back for the next try.
  $effect(() => {
    if (!wrong || busy) return;
    password = "";
    tick().then(() => body?.querySelector("input")?.focus());
  });
</script>

<Dialog open title="Enter password" {busy} onclose={onskip}>
  <div class="body" bind:this={body}>
    <p>This PDF is password-protected.</p>
    <TextField
      label="Password"
      type="password"
      autocomplete="off"
      bind:value={password}
      error={wrong ? "Wrong password. Try again." : undefined}
      onkeydown={(event) => {
        if (event.key === "Enter") open();
      }}
    />
  </div>
  {#snippet actions()}
    <Button onclick={onskip}>Skip</Button>
    <Button variant="primary" disabled={password === ""} loading={busy} onclick={open}>Open</Button>
  {/snippet}
</Dialog>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  p {
    margin: 0;
  }
</style>
