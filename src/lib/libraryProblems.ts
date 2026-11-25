// Fixed messages for library problems found by the Rust core (T-07).
// Texts are copied from the design brief, Section 7, row "S-01 Libraries".
// The keys match `LibraryProblem` in src-tauri/src/db/mod.rs.

export type LibraryProblem = "newer_version" | "damaged" | "not_found";

export interface LibraryProblemText {
  message: string;
  /** Label of the button that offers a way out, if any. */
  action?: "Restore from backup" | "Locate folder";
  /** The library is opened, but nothing in it can be changed. */
  readOnly: boolean;
}

export const LIBRARY_PROBLEMS: Readonly<Record<LibraryProblem, LibraryProblemText>> = {
  newer_version: {
    message: "Created with a newer SLReader. Update the app to edit this library.",
    readOnly: true,
  },
  damaged: {
    message: "This library's database is damaged.",
    action: "Restore from backup",
    readOnly: false,
  },
  not_found: {
    message: "Folder not found.",
    action: "Locate folder",
    readOnly: false,
  },
};
