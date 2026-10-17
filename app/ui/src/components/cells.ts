// One glyph is one cell (V2). The core counts a character such as U+273B as one cell, and so do
// conhost and Windows Terminal, which draw each cell in the same box whatever font the glyph
// comes from. A web page does not: the preview's font is Cascadia Mono (or the theme's), and a
// glyph it lacks falls back to another font and is drawn wider or narrower than 1ch, which
// pushes the rest of the row, the welcome box's right edge `│`, out of line.
//
// So the preview puts every such glyph in a box of its own, exactly 1ch wide. This decides no
// width: the cells are the core's count (`unicode-width` in `preview/cells.rs`), and this only
// makes the page honour it for the glyphs a monospace font may not have. What it does not box:
//  - what Cascadia Mono has and draws in one cell by itself: ASCII, Latin (U+0000 to U+024F), the
//    box-drawing and block-element blocks (U+2500 to U+259F);
//  - what is two cells wide (Chinese, Japanese, Korean, emoji), which the font's own advance is
//    nearest to, and a glyph with an emoji variation selector (two cells in Windows Terminal).
// The text itself is not touched: the boxes hold the characters as they are.

/** A character with the marks after it (Thai vowels and tone marks above and below a letter, a
 * combining accent): the core counts the marks as no cell, so the character and its marks are one
 * cell. A mark that leads the text, with no character before it, is a cluster of its own. */
const CLUSTER = /\P{M}\p{M}*|\p{M}+/gu;
/** Drawn in one cell by Cascadia Mono itself, and so by any monospace font. */
const OWN_CELL = /^[\u0000-ɏ─-▟]/u;
/** Two cells wide: Hangul Jamo, CJK and its symbols, Hangul, compatibility ideographs, CJK forms,
 * fullwidth forms, everything outside the first plane (emoji, extended CJK), and the BMP emoji
 * that default to emoji presentation. */
const WIDE = /^(?:[ᄀ-ᅟ⺀-꓏가-힣豈-﫿︰-﹯＀-｠￠-￦\u{10000}-\u{10ffff}]|\p{Emoji_Presentation})/u;
/** VARIATION SELECTOR-16: asks for the emoji look, which Windows Terminal draws two cells wide. */
const EMOJI_LOOK = "️";

export interface Piece {
  text: string;
  /** In a box of its own, one cell wide (always exactly one cluster). */
  cell: boolean;
}

/** `text` cut into runs of what draws itself and single boxed glyphs, in order; the pieces'
 * texts joined are `text`. */
export function pieces(text: string): Piece[] {
  const out: Piece[] = [];
  for (const [cluster] of text.matchAll(CLUSTER)) {
    const cell = !OWN_CELL.test(cluster) && !WIDE.test(cluster) && !cluster.includes(EMOJI_LOOK);
    const last = out.at(-1);
    if (!cell && last && !last.cell) last.text += cluster;
    else out.push({ text: cluster, cell });
  }
  return out;
}
