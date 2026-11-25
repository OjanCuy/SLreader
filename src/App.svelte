<script lang="ts">
  // Temporary start screen for T-02: proves the UI can call a Rust command.
  // Replaced by the S-01 Libraries screen in T-09.
  import { getAppInfo } from "./lib/api";

  const info = getAppInfo();
</script>

<main>
  {#await info then app}
    <h1>{app.name}</h1>
    <p>{app.version}</p>
  {/await}
</main>
