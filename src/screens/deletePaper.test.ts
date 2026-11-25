// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
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
});

afterEach(() => {
  cleanup();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

function show(problem: api.LibrarySummary["problem"] = null) {
  render(WorkspacePlaceholder, {
    library: {
      id: 1,
      folderPath: "C:\\reviews\\aqua",
      name: "Aquaculture",
      paperCount: 2,
      lastOpenedAt: null,
      problem,
      cloudSynced: false,
      session: 7,
    },
    onswitch: vi.fn(),
    onshowall: vi.fn(),
  });
}

const deleteButton = () => screen.queryByRole("button", { name: "Delete paper" });

/** Selects a.pdf and clicks "Delete paper"; returns the dialog. */
async function askToDelete() {
  show();
  await fireEvent.click(await screen.findByRole("button", { name: "a.pdf" }));
  await fireEvent.click(deleteButton()!);
  return screen.getByRole("dialog", { name: "Delete paper?" });
}

describe("deleting a paper (FR-01.7)", () => {
  it("offers Delete paper only when a paper is selected", async () => {
    show();
    await screen.findByRole("button", { name: "a.pdf" });
    expect(deleteButton()).toBeNull();
  });

  it("offers nothing in a read-only library", async () => {
    show("newer_version");
    await fireEvent.click(await screen.findByRole("button", { name: "a.pdf" }));
    expect(deleteButton()).toBeNull();
  });

  it("asks first, with focus on Cancel, and keeps the paper on Cancel", async () => {
    const dialog = await askToDelete();
    expect(dialog.textContent?.replace(/\s+/g, " ")).toContain(
      "a.pdf and its PDF file, decision, notes, highlights, chats, and AI suggestions will be permanently deleted. This can't be undone.",
    );
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });
    expect(document.activeElement).toBe(cancel);

    await fireEvent.click(cancel);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(mocked.deletePaper).not.toHaveBeenCalled();
  });

  it("deletes after confirmation and closes the reader", async () => {
    mocked.deletePaper.mockResolvedValue();
    const dialog = await askToDelete();
    mocked.listPapers.mockResolvedValue([paper("2", "b.pdf")]);

    await fireEvent.click(within(dialog).getByRole("button", { name: "Delete paper" }));
    expect(await screen.findByText("Select a paper from the list.")).toBeTruthy();
    expect(mocked.deletePaper).toHaveBeenCalledExactlyOnceWith(7, "1");
    expect(screen.queryByRole("button", { name: "a.pdf" })).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(deleteButton()).toBeNull();
  });

  it("shows an error and keeps the paper when deleting fails", async () => {
    mocked.deletePaper.mockRejectedValue({ code: "other", detail: "locked" });
    const dialog = await askToDelete();

    await fireEvent.click(within(dialog).getByRole("button", { name: "Delete paper" }));
    await vi.waitFor(() =>
      expect(toasts.map((toast) => toast.message)).toEqual(["Something went wrong. Try again."]),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByRole("button", { name: "a.pdf" })).toBeTruthy();
  });
});
