import { describe, expect, it } from "vitest";
import { backupErrorMessage, replaceMessage, restoredMessage } from "./backupTexts";

describe("backup texts", () => {
  it("names what will be replaced", () => {
    const at = (replacedPapers: number, replacedHighlights: number) =>
      replaceMessage({ replacedPapers, replacedHighlights, papersAfter: 0 });
    expect(at(12, 40)).toBe(
      "12 papers and 40 highlights in this library will be replaced with their versions from the backup. Everything else is kept.",
    );
    expect(at(1, 1)).toMatch(/^1 paper and 1 highlight in this library/);
    expect(at(3, 0)).toMatch(/^3 papers in this library/);
    expect(at(0, 1)).toMatch(/^1 highlight in this library/);
  });

  it("reports what was added (PRD Section 8)", () => {
    expect(restoredMessage({ addedReasons: 0, addedTags: 0 })).toBe("Backup restored.");
    expect(restoredMessage({ addedReasons: 2, addedTags: 1 })).toBe(
      "Backup restored. Added 2 exclusion reasons and 1 tag from the backup.",
    );
    expect(restoredMessage({ addedReasons: 1, addedTags: 0 })).toBe(
      "Backup restored. Added 1 exclusion reason from the backup.",
    );
    expect(restoredMessage({ addedReasons: 0, addedTags: 3 })).toBe(
      "Backup restored. Added 3 tags from the backup.",
    );
  });

  it("explains a file that cannot be restored", () => {
    expect(backupErrorMessage({ code: "not_a_backup", detail: "" }, "x")).toBe(
      "This file isn't a SLReader backup.",
    );
    expect(backupErrorMessage({ code: "newer_backup", detail: "" }, "x")).toBe(
      "This backup was made with a newer SLReader. Update the app to restore it.",
    );
    expect(backupErrorMessage({ code: "other", detail: "" }, "fallback")).toBe("fallback");
    expect(backupErrorMessage(new Error("?"), "fallback")).toBe("fallback");
  });
});
