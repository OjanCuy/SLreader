// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
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

const library = (problem: api.LibrarySummary["problem"] = null, session = 7): api.LibrarySummary => ({
  id: 1,
  folderPath: "C:\\reviews\\aqua",
  name: "Aquaculture",
  paperCount: 1,
  lastOpenedAt: null,
  problem,
  cloudSynced: false,
  session,
});

const file: api.BackupFile = { path: "C:\\b\\aqua.json", libraryName: "Aquaculture", paperCount: 3 };
const nothingReplaced: api.RestorePreview = { replacedPapers: 0, replacedHighlights: 0, papersAfter: 4 };

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});

beforeEach(() => {
  vi.resetAllMocks();
  mocked.listLibraries.mockResolvedValue([]);
  mocked.listPapers.mockResolvedValue([paper]);
  mocked.readPaperPdf.mockReturnValue(new Promise(() => {}));
  mocked.onFileDrag.mockResolvedValue(() => {});
  mocked.onPaperTextChanged.mockResolvedValue(() => {});
  mocked.listExtractionTags.mockResolvedValue([]);
  mocked.getScreening.mockResolvedValue({ decision: "undecided", exclusionReasonId: null, assessmentNote: "" });
  mocked.getMetadata.mockResolvedValue({
    metadata: { refNo: null, title: "", year: null, sourceDatabase: "", doi: "" },
    sameRefNo: [],
  });
  mocked.pickBackup.mockResolvedValue(file);
  mocked.checkRestore.mockResolvedValue(nothingReplaced);
  mocked.restoreBackup.mockResolvedValue({ addedReasons: 0, addedTags: 0 });
  mocked.openLibrary.mockResolvedValue(library(null, 8));
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

const messages = () => toasts.map((toast) => toast.message);

function show(problem: api.LibrarySummary["problem"] = null) {
  const onswitch = vi.fn();
  render(WorkspacePlaceholder, { library: library(problem), onswitch, onshowall: vi.fn() });
  return { onswitch };
}

async function menuItem(name: RegExp) {
  await screen.findByRole("button", { name: "a.pdf" });
  await fireEvent.click(screen.getByRole("button", { name: "Export" }));
  return screen.getByRole("menuitem", { name }) as HTMLButtonElement;
}

const restoreItem = () => menuItem(/Restore from backup/);

describe("backing up (FR-07.1)", () => {
  it("saves the backup and names the file", async () => {
    mocked.exportBackup.mockResolvedValue({ status: "saved", fileName: "Aquaculture backup.json" });
    show();
    await fireEvent.click(await menuItem(/Backup \(JSON\)/));
    await vi.waitFor(() => expect(messages()).toEqual(["Exported Aquaculture backup.json"]));
    expect(mocked.exportBackup).toHaveBeenCalledExactlyOnceWith(7, undefined);
  });

  it("offers Save as when the file is open in another app", async () => {
    mocked.exportBackup.mockResolvedValueOnce({
      status: "in_use",
      fileName: "b.json",
      suggestedName: "b (2).json",
    });
    show();
    await fireEvent.click(await menuItem(/Backup \(JSON\)/));
    await vi.waitFor(() => expect(toasts[0]?.actions?.[0]?.label).toBe("Save as…"));
    const action = toasts[0]!.actions![0]!;
    dismissToast(toasts[0]!.id);
    action.onselect();
    await vi.waitFor(() => expect(mocked.exportBackup).toHaveBeenLastCalledWith(7, "b (2).json"));
  });

  it("is possible for a read-only library", async () => {
    show("newer_version");
    expect((await menuItem(/Backup \(JSON\)/)).disabled).toBe(false);
  });
});

describe("restoring into the open library (FR-07.2)", () => {
  it("restores at once when nothing is replaced, then opens the library afresh", async () => {
    const { onswitch } = show();
    await fireEvent.click(await restoreItem());
    await vi.waitFor(() => expect(onswitch).toHaveBeenCalledWith(library(null, 8)));
    expect(mocked.restoreBackup).toHaveBeenCalledExactlyOnceWith(7, file.path);
    expect(messages()).toEqual(["Backup restored."]);
  });

  it("asks first, naming what will be replaced", async () => {
    mocked.checkRestore.mockResolvedValue({ replacedPapers: 12, replacedHighlights: 40, papersAfter: 12 });
    show();
    await fireEvent.click(await restoreItem());
    expect(
      await screen.findByText(
        "12 papers and 40 highlights in this library will be replaced with their versions from the backup. Everything else is kept.",
      ),
    ).toBeTruthy();
    expect(mocked.restoreBackup).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "Restore" }));
    await vi.waitFor(() => expect(mocked.restoreBackup).toHaveBeenCalledOnce());
  });

  it("changes nothing when the user cancels", async () => {
    mocked.checkRestore.mockResolvedValue({ replacedPapers: 1, replacedHighlights: 0, papersAfter: 1 });
    const { onswitch } = show();
    await fireEvent.click(await restoreItem());
    await fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(screen.queryByText(/will be replaced/)).toBeNull();
    expect(mocked.restoreBackup).not.toHaveBeenCalled();
    expect(onswitch).not.toHaveBeenCalled();
  });

  it("reports the reasons and tags it added", async () => {
    mocked.restoreBackup.mockResolvedValue({ addedReasons: 2, addedTags: 1 });
    show();
    await fireEvent.click(await restoreItem());
    await vi.waitFor(() =>
      expect(messages()).toEqual(["Backup restored. Added 2 exclusion reasons and 1 tag from the backup."]),
    );
  });

  it("refuses more than 250 papers before changing anything", async () => {
    mocked.checkRestore.mockResolvedValue({ replacedPapers: 0, replacedHighlights: 0, papersAfter: 263 });
    show();
    await fireEvent.click(await restoreItem());
    await vi.waitFor(() =>
      expect(messages()).toEqual(["Library limit is 250 papers. This backup would make 263."]),
    );
    expect(mocked.restoreBackup).not.toHaveBeenCalled();
  });

  it("says when the file is not a backup", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.pickBackup.mockRejectedValue({ code: "not_a_backup", detail: "" });
    show();
    await fireEvent.click(await restoreItem());
    await vi.waitFor(() => expect(messages()).toEqual(["This file isn't a SLReader backup."]));
  });

  it("says when the backup is from a newer SLReader", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.pickBackup.mockRejectedValue({ code: "newer_backup", detail: "" });
    show();
    await fireEvent.click(await restoreItem());
    await vi.waitFor(() =>
      expect(messages()).toEqual([
        "This backup was made with a newer SLReader. Update the app to restore it.",
      ]),
    );
  });

  it("says when the restore could not be saved", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.restoreBackup.mockRejectedValue({ code: "other", detail: "disk full" });
    const { onswitch } = show();
    await fireEvent.click(await restoreItem());
    await vi.waitFor(() =>
      expect(messages()).toEqual(["Couldn't save: the disk is full or the folder is read-only."]),
    );
    expect(onswitch).not.toHaveBeenCalled();
  });

  it("does nothing when no file is chosen", async () => {
    mocked.pickBackup.mockResolvedValue(null);
    show();
    await fireEvent.click(await restoreItem());
    await vi.waitFor(() => expect(mocked.pickBackup).toHaveBeenCalled());
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(mocked.checkRestore).not.toHaveBeenCalled();
    expect(messages()).toEqual([]);
  });

  it("saves a note being typed before choosing the backup", async () => {
    const order: string[] = [];
    mocked.setAssessmentNote.mockImplementation(async () => void order.push("saved"));
    mocked.pickBackup.mockImplementation(async () => (order.push("picked"), null));
    show();
    await fireEvent.click(await screen.findByRole("button", { name: "a.pdf" }));
    await fireEvent.input(await screen.findByLabelText("Assessment notes"), { target: { value: "n" } });
    await fireEvent.click(await restoreItem());
    await vi.waitFor(() => expect(order).toEqual(["saved", "picked"]));
  });

  it("is not offered for a read-only library", async () => {
    show("newer_version");
    expect((await restoreItem()).disabled).toBe(true);
  });
});
