// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
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
  mocked.listLibraries.mockResolvedValue([]);
  mocked.listPapers.mockResolvedValue([paper("1", "a.pdf"), paper("2", "b.pdf")]);
  mocked.readPaperPdf.mockReturnValue(new Promise(() => {}));
  mocked.onFileDrag.mockResolvedValue(() => {});
  mocked.onPaperTextChanged.mockResolvedValue(() => {});
  mocked.listHighlights.mockResolvedValue([]);
  mocked.listHighlightCategories.mockResolvedValue([]);
  mocked.listExtractionTags.mockResolvedValue([]);
  mocked.listExclusionReasons.mockResolvedValue([]);
  mocked.getScreening.mockImplementation(async (_session, id) => ({
    decision: id === "2" ? "include" : "undecided",
    exclusionReasonId: null,
    assessmentNote: "",
  }));
  mocked.setDecision.mockResolvedValue();
});

afterEach(cleanup);

function show() {
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
}

const tab = (name: string) => screen.getByRole("tab", { name });
const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;
const open = async (file: string) => fireEvent.click(await screen.findByRole("button", { name: file }));

describe("the paper's tabs (Brief Section 6, S-02)", () => {
  it("has no tabs until a paper is selected", async () => {
    show();
    await screen.findByRole("button", { name: "a.pdf" });
    expect(screen.queryByRole("tab")).toBeNull();
  });

  it("starts on Decision and switches with clicks and arrow keys", async () => {
    show();
    await open("a.pdf");
    expect(tab("Decision").getAttribute("aria-selected")).toBe("true");
    await vi.waitFor(() => expect(radio("Undecided").checked).toBe(true));

    await fireEvent.keyDown(tab("Decision"), { key: "ArrowRight" });
    expect(tab("Highlights").getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(tab("Highlights"));
    expect(screen.queryByRole("radio")).toBeNull();
    await fireEvent.click(tab("Decision"));
    expect(screen.getByRole("tabpanel", { name: "Decision" })).toBeTruthy();
  });

  it("shows each paper's own decision and saves a change to the open paper only", async () => {
    show();
    await open("a.pdf");
    await vi.waitFor(() => expect(radio("Undecided").checked).toBe(true));
    await open("b.pdf");
    await vi.waitFor(() => expect(radio("Include").checked).toBe(true));
    // Moving between papers and tabs never writes a decision (FR-11.4).
    await fireEvent.click(tab("Highlights"));
    await fireEvent.click(tab("Decision"));
    expect(mocked.setDecision).not.toHaveBeenCalled();

    await fireEvent.click(radio("Exclude"));
    await vi.waitFor(() => expect(mocked.setDecision).toHaveBeenCalledExactlyOnceWith(7, "2", "exclude"));
  });
});
