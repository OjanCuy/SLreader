<script lang="ts">
  import type { HTMLButtonAttributes } from "svelte/elements";
  import Icon, { type IconName } from "./Icon.svelte";

  interface Props extends Omit<HTMLButtonAttributes, "class" | "title" | "aria-label"> {
    icon: IconName;
    /** Required: shown as the tooltip and read as the accessible name. */
    label: string;
    size?: 32 | 44;
    /** Set (true or false) only for on/off buttons; leave undefined otherwise. */
    toggled?: boolean;
  }

  let {
    icon,
    label,
    size = 32,
    toggled,
    type = "button",
    ...rest
  }: Props = $props();
</script>

<button
  {...rest}
  {type}
  class="icon-button"
  class:large={size === 44}
  aria-label={label}
  aria-pressed={toggled}
  title={label}
>
  <Icon name={icon} size={size === 44 ? 20 : 16} />
</button>

<style>
  .icon-button {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: var(--target-dense);
    height: var(--target-dense);
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-primary);
    cursor: pointer;
    transition: background-color var(--motion-fast);
  }

  .large {
    width: var(--target-primary);
    height: var(--target-primary);
  }

  .icon-button:hover {
    background: color-mix(in srgb, var(--text-primary) 8%, transparent);
  }

  .icon-button:active {
    background: color-mix(in srgb, var(--text-primary) 16%, transparent);
  }

  .icon-button[aria-pressed="true"] {
    border-color: var(--accent);
    background: var(--accent-subtle);
    color: var(--accent);
  }

  .icon-button:disabled {
    opacity: 0.5;
    cursor: default;
    pointer-events: none;
  }
</style>
