// The Decision tab of the open paper: screening (FR-04.1 to FR-04.4) and metadata
// (FR-05.1, FR-05.4). One store per opened library.
//
// A change shows on screen at once and is then saved. A change that cannot be saved
// stays on screen with "Try again" (PRD Section 8), also after switching papers.
// Each field has its own command, so only the user's decision action writes the
// decision (FR-11.4).

import {
  getMetadata,
  getScreening,
  listExclusionReasons,
  setAssessmentNote,
  setDecision,
  setExclusionReason,
  setMetadata,
  type Decision,
  type ExclusionReason,
  type Metadata,
  type Screening,
} from "../lib/api";
import { showToast } from "../lib/components/toasts.svelte";
import { SAVE_ERROR } from "../lib/libraryTexts";

/** Typed text is saved this long after the last key press, and when its field is left. */
export const NOTE_SAVE_DELAY_MS = 600;

type Changes = Partial<Screening> & { metadata?: Metadata };

interface Unsaved {
  session: number;
  changes: Changes;
}

export type DecisionStore = ReturnType<typeof createDecisionStore>;

interface Listeners {
  /** A decision or reason was saved: the counts have changed. */
  onsaved?: () => void;
  /** Metadata was saved: the paper list may show another title. */
  onmetadatasaved?: () => void;
}

export function createDecisionStore({ onsaved, onmetadatasaved }: Listeners = {}) {
  let paperId = $state<string | null>(null);
  let status = $state<"loading" | "ready" | "error">("loading");
  let screening = $state.raw<Screening | null>(null);
  let reasons = $state.raw<ExclusionReason[]>([]);
  /** The note of the open paper has just been saved ("Saved"). */
  let noteSaved = $state(false);
  let metadata = $state.raw<Metadata | null>(null);
  /** The metadata of the open paper has just been saved ("Saved"). */
  let metadataSaved = $state(false);
  /** Other papers with the open paper's reference number (FR-05.4). */
  let sameRefNo = $state.raw<string[]>([]);
  /** Papers with a change that could not be saved. */
  let failed = $state.raw<ReadonlySet<string>>(new Set<string>());

  let session = -1;
  /** Bumped on every open and close, so that a late answer is dropped. */
  let generation = 0;
  let disposed = false;
  const unsaved = new Map<string, Unsaved>();
  /** The running save of each paper: saves of one paper never overtake. */
  const saving = new Map<string, Promise<void>>();
  const timers = new Map<string, ReturnType<typeof setTimeout>>();

  function reset(next: string | null) {
    flush();
    generation++;
    paperId = next;
    status = "loading";
    screening = null;
    noteSaved = false;
    metadata = null;
    metadataSaved = false;
    sameRefNo = [];
  }

  /** Loads a paper's screening. Never rejects: a failure shows in `status`. */
  async function open(ofSession: number, id: string): Promise<void> {
    reset(id);
    session = ofSession;
    const mine = generation;
    try {
      // So that what is read includes the changes still being saved.
      await Promise.all(saving.values());
      const [stored, choices, described] = await Promise.all([
        getScreening(ofSession, id),
        listExclusionReasons(ofSession),
        getMetadata(ofSession, id),
      ]);
      if (mine !== generation) return;
      if (!stored || !described) throw new Error("nothing stored");
      reasons = choices ?? [];
      // Changes that could not be saved stay on screen.
      const { metadata: unsavedMetadata, ...unsavedScreening } = unsaved.get(id)?.changes ?? {};
      screening = { ...stored, ...unsavedScreening };
      metadata = unsavedMetadata ?? described.metadata;
      // The warning belongs to the stored number only.
      sameRefNo = metadata.refNo === described.metadata.refNo ? described.sameRefNo : [];
      status = "ready";
    } catch (failure) {
      console.error("The decision could not be read", failure);
      if (mine === generation) status = "error";
    }
  }

  /** Changes the open paper on screen and notes what has to be saved. */
  function change(changes: Changes): string | null {
    if (paperId === null || !screening || !metadata) return null;
    const { metadata: newMetadata, ...screened } = changes;
    screening = { ...screening, ...screened };
    if (newMetadata) metadata = newMetadata;
    const entry = unsaved.get(paperId) ?? { session, changes: {} };
    Object.assign(entry.changes, changes);
    unsaved.set(paperId, entry);
    return paperId;
  }

  /** The user's decision (FR-04.1). */
  function decide(decision: Decision): Promise<void> {
    if (screening?.decision === decision) return Promise.resolve();
    const id = change({ decision });
    return id === null ? Promise.resolve() : save(id);
  }

  function chooseReason(exclusionReasonId: string | null): Promise<void> {
    const id = change({ exclusionReasonId });
    return id === null ? Promise.resolve() : save(id);
  }

  /** The note: saved shortly after typing stops. */
  function editNote(assessmentNote: string): void {
    const id = change({ assessmentNote });
    if (id === null) return;
    noteSaved = false;
    saveSoon(id);
  }

  /** Reference number, title, year, source database, or DOI: saved shortly after typing. */
  function editMetadata(patch: Partial<Metadata>): void {
    if (!metadata) return;
    const refNoChanged = "refNo" in patch && patch.refNo !== metadata.refNo;
    const id = change({ metadata: { ...metadata, ...patch } });
    if (id === null) return;
    metadataSaved = false;
    if (refNoChanged) sameRefNo = [];
    saveSoon(id);
  }

  function saveSoon(id: string) {
    clearTimeout(timers.get(id));
    timers.set(id, setTimeout(() => save(id), NOTE_SAVE_DELAY_MS));
  }

  function save(id: string): Promise<void> {
    clearTimeout(timers.get(id));
    timers.delete(id);
    const run = (saving.get(id) ?? Promise.resolve()).then(() => write(id));
    saving.set(id, run);
    return run;
  }

  async function write(id: string): Promise<void> {
    const entry = unsaved.get(id);
    if (!entry) return;
    const { changes } = entry;
    try {
      // Each value is forgotten only if it was not changed again meanwhile.
      const { decision, exclusionReasonId, assessmentNote, metadata: described } = changes;
      if (decision !== undefined) {
        await setDecision(entry.session, id, decision);
        if (changes.decision === decision) delete changes.decision;
        onsaved?.();
      }
      if (exclusionReasonId !== undefined) {
        await setExclusionReason(entry.session, id, exclusionReasonId);
        if (changes.exclusionReasonId === exclusionReasonId) delete changes.exclusionReasonId;
        onsaved?.();
      }
      if (assessmentNote !== undefined) {
        await setAssessmentNote(entry.session, id, assessmentNote);
        if (changes.assessmentNote === assessmentNote) {
          delete changes.assessmentNote;
          if (paperId === id) noteSaved = true;
        }
      }
      if (described !== undefined) {
        const others = await setMetadata(entry.session, id, described);
        if (changes.metadata === described) {
          delete changes.metadata;
          if (paperId === id) {
            metadataSaved = true;
            sameRefNo = others ?? [];
          }
        }
        onmetadatasaved?.();
      }
      if (Object.keys(changes).length === 0 && unsaved.get(id) === entry) unsaved.delete(id);
      setFailed(id, false);
    } catch {
      // The paper was deleted meanwhile: there is nothing left to save.
      if (unsaved.get(id) !== entry) return;
      setFailed(id, true);
      // The library was left: its banner is gone, so say it here.
      if (disposed) showToast(SAVE_ERROR, "error");
    }
  }

  /** Saves the notes that are still waiting, e.g. when their field is left. */
  function flush(): void {
    for (const id of [...timers.keys()]) save(id);
  }

  /** "Try again": saves everything that could not be saved. */
  function retry(): Promise<unknown> {
    return Promise.all([...failed].map(save));
  }

  /** The paper was deleted: drop what was waiting for it. */
  function forget(id: string): void {
    clearTimeout(timers.get(id));
    timers.delete(id);
    unsaved.delete(id);
    setFailed(id, false);
  }

  /** The library is being left. */
  function dispose(): void {
    disposed = true;
    if (failed.size > 0) showToast(SAVE_ERROR, "error");
    flush();
  }

  function setFailed(id: string, value: boolean) {
    if (failed.has(id) === value) return;
    const next = new Set(failed);
    if (value) next.add(id);
    else next.delete(id);
    failed = next;
  }

  return {
    get paperId() {
      return paperId;
    },
    get status() {
      return status;
    },
    get screening() {
      return screening;
    },
    get reasons() {
      return reasons;
    },
    get noteSaved() {
      return noteSaved;
    },
    get metadata() {
      return metadata;
    },
    get metadataSaved() {
      return metadataSaved;
    },
    get sameRefNo() {
      return sameRefNo;
    },
    /** A change, of this or an earlier paper, could not be saved. */
    get saveFailed() {
      return failed.size > 0;
    },
    open,
    close: () => reset(null),
    decide,
    chooseReason,
    editNote,
    editMetadata,
    flush,
    retry,
    forget,
    dispose,
  };
}
