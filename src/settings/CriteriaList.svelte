<!--
  CriteriaList / CriterionRow (Brief S-04): one of the three lists of the library, with
  a drag handle, code and description (a tag has a name only), delete, and "Add".
  Every delete of a saved row is confirmed (O-10). The handle also moves its row with
  the arrow keys.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";
  import type { CriteriaKind } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import TextField from "../lib/components/TextField.svelte";
  import { showToast } from "../lib/components/toasts.svelte";
  import { GENERIC_ERROR } from "../lib/libraryTexts";
  import type { Row, SettingsStore } from "./settingsStore.svelte";
  import { DELETE_TITLES, deleteMessage, repeatedCodes, REQUIRED } from "./settingsTexts";

  interface Props {
    store: SettingsStore;
    kind: CriteriaKind;
    readOnly: boolean;
  }

  let { store, kind, readOnly }: Props = $props();

  const list = $derived(store.lists[kind]);
  const repeated = $derived(kind === "tag" ? [] : repeatedCodes(list.rows.map((row) => row.code)));

  // Untracked: a failed load must not start itself again.
  $effect(() => {
    const shown = kind;
    untrack(() => store.load(shown));
  });

  let items = $state<HTMLElement>();

  /** An empty required field, once it was saved before or the user has left it. */
  const missing = (row: Row, field: "code" | "label") =>
    row[field].trim() === "" && (row.stored !== null || row.touched[field]) ? REQUIRED : undefined;

  async function add() {
    store.add(kind);
    await tick();
    // The description of the new row; its code is already filled in.
    items?.querySelector<HTMLInputElement>("li:last-child .label input")?.focus();
  }

  // ---- Delete ----

  let confirming = $state.raw<{ row: Row; message: string } | null>(null);
  let deleting = $state(false);
  let asking = false;

  async function askToDelete(row: Row) {
    if (asking) return;
    asking = true;
    try {
      const uses = await store.askToRemove(kind, row);
      if (uses === null) return;
      const name = (kind === "tag" ? row.stored?.label : row.stored?.code) ?? "";
      confirming = { row, message: deleteMessage(kind, name, uses) };
    } catch {
      showToast(GENERIC_ERROR, "error");
    } finally {
      asking = false;
    }
  }

  async function confirmDelete() {
    if (!confirming || deleting) return;
    deleting = true;
    try {
      await store.remove(kind, confirming.row);
    } finally {
      deleting = false;
      confirming = null;
    }
  }

  // ---- Reorder ----

  async function moveByKey(event: KeyboardEvent, row: Row, index: number) {
    const by = event.key === "ArrowUp" ? -1 : event.key === "ArrowDown" ? 1 : 0;
    if (by === 0) return;
    event.preventDefault();
    const handle = event.currentTarget as HTMLElement;
    if (!store.move(kind, row, index + by)) return;
    store.saveOrder(kind);
    // Moving the row in the page takes the focus away.
    await tick();
    handle.focus();
  }

  let dragging = $state<number | null>(null);
  let stopDrag: (() => void) | undefined;

  // Pointer events: the web view's own drag and drop is taken by the file drop (T-12).
  function startDrag(event: PointerEvent, row: Row) {
    if (event.button !== 0) return;
    event.preventDefault();
    stopDrag?.();
    dragging = row.key;
    let moved = false;
    const onmove = (move: PointerEvent) => {
      // The row goes after every other row whose middle is above the pointer.
      const others = [...(items?.children ?? [])].filter(
        (item) => item.getAttribute("data-key") !== String(row.key),
      );
      const to = others.filter((item) => {
        const box = item.getBoundingClientRect();
        return box.top + box.height / 2 < move.clientY;
      }).length;
      if (store.move(kind, row, to)) moved = true;
    };
    const finish = () => {
      stopDrag?.();
      if (moved) store.saveOrder(kind);
    };
    window.addEventListener("pointermove", onmove);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
    stopDrag = () => {
      window.removeEventListener("pointermove", onmove);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
      dragging = null;
      stopDrag = undefined;
    };
  }

  $effect(() => () => stopDrag?.());
</script>

{#if kind === "tag"}
  <p class="note">Order sets export columns</p>
{/if}

{#if list.status === "error"}
  <p class="problem" role="alert"><Icon name="error" />{GENERIC_ERROR}</p>
{:else if list.status === "ready"}
  {#each repeated as code (code)}
    <InlineBanner tone="warning" title="Code {code} is used more than once." />
  {/each}

  {#if list.rows.length === 0}
    <p class="note">{kind === "tag" ? "No tags yet." : "No criteria yet."}</p>
  {:else}
    <ul bind:this={items}>
      {#each list.rows as row, index (row.key)}
        <li data-key={row.key} class:dragging={dragging === row.key}>
          {#if !readOnly}
            <button
              type="button"
              class="handle"
              aria-label="Reorder"
              title="Reorder"
              disabled={row.id === null}
              onpointerdown={(event) => startDrag(event, row)}
              onkeydown={(event) => moveByKey(event, row, index)}
            >
              <Icon name="drag" />
            </button>
          {/if}
          {#if kind !== "tag"}
            <div class="code">
              <TextField
                label="Code"
                value={row.code}
                error={missing(row, "code")}
                readonly={readOnly}
                oninput={(event) => store.edit(kind, row, "code", event.currentTarget.value)}
                onblur={() => store.leave(row, "code")}
              />
            </div>
          {/if}
          <div class="label">
            <TextField
              label={kind === "tag" ? "Tag name" : "Description"}
              value={row.label}
              error={missing(row, "label")}
              readonly={readOnly}
              oninput={(event) => store.edit(kind, row, "label", event.currentTarget.value)}
              onblur={() => store.leave(row, "label")}
            />
          </div>
          {#if !readOnly}
            <div class="remove">
              <IconButton icon="trash" label="Delete" onclick={() => askToDelete(row)} />
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if !readOnly}
    <div><Button icon="plus" onclick={add}>Add</Button></div>
  {/if}
{/if}

{#if confirming}
  <Dialog open title={DELETE_TITLES[kind]} busy={deleting} onclose={() => (confirming = null)}>
    <p class="message">{confirming.message}</p>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (confirming = null)}>Cancel</Button>
      <Button variant="danger" loading={deleting} onclick={confirmDelete}>Delete</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  ul {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--surface-panel);
  }

  li.dragging {
    border-color: var(--accent);
    background: var(--accent-subtle);
  }

  .code {
    flex: none;
    width: 96px;
  }

  .label {
    flex: 1;
    min-width: 0;
  }

  /* Level with the inputs, below their labels. */
  .handle,
  .remove {
    align-self: flex-start;
    margin-top: var(--space-6);
  }

  .handle {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: var(--target-dense);
    height: var(--target-dense);
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-secondary);
    cursor: grab;
    touch-action: none;
  }

  .handle:hover:not(:disabled) {
    background: var(--surface-sunken);
  }

  .handle:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring);
  }

  .handle:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .dragging .handle {
    cursor: grabbing;
  }

  .note {
    margin: 0;
    color: var(--text-secondary);
  }

  .message {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .problem {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--status-error);
  }
</style>
