// The example criteria and tags of "Load a template" (FR-08.1, Brief O-09), and its
// texts. The content is the prototype web app's, in English (approved in T-36).
// Inclusion criteria and research questions have no template.

import type { SettingsSection } from "./settingsTexts";

export type TemplateKind = "exclusion" | "tag";

export interface TemplateItem {
  /** Empty for a tag. */
  code: string;
  label: string;
}

const REASONS = [
  "Wrong document type / no full text",
  "Language or year outside criteria",
  "Not about fish farming operating systems",
  "Does not address the intervention",
  "Duplicate / no data for the research questions",
  "RAS is not the object",
  "Pure simulation / standalone algorithm / review",
  "Non-finfish commodity",
  "Design not detailed / no quantitative results",
];

const TAGS = [
  "Species",
  "RAS scale",
  "Intervention",
  "Trial duration",
  "FCR",
  "Quantitative results",
  "Limitations",
];

export const TEMPLATES: Record<TemplateKind, TemplateItem[]> = {
  exclusion: REASONS.map((label, at) => ({ code: `E${at + 1}`, label })),
  tag: TAGS.map((label) => ({ code: "", label })),
};

export const hasTemplate = (kind: SettingsSection): kind is TemplateKind =>
  kind === "exclusion" || kind === "tag";

export const LOAD_TEMPLATE = "Load a template";
export const TEMPLATE_NAME = "Fish farming (RAS) example";
export const NOTHING_TO_ADD = "This list already has every item in the template.";
export const LOAD = "Load";

export const addedNote = (count: number) =>
  `${count === 1 ? "1 item" : `${count} items`} will be added. Items you already have are skipped.`;

/**
 * The template items a list lacks: a reason is there when its code is, a tag when its
 * name is, whatever the letter case. A new row without text does not count.
 */
export function missingItems(
  kind: TemplateKind,
  rows: { code: string; label: string; stored: TemplateItem | null }[],
): TemplateItem[] {
  const key = (item: TemplateItem) => (kind === "tag" ? item.label : item.code).trim().toLowerCase();
  const known = new Set(
    rows
      .filter((row) => row.stored !== null || row.label.trim() !== "")
      .flatMap((row) => (row.stored ? [key(row), key(row.stored)] : [key(row)])),
  );
  return TEMPLATES[kind].filter((item) => !known.has(key(item)));
}
