import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  createLibrary,
  getAppInfo,
  importPdfs,
  isCloudFolder,
  listLibraries,
  listPapers,
  locateLibrary,
  openLibrary,
  openLibraryFolder,
  pickFolder,
  pickPdfs,
  removeLibrary,
  renameLibrary,
} from "./api";

// Replace the real Tauri bridge: tests run in Node, without the desktop app.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mockedInvoke = vi.mocked(invoke);

describe("getAppInfo", () => {
  // Braces matter: a function returned from beforeEach is run as cleanup after each test.
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  it("calls the app_info command and returns its result", async () => {
    mockedInvoke.mockResolvedValue({ name: "SLReader", version: "0.1.0" });

    await expect(getAppInfo()).resolves.toEqual({ name: "SLReader", version: "0.1.0" });
    expect(mockedInvoke).toHaveBeenCalledWith("app_info");
  });

  it("passes errors from Rust to the caller", async () => {
    mockedInvoke.mockRejectedValue(new Error("command failed"));

    await expect(getAppInfo()).rejects.toThrow("command failed");
  });
});

describe("library commands", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
    mockedInvoke.mockResolvedValue(undefined);
  });

  // Names and arguments must match the #[tauri::command] functions in commands.rs.
  it.each([
    [() => listLibraries(), ["list_libraries"]],
    [() => createLibrary("C:/r", "R"), ["create_library", { folderPath: "C:/r", name: "R" }]],
    [() => openLibraryFolder("C:/r"), ["open_library_folder", { folderPath: "C:/r" }]],
    [() => openLibrary(1), ["open_library", { id: 1 }]],
    [() => renameLibrary(1, "N"), ["rename_library", { id: 1, name: "N" }]],
    [() => removeLibrary(1), ["remove_library", { id: 1 }]],
    [() => locateLibrary(1, "C:/r"), ["locate_library", { id: 1, folderPath: "C:/r" }]],
    [() => pickFolder(), ["pick_folder"]],
    [() => isCloudFolder("C:/r"), ["is_cloud_folder", { folderPath: "C:/r" }]],
    [() => pickPdfs(), ["pick_pdfs"]],
    [
      () => importPdfs(7, ["C:/a.pdf"]),
      ["import_pdfs", { session: 7, paths: ["C:/a.pdf"], allowIdentical: false }],
    ],
    [
      () => importPdfs(7, ["C:/a.pdf"], true),
      ["import_pdfs", { session: 7, paths: ["C:/a.pdf"], allowIdentical: true }],
    ],
    [() => listPapers(7), ["list_papers", { session: 7 }]],
  ] as [() => Promise<unknown>, unknown[]][])("calls %#", async (call, expected) => {
    await call();
    expect(mockedInvoke).toHaveBeenCalledWith(...(expected as [string]));
  });
});
