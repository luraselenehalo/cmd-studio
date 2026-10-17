// What styles.css does that a jsdom test can see. jsdom has no layout, but it applies the
// stylesheet's rules with their selectors and inherits `font-family`, so which font an element
// gets is testable (F1 of the phase 3 VM check). What needs a layout (F2: pages that fit and
// scroll at Windows' Text size 150 %) is checked by the controller in the browser pane; the
// declarations that carry it are pinned here so a later edit cannot undo them unseen.
import { render, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { TerminalPreview } from "./components/TerminalPreview";
import type { PreviewResult } from "./bindings/PreviewResult";
import { I18nProvider } from "./i18n";
import { PRESETS } from "./ipc/presets";

let sheet: HTMLStyleElement;

beforeAll(() => {
  sheet = document.createElement("style");
  sheet.textContent = readFileSync(join(import.meta.dirname, "styles.css"), "utf8");
  document.head.append(sheet);
});

afterAll(() => {
  sheet.remove();
});

const fontOf = (el: Element) => window.getComputedStyle(el).fontFamily;

describe("the Thai UI's font (F1)", () => {
  /** The preview as the Thai UI shows it: inside the shell's `<div lang="th">`, a Thai user name
   * in the welcome box, and Thai text of the UI beside it. */
  function thaiPreview() {
    const theme = structuredClone(PRESETS.find((p) => p.name === "Nord")!);
    const fg = theme.colors.foreground;
    const result: PreviewResult = {
      banner: [
        [{ text: "│ ", fg, bg: null, bold: false }],
        [{ text: "✻ Welcome back, ผู้ใช้!", fg, bg: null, bold: true }],
      ],
      prompt: [[{ text: "C:\\Users\\ผู้ใช้>", fg, bg: null, bold: false }]],
    };
    render(
      <I18nProvider language="th" setLanguage={() => {}}>
        <div lang="th">
          <p>ข้อความของแอป</p>
          <TerminalPreview theme={theme} result={result} />
        </div>
      </I18nProvider>,
    );
    const preview = screen.getByRole("img");
    const pre = preview.querySelector("pre") as HTMLElement;
    return { theme, preview, pre, uiText: screen.getByText("ข้อความของแอป") };
  }

  it("gives the Thai UI's text the Thai font and the preview's text the preview's font, spans and glyph boxes included", () => {
    const { theme, pre, uiText } = thaiPreview();
    expect(fontOf(uiText)).toMatch(/^"Leelawadee UI"/);
    expect(fontOf(pre)).toContain(`"${theme.terminal.font}"`);
    expect(fontOf(pre)).toContain("monospace");
    // Every element inside the text layer, down to the boxes of single glyphs, draws in the font
    // the `<pre>` sets: a proportional font would make the welcome box's right edge ragged.
    const inside = Array.from(pre.querySelectorAll("*"));
    expect(inside.some((el) => el.classList.contains("cell"))).toBe(true);
    for (const el of inside) expect(fontOf(el), el.outerHTML.slice(0, 60)).toBe(fontOf(pre));
  });

  it("keeps the preview's font on a part of it that carries lang=\"th\"", () => {
    // A mark a later change may add so a screen reader reads a Thai user name as Thai.
    const { preview, pre } = thaiPreview();
    const thai = Array.from(preview.querySelectorAll("span")).find((s) => s.textContent?.includes("ผู้ใช้"));
    expect(thai).toBeDefined();
    thai?.setAttribute("lang", "th");
    expect(fontOf(thai as Element)).toBe(fontOf(pre));
  });
});

describe("pages that fit and scroll at Windows' Text size 150 % (F2)", () => {
  /** The rules of the stylesheet whose selector list has exactly this selector. */
  function rulesFor(selector: string): CSSStyleRule[] {
    const rules = Array.from((sheet.sheet as CSSStyleSheet).cssRules).filter((r): r is CSSStyleRule => r instanceof CSSStyleRule);
    return rules.filter((r) => r.selectorText.split(",").some((s) => s.trim() === selector));
  }
  const declared = (selector: string, property: string) =>
    rulesFor(selector)
      .map((r) => r.style.getPropertyValue(property))
      .filter((v) => v !== "")
      .at(-1);

  it("lets the main column be as narrow as the window, so a wide preview scrolls inside its frame instead of widening the page", () => {
    // A `1fr` track does not shrink below its content: the reason Home and Help's report ran off
    // the right edge.
    expect(declared(".shell", "grid-template-columns")).toBe("13rem minmax(0, 1fr)");
    expect(declared(".terminal-text", "overflow-x")).toBe("auto");
    expect(declared(".terminal-text", "white-space")).toBe("pre");
  });

  it("wraps text that cannot break by itself, a path or a command line, inside its own box", () => {
    for (const selector of ["pre", "code", ".card", ".theme-card", ".notice", ".check-list li", ".plain-list li"]) {
      expect(declared(selector, "overflow-wrap"), selector).toBe("anywhere");
    }
    expect(declared("pre", "white-space")).toBe("pre-wrap");
    // The preview's text layer is the one place that does not wrap: a terminal's line is kept.
    expect(declared(".terminal-text", "overflow-wrap")).toBe("normal");
  });

  it("gives no page a height of its own or hides what overflows it: the page scrolls down", () => {
    // Control: the lookup does see a rule written as a selector list.
    expect(declared("body", "min-height")).toBe("100%");
    for (const selector of ["html", "body", "#root", ".shell", "main", ".wizard", ".sidebar", ".editor", ".editor-box"]) {
      for (const property of ["height", "max-height", "overflow", "overflow-y"]) {
        expect(declared(selector, property), `${selector} ${property}`).toBeUndefined();
      }
    }
  });

  it("stacks the editor's form and preview where less than 36rem is left, measured on the box that holds only them", () => {
    expect(declared(".editor-box", "container-type")).toBe("inline-size");
    const css = readFileSync(join(import.meta.dirname, "styles.css"), "utf8");
    expect(css).toMatch(/@container editor \(max-width: 36rem\)\s*\{\s*\.editor\s*\{\s*grid-template-columns: minmax\(0, 1fr\);/);
    // The editor's own track list is unchanged above that: the form, and a 22 to 28rem preview.
    expect(declared(".editor", "grid-template-columns")).toBe("minmax(0, 1fr) minmax(22rem, 28rem)");
  });
});
