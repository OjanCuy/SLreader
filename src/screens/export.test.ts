// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
import WorkspacePlaceholder from "./WorkspacePlaceholder.svelte";

vi.mock("../lib/api");
const mocked = vi.mocked(api);

const paper: api.PaperListItem = {
  id: "1",
  fileName: "a.pdf",
  title: "",
  fileMissing: false,
  textStatus: "text_ready",
  extracting: false,
};

beforeEach(() => {
  vi.resetAllMocks();
  mocked.listLibraries.mockResolvedValue([]);
  mocked.listPapers.mockResolvedValue([paper]);
  mocked.readPaperPdf.mockReturnValue(new Promise(() => {}));
  mocked.onFileDrag.mockResolvedValue(() => {});
  mocked.onPaperTextChanged.mockResolvedValue(() => {});
  mocked.getScreening.mockResolvedValue({ decision: "undecided", exclusionReasonId: null, assessmentNote: "" });
  mocked.getMetadata.mockResolvedValue({
    metadata: { refNo: null, title: "", year: null, sourceDatabase: "", doi: "" },
    sameRefNo: [],
  });
  mocked.exportDecisions.mockResolvedValue("decisions.csv");
  mocked.exportExtraction.mockResolvedValue("extraction.csv");
  mocked.listExtractionTags.mockResolvedValue([{ id: "f", label: "FCR" }]);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

const messages = () => toasts.map((toast) => toast.message);

function show(problem: api.LibrarySummary["problem"] = null) {
  render(WorkspacePlaceholder, {
    library: {
      id: 1,
      folderPath: "C:\\reviews\\aqua",
      name: "Aquaculture",
      paperCount: 1,
      lastOpenedAt: null,
      problem,
      cloudSynced: false,
      session: 7,
    },
    onswitch: vi.fn(),
    onshowall: vi.fn(),
  });
}

/** Opens the Export menu once the paper list is read, and returns the item. */
async function exportItem(papers = true, item = /Decisions \(CSV\)/) {
  if (papers) await screen.findByRole("button", { name: "a.pdf" });
  else await screen.findByText("No papers yet.");
  await fireEvent.click(screen.getByRole("button", { name: "Export" }));
  return screen.getByRole("menuitem", { name: item }) as HTMLButtonElement;
}

const extractionItem = (papers = true) => exportItem(papers, /Extraction \(CSV\)/);

describe("exporting the decisions (FR-06.1)", () => {
  it("exports and names the saved file", async () => {
    show();
    await fireEvent.click(await exportItem());
    await vi.waitFor(() => expect(messages()).toEqual(["Exported decisions.csv"]));
    expect(mocked.exportDecisions).toHaveBeenCalledExactlyOnceWith(7);
  });

  it("is also possible for a read-only library", async () => {
    show("newer_version");
    expect((await exportItem()).disabled).toBe(false);
  });

  it("is disabled with a reason while there are no papers", async () => {
    mocked.listPapers.mockResolvedValue([]);
    show();
    const item = await exportItem(false);
    expect(item.disabled).toBe(true);
    expect(item.textContent).toContain("No papers yet.");
  });

  it("says nothing when the user cancels", async () => {
    mocked.exportDecisions.mockResolvedValue(null);
    show();
    await fireEvent.click(await exportItem());
    await vi.waitFor(() => expect(mocked.exportDecisions).toHaveBeenCalled());
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(messages()).toEqual([]);
  });

  it("says when the file cannot be written", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.exportDecisions.mockRejectedValue({ code: "other", detail: "access denied" });
    show();
    await fireEvent.click(await exportItem());
    await vi.waitFor(() =>
      expect(messages()).toEqual(["Couldn't save: the disk is full or the folder is read-only."]),
    );
  });

  it("saves a note that is still being typed before exporting", async () => {
    const order: string[] = [];
    mocked.setAssessmentNote.mockImplementation(async () => void order.push("saved"));
    mocked.exportDecisions.mockImplementation(async () => (order.push("exported"), null));
    show();
    await fireEvent.click(await screen.findByRole("button", { name: "a.pdf" }));
    await fireEvent.input(await screen.findByLabelText("Assessment notes"), { target: { value: "n" } });
    await fireEvent.click(await exportItem());
    await vi.waitFor(() => expect(order).toEqual(["saved", "exported"]));
  });

  it("does not export while a change could not be saved", async () => {
    mocked.setAssessmentNote.mockRejectedValue(new Error("disk full"));
    show();
    await fireEvent.click(await screen.findByRole("button", { name: "a.pdf" }));
    await fireEvent.input(await screen.findByLabelText("Assessment notes"), { target: { value: "n" } });
    await fireEvent.click(await exportItem());
    await vi.waitFor(() =>
      expect(messages()).toEqual(["Couldn't save: the disk is full or the folder is read-only."]),
    );
    expect(mocked.exportDecisions).not.toHaveBeenCalled();
  });
});

describe("exporting the extraction (FR-06.2)", () => {
  it("exports and names the saved file", async () => {
    show();
    await fireEvent.click(await extractionItem());
    await vi.waitFor(() => expect(messages()).toEqual(["Exported extraction.csv"]));
    expect(mocked.exportExtraction).toHaveBeenCalledExactlyOnceWith(7);
    expect(mocked.exportDecisions).not.toHaveBeenCalled();
  });

  it("is also possible for a read-only library", async () => {
    show("newer_version");
    expect((await extractionItem()).disabled).toBe(false);
  });

  it("is disabled with a reason while there are no tags", async () => {
    mocked.listExtractionTags.mockResolvedValue([]);
    show();
    const item = await extractionItem();
    await vi.waitFor(() => expect(item.disabled).toBe(true));
    expect(item.textContent).toContain("No tags yet.");
    const decisions = screen.getByRole("menuitem", { name: /Decisions/ }) as HTMLButtonElement;
    expect(decisions.disabled).toBe(false);
  });

  it("names the missing papers first", async () => {
    mocked.listPapers.mockResolvedValue([]);
    mocked.listExtractionTags.mockResolvedValue([]);
    show();
    const item = await extractionItem(false);
    expect(item.disabled).toBe(true);
    expect(item.textContent).toContain("No papers yet.");
    expect(item.textContent).not.toContain("No tags yet.");
  });

  it("stays possible when the tags cannot be read", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.listExtractionTags.mockRejectedValue(new Error("busy"));
    show();
    expect((await extractionItem()).disabled).toBe(false);
  });

  it("saves a note that is still being typed before exporting", async () => {
    const order: string[] = [];
    mocked.setAssessmentNote.mockImplementation(async () => void order.push("saved"));
    mocked.exportExtraction.mockImplementation(async () => (order.push("exported"), null));
    show();
    await fireEvent.click(await screen.findByRole("button", { name: "a.pdf" }));
    await fireEvent.input(await screen.findByLabelText("Assessment notes"), { target: { value: "n" } });
    await fireEvent.click(await extractionItem());
    await vi.waitFor(() => expect(order).toEqual(["saved", "exported"]));
  });
});
