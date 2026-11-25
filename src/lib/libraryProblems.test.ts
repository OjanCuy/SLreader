import { describe, expect, it } from "vitest";
import { LIBRARY_PROBLEMS } from "./libraryProblems";

describe("library problem texts", () => {
  it("match the design brief word for word", () => {
    expect(LIBRARY_PROBLEMS).toEqual({
      newer_version: {
        message: "Created with a newer SLReader. Update the app to edit this library.",
        readOnly: true,
      },
      damaged: {
        message: "This library's database is damaged.",
        action: "Restore from backup",
        readOnly: false,
      },
      not_found: { message: "Folder not found.", action: "Locate folder", readOnly: false },
    });
  });
});
