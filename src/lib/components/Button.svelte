<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";
  import Icon, { type IconName } from "./Icon.svelte";
  import Spinner from "./Spinner.svelte";

  interface Props extends Omit<HTMLButtonAttributes, "class"> {
    variant?: "primary" | "secondary" | "ghost" | "danger";
    size?: "md" | "lg";
    icon?: IconName;
    loading?: boolean;
    children: Snippet;
  }

  let {
    variant = "secondary",
    size = "md",
    icon,
    loading = false,
    disabled = false,
    type = "button",
    onclick,
    children,
    ...rest
  }: Props = $props();
</script>

<!-- A loading button stays focusable, but ignores clicks (also for type="submit"). -->
<button
  {...rest}
  {type}
  {disabled}
  class="button {variant} {size}"
  aria-busy={loading || undefined}
  aria-disabled={loading || undefined}
  onclick={(event) => {
    if (loading) event.preventDefault();
    else onclick?.(event);
  }}
>
  {#if loading}
    <Spinner />
  {:else if icon}
    <Icon name={icon} />
  {/if}
  {@render children()}
</button>

<style>
  .button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    font: var(--type-body);
    font-weight: 500;
    cursor: pointer;
    transition:
      background-color var(--motion-fast),
      border-color var(--motion-fast);
  }

  .md {
    min-height: var(--target-dense);
  }

  .lg {
    min-height: var(--target-primary);
    padding: 0 var(--space-4);
  }

  .primary {
    background: var(--accent);
    color: var(--accent-on);
  }

  .primary:hover {
    background: color-mix(in srgb, var(--accent) 88%, var(--text-primary));
  }

  .primary:active {
    background: color-mix(in srgb, var(--accent) 76%, var(--text-primary));
  }

  .secondary {
    border-color: var(--border-control);
    background: var(--surface-panel);
    color: var(--text-primary);
  }

  .ghost {
    background: transparent;
    color: var(--text-primary);
  }

  .danger {
    border-color: var(--border-control);
    background: var(--surface-panel);
    color: var(--status-error);
  }

  .secondary:hover,
  .ghost:hover,
  .danger:hover {
    background: color-mix(in srgb, var(--text-primary) 7%, var(--surface-panel));
  }

  .secondary:active,
  .ghost:active,
  .danger:active {
    background: color-mix(in srgb, var(--text-primary) 14%, var(--surface-panel));
  }

  .button:disabled {
    opacity: 0.5;
    cursor: default;
    pointer-events: none;
  }

  .button[aria-busy="true"] {
    cursor: progress;
  }
</style>
