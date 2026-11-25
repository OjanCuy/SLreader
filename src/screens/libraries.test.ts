// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import type { LibrarySummary } from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
import { lastOpenedText, libraryErrorMessage, libraryTitle } from "../lib/libraryTexts";
import LibrariesScreen from "./LibrariesScreen.svelte";

vi.mock("../lib/api");
const mocked = vi.mocked(api);

const library = (over: Partial<LibrarySummary> = {}): LibrarySummary => ({
  id: 1,
  folderPath: "C:\\reviews\\aqua",
  name: "Aquaculture",
  paperCount: 12,
  lastOpenedAt: "2026-11-25T10:00:00.000Z",
  problem: null,
  session: null,
  ...over,
});

const missing = library({
  id: 2,
  folderPath: "D:\\gone\\fish",
  name: null,
  paperCount: null,
  problem: "not_found",
});

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
});

afterEach(() => {
  cleanup();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

async function show(libraries: LibrarySummary[]) {
  mocked.listLibraries.mockResolvedValue(libraries);
  const onopen = vi.fn();
  render(LibrariesScreen, { onopen });
  await vi.waitFor(() => expect(mocked.listLibraries).toHaveBeenCalled());
  await screen.findByRole(libraries.length ? "list" : "button", libraries.length ? {} : { name: "Create library" });
  return { onopen };
}

const button = (name: string | RegExp) => screen.getByRole("button", { name });

async function chooseFromMenu(title: string, item: string) {
  await fireEvent.click(button(`Options for ${title}`));
  await fireEvent.click(await screen.findByRole("menuitem", { name: item }));
}

describe("Libraries screen", () => {
  it("shows the empty state with one Create library button", async () => {
    await show([]);
    expect(
      await screen.findByText("No libraries yet. Create one to start a review."),
    ).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Create library" })).toHaveLength(1);
  });

  it("shows name, paper count and date, and opens a library on click", async () => {
    mocked.openLibrary.mockResolvedValue(library());
    const { onopen } = await show([library()]);

    const card = button(/Aquaculture.*12 papers.*Last opened/s);
    await fireEvent.click(card);
    await vi.waitFor(() => expect(onopen).toHaveBeenCalledWith(library()));
    expect(mocked.openLibrary).toHaveBeenCalledWith(1);
  });

  it("marks a missing library and lets the user locate its folder", async () => {
    mocked.pickFolder.mockResolvedValue("E:\\fish");
    mocked.locateLibrary.mockResolvedValue();
    await show([missing]);

    const card = screen.getByRole("listitem");
    expect(within(card).getByText("Folder not found.")).toBeTruthy();
    expect(within(card).getByText("D:\\gone\\fish")).toBeTruthy();
    // It cannot be opened.
    expect(within(card).queryByRole("button", { name: /^fish/ })).toBeNull();

    await fireEvent.click(button("Locate folder"));
    await vi.waitFor(() => expect(mocked.locateLibrary).toHaveBeenCalledWith(2, "E:\\fish"));
    await vi.waitFor(() => expect(mocked.listLibraries).toHaveBeenCalledTimes(2));
  });

  it("does nothing when the folder dialog is cancelled", async () => {
    mocked.pickFolder.mockResolvedValue(null);
    const { onopen } = await show([library()]);
    await fireEvent.click(button("Open library"));
    await vi.waitFor(() => expect(mocked.pickFolder).toHaveBeenCalled());
    expect(mocked.openLibraryFolder).not.toHaveBeenCalled();
    expect(onopen).not.toHaveBeenCalled();
  });

  it("explains a folder without a library, without showing technical detail", async () => {
    mocked.pickFolder.mockResolvedValue("C:\\empty");
    mocked.openLibraryFolder.mockRejectedValue({ code: "not_found", detail: "secret detail" });
    const { onopen } = await show([library()]);

    await fireEvent.click(button("Open library"));
    await vi.waitFor(() =>
      expect(toasts.map((t) => t.message)).toEqual(["No SLReader library in this folder."]),
    );
    expect(onopen).not.toHaveBeenCalled();
  });

  it("removes from the list only after confirmation", async () => {
    mocked.removeLibrary.mockResolvedValue();
    await show([library()]);

    await chooseFromMenu("Aquaculture", "Remove from list");
    expect(
      screen.getByText("The library folder and its files stay on your computer."),
    ).toBeTruthy();
    // The safe button has the focus.
    expect(document.activeElement).toBe(button("Cancel"));
    await fireEvent.click(button("Cancel"));
    expect(mocked.removeLibrary).not.toHaveBeenCalled();

    await chooseFromMenu("Aquaculture", "Remove from list");
    await fireEvent.click(button("Remove from list"));
    await vi.waitFor(() => expect(mocked.removeLibrary).toHaveBeenCalledWith(1));
  });

  it("creates a library once name and folder are given, then opens it", async () => {
    mocked.pickFolder.mockResolvedValue("C:\\new");
    mocked.createLibrary.mockResolvedValue(library());
    const { onopen } = await show([]);

    await fireEvent.click(await screen.findByRole("button", { name: "Create library" }));
    const dialog = screen.getByRole("dialog");
    const submit = within(dialog).getByRole("button", { name: "Create library" });
    expect((submit as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Review" } });
    expect((submit as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(button("Choose folder"));
    await vi.waitFor(() => expect((submit as HTMLButtonElement).disabled).toBe(false));

    await fireEvent.click(submit);
    await vi.waitFor(() => expect(onopen).toHaveBeenCalled());
    expect(mocked.createLibrary).toHaveBeenCalledWith("C:\\new", "Review");
  });

  it("keeps the create dialog open with a message when creating fails", async () => {
    mocked.pickFolder.mockResolvedValue("C:\\used");
    mocked.createLibrary.mockRejectedValue({ code: "already_exists", detail: "x" });
    const { onopen } = await show([library()]);

    await fireEvent.click(button("Create library"));
    await fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Review" } });
    await fireEvent.click(button("Choose folder"));
    const submit = within(screen.getByRole("dialog")).getByRole("button", {
      name: "Create library",
    });
    await vi.waitFor(() => expect((submit as HTMLButtonElement).disabled).toBe(false));
    await fireEvent.click(submit);

    expect(
      await screen.findByText("This folder already has a library. Use Open library instead."),
    ).toBeTruthy();
    expect((screen.getByLabelText("Name") as HTMLInputElement).value).toBe("Review");
    expect(onopen).not.toHaveBeenCalled();
  });

  it("renames a library and reloads the list", async () => {
    mocked.renameLibrary.mockResolvedValue();
    await show([library()]);

    await chooseFromMenu("Aquaculture", "Rename");
    const field = screen.getByLabelText("Name") as HTMLInputElement;
    expect(field.value).toBe("Aquaculture");
    await fireEvent.input(field, { target: { value: "Fish" } });
    await fireEvent.click(button("Rename"));

    await vi.waitFor(() => expect(mocked.renameLibrary).toHaveBeenCalledWith(1, "Fish"));
    await vi.waitFor(() => expect(mocked.listLibraries).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("offers Try again when the list cannot be read", async () => {
    mocked.listLibraries.mockRejectedValueOnce({ code: "other", detail: "x" });
    mocked.listLibraries.mockResolvedValue([library()]);
    render(LibrariesScreen, { onopen: vi.fn() });

    expect(await screen.findByText("Something went wrong. Try again.")).toBeTruthy();
    await fireEvent.click(button("Try again"));
    expect(await screen.findByText("Aquaculture")).toBeTruthy();
  });
});

describe("library texts", () => {
  it("chooses a message by error code and action", () => {
    expect(libraryErrorMessage({ code: "other" }, "create")).toBe(
      "Couldn't save: the disk is full or the folder is read-only.",
    );
    expect(libraryErrorMessage({ code: "other" }, "open")).toBe(
      "Something went wrong. Try again.",
    );
    expect(libraryErrorMessage({ code: "not_found" }, "create")).toBe("Folder not found.");
    expect(libraryErrorMessage({ code: "damaged" }, "open")).toBe(
      "This library's database is damaged.",
    );
    expect(libraryErrorMessage(new Error("boom"), "remove")).toBe(
      "Something went wrong. Try again.",
    );
  });

  it("formats the date and falls back to the folder name", () => {
    // Midday UTC is the same calendar day in nearly every time zone.
    expect(lastOpenedText("2026-09-05T12:00:00.000Z")).toBe("Last opened 5 Sep 2026");
    expect(libraryTitle({ name: null, folderPath: "D:\\gone\\fish\\" })).toBe("fish");
  });
});
