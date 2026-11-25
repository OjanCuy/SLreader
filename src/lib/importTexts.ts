// Messages after importing PDFs (Brief Section 7, row "S-02 Paper list").
// Do not reword them. "1 PDF added." was approved in T-12.

import type { ImportReport } from "./api";
import type { ToastTone } from "./components/toasts.svelte";
import { SAVE_ERROR } from "./libraryTexts";

export const MAX_PAPERS = 250;
export const DROP_HERE = "Drop PDF files to add them";
export const ONLY_PDFS = "Only PDF files can be added";

export interface ImportMessage {
  message: string;
  tone: ToastTone;
}

/** The toasts to show for one import, in order: the good news first. */
export function importMessages(report: ImportReport): ImportMessage[] {
  const outcomes = report.files.map((file) => file.outcome);
  const overLimit = outcomes.includes("over_limit");
  const messages: ImportMessage[] = [];

  if (report.added > 0 && !overLimit) {
    const message = report.added === 1 ? "1 PDF added." : `${report.added} PDFs added.`;
    messages.push({ message, tone: "success" });
  }
  for (const file of report.files) {
    if (file.outcome === "already_in_library") {
      messages.push({ message: `Already in library: ${file.fileName}`, tone: "error" });
    } else if (file.outcome === "not_pdf") {
      messages.push({ message: `Could not open ${file.fileName}.`, tone: "error" });
    }
  }
  if (outcomes.includes("write_failed")) {
    messages.push({ message: SAVE_ERROR, tone: "error" });
  }
  if (overLimit) {
    messages.push({
      message: `Library limit is ${MAX_PAPERS} papers. ${report.added} of ${report.files.length} files were added.`,
      tone: "error",
    });
  }
  return messages;
}

/** True when at least one of the dragged files may be a PDF. */
export function hasPdf(paths: string[]): boolean {
  return paths.some((path) => path.toLowerCase().endsWith(".pdf"));
}
