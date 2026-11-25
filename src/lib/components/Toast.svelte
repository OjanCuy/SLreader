<script lang="ts">
  import Button from "./Button.svelte";
  import Icon from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";
  import type { ToastAction, ToastTone } from "./toasts.svelte";

  interface Props {
    tone?: ToastTone;
    message: string;
    actions?: ToastAction[];
    onclose: () => void;
  }

  let { tone = "neutral", message, actions, onclose }: Props = $props();

  const ICONS = { neutral: "info", success: "check-circle", error: "error" } as const;
</script>

<div class="toast {tone}">
  <span class="icon"><Icon name={ICONS[tone]} /></span>
  <span class="message">{message}</span>
  {#if actions}
    <span class="actions">
      {#each actions as action (action.label)}
        <Button
          onclick={() => {
            onclose();
            action.onselect();
          }}
        >
          {action.label}
        </Button>
      {/each}
    </span>
  {/if}
  {#if tone === "error"}
    <IconButton icon="close" label="Close" onclick={onclose} />
  {/if}
</div>

<style>
  .toast {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--target-primary);
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-3);
    /* A solid border in both themes keeps the toast visible over any content. */
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--surface-panel);
    box-shadow: var(--shadow-float);
    pointer-events: auto;
  }

  .message {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    width: 100%;
    padding-bottom: var(--space-1);
  }

  .icon {
    display: inline-flex;
    color: var(--text-secondary);
  }

  .success .icon {
    color: var(--status-success);
  }

  .error {
    border-color: var(--status-error);
  }

  .error .icon {
    color: var(--status-error);
  }
</style>
