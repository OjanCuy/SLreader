<script lang="ts">
  import type { LibrarySummary } from "./lib/api";
  import Button from "./lib/components/Button.svelte";
  import ToastRegion from "./lib/components/ToastRegion.svelte";
  import LibrariesScreen from "./screens/LibrariesScreen.svelte";
  import WorkspacePlaceholder from "./screens/WorkspacePlaceholder.svelte";

  /** The open library; the Libraries screen is shown while there is none. */
  let current = $state<LibrarySummary>();

  // Component gallery (T-05), development only: import.meta.env.DEV is false in releases.
  const gallery = import.meta.env.DEV ? () => import("./gallery/Gallery.svelte") : null;
  let showGallery = $state(false);
</script>

{#if current}
  <!-- A new session means another library: mount again so no screen state carries over. -->
  {#key current.session}
    <WorkspacePlaceholder
      library={current}
      onswitch={(library) => (current = library)}
      onshowall={() => (current = undefined)}
    />
  {/key}
{:else}
  <LibrariesScreen onopen={(library) => (current = library)} />
{/if}

{#if gallery}
  <div class="dev">
    <Button variant="ghost" onclick={() => (showGallery = !showGallery)}>
      Component gallery (dev)
    </Button>
  </div>
  {#if showGallery}
    {#await gallery() then { default: Gallery }}
      <Gallery />
    {/await}
  {/if}
{/if}

<ToastRegion />

<style>
  .dev {
    max-width: 720px;
    margin: var(--space-12) auto var(--space-3);
    padding: 0 var(--panel-padding);
  }
</style>
