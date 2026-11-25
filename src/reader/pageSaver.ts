// Saves the page being read (FR-02.3) shortly after it stops changing, so that
// scrolling through a paper does not write to the library for every page.

export const SAVE_DELAY_MS = 400;

export interface PageSaver {
  change: (page: number) => void;
  /** Saves a waiting page at once, e.g. when the paper is closed. */
  flush: () => void;
}

export function createPageSaver(
  save: (page: number) => Promise<unknown>,
  delay = SAVE_DELAY_MS,
): PageSaver {
  let waiting: number | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function flush() {
    clearTimeout(timer);
    if (waiting === null) return;
    const page = waiting;
    waiting = null;
    // Not shown to the user: only the reading position is lost.
    save(page)?.catch((failure) => console.error("The last read page was not saved", failure));
  }

  return {
    change(page) {
      waiting = page;
      clearTimeout(timer);
      timer = setTimeout(flush, delay);
    },
    flush,
  };
}
