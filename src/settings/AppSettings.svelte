<!--
  The sections of the "App" group of S-04: the CSV separator under "Export" (FR-08.3)
  and the theme under "Appearance" (FR-08.5). Labels approved in T-37.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import SegmentedControl from "../lib/components/SegmentedControl.svelte";
  import { GENERIC_ERROR } from "../lib/libraryTexts";
  import { THEME_OPTIONS } from "../lib/theme";
  import type { AppSettingsStore } from "./appSettingsStore.svelte";
  import { DELIMITER_LABEL, DELIMITER_OPTIONS, THEME_LABEL, type AppSection } from "./settingsTexts";

  interface Props {
    store: AppSettingsStore;
    section: AppSection;
  }

  let { store, section }: Props = $props();

  onMount(() => {
    store.load();
  });
</script>

{#if store.status === "error"}
  <InlineBanner tone="error" title={GENERIC_ERROR} />
{:else if store.status === "ready"}
  {#if section === "export"}
    <SegmentedControl
      label={DELIMITER_LABEL}
      showLabel
      options={DELIMITER_OPTIONS}
      value={store.delimiter}
      onchange={store.chooseDelimiter}
    />
  {:else}
    <SegmentedControl
      label={THEME_LABEL}
      showLabel
      options={THEME_OPTIONS}
      value={store.theme}
      onchange={store.chooseTheme}
    />
  {/if}
{/if}
