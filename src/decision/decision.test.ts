// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
import { GENERIC_ERROR, SAVE_ERROR } from "../lib/libraryTexts";
import DecisionPanel from "./DecisionPanel.svelte";
import { createDecisionStore, NOTE_SAVE_DELAY_MS } from "./decisionStore.svelte";

vi.mock("../lib/api");
const mocked = vi.mocked(api);

const UNDECIDED: api.Screening = { decision: "undecided", exclusionReasonId: null, assessmentNote: "" };
const REASONS: api.ExclusionReason[] = [{ id: "r3", code: "E3", label: "Wrong population" }];
const EMPTY: api.Metadata = { refNo: null, title: "", year: null, sourceDatabase: "", doi: "" };

beforeEach(() => {
  vi.resetAllMocks();
  mocked.getScreening.mockResolvedValue(UNDECIDED);
  mocked.listExclusionReasons.mockResolvedValue(REASONS);
  mocked.setDecision.mockResolvedValue();
  mocked.setExclusionReason.mockResolvedValue();
  mocked.setAssessmentNote.mockResolvedValue();
  mocked.getMetadata.mockResolvedValue({ metadata: EMPTY, sameRefNo: [] });
  mocked.setMetadata.mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

async function show(readOnly = false, listeners: Parameters<typeof createDecisionStore>[0] = {}) {
  const store = createDecisionStore(listeners);
  const ondelete = vi.fn();
  render(DecisionPanel, { store, name: "kim-2021.pdf", readOnly, ondelete });
  await store.open(7, "p1");
  await tick();
  return { store, ondelete };
}

const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;
const note = () => screen.getByLabelText("Assessment notes") as HTMLTextAreaElement;
const reason = () => screen.queryByLabelText("Exclusion reason") as HTMLSelectElement | null;
const type = (value: string) => fireEvent.input(note(), { target: { value } });

describe("the Decision tab (FR-04.1 to FR-04.4)", () => {
  it("shows the file name and starts with the stored screening", async () => {
    await show();
    expect(screen.getByText("kim-2021.pdf")).toBeTruthy();
    expect(screen.getByRole("group", { name: "Decision" })).toBeTruthy();
    expect(radio("Undecided").checked).toBe(true);
    expect(reason()).toBeNull();
    expect(note().value).toBe("");
  });

  it("shows each choice with an icon and text, not color alone", async () => {
    await show();
    for (const name of ["Include", "Exclude", "Undecided"]) {
      expect(radio(name).parentElement?.querySelector("[data-icon]")).toBeTruthy();
    }
  });

  it("changes nothing just by showing a paper (FR-11.4)", async () => {
    const { store } = await show();
    await store.open(7, "p2");
    store.flush();
    await tick();
    expect(mocked.setDecision).not.toHaveBeenCalled();
    expect(mocked.setExclusionReason).not.toHaveBeenCalled();
    expect(mocked.setAssessmentNote).not.toHaveBeenCalled();
  });

  it("saves the decision at once, and only the decision", async () => {
    await show();
    await fireEvent.click(radio("Include"));
    await tick();
    expect(mocked.setDecision).toHaveBeenCalledExactlyOnceWith(7, "p1", "include");
    expect(mocked.setExclusionReason).not.toHaveBeenCalled();
    expect(mocked.setAssessmentNote).not.toHaveBeenCalled();
  });

  it("offers the reason only while excluded, and allows No reason", async () => {
    await show();
    await fireEvent.click(radio("Exclude"));
    expect(Array.from(reason()!.options, (option) => option.textContent)).toEqual([
      "No reason",
      "E3 - Wrong population",
    ]);
    await fireEvent.change(reason()!, { target: { value: "r3" } });
    await tick();
    expect(mocked.setExclusionReason).toHaveBeenLastCalledWith(7, "p1", "r3");
    await fireEvent.change(reason()!, { target: { value: "" } });
    await tick();
    expect(mocked.setExclusionReason).toHaveBeenLastCalledWith(7, "p1", null);

    await fireEvent.change(reason()!, { target: { value: "r3" } });
    await fireEvent.click(radio("Include"));
    expect(reason()).toBeNull();
    // The reason is kept for when the paper is excluded again.
    await fireEvent.click(radio("Exclude"));
    expect(reason()!.value).toBe("r3");
  });

  it("points to Settings when the library has no reasons yet", async () => {
    mocked.listExclusionReasons.mockResolvedValue([]);
    mocked.getScreening.mockResolvedValue({ ...UNDECIDED, decision: "exclude" });
    await show();
    expect(screen.getByText("Add in Settings")).toBeTruthy();
  });

  it("saves the note automatically, without touching the decision", async () => {
    await show();
    vi.useFakeTimers();
    await type("Pond");
    await vi.advanceTimersByTimeAsync(NOTE_SAVE_DELAY_MS - 1);
    await type("Pond trial only");
    await vi.advanceTimersByTimeAsync(NOTE_SAVE_DELAY_MS - 1);
    expect(mocked.setAssessmentNote).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(mocked.setAssessmentNote).toHaveBeenCalledExactlyOnceWith(7, "p1", "Pond trial only");
    expect(screen.getByText("Saved")).toBeTruthy();
    expect(mocked.setDecision).not.toHaveBeenCalled();
  });

  it("saves the note at once when the field is left", async () => {
    await show();
    await type("short");
    await fireEvent.blur(note());
    await tick();
    expect(mocked.setAssessmentNote).toHaveBeenCalledExactlyOnceWith(7, "p1", "short");
  });

  it("saves a waiting note for the right paper when another is opened", async () => {
    const { store } = await show();
    await type("typed just now");
    await store.open(7, "p2");
    expect(mocked.setAssessmentNote).toHaveBeenCalledExactlyOnceWith(7, "p1", "typed just now");
  });

  it("keeps the change on screen with Try again when saving fails", async () => {
    const { store } = await show();
    mocked.setDecision.mockRejectedValue({ code: "other", detail: "disk full" });
    await fireEvent.click(radio("Exclude"));
    await tick();
    expect(screen.getByRole("alert").textContent).toContain(SAVE_ERROR);
    expect(radio("Exclude").checked).toBe(true);

    // Still there on the next paper, and on the way back.
    await store.open(7, "p2");
    await tick();
    expect(screen.getByRole("alert").textContent).toContain(SAVE_ERROR);
    await store.open(7, "p1");
    await tick();
    expect(radio("Exclude").checked).toBe(true);

    mocked.setDecision.mockResolvedValue();
    await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await tick();
    expect(mocked.setDecision).toHaveBeenLastCalledWith(7, "p1", "exclude");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("never lets an older decision overtake a newer one", async () => {
    const { store } = await show();
    let finish = () => {};
    mocked.setDecision.mockImplementationOnce(() => new Promise<void>((resolve) => (finish = resolve)));
    const first = store.decide("include");
    const second = store.decide("exclude");
    await tick();
    expect(mocked.setDecision).toHaveBeenCalledTimes(1);
    finish();
    await Promise.all([first, second]);
    expect(mocked.setDecision.mock.calls.at(-1)).toEqual([7, "p1", "exclude"]);
    expect(store.screening?.decision).toBe("exclude");
  });

  it("drops what was waiting for a deleted paper", async () => {
    const { store } = await show();
    mocked.setAssessmentNote.mockRejectedValue(new Error("busy"));
    await type("gone soon");
    await fireEvent.blur(note());
    await tick();
    expect(store.saveFailed).toBe(true);
    store.forget("p1");
    expect(store.saveFailed).toBe(false);
    mocked.setAssessmentNote.mockClear();
    await store.retry();
    expect(mocked.setAssessmentNote).not.toHaveBeenCalled();
  });

  it("says so when a save fails after the library was left", async () => {
    const { store } = await show();
    mocked.setAssessmentNote.mockRejectedValue({ code: "not_active", detail: "" });
    await type("late");
    store.dispose();
    await tick();
    expect(toasts.map((toast) => toast.message)).toContain(SAVE_ERROR);
  });

  it("changes nothing in a read-only library", async () => {
    mocked.getScreening.mockResolvedValue({ decision: "exclude", exclusionReasonId: "r3", assessmentNote: "n" });
    await show(true);
    expect((screen.getByRole("group", { name: "Decision" }) as HTMLFieldSetElement).disabled).toBe(true);
    expect(reason()!.disabled).toBe(true);
    expect(note().readOnly).toBe(true);
    expect(screen.queryByRole("button", { name: "Delete paper" })).toBeNull();
  });

  it("offers no decision when the stored one cannot be read, but can still delete", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.getScreening.mockRejectedValue(new Error("busy"));
    const { ondelete } = await show();
    expect(screen.getByRole("alert").textContent).toContain(GENERIC_ERROR);
    expect(screen.queryByRole("radio")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Delete paper" }));
    expect(ondelete).toHaveBeenCalled();
  });

  it("drops the answer for a paper that is no longer open", async () => {
    const store = createDecisionStore();
    let late = (_: api.Screening) => {};
    mocked.getScreening.mockImplementationOnce(() => new Promise((resolve) => (late = resolve)));
    const first = store.open(7, "p1");
    await store.open(7, "p2");
    late({ ...UNDECIDED, decision: "include" });
    await first;
    expect(store.screening?.decision).toBe("undecided");
  });
});

describe("metadata (FR-05.1, FR-05.4)", () => {
  const field = (label: string) => screen.getByLabelText(label) as HTMLInputElement;
  const enter = (label: string, value: string) => fireEvent.input(field(label), { target: { value } });
  const leave = async (label: string) => {
    await fireEvent.blur(field(label));
    await tick();
  };

  it("shows the stored values", async () => {
    mocked.getMetadata.mockResolvedValue({
      metadata: { refNo: 46, title: "Tilapia", year: 2021, sourceDatabase: "Scopus", doi: "10.1/x" },
      sameRefNo: [],
    });
    await show();
    expect(["Reference No", "Year", "Title", "Source database", "DOI"].map((l) => field(l).value)).toEqual([
      "46",
      "2021",
      "Tilapia",
      "Scopus",
      "10.1/x",
    ]);
  });

  it("saves all fields together, without touching the decision", async () => {
    const onmetadatasaved = vi.fn();
    await show(false, { onmetadatasaved });
    await enter("Title", "Tilapia");
    await enter("Source database", "Scopus");
    await leave("Source database");
    expect(mocked.setMetadata).toHaveBeenCalledExactlyOnceWith(7, "p1", {
      ...EMPTY,
      title: "Tilapia",
      sourceDatabase: "Scopus",
    });
    expect(screen.getByText("Saved")).toBeTruthy();
    expect(onmetadatasaved).toHaveBeenCalledTimes(1);
    expect(mocked.setDecision).not.toHaveBeenCalled();
    expect(mocked.setAssessmentNote).not.toHaveBeenCalled();
  });

  it("takes only digits as reference number and year", async () => {
    await show();
    await enter("Reference No", "#4a6");
    expect(field("Reference No").value).toBe("46");
    await enter("Year", "20215");
    expect(field("Year").value).toBe("2021");
    await leave("Year");
    expect(mocked.setMetadata).toHaveBeenLastCalledWith(7, "p1", expect.objectContaining({ refNo: 46, year: 2021 }));
    await enter("Reference No", "");
    await leave("Reference No");
    expect(mocked.setMetadata).toHaveBeenLastCalledWith(7, "p1", expect.objectContaining({ refNo: null }));
  });

  it("keeps a title on one line", async () => {
    await show();
    await enter("Title", "Tilapia\r\nin ponds");
    expect(field("Title").value).toBe("Tilapia in ponds");
  });

  it("warns about a reference number used twice, and still saves it", async () => {
    mocked.setMetadata.mockResolvedValue(["kim-2021.pdf"]);
    await show();
    await enter("Reference No", "46");
    expect(screen.queryByText(/also used by/)).toBeNull();
    await leave("Reference No");
    expect(screen.getByText("Reference No 46 is also used by kim-2021.pdf.")).toBeTruthy();
    expect(mocked.setMetadata).toHaveBeenCalledOnce();
    // The warning goes while another number is typed.
    await enter("Reference No", "47");
    expect(screen.queryByText(/also used by/)).toBeNull();
  });

  it("names how many more papers use the number", async () => {
    mocked.getMetadata.mockResolvedValue({
      metadata: { ...EMPTY, refNo: 46 },
      sameRefNo: ["kim-2021.pdf", "b.pdf", "c.pdf"],
    });
    await show();
    expect(screen.getByText("Reference No 46 is also used by kim-2021.pdf and 2 more.")).toBeTruthy();
  });

  it("saves what was typed for the right paper, and keeps it when saving fails", async () => {
    const { store } = await show();
    mocked.setMetadata.mockRejectedValue(new Error("disk full"));
    await enter("DOI", "10.1/kept");
    await store.open(7, "p2");
    await tick();
    expect(mocked.setMetadata).toHaveBeenLastCalledWith(7, "p1", expect.objectContaining({ doi: "10.1/kept" }));
    expect(field("DOI").value).toBe("");
    expect(screen.getByRole("alert").textContent).toContain(SAVE_ERROR);
    await store.open(7, "p1");
    await tick();
    expect(field("DOI").value).toBe("10.1/kept");
  });

  it("changes nothing in a read-only library", async () => {
    await show(true);
    for (const label of ["Reference No", "Year", "Title", "Source database", "DOI"]) {
      expect(field(label).readOnly).toBe(true);
    }
  });
});
