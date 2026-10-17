// Paints the spans the core drew (spec §5.6): a mock cmd window whose background, font and
// opacity come from the theme. Nothing here decides a color: `fg`/`bg` are the core's.
import type { PreviewResult } from "../bindings/PreviewResult";
import type { Theme } from "../bindings/Theme";
import type { Line } from "../bindings/Line";
import { useT } from "../i18n";

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
          {span.text}
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
    <pre
      className="terminal"
      role="img"
      aria-label={t("a11y.terminalPreview")}
      style={{
        backgroundColor: colors.background,
        color: colors.foreground,
        fontFamily: `"${terminal.font}", "Cascadia Mono", Consolas, monospace`,
        opacity: terminal.opacity / 100,
        // `padding` is `[horizontal, vertical]`; CSS takes vertical first.
        padding: `${terminal.padding[1]}px ${terminal.padding[0]}px`,
      }}
    >
      {lines.map((line, i) => (
        <Row key={i} line={line} />
      ))}
    </pre>
  );
}
