<!-- Top bar menu: the open library's name, the other libraries, and the way back (FR-14.2). -->
<script lang="ts">
  import { onMount } from "svelte";
  import { listLibraries, openLibrary, type LibrarySummary } from "../lib/api";
  import Menu, { type MenuItem } from "../lib/components/Menu.svelte";
  import { showToast } from "../lib/components/toasts.svelte";
  import { LIBRARY_PROBLEMS } from "../lib/libraryProblems";
  import { libraryErrorMessage, libraryTitle } from "../lib/libraryTexts";
  import CreateLibraryDialog from "./CreateLibraryDialog.svelte";

  interface Props {
    current: LibrarySummary;
    /** Another library was opened and is now the active one. */
    onswitch: (library: LibrarySummary) => void;
    onshowall: () => void;
  }

  let { current, onswitch, onshowall }: Props = $props();

  let libraries = $state<LibrarySummary[]>([]);
  let creating = $state(false);
  let switching = false;

  onMount(refresh);

  async function refresh() {
    try {
      libraries = await listLibraries();
    } catch (failure) {
      showToast(libraryErrorMessage(failure, "load"), "error");
    }
  }

  async function switchTo(library: LibrarySummary) {
    if (library.id === current.id || switching) return;
    switching = true;
    try {
      onswitch(await openLibrary(library.id));
    } catch (failure) {
      // The current library stays open.
      showToast(libraryErrorMessage(failure, "load"), "error");
      await refresh();
    } finally {
      switching = false;
    }
  }

  // The folder of the open library can disappear while the app is running.
  const currentMissing = $derived(
    libraries.some((library) => library.id === current.id && library.problem === "not_found"),
  );

  const items = $derived<MenuItem[]>([
    ...libraries.map((library): MenuItem => {
      const problem = library.problem ? LIBRARY_PROBLEMS[library.problem] : null;
      const blocked = problem !== null && !problem.readOnly;
      return {
        label: libraryTitle(library),
        checked: library.id === current.id,
        // "All libraries" offers the way to repair it.
        disabled: blocked,
        hint: blocked ? problem.message : undefined,
        hintIcon: blocked ? "error" : undefined,
        onselect: () => switchTo(library),
      };
    }),
    { separator: true },
    { label: "Create library", onselect: () => (creating = true) },
    { label: "All libraries", onselect: () => onshowall() },
  ]);
</script>

<Menu
  label={libraryTitle(current)}
  icon={currentMissing ? "error" : undefined}
  {items}
  onopen={refresh}
/>

{#if creating}
  <CreateLibraryDialog
    onclose={() => (creating = false)}
    oncreated={(library) => {
      creating = false;
      onswitch(library);
    }}
  />
{/if}
