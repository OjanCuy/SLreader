<!--
  TabBar (Brief Section 8): tabs over a panel. Left and right arrows, Home, and End
  move between them. The panel uses `tabPanelId` and `tabId` to connect to its tab.
-->
<script lang="ts" module>
  export const tabId = (prefix: string, value: string) => `${prefix}-tab-${value}`;
  export const tabPanelId = (prefix: string) => `${prefix}-panel`;
</script>

<script lang="ts" generics="T extends string">
  interface Props {
    tabs: ReadonlyArray<{ value: T; label: string }>;
    value: T;
    /** Makes the ids unique on the page. */
    prefix: string;
  }

  let { tabs, value = $bindable(), prefix }: Props = $props();

  let bar: HTMLDivElement;

  function onkeydown(event: KeyboardEvent) {
    const current = tabs.findIndex((tab) => tab.value === value);
    const next = {
      ArrowRight: (current + 1) % tabs.length,
      ArrowLeft: (current + tabs.length - 1) % tabs.length,
      Home: 0,
      End: tabs.length - 1,
    }[event.key];
    if (next === undefined) return;
    event.preventDefault();
    value = tabs[next].value;
    bar.querySelectorAll("button")[next]?.focus();
  }
</script>

<div class="tabs" role="tablist" bind:this={bar}>
  {#each tabs as tab (tab.value)}
    <button
      type="button"
      role="tab"
      id={tabId(prefix, tab.value)}
      aria-selected={tab.value === value}
      aria-controls={tabPanelId(prefix)}
      tabindex={tab.value === value ? 0 : -1}
      onclick={() => (value = tab.value)}
      {onkeydown}
    >
      {tab.label}
    </button>
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    flex: none;
    gap: var(--space-1);
    padding: 0 var(--panel-padding);
    border-bottom: 1px solid var(--border-subtle);
  }

  button {
    min-height: var(--target-primary);
    padding: 0 var(--space-3);
    border: 0;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
  }

  button:hover {
    color: var(--text-primary);
  }

  /* Selected: underline and weight as well as color. */
  button[aria-selected="true"] {
    border-bottom-color: var(--accent);
    color: var(--accent);
    font-weight: 600;
  }

  button:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring);
    outline-offset: calc(-1 * var(--focus-ring-width));
  }
</style>
