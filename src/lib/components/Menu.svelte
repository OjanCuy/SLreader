<script lang="ts" module>
  import type { IconName } from "./Icon.svelte";

  export type MenuItem =
    | {
        label: string;
        icon?: IconName;
        /** Shown on the right, e.g. "Ctrl+U". The screen registers the actual key. */
        shortcut?: string;
        disabled?: boolean;
        onselect: () => void;
      }
    | { separator: true };
</script>

<script lang="ts">
  import { tick } from "svelte";
  import Button from "./Button.svelte";
  import Icon from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";

  interface Props {
    /** Text of the trigger button; with `iconOnly`, its tooltip and accessible name. */
    label: string;
    items: MenuItem[];
    icon?: IconName;
    iconOnly?: boolean;
    /** Which edge of the trigger the menu lines up with. */
    align?: "start" | "end";
    disabled?: boolean;
  }

  let {
    label,
    items,
    icon,
    iconOnly = false,
    align = "start",
    disabled = false,
  }: Props = $props();

  const id = $props.id();

  let open = $state(false);
  let root: HTMLDivElement;
  let list = $state<HTMLDivElement>();

  const trigger = () => root.querySelector<HTMLButtonElement>("button")!;

  const enabledItems = () =>
    Array.from(list?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? []);

  async function show(focus: "first" | "last") {
    open = true;
    await tick();
    const all = enabledItems();
    (focus === "first" ? all[0] : all.at(-1))?.focus();
  }

  function close(returnFocus = true) {
    open = false;
    if (returnFocus) trigger().focus();
  }

  function onTriggerKeydown(event: KeyboardEvent) {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    show(event.key === "ArrowDown" ? "first" : "last");
  }

  function onMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      // Only the menu closes, not a dialog around it.
      event.preventDefault();
      event.stopPropagation();
      close();
      return;
    }
    if (event.key === "Tab") {
      // Back to the trigger, then let Tab move on from there.
      close();
      return;
    }
    const all = enabledItems();
    const current = all.indexOf(document.activeElement as HTMLButtonElement);
    const next = {
      ArrowDown: (current + 1) % all.length,
      ArrowUp: (current <= 0 ? all.length : current) - 1,
      Home: 0,
      End: all.length - 1,
    }[event.key];
    if (next === undefined) return;
    event.preventDefault();
    all[next]?.focus();
  }

  const triggerProps = $derived({
    "aria-haspopup": "menu" as const,
    "aria-expanded": open,
    "aria-controls": open ? id : undefined,
    disabled,
    onclick: () => (open ? close() : show("first")),
    onkeydown: onTriggerKeydown,
  });
</script>

<svelte:window
  onpointerdown={(event) => {
    if (open && !root.contains(event.target as Node)) close(false);
  }}
/>

<div class="menu" bind:this={root}>
  {#if iconOnly}
    <IconButton {...triggerProps} {label} icon={icon ?? "more"} />
  {:else}
    <Button {...triggerProps} {icon}>
      {label}
      <Icon name="chevron-down" />
    </Button>
  {/if}

  {#if open}
    <div
      bind:this={list}
      {id}
      class="list {align}"
      role="menu"
      tabindex="-1"
      aria-label={label}
      onkeydown={onMenuKeydown}
    >
      {#each items as item, index (index)}
        {#if "separator" in item}
          <hr />
        {:else}
          <button
            type="button"
            role="menuitem"
            tabindex="-1"
            disabled={item.disabled}
            onclick={() => {
              // Return focus first, so a dialog opened by the item can restore it later.
              close();
              item.onselect();
            }}
          >
            <span class="icon">
              {#if item.icon}<Icon name={item.icon} />{/if}
            </span>
            <span class="label">{item.label}</span>
            {#if item.shortcut}<kbd>{item.shortcut}</kbd>{/if}
          </button>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .menu {
    position: relative;
    display: inline-block;
  }

  .list {
    position: absolute;
    top: calc(100% + var(--space-1));
    z-index: 10;
    min-width: 200px;
    padding: var(--space-1);
    border: var(--border-float);
    border-radius: var(--radius-md);
    background: var(--surface-panel);
    box-shadow: var(--shadow-float);
  }

  .start {
    left: 0;
  }

  .end {
    right: 0;
  }

  [role="menuitem"] {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: var(--target-dense);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    text-align: left;
    white-space: nowrap;
    cursor: pointer;
  }

  [role="menuitem"]:hover,
  [role="menuitem"]:focus-visible {
    background: color-mix(in srgb, var(--text-primary) 8%, transparent);
  }

  [role="menuitem"]:active {
    background: color-mix(in srgb, var(--text-primary) 16%, transparent);
  }

  [role="menuitem"]:focus-visible {
    outline-offset: calc(-1 * var(--focus-ring-width));
  }

  [role="menuitem"]:disabled {
    opacity: 0.5;
    cursor: default;
    pointer-events: none;
  }

  .icon {
    display: inline-flex;
    width: 16px;
  }

  .label {
    flex: 1;
  }

  kbd {
    color: var(--text-secondary);
    font: var(--type-caption);
  }

  hr {
    margin: var(--space-1) 0;
    border: 0;
    border-top: 1px solid var(--border-subtle);
  }
</style>
