// Paints the spans the core drew (spec §5.6): a mock cmd window whose background, font and
// opacity come from the theme. Nothing here decides a color: `fg`/`bg` are the core's. The
// theme's opacity fades only the window's background, on a layer of its own under the text, as
// Windows Terminal's opacity does; the text stays opaque. A glyph the font may lack, which a
// fallback font would draw wider than one cell, sits in a box exactly 1ch wide (cells.ts), as the
// core counts it, so the welcome box's right edge lines up; the text itself is not changed.
import type { PreviewResult } from "../bindings/PreviewResult";
import type { Theme } from "../bindings/Theme";
import type { Line } from "../bindings/Line";
import { useT } from "../i18n";
import { pieces } from "./cells";

/** The box of a glyph of `cells` cells (1 but for Thai SARA AM): `cells` ch wide, the glyph
 * centered, so a fallback font's wider glyph spills out evenly on both sides instead of pushing
 * what follows. */
function cellStyle(cells: number) {
  return { display: "inline-flex", justifyContent: "center", width: `${cells}ch` } as const;
}

function Row({ line }: { line: Line }) {
  return (
    <>
      {line.map((span, i) => (
        <span
          key={i}
          style={{
            color: span.fg,
            backgroundColor: span.bg ?? undefined,
            fontWeight: span.bold ? 700 : 400,
          }}
        >
          {pieces(span.text).map((piece, j) =>
            piece.cell ? (
              <span key={j} className="cell" style={cellStyle(piece.cells)}>
                {piece.text}
              </span>
            ) : (
              piece.text
            ),
          )}
        </span>
      ))}
      {"\n"}
    </>
  );
}

export function TerminalPreview({
  theme,
  result,
  compact = false,
}: {
  theme: Theme;
  result: PreviewResult | null;
  /** Only the prompt rows (a theme card). */
  compact?: boolean;
}) {
  const t = useT();
  const { colors, terminal } = theme;
  const lines: Line[] = result ? (compact ? result.prompt : [...result.banner, ...result.prompt]) : [];
  return (
    <div className="terminal" role="img" aria-label={t("a11y.terminalPreview")}>
      <div
        className="terminal-bg"
        style={{ backgroundColor: colors.background, opacity: terminal.opacity / 100 }}
      />
      <pre
        className="terminal-text"
        style={{
          color: colors.foreground,
          fontFamily: `"${terminal.font}", "Cascadia Mono", Consolas, monospace`,
          // `padding` is `[horizontal, vertical]`; CSS takes vertical first.
          padding: `${terminal.padding[1]}px ${terminal.padding[0]}px`,
        }}
      >
        {lines.map((line, i) => (
          <Row key={i} line={line} />
        ))}
      </pre>
    </div>
  );
}
