// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readSelection } from "./selection";

// Two drawn pages, as PageView builds them: each line of text is a span, line ends are <br>.
const PAGES = `
  <div data-page="3"><div class="textLayer"
    ><span data-line="0">Feed conversion </span><br
    ><span data-line="1">ratio was 1.4</span
    ><div class="endOfContent"></div></div></div>
  <div data-page="4"><div class="textLayer"><span data-line="0">Results on page four</span></div></div>
`;

/**
 * jsdom has no layout. Each character is 10 px wide and each line 12 px high, 20 px
 * apart; pages are 1000 x 1000 px, one below the other.
 */
function fakeLayout(root: HTMLElement) {
  root.querySelectorAll<HTMLElement>("[data-page]").forEach((page, index) => {
    page.getBoundingClientRect = () => new DOMRect(0, index * 1000, 1000, 1000);
  });
  Range.prototype.getClientRects = function (this: Range) {
    const span = this.startContainer.parentElement!;
    const pageIndex = Array.from(root.querySelectorAll("[data-page]")).indexOf(span.closest("[data-page]")!);
    const top = pageIndex * 1000 + 100 + Number(span.dataset.line) * 20;
    const rect = new DOMRect(100 + this.startOffset * 10, top, (this.endOffset - this.startOffset) * 10, 12);
    return [rect] as unknown as DOMRectList;
  };
}

let root: HTMLElement;

beforeEach(() => {
  root = document.createElement("div");
  root.innerHTML = PAGES;
  document.body.append(root);
  fakeLayout(root);
});

afterEach(() => {
  root.remove();
  document.getSelection()?.removeAllRanges();
  vi.restoreAllMocks();
});

function textOf(page: string, line: number): Text {
  return root.querySelector(`[data-page="${page}"] [data-line="${line}"]`)!.firstChild as Text;
}

function select(from: [Text, number], to: [Text, number]): Selection {
  const range = document.createRange();
  range.setStart(...from);
  range.setEnd(...to);
  const selection = document.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
  return selection;
}

describe("reading the selection for a highlight", () => {
  it("gives the page, the selected text, and one rectangle per line in shares of the page", () => {
    const result = readSelection(select([textOf("3", 0), 5], [textOf("3", 1), 9]), root);
    expect(result).not.toBeNull();
    expect(result!.page).toBe(3);
    expect(result!.text).toBe("conversion ratio was");
    expect(result!.crossed).toBe(false);
    // "conversion " from character 5 to 16, then "ratio was" from 0 to 9 on the next line.
    const close = (x: number, y: number, w: number, h: number) => ({
      x: expect.closeTo(x, 10),
      y: expect.closeTo(y, 10),
      w: expect.closeTo(w, 10),
      h: expect.closeTo(h, 10),
    });
    expect(result!.rects).toEqual([close(0.15, 0.1, 0.11, 0.012), close(0.1, 0.12, 0.09, 0.012)]);
    expect(result!.bounds).toEqual({ left: 100, top: 100, right: 260, bottom: 132 });
  });

  it("keeps only the first page of a selection over two pages (FR-03.5)", () => {
    const result = readSelection(select([textOf("3", 1), 6], [textOf("4", 0), 7]), root);
    expect(result!.page).toBe(3);
    expect(result!.text).toBe("was 1.4");
    expect(result!.crossed).toBe(true);
    expect(result!.rects).toHaveLength(1);
  });

  it("starts on the next page when nothing is selected on the first one", () => {
    const end = textOf("3", 1);
    const result = readSelection(select([end, end.length], [textOf("4", 0), 7]), root);
    expect(result!.page).toBe(4);
    expect(result!.text).toBe("Results");
    expect(result!.crossed).toBe(false);
  });

  it("gives nothing without a selection, or for a selection outside the pages", () => {
    expect(readSelection(null, root)).toBeNull();
    const text = textOf("3", 0);
    expect(readSelection(select([text, 3], [text, 3]), root)).toBeNull();

    const outside = document.createElement("p");
    outside.textContent = "Search in PDF";
    document.body.append(outside);
    expect(readSelection(select([outside.firstChild as Text, 0], [outside.firstChild as Text, 6]), root)).toBeNull();
    outside.remove();
  });
});
