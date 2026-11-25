// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { flushSync } from "svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import type { FileDrag, ImportReport, LibrarySummary } from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
import ToastRegion from "../lib/components/ToastRegion.svelte";
import { importMessages } from "../lib/importTexts";
import WorkspacePlaceholder from "./WorkspacePlaceholder.svelte";

vi.mock("../lib/api");
const mocked = vi.mocked(api);

const library = (over: Partial<LibrarySummary> = {}): LibrarySummary => ({
  id: 1,
  folderPath: "C:\\reviews\\aqua",
  name: "Aquaculture",
  paperCount: 0,
  lastOpenedAt: null,
  problem: null,
  cloudSynced: false,
  session: 7,
  ...over,
});

const report = (...files: [string, api.ImportOutcome][]): ImportReport => ({
  files: files.map(([fileName, outcome]) => ({
    fileName,
    path: `C:\\d\\${fileName}`,
    outcome,
    identicalTo: outcome === "identical" ? "original.pdf" : null,
  })),
  added: files.filter(([, o]) => o === "added" || o === "attached").length,
});

let drag: (event: FileDrag) => void = () => {};

beforeEach(() => {
  vi.resetAllMocks();
  mocked.listLibraries.mockResolvedValue([]);
  mocked.listPapers.mockResolvedValue([]);
  mocked.onFileDrag.mockImplementation(async (handler) => {
    drag = handler;
    return () => {};
  });
});

afterEach(() => {
  cleanup();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

async function show(over: Partial<LibrarySummary> = {}) {
  render(WorkspacePlaceholder, { library: library(over), onswitch: vi.fn(), onshowall: vi.fn() });
  await vi.waitFor(() => expect(mocked.listPapers).toHaveBeenCalledWith(7));
}

const messages = () => toasts.map((toast) => toast.message);

describe("importing PDFs", () => {
  it("shows the empty state with Upload PDF and the drop hint", async () => {
    await show();
    expect(await screen.findByText("No papers yet.")).toBeTruthy();
    expect(screen.getByText("or drop PDF files here")).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Upload PDF" })).toHaveLength(2);
  });

  it("imports the picked files, reports them, and lists the papers", async () => {
    mocked.pickPdfs.mockResolvedValue(["C:\\d\\a.pdf", "C:\\d\\b.pdf"]);
    mocked.importPdfs.mockResolvedValue(report(["a.pdf", "added"], ["b.pdf", "added"]));
    await show();
    mocked.listPapers.mockResolvedValue([
      { id: "1", fileName: "a.pdf", title: "", fileMissing: false },
      { id: "2", fileName: "b.pdf", title: "", fileMissing: true },
    ]);

    await fireEvent.click(screen.getAllByRole("button", { name: "Upload PDF" })[0]);
    await vi.waitFor(() => expect(messages()).toEqual(["2 PDFs added."]));
    expect(mocked.importPdfs).toHaveBeenCalledWith(7, ["C:\\d\\a.pdf", "C:\\d\\b.pdf"], false);
    expect(await screen.findByText("a.pdf")).toBeTruthy();
    expect(screen.getByText("PDF file is missing.")).toBeTruthy();
  });

  it("does nothing when the file dialog is cancelled", async () => {
    mocked.pickPdfs.mockResolvedValue([]);
    await show();
    await fireEvent.click(screen.getAllByRole("button", { name: "Upload PDF" })[0]);
    await vi.waitFor(() => expect(mocked.pickPdfs).toHaveBeenCalled());
    expect(mocked.importPdfs).not.toHaveBeenCalled();
  });

  it("imports dropped files and shows the overlay while dragging", async () => {
    mocked.importPdfs.mockResolvedValue(report(["a.pdf", "added"]));
    await show();
    await vi.waitFor(() => expect(mocked.onFileDrag).toHaveBeenCalled());

    drag({ type: "enter", paths: ["C:\\d\\a.pdf"] });
    flushSync();
    expect(screen.getByText("Drop PDF files to add them")).toBeTruthy();

    drag({ type: "drop", paths: ["C:\\d\\a.pdf", "C:\\d\\notes.txt"] });
    flushSync();
    expect(screen.queryByText("Drop PDF files to add them")).toBeNull();
    await vi.waitFor(() =>
      expect(mocked.importPdfs).toHaveBeenCalledWith(
        7,
        ["C:\\d\\a.pdf", "C:\\d\\notes.txt"],
        false,
      ),
    );
  });

  it("rejects a drag without PDFs and imports nothing", async () => {
    await show();
    await vi.waitFor(() => expect(mocked.onFileDrag).toHaveBeenCalled());

    drag({ type: "enter", paths: ["C:\\d\\notes.docx"] });
    flushSync();
    expect(screen.getByText("Only PDF files can be added")).toBeTruthy();
    drag({ type: "drop", paths: ["C:\\d\\notes.docx"] });
    flushSync();
    expect(screen.queryByText("Only PDF files can be added")).toBeNull();
    expect(mocked.importPdfs).not.toHaveBeenCalled();

    drag({ type: "enter", paths: ["C:\\d\\a.pdf"] });
    drag({ type: "leave" });
    flushSync();
    expect(screen.queryByText("Drop PDF files to add them")).toBeNull();
  });

  it("does not import into a read-only library", async () => {
    await show({ problem: "newer_version" });
    for (const button of screen.getAllByRole("button", { name: "Upload PDF" })) {
      expect((button as HTMLButtonElement).disabled).toBe(true);
    }
    await vi.waitFor(() => expect(mocked.onFileDrag).toHaveBeenCalled());
    drag({ type: "drop", paths: ["C:\\d\\a.pdf"] });
    await vi.waitFor(() =>
      expect(messages()).toEqual([
        "Created with a newer SLReader. Update the app to edit this library.",
      ]),
    );
    expect(mocked.importPdfs).not.toHaveBeenCalled();
  });
});

describe("identical PDFs", () => {
  const QUESTION = "This PDF looks identical to original.pdf already in the library.";

  async function uploadIdentical() {
    mocked.pickPdfs.mockResolvedValue(["C:\\d\\copy.pdf"]);
    mocked.importPdfs.mockResolvedValueOnce(report(["copy.pdf", "identical"]));
    const view = render(WorkspacePlaceholder, {
      library: library(),
      onswitch: vi.fn(),
      onshowall: vi.fn(),
    });
    render(ToastRegion);
    await fireEvent.click(screen.getAllByRole("button", { name: "Upload PDF" })[0]);
    await screen.findByText(QUESTION);
    return view;
  }

  it("asks with Add anyway and Skip, and keeps asking until answered", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    await uploadIdentical();
    vi.advanceTimersByTime(60_000);
    expect(messages()).toEqual([QUESTION]);
    expect(screen.getByRole("button", { name: "Add anyway" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Skip" })).toBeTruthy();
    vi.useRealTimers();
  });

  it("imports the file after Add anyway", async () => {
    await uploadIdentical();
    mocked.importPdfs.mockResolvedValueOnce(report(["copy.pdf", "added"]));

    await fireEvent.click(screen.getByRole("button", { name: "Add anyway" }));
    await vi.waitFor(() => expect(messages()).toEqual(["1 PDF added."]));
    expect(mocked.importPdfs).toHaveBeenLastCalledWith(7, ["C:\\d\\copy.pdf"], true);
  });

  it("does nothing more after Skip", async () => {
    await uploadIdentical();
    await fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    expect(messages()).toEqual([]);
    expect(mocked.importPdfs).toHaveBeenCalledTimes(1);
  });

  it("withdraws the question when the library is left", async () => {
    const view = await uploadIdentical();
    view.unmount();
    expect(messages()).toEqual([]);
  });
});

describe("import messages", () => {
  it("say how many PDFs were added", () => {
    expect(importMessages(report(["a.pdf", "added"]))).toEqual([
      { message: "1 PDF added.", tone: "success" },
    ]);
    expect(importMessages(report(["a.pdf", "added"], ["b.pdf", "attached"]))[0].message).toBe(
      "2 PDFs added.",
    );
  });

  it("name each skipped or unreadable file", () => {
    const list = importMessages(
      report(["a.pdf", "added"], ["b.pdf", "already_in_library"], ["c.txt", "not_pdf"]),
    ).map((m) => m.message);
    expect(list).toEqual(["1 PDF added.", "Already in library: b.pdf", "Could not open c.txt."]);
  });

  it("explain the 250-paper limit once, instead of the success message", () => {
    const list = importMessages(
      report(["a.pdf", "added"], ["b.pdf", "over_limit"], ["c.pdf", "over_limit"]),
    );
    expect(list).toEqual([
      { message: "Library limit is 250 papers. 1 of 3 files were added.", tone: "error" },
    ]);
  });

  it("report a save failure once", () => {
    const list = importMessages(report(["a.pdf", "write_failed"], ["b.pdf", "write_failed"]));
    expect(list.map((m) => m.message)).toEqual([
      "Couldn't save: the disk is full or the folder is read-only.",
    ]);
  });
});
