<!--
  Development only: every basic component and its states, for checking by eye.
  Sample texts come from the design brief. Not part of the released app.
-->
<script lang="ts">
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";
  import EmptyState from "../lib/components/EmptyState.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import Menu, { type MenuItem } from "../lib/components/Menu.svelte";
  import ProgressBar from "../lib/components/ProgressBar.svelte";
  import SegmentedControl from "../lib/components/SegmentedControl.svelte";
  import Select from "../lib/components/Select.svelte";
  import Skeleton from "../lib/components/Skeleton.svelte";
  import TextArea from "../lib/components/TextArea.svelte";
  import TextField from "../lib/components/TextField.svelte";
  import ToastRegion from "../lib/components/ToastRegion.svelte";
  import { showToast } from "../lib/components/toasts.svelte";

  let toggled = $state(false);
  let page = $state("4");
  let note = $state("");
  let reason = $state("");
  let pageMode = $state<"normal" | "dark" | "sepia">("normal");
  let dialog = $state<"none" | "idle" | "busy">("none");

  function openBusyDialog() {
    dialog = "busy";
    setTimeout(() => (dialog = "none"), 3000);
  }

  const menuItems: MenuItem[] = [
    { label: "Upload PDF", icon: "upload", shortcut: "Ctrl+U", onselect: () => showToast("Saved") },
    { label: "Import references", disabled: true, onselect: () => {} },
    { separator: true },
    { label: "Create library", onselect: () => (dialog = "idle") },
  ];
</script>

<section>
  <h2>Button</h2>
  {#each ["primary", "secondary", "ghost", "danger"] as const as variant (variant)}
    <div class="row">
      <Button {variant}>{variant}</Button>
      <Button {variant} size="lg" icon="upload">lg with icon</Button>
      <Button {variant} loading>loading</Button>
      <Button {variant} disabled>disabled</Button>
    </div>
  {/each}

  <h2>IconButton</h2>
  <div class="row">
    <IconButton icon="trash" label="Delete paper" />
    <IconButton icon="upload" label="Upload PDF" size={44} />
    <IconButton icon="info" label="toggled" {toggled} onclick={() => (toggled = !toggled)} />
    <IconButton icon="trash" label="disabled" disabled />
  </div>

  <h2>TextField</h2>
  <div class="column">
    <TextField label="Page" numeric suffix="/ 12" bind:value={page} />
    <TextField label="DOI" hint="hint" prefix="https://doi.org/" />
    <TextField label="No Ref" error="invalid" value="12" />
    <TextField label="File name" readonly value="read-only.pdf" />
    <TextField label="Title" disabled value="disabled" />
  </div>

  <h2>TextArea</h2>
  <div class="column">
    <TextArea label="Assessment notes" autoGrow rows={2} maxRows={6} bind:value={note} />
    <TextArea label="Research questions" error="invalid" />
    <TextArea label="Research questions" disabled value="disabled" />
  </div>

  <h2>Select</h2>
  <div class="column">
    <Select
      label="Exclusion reason"
      bind:value={reason}
      options={[
        { value: "", label: "No reason" },
        { value: "e1", label: "E1" },
        { value: "e2", label: "disabled", disabled: true },
      ]}
    />
    <Select label="Tag" disabled options={[{ value: "", label: "No tag" }]} />
  </div>

  <h2>SegmentedControl</h2>
  <div class="row">
    <SegmentedControl
      label="Page mode"
      bind:value={pageMode}
      options={[
        { value: "normal", label: "Normal" },
        { value: "dark", label: "Dark" },
        { value: "sepia", label: "Sepia" },
      ]}
    />
    <SegmentedControl
      label="CSV separator"
      disabled
      value="semicolon"
      options={[
        { value: "semicolon", label: "Semicolon" },
        { value: "comma", label: "Comma" },
      ]}
    />
  </div>

  <h2>Menu and Dialog</h2>
  <div class="row">
    <Menu label="Export" items={menuItems} />
    <Menu label="More" iconOnly items={menuItems} />
    <Button onclick={() => (dialog = "idle")}>Open dialog</Button>
    <Button onclick={openBusyDialog}>Open busy dialog (closes after 3 s)</Button>
  </div>

  <h2>Toast</h2>
  <div class="row">
    <Button onclick={() => showToast("Saved")}>neutral</Button>
    <Button onclick={() => showToast("3 PDFs added.", "success")}>success</Button>
    <Button onclick={() => showToast("Could not open name.pdf.", "error")}>error</Button>
  </div>

  <h2>InlineBanner</h2>
  <div class="column">
    <InlineBanner tone="info" title="Import references to link this paper." />
    <InlineBanner tone="warning" title="3 notes link to #12. Update them to #14?" />
    <InlineBanner tone="error" title="Couldn't save: the disk is full or the folder is read-only.">
      {#snippet actions()}
        <Button>Try again</Button>
      {/snippet}
    </InlineBanner>
  </div>

  <h2>EmptyState</h2>
  <EmptyState size="display" title="No libraries yet. Create one to start a review.">
    {#snippet action()}
      <Button variant="primary" size="lg">Create library</Button>
    {/snippet}
  </EmptyState>
  <EmptyState title="No papers yet." description="or drop PDF files here">
    {#snippet action()}
      <Button>Upload PDF</Button>
    {/snippet}
  </EmptyState>

  <h2>ProgressBar</h2>
  <div class="column">
    <ProgressBar label="Indexing 42 of 80 papers" value={42} max={80} />
    <ProgressBar label="Extracting text…" indeterminate />
    <ProgressBar label="paused" value={42} max={80} paused />
  </div>

  <h2>Skeleton</h2>
  <div class="column">
    <Skeleton count={2} />
    <Skeleton shape="card" />
  </div>
</section>

<Dialog
  open={dialog !== "none"}
  title="Create library"
  busy={dialog === "busy"}
  onclose={() => (dialog = "none")}
>
  <TextField label="Library name" />
  {#snippet actions()}
    <Button onclick={() => (dialog = "none")}>Cancel</Button>
    <Button variant="primary" loading={dialog === "busy"} onclick={() => (dialog = "none")}>
      Create library
    </Button>
  {/snippet}
</Dialog>

<ToastRegion />

<style>
  section {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-3);
    max-width: 720px;
    padding: 0 var(--space-8) var(--space-12);
  }

  h2 {
    margin: var(--space-4) 0 0;
    font: var(--type-section);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
  }

  .column {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    max-width: 380px;
  }
</style>
