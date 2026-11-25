import { describe, expect, it } from "vitest";
import {
  findMatches,
  firstMatchFrom,
  indexPage,
  segments,
  stepMatch,
  type Match,
  type TextItem,
} from "./search";

/** One item per string; a trailing "\n" means the line ends after the item. */
const items = (...parts: string[]): TextItem[] =>
  parts.map((part) => ({ str: part.replace(/\n$/, ""), hasEOL: part.endsWith("\n") }));

/** The text each result covers, taken from the original items. */
function found(query: string, ...parts: string[]): string[] {
  const list = items(...parts);
  return findMatches([indexPage(list)], query).map((match) =>
    segments(match, (item) => list[item].str.length)
      .map(({ item, from, to }) => list[item].str.slice(from, to))
      .join("|"),
  );
}

describe("findMatches", () => {
  it("finds every result, ignoring upper and lower case", () => {
    expect(found("fish", "Fish farms feed FISH to fish.")).toEqual(["Fish", "FISH", "fish"]);
  });

  it("returns nothing for an empty query or a word that is not there", () => {
    expect(found("", "text")).toEqual([]);
    expect(found("   ", "a b c d")).toEqual([]);
    expect(found("salmon", "tilapia")).toEqual([]);
  });

  it("finds a word that is split over several items", () => {
    expect(found("aquaculture", "Aqua", "cul", "ture systems")).toEqual(["Aqua|cul|ture"]);
  });

  it("finds a phrase across a line break and extra spaces", () => {
    expect(found("feed conversion ratio", "The feed\n", "conversion  ratio was 1.4")).toEqual([
      "feed|conversion  ratio",
    ]);
  });

  it("finds a hyphenated line break with and without the hyphen", () => {
    const lines = ["recircu-\n", "lating systems"];
    expect(found("recirculating", ...lines)).toEqual(["recircu-|lating"]);
    expect(found("recircu-lating", ...lines)).toEqual(["recircu-|lating"]);
    expect(found("meta-analysis", "a meta-\n", "analysis")).toEqual(["meta-|analysis"]);
    // The line end may come as an empty item of its own.
    expect(found("recirculating", "recircu-", "\n", "lating")).toEqual(["recircu-||lating"].map((t) => t.replace("||", "|")));
    expect(found("recirculating", "recircu­\n", "lating")).toEqual(["recircu­|lating"]);
  });

  it("keeps hyphens inside a line, ranges of numbers, and dashes between words", () => {
    expect(found("wellknown", "a well-known fact")).toEqual([]);
    expect(found("well-known", "a well‐known fact")).toEqual(["well‐known"]);
    expect(found("1015", "pages 10-\n", "15")).toEqual([]);
  });

  it("matches ligatures, accents, and curly quotes, and marks the right characters", () => {
    expect(found("fish", "the ﬁsh tank")).toEqual(["ﬁsh"]);
    expect(found("tank", "the ﬁsh tank")).toEqual(["tank"]);
    expect(found("muller", "by Müller (2020)")).toEqual(["Müller"]);
    expect(found("müller", "by Muller")).toEqual(["Muller"]);
    // An accent stored as its own character.
    expect(found("muller", "Müller")).toEqual(["Müller"]);
    expect(found("farmer's", "the farmer’s pond")).toEqual(["farmer’s"]);
  });

  it("keeps positions right after characters that take two code units", () => {
    expect(found("pond", "𝑥 = 1 in the pond")).toEqual(["pond"]);
    expect(found("x", "𝑥")).toEqual(["𝑥"]);
  });

  it("treats the query as plain text, not as a pattern", () => {
    expect(found("(p<0.05)", "significant (p<0.05) here")).toEqual(["(p<0.05)"]);
    expect(found("a.c", "abc a.c")).toEqual(["a.c"]);
    expect(found("[1]+", "see [1]+ and 1")).toEqual(["[1]+"]);
  });

  it("lists results page by page", () => {
    const pages = [items("no"), items("fish fish"), items("fish")].map(indexPage);
    expect(findMatches(pages, "fish").map((match) => match.page)).toEqual([1, 1, 2]);
  });
});

describe("moving between results", () => {
  const on = (page: number) => ({ page }) as Match;

  it("starts at the current page, or at the first result when none follow", () => {
    const matches = [on(0), on(2), on(5)];
    expect(firstMatchFrom(matches, 1)).toBe(1);
    expect(firstMatchFrom(matches, 2)).toBe(1);
    expect(firstMatchFrom(matches, 6)).toBe(0);
    expect(firstMatchFrom([], 0)).toBe(0);
  });

  it("goes round at both ends", () => {
    expect(stepMatch(0, 1, 3)).toBe(1);
    expect(stepMatch(2, 1, 3)).toBe(0);
    expect(stepMatch(0, -1, 3)).toBe(2);
    expect(stepMatch(0, 1, 0)).toBe(0);
  });
});
