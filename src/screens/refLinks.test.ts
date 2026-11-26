// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
import { linkedNumbers, offerMessage, renumberLinks, updatedMessage } from "../lib/refLinks";
import WorkspacePlaceholder from "./WorkspacePlaceholder.svelte";

vi.mock("../lib/api");
const mocked = vi.mocked(api);

const paper = (id: string, fileName: string): api.PaperListItem => ({
  id,
  fileName,
  title: "",
  fileMissing: false,
  textStatus: "text_ready",
  extracting: false,
});

beforeEach(() => {
  vi.resetAllMocks();
  // jsdom has no modal dialogs.
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
  HTMLDialogElement.prototype.close = function () {
    this.open = false;
  };
  mocked.listLibraries.mockResolvedValue([]);
  mocked.listPapers.mockResolvedValue([paper("1", "a.pdf"), paper("2", "b.pdf")]);
  mocked.readPaperPdf.mockReturnValue(new Promise(() => {}));
  mocked.onFileDrag.mockResolvedValue(() => {});
  mocked.onPaperTextChanged.mockResolvedValue(() => {});
  mocked.listHighlights.mockResolvedValue([]);
  mocked.listHighlightCategories.mockResolvedValue([]);
  mocked.listExtractionTags.mockResolvedValue([]);
  mocked.listExclusionReasons.mockResolvedValue([]);
  mocked.listReferences.mockResolvedValue([]);
  mocked.getScreening.mockImplementation(async (_session, id) => ({
    decision: "undecided",
    exclusionReasonId: null,
    assessmentNote: id === "1" ? "Like [#12], see [#99]" : "",
  }));
  mocked.getMetadata.mockImplementation(async (_session, id) => ({
    metadata: { refNo: id === "1" ? 12 : null, title: "", year: null, sourceDatabase: "", doi: "" },
    sameRefNo: [],
  }));
  mocked.setMetadata.mockResolvedValue([]);
  mocked.setAssessmentNote.mockResolvedValue();
  mocked.countNoteLinks.mockResolvedValue(3);
  mocked.updateNoteLinks.mockResolvedValue(3);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

const messages = () => toasts.map((toast) => toast.message);
const note = () => screen.getByLabelText("Assessment notes") as HTMLTextAreaElement;
const refNo = () => screen.getByLabelText("Reference No") as HTMLInputElement;
const link = (name: string) => screen.queryByRole("button", { name });

async function open(file = "a.pdf") {
  render(WorkspacePlaceholder, {
    library: {
      id: 1,
      folderPath: "C:\\reviews\\aqua",
      name: "Aquaculture",
      paperCount: 2,
      lastOpenedAt: null,
      problem: null,
      cloudSynced: false,
      session: 7,
    },
    onswitch: vi.fn(),
    onshowall: vi.fn(),
  });
  await fireEvent.click(await screen.findByRole("button", { name: file }));
  await screen.findByLabelText("Assessment notes");
}

/** Types a new Reference No and leaves the field. */
async function renumber(value: string) {
  await fireEvent.input(refNo(), { target: { value } });
  await fireEvent.blur(refNo());
}

describe("reading links", () => {
  it("finds each number once, and only exact links", () => {
    expect(linkedNumbers("[#46] [#7] [#046] [# 5] #6 [#1234567890]")).toEqual([46, 7]);
  });

  it("renumbers only the links to the old number", () => {
    expect(renumberLinks("[#12] [#123] [#012] 12", 12, 4)).toBe("[#4] [#123] [#4] 12");
  });

  it("words one and several notes", () => {
    expect(offerMessage(3, 12, 14)).toBe("3 notes link to #12. Update them to #14?");
    expect(offerMessage(1, 12, 14)).toBe("1 note links to #12. Update it to #14?");
    expect(updatedMessage(1)).toBe("1 note updated.");
  });
});

describe("following a link (FR-05.5)", () => {
  it("shows the links under the note, also while typing", async () => {
    await open();
    expect(link("[#12]")).toBeTruthy();
    await fireEvent.input(note(), { target: { value: "Only [#5]" } });
    expect(link("[#5]")).toBeTruthy();
    expect(link("[#12]")).toBeNull();
  });

  it("opens the paper with that number", async () => {
    mocked.paperForRefNo.mockResolvedValue("2");
    await open();
    await fireEvent.click(link("[#99]")!);
    await vi.waitFor(() => expect(note().value).toBe(""));
    expect(mocked.paperForRefNo).toHaveBeenCalledExactlyOnceWith(7, 99);
    expect(screen.getByRole("button", { name: "b.pdf" }).getAttribute("aria-current")).toBe("true");
  });

  it("says when no paper has the number", async () => {
    mocked.paperForRefNo.mockResolvedValue(null);
    await open();
    await fireEvent.click(link("[#99]")!);
    await vi.waitFor(() => expect(messages()).toEqual(["No paper with Reference No 99."]));
    expect(note().value).toBe("Like [#12], see [#99]");
  });
});

describe("a changed reference number (PRD Section 8)", () => {
  const dialog = () => screen.findByRole("dialog", { name: "Update note links?" });

  it("offers an update once the field is left, and keeps the notes on Keep as is", async () => {
    await open();
    await fireEvent.input(refNo(), { target: { value: "1" } });
    await fireEvent.input(refNo(), { target: { value: "14" } });
    expect(mocked.countNoteLinks).not.toHaveBeenCalled();
    await fireEvent.blur(refNo());
    const box = await dialog();
    expect(box.textContent).toContain("3 notes link to #12. Update them to #14?");
    expect(mocked.countNoteLinks).toHaveBeenCalledExactlyOnceWith(7, 12, 14);
    const keep = within(box).getByRole("button", { name: "Keep as is" });
    expect(document.activeElement).toBe(keep);
    await fireEvent.click(keep);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(mocked.updateNoteLinks).not.toHaveBeenCalled();
    expect(note().value).toBe("Like [#12], see [#99]");
  });

  it("updates the links on screen after the confirmation", async () => {
    await open();
    await renumber("14");
    await fireEvent.click(within(await dialog()).getByRole("button", { name: "Update links" }));
    await vi.waitFor(() => expect(note().value).toBe("Like [#14], see [#99]"));
    expect(mocked.updateNoteLinks).toHaveBeenCalledExactlyOnceWith(7, 12, 14);
    expect(messages()).toEqual(["3 notes updated."]);
    // Only note text: never a decision (FR-11.4).
    expect(mocked.setDecision).not.toHaveBeenCalled();
  });

  it("keeps the old text when the update fails, with Try again", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.updateNoteLinks.mockRejectedValue({ code: "other", detail: "disk full" });
    await open();
    await renumber("14");
    await fireEvent.click(within(await dialog()).getByRole("button", { name: "Update links" }));
    await vi.waitFor(() =>
      expect(messages()).toEqual(["Couldn't save: the disk is full or the folder is read-only."]),
    );
    expect(note().value).toBe("Like [#12], see [#99]");
  });

  it("asks nothing when no note links to the old number", async () => {
    mocked.countNoteLinks.mockResolvedValue(0);
    await open();
    await renumber("14");
    await vi.waitFor(() => expect(mocked.countNoteLinks).toHaveBeenCalled());
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("asks nothing when a number is removed or set for the first time", async () => {
    await open();
    await renumber("");
    await fireEvent.input(refNo(), { target: { value: "5" } });
    await fireEvent.input(refNo(), { target: { value: "12" } });
    await fireEvent.blur(refNo());
    await fireEvent.click(screen.getByRole("button", { name: "b.pdf" }));
    await vi.waitFor(() => expect(refNo().value).toBe(""));
    await renumber("3");
    await vi.waitFor(() => expect(mocked.setMetadata).toHaveBeenCalledWith(7, "2", expect.anything()));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(mocked.countNoteLinks).not.toHaveBeenCalled();
  });

  it("asks only after the number could be saved", async () => {
    mocked.setMetadata.mockRejectedValueOnce(new Error("disk full"));
    await open();
    await renumber("14");
    const retry = await screen.findByRole("button", { name: "Try again" });
    expect(mocked.countNoteLinks).not.toHaveBeenCalled();
    await fireEvent.click(retry);
    expect(await dialog()).toBeTruthy();
  });

  it("offers an update after linking a reference", async () => {
    mocked.listReferences.mockResolvedValue([
      { id: "r", no: 46, title: "T", year: null, linkedPaperId: null, linkedPaperName: null },
    ]);
    mocked.linkReference.mockResolvedValue({
      status: "linked",
      metadata: { refNo: 46, title: "T", year: null, sourceDatabase: "", doi: "" },
      sameRefNo: [],
    });
    await open();
    await fireEvent.click(screen.getByRole("combobox", { name: "Link to Reference No" }));
    await fireEvent.click(screen.getByRole("option"));
    expect(await dialog()).toBeTruthy();
    expect(mocked.countNoteLinks).toHaveBeenCalledWith(7, 12, 46);
  });
});
