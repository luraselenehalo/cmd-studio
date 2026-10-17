import type { ReactNode } from "react";
import type { ThemeSummary } from "../bindings/ThemeSummary";
import type { Theme } from "../bindings/Theme";
import { useT } from "../i18n";

/** A theme in the library or the wizard: name, badges, and its eight normal ANSI colors as
 * swatches on its background. The colors come with the summary (one `themes_list` draws every
 * card, with no `theme_get` per card). */
export function ThemeCard({
  summary,
  theme,
  selected = false,
  onSelect,
  children,
}: {
  summary: ThemeSummary;
  /** The full theme, when the caller has it; its colors then draw the swatches. */
  theme?: Theme | null;
  selected?: boolean;
  onSelect?: () => void;
  children?: ReactNode;
}) {
  const t = useT();
  const colors = theme?.colors ?? summary.colors;
  return (
    <div className={`theme-card${selected ? " selected" : ""}`}>
      {onSelect ? (
        <button type="button" onClick={onSelect} aria-pressed={selected} style={{ width: "100%", textAlign: "left" }}>
          <strong>{summary.name}</strong>
        </button>
      ) : (
        <strong>{summary.name}</strong>
      )}
      {summary.inUse && <span className="badge in-use">{t("themes.inUse")}</span>}
      {summary.source === "preset" && <span className="badge">{t("themes.preset")}</span>}
      {summary.source === "draft" && <span className="badge">{t("themes.draft")}</span>}
      {summary.source === "mine" && summary.hasDraft && <span className="badge">{t("themes.hasDraft")}</span>}
      <div className="badge">{t(summary.tone === "dark" ? "themes.filter.dark" : "themes.filter.light")}</div>
      <div className="swatches" aria-hidden="true" style={{ background: colors.background, padding: "0.3rem", borderRadius: "0.3rem" }}>
        {colors.ansi.slice(0, 8).map((c, i) => (
          <span key={i} style={{ background: c }} />
        ))}
      </div>
      {children}
    </div>
  );
}
