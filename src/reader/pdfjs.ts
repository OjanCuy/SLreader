// The only place that sets up PDF.js. Components get an opened document from here.
//
// The PDF bytes come from Rust (api.readPaperPdf); PDF.js never fetches a PDF itself.
// Its helper files (character maps, standard fonts, image decoders, a color profile)
// are served by the app at /pdfjs/, see the pdfjsAssets plugin in vite.config.ts.
import {
  getDocument,
  GlobalWorkerOptions,
  type PDFDocumentLoadingTask,
  type PDFDocumentProxy,
} from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import type { PageSize } from "./layout";
import type { TextItem } from "./search";

GlobalWorkerOptions.workerSrc = workerUrl;

const assets = (folder: string) => new URL(`/pdfjs/${folder}/`, window.location.href).href;

export interface OpenedPdf {
  doc: PDFDocumentProxy;
  /** Every page at 100%, in PDF points, with the page's own rotation applied. */
  sizes: PageSize[];
  /** Frees the document and its worker memory. Safe to call more than once. */
  close: () => void;
}

/** Thrown when PDF.js cannot open the file: damaged, not a PDF, or password-protected. */
export class PdfOpenError extends Error {
  constructor(readonly passwordProtected: boolean, cause: unknown) {
    super("PDF.js could not open the document", { cause });
  }
}

export async function openPdf(data: ArrayBuffer): Promise<OpenedPdf> {
  const task: PDFDocumentLoadingTask = getDocument({
    // PDF.js takes ownership of the bytes; the caller keeps no copy.
    data: new Uint8Array(data),
    cMapUrl: assets("cmaps"),
    standardFontDataUrl: assets("standard_fonts"),
    wasmUrl: assets("wasm"),
    iccUrl: assets("iccs"),
    // XFA forms are interactive government forms, not journal articles.
    enableXfa: false,
  });
  const close = () => void task.destroy();
  try {
    const doc = await task.promise;
    const sizes = await Promise.all(
      Array.from({ length: doc.numPages }, async (_, index) => {
        const viewport = (await doc.getPage(index + 1)).getViewport({ scale: 1 });
        return { width: viewport.width, height: viewport.height };
      }),
    );
    return { doc, sizes, close };
  } catch (failure) {
    close();
    const name = (failure as { name?: string } | null)?.name;
    throw new PdfOpenError(name === "PasswordException", failure);
  }
}

/**
 * The text items of every page, for searching. They are the same items, in the same
 * order, as in the page's text layer. A page whose text cannot be read counts as empty.
 * Rejects when the document is closed meanwhile.
 */
export async function readPageTexts(doc: PDFDocumentProxy): Promise<TextItem[][]> {
  const pages: TextItem[][] = [];
  // One page at a time, so that drawing pages is not held up.
  for (let number = 1; number <= doc.numPages; number++) {
    const page = await doc.getPage(number);
    try {
      const { items } = await page.getTextContent();
      pages.push(items.filter((item) => "str" in item));
    } catch (failure) {
      console.error(`The text of page ${number} could not be read`, failure);
      pages.push([]);
    }
  }
  return pages;
}

/** PDF.js rejects a render or text layer that was cancelled on purpose with this. */
export function isCancelled(failure: unknown): boolean {
  const name = (failure as { name?: string } | null)?.name;
  return name === "RenderingCancelledException" || name === "AbortException";
}
