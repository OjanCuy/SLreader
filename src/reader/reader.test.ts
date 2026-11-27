// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
import ToastRegion from "../lib/components/ToastRegion.svelte";
import { attachMessage } from "../lib/importTexts";
import { SAVE_ERROR } from "../lib/libraryTexts";
import { createHighlightStore } from "./highlightStore.svelte";
import * as pdfjs from "./pdfjs";
import Reader from "./Reader.svelte";
import * as selection from "./selection";

vi.mock("../lib/api");
// jsdom has no layout to measure a selection; selection.test.ts covers the measuring.
vi.mock("./selection");
const readSelection = vi.mocked(selection.readSelection);
vi.mock("./pdfjs", async (original) => ({
  ...(await original<typeof import("./pdfjs")>()),
  openPdf: vi.fn(),
  readPageTexts: vi.fn(),
}));
const mocked = vi.mocked(api);
const openPdf = vi.mocked(pdfjs.openPdf);
const readPageTexts = vi.mocked(pdfjs.readPageTexts);

const LETTER = { width: 612, height: 792 };

/** A document whose pages never finish loading: enough for the toolbar and layout. */
function fakePdf(pages: number): pdfjs.OpenedPdf {
  return {
    doc: { getPage: () => new Promise(() => {}) } as never,
    sizes: Array.from({ length: pages }, () => LETTER),
    close: vi.fn(),
  };
}

// bind:clientWidth needs ResizeObserver, which jsdom lacks. The view has no size here.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};

const missing: api.LibraryError = { code: "not_found", detail: "the paper has no PDF file" };

beforeEach(() => {
  vi.resetAllMocks();
  // jsdom has no modal dialogs.
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
  mocked.readPaperPdf.mockResolvedValue(new ArrayBuffer(8));
  openPdf.mockImplementation(async () => fakePdf(3));
});

afterEach(() => {
  cleanup();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

function show(props: Partial<{ paperId: string | null; readOnly: boolean; onattached: () => void }> = {}) {
  render(ToastRegion);
  return render(Reader, { session: 7, paperId: "p1", readOnly: false, onattached: vi.fn(), ...props });
}

describe("Reader states (Brief Section 7, S-02 Reader)", () => {
  it("asks to pick a paper when none is selected", () => {
    show({ paperId: null });
    expect(screen.getByText("Select a paper from the list.")).toBeTruthy();
    expect(mocked.readPaperPdf).not.toHaveBeenCalled();
  });

  it("shows Loading PDF… while the file is read", async () => {
    mocked.readPaperPdf.mockReturnValue(new Promise(() => {}));
    show();
    expect(screen.getByRole("status").textContent).toContain("Loading PDF…");
    expect(mocked.readPaperPdf).toHaveBeenCalledWith(7, "p1");
  });

  it("opens the PDF with the toolbar", async () => {
    show();
    expect(await screen.findByText("/ 3")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Zoom out" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Zoom in" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Fit width" })).toBeTruthy();
    expect((screen.getByLabelText("Page") as HTMLInputElement).value).toBe("1");
  });

  it("says the file is missing, with an icon and Attach PDF", async () => {
    mocked.readPaperPdf.mockRejectedValue(missing);
    show();
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("PDF file is missing.");
    expect(alert.querySelector("[data-icon='error']")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Attach PDF" })).toBeTruthy();
  });

  it("offers no Attach PDF in a read-only library", async () => {
    mocked.readPaperPdf.mockRejectedValue(missing);
    show({ readOnly: true });
    await screen.findByText("PDF file is missing.");
    expect(screen.queryByRole("button", { name: "Attach PDF" })).toBeNull();
  });

  it("says a PDF that PDF.js cannot read can't be opened", async () => {
    openPdf.mockRejectedValue(new pdfjs.PdfOpenError("damaged", new Error("Invalid PDF structure")));
    show();
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("This PDF can't be opened.");
    expect(alert.querySelector("[data-icon='error']")).toBeTruthy();
  });

  it("closes the document when another paper is opened", async () => {
    const first = fakePdf(2);
    openPdf.mockResolvedValueOnce(first);
    const { rerender } = show();
    await screen.findByText("/ 2");
    await rerender({ paperId: "p2" });
    expect(first.close).toHaveBeenCalled();
    expect(await screen.findByText("/ 3")).toBeTruthy();
    expect(mocked.readPaperPdf).toHaveBeenLastCalledWith(7, "p2");
  });
});

describe("password-protected PDF (PRD Section 8)", () => {
  const RIGHT = "tilapia-19";

  beforeEach(() => {
    openPdf.mockImplementation(async (_bytes, password) => {
      if (password === undefined) throw new pdfjs.PdfOpenError("locked", new Error("No password given"));
      if (password !== RIGHT) throw new pdfjs.PdfOpenError("wrong_password", new Error("Incorrect Password"));
      return fakePdf(3);
    });
    mocked.extractLockedPaperText.mockResolvedValue();
  });

  const field = () => screen.getByLabelText("Password") as HTMLInputElement;
  const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;

  async function enter(password: string) {
    await fireEvent.input(field(), { target: { value: password } });
    await fireEvent.click(button("Open"));
  }

  /** Every argument the app passed to Rust, as text. */
  function everythingSentToRust(): string {
    const calls: unknown[] = Object.values(mocked).map((fn) => (vi.isMockFunction(fn) ? fn.mock.calls : []));
    return JSON.stringify(calls);
  }

  it("asks for the password when the paper is opened", async () => {
    show();
    const dialog = await screen.findByRole("dialog", { name: "Enter password" });
    expect(dialog.textContent).toContain("This PDF is password-protected.");
    expect(field().type).toBe("password");
    expect(button("Skip")).toBeTruthy();
    expect(button("Open").disabled).toBe(true);
  });

  it("says a wrong password is wrong and lets the user try again", async () => {
    show();
    await screen.findByRole("dialog");
    await enter("wrong");
    expect(await screen.findByText("Wrong password. Try again.")).toBeTruthy();
    expect(openPdf).toHaveBeenLastCalledWith(expect.any(ArrayBuffer), "wrong");
    // PDF.js took the bytes: they are read again for each try.
    expect(mocked.readPaperPdf).toHaveBeenCalledTimes(2);
    await vi.waitFor(() => expect(field().value).toBe(""));
    expect(mocked.extractLockedPaperText).not.toHaveBeenCalled();

    await enter(RIGHT);
    expect(await screen.findByText("/ 3")).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("extracts the text once with the right password and sends it nowhere else", async () => {
    show();
    await screen.findByRole("dialog");
    await enter(RIGHT);
    await screen.findByText("/ 3");
    expect(mocked.extractLockedPaperText).toHaveBeenCalledExactlyOnceWith(7, "p1", RIGHT);
    mocked.extractLockedPaperText.mockClear();
    expect(everythingSentToRust()).not.toContain(RIGHT);
  });

  it("opens but extracts nothing in a read-only library", async () => {
    show({ readOnly: true });
    await screen.findByRole("dialog");
    await enter(RIGHT);
    await screen.findByText("/ 3");
    expect(mocked.extractLockedPaperText).not.toHaveBeenCalled();
  });

  it("Skip leaves the PDF closed until Enter password is clicked", async () => {
    show();
    await screen.findByRole("dialog");
    await fireEvent.click(button("Skip"));
    expect(screen.queryByRole("dialog")).toBeNull();
    const state = screen.getByRole("status");
    expect(state.textContent).toContain("This PDF is password-protected.");
    expect(state.querySelector("[data-icon]")).toBeTruthy();

    await fireEvent.click(button("Enter password"));
    await enter(RIGHT);
    expect(await screen.findByText("/ 3")).toBeTruthy();
  });

  it("does not use one paper's password for another paper", async () => {
    const { rerender } = show();
    await screen.findByRole("dialog");
    await enter(RIGHT);
    await screen.findByText("/ 3");

    await rerender({ paperId: "p2" });
    expect(await screen.findByRole("dialog", { name: "Enter password" })).toBeTruthy();
    expect(openPdf).toHaveBeenLastCalledWith(expect.any(ArrayBuffer), undefined);
    // Opening p1 again asks again: the password was not kept.
    await fireEvent.click(button("Skip"));
    await rerender({ paperId: "p1" });
    await screen.findByRole("dialog");
    expect(openPdf).toHaveBeenLastCalledWith(expect.any(ArrayBuffer), undefined);
  });

  it("tells a missing password, a wrong one, and a damaged file apart", () => {
    const passwordException = (code: number) => Object.assign(new Error("x"), { name: "PasswordException", code });
    expect(pdfjs.problemOf(passwordException(1))).toBe("locked");
    expect(pdfjs.problemOf(passwordException(2))).toBe("wrong_password");
    expect(pdfjs.problemOf(Object.assign(new Error("x"), { name: "InvalidPDFException" }))).toBe("damaged");
    expect(pdfjs.problemOf(null)).toBe("damaged");
  });
});

describe("highlights (FR-03.1, FR-03.2, FR-03.5)", () => {
  const SELECTED: selection.PageSelection = {
    page: 1,
    text: "Feed conversion ratio was 1.4",
    rects: [{ x: 0.1, y: 0.2, w: 0.5, h: 0.02 }],
    crossed: false,
    bounds: { left: 100, top: 200, right: 400, bottom: 212 },
  };

  /** The 4 defaults (FR-03.1). */
  const DEFAULTS: api.HighlightCategory[] = [
    { id: "c-inc", label: "Inclusion", color: "green", sortOrder: 0 },
    { id: "c-exc", label: "Exclusion", color: "red", sortOrder: 1 },
    { id: "c-met", label: "Method", color: "blue", sortOrder: 2 },
    { id: "c-res", label: "Result", color: "yellow", sortOrder: 3 },
  ];

  beforeEach(() => {
    mocked.listHighlights.mockResolvedValue([]);
    mocked.listHighlightCategories.mockResolvedValue(DEFAULTS);
    mocked.addHighlight.mockImplementation(async (_session, _paper, highlight) => ({
      ...highlight,
      id: `h-${highlight.categoryId}`,
      note: "",
      extractionTagId: null,
      createdAt: "2026-11-26T00:00:00Z",
    }));
  });

  async function open(props: Partial<{ readOnly: boolean }> = {}) {
    const view = show(props);
    await screen.findByText("/ 3");
    return view;
  }

  /** The user releases the mouse after selecting text. */
  async function selectText(selected: selection.PageSelection | null = SELECTED) {
    readSelection.mockReturnValue(selected);
    await fireEvent.pointerUp(document);
    await new Promise((resolve) => setTimeout(resolve, 0));
  }

  const colorButton = (name: string) => screen.queryByRole("button", { name });

  it("offers the library's categories for a selection and saves the picked one", async () => {
    await open();
    expect(mocked.listHighlightCategories).toHaveBeenCalledWith(7);
    expect(colorButton("Method")).toBeNull();
    await selectText();
    const buttons = screen.getAllByRole("button").filter((button) => button.hasAttribute("aria-keyshortcuts"));
    expect(buttons.map((button) => [button.textContent?.trim(), button.getAttribute("aria-keyshortcuts")])).toEqual([
      ["Inclusion", "1"],
      ["Exclusion", "2"],
      ["Method", "3"],
      ["Result", "4"],
    ]);

    await fireEvent.click(colorButton("Method")!);
    expect(mocked.addHighlight).toHaveBeenCalledExactlyOnceWith(7, "p1", {
      page: 1,
      categoryId: "c-met",
      text: "Feed conversion ratio was 1.4",
      rects: SELECTED.rects,
    });
    expect(colorButton("Method")).toBeNull();
    const mark = await screen.findByRole("img", { name: "Highlight, Method, page 1" });
    expect(mark.getAttribute("style")).toContain("var(--hl-blue-fill)");
    const piece = mark.querySelector<HTMLElement>(".hl-rect")!;
    expect([piece.style.left, piece.style.top, piece.style.width, piece.style.height]).toEqual([
      "10%",
      "20%",
      "50%",
      "2%",
    ]);
  });

  it("shows categories the user named, in their colors, up to key 8", async () => {
    const eight: api.HighlightCategory[] = (
      ["gray", "pink", "orange", "purple", "yellow", "blue", "red", "green"] as const
    ).map((color, index) => ({ id: `c${index}`, label: `Tag ${index + 1}`, color, sortOrder: index }));
    mocked.listHighlightCategories.mockResolvedValue(eight);
    await open();
    await selectText();
    expect(colorButton("Tag 8")?.querySelector(".swatch")?.getAttribute("style")).toContain(
      "var(--hl-green-swatch)",
    );
    await fireEvent.keyDown(window, { key: "8" });
    expect(mocked.addHighlight).toHaveBeenLastCalledWith(7, "p1", expect.objectContaining({ categoryId: "c7" }));
    expect(await screen.findByRole("img", { name: "Highlight, Tag 8, page 1" })).toBeTruthy();
  });

  it("ignores a key with no category", async () => {
    await open();
    await selectText();
    await fireEvent.keyDown(window, { key: "5" });
    expect(mocked.addHighlight).not.toHaveBeenCalled();
    expect(colorButton("Inclusion")).toBeTruthy();
  });

  it("picks with the number keys and closes with Esc", async () => {
    await open();
    await selectText();
    await fireEvent.keyDown(window, { key: "4" });
    expect(mocked.addHighlight).toHaveBeenLastCalledWith(7, "p1", expect.objectContaining({ categoryId: "c-res" }));

    await selectText();
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(colorButton("Inclusion")).toBeNull();
    await fireEvent.keyDown(window, { key: "1" });
    expect(mocked.addHighlight).toHaveBeenCalledTimes(1);
  });

  it("does not take keys typed in a field", async () => {
    await open();
    await selectText();
    await fireEvent.keyDown(screen.getByLabelText("Page"), { key: "2" });
    expect(mocked.addHighlight).not.toHaveBeenCalled();
  });

  it("says when a selection over two pages was saved on the first page only", async () => {
    await open();
    await selectText({ ...SELECTED, crossed: true });
    await fireEvent.click(colorButton("Exclusion")!);
    expect(await screen.findByText("Highlights can't cross pages; saved on page 1.")).toBeTruthy();
    expect(mocked.addHighlight).toHaveBeenCalledWith(7, "p1", expect.objectContaining({ page: 1, categoryId: "c-exc" }));
  });

  it("keeps the highlight to try again when saving fails", async () => {
    await open();
    mocked.addHighlight.mockRejectedValueOnce({ code: "other", detail: "disk full" });
    await selectText();
    await fireEvent.click(colorButton("Inclusion")!);
    expect(await screen.findByText(SAVE_ERROR)).toBeTruthy();
    expect(screen.queryByRole("img", { name: /^Highlight/ })).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("img", { name: "Highlight, Inclusion, page 1" })).toBeTruthy();
    expect(mocked.addHighlight).toHaveBeenCalledTimes(2);
    expect(mocked.addHighlight.mock.calls[0]).toEqual(mocked.addHighlight.mock.calls[1]);
  });

  it("offers no colors in a read-only library", async () => {
    await open({ readOnly: true });
    await selectText();
    expect(colorButton("Inclusion")).toBeNull();
    expect(mocked.addHighlight).not.toHaveBeenCalled();
  });

  it("shows the stored highlights when the paper is opened again", async () => {
    mocked.listHighlights.mockResolvedValue([
      {
        id: "h1",
        page: 2,
        categoryId: "c-exc",
        text: "Pond trial",
        rects: [{ x: 0.1, y: 0.5, w: 0.3, h: 0.02 }],
        note: "",
        extractionTagId: null,
        createdAt: "2026-11-25T00:00:00Z",
      },
    ]);
    await open();
    expect(mocked.listHighlights).toHaveBeenCalledWith(7, "p1");
    expect(await screen.findByRole("img", { name: "Highlight, Exclusion, page 2" })).toBeTruthy();
  });

  it("opens the PDF even when the highlights cannot be read", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.listHighlights.mockRejectedValue({ code: "other", detail: "busy" });
    await open();
    expect(screen.queryByRole("img", { name: /^Highlight/ })).toBeNull();
  });

  it("makes no highlights when the categories cannot be read", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.listHighlightCategories.mockRejectedValue({ code: "other", detail: "busy" });
    await open();
    await selectText();
    expect(colorButton("Inclusion")).toBeNull();
    expect(readSelection).not.toHaveBeenCalled();
  });
});

describe("Attach PDF", () => {
  it("copies the chosen file in and opens the paper again", async () => {
    mocked.readPaperPdf.mockRejectedValueOnce(missing);
    mocked.pickPdf.mockResolvedValue("C:\\Downloads\\kim.pdf");
    mocked.attachPdf.mockResolvedValue("attached");
    const onattached = vi.fn();
    show({ onattached });

    await fireEvent.click(await screen.findByRole("button", { name: "Attach PDF" }));
    expect(await screen.findByText("/ 3")).toBeTruthy();
    expect(mocked.attachPdf).toHaveBeenCalledWith(7, "p1", "C:\\Downloads\\kim.pdf");
    expect(onattached).toHaveBeenCalled();
    expect(toasts).toHaveLength(0);
  });

  it("does nothing when the picker is cancelled", async () => {
    mocked.readPaperPdf.mockRejectedValue(missing);
    mocked.pickPdf.mockResolvedValue(null);
    show();
    await fireEvent.click(await screen.findByRole("button", { name: "Attach PDF" }));
    await vi.waitFor(() => expect(mocked.pickPdf).toHaveBeenCalled());
    expect(mocked.attachPdf).not.toHaveBeenCalled();
    expect(screen.getByText("PDF file is missing.")).toBeTruthy();
  });

  it("reports a file that is not a PDF with the import text", async () => {
    mocked.readPaperPdf.mockRejectedValue(missing);
    mocked.pickPdf.mockResolvedValue("C:\\Downloads\\notes.pdf");
    mocked.attachPdf.mockResolvedValue("not_pdf");
    show();
    await fireEvent.click(await screen.findByRole("button", { name: "Attach PDF" }));
    expect(await screen.findByText("Could not open notes.pdf.")).toBeTruthy();
    expect(screen.getByText("PDF file is missing.")).toBeTruthy();
  });

  it("uses the approved texts only", () => {
    expect(attachMessage("write_failed", "C:\\a.pdf")?.message).toBe(
      "Couldn't save: the disk is full or the folder is read-only.",
    );
    expect(attachMessage("not_pdf", "/home/u/b.pdf")?.message).toBe("Could not open b.pdf.");
    expect(attachMessage("attached", "C:\\a.pdf")).toBeNull();
    expect(attachMessage("already_in_library", "C:\\a.pdf")).toBeNull();
  });
});

describe("ReaderToolbar", () => {
  it("zooms in and out by steps", async () => {
    show();
    await screen.findByText("100%");
    await fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(await screen.findByText("110%")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    await fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(await screen.findByText("90%")).toBeTruthy();
  });

  it("puts back the current page when the typed page is not a number", async () => {
    show();
    const field = (await screen.findByLabelText("Page")) as HTMLInputElement;
    await fireEvent.focus(field);
    await fireEvent.input(field, { target: { value: "abc" } });
    await fireEvent.blur(field);
    expect(field.value).toBe("1");
  });

  it("only draws the pages near the view (FR-02.4)", async () => {
    openPdf.mockResolvedValue(fakePdf(300));
    const { container } = show();
    await screen.findByText("/ 300");
    expect(container.querySelectorAll(".slot")).toHaveLength(300);
    // The view has no height in this test: the first page and one more are drawn.
    expect(container.querySelectorAll(".slot .page").length).toBeLessThanOrEqual(2);
  });
});

describe("search in the PDF (FR-02.1)", () => {
  const line = (str: string) => ({ str, hasEOL: true });

  beforeEach(() => {
    // "fish" once on page 1 and twice on page 3.
    readPageTexts.mockResolvedValue([[line("Fish farms")], [line("none")], [line("fish and fish")]]);
  });

  async function open() {
    const view = show();
    await screen.findByText("/ 3");
    const field = screen.getByRole("textbox", { name: "Search in PDF" }) as HTMLInputElement;
    const count = () => view.container.querySelector(".search [role='status']")!;
    const type = (value: string) => fireEvent.input(field, { target: { value } });
    const shows = (text: string) => vi.waitFor(() => expect(count().textContent?.trim()).toBe(text));
    return { field, count, type, shows };
  }

  const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;

  it("counts results on all pages and reads the text only once", async () => {
    const { type, shows } = await open();
    expect(readPageTexts).not.toHaveBeenCalled();
    await type("fish");
    await shows("1/3");
    await type("farms");
    await shows("1/1");
    expect(readPageTexts).toHaveBeenCalledTimes(1);
  });

  it("moves with Enter, Shift+Enter, and the buttons, going round", async () => {
    const { field, type, shows } = await open();
    await type("fish");
    await shows("1/3");
    await fireEvent.keyDown(field, { key: "Enter" });
    await shows("2/3");
    await fireEvent.keyDown(field, { key: "Enter", shiftKey: true });
    await shows("1/3");
    await fireEvent.click(button("Previous result"));
    await shows("3/3");
    await fireEvent.click(button("Next result"));
    await shows("1/3");
  });

  it("shows 0/0 with a warning icon when nothing is found", async () => {
    const { type, shows, count } = await open();
    await type("salmon");
    await shows("0/0");
    expect(count().querySelector("[data-icon='warning']")).toBeTruthy();
    expect(button("Next result").disabled).toBe(true);
    expect(button("Previous result").disabled).toBe(true);
  });

  it("clears with the button and with Esc", async () => {
    const { field, type, shows } = await open();
    expect(button("Clear search").disabled).toBe(true);
    await type("fish");
    await shows("1/3");
    await fireEvent.click(button("Clear search"));
    expect(field.value).toBe("");
    await shows("");
    await type("fish");
    await fireEvent.keyDown(field, { key: "Escape" });
    expect(field.value).toBe("");
    await shows("");
  });

  it("goes to the search field with Ctrl+F", async () => {
    const { field } = await open();
    const notPrevented = await fireEvent.keyDown(window, { key: "f", ctrlKey: true });
    expect(notPrevented).toBe(false);
    expect(document.activeElement).toBe(field);
  });

  it("tries again after the text could not be read", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    readPageTexts.mockRejectedValueOnce(new Error("closed"));
    const { type, shows } = await open();
    await type("fish");
    await vi.waitFor(() => expect(readPageTexts).toHaveBeenCalledTimes(1));
    await type("fis");
    await shows("1/3");
  });
});

describe("page mode (FR-02.2)", () => {
  const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;

  it("offers Normal, Dark, and Sepia, and starts with the stored one", async () => {
    mocked.getPageMode.mockResolvedValue("sepia");
    const { container } = show();
    await screen.findByText("/ 3");
    expect(screen.getByRole("group", { name: "Page mode" })).toBeTruthy();
    await vi.waitFor(() => expect(radio("Sepia").checked).toBe(true));
    expect(container.querySelector(".slot.sepia .page.sepia")).toBeTruthy();
  });

  it("changes the pages and saves the choice, also for the next paper", async () => {
    mocked.getPageMode.mockResolvedValue("normal");
    mocked.setPageMode.mockResolvedValue();
    const { container, rerender } = show();
    await screen.findByText("/ 3");
    await fireEvent.click(radio("Dark"));
    expect(mocked.setPageMode).toHaveBeenCalledExactlyOnceWith("dark");
    expect(container.querySelector(".page.dark")).toBeTruthy();

    await rerender({ paperId: "p2" });
    await screen.findByText("/ 3");
    expect(radio("Dark").checked).toBe(true);
  });
});

describe("zoom (FR-08.5)", () => {
  // jsdom has no layout: give the view a size, so that "Fit width" has one to fit.
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(850);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(600);
    mocked.setZoom.mockResolvedValue();
  });

  afterEach(() => vi.restoreAllMocks());

  const button = (name: string) => screen.getByRole("button", { name });

  it("fits the width when nothing is stored, and saves nothing by itself", async () => {
    mocked.getZoom.mockResolvedValue(null);
    show();
    // The room between the gaps is as wide as the page at 100%.
    expect(await screen.findByText("100%")).toBeTruthy();
    expect(mocked.setZoom).not.toHaveBeenCalled();
  });

  it("opens at the stored zoom", async () => {
    mocked.getZoom.mockResolvedValue(1.5);
    show();
    expect(await screen.findByText("150%")).toBeTruthy();
    expect(mocked.setZoom).not.toHaveBeenCalled();
  });

  it("saves the zoom and keeps it for the next paper", async () => {
    mocked.getZoom.mockResolvedValue(1.5);
    const { rerender } = show();
    await screen.findByText("150%");
    await fireEvent.click(button("Zoom in"));
    expect(mocked.setZoom).toHaveBeenCalledExactlyOnceWith(1.75);
    await rerender({ paperId: "p2" });
    await vi.waitFor(() => expect(openPdf).toHaveBeenCalledTimes(2));
    expect(await screen.findByText("175%")).toBeTruthy();
    expect(mocked.getZoom).toHaveBeenCalledTimes(1);
  });

  it("saves Fit width as no zoom", async () => {
    mocked.getZoom.mockResolvedValue(1.5);
    show();
    await screen.findByText("150%");
    await fireEvent.click(button("Fit width"));
    expect(mocked.setZoom).toHaveBeenCalledExactlyOnceWith(null);
    expect(await screen.findByText("100%")).toBeTruthy();
  });

  it("opens the PDF when the zoom cannot be read", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.getZoom.mockRejectedValue(new Error("broken"));
    show();
    expect(await screen.findByText("100%")).toBeTruthy();
  });
});

describe("last page read (FR-02.3)", () => {
  // jsdom has no layout: give elements a size and a scroll position that can be set.
  const scrolled = new WeakMap<Element, number>();

  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(850);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(600);
    Object.defineProperty(HTMLElement.prototype, "scrollTop", {
      configurable: true,
      get() {
        return scrolled.get(this) ?? 0;
      },
      set(value: number) {
        scrolled.set(this, value);
      },
    });
    openPdf.mockImplementation(async () => fakePdf(10));
    mocked.saveLastPage.mockResolvedValue();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    delete (HTMLElement.prototype as { scrollTop?: number }).scrollTop;
  });

  const field = () => screen.getByLabelText("Page") as HTMLInputElement;

  async function goTo(page: number) {
    await fireEvent.focus(field());
    await fireEvent.input(field(), { target: { value: String(page) } });
    await fireEvent.blur(field());
  }

  async function openAt(page: number, readOnly = false) {
    mocked.getLastPage.mockResolvedValue(page);
    const view = show({ readOnly });
    await screen.findByText("/ 10");
    // The view moves to its start page just after "Fit width" has sized the pages.
    await new Promise((resolve) => setTimeout(resolve, 20));
    return view;
  }

  it("opens the paper at its stored page without saving it again", async () => {
    const view = await openAt(6);
    expect(mocked.getLastPage).toHaveBeenCalledWith(7, "p1");
    await vi.waitFor(() => expect(field().value).toBe("6"));
    view.unmount();
    expect(mocked.saveLastPage).not.toHaveBeenCalled();
  });

  it("jumps to a highlight and flashes it (FR-03.4)", async () => {
    mocked.listHighlightCategories.mockResolvedValue([{ id: "c", label: "Method", color: "blue", sortOrder: 0 }]);
    mocked.listHighlights.mockResolvedValue([
      {
        id: "h1",
        page: 6,
        categoryId: "c",
        text: "t",
        rects: [{ x: 0.1, y: 0.5, w: 0.3, h: 0.02 }],
        note: "",
        extractionTagId: null,
        createdAt: "",
      },
    ]);
    mocked.getLastPage.mockResolvedValue(1);
    const store = createHighlightStore();
    const { container } = render(Reader, { session: 7, paperId: "p1", readOnly: false, onattached: vi.fn(), store });
    await screen.findByText("/ 10");
    await new Promise((resolve) => setTimeout(resolve, 20));

    store.show("h1");
    await vi.waitFor(() => expect(field().value).toBe("6"));
    await vi.waitFor(() => expect(container.querySelector(".highlight.flash.active")).toBeTruthy());
    // The highlight itself is in view, not only the top of its page.
    const scroller = container.querySelector<HTMLElement>(".scroller")!;
    const slot = container.querySelectorAll<HTMLElement>(".slot")[5];
    const target = parseFloat(slot.style.top) + 0.5 * parseFloat(slot.style.height);
    expect(target).toBeGreaterThan(scroller.scrollTop);
    expect(target).toBeLessThan(scroller.scrollTop + 600);
  });

  it("goes to the stored page, and saves new ones, at a stored zoom too (FR-08.5)", async () => {
    mocked.getZoom.mockResolvedValue(1.5);
    await openAt(6);
    expect(screen.getByText("150%")).toBeTruthy();
    await vi.waitFor(() => expect(field().value).toBe("6"));
    expect(mocked.saveLastPage).not.toHaveBeenCalled();
    await goTo(4);
    await vi.waitFor(() => expect(mocked.saveLastPage).toHaveBeenCalledExactlyOnceWith(7, "p1", 4));
  });

  it("saves the page the user moved to", async () => {
    await openAt(1);
    await goTo(4);
    await vi.waitFor(() => expect(mocked.saveLastPage).toHaveBeenCalledExactlyOnceWith(7, "p1", 4));
  });

  it("saves at once, for the right paper, when another paper is opened", async () => {
    const view = await openAt(1);
    await goTo(8);
    await view.rerender({ paperId: "p2" });
    expect(mocked.saveLastPage).toHaveBeenCalledExactlyOnceWith(7, "p1", 8);
  });

  it("saves nothing in a read-only library", async () => {
    const view = await openAt(3, true);
    await vi.waitFor(() => expect(field().value).toBe("3"));
    await goTo(5);
    view.unmount();
    expect(mocked.saveLastPage).not.toHaveBeenCalled();
  });

  it("shows the last page when the stored one is past the end", async () => {
    await openAt(99);
    await vi.waitFor(() => expect(field().value).toBe("10"));
  });

  it("starts at page 1 when the stored page cannot be read", async () => {
    mocked.getLastPage.mockRejectedValue(new Error("busy"));
    show();
    await screen.findByText("/ 10");
    expect(field().value).toBe("1");
  });
});
