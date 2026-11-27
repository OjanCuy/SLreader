// UI texts and small rules of S-04 "This library" (Brief Section 7, approved in T-35).

import type { CriteriaKind } from "../lib/api";

export type SettingsSection = "questions" | CriteriaKind;

export const SECTIONS: { value: SettingsSection; label: string }[] = [
  { value: "questions", label: "Research questions" },
  { value: "inclusion", label: "Inclusion criteria" },
  { value: "exclusion", label: "Exclusion criteria" },
  { value: "tag", label: "Extraction tags" },
];

/** The "App" group: settings shared by all libraries (FR-08.5). */
export type AppSection = "export" | "appearance";

/** Any section of the Settings screen. */
export type SettingsPage = SettingsSection | AppSection;

export const APP_SECTIONS: { value: AppSection; label: string }[] = [
  { value: "export", label: "Export" },
  { value: "appearance", label: "Appearance" },
];

export const DELIMITER_LABEL = "CSV separator";
export const DELIMITER_OPTIONS = [
  { value: ";", label: "Semicolon" },
  { value: ",", label: "Comma" },
] as const;

export const THEME_LABEL = "Theme";

export const SAVED = "Saved";
export const REQUIRED = "This field is required.";

export const DELETE_TITLES: Record<SettingsSection, string> = {
  questions: "Delete research question?",
  inclusion: "Delete inclusion criterion?",
  exclusion: "Delete exclusion reason?",
  tag: "Delete tag?",
};

/** The number of a research question, by its place in the list. */
export const questionCode = (index: number) => `RQ${index + 1}`;

/** `name` is the code of a criterion or question, or the name of a tag. */
export function deleteMessage(kind: SettingsSection, name: string, uses: number): string {
  if (uses === 0) return `${name} will be deleted. This can't be undone.`;
  if (kind === "tag") {
    return uses === 1
      ? "1 highlight uses this tag. Delete anyway?"
      : `${uses} highlights use this tag. Delete anyway?`;
  }
  return uses === 1
    ? "1 paper uses this reason. Delete anyway?"
    : `${uses} papers use this reason. Delete anyway?`;
}

/** The code offered for a new row: I3 after I1 and I2, E1 in an empty list. */
export function nextCode(kind: "inclusion" | "exclusion", codes: string[]): string {
  const prefix = kind === "inclusion" ? "I" : "E";
  const pattern = new RegExp(`^${prefix}(\\d+)$`, "i");
  const numbers = codes.map((code) => Number(pattern.exec(code.trim())?.[1] ?? 0));
  return `${prefix}${Math.max(0, ...numbers) + 1}`;
}

/** The codes used more than once, as first written; letter case does not matter. */
export function repeatedCodes(codes: string[]): string[] {
  const seen = new Map<string, { code: string; times: number }>();
  for (const code of codes.map((each) => each.trim())) {
    if (code === "") continue;
    const entry = seen.get(code.toLowerCase()) ?? { code, times: 0 };
    entry.times++;
    seen.set(code.toLowerCase(), entry);
  }
  return [...seen.values()].filter((entry) => entry.times > 1).map((entry) => entry.code);
}
