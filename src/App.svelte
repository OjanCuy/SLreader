<script lang="ts">
  // Temporary start screen: proves the Rust bridge (T-02) and the theme and fonts (T-04).
  // Replaced by the S-01 Libraries screen in T-09.
  // In development it also shows the component gallery (T-05).
  import { getAppInfo } from "./lib/api";
  import SegmentedControl from "./lib/components/SegmentedControl.svelte";
  import { applyTheme, THEME_OPTIONS, type ThemeChoice } from "./lib/theme";

  const info = getAppInfo();

  // Dropped from release builds, because import.meta.env.DEV is false there.
  const gallery = import.meta.env.DEV ? import("./gallery/Gallery.svelte") : null;

  let theme = $state<ThemeChoice>("system");
  $effect(() => applyTheme(theme));
</script>

<main>
  {#await info then app}
    <h1>{app.name}</h1>
    <p class="version">{app.version}</p>
  {/await}

  <SegmentedControl label="Appearance" showLabel options={THEME_OPTIONS} bind:value={theme} />
</main>

{#if gallery}
  {#await gallery then { default: Gallery }}
    <Gallery />
  {/await}
{/if}

<style>
  main {
    padding: var(--space-8);
  }

  h1 {
    margin: 0;
    font: var(--type-paper-title);
  }

  .version {
    margin: var(--space-1) 0 var(--space-6);
    color: var(--text-secondary);
    font-variant-numeric: var(--type-numeric);
  }
</style>
