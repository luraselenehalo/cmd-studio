// One glyph is one cell (V2). The core counts a character such as U+273B as one cell, and so do
// conhost and Windows Terminal, which draw each cell in the same box whatever font the glyph
// comes from. A web page does not: the preview's font is Cascadia Mono (or the theme's), and a
// glyph it lacks falls back to another font and is drawn wider or narrower than 1ch, which
// pushes the rest of the row, the welcome box's right edge `│`, out of line.
//
// So the preview puts every such glyph in a box of its own, as wide as the core counts it (1ch
// for nearly all). This decides no width: the cells are the core's count (`unicode-width` in
// crates/core/src/preview/cells.rs, whose fixtures the tests of cells.test.ts repeat), and this
// only makes the page honour it for the glyphs a monospace font may not have. What it does not box:
//  - what Cascadia Mono has and draws in one cell by itself: ASCII, Latin (U+0000 to U+024F), the
//    box-drawing and block-element blocks (U+2500 to U+259F);
//  - what is two cells wide (Chinese, Japanese, Korean, emoji, a few symbols such as U+2329),
//    which the font's own advance is nearest to;
//  - what the font draws as one glyph over several characters: an emoji with the emoji look
//    (U+FE0F), and any sequence joined with U+200D, which must not be cut by a box;
//  - what takes no cell by itself (a zero-width space, a lone mark).
// The text itself is not touched: the boxes hold the characters as they are.

/** What takes no cell after a character, and so stays in the cell before it: marks (the Thai
 * vowel and tone marks above and below a letter, a combining accent, the emoji look), format
 * characters (U+200B ZERO WIDTH SPACE, U+200D JOINER, bidi marks, ...), and the few letters the
 * core counts as none (conjoining Hangul vowels and finals, the halfwidth voiced marks). */
const ZERO_WIDTH = String.raw`\p{M}\p{Cf}\u{0d4e}\u{1160}-\u{11ff}\u{a8fa}\u{d7b0}-\u{d7fb}\u{ff9e}\u{ff9f}`;
/** Thai and Lao SARA AM (U+0E33, U+0EB3) is a letter that follows the consonant and its tone
 * mark, and the core counts it as a cell of its own; the shaper draws it with them, so it is in
 * their box, which is one cell wider. The spacing vowel signs of other scripts (Devanagari and
 * the like), which the core mostly counts as a cell too, are counted as none here, as before:
 * only the Thai a Thai user's name holds is handled. */
const SARA_AM = String.raw`\u{0e33}\u{0eb3}`;
/** A character with what follows it in its glyph (a run of those alone, for a text that starts
 * with some). */
const CLUSTER = new RegExp(String.raw`[^${ZERO_WIDTH}${SARA_AM}][${ZERO_WIDTH}${SARA_AM}]*|[${ZERO_WIDTH}${SARA_AM}]+`, "gu");
const TAKES_NO_CELL = new RegExp(String.raw`^[${ZERO_WIDTH}]$`, "u");
/** Drawn in one cell by Cascadia Mono itself, and so by any monospace font. */
const OWN_CELL = /^[\u{0000}-\u{024f}\u{2500}-\u{259f}]/u;
/** Two cells wide: Hangul Jamo (initials), the angle brackets U+2329 and U+232A, the trigrams and
 * hexagram symbols of Miscellaneous Symbols, CJK and its symbols, Hangul, compatibility
 * ideographs, vertical and small CJK forms, fullwidth forms, everything outside the first plane
 * (emoji, extended CJK), the Khmer vowel U+17A4 that `unicode-width` counts as two, and the BMP
 * emoji that default to emoji presentation. Checked against `unicode-width` over the whole BMP:
 * it counts no BMP character as two that is not matched here. */
const WIDE = new RegExp(
  String.raw`^(?:[\u{1100}-\u{115f}\u{17a4}\u{2329}\u{232a}\u{2630}-\u{2637}\u{268a}-\u{268f}\u{2e80}-\u{a4cf}\u{a960}-\u{a97c}\u{ac00}-\u{d7a3}\u{f900}-\u{faff}\u{fe10}-\u{fe19}\u{fe30}-\u{fe6f}\u{ff00}-\u{ff60}\u{ffe0}-\u{ffe6}\u{10000}-\u{10ffff}]|\p{Emoji_Presentation})`,
  "u",
);
/** VARIATION SELECTOR-16: asks for the emoji look, which Windows Terminal draws two cells wide. */
const EMOJI_LOOK = "\u{fe0f}";
/** ZERO WIDTH JOINER: what is joined with it is one glyph. */
const JOINER = "\u{200d}";

export interface Piece {
  text: string;
  /** In a box of its own (always exactly one cluster), as wide as `cells`. */
  cell: boolean;
  /** The width of the box, in cells; 0 for a run that is left to the font. */
  cells: number;
}

/** `text` cut into runs of what draws itself and single boxed glyphs, in order; the pieces'
 * texts joined are `text`. */
export function pieces(text: string): Piece[] {
  // The clusters, with a sequence joined by U+200D kept whole.
  const clusters: string[] = [];
  for (const [cluster] of text.matchAll(CLUSTER)) {
    const last = clusters.at(-1);
    if (last !== undefined && last.endsWith(JOINER)) clusters[clusters.length - 1] = last + cluster;
    else clusters.push(cluster);
  }
  const out: Piece[] = [];
  for (const cluster of clusters) {
    const cells = [...cluster].filter((c) => !TAKES_NO_CELL.test(c)).length;
    const cell =
      cells > 0 &&
      !OWN_CELL.test(cluster) &&
      !WIDE.test(cluster) &&
      !cluster.includes(EMOJI_LOOK) &&
      !cluster.includes(JOINER);
    const last = out.at(-1);
    if (!cell && last && !last.cell) last.text += cluster;
    else out.push({ text: cluster, cell, cells: cell ? cells : 0 });
  }
  return out;
}
