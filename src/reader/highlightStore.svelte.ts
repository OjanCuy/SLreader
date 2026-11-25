// The highlights of the open paper, shared by the reader and the Highlights tab
// (FR-03.1 to FR-03.5). One store per opened library.
//
// Changes show on screen at once and are then saved. A change that cannot be saved
// stays on screen with "Try again" (PRD Section 8), also after switching papers.

import {
  addHighlight,
  deleteHighlight,
  listExtractionTags,
  listHighlightCategories,
  listHighlights,
  updateHighlight,
  type ExtractionTag,
  type Highlight,
  type HighlightCategory,
  type HighlightEdit,
  type NewHighlight,
} from "../lib/api";
import { dismissToast, showToast } from "../lib/components/toasts.svelte";
import { GENERIC_ERROR, SAVE_ERROR } from "../lib/libraryTexts";
import { crossPageMessage } from "./highlights";

/** A note is saved this long after the last key press, and when its field is left. */
export const NOTE_SAVE_DELAY_MS = 600;

interface Unsaved {
  session: number;
  edit: HighlightEdit;
  noteChanged: boolean;
}

export type HighlightStore = ReturnType<typeof createHighlightStore>;

export function createHighlightStore() {
  let session = -1;
  let paperId = $state<string | null>(null);
  let status = $state<"loading" | "ready" | "error">("loading");
  let highlights = $state.raw<Highlight[]>([]);
  let categories = $state.raw<HighlightCategory[]>([]);
  let tags = $state.raw<ExtractionTag[]>([]);
  /** The highlight shown as selected, on its card and on the page. */
  let activeId = $state<string | null>(null);
  /** "Show this highlight in the PDF"; `n` changes on every request. */
  let jump = $state.raw<{ id: string; n: number } | null>(null);
  /** Highlights whose note has just been saved ("Saved"). */
  let saved = $state.raw<ReadonlySet<string>>(new Set<string>());

  /** Bumped on every open and close, so that a late answer is dropped. */
  let generation = 0;
  let jumps = 0;
  const unsaved = new Map<string, Unsaved>();
  /** The running save of each highlight: saves of one highlight never overtake. */
  const saving = new Map<string, Promise<void>>();
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  const failures = new Map<string, number>();

  function reset(next: string | null) {
    flush();
    generation++;
    paperId = next;
    status = "loading";
    highlights = [];
    activeId = null;
    jump = null;
    saved = new Set();
  }

  /** Loads a paper's highlights. Never rejects: a failure shows in `status`. */
  async function open(ofSession: number, id: string): Promise<void> {
    reset(id);
    session = ofSession;
    const mine = generation;
    try {
      // So that what is read includes the changes still being saved.
      await Promise.all(saving.values());
      const [list, kinds, names] = await Promise.all([
        listHighlights(ofSession, id),
        listHighlightCategories(ofSession),
        listExtractionTags(ofSession),
      ]);
      if (mine !== generation) return;
      categories = kinds ?? [];
      tags = names ?? [];
      // Changes that could not be saved stay on screen.
      highlights = (list ?? []).map((each) => ({ ...each, ...unsaved.get(each.id)?.edit }));
      status = "ready";
    } catch (failure) {
      console.error("The highlights could not be read", failure);
      if (mine === generation) status = "error";
    }
  }

  async function add(highlight: NewHighlight, crossed: boolean): Promise<void> {
    if (paperId !== null) await addTo(session, paperId, highlight, crossed);
  }

  async function addTo(ofSession: number, id: string, highlight: NewHighlight, crossed: boolean) {
    try {
      const added = await addHighlight(ofSession, id, highlight);
      if (paperId === id) {
        // Sorting is stable: within a page the newest comes last.
        highlights = [...highlights, added].sort((a, b) => a.page - b.page);
        activeId = added.id;
      }
      if (crossed) showToast(crossPageMessage(highlight.page));
    } catch {
      showToast(SAVE_ERROR, "error", [
        { label: "Try again", onselect: () => addTo(ofSession, id, highlight, crossed) },
      ]);
    }
  }

  /** Changes a highlight on screen and notes what has to be saved. */
  function change(id: string, patch: Partial<HighlightEdit>): boolean {
    const current = highlights.find((each) => each.id === id);
    if (!current) return false;
    const next = { ...current, ...patch };
    highlights = highlights.map((each) => (each.id === id ? next : each));
    unsaved.set(id, {
      session,
      edit: {
        categoryId: next.categoryId,
        extractionTagId: next.extractionTagId,
        note: next.note,
      },
      noteChanged: "note" in patch || (unsaved.get(id)?.noteChanged ?? false),
    });
    setSaved(id, false);
    return true;
  }

  /** The category or the tag: saved at once. */
  function edit(id: string, patch: Partial<Omit<HighlightEdit, "note">>): Promise<void> {
    return change(id, patch) ? save(id) : Promise.resolve();
  }

  /** The note: saved shortly after typing stops. */
  function editNote(id: string, note: string): void {
    if (!change(id, { note })) return;
    clearTimeout(timers.get(id));
    timers.set(id, setTimeout(() => save(id), NOTE_SAVE_DELAY_MS));
  }

  function save(id: string): Promise<void> {
    clearTimeout(timers.get(id));
    timers.delete(id);
    const run = (saving.get(id) ?? Promise.resolve()).then(async () => {
      const pending = unsaved.get(id);
      if (!pending) return;
      try {
        await updateHighlight(pending.session, id, pending.edit);
        clearFailure(id);
        // Changed again meanwhile: the next save takes care of it.
        if (unsaved.get(id) !== pending) return;
        unsaved.delete(id);
        if (pending.noteChanged) setSaved(id, true);
      } catch {
        clearFailure(id);
        failures.set(
          id,
          showToast(SAVE_ERROR, "error", [{ label: "Try again", onselect: () => save(id) }]),
        );
      }
    });
    saving.set(id, run);
    return run;
  }

  /** Saves the notes that are still waiting, e.g. when their field is left. */
  function flush(id?: string): void {
    for (const waiting of [...timers.keys()]) {
      if (id === undefined || waiting === id) save(waiting);
    }
  }

  /** Deletes a highlight. Only call it after the user has confirmed. */
  async function remove(id: string): Promise<void> {
    const ofSession = session;
    clearTimeout(timers.get(id));
    timers.delete(id);
    try {
      await saving.get(id);
      await deleteHighlight(ofSession, id);
    } catch {
      showToast(GENERIC_ERROR, "error");
      // A note that was waiting is still to be saved.
      if (unsaved.has(id)) save(id);
      return;
    }
    unsaved.delete(id);
    clearFailure(id);
    highlights = highlights.filter((each) => each.id !== id);
    if (activeId === id) activeId = null;
  }

  /** A click on a card: select it and show it in the PDF (FR-03.4). */
  function show(id: string): void {
    activeId = id;
    jump = { id, n: ++jumps };
  }

  function setSaved(id: string, value: boolean) {
    if (saved.has(id) === value) return;
    const next = new Set(saved);
    if (value) next.add(id);
    else next.delete(id);
    saved = next;
  }

  function clearFailure(id: string) {
    const toast = failures.get(id);
    if (toast !== undefined) dismissToast(toast);
    failures.delete(id);
  }

  return {
    get paperId() {
      return paperId;
    },
    get status() {
      return status;
    },
    get highlights() {
      return highlights;
    },
    get categories() {
      return categories;
    },
    get tags() {
      return tags;
    },
    get activeId() {
      return activeId;
    },
    get jump() {
      return jump;
    },
    get saved() {
      return saved;
    },
    open,
    close: () => reset(null),
    add,
    edit,
    editNote,
    flush,
    remove,
    show,
  };
}
