// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
import ToastRegion from "../lib/components/ToastRegion.svelte";
import { GENERIC_ERROR, SAVE_ERROR } from "../lib/libraryTexts";
import { createHighlightStore, NOTE_SAVE_DELAY_MS } from "./highlightStore.svelte";
import HighlightsPanel from "./HighlightsPanel.svelte";

vi.mock("../lib/api");
const mocked = vi.mocked(api);

const CATEGORIES: api.HighlightCategory[] = [
  { id: "c-inc", label: "Inclusion", color: "green", sortOrder: 0 },
  { id: "c-met", label: "Method", color: "blue", sortOrder: 1 },
];

function highlight(id: string, page: number, more: Partial<api.Highlight> = {}): api.Highlight {
  return {
    id,
    page,
    categoryId: "c-inc",
    text: `text of ${id}`,
    rects: [{ x: 0.1, y: 0.2, w: 0.5, h: 0.02 }],
    note: "",
    extractionTagId: null,
    createdAt: "2026-11-26T00:00:00Z",
    ...more,
  };
}

beforeEach(() => {
  vi.resetAllMocks();
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
  mocked.listHighlights.mockResolvedValue([highlight("h1", 2), highlight("h2", 5, { note: "old note" })]);
  mocked.listHighlightCategories.mockResolvedValue(CATEGORIES);
  mocked.listExtractionTags.mockResolvedValue([{ id: "t-fcr", label: "FCR" }]);
  mocked.updateHighlight.mockResolvedValue();
  mocked.deleteHighlight.mockResolvedValue();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

async function show(readOnly = false) {
  const store = createHighlightStore();
  render(ToastRegion);
  const view = render(HighlightsPanel, { store, readOnly });
  await store.open(7, "p1");
  await new Promise((resolve) => setTimeout(resolve, 0));
  return { store, view };
}

const cards = () => Array.from(document.querySelectorAll<HTMLElement>(".card"));
const card = (index: number) => within(cards()[index]);
const note = (index: number) => card(index).getByLabelText("Note") as HTMLTextAreaElement;
const type = (index: number, value: string) => fireEvent.input(note(index), { target: { value } });
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("the Highlights tab (FR-03.3, FR-03.4)", () => {
  it("shows the legend and a card per highlight, by page", async () => {
    await show();
    const legend = Array.from(document.querySelectorAll(".legend li")).map((li) => li.textContent?.trim());
    expect(legend).toEqual(["Inclusion", "Method"]);
    expect(cards()).toHaveLength(2);
    expect(card(0).getByText("p. 2")).toBeTruthy();
    expect(card(0).getByText("text of h1")).toBeTruthy();
    // The color is never shown without the category's name.
    expect(card(0).getByRole("button", { name: "Change category: Inclusion" }).textContent).toContain("Inclusion");
    expect(note(1).value).toBe("old note");
  });

  it("says how to add a highlight when there are none", async () => {
    mocked.listHighlights.mockResolvedValue([]);
    await show();
    expect(screen.getByText("Select text in the PDF, then pick a color to add a highlight.")).toBeTruthy();
  });

  it("says something went wrong instead of showing an empty list", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.listHighlights.mockRejectedValue({ code: "other", detail: "busy" });
    await show();
    expect(screen.getByRole("alert").textContent).toContain(GENERIC_ERROR);
    expect(screen.queryByText(/pick a color/)).toBeNull();
  });

  it("selects a highlight and asks the reader to show it when its text is clicked", async () => {
    const { store } = await show();
    await fireEvent.click(card(1).getByText("text of h2"));
    expect(store.activeId).toBe("h2");
    expect(store.jump).toMatchObject({ id: "h2" });
    expect(cards()[1].getAttribute("aria-current")).toBe("true");
    const first = store.jump!.n;
    await fireEvent.click(card(1).getByText("text of h2"));
    expect(store.jump!.n).not.toBe(first);
  });

  it("changes the category", async () => {
    await show();
    await fireEvent.click(card(0).getByRole("button", { name: "Change category: Inclusion" }));
    await fireEvent.click(card(0).getByRole("button", { name: "Method" }));
    await tick();
    expect(mocked.updateHighlight).toHaveBeenCalledExactlyOnceWith(7, "h1", {
      categoryId: "c-met",
      extractionTagId: null,
      note: "",
    });
    expect(card(0).getByRole("button", { name: "Change category: Method" })).toBeTruthy();
    expect(card(0).queryByRole("list", { name: "Change category" })).toBeNull();
  });

  it("sets and clears the tag", async () => {
    await show();
    const select = card(0).getByLabelText("Tag") as HTMLSelectElement;
    expect(Array.from(select.options, (option) => option.textContent)).toEqual(["No tag", "FCR"]);
    expect(card(0).queryByText("Add in Settings")).toBeNull();
    await fireEvent.change(select, { target: { value: "t-fcr" } });
    await tick();
    expect(mocked.updateHighlight).toHaveBeenLastCalledWith(7, "h1", expect.objectContaining({ extractionTagId: "t-fcr" }));
    await fireEvent.change(select, { target: { value: "" } });
    await tick();
    expect(mocked.updateHighlight).toHaveBeenLastCalledWith(7, "h1", expect.objectContaining({ extractionTagId: null }));
  });

  it("points to Settings when the library has no tags yet", async () => {
    mocked.listExtractionTags.mockResolvedValue([]);
    await show();
    expect(card(0).getByText("Add in Settings")).toBeTruthy();
  });

  it("saves a note once, shortly after typing stops", async () => {
    await show();
    vi.useFakeTimers();
    await type(0, "F");
    await vi.advanceTimersByTimeAsync(NOTE_SAVE_DELAY_MS - 1);
    await type(0, "FCR 1.4");
    await vi.advanceTimersByTimeAsync(NOTE_SAVE_DELAY_MS - 1);
    expect(mocked.updateHighlight).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(mocked.updateHighlight).toHaveBeenCalledExactlyOnceWith(7, "h1", expect.objectContaining({ note: "FCR 1.4" }));
    expect(card(0).getByText("Saved")).toBeTruthy();
    expect(card(1).queryByText("Saved")).toBeNull();
  });

  it("saves the note at once when the field is left", async () => {
    await show();
    await type(1, "new note");
    await fireEvent.blur(note(1));
    await tick();
    expect(mocked.updateHighlight).toHaveBeenCalledExactlyOnceWith(7, "h2", expect.objectContaining({ note: "new note" }));
  });

  it("saves a waiting note for the right paper when another paper is opened", async () => {
    const { store } = await show();
    await type(0, "typed just now");
    mocked.listHighlights.mockResolvedValue([]);
    await store.open(7, "p2");
    expect(mocked.updateHighlight).toHaveBeenCalledExactlyOnceWith(7, "h1", expect.objectContaining({ note: "typed just now" }));
  });

  it("keeps an unsaved note on screen, also after switching papers, until Try again works", async () => {
    const { store } = await show();
    mocked.updateHighlight.mockRejectedValue({ code: "other", detail: "disk full" });
    await type(0, "do not lose me");
    await fireEvent.blur(note(0));
    expect(await screen.findByText(SAVE_ERROR)).toBeTruthy();
    expect(note(0).value).toBe("do not lose me");
    expect(card(0).queryByText("Saved")).toBeNull();

    await store.open(7, "p2");
    await store.open(7, "p1");
    await tick();
    expect(note(0).value).toBe("do not lose me");
    expect(screen.getAllByText(SAVE_ERROR)).toHaveLength(1);

    mocked.updateHighlight.mockResolvedValue();
    await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await tick();
    expect(mocked.updateHighlight).toHaveBeenLastCalledWith(7, "h1", expect.objectContaining({ note: "do not lose me" }));
    expect(screen.queryByText(SAVE_ERROR)).toBeNull();
    expect(card(0).getByText("Saved")).toBeTruthy();
  });

  it("never lets an older save overtake a newer one", async () => {
    const { store } = await show();
    let finish = () => {};
    mocked.updateHighlight.mockImplementationOnce(() => new Promise<void>((resolve) => (finish = resolve)));
    const first = store.edit("h1", { categoryId: "c-met" });
    const second = store.edit("h1", { extractionTagId: "t-fcr" });
    await tick();
    expect(mocked.updateHighlight).toHaveBeenCalledTimes(1);
    finish();
    await Promise.all([first, second]);
    expect(mocked.updateHighlight).toHaveBeenLastCalledWith(7, "h1", {
      categoryId: "c-met",
      extractionTagId: "t-fcr",
      note: "",
    });
  });

  it("deletes only after the user confirms", async () => {
    await show();
    await fireEvent.click(card(0).getByRole("button", { name: "Delete highlight" }));
    const dialog = screen.getByRole("dialog", { name: "Delete highlight?" });
    expect(dialog.textContent?.replace(/\s+/g, " ")).toContain(
      "This highlight on page 2 and its note will be permanently deleted. This can't be undone.",
    );
    // Focus starts on the safe button.
    expect(document.activeElement?.textContent?.trim()).toBe("Cancel");
    await fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(mocked.deleteHighlight).not.toHaveBeenCalled();
    expect(cards()).toHaveLength(2);

    await fireEvent.click(card(0).getByRole("button", { name: "Delete highlight" }));
    await fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Delete highlight" }));
    await tick();
    expect(mocked.deleteHighlight).toHaveBeenCalledExactlyOnceWith(7, "h1");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(cards()).toHaveLength(1);
    expect(card(0).getByText("text of h2")).toBeTruthy();
  });

  it("keeps the highlight when deleting fails", async () => {
    await show();
    mocked.deleteHighlight.mockRejectedValue({ code: "other", detail: "disk full" });
    await fireEvent.click(card(0).getByRole("button", { name: "Delete highlight" }));
    await fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Delete highlight" }));
    expect(await screen.findByText(GENERIC_ERROR)).toBeTruthy();
    expect(cards()).toHaveLength(2);
  });

  it("changes nothing in a read-only library, but still jumps", async () => {
    const { store } = await show(true);
    expect((card(0).getByRole("button", { name: /Change category/ }) as HTMLButtonElement).disabled).toBe(true);
    expect((card(0).getByLabelText("Tag") as HTMLSelectElement).disabled).toBe(true);
    expect(note(0).readOnly).toBe(true);
    expect(card(0).queryByRole("button", { name: "Delete highlight" })).toBeNull();
    await fireEvent.click(card(0).getByText("text of h1"));
    expect(store.jump).toMatchObject({ id: "h1" });
  });

  it("adds a new highlight in page order and selects it", async () => {
    const { store } = await show();
    mocked.addHighlight.mockResolvedValue(highlight("h3", 3));
    await store.add({ page: 3, categoryId: "c-inc", text: "t", rects: highlight("h3", 3).rects }, false);
    expect(store.highlights.map((each) => each.id)).toEqual(["h1", "h3", "h2"]);
    expect(store.activeId).toBe("h3");
  });

  it("drops the answer for a paper that is no longer open", async () => {
    const store = createHighlightStore();
    let late = (_: api.Highlight[]) => {};
    mocked.listHighlights.mockImplementationOnce(() => new Promise((resolve) => (late = resolve)));
    const first = store.open(7, "p1");
    mocked.listHighlights.mockResolvedValue([highlight("other", 1)]);
    await store.open(7, "p2");
    late([highlight("h1", 2)]);
    await first;
    expect(store.highlights.map((each) => each.id)).toEqual(["other"]);
  });
});
