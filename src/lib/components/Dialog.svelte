<script lang="ts">
  import { tick, type Snippet } from "svelte";

  interface Props {
    open: boolean;
    title: string;
    /** Buttons, shown bottom right. Put the primary one last. */
    actions: Snippet;
    children: Snippet;
    size?: "sm" | "md";
    /** While true the dialog cannot be closed and its controls are disabled. */
    busy?: boolean;
    /** Esc was pressed. The caller decides by setting `open` to false. */
    onclose: () => void;
  }

  let { open, title, actions, children, size = "sm", busy = false, onclose }: Props = $props();

  const id = $props.id();

  let dialog = $state<HTMLDialogElement>();

  // Open as a modal (Chromium traps focus inside it) and give focus back on close.
  $effect(() => {
    const element = dialog;
    if (!element) return;
    const opener = document.activeElement as HTMLElement | null;
    element.showModal();
    // The first field, or the element marked data-autofocus (the safe button of a
    // delete confirmation), otherwise the first button.
    const target =
      element.querySelector<HTMLElement>("[data-autofocus]") ??
      element.querySelector<HTMLElement>(".body :is(input, textarea, select)") ??
      element.querySelector<HTMLElement>("button");
    target?.focus();
    return () => {
      element.close();
      opener?.focus();
    };
  });

  // Disabled controls lose focus; keep it on the dialog so that Esc is still caught here.
  $effect(() => {
    if (busy) tick().then(() => dialog?.focus());
  });
</script>

{#if open}
  <dialog
    bind:this={dialog}
    class={size}
    tabindex="-1"
    aria-labelledby="{id}-title"
    aria-busy={busy || undefined}
    onkeydown={(event) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      event.preventDefault();
      if (!busy) onclose();
    }}
    oncancel={(event) => {
      event.preventDefault();
      if (!busy) onclose();
    }}
  >
    <h2 id="{id}-title">{title}</h2>
    <fieldset disabled={busy}>
      <div class="body">{@render children()}</div>
      <div class="actions">{@render actions()}</div>
    </fieldset>
  </dialog>
{/if}

<style>
  dialog {
    width: min(90vw, 400px);
    max-height: 90vh;
    overflow: auto;
    padding: var(--space-6);
    border: var(--border-float);
    border-radius: var(--radius-md);
    background: var(--surface-panel);
    color: var(--text-primary);
    box-shadow: var(--shadow-float);
  }

  .md {
    width: min(90vw, 560px);
  }

  dialog::backdrop {
    background: color-mix(in srgb, var(--surface-sunken) 75%, transparent);
  }

  h2 {
    margin: 0 0 var(--space-3);
    font: var(--type-display);
  }

  fieldset {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-6);
  }
</style>
