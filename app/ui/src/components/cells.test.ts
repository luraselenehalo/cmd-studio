// How the preview cuts a text into cells (cells.ts), against the core's own count (the fixtures
// are those of crates/core/src/preview/cells.rs: `prompt_cells` counts a character at a time and
// `banner_cells` counts the whole text, which differ only for a joined emoji).
import { describe, expect, it } from "vitest";
import { pieces, type Piece } from "./cells";

/** The cells of `text` as the page draws them: a boxed glyph is the box's width, and a run left
 * to the font is its characters that take a cell (a mark or a format character takes none).
 * Only meaningful for text with no wide character, which the font draws by its own advance. */
function cellsOf(text: string): number {
  return pieces(text).reduce(
    (sum, piece) => sum + (piece.cell ? piece.cells : [...piece.text].filter((c) => !/[\p{M}\p{Cf}]/u.test(c)).length),
    0,
  );
}

const texts = (list: Piece[]) => list.map((p) => p.text);
const boxed = (list: Piece[]) => list.filter((p) => p.cell).map((p) => p.text);

describe("the cells of a text", () => {
  it("counts as the core does", () => {
    // [text, the core's count]: from cells.rs's tests, and the Thai the welcome box can hold.
    const fixtures: [string, number][] = [
      ["abc", 3],
      ["ผู้ทดสอบ", 6],
      ["ที่นี่", 2],
      ["มี.ค.", 4],
      ["·×①", 3],
      ["\u{e0b0}\u{e0a0}╭─✓✗❯", 7],
      ["✻ Welcome", 9],
      ["ﾊ", 1],
      // Thai SARA AM is a cell of its own, after the letter and its tone mark.
      ["น้ำ", 2],
      ["สำนักงาน", 7],
      ["ผู้ใช้", 3],
      // A zero-width space takes no cell.
      ["a\u{200b}b", 2],
      ["\u{200b}✻", 1],
    ];
    for (const [text, core] of fixtures) expect(cellsOf(text), JSON.stringify(text)).toBe(core);
  });

  it("keeps a Thai letter, its marks and its SARA AM in one box as wide as the core counts them", () => {
    // น + MAI THO + SARA AM: the shaper puts the ring of SARA AM over the letter, below the tone
    // mark, so the three cannot be in boxes of their own; the core counts 2 cells (น้ and ำ).
    expect(pieces("น้ำ")).toEqual([{ text: "น้ำ", cell: true, cells: 2 }]);
    expect(pieces("สำนักงาน").map((p) => [p.text, p.cells])).toEqual([
      ["สำ", 2],
      ["นั", 1],
      ["ก", 1],
      ["ง", 1],
      ["า", 1],
      ["น", 1],
    ]);
    // The same letters with marks only: a box of one cell each.
    expect(pieces("ผู้ใช้").map((p) => [p.text, p.cells])).toEqual([
      ["ผู้", 1],
      ["ใ", 1],
      ["ช้", 1],
    ]);
  });

  it("gives a zero-width format character no box, and keeps it with the cell before it", () => {
    // Left to the text around it...
    expect(pieces("a\u{200b}b")).toEqual([{ text: "a\u{200b}b", cell: false, cells: 0 }]);
    // ...alone in front of a glyph...
    expect(pieces("\u{200b}✻")).toEqual([
      { text: "\u{200b}", cell: false, cells: 0 },
      { text: "✻", cell: true, cells: 1 },
    ]);
    // ...or inside the glyph's box after it, where it takes no cell.
    expect(pieces("✻\u{200b}x")).toEqual([
      { text: "✻\u{200b}", cell: true, cells: 1 },
      { text: "x", cell: false, cells: 0 },
    ]);
    // A word joiner, a bidi mark and a zero-width non-joiner are the same.
    for (const format of ["\u{2060}", "\u{200e}", "\u{200c}", "\u{feff}"]) {
      expect(boxed(pieces(`✓${format}`)), JSON.stringify(format)).toEqual([`✓${format}`]);
      expect(boxed(pieces(`a${format}b`)), JSON.stringify(format)).toEqual([]);
    }
  });

  it("never splits a joined emoji sequence, nor puts a box in it", () => {
    const family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    expect(pieces(family)).toEqual([{ text: family, cell: false, cells: 0 }]);
    // Between two glyphs the sequence stays whole in the run between their boxes.
    expect(pieces(`✻ ${family} ✻`)).toEqual([
      { text: "✻", cell: true, cells: 1 },
      { text: ` ${family} `, cell: false, cells: 0 },
      { text: "✻", cell: true, cells: 1 },
    ]);
    // A joiner after a glyph that would be boxed pulls the sequence out of its box: the font
    // draws the whole sequence, the DOM does not cut it.
    const heart = "\u{2764}\u{200d}\u{1f525}";
    expect(pieces(heart)).toEqual([{ text: heart, cell: false, cells: 0 }]);
    // The emoji look (VS16) is two cells in Windows Terminal: not boxed either.
    expect(boxed(pieces("\u{2764}\u{fe0f}"))).toEqual([]);
  });

  it("does not box what the core counts as two cells", () => {
    // Wide characters of the BMP that are not CJK, emoji or fullwidth forms.
    const wide = [
      "\u{2329}",
      "\u{232a}",
      "\u{2630}",
      "\u{268a}",
      "\u{17a4}",
      "\u{a960}",
      "\u{fe10}",
      "\u{231a}",
      "\u{2705}",
      "\u{1100}",
      "中",
      "한",
      "Ａ",
      "📁",
    ];
    for (const glyph of wide) expect(boxed(pieces(`x${glyph}y`)), JSON.stringify(glyph)).toEqual([]);
    expect(texts(pieces("\u{2329}x\u{232a}"))).toEqual(["\u{2329}x\u{232a}"]);
    // A Korean syllable written as its Jamo: the initial is wide, the vowel and final take none.
    const jamo = "\u{1112}\u{1161}\u{11ab}";
    expect(pieces(jamo)).toEqual([{ text: jamo, cell: false, cells: 0 }]);
  });

  it("still boxes what is one cell and may be missing from the font", () => {
    expect(boxed(pieces("✻ ↑ ✓ ✗ ❯ \u{e0b0} ① ﾊ"))).toEqual(["✻", "↑", "✓", "✗", "❯", "\u{e0b0}", "①", "ﾊ"]);
    // A halfwidth voiced mark stays with its kana.
    expect(pieces("ﾊﾞ")).toEqual([{ text: "ﾊﾞ", cell: true, cells: 1 }]);
  });

  it("changes no text: the pieces joined are the text", () => {
    const samples = [
      "",
      "C:\\Users\\tester> dir",
      "น้ำ สำนักงาน ผู้ใช้ มี.ค.",
      "✻\u{200b}\u{200d}✓ \u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} ❤\u{fe0f} 中文 한글",
      "\u{200b}\u{200d}\u{301}x",
      "a\u{d800}b",
      "\u{1112}\u{1161}\u{11ab}\u{2329}\u{a960}",
    ];
    for (const text of samples) expect(pieces(text).map((p) => p.text).join(""), JSON.stringify(text)).toBe(text);
  });
});
