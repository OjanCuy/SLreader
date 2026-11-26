// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
import CriteriaList from "./CriteriaList.svelte";
import SettingsScreen from "./SettingsScreen.svelte";
import { createSettingsStore, SAVE_DELAY_MS } from "./settingsStore.svelte";
import { deleteMessage, nextCode, repeatedCodes } from "./settingsTexts";

vi.mock("../lib/api");
const mocked = vi.mocked(api);

const REASONS: api.Criterion[] = [
  { id: "r1", code: "E1", label: "Review article" },
  { id: "r2", code: "E2", label: "Wrong population" },
];

beforeEach(() => {
  vi.resetAllMocks();
  // jsdom has no modal dialogs.
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
  HTMLDialogElement.prototype.close = function () {
    this.open = false;
  };
  mocked.listCriteria.mockResolvedValue(REASONS.map((reason) => ({ ...reason })));
  mocked.addCriterion.mockResolvedValue("new");
  mocked.updateCriterion.mockResolvedValue();
  mocked.deleteCriterion.mockResolvedValue();
  mocked.reorderCriteria.mockResolvedValue();
  mocked.countCriterionUses.mockResolvedValue(0);
  mocked.getResearchQuestions.mockResolvedValue("RQ1?");
  mocked.setResearchQuestions.mockResolvedValue();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

const messages = () => toasts.map((toast) => toast.message);

async function show(kind: api.CriteriaKind | "questions" = "exclusion", readOnly = false) {
  const store = createSettingsStore(() => 7);
  render(CriteriaList, { store, kind, readOnly });
  await vi.waitFor(() => expect(store.lists[kind].status).toBe("ready"));
  await Promise.resolve();
  return store;
}

const rows = () => screen.queryAllByRole("listitem");
const row = (index: number) => within(rows()[index]!);
const field = (index: number, name: string) => row(index).getByRole("textbox", { name }) as HTMLInputElement;

async function type(input: HTMLInputElement, value: string) {
  await fireEvent.input(input, { target: { value } });
  await fireEvent.blur(input);
}

describe("the settings texts", () => {
  it("offers the next code", () => {
    expect(nextCode("exclusion", [])).toBe("E1");
    expect(nextCode("exclusion", ["E1", "e7", "X9", ""])).toBe("E8");
    expect(nextCode("inclusion", ["I2", "E5"])).toBe("I3");
  });

  it("finds repeated codes whatever their case", () => {
    expect(repeatedCodes(["E1", "e1 ", "E2", "", ""])).toEqual(["E1"]);
  });

  it("names how many papers or highlights lose an item", () => {
    expect(deleteMessage("exclusion", "E1", 0)).toBe("E1 will be deleted. This can't be undone.");
    expect(deleteMessage("exclusion", "E1", 1)).toBe("1 paper uses this reason. Delete anyway?");
    expect(deleteMessage("exclusion", "E1", 3)).toBe("3 papers use this reason. Delete anyway?");
    expect(deleteMessage("tag", "FCR", 5)).toBe("5 highlights use this tag. Delete anyway?");
  });
});

describe("the lists of a library (FR-08.1, FR-08.2, FR-08.4)", () => {
  it("shows an empty list with Add", async () => {
    mocked.listCriteria.mockResolvedValue([]);
    await show("inclusion");
    expect(screen.getByText("No criteria yet.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Add" })).toBeTruthy();
  });

  it("gives tags a name only, with the note about export columns", async () => {
    mocked.listCriteria.mockResolvedValue([{ id: "t1", code: "", label: "FCR" }]);
    await show("tag");
    expect(screen.getByText("Order sets export columns")).toBeTruthy();
    expect(field(0, "Tag name").value).toBe("FCR");
    expect(row(0).queryByRole("textbox", { name: "Code" })).toBeNull();
  });

  it("saves a new row only once its description is filled", async () => {
    await show();
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(field(2, "Code").value).toBe("E3");
    expect(document.activeElement).toBe(field(2, "Description"));
    await fireEvent.blur(field(2, "Code"));
    expect(row(2).queryByText("This field is required.")).toBeNull();
    expect(mocked.addCriterion).not.toHaveBeenCalled();

    await type(field(2, "Description"), "No full text");
    await vi.waitFor(() => expect(messages()).toEqual(["Saved"]));
    expect(mocked.addCriterion).toHaveBeenCalledExactlyOnceWith(7, "exclusion", "E3", "No full text");

    // The next change updates the same row, and only one "Saved" is on screen.
    await type(field(2, "Description"), "No full text available");
    await vi.waitFor(() =>
      expect(mocked.updateCriterion).toHaveBeenCalledWith(7, "exclusion", "new", "E3", "No full text available"),
    );
    expect(mocked.addCriterion).toHaveBeenCalledOnce();
    expect(messages()).toEqual(["Saved"]);
  });

  it("keeps the old value when a required field is emptied", async () => {
    await show();
    await type(field(0, "Description"), "  ");
    expect(row(0).getByText("This field is required.")).toBeTruthy();
    expect(mocked.updateCriterion).not.toHaveBeenCalled();
  });

  it("saves shortly after typing stops, once", async () => {
    await show();
    vi.useFakeTimers();
    await fireEvent.input(field(0, "Description"), { target: { value: "Re" } });
    await fireEvent.input(field(0, "Description"), { target: { value: "Reviews" } });
    expect(mocked.updateCriterion).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(SAVE_DELAY_MS);
    expect(mocked.updateCriterion).toHaveBeenCalledExactlyOnceWith(7, "exclusion", "r1", "E1", "Reviews");
  });

  it("warns about a repeated code and still saves it", async () => {
    await show();
    await type(field(1, "Code"), "e1");
    expect(screen.getByText("Code E1 is used more than once.")).toBeTruthy();
    await vi.waitFor(() =>
      expect(mocked.updateCriterion).toHaveBeenCalledWith(7, "exclusion", "r2", "e1", "Wrong population"),
    );
  });

  it("keeps a change that cannot be saved, with Try again", async () => {
    mocked.updateCriterion.mockRejectedValueOnce({ code: "other", detail: "disk full" });
    const store = await show();
    await type(field(0, "Description"), "Reviews");
    await vi.waitFor(() => expect(store.failed).toBe(true));
    expect(field(0, "Description").value).toBe("Reviews");
    await store.retry();
    expect(store.failed).toBe(false);
    expect(mocked.updateCriterion).toHaveBeenCalledTimes(2);
  });

  it("asks before deleting, with the number of papers, and keeps the row on Cancel", async () => {
    mocked.countCriterionUses.mockResolvedValue(3);
    await show();
    await fireEvent.click(row(0).getByRole("button", { name: "Delete" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete exclusion reason?" });
    expect(dialog.textContent).toContain("3 papers use this reason. Delete anyway?");
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });
    expect(document.activeElement).toBe(cancel);
    await fireEvent.click(cancel);
    expect(mocked.deleteCriterion).not.toHaveBeenCalled();
    expect(rows()).toHaveLength(2);
  });

  it("deletes after the confirmation, also an unused item", async () => {
    await show();
    await fireEvent.click(row(0).getByRole("button", { name: "Delete" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("E1 will be deleted. This can't be undone.");
    expect(mocked.deleteCriterion).not.toHaveBeenCalled();
    await fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await vi.waitFor(() => expect(rows()).toHaveLength(1));
    expect(mocked.deleteCriterion).toHaveBeenCalledExactlyOnceWith(7, "exclusion", "r1");
  });

  it("keeps the row when deleting fails", async () => {
    mocked.deleteCriterion.mockRejectedValue({ code: "other", detail: "locked" });
    await show();
    await fireEvent.click(row(0).getByRole("button", { name: "Delete" }));
    const dialog = await screen.findByRole("dialog");
    await fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await vi.waitFor(() => expect(messages()).toEqual(["Something went wrong. Try again."]));
    expect(rows()).toHaveLength(2);
  });

  it("removes a row that was never saved without asking", async () => {
    await show();
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await fireEvent.click(row(2).getByRole("button", { name: "Delete" }));
    await vi.waitFor(() => expect(rows()).toHaveLength(2));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(mocked.deleteCriterion).not.toHaveBeenCalled();
  });

  it("moves a row with the arrow keys and saves the order", async () => {
    await show();
    const handle = row(1).getByRole("button", { name: "Reorder" });
    handle.focus();
    await fireEvent.keyDown(handle, { key: "ArrowUp" });
    await vi.waitFor(() =>
      expect(mocked.reorderCriteria).toHaveBeenCalledExactlyOnceWith(7, "exclusion", ["r2", "r1"]),
    );
    expect(field(0, "Code").value).toBe("E2");
    expect(document.activeElement).toBe(row(0).getByRole("button", { name: "Reorder" }));
    // Nothing happens past the end of the list.
    await fireEvent.keyDown(document.activeElement!, { key: "ArrowUp" });
    expect(mocked.reorderCriteria).toHaveBeenCalledOnce();
  });

  it("saves the order again when a row is saved above a later one", async () => {
    mocked.listCriteria.mockResolvedValue([]);
    mocked.addCriterion.mockResolvedValueOnce("second").mockResolvedValueOnce("first");
    await show();
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await type(field(1, "Description"), "B");
    await type(field(0, "Description"), "A");
    await vi.waitFor(() =>
      expect(mocked.reorderCriteria).toHaveBeenCalledExactlyOnceWith(7, "exclusion", ["first", "second"]),
    );
  });

  it("reads a list that cannot be read only once", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.listCriteria.mockRejectedValue(new Error("busy"));
    render(CriteriaList, { store: createSettingsStore(() => 7), kind: "exclusion", readOnly: false });
    expect(await screen.findByRole("alert")).toBeTruthy();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(mocked.listCriteria).toHaveBeenCalledOnce();
  });

  it("only shows the list of a read-only library", async () => {
    await show("exclusion", true);
    expect(field(0, "Description").readOnly).toBe(true);
    expect(screen.queryByRole("button")).toBeNull();
  });
});

describe("the research questions (FR-08.4)", () => {
  const question = (index: number) => field(index, "Question");
  const savedTexts = () => mocked.setResearchQuestions.mock.calls.map(([, text]) => text);

  beforeEach(() => mocked.getResearchQuestions.mockResolvedValue("RQ one?\n\nRQ two?\n"));

  it("shows one numbered row per line, and writes nothing just by showing them", async () => {
    await show("questions");
    expect(rows().map((item) => item.querySelector(".number")?.textContent)).toEqual(["RQ1", "RQ2"]);
    expect([question(0).value, question(1).value]).toEqual(["RQ one?", "RQ two?"]);
    expect(row(0).queryByRole("textbox", { name: "Code" })).toBeNull();
    expect(mocked.setResearchQuestions).not.toHaveBeenCalled();
  });

  it("shows an empty list with Add", async () => {
    mocked.getResearchQuestions.mockResolvedValue("");
    await show("questions");
    expect(screen.getByText("No research questions yet.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Add" })).toBeTruthy();
  });

  it("saves all questions as one text, also a new one", async () => {
    await show("questions");
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(document.activeElement).toBe(question(2));
    await type(question(2), " RQ three? ");
    await vi.waitFor(() => expect(savedTexts()).toEqual(["RQ one?\nRQ two?\nRQ three?"]));
    expect(messages()).toEqual(["Saved"]);
  });

  it("keeps the old question when its field is emptied", async () => {
    await show("questions");
    await type(question(0), "");
    expect(row(0).getByText("This field is required.")).toBeTruthy();
    await type(question(1), "Changed?");
    await vi.waitFor(() => expect(savedTexts()).toEqual(["RQ one?\nChanged?"]));
  });

  it("saves a new order", async () => {
    await show("questions");
    await fireEvent.keyDown(row(1).getByRole("button", { name: "Reorder" }), { key: "ArrowUp" });
    await vi.waitFor(() => expect(savedTexts()).toEqual(["RQ two?\nRQ one?"]));
  });

  it("deletes a question only after the confirmation", async () => {
    await show("questions");
    await fireEvent.click(row(1).getByRole("button", { name: "Delete" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete research question?" });
    expect(dialog.textContent).toContain("RQ2 will be deleted. This can't be undone.");
    expect(mocked.setResearchQuestions).not.toHaveBeenCalled();
    await fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await vi.waitFor(() => expect(rows()).toHaveLength(1));
    expect(savedTexts()).toEqual(["RQ one?"]);
  });

  it("keeps the question when deleting fails", async () => {
    mocked.setResearchQuestions.mockRejectedValue(new Error("disk full"));
    await show("questions");
    await fireEvent.click(row(0).getByRole("button", { name: "Delete" }));
    await fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Delete" }));
    await vi.waitFor(() => expect(messages()).toEqual(["Something went wrong. Try again."]));
    expect(rows()).toHaveLength(2);
  });
});

describe("the Settings screen", () => {
  it("starts on the research questions and switches sections", async () => {
    const store = createSettingsStore(() => 7);
    const onclose = vi.fn();
    render(SettingsScreen, { store, section: "questions", readOnly: false, onclose });
    expect(((await screen.findByRole("textbox", { name: "Question" })) as HTMLInputElement).value).toBe("RQ1?");

    await fireEvent.click(screen.getByRole("button", { name: "Extraction tags" }));
    expect(await screen.findByText("Order sets export columns")).toBeTruthy();
    expect(mocked.listCriteria).toHaveBeenCalledWith(7, "tag");

    await fireEvent.click(screen.getByRole("button", { name: "Back to reader" }));
    expect(onclose).toHaveBeenCalledOnce();
  });
});
