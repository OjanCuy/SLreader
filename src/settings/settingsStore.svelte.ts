// The criteria of the open library, edited in Settings (FR-08.1, FR-08.2, FR-08.4).
// One store per opened library, so text that could not be saved is still there when
// Settings is opened again.
//
// Research questions are rows on screen too, but are saved together as one text with a
// question per line.
//
// A change shows on screen at once and is saved shortly after typing stops. A change
// that cannot be saved stays on screen with "Try again" (PRD Section 8). Saves run one
// after another, so they never overtake each other.

import {
  addCriterion,
  addTemplateItems,
  countCriterionUses,
  deleteCriterion,
  getResearchQuestions,
  listCriteria,
  reorderCriteria,
  setResearchQuestions,
  updateCriterion,
  type CriteriaKind,
} from "../lib/api";
import { dismissToast, showToast } from "../lib/components/toasts.svelte";
import { GENERIC_ERROR, SAVE_ERROR } from "../lib/libraryTexts";
import { nextCode, SAVED, type SettingsSection } from "./settingsTexts";
import type { TemplateItem, TemplateKind } from "./templates";

/** Typed text is saved this long after the last key press, and when its field is left. */
export const SAVE_DELAY_MS = 600;

export interface Row {
  /** Stable for the list on screen, also before the row is saved. */
  key: number;
  /** Null until the row is saved for the first time. */
  id: string | null;
  code: string;
  label: string;
  /** What is saved. */
  stored: { code: string; label: string } | null;
  /** Fields the user has left, so that an empty one may be reported. */
  touched: { code: boolean; label: boolean };
}

export interface CriteriaList {
  status: "idle" | "loading" | "ready" | "error";
  rows: Row[];
}

export type SettingsStore = ReturnType<typeof createSettingsStore>;

export function createSettingsStore(session: () => number) {
  const lists = $state<Record<SettingsSection, CriteriaList>>({
    questions: { status: "idle", rows: [] },
    inclusion: { status: "idle", rows: [] },
    exclusion: { status: "idle", rows: [] },
    tag: { status: "idle", rows: [] },
  });
  /** The research questions as saved. */
  let storedQuestions = "";
  /** A change could not be saved. */
  let failed = $state(false);

  let keys = 0;
  let disposed = false;
  let savedToast: number | null = null;
  /** The end of the chain of saves. */
  let queue: Promise<void> = Promise.resolve();
  const failures = new Map<string, () => Promise<boolean>>();
  /** Saves waiting for the typing to stop. */
  const waiting = new Map<string, { timer: ReturnType<typeof setTimeout>; run: () => void }>();

  async function read(kind: SettingsSection): Promise<{ id: string; code: string; label: string }[]> {
    if (kind !== "questions") return (await listCriteria(session(), kind)) ?? [];
    const lines = ((await getResearchQuestions(session())) ?? "")
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line !== "");
    // Without blank lines, so that nothing is written until the user changes something.
    storedQuestions = lines.join("\n");
    return lines.map((label, at) => ({ id: `line-${at}`, code: "", label }));
  }

  async function load(kind: SettingsSection): Promise<void> {
    const list = lists[kind];
    if (list.status === "loading" || list.status === "ready") return;
    list.status = "loading";
    try {
      list.rows = (await read(kind)).map(savedRow);
      list.status = "ready";
    } catch (failure) {
      console.error("The list could not be read", failure);
      list.status = "error";
    }
  }

  /**
   * Saves the research questions as they are on screen, leaving out `without`. A new
   * row without text is left out, and an emptied one keeps its old question.
   */
  async function writeQuestions(without?: Row): Promise<boolean> {
    const rows = lists.questions.rows.filter((row) => row !== without);
    const values = rows.map((row) => row.label.trim() || (row.stored?.label ?? ""));
    const text = values.filter((value) => value !== "").join("\n");
    if (text === storedQuestions) return false;
    await setResearchQuestions(session(), text);
    storedQuestions = text;
    rows.forEach((row, at) => {
      const label = values[at]!;
      if (label === "") return;
      row.id ??= `new-${row.key}`;
      row.stored = { code: "", label };
    });
    return true;
  }

  function showSaved() {
    if (savedToast !== null) dismissToast(savedToast);
    savedToast = showToast(SAVED, "success");
  }

  /** Runs `write` after the saves before it. It resolves to false when there was nothing to save. */
  function enqueue(name: string, write: () => Promise<boolean>): void {
    queue = queue.then(async () => {
      try {
        const wrote = await write();
        failures.delete(name);
        if (wrote) showSaved();
      } catch {
        failures.set(name, write);
        // The library was left: its banner is gone, so say it here.
        if (disposed) showToast(SAVE_ERROR, "error");
      }
      failed = failures.size > 0;
    });
  }

  function saveSoon(name: string, run: () => void) {
    clearTimeout(waiting.get(name)?.timer);
    const now = () => {
      clearTimeout(waiting.get(name)?.timer);
      waiting.delete(name);
      run();
    };
    waiting.set(name, { timer: setTimeout(now, SAVE_DELAY_MS), run: now });
  }

  /** Saves everything that is still waiting; resolves when all saves are done. */
  function flush(): Promise<void> {
    for (const { run } of [...waiting.values()]) run();
    return queue;
  }

  /** Saves a row once its required fields are filled; an emptied field keeps the old value. */
  function saveRow(kind: SettingsSection, row: Row): void {
    enqueue(`row-${row.key}`, async () => {
      const list = lists[kind];
      if (!list.rows.includes(row)) return false;
      if (kind === "questions") return writeQuestions();
      const label = row.label.trim();
      const code = kind === "tag" ? "" : row.code.trim();
      if (label === "" || (kind !== "tag" && code === "")) return false;
      if (row.stored?.code === code && row.stored.label === label) return false;
      if (row.id === null) {
        row.id = await addCriterion(session(), kind, code, label);
        // It was added at the end; a row below it may have been saved first.
        const after = list.rows.slice(list.rows.indexOf(row) + 1);
        if (after.some((other) => other.id !== null)) saveOrder(kind);
      } else {
        await updateCriterion(session(), kind, row.id, code, label);
      }
      row.stored = { code, label };
      return true;
    });
  }

  function edit(kind: SettingsSection, row: Row, field: "code" | "label", value: string): void {
    row[field] = value;
    saveSoon(`row-${row.key}`, () => saveRow(kind, row));
  }

  /** A field of a row was left. */
  function leave(row: Row, field: "code" | "label"): void {
    row.touched[field] = true;
    waiting.get(`row-${row.key}`)?.run();
  }

  /** A saved item as a row of a list. */
  function savedRow({ id, code, label }: { id: string; code: string; label: string }): Row {
    return {
      key: ++keys,
      id,
      code,
      label,
      stored: { code, label },
      touched: { code: false, label: false },
    };
  }

  /**
   * "Load a template": saves the items the list lacks and shows them at its end.
   * Resolves to false when they could not be saved; then nothing was added.
   */
  async function addTemplate(kind: TemplateKind, items: TemplateItem[]): Promise<boolean> {
    let saved = true;
    const done = queue.then(async () => {
      try {
        const added = await addTemplateItems(session(), kind, items);
        // Rows added but never filled in would sit between the old and the new ones.
        for (const row of lists[kind].rows.filter((row) => row.id === null && row.label.trim() === "")) {
          drop(kind, row);
        }
        lists[kind].rows.push(...added.map(savedRow));
        if (added.length > 0) showSaved();
      } catch (failure) {
        console.error("The template could not be saved", failure);
        saved = false;
      }
    });
    queue = done;
    await done;
    return saved;
  }

  function add(kind: SettingsSection): Row {
    const { rows } = lists[kind];
    rows.push({
      key: ++keys,
      id: null,
      code: kind === "tag" || kind === "questions" ? "" : nextCode(kind, rows.map((row) => row.code)),
      label: "",
      stored: null,
      touched: { code: false, label: false },
    });
    // The proxy kept in the state, not the plain object pushed into it.
    return rows[rows.length - 1]!;
  }

  /** Moves a row to another position on screen. `saveOrder` saves it. */
  function move(kind: SettingsSection, row: Row, to: number): boolean {
    const { rows } = lists[kind];
    const from = rows.indexOf(row);
    const target = Math.max(0, Math.min(rows.length - 1, to));
    if (from < 0 || from === target) return false;
    rows.splice(from, 1);
    rows.splice(target, 0, row);
    return true;
  }

  function saveOrder(kind: SettingsSection): void {
    enqueue(`order-${kind}`, async () => {
      if (kind === "questions") return writeQuestions();
      const ids = lists[kind].rows.flatMap((row) => (row.id === null ? [] : [row.id]));
      await reorderCriteria(session(), kind, ids);
      return true;
    });
  }

  function drop(kind: SettingsSection, row: Row) {
    clearTimeout(waiting.get(`row-${row.key}`)?.timer);
    waiting.delete(`row-${row.key}`);
    failures.delete(`row-${row.key}`);
    failed = failures.size > 0;
    const { rows } = lists[kind];
    const at = rows.indexOf(row);
    if (at >= 0) rows.splice(at, 1);
  }

  /**
   * The user asked to delete a row. A row that was never saved is removed at once and
   * resolves to null. Otherwise resolves to how many papers or highlights use it, for
   * the confirmation; nothing is deleted yet. Rejects when that cannot be read.
   */
  async function askToRemove(kind: SettingsSection, row: Row): Promise<number | null> {
    waiting.get(`row-${row.key}`)?.run();
    await queue;
    if (row.id === null) {
      drop(kind, row);
      return null;
    }
    return kind === "questions" ? 0 : countCriterionUses(session(), kind, row.id);
  }

  /** Deletes a saved row. Only call it after the user has confirmed. */
  async function remove(kind: SettingsSection, row: Row): Promise<void> {
    const done = queue.then(async () => {
      if (row.id === null) return;
      try {
        if (kind === "questions") await writeQuestions(row);
        else await deleteCriterion(session(), kind, row.id);
        drop(kind, row);
        showSaved();
      } catch {
        showToast(GENERIC_ERROR, "error");
      }
    });
    queue = done;
    await done;
  }

  /** "Try again": saves everything that could not be saved. */
  function retry(): Promise<void> {
    for (const [name, write] of [...failures]) enqueue(name, write);
    return queue;
  }

  /** The library is being left. */
  function dispose(): void {
    disposed = true;
    if (failures.size > 0) showToast(SAVE_ERROR, "error");
    flush();
  }

  return {
    lists,
    get failed() {
      return failed;
    },
    load,
    edit,
    leave,
    add,
    addTemplate,
    move,
    saveOrder,
    askToRemove,
    remove,
    flush,
    retry,
    dispose,
  };
}
