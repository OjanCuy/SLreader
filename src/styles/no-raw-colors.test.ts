// Color values may only live in tokens.css (CLAUDE.md, design brief Section 3).
import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const src = new URL("../", import.meta.url);
const files = readdirSync(src, { recursive: true, encoding: "utf8" })
  .map((file) => file.replaceAll("\\", "/"))
  .filter((file) => /\.(svelte|css)$/.test(file) && file !== "styles/tokens.css");

const RAW_COLOR = /#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{3,4})\b|\b(?:rgba?|hsla?|oklch)\(/i;

describe("no raw colors outside tokens.css", () => {
  it.each(files)("%s", (file) => {
    // "#12" in a UI text is a reference number, not a color.
    const css = readFileSync(new URL(file, src), "utf8").replaceAll(/title="[^"]*"/g, "");
    expect(css.match(RAW_COLOR)?.[0]).toBeUndefined();
  });
});
