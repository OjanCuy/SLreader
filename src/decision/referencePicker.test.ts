// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
import DecisionPanel from "./DecisionPanel.svelte";
import { createDecisionStore } from "./decisionStore.svelte";
import { matchesQuery, rankReferences } from "./similarity";

vi.mock("../lib/api");
const mocked = vi.mocked(api);

const reference = (id: string, no: number, title: string, other: Partial<api.ReferenceChoice> = {}) => ({
  id,
  no,
  title,
  year: null,
  linkedPaperId: null,
  linkedPaperName: null,
  ...other,
});

const FILE = "Kim 2021 recirculating aquaculture tilapia.pdf";
const REFERENCES: api.ReferenceChoice[] = [
  reference("a", 12, "Feed cost in shrimp farming"),
  reference("b", 46, "Recirculating aquaculture system for tilapia"),
  reference("c", 7, "Aquaculture water quality", { linkedPaperId: "p2", linkedPaperName: "lee.pdf" }),
];
const EMPTY: api.Metadata = { refNo: null, title: "", year: null, sourceDatabase: "", doi: "" };
const FILLED: api.Metadata = { ...EMPTY, refNo: 46, title: "Recirculating aquaculture system for tilapia" };

beforeEach(() => {
  vi.resetAllMocks();
  // jsdom has no modal dialogs.
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
  HTMLDialogElement.prototype.close = function () {
    this.open = false;
  };
  mocked.getScreening.mockResolvedValue({ decision: "undecided", exclusionReasonId: null, assessmentNote: "" });
  mocked.listExclusionReasons.mockResolvedValue([]);
  mocked.getMetadata.mockResolvedValue({ metadata: EMPTY, sameRefNo: [] });
  mocked.listReferences.mockResolvedValue(REFERENCES);
  mocked.linkReference.mockResolvedValue({ status: "linked", metadata: FILLED, sameRefNo: [] });
  mocked.unlinkReference.mockResolvedValue();
  mocked.setMetadata.mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

async function show(readOnly = false) {
  const store = createDecisionStore();
  render(DecisionPanel, { store, name: FILE, readOnly, ondelete: vi.fn(), onsettings: vi.fn() });
  await store.open(7, "p1");
  await new Promise((resolve) => setTimeout(resolve, 0));
  return store;
}

const picker = () => screen.getByRole("combobox", { name: "Link to Reference No" }) as HTMLInputElement;
const options = () => screen.queryAllByRole("option");
const names = () => options().map((option) => option.querySelector(".reference")?.textContent);
const title = () => screen.getByLabelText("Title") as HTMLTextAreaElement;
const choose = async (index: number) => {
  await fireEvent.click(picker());
  await fireEvent.click(options()[index]!);
};

describe("ordering by similarity (FR-05.3)", () => {
  it("puts the title that looks like the file name first", () => {
    const ranked = rankReferences(REFERENCES, FILE, "p1");
    expect(ranked.map((each) => each.reference.no)).toEqual([46, 7, 12]);
    expect(ranked.map((each) => each.best)).toEqual([true, false, false]);
  });

  it("gives Best match only to a reference this paper can take", () => {
    const takenBest = REFERENCES.map((each) =>
      each.id === "b" ? { ...each, linkedPaperId: "p9", linkedPaperName: "x.pdf" } : each,
    );
    expect(rankReferences(takenBest, FILE, "p1").some((each) => each.best)).toBe(false);
  });

  it("names no best match for a file name that says nothing", () => {
    const ranked = rankReferences(REFERENCES, "scan0001.pdf", "p1");
    expect(ranked.map((each) => each.reference.no)).toEqual([12, 46, 7]);
    expect(ranked.some((each) => each.best)).toBe(false);
  });

  it("searches the number and every word, without regard to case or accents", () => {
    const cafe = reference("d", 8, "Café ponds");
    expect(matchesQuery(cafe, "cafe PON")).toBe(true);
    expect(matchesQuery(cafe, "8")).toBe(true);
    expect(matchesQuery(cafe, "cafe shrimp")).toBe(false);
  });
});

describe("the ReferencePicker (FR-05.3, FR-05.4)", () => {
  it("says to import references when there are none", async () => {
    mocked.listReferences.mockResolvedValue([]);
    await show();
    expect(screen.getByText("Import references to link this paper.")).toBeTruthy();
    expect(screen.queryByRole("combobox", { name: "Link to Reference No" })).toBeNull();
  });

  it("lists the most similar first and marks it and the taken one", async () => {
    await show();
    expect(options()).toHaveLength(0);
    await fireEvent.click(picker());
    expect(names()[0]).toBe("46 - Recirculating aquaculture system for tilapia");
    expect(within(options()[0]!).getByText("Best match")).toBeTruthy();
    expect(options()[1]!.getAttribute("aria-disabled")).toBe("true");
    expect(within(options()[1]!).getByText("Linked to another paper: lee.pdf")).toBeTruthy();
    await fireEvent.click(options()[1]!);
    expect(mocked.linkReference).not.toHaveBeenCalled();
  });

  it("filters while typing and links the first result with Enter", async () => {
    await show();
    await fireEvent.input(picker(), { target: { value: "shrimp" } });
    await vi.waitFor(() => expect(picker().getAttribute("aria-activedescendant")).toBeTruthy());
    expect(names()).toEqual(["12 - Feed cost in shrimp farming"]);
    await fireEvent.keyDown(picker(), { key: "Enter" });
    await vi.waitFor(() => expect(mocked.linkReference).toHaveBeenCalledWith(7, "p1", "a", false));
  });

  it("skips the taken reference with the arrow keys and closes with Esc", async () => {
    await show();
    await fireEvent.keyDown(picker(), { key: "ArrowDown" });
    await fireEvent.keyDown(picker(), { key: "ArrowDown" });
    expect(picker().getAttribute("aria-activedescendant")).toBe(options()[2]!.id);
    await fireEvent.keyDown(picker(), { key: "Escape" });
    expect(options()).toHaveLength(0);
    expect(mocked.linkReference).not.toHaveBeenCalled();
  });

  it("says when nothing matches", async () => {
    await show();
    await fireEvent.input(picker(), { target: { value: "zebra" } });
    expect(screen.getByText("No matching references.")).toBeTruthy();
  });

  it("fills the metadata from the chosen reference, and unlinks", async () => {
    await show();
    mocked.listReferences.mockResolvedValue(
      REFERENCES.map((each) => (each.id === "b" ? { ...each, linkedPaperId: "p1" } : each)),
    );
    await choose(0);
    await vi.waitFor(() => expect(title().value).toBe(FILLED.title));
    await vi.waitFor(() => expect(picker().value).toBe(`46 - ${FILLED.title}`));
    // Never a decision (FR-11.4).
    expect(mocked.setDecision).not.toHaveBeenCalled();

    mocked.listReferences.mockResolvedValue(REFERENCES);
    await fireEvent.click(screen.getByRole("button", { name: "Unlink" }));
    await vi.waitFor(() => expect(picker().value).toBe(""));
    expect(mocked.unlinkReference).toHaveBeenCalledExactlyOnceWith(7, "p1");
    expect(title().value).toBe(FILLED.title);
  });

  it("asks before replacing metadata and changes nothing on Cancel", async () => {
    mocked.linkReference.mockResolvedValue({ status: "needs_confirmation" });
    await show();
    await choose(0);
    const dialog = await screen.findByRole("dialog", { name: "Replace metadata?" });
    expect(dialog.textContent).toContain(
      "The metadata of this paper will be replaced with the data of Reference No 46.",
    );
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });
    expect(document.activeElement).toBe(cancel);
    await fireEvent.click(cancel);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(mocked.linkReference).toHaveBeenCalledOnce();
  });

  it("replaces after the confirmation", async () => {
    mocked.linkReference.mockResolvedValueOnce({ status: "needs_confirmation" });
    await show();
    await choose(0);
    await fireEvent.click(await screen.findByRole("button", { name: "Replace" }));
    await vi.waitFor(() => expect(title().value).toBe(FILLED.title));
    expect(mocked.linkReference).toHaveBeenLastCalledWith(7, "p1", "b", true);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("names the paper when the reference was taken meanwhile", async () => {
    mocked.linkReference.mockResolvedValue({ status: "taken", paper: "lee.pdf" });
    await show();
    await choose(0);
    await vi.waitFor(() =>
      expect(toasts.map((toast) => toast.message)).toEqual([
        "Reference No 46 is already linked to lee.pdf.",
      ]),
    );
  });

  it("saves typed metadata before linking, and does not link when that fails", async () => {
    const store = await show();
    mocked.setMetadata.mockRejectedValue(new Error("disk full"));
    await fireEvent.input(title(), { target: { value: "Mine" } });
    await store.link("b");
    expect(mocked.setMetadata).toHaveBeenCalled();
    expect(mocked.linkReference).not.toHaveBeenCalled();
    expect(title().value).toBe("Mine");
  });

  it("reads the list again after the number is changed", async () => {
    const store = await show();
    mocked.listReferences.mockClear();
    await fireEvent.input(screen.getByLabelText("Reference No"), { target: { value: "5" } });
    await store.link("b");
    expect(mocked.listReferences).toHaveBeenCalled();
  });

  it("only shows the link in a read-only library", async () => {
    mocked.listReferences.mockResolvedValue([reference("b", 46, "T", { linkedPaperId: "p1" })]);
    await show(true);
    expect(picker().readOnly).toBe(true);
    await fireEvent.click(picker());
    expect(options()).toHaveLength(0);
    expect(screen.queryByRole("button", { name: "Unlink" })).toBeNull();
  });
});
