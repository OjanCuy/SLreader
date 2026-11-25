<!--
  SummaryBar (FR-04.5; Brief Section 8): how many papers there are, per decision, and
  per exclusion reason. Reason chips show the code, with the description as tooltip.
  Decisions always have an icon and text, never color alone.
-->
<script lang="ts">
  import type { Summary } from "../lib/api";
  import Icon, { type IconName } from "../lib/components/Icon.svelte";

  interface Props {
    summary: Summary;
  }

  let { summary }: Props = $props();

  type Tone = "neutral" | "include" | "exclude" | "undecided";

  const totals = $derived<{ label: string; count: number; tone: Tone; icon?: IconName }[]>([
    { label: "Papers", count: summary.papers, tone: "neutral" },
    { label: "Include", count: summary.include, tone: "include", icon: "check-circle" },
    { label: "Exclude", count: summary.exclude, tone: "exclude", icon: "close" },
    { label: "Undecided", count: summary.undecided, tone: "undecided", icon: "minus" },
  ]);
</script>

<div class="summary">
  <ul>
    {#each totals as chip (chip.label)}
      <li class={chip.tone}>
        {#if chip.icon}<Icon name={chip.icon} size={12} />{/if}
        <span>{chip.label}</span>
        <span class="count">{chip.count}</span>
      </li>
    {/each}
  </ul>
  <ul>
    {#each summary.reasons as reason (reason.id)}
      <li title={reason.label}>
        <span>{reason.code}<span class="hidden">, {reason.label}</span></span>
        <span class="count">{reason.count}</span>
      </li>
    {/each}
    <li>
      <span>No reason</span>
      <span class="count">{summary.noReason}</span>
    </li>
  </ul>
</div>

<style>
  .summary {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-4);
    min-width: 0;
  }

  ul {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-2);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font: var(--type-caption);
    line-height: 22px;
    white-space: nowrap;
  }

  .include {
    color: var(--decision-include);
  }

  .exclude {
    color: var(--decision-exclude);
  }

  .undecided {
    color: var(--decision-undecided);
  }

  .count {
    color: var(--text-primary);
    font-variant-numeric: var(--type-numeric);
    font-weight: 600;
  }

  /* Read by screen readers; sighted users get the tooltip. */
  .hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
</style>
