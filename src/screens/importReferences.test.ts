// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
import { duplicatesMessage } from "../lib/referenceTexts";
import ImportReferencesDialog from "./ImportReferencesDialog.svelte";

vi.mock("../lib/api");
const mocked = vi.mocked(api);

beforeEach(() => {
  vi.resetAllMocks();
  // jsdom has no modal dialogs.
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
  HTMLDialogElement.prototype.close = function () {
    this.open = false;
  };
});

afterEach(() => {
  cleanup();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

const LIST = "46;Tilapia ponds";
const field = () => screen.getByRole("textbox", { name: "Reference list" }) as HTMLTextAreaElement;
const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;
const messages = () => toasts.map((toast) => toast.message);

function show() {
  const onclose = vi.fn();
  render(ImportReferencesDialog, { session: 7, onclose });
  return onclose;
}

async function paste(text = LIST) {
  const onclose = show();
  await fireEvent.input(field(), { target: { value: text } });
  await fireEvent.click(button("Import"));
  return onclose;
}

describe("importing a reference list (FR-05.2)", () => {
  it("starts empty, with the example header and Import disabled", () => {
    show();
    expect(field().placeholder).toBe("Reference No; Title; Year; Source Database; DOI");
    expect(button("Import").disabled).toBe(true);
  });

  it("imports, reports the counts and repeated numbers, and closes", async () => {
    mocked.importReferences.mockResolvedValue({
      status: "imported",
      imported: 42,
      skipped: 2,
      duplicates: [12, 46],
    });
    const onclose = await paste();
    await vi.waitFor(() => expect(onclose).toHaveBeenCalledOnce());
    expect(mocked.importReferences).toHaveBeenCalledExactlyOnceWith(7, LIST, false);
    expect(messages()).toEqual(["42 rows imported, 2 skipped.", "Duplicate Reference No: 12, 46."]);
  });

  it("names five repeated numbers at most", () => {
    expect(duplicatesMessage([1, 2, 3, 4, 5, 6, 7, 8])).toBe(
      "Duplicate Reference No: 1, 2, 3, 4, 5 and 3 more.",
    );
  });

  it("stays open when no row is found", async () => {
    mocked.importReferences.mockResolvedValue({ status: "empty" });
    const onclose = await paste("hello");
    expect(await screen.findByText("No reference rows found.")).toBeTruthy();
    expect(onclose).not.toHaveBeenCalled();
    expect(field().value).toBe("hello");
  });

  it("asks before replacing a list, with focus on Cancel, and keeps the text on Cancel", async () => {
    mocked.importReferences.mockResolvedValue({ status: "needs_confirmation", existing: 42 });
    const onclose = await paste();
    const dialog = await screen.findByRole("dialog", { name: "Replace reference list?" });
    expect(dialog.textContent).toContain(
      "The current 42 references will be replaced. Papers keep their metadata, but their links to the old references are removed.",
    );
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });
    expect(document.activeElement).toBe(cancel);

    await fireEvent.click(cancel);
    expect(field().value).toBe(LIST);
    expect(mocked.importReferences).toHaveBeenCalledOnce();
    expect(onclose).not.toHaveBeenCalled();
  });

  it("replaces the list after confirmation", async () => {
    mocked.importReferences
      .mockResolvedValueOnce({ status: "needs_confirmation", existing: 1 })
      .mockResolvedValueOnce({ status: "imported", imported: 1, skipped: 0, duplicates: [] });
    const onclose = await paste();
    await fireEvent.click(await screen.findByRole("button", { name: "Replace list" }));
    await vi.waitFor(() => expect(onclose).toHaveBeenCalledOnce());
    expect(mocked.importReferences).toHaveBeenLastCalledWith(7, LIST, true);
    expect(messages()).toEqual(["1 row imported, 0 skipped."]);
  });

  it("shows a save error inside the dialog and keeps the text", async () => {
    mocked.importReferences.mockRejectedValue({ code: "other", detail: "disk full" });
    const onclose = await paste();
    expect(await screen.findByText("Couldn't save: the disk is full or the folder is read-only.")).toBeTruthy();
    expect(field().value).toBe(LIST);
    expect(onclose).not.toHaveBeenCalled();
  });

  it("fills the field from a CSV file", async () => {
    mocked.pickReferenceFile.mockResolvedValue({ fileName: "list.csv", text: LIST });
    show();
    await fireEvent.click(button("Choose CSV file"));
    await vi.waitFor(() => expect(field().value).toBe(LIST));
    expect(mocked.importReferences).not.toHaveBeenCalled();
  });

  it("says which file could not be opened and keeps the text", async () => {
    mocked.pickReferenceFile.mockResolvedValue({ fileName: "list.xlsx", text: null });
    show();
    await fireEvent.input(field(), { target: { value: "typed" } });
    await fireEvent.click(button("Choose CSV file"));
    expect(await screen.findByText("Could not open list.xlsx.")).toBeTruthy();
    expect(field().value).toBe("typed");
  });
});
