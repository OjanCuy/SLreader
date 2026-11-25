// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import { tick } from "svelte";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App.svelte";
import * as api from "../lib/api";
import type { LibrarySummary } from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";

vi.mock("../lib/api");
const mocked = vi.mocked(api);

const library = (id: number, name: string, over: Partial<LibrarySummary> = {}): LibrarySummary => ({
  id,
  folderPath: `C:\\reviews\\${name.toLowerCase()}`,
  name,
  paperCount: id,
  lastOpenedAt: null,
  problem: null,
  cloudSynced: false,
  session: null,
  ...over,
});

const alpha = library(1, "Alpha");
const beta = library(2, "Beta");
const gone = library(3, "Gone", { name: null, problem: "not_found" });

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
  mocked.listLibraries.mockResolvedValue([alpha, beta, gone]);
  mocked.openLibrary.mockImplementation(async (id) => ({
    ...(id === 1 ? alpha : beta),
    session: id,
  }));
  // Each library has its own paper; the session tells them apart.
  mocked.listPapers.mockImplementation(async (session) => [
    { id: `p${session}`, fileName: `paper-${session}.pdf`, title: "", fileMissing: false },
  ]);
  mocked.onFileDrag.mockResolvedValue(() => {});
});

afterEach(() => {
  cleanup();
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

const heading = () => screen.getByRole("heading", { level: 1 }).textContent;

/** Starts the app and opens Alpha from the Libraries screen. */
async function openAlpha() {
  render(App);
  await fireEvent.click(await screen.findByRole("button", { name: /^Alpha/ }));
  await vi.waitFor(() => expect(heading()).toBe("Alpha"));
}

async function openSwitcher(current: string) {
  await fireEvent.click(screen.getByRole("button", { name: current }));
  await tick();
  return within(screen.getByRole("menu"));
}

describe("LibrarySwitcher", () => {
  it("lists the libraries, marks the open one, and disables a missing one with its reason", async () => {
    await openAlpha();
    const menu = await openSwitcher("Alpha");

    await vi.waitFor(() => expect(menu.getAllByRole("menuitemradio")).toHaveLength(3));
    const [a, b, missing] = menu.getAllByRole("menuitemradio") as HTMLButtonElement[];
    expect(a.getAttribute("aria-checked")).toBe("true");
    expect(a.querySelector('[data-icon="check"]')).not.toBeNull();
    expect(b.getAttribute("aria-checked")).toBe("false");
    expect(missing.disabled).toBe(true);
    expect(missing.textContent).toContain("Folder not found.");
    expect(menu.getByRole("menuitem", { name: "Create library" })).toBeTruthy();
    expect(menu.getByRole("menuitem", { name: "All libraries" })).toBeTruthy();
  });

  it("switches to another library and shows only its details", async () => {
    await openAlpha();
    expect(await screen.findByText("paper-1.pdf")).toBeTruthy();

    const menu = await openSwitcher("Alpha");
    await fireEvent.click(await menu.findByRole("menuitemradio", { name: "Beta" }));

    await vi.waitFor(() => expect(heading()).toBe("Beta"));
    expect(await screen.findByText("paper-2.pdf")).toBeTruthy();
    expect(screen.queryByText("paper-1.pdf")).toBeNull();
    expect(mocked.listPapers).toHaveBeenLastCalledWith(2);
    expect(mocked.openLibrary).toHaveBeenLastCalledWith(2);
  });

  it("does not reopen the library that is already open", async () => {
    await openAlpha();
    const menu = await openSwitcher("Alpha");
    await fireEvent.click(await menu.findByRole("menuitemradio", { name: "Alpha" }));
    expect(mocked.openLibrary).toHaveBeenCalledTimes(1);
  });

  it("stays on the current library when switching fails", async () => {
    await openAlpha();
    mocked.openLibrary.mockRejectedValue({ code: "damaged", detail: "x" });

    const menu = await openSwitcher("Alpha");
    await fireEvent.click(await menu.findByRole("menuitemradio", { name: "Beta" }));

    await vi.waitFor(() =>
      expect(toasts.map((t) => t.message)).toEqual(["This library's database is damaged."]),
    );
    expect(heading()).toBe("Alpha");
  });

  it("goes back to all libraries", async () => {
    await openAlpha();
    const menu = await openSwitcher("Alpha");
    await fireEvent.click(menu.getByRole("menuitem", { name: "All libraries" }));
    expect(await screen.findByRole("button", { name: "Open library" })).toBeTruthy();
  });

  it("creates a library from the switcher and opens it", async () => {
    await openAlpha();
    mocked.pickFolder.mockResolvedValue("C:\\new");
    mocked.createLibrary.mockResolvedValue(library(4, "Delta", { session: 9 }));

    const menu = await openSwitcher("Alpha");
    await fireEvent.click(menu.getByRole("menuitem", { name: "Create library" }));
    await fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Delta" } });
    await fireEvent.click(screen.getByRole("button", { name: "Choose folder" }));
    const submit = within(screen.getByRole("dialog")).getByRole("button", {
      name: "Create library",
    }) as HTMLButtonElement;
    await vi.waitFor(() => expect(submit.disabled).toBe(false));
    await fireEvent.click(submit);

    await vi.waitFor(() => expect(heading()).toBe("Delta"));
  });
});
