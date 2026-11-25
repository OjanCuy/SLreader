import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { getAppInfo } from "./api";

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
