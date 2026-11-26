// UI texts and small rules of S-04 "This library" (Brief Section 7, approved in T-35).

import type { CriteriaKind } from "../lib/api";

export type SettingsSection = "questions" | CriteriaKind;

export const SECTIONS: { value: SettingsSection; label: string }[] = [
  { value: "questions", label: "Research questions" },
  { value: "inclusion", label: "Inclusion criteria" },
  { value: "exclusion", label: "Exclusion criteria" },
  { value: "tag", label: "Extraction tags" },
];

export const SAVED = "Saved";
export const REQUIRED = "This field is required.";

export const DELETE_TITLES: Record<CriteriaKind, string> = {
  inclusion: "Delete inclusion criterion?",
  exclusion: "Delete exclusion reason?",
  tag: "Delete tag?",
};

/** `name` is the code of a criterion, or the name of a tag. */
export function deleteMessage(kind: CriteriaKind, name: string, uses: number): string {
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
