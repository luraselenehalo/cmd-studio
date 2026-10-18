// What styles.css does that a jsdom test can see. jsdom has no layout, but it applies the
// stylesheet's rules with their selectors and inherits `font-family`, so which font an element
// gets is testable (F1 of the phase 3 VM check). What needs a layout (F2: pages that fit and
// scroll at Windows' Text size 150 %) is checked by the controller in the browser pane; the
// declarations that carry it are pinned here so a later edit cannot undo them unseen. The stacked
// editor is also checked in the page itself: which elements the stylesheet lifts, and where the
// rest sits in the Tab order.
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { TerminalPreview } from "./components/TerminalPreview";
import type { PreviewResult } from "./bindings/PreviewResult";
import { I18nProvider } from "./i18n";
import { IpcProvider } from "./ipc/context";
import { createMockIpc } from "./ipc/mock";
import { PRESETS } from "./ipc/presets";
import { IpcError } from "./ipc/types";

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

  /** The `@container editor (...)` rule of the stylesheet: its condition and what is inside it. */
  function editorContainerRule(css: string): { condition: string; body: string } {
    const start = css.indexOf("@container editor (");
    expect(start, "an @container editor rule").toBeGreaterThanOrEqual(0);
    const open = css.indexOf("{", start);
    let depth = 0;
    let end = open;
    for (; end < css.length; end++) {
      if (css[end] === "{") depth++;
      if (css[end] === "}" && --depth === 0) break;
    }
    return { condition: css.slice(start + "@container editor ".length, open).trim(), body: css.slice(open + 1, end) };
  }

  it("stacks the editor, the preview on top, exactly where its form would get less than 20rem", () => {
    expect(declared(".editor-box", "container-type")).toBe("inline-size");
    const css = readFileSync(join(import.meta.dirname, "styles.css"), "utf8");
    const { condition, body } = editorContainerRule(css);
    // Side by side the preview takes its 28rem before the form gets anything, so the form is what
    // the box leaves after the preview and the gap. It is 20rem where the box is 20 + gap + 28.
    const tracks = declared(".editor", "grid-template-columns");
    expect(tracks).toBe("minmax(0, 1fr) minmax(22rem, 28rem)");
    const previewMax = Number(/minmax\(22rem, ([\d.]+)rem\)/.exec(tracks ?? "")?.[1]);
    const gap = Number(/^([\d.]+)rem$/.exec(declared(".editor", "gap") ?? "")?.[1]);
    expect(condition).toBe(`(width < ${20 + gap + previewMax}rem)`);
    // Under it: one column, the preview above the form, and not stuck to the top of the window.
    // The column's box goes (`display: contents`, which is also what ends its sticking), its
    // heading, preview and error notice are lifted with `order`, and its sample fields stay in
    // the page's order, so they come after the form on the screen as they do for Tab.
    expect(body).toMatch(/\.editor\s*\{\s*grid-template-columns: minmax\(0, 1fr\);\s*\}/);
    expect(body).toMatch(/\.editor \.preview-column\s*\{\s*display: contents;\s*\}/);
    expect(body).toMatch(/\.editor \.preview-top\s*\{\s*order: -1;\s*\}/);
    expect(body.match(/\border:/g), "one lifted group, nothing else is reordered").toHaveLength(1);
    expect(body).not.toMatch(/position:/);
    // Outside the rule the column keeps its box and its sticking: `display: contents` there would
    // take the preview's column out of the wide layout, where the two columns sit side by side.
    expect(declared(".editor .preview-column", "position"), "the lookup sees the column's own rule").toBe("sticky");
    expect(declared(".editor .preview-column", "display")).toBeUndefined();
    // What that leaves of the windows of the spec at Text size 100 % (16 px a rem): the box is the
    // window less the sidebar, the page's side padding and a 17 px scrollbar. The 1100 px default
    // keeps two columns and a form of at least 20rem; the 900 px minimum stacks.
    const sidebar = Number(/^([\d.]+)rem /.exec(declared(".shell", "grid-template-columns") ?? "")?.[1]);
    const sidePadding = Number(/^[\d.]+rem ([\d.]+)rem /.exec(declared("main", "padding") ?? "")?.[1]);
    const box = (windowWidth: number) => (windowWidth - (sidebar + 2 * sidePadding) * 16 - 17) / 16;
    expect(box(1100) - gap - previewMax).toBeGreaterThanOrEqual(20);
    expect(box(1100)).toBeGreaterThanOrEqual(20 + gap + previewMax);
    expect(box(900)).toBeLessThan(20 + gap + previewMax);
  });

  it("keeps the editor's Tab order and its stacked order together: the sample fields come after the form in both", async () => {
    const ipc = createMockIpc({ settings: { wizardDone: true } });
    const theme = PRESETS.find((p) => p.name === "Terracotta")!;
    const request = { themeId: theme.id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    const planned = await ipc.planApply(request);
    if (planned.kind !== "ready") throw new Error("no plan");
    await ipc.apply(request, planned.plan.digest, () => {});
    // A preview that could not be drawn: its error notice belongs to the lifted group too.
    vi.spyOn(ipc, "previewRender").mockRejectedValue(new IpcError({ code: "E_PREVIEW", messageTh: "(E_PREVIEW)", messageEn: "(E_PREVIEW)", details: "" }));
    const user = userEvent.setup();
    render(
      <IpcProvider ipc={ipc}>
        <App />
      </IpcProvider>,
    );
    await user.click(await screen.findByRole("button", { name: "Customize" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    const column = (await screen.findByText("(E_PREVIEW)")).closest(".preview-column") as HTMLElement;
    // The column holds two groups, in this order of the page: what is lifted, then the sample fields.
    expect(Array.from(column.children).map((c) => c.className)).toEqual(["preview-top", "preview-sample"]);
    const [top, sample] = Array.from(column.children) as HTMLElement[];
    expect(top!.querySelector("h2")?.textContent).toBe("Preview");
    expect(top!.querySelector(".terminal")).not.toBeNull();
    expect(top!.querySelector(".notice.error")).not.toBeNull();
    expect(sample!.querySelectorAll("input")).toHaveLength(3);
    expect(top!.querySelector("input")).toBeNull();
    // The form is before the column in the page (the Tab order), and the lifted group is the
    // only one the stacked layout moves in front of it.
    const form = column.parentElement!.firstElementChild!;
    expect(form).not.toBe(column);
    expect(form.compareDocumentPosition(sample!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("never breaks a button's label in the middle of a word, whatever the box around it wraps (F2)", () => {
    // Inherited from a card, a notice or a list, `anywhere` would break a squeezed label mid-word.
    for (const selector of ["button", ".button"]) {
      expect(declared(selector, "overflow-wrap"), selector).toBe("normal");
    }
  });
});
