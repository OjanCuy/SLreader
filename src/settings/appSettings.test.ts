// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { dismissToast, toasts } from "../lib/components/toasts.svelte";
import { createAppSettingsStore } from "./appSettingsStore.svelte";
import SettingsScreen from "./SettingsScreen.svelte";
import { createSettingsStore } from "./settingsStore.svelte";

vi.mock("../lib/api");
const mocked = vi.mocked(api);

beforeEach(() => {
  vi.resetAllMocks();
  mocked.getCsvDelimiter.mockResolvedValue(";");
  mocked.getTheme.mockResolvedValue("system");
  mocked.setCsvDelimiter.mockResolvedValue();
  mocked.setTheme.mockResolvedValue();
  mocked.listCriteria.mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  document.documentElement.removeAttribute("data-theme");
  [...toasts].forEach((toast) => dismissToast(toast.id));
});

const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;
const theme = () => document.documentElement.getAttribute("data-theme");

function open(section: "export" | "appearance", readOnly = false) {
  const app = createAppSettingsStore();
  render(SettingsScreen, {
    store: createSettingsStore(() => 7),
    app,
    section,
    readOnly,
    notice: "This library is read-only.",
    onclose: vi.fn(),
  });
  return app;
}

describe("the app settings (FR-08.3, FR-08.5)", () => {
  it("shows the stored CSV separator and saves another one", async () => {
    mocked.getCsvDelimiter.mockResolvedValue(",");
    open("export");
    await screen.findByRole("group", { name: "CSV separator" });
    expect(radio("Comma").checked).toBe(true);
    await fireEvent.click(radio("Semicolon"));
    await vi.waitFor(() => expect(toasts.map((toast) => toast.message)).toEqual(["Saved"]));
    expect(mocked.setCsvDelimiter).toHaveBeenCalledExactlyOnceWith(";");
    expect(radio("Semicolon").checked).toBe(true);
  });

  it("applies the theme at once and saves it", async () => {
    open("appearance");
    await screen.findByRole("group", { name: "Theme" });
    expect(radio("System").checked).toBe(true);
    await fireEvent.click(radio("Dark"));
    expect(theme()).toBe("dark");
    await vi.waitFor(() => expect(mocked.setTheme).toHaveBeenCalledExactlyOnceWith("dark"));
    await fireEvent.click(radio("System"));
    expect(theme()).toBeNull();
    await vi.waitFor(() => expect(mocked.setTheme).toHaveBeenLastCalledWith("system"));
  });

  it("can be changed in a read-only library, without its warning", async () => {
    open("export", true);
    await screen.findByRole("group", { name: "CSV separator" });
    expect(screen.queryByText("This library is read-only.")).toBeNull();
    expect(radio("Comma").disabled).toBe(false);
  });

  it("keeps an unsaved choice on screen and saves it with Try again", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.setCsvDelimiter.mockRejectedValueOnce(new Error("disk full"));
    const app = open("export");
    await screen.findByRole("group", { name: "CSV separator" });
    await fireEvent.click(radio("Comma"));
    await screen.findByText("Couldn't save: the disk is full or the folder is read-only.");
    expect(radio("Comma").checked).toBe(true);
    expect(app.delimiterFailed).toBe(true);
    await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await vi.waitFor(() => expect(app.delimiterFailed).toBe(false));
    expect(mocked.setCsvDelimiter).toHaveBeenLastCalledWith(",");
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  });

  it("saves one choice after another, so the last one wins", async () => {
    let finish = () => {};
    mocked.setTheme.mockReturnValueOnce(new Promise<void>((resolve) => (finish = resolve)));
    const app = open("appearance");
    await screen.findByRole("group", { name: "Theme" });
    await fireEvent.click(radio("Dark"));
    await fireEvent.click(radio("Light"));
    await vi.waitFor(() => expect(mocked.setTheme).toHaveBeenCalledTimes(1));
    finish();
    await app.flush();
    expect(mocked.setTheme).toHaveBeenCalledTimes(2);
    expect(mocked.setTheme).toHaveBeenLastCalledWith("light");
  });

  it("says when the settings cannot be read, and offers no choice", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocked.getTheme.mockRejectedValue(new Error("broken"));
    open("appearance");
    await screen.findByText("Something went wrong. Try again.");
    expect(screen.queryByRole("radio")).toBeNull();
  });

  it("lists the sections of both groups", async () => {
    open("export");
    for (const name of ["Research questions", "Extraction tags", "Export", "Appearance"]) {
      expect(screen.getByRole("button", { name })).toBeTruthy();
    }
    await fireEvent.click(screen.getByRole("button", { name: "Appearance" }));
    expect(await screen.findByRole("group", { name: "Theme" })).toBeTruthy();
  });
});
