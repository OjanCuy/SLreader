// Checks the color tokens against the design brief:
// the contrast pairs of Section 10 must pass WCAG 2.1 AA in both themes.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

type Theme = 0 | 1; // light, dark

const colors = new Map<string, [string, string]>();
for (const m of css.matchAll(/--([\w-]+):\s*light-dark\((#[0-9a-f]{6}),\s*(#[0-9a-f]{6})\)/g)) {
  colors.set(m[1], [m[2], m[3]]);
}

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(foreground: string, background: string, theme: Theme): number {
  const pick = (name: string) => {
    const value = colors.get(name);
    if (!value) throw new Error(`Unknown color token --${name}`);
    return luminance(value[theme]);
  };
  const [high, low] = [pick(foreground), pick(background)].sort((a, b) => b - a);
  return (high + 0.05) / (low + 0.05);
}

// [foreground, background, required ratio]
const pairs: [string, string, number][] = [
  ["text-primary", "surface-panel", 4.5],
  ["text-primary", "surface-app", 4.5],
  ["text-primary", "surface-sunken", 4.5],
  ["text-secondary", "surface-panel", 4.5],
  ["text-secondary", "surface-app", 4.5],
  ["text-secondary", "surface-sunken", 4.5],
  ["accent", "surface-panel", 4.5],
  ["accent", "surface-app", 4.5],
  ["accent-on", "accent", 4.5],
  ["decision-include", "surface-panel", 4.5],
  ["decision-exclude", "surface-panel", 4.5],
  ["decision-undecided", "surface-panel", 4.5],
  ["decision-include-on", "decision-include", 4.5],
  ["decision-exclude-on", "decision-exclude", 4.5],
  ["decision-undecided-on", "decision-undecided", 4.5],
  ["status-error", "surface-panel", 4.5],
  ["status-warning", "surface-panel", 4.5],
  ["status-success", "surface-panel", 4.5],
  ["ai-ink", "surface-panel", 4.5],
  ["border-control", "surface-panel", 3],
  ["border-control", "surface-app", 3],
  ["focus-ring", "surface-panel", 3],
  ["focus-ring", "surface-app", 3],
  ["hl-include-swatch", "surface-panel", 3],
  ["hl-exclude-swatch", "surface-panel", 3],
  ["hl-method-swatch", "surface-panel", 3],
  ["hl-result-swatch", "surface-panel", 3],
];

describe.each([
  ["light", 0],
  ["dark", 1],
] as const)("%s theme contrast", (_name, theme) => {
  it.each(pairs)("%s on %s is at least %s:1", (foreground, background, required) => {
    expect(contrast(foreground, background, theme)).toBeGreaterThanOrEqual(required);
  });
});

describe("token values", () => {
  it("matches sample ratios printed in the design brief", () => {
    expect(contrast("text-primary", "surface-panel", 0)).toBeCloseTo(15.9, 1);
    expect(contrast("text-primary", "surface-panel", 1)).toBeCloseTo(13.4, 1);
    expect(contrast("accent", "surface-panel", 0)).toBeCloseTo(11.0, 1);
    expect(contrast("accent", "surface-panel", 1)).toBeCloseTo(7.5, 1);
  });
});
