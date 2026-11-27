<!--
  S-02 Reader (FR-02.1): opens the selected paper's PDF at the page it was last read
  at (FR-02.3), and keeps the page mode (FR-02.2) and the zoom (FR-08.5).
  States from Brief Section 7: "Select a paper from the list.", "Loading PDF…",
  "PDF file is missing." + "Attach PDF", "This PDF can't be opened.".
  A password-protected PDF asks for its password when opened (PRD Section 8, T-19):
  "This PDF is password-protected." + "Enter password" / "Skip". The password is used
  to open the PDF and, once, to extract its text; it is never stored.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    attachPdf,
    extractLockedPaperText,
    getLastPage,
    getPageMode,
    getZoom,
    pickPdf,
    readPaperPdf,
    saveLastPage,
    setPageMode,
    setZoom,
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
  import { createHighlightStore, type HighlightStore } from "./highlightStore.svelte";
  import { createPageSaver, type PageSaver } from "./pageSaver";
  import PasswordDialog from "./PasswordDialog.svelte";
  import PdfView from "./PdfView.svelte";
  import { openPdf, PdfOpenError, type OpenedPdf } from "./pdfjs";

  interface Props {
    session: number;
    /** The selected paper, or null. */
    paperId: string | null;
    /** No "Attach PDF" in a read-only library, and no saved reading position. */
    readOnly: boolean;
    /** A file was attached: the paper list should refresh. */
    onattached: () => void;
    /** The highlights of the open paper, shared with the Highlights tab. */
    store?: HighlightStore;
  }

  let {
    session,
    paperId,
    readOnly,
    onattached,
    store = createHighlightStore(),
  }: Props = $props();

  type View =
    | { kind: "empty" }
    | { kind: "loading" }
    | { kind: "missing" }
    | { kind: "broken" }
    | { kind: "locked" }
    | {
        kind: "ready";
        pdf: OpenedPdf;
        startPage: number;
        startZoom: number | null;
        saver: PageSaver;
      };

  let view = $state<View>({ kind: "empty" });
  /** Bumped to open the same paper again, e.g. after "Attach PDF" or a password. */
  let reload = $state(0);
  let attaching = $state(false);

  /** The password dialog is open. */
  let askingPassword = $state(false);
  let wrongPassword = $state(false);
  /**
   * The password to try on the next opening, for `passwordFor` only. A plain variable,
   * not state, and cleared as soon as it is taken: it is never kept or saved.
   */
  let password: string | undefined;
  let passwordFor: string | null = null;

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

  /** The zoom the next PDF opens with, shared by all papers; null is "Fit width". */
  let zoom: number | null = null;
  const zoomRead = (async () => {
    try {
      zoom = (await getZoom()) ?? null;
    } catch (failure) {
      console.error("The zoom could not be read", failure);
    }
  })();

  /** Only possible with a PDF on screen, so after the stored zoom was read. */
  function chooseZoom(next: number | null) {
    zoom = next;
    setZoom(next)?.catch((failure) => console.error("The zoom was not saved", failure));
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
    // Another paper: nothing about the last one's password carries over.
    if (passwordFor !== id) {
      password = undefined;
      passwordFor = id;
      askingPassword = false;
      wrongPassword = false;
    }
    if (id === null) {
      untrack(() => store.close());
      view = { kind: "empty" };
      return;
    }
    view = { kind: "loading" };
    // The Highlights tab shows them even when the PDF cannot be opened.
    const highlightsRead = untrack(() => store.open(session, id));
    let stopped = false;
    let opened: OpenedPdf | undefined;
    // Bound to this paper, so a late save never goes to another one.
    const ofSession = session;
    const writable = !readOnly;
    const saver = createPageSaver(async (page) => {
      if (writable) await saveLastPage(ofSession, id, page);
    });
    const tried = password;
    password = undefined;

    (async () => {
      try {
        const [bytes, startPage] = await Promise.all([
          readPaperPdf(session, id),
          lastPage(id),
          highlightsRead,
          zoomRead,
        ]);
        if (stopped) return;
        opened = await openPdf(bytes, tried);
        if (stopped) return opened.close();
        view = { kind: "ready", pdf: opened, startPage, startZoom: zoom, saver };
        askingPassword = false;
        wrongPassword = false;
        if (tried !== undefined && writable) {
          // The text of a locked PDF can only be read with its password (FR-01.6).
          extractLockedPaperText(ofSession, id, tried).catch((failure: LibraryError) =>
            console.error("The text of the locked PDF was not extracted", failure?.code),
          );
        }
      } catch (failure) {
        if (stopped) return;
        if (failure instanceof PdfOpenError && failure.problem !== "damaged") {
          view = { kind: "locked" };
          wrongPassword = failure.problem === "wrong_password";
          // Asked when the paper is opened; after "Skip", only "Enter password" asks again.
          if (failure.problem === "locked" && tried === undefined) askingPassword = true;
          return;
        }
        askingPassword = false;
        view = isMissing(failure) ? { kind: "missing" } : { kind: "broken" };
      }
    })();

    return () => {
      stopped = true;
      saver.flush();
      opened?.close();
    };
  });

  /** "Open" in the password dialog: open the same paper again with this password. */
  function tryPassword(value: string) {
    password = value;
    wrongPassword = false;
    reload++;
  }

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
        startZoom={view.startZoom}
        onzoom={chooseZoom}
        {mode}
        onmode={chooseMode}
        highlights={store.highlights}
        categories={store.categories}
        activeHighlight={store.activeId}
        jump={store.jump}
        onhighlight={readOnly ? undefined : store.add}
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
  {:else if view.kind === "locked"}
    <div class="state" role="status">
      <p class="locked"><Icon name="info" />This PDF is password-protected.</p>
      <Button onclick={() => (askingPassword = true)}>Enter password</Button>
    </div>
  {:else}
    <EmptyState title="Select a paper from the list." />
  {/if}
</section>

{#if askingPassword}
  <PasswordDialog
    busy={view.kind === "loading"}
    wrong={wrongPassword}
    onopen={tryPassword}
    onskip={() => {
      askingPassword = false;
      wrongPassword = false;
    }}
  />
{/if}

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

  .locked {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
  }

  .problem {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--status-error);
  }
</style>
