<script lang="ts">
  // Temporary start screen: proves the Rust bridge (T-02) and the theme and fonts (T-04).
  // Replaced by the S-01 Libraries screen in T-09.
  import { getAppInfo } from "./lib/api";
  import { applyTheme, THEME_OPTIONS, type ThemeChoice } from "./lib/theme";

  const info = getAppInfo();

  let theme = $state<ThemeChoice>("system");
  $effect(() => applyTheme(theme));
</script>

<main>
  {#await info then app}
    <h1>{app.name}</h1>
    <p class="version">{app.version}</p>
  {/await}

  <fieldset>
    <legend>Appearance</legend>
    {#each THEME_OPTIONS as option (option.value)}
      <label>
        <input type="radio" name="theme" value={option.value} bind:group={theme} />
        {option.label}
      </label>
    {/each}
  </fieldset>
</main>

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

  fieldset {
    display: inline-flex;
    gap: var(--space-4);
    margin: 0;
    padding: var(--panel-padding);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-md);
    background: var(--surface-panel);
  }

  legend {
    font: var(--type-section);
  }

  label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--target-dense);
  }
</style>
