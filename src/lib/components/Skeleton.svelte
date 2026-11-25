<script lang="ts">
  interface Props {
    shape?: "row" | "card";
    count?: number;
  }

  let { shape = "row", count = 1 }: Props = $props();
</script>

<!-- Placeholder only. The loading area around it sets aria-busy. -->
<div class="skeletons" aria-hidden="true">
  {#each { length: count }, index (index)}
    <div class="skeleton {shape}">
      <span></span>
      <span></span>
    </div>
  {/each}
</div>

<style>
  .skeletons {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .skeleton {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .card {
    padding: var(--space-4);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--surface-panel);
  }

  span {
    height: 12px;
    border-radius: var(--radius-sm);
    background: var(--border-subtle);
  }

  span:first-child {
    width: 80%;
  }

  span:last-child {
    width: 45%;
  }
</style>
