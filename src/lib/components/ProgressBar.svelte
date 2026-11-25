<script lang="ts">
  interface Props {
    /** Visible text and accessible name, e.g. "Indexing 42 of 80 papers". */
    label: string;
    value?: number;
    max?: number;
    /** Unknown duration: `value` is ignored. */
    indeterminate?: boolean;
    paused?: boolean;
  }

  let { label, value = 0, max = 100, indeterminate = false, paused = false }: Props = $props();

  const id = $props.id();

  const clamped = $derived(Math.min(Math.max(value, 0), max));
  const percent = $derived(max > 0 ? (clamped / max) * 100 : 0);
</script>

<div class="progress" class:paused>
  <span class="label" {id}>{label}</span>
  <div
    class="track"
    role="progressbar"
    aria-labelledby={id}
    aria-valuemin="0"
    aria-valuemax={max}
    aria-valuenow={indeterminate ? undefined : clamped}
  >
    <div class="fill" class:indeterminate style:width={indeterminate ? undefined : `${percent}%`}></div>
  </div>
</div>

<style>
  .progress {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .label {
    color: var(--text-secondary);
    font: var(--type-caption);
    font-variant-numeric: var(--type-numeric);
  }

  .track {
    height: 4px;
    overflow: hidden;
    border-radius: var(--radius-full);
    background: var(--border-subtle);
  }

  .fill {
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
    transition: width var(--motion-panel);
  }

  .paused .fill {
    background: var(--text-secondary);
  }

  .indeterminate {
    width: 35%;
    animation: slide 1400ms ease-in-out infinite;
  }

  .paused .indeterminate {
    animation-play-state: paused;
  }

  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(290%);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .indeterminate {
      animation: none;
    }
  }
</style>
