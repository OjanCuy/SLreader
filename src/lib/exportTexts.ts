// UI texts of the Export menu, O-05 (Brief Section 7, approved in T-28 to T-30).

export const DECISIONS_ITEM = "Decisions (CSV)";
export const EXTRACTION_ITEM = "Extraction (CSV)";
export const NOTHING_TO_EXPORT = "No papers yet.";
export const NO_TAGS_TO_EXPORT = "No tags yet.";
export const exportedMessage = (fileName: string) => `Exported ${fileName}`;
export const fileInUseMessage = (fileName: string) =>
  `${fileName} is open in another app. Close it or save with a new name.`;
export const SAVE_AS = "Save as…";
export const MARKDOWN_ITEM = "Highlights and notes (Markdown)";
export const exportedFilesMessage = (count: number, folderName: string) =>
  `Exported ${count} ${count === 1 ? "file" : "files"} to ${folderName}`;
