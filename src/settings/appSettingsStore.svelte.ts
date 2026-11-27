// The app settings shown in Settings: the CSV separator and the theme (FR-08.3,
// FR-08.5). They are shared by all libraries, also changeable in a read-only one.
//
// A choice shows on screen at once and is saved at once. One that cannot be saved stays
// on screen with "Try again" (PRD Section 8). Saves run one after another, so an older
// choice never overtakes a newer one.

import {
  getCsvDelimiter,
  getTheme,
  setCsvDelimiter,
  setTheme,
  type CsvDelimiter,
} from "../lib/api";
import { dismissToast, showToast } from "../lib/components/toasts.svelte";
import { SAVE_ERROR } from "../lib/libraryTexts";
import { applyTheme, type ThemeChoice } from "../lib/theme";
import { SAVED } from "./settingsTexts";

export type AppSettingsStore = ReturnType<typeof createAppSettingsStore>;

type Key = "delimiter" | "theme";

export function createAppSettingsStore() {
  let status = $state<"idle" | "loading" | "ready" | "error">("idle");
  let delimiter = $state<CsvDelimiter>(";");
  let theme = $state<ThemeChoice>("system");
  /** The choices that could not be saved. */
  let unsaved = $state.raw<ReadonlySet<Key>>(new Set());

  let disposed = false;
  let savedToast: number | null = null;
  /** The end of the chain of saves. */
  let queue: Promise<void> = Promise.resolve();

  /** Saves the choice as it is on screen when its turn comes. */
  const writes: Record<Key, () => Promise<void>> = {
    delimiter: () => setCsvDelimiter(delimiter),
    theme: () => setTheme(theme),
  };

  async function load(): Promise<void> {
    if (status === "loading" || status === "ready") return;
    status = "loading";
    try {
      const [storedDelimiter, storedTheme] = await Promise.all([getCsvDelimiter(), getTheme()]);
      delimiter = storedDelimiter;
      theme = storedTheme;
      status = "ready";
    } catch (failure) {
      console.error("The app settings could not be read", failure);
      status = "error";
    }
  }

  function save(key: Key): void {
    queue = queue.then(async () => {
      const next = new Set(unsaved);
      try {
        await writes[key]();
        next.delete(key);
        if (savedToast !== null) dismissToast(savedToast);
        savedToast = showToast(SAVED, "success");
      } catch (failure) {
        console.error("The app setting was not saved", failure);
        next.add(key);
        // The library was left: its banner is gone, so say it here.
        if (disposed) showToast(SAVE_ERROR, "error");
      }
      unsaved = next;
    });
  }

  return {
    get status() {
      return status;
    },
    get delimiter() {
      return delimiter;
    },
    get theme() {
      return theme;
    },
    get failed() {
      return unsaved.size > 0;
    },
    /** An export must not use a separator other than the one on screen. */
    get delimiterFailed() {
      return unsaved.has("delimiter");
    },
    load,
    chooseDelimiter(next: CsvDelimiter) {
      delimiter = next;
      save("delimiter");
    },
    chooseTheme(next: ThemeChoice) {
      theme = next;
      applyTheme(next);
      save("theme");
    },
    /** Resolves when all saves are done. */
    flush: () => queue,
    /** "Try again": saves everything that could not be saved. */
    retry(): Promise<void> {
      for (const key of unsaved) save(key);
      return queue;
    },
    /** The library is being left. */
    dispose() {
      disposed = true;
      if (unsaved.size > 0) showToast(SAVE_ERROR, "error");
    },
  };
}
