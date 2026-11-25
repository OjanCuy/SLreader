// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
import ToastRegion from "../lib/components/ToastRegion.svelte";
import { attachMessage } from "../lib/importTexts";
import * as pdfjs from "./pdfjs";
import Reader from "./Reader.svelte";

vi.mock("../lib/api");
vi.mock("./pdfjs", async (original) => ({
  ...(await original<typeof import("./pdfjs")>()),
  openPdf: vi.fn(),
}));
const mocked = vi.mocked(api);
const openPdf = vi.mocked(pdfjs.openPdf);

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
    openPdf.mockRejectedValue(new pdfjs.PdfOpenError(false, new Error("Invalid PDF structure")));
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
