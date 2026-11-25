import { describe, expect, it, vi } from "vitest";
import { applyTheme, THEME_OPTIONS } from "./theme";

function fakeRoot() {
  return { setAttribute: vi.fn(), removeAttribute: vi.fn() };
}

describe("applyTheme", () => {
  it.each(["light", "dark"] as const)("forces the %s theme", (choice) => {
    const root = fakeRoot();
    applyTheme(choice, root);
    expect(root.setAttribute).toHaveBeenCalledWith("data-theme", choice);
  });

  it("removes the override so the system theme applies again", () => {
    const root = fakeRoot();
    applyTheme("system", root);
    expect(root.removeAttribute).toHaveBeenCalledWith("data-theme");
    expect(root.setAttribute).not.toHaveBeenCalled();
  });

  it("offers the options from the design brief, in order", () => {
    expect(THEME_OPTIONS.map((o) => o.label)).toEqual(["System", "Light", "Dark"]);
  });
});
