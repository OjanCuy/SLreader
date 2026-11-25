<!--
  S-02 Reader (FR-02.1): opens the selected paper's PDF at the page it was last read
  at (FR-02.3), and keeps the page mode (FR-02.2).
  States from Brief Section 7: "Select a paper from the list.", "Loading PDF…",
  "PDF file is missing." + "Attach PDF", "This PDF can't be opened.".
  Password-protected PDFs get their own prompt in T-19; until then they cannot be opened.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    attachPdf,
    getLastPage,
    getPageMode,
    pickPdf,
    readPaperPdf,
    saveLastPage,
    setPageMode,
    type LibraryError,
    type PageMode,
  } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import EmptyState from "../lib/components/EmptyState.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import { showToast } from "../lib/components/toasts.svelte";
  import { attachMessage } from "../lib/importTexts";
  import { libraryErrorMessage } from "../lib/libraryTexts";
  import { createPageSaver, type PageSaver } from "./pageSaver";
  import PdfView from "./PdfView.svelte";
  import { openPdf, type OpenedPdf } from "./pdfjs";

  interface Props {
    session: number;
    /** The selected paper, or null. */
    paperId: string | null;
    /** No "Attach PDF" in a read-only library, and no saved reading position. */
    readOnly: boolean;
    /** A file was attached: the paper list should refresh. */
    onattached: () => void;
  }

  let { session, paperId, readOnly, onattached }: Props = $props();

  type View =
    | { kind: "empty" }
    | { kind: "loading" }
    | { kind: "missing" }
    | { kind: "broken" }
    | { kind: "ready"; pdf: OpenedPdf; startPage: number; saver: PageSaver };

  let view = $state<View>({ kind: "empty" });
  /** Bumped to open the same paper again, e.g. after "Attach PDF". */
  let reload = $state(0);
  let attaching = $state(false);

  let mode = $state<PageMode>("normal");
  /** The user's choice wins over a stored mode that arrives late. */
  let modeChosen = false;

  onMount(async () => {
    try {
      const stored = await getPageMode();
      if (!modeChosen && stored) mode = stored;
    } catch (failure) {
      console.error("The page mode could not be read", failure);
    }
  });

  function chooseMode(next: PageMode) {
    modeChosen = true;
    mode = next;
    setPageMode(next)?.catch((failure) => console.error("The page mode was not saved", failure));
  }

  /** The first page is a fine place to start when the stored one cannot be read. */
  async function lastPage(id: string): Promise<number> {
    try {
      return (await getLastPage(session, id)) ?? 1;
    } catch {
      return 1;
    }
  }

  $effect(() => {
    const id = paperId;
    void reload;
    if (id === null) {
      view = { kind: "empty" };
      return;
    }
    view = { kind: "loading" };
    let stopped = false;
    let opened: OpenedPdf | undefined;
    // Bound to this paper, so a late save never goes to another one.
    const ofSession = session;
    const writable = !readOnly;
    const saver = createPageSaver(async (page) => {
      if (writable) await saveLastPage(ofSession, id, page);
    });

    (async () => {
      try {
        const [bytes, startPage] = await Promise.all([readPaperPdf(session, id), lastPage(id)]);
        if (stopped) return;
        opened = await openPdf(bytes);
        if (stopped) return opened.close();
        view = { kind: "ready", pdf: opened, startPage, saver };
      } catch (failure) {
        if (stopped) return;
        view = isMissing(failure) ? { kind: "missing" } : { kind: "broken" };
      }
    })();

    return () => {
      stopped = true;
      saver.flush();
      opened?.close();
    };
  });

  function isMissing(failure: unknown): boolean {
    return (failure as LibraryError | null)?.code === "not_found";
  }

  async function attach() {
    if (paperId === null || attaching) return;
    attaching = true;
    try {
      const path = await pickPdf();
      if (path === null) return;
      const outcome = await attachPdf(session, paperId, path);
      const message = attachMessage(outcome, path);
      if (message) showToast(message.message, message.tone);
      if (outcome === "attached" || outcome === "already_in_library") {
        onattached();
        reload++;
      }
    } catch (failure) {
      showToast(libraryErrorMessage(failure, "create"), "error");
    } finally {
      attaching = false;
    }
  }
</script>

<section class="reader">
  {#if view.kind === "ready"}
    {#key view.pdf}
      <PdfView
        pdf={view.pdf}
        startPage={view.startPage}
        onpagechange={view.saver.change}
        {mode}
        onmode={chooseMode}
      />
    {/key}
  {:else if view.kind === "loading"}
    <p class="state" role="status"><Spinner />Loading PDF…</p>
  {:else if view.kind === "missing"}
    <div class="state" role="alert">
      <p class="problem"><Icon name="error" />PDF file is missing.</p>
      {#if !readOnly}
        <Button icon="upload" loading={attaching} onclick={attach}>Attach PDF</Button>
      {/if}
    </div>
  {:else if view.kind === "broken"}
    <p class="state problem" role="alert"><Icon name="error" />This PDF can't be opened.</p>
  {:else}
    <EmptyState title="Select a paper from the list." />
  {/if}
</section>

<style>
  .reader {
    display: flex;
    flex-direction: column;
    min-width: 0;
    height: 100%;
    background: var(--surface-sunken);
  }

  .reader > :global(*) {
    flex: 1;
    min-height: 0;
  }

  .state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    margin: 0;
    padding: var(--space-8) var(--panel-padding);
    color: var(--text-secondary);
    font: var(--type-body);
  }

  p.state {
    flex-direction: row;
    gap: var(--space-2);
  }

  .problem {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--status-error);
  }
</style>
