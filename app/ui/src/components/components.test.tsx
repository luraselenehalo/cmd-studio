// The shared components on their own, against the mock IPC: what the screens' tests
// (Themes.test.tsx) do not pin. The dialog keeps every key typed into it and keeps focus
// inside, and Escape does nothing while it has no onClose; notes show their details; one
// predicate tells an AppError; the preview fades only its background; the progress keeps one
// status region and the rollback has no step numbers; the plan lists the other add-on's
// commands it turns off; Try again asks about that add-on again; a plan that changed is shown
// again, never applied; what a rollback left is listed under its header; focus follows the
// stages; "busy" brackets the apply; "Copy details" says whether it copied.
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { Theme } from "../bindings/Theme";
import type { ThemeSummary } from "../bindings/ThemeSummary";
import { I18nProvider, type Language } from "../i18n";
import { IpcProvider } from "../ipc/context";
import { createMockIpc } from "../ipc/mock";
import { PRESETS } from "../ipc/presets";
import type { Ipc } from "../ipc/types";
import { AppStateContext, DEFAULT_SETTINGS, useDraftProblems, type AppState, type LastApply } from "../state";
import { AfterApplyBar } from "./AfterApplyBar";
import { ApplyFlow, ProgressView } from "./ApplyFlow";
import { Dialog } from "./Dialog";
import { ErrorBoundary } from "./ErrorBoundary";
import { ErrorNotice, NotesList, UnrestoredList } from "./Notices";
import { TerminalPreview } from "./TerminalPreview";
import { ThemeCard } from "./ThemeCard";

function mount(children: ReactNode, { ipc = createMockIpc(), language = "en" }: { ipc?: Ipc; language?: Language } = {}) {
  const wrap = (inner: ReactNode) => (
    <I18nProvider language={language} setLanguage={() => {}}>
      <IpcProvider ipc={ipc}>{inner}</IpcProvider>
    </I18nProvider>
  );
  const view = render(wrap(children));
  return { ...view, rerender: (inner: ReactNode) => view.rerender(wrap(inner)) };
}

function presetId(name: string): string {
  const found = PRESETS.find((p) => p.name === name);
  if (!found) throw new Error(`no preset ${name}`);
  return found.id;
}

const REQUEST: ApplyRequest = {
  themeId: presetId("Nord"),
  target: "all",
  setWtDefaultProfile: true,
  replaceOtherClink: false,
};

const OTHER_CLINK = '"C:\\clink\\clink_x64.exe" inject --autorun';

/** A rename dialog whose parent re-renders on every key, with a new `onClose` each time. */
function Rename({ startOpen = true }: { startOpen?: boolean }) {
  const [open, setOpen] = useState(startOpen);
  const [name, setName] = useState("My Terracotta");
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Rename
      </button>
      {open && (
        <Dialog title="Rename the theme" onClose={() => setOpen(false)}>
          <label>
            New name
            <input value={name} onChange={(e) => setName(e.target.value)} />
          </label>
        </Dialog>
      )}
    </>
  );
}

describe("Dialog", () => {
  it("keeps both characters typed into a field while its parent re-renders", async () => {
    const user = userEvent.setup();
    mount(<Rename />);
    const field = screen.getByLabelText("New name") as HTMLInputElement;
    await user.type(field, "XY");
    expect(field.value).toBe("My TerracottaXY");
    expect(document.activeElement).toBe(field);
  });

  it("keeps Tab and Shift+Tab inside, closes on Escape and gives focus back", async () => {
    const user = userEvent.setup();
    mount(<Rename startOpen={false} />);
    const opener = screen.getByRole("button", { name: "Rename" });
    await user.click(opener);
    const dialog = screen.getByRole("dialog", { name: "Rename the theme" });
    const field = within(dialog).getByLabelText("New name");
    const close = within(dialog).getByRole("button", { name: "Close" });
    // The dialog itself has focus first; Shift+Tab from it goes to its last control.
    expect(document.activeElement).toBe(dialog);
    await user.tab({ shift: true });
    expect(document.activeElement).toBe(close);
    await user.tab();
    expect(document.activeElement).toBe(field);
    await user.tab({ shift: true });
    expect(document.activeElement).toBe(close);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });

  it("does nothing on Escape while it has no onClose, and closes once it has one again", async () => {
    // As the apply dialog does while an apply runs (ruling R7): no onClose, so no way out.
    function Busy() {
      const [busy, setBusy] = useState(true);
      const [open, setOpen] = useState(true);
      if (!open) return null;
      return (
        <Dialog title="Applying" onClose={busy ? undefined : () => setOpen(false)}>
          <button type="button" onClick={() => setBusy(false)}>
            Finish
          </button>
        </Dialog>
      );
    }
    const user = userEvent.setup();
    mount(<Busy />);
    const dialog = screen.getByRole("dialog", { name: "Applying" });
    expect(within(dialog).queryByRole("button", { name: "Close" })).toBeNull();
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog", { name: "Applying" })).toBe(dialog);
    await user.click(within(dialog).getByRole("button", { name: "Finish" }));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("TerminalPreview", () => {
  it("fades only the window's background, never the text, as Windows Terminal's opacity does", () => {
    const theme = structuredClone(PRESETS.find((p) => p.name === "Nord")!);
    theme.terminal.opacity = 30;
    const result: PreviewResult = {
      banner: [],
      prompt: [[{ text: "C:\\Users\\tester>", fg: theme.colors.foreground, bg: null, bold: false }]],
    };
    mount(<TerminalPreview theme={theme} result={result} />);
    const preview = screen.getByRole("img", { name: "Preview of the cmd window" });
    const text = screen.getByText("C:\\Users\\tester>");
    // Nothing between the text and the preview's frame, the frame included, fades it.
    for (let el: HTMLElement | null = text; el !== null; el = el === preview ? null : el.parentElement) {
      expect(Number(window.getComputedStyle(el).opacity || "1"), el.tagName).toBeGreaterThanOrEqual(1);
    }
    // The background is a layer of its own, at the theme's opacity.
    const layer = preview.querySelector(".terminal-bg") as HTMLElement;
    expect(layer.style.opacity).toBe("0.3");
    expect(layer.style.backgroundColor).not.toBe("");
    expect(layer.contains(text)).toBe(false);
  });

  /** A result with this text on its prompt row, in one span of the theme's foreground color. */
  function withText(...texts: string[]): { theme: Theme; result: PreviewResult } {
    const theme = structuredClone(PRESETS.find((p) => p.name === "Nord")!);
    const prompt = texts.map((text) => [{ text, fg: theme.colors.foreground, bg: null, bold: false }]);
    return { theme, result: { banner: [], prompt } };
  }

  it("draws a glyph the font may lack in a box exactly one cell wide, and changes no text (V2)", () => {
    // The welcome box's rows as the core draws them: the icon U+273B is one cell for the core,
    // for conhost and for Windows Terminal, but Cascadia Mono has none, so a fallback font draws
    // it wider than 1ch and pushes the frame's right edge out.
    const { theme, result } = withText(
      "╭──────────────────────────────────────────╮",
      "│ ✻ Welcome back, tester!                  │",
      "╰──────────────────────────────────────────╯",
      " Tips: help for commands · cls to clear · ↑ for history",
      "✗ 1 ❯ npm test \u{e0b0}",
    );
    mount(<TerminalPreview theme={theme} result={result} />);
    const preview = screen.getByRole("img", { name: "Preview of the cmd window" });
    const boxes = Array.from(preview.querySelectorAll<HTMLElement>(".cell"));
    // Each of the glyphs is in a box of its own, one cell wide.
    expect(boxes.map((b) => b.textContent)).toEqual(["✻", "↑", "✗", "❯", "\u{e0b0}"]);
    for (const box of boxes) expect(box.style.width, box.textContent ?? "").toBe("1ch");
    // The text is the core's, untouched: what is copied and what a screen reader would find in
    // the preview's text is the same as without the boxes.
    const pre = preview.querySelector("pre") as HTMLElement;
    expect(pre.textContent).toBe(`${result.prompt.map((line) => line.map((span) => span.text).join("")).join("\n")}\n`);
    // The preview is still one image to a screen reader, named by its label.
    expect(preview.getAttribute("aria-label")).toBe("Preview of the cmd window");
  });

  it("leaves what a monospace font draws in one cell on its own, and what is two cells wide", () => {
    const { theme, result } = withText(
      "C:\\Users\\tester> dir ░▒▓█ ┌─┐ │ ╰╯ Ñ é · \u{a0}",
      "中文 한글 ＡＢ 📁 ✅ \u{2764}\u{fe0f}",
    );
    mount(<TerminalPreview theme={theme} result={result} />);
    const preview = screen.getByRole("img", { name: "Preview of the cmd window" });
    // ASCII, Latin, the box-drawing and block blocks are in Cascadia Mono, and a wide character
    // takes the two cells its font gives it: none of them is put in a one-cell box.
    expect(preview.querySelectorAll(".cell")).toHaveLength(0);
  });

  it("keeps a letter together with its marks in one box, as the core counts them (Thai is 3 cells in ผู้ใช้)", () => {
    // ผู้ใช้ : the core counts ผ, ใ and ช as one cell each and the vowel and tone marks as none.
    const { theme, result } = withText("C:\\Users\\ผู้ใช้>");
    mount(<TerminalPreview theme={theme} result={result} />);
    const boxes = Array.from(screen.getByRole("img", { name: "Preview of the cmd window" }).querySelectorAll<HTMLElement>(".cell"));
    expect(boxes.map((b) => b.textContent)).toEqual(["ผู้", "ใ", "ช้"]);
    for (const box of boxes) expect(box.style.width).toBe("1ch");
  });
});

describe("Notices", () => {
  it("shows a note's details under its message, in Thai as in English", () => {
    mount(
      <NotesList
        notes={[
          {
            code: "E_SNAPSHOT_READ",
            messageTh: "อ่านจุดย้อนกลับไม่ได้",
            messageEn: "A restore point could not be read",
            details: "restore point 1791694709000-abcd1234: manifest.json is not valid JSON",
          },
          { code: "E_TXN_INTERRUPTED", messageTh: "คืนค่าที่ค้างไว้แล้ว", messageEn: "An unfinished apply was put back", details: "" },
        ]}
      />,
      { language: "th" },
    );
    const first = screen.getByText("อ่านจุดย้อนกลับไม่ได้").closest("li") as HTMLElement;
    expect(within(first).getByText(/manifest\.json is not valid JSON/)).toBeTruthy();
    expect(within(first).getByText("ดูรายละเอียด")).toBeTruthy();
    // A note with no details has no empty toggle.
    const second = screen.getByText("คืนค่าที่ค้างไว้แล้ว").closest("li") as HTMLElement;
    expect(within(second).queryByText("ดูรายละเอียด")).toBeNull();
  });

  it("marks the core's English text inside the Thai UI with lang=\"en\" (Task 1 carry)", () => {
    /** The language of the nearest mark above `el`, the mark of the text itself included. */
    const langOf = (el: HTMLElement) => el.closest("[lang]")?.getAttribute("lang");
    const notes = [
      {
        code: "E_SNAPSHOT_READ",
        messageTh: "อ่านจุดย้อนกลับไม่ได้",
        messageEn: "A restore point could not be read",
        details: "restore point 1791694709000-abcd1234: manifest.json is not valid JSON",
      },
      // No Thai text for this note: the English one is shown, and is marked so.
      { code: "E_TXN_INTERRUPTED", messageTh: "", messageEn: "An unfinished apply was put back", details: "" },
    ];
    const th = mount(<NotesList notes={notes} />, { language: "th" });
    expect(langOf(screen.getByText("อ่านจุดย้อนกลับไม่ได้"))).toBe("th");
    expect(langOf(screen.getByText(/manifest\.json is not valid JSON/))).toBe("en");
    expect(langOf(screen.getByText("An unfinished apply was put back"))).toBe("en");
    th.unmount();
    mount(<NotesList notes={notes} />, { language: "en" });
    expect(langOf(screen.getByText("A restore point could not be read"))).toBe("en");
    expect(langOf(screen.getByText(/manifest\.json is not valid JSON/))).toBe("en");
  });

  it("marks an error's details lang=\"en\" in the Thai UI, and its text by the language shown", () => {
    const langOf = (el: HTMLElement) => el.closest("[lang]")?.getAttribute("lang");
    const error = {
      code: "E_WT_PARSE",
      messageTh: "อ่านการตั้งค่า Windows Terminal ไม่ได้",
      messageEn: "Windows Terminal's settings could not be read",
      details: "expected `,` or `}` at line 3 column 7",
    };
    const th = mount(<ErrorNotice error={error} />, { language: "th" });
    expect(langOf(screen.getByText("อ่านการตั้งค่า Windows Terminal ไม่ได้"))).toBe("th");
    expect(langOf(screen.getByText(/expected `,` or `}` at line 3/))).toBe("en");
    th.unmount();
    // Without a Thai text the English one is shown in the Thai UI, and says so.
    const bare = mount(<ErrorNotice error={{ ...error, messageTh: "" }} />, { language: "th" });
    expect(langOf(screen.getByText("Windows Terminal's settings could not be read"))).toBe("en");
    bare.unmount();
    mount(<ErrorNotice error={error} />, { language: "en" });
    expect(langOf(screen.getByText("Windows Terminal's settings could not be read"))).toBe("en");
    expect(langOf(screen.getByText(/expected `,` or `}` at line 3/))).toBe("en");
  });

  it("marks the English fallback lang=\"en\" at the other places that pick a text too (m3)", () => {
    // One of them, the steps a rollback could not put back: the same rule as above, through
    // pickedLanguage, at every `pick()` of the UI (AfterApplyBar, Help and Themes as well).
    const langOf = (el: HTMLElement) => el.closest("[lang]")?.getAttribute("lang");
    const steps = [
      { opIndex: 1, target: "settings.json", code: "E_WT_WRITE", message: "m", fix: "Delete the scheme by hand", fixTh: "ลบชุดสีเอง" },
      { opIndex: 2, target: "AutoRun", code: "E_AUTORUN_WRITE", message: "m", fix: "Take out our part by hand", fixTh: "" },
    ];
    mount(<UnrestoredList steps={steps} />, { language: "th" });
    expect(langOf(screen.getByText("ลบชุดสีเอง"))).toBe("th");
    expect(langOf(screen.getByText("Take out our part by hand"))).toBe("en");
  });

  it("tells an AppError by all four fields, so a partial one is an internal problem, not a crash", () => {
    mount(<ErrorNotice error={{ code: "E_WT_PARSE", messageEn: "Windows Terminal's settings could not be read" }} />, {
      language: "th",
    });
    expect(screen.getByText(/รหัส E_INTERNAL/)).toBeTruthy();
    expect(screen.getByText(/CMD Studio เองมีปัญหา/)).toBeTruthy();
    // What was thrown is kept in the details.
    expect(screen.getByText(/"code":"E_WT_PARSE"/)).toBeTruthy();
  });
});

describe("ProgressView", () => {
  it("counts an apply's steps, the state.json write included", () => {
    const { container } = mount(<ProgressView progress={{ phase: "applying", step: 3, total: 7, kind: "writeGenerated" }} />);
    expect(screen.getByText("Applying (3 of 7)")).toBeTruthy();
    const bar = container.querySelector("progress") as HTMLProgressElement;
    expect(bar.getAttribute("value")).toBe("3");
    expect(bar.getAttribute("max")).toBe("7");
  });

  it("draws putting things back without step numbers and with a bar that claims no fraction", () => {
    const progress = { phase: "rollingBack", step: 2, total: 7, kind: null } as const;
    const en = mount(<ProgressView progress={progress} />);
    expect(screen.getByText("Putting things back…")).toBeTruthy();
    expect(screen.queryByText(/\d/)).toBeNull();
    expect(en.container.querySelector("progress")?.hasAttribute("value")).toBe(false);
    en.unmount();
    const th = mount(<ProgressView progress={progress} />, { language: "th" });
    expect(screen.getByText("กำลังคืนค่าเดิม…")).toBeTruthy();
    expect(screen.queryByText(/\d/)).toBeNull();
    expect(th.container.querySelector("progress")?.hasAttribute("value")).toBe(false);
  });

  it("keeps one status region from before the first event on, and names the bar by its text", () => {
    const view = mount(<ProgressView progress={null} />);
    const region = screen.getByRole("status");
    expect(region.textContent).toContain("Applying…");
    view.rerender(<ProgressView progress={{ phase: "backingUp", step: 1, total: 7, kind: null }} />);
    expect(screen.getByRole("status")).toBe(region);
    const text = screen.getByText("Saving a restore point (1 of 7)");
    const bar = region.querySelector("progress") as HTMLProgressElement;
    expect(text.id).not.toBe("");
    expect(bar.getAttribute("aria-labelledby")).toBe(text.id);
    expect(bar.hasAttribute("aria-label")).toBe(false);
  });
});

describe("ThemeCard", () => {
  it("draws the eight swatches from the summary's own colors", () => {
    const preset = PRESETS.find((p) => p.name === "Nord")!;
    const summary: ThemeSummary = {
      id: preset.id,
      name: preset.name,
      tone: preset.tone,
      layout: preset.prompt.layout,
      colors: preset.colors,
      source: "preset",
      inUse: true,
      hasDraft: false,
    };
    const { container } = mount(<ThemeCard summary={summary} />);
    expect(container.querySelectorAll(".swatches span")).toHaveLength(8);
    expect(screen.getByText("In use")).toBeTruthy();
    expect(screen.getByText("Built in")).toBeTruthy();
  });
});

describe("ApplyFlow", () => {
  it("lists in the plan the other add-on's commands it turns off, then applies", async () => {
    const ipc = createMockIpc({ otherClink: [OTHER_CLINK] });
    const onApplied = vi.fn();
    const user = userEvent.setup();
    mount(<ApplyFlow request={REQUEST} onApplied={onApplied} />, { ipc });
    await screen.findByRole("heading", { name: "Another cmd add-on is already set up" });
    await user.click(screen.getByRole("button", { name: "Use CMD Studio's instead" }));
    await screen.findByRole("button", { name: "Apply" });
    const intro = screen.getByText("Turning off the other cmd add-on takes these commands out of cmd's start-up:");
    expect(within(intro.parentElement as HTMLElement).getByText(OTHER_CLINK)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await screen.findByText(/Theme "Nord" applied/);
    expect(onApplied).toHaveBeenCalledTimes(1);
    expect(onApplied.mock.calls[0]?.[0]).toMatchObject({ status: "committed", themeName: "Nord" });
    // The request that was applied comes with it: the user's yes to the other add-on included.
    expect(onApplied.mock.calls[0]?.[1]).toEqual({ ...REQUEST, replaceOtherClink: true });
  });

  it("asks about the other add-on again when a failed apply is tried again", async () => {
    const ipc = createMockIpc({ otherClink: [OTHER_CLINK], failApplyWith: "E_VERIFY_OMP" });
    const onApplied = vi.fn();
    const user = userEvent.setup();
    mount(<ApplyFlow request={REQUEST} onApplied={onApplied} />, { ipc });
    await user.click(await screen.findByRole("button", { name: "Use CMD Studio's instead" }));
    await user.click(await screen.findByRole("button", { name: "Apply" }));
    await screen.findByText("The theme could not be applied. Everything was put back as it was.");
    // A rolled-back outcome still carries its snapshotId; it is not an apply anyone may undo.
    expect(onApplied).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await screen.findByRole("heading", { name: "Another cmd add-on is already set up" });
  });

  it("asks about the other add-on again when a plan that failed is tried again", async () => {
    const ipc = createMockIpc({ otherClink: [OTHER_CLINK] });
    const planApply = ipc.planApply.bind(ipc);
    let failNext = false;
    ipc.planApply = async (request) => {
      if (request.replaceOtherClink && !failNext) {
        failNext = true;
        throw new Error("the plan could not be made");
      }
      return planApply(request);
    };
    const user = userEvent.setup();
    mount(<ApplyFlow request={REQUEST} />, { ipc });
    await user.click(await screen.findByRole("button", { name: "Use CMD Studio's instead" }));
    await screen.findByText(/Code E_INTERNAL/);
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await screen.findByRole("heading", { name: "Another cmd add-on is already set up" });
  });

  it("says the details were copied, and says so when they could not be", async () => {
    const ipc = createMockIpc({ failApplyWith: "E_VERIFY_OMP" });
    const user = userEvent.setup();
    mount(<ApplyFlow request={REQUEST} />, { ipc });
    await user.click(await screen.findByRole("button", { name: "Apply" }));
    const copy = await screen.findByRole("button", { name: "Copy details for a problem report" });
    await user.click(copy);
    expect(await screen.findByText("Copied")).toBeTruthy();
    expect(await navigator.clipboard.readText()).toMatch(/^CMD Studio diagnostics/);
    ipc.diagnosticsCopy = async () => {
      throw new Error("the log could not be read");
    };
    await user.click(copy);
    expect(await screen.findByText("The details could not be copied")).toBeTruthy();
    expect(screen.queryByText("Copied")).toBeNull();
  });

  it("says Copied in a status region that is there before the copy, not mounted with its text (item 10)", async () => {
    const ipc = createMockIpc({ failApplyWith: "E_VERIFY_OMP" });
    const user = userEvent.setup();
    mount(<ApplyFlow request={REQUEST} />, { ipc });
    await user.click(await screen.findByRole("button", { name: "Apply" }));
    const copy = await screen.findByRole("button", { name: "Copy details for a problem report" });
    const region = screen.getByRole("status");
    expect(region.textContent).toBe("");
    await user.click(copy);
    await waitFor(() => expect(region.textContent).toBe("Copied"));
    expect(screen.getByRole("status")).toBe(region);
  });

  it("tries again with the request it was mounted with, not a later prop", async () => {
    const ipc = createMockIpc({ failApplyWith: "E_VERIFY_OMP" });
    const planApply = ipc.planApply.bind(ipc);
    const planned: string[] = [];
    ipc.planApply = (request) => {
      planned.push(request.themeId);
      return planApply(request);
    };
    const user = userEvent.setup();
    const view = mount(<ApplyFlow request={REQUEST} />, { ipc });
    await user.click(await screen.findByRole("button", { name: "Apply" }));
    await screen.findByText("The theme could not be applied. Everything was put back as it was.");
    view.rerender(<ApplyFlow request={{ ...REQUEST, themeId: presetId("Paper") }} />);
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await screen.findByRole("button", { name: "Apply" });
    expect(planned).toEqual([REQUEST.themeId, REQUEST.themeId]);
  });

  it("shows a plan that changed while the user read it, and applies nothing until Apply again", async () => {
    const ipc = createMockIpc();
    const apply = ipc.apply.bind(ipc);
    let stale = true;
    ipc.apply = (request, digest, onProgress) => {
      // The first apply carries the digest of a plan from before: the machine changed since.
      const sent = stale ? "a digest from before" : digest;
      stale = false;
      return apply(request, sent, onProgress);
    };
    const onApplied = vi.fn();
    const user = userEvent.setup();
    mount(<ApplyFlow request={REQUEST} onApplied={onApplied} />, { ipc });
    await user.click(await screen.findByRole("button", { name: "Apply" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toBe("This computer changed while you were looking. Here is the new plan; check it again.");
    expect(screen.getByText("Update the prompt and the welcome box")).toBeTruthy();
    expect(ipc.state.calls.filter((c) => c === "apply")).toHaveLength(1);
    expect(ipc.state.applied).toBeNull();
    // No outcome either way (the plan's own step "Remember what was applied" aside).
    expect(screen.queryByText(/Theme "Nord" applied|could not be applied/)).toBeNull();
    expect(onApplied).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await screen.findByText(/Theme "Nord" applied/);
    expect(ipc.state.calls.filter((c) => c === "apply")).toHaveLength(2);
    expect(onApplied).toHaveBeenCalledTimes(1);
  });

  it("lists right under its header what a rollback could not put back, with the fix in the UI's language", async () => {
    const texts = {
      en: {
        apply: "Apply",
        header: "The theme could not be applied, and not everything could be put back. Fix these by hand:",
        fix: "Close Windows Terminal, then go back to the latest restore point in Help",
      },
      th: {
        apply: "ใช้ธีม",
        header: "ใช้ธีมไม่สำเร็จ และคืนค่าเดิมได้ไม่ครบ ต้องแก้รายการต่อไปนี้เอง:",
        fix: "ปิด Windows Terminal แล้วกลับไปที่จุดย้อนกลับล่าสุดในหน้าช่วยเหลือ",
      },
    };
    for (const language of ["en", "th"] as const) {
      const ipc = createMockIpc();
      ipc.apply = async () => ({
        status: "rollbackIncomplete",
        themeName: "Nord",
        snapshotId: "1791694709000-abcd1234",
        error: { code: "E_ROLLBACK_INCOMPLETE", messageTh: "(E_ROLLBACK_INCOMPLETE)", messageEn: "(E_ROLLBACK_INCOMPLETE)", details: "step 5 was not put back" },
        unrestored: [
          {
            opIndex: 4,
            target: "Windows Terminal's settings",
            code: "E_WT_WRITE",
            message: "access is denied",
            fix: texts.en.fix,
            fixTh: texts.th.fix,
          },
        ],
        notes: [],
        plan: null,
      });
      const user = userEvent.setup();
      const view = mount(<ApplyFlow request={REQUEST} />, { ipc, language });
      await user.click(await screen.findByRole("button", { name: texts[language].apply }));
      const header = await screen.findByText(texts[language].header);
      // "…by hand:" points at the list: it comes right after the header.
      const list = header.nextElementSibling as HTMLElement;
      expect(within(list).getByText("Windows Terminal's settings")).toBeTruthy();
      expect(within(list).getByText("E_WT_WRITE")).toBeTruthy();
      expect(within(list).getByText(texts[language].fix)).toBeTruthy();
      view.unmount();
    }
  });

  it("moves focus to each new stage, and back to the opener when its dialog closes", async () => {
    function Host() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            Apply Nord
          </button>
          {open && (
            <Dialog title="What will change" onClose={() => setOpen(false)}>
              <ApplyFlow request={REQUEST} titled={false} />
            </Dialog>
          )}
        </>
      );
    }
    const ipc = createMockIpc({ otherClink: [OTHER_CLINK] });
    const user = userEvent.setup();
    mount(<Host />, { ipc });
    const opener = screen.getByRole("button", { name: "Apply Nord" });
    await user.click(opener);
    const dialog = screen.getByRole("dialog", { name: "What will change" });
    const replace = await within(dialog).findByRole("button", { name: "Use CMD Studio's instead" });
    await user.click(replace);
    // The pressed button is gone; focus is on the new stage, not dropped to the page.
    const apply = await within(dialog).findByRole("button", { name: "Apply" });
    expect(document.activeElement).not.toBe(document.body);
    expect(document.activeElement).not.toBe(dialog);
    expect(document.activeElement?.contains(apply)).toBe(true);
    await user.click(apply);
    const done = await within(dialog).findByText(/Theme "Nord" applied/);
    expect(document.activeElement?.contains(done)).toBe(true);
    await user.keyboard("{Escape}");
    expect(document.activeElement).toBe(opener);
  });

  it("says it is busy from just before the apply until the apply has ended", async () => {
    const ipc = createMockIpc();
    const events: string[] = [];
    const apply = ipc.apply.bind(ipc);
    ipc.apply = async (request, digest, onProgress) => {
      events.push("apply starts");
      try {
        return await apply(request, digest, onProgress);
      } finally {
        events.push("apply ends");
      }
    };
    const user = userEvent.setup();
    mount(<ApplyFlow request={REQUEST} onBusy={(busy) => events.push(`busy ${busy}`)} />, { ipc });
    await user.click(await screen.findByRole("button", { name: "Apply" }));
    await screen.findByText(/Theme "Nord" applied/);
    expect(events).toEqual(["busy true", "apply starts", "apply ends", "busy false"]);
  });
});

describe("ErrorBoundary", () => {
  it("shows the E_INTERNAL notice in English and Thai with a way out, never a blank window, when a component inside it throws (item 8, round 2 fix 3)", async () => {
    const thrown = new Error("Cannot read properties of undefined (reading 'themes')");
    function Broken(): ReactNode {
      throw thrown;
    }
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    const reload = vi.fn();
    try {
      // No provider around it: main.tsx puts it outside App, which holds the language, so the
      // language is not known there: both are shown.
      render(
        <ErrorBoundary reload={reload}>
          <Broken />
        </ErrorBoundary>,
      );
      const notice = screen.getByRole("alert");
      expect(within(notice).getByText("Something went wrong")).toBeTruthy();
      expect(within(notice).getByText("มีบางอย่างผิดพลาด")).toBeTruthy();
      expect(
        within(notice).getByText(
          "CMD Studio itself ran into a problem; the details say where. Try again, and if it stays, report it with the diagnostics",
        ),
      ).toBeTruthy();
      expect(
        within(notice).getByText(
          "CMD Studio เองมีปัญหา รายละเอียดบอกว่าตรงไหน ลองใหม่อีกครั้ง ถ้ายังเป็นอยู่ให้แจ้งปัญหาพร้อมข้อมูลวินิจฉัย",
        ),
      ).toBeTruthy();
      expect(within(notice).getByText(/Code E_INTERNAL/)).toBeTruthy();
      expect(within(notice).getByText(/รหัส E_INTERNAL/)).toBeTruthy();
      expect(within(notice).getByText("Cannot read properties of undefined (reading 'themes')")).toBeTruthy();
      // What went wrong is on the console as well, with where it happened.
      expect(logged).toHaveBeenCalledWith("CMD Studio: a screen could not be drawn", thrown, expect.any(String));
      // The way out: Reload, in both languages.
      await userEvent.setup().click(screen.getByRole("button", { name: "Reload · โหลดใหม่" }));
      expect(reload).toHaveBeenCalledTimes(1);
    } finally {
      logged.mockRestore();
    }
  });

  it("shows its children while nothing throws", () => {
    render(
      <ErrorBoundary>
        <p>The library</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("The library")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("AfterApplyBar", () => {
  it("words the bar by the target of the request that was applied, not by the settings (item 12)", async () => {
    // As phase 3's doctor fixes do: a flow with its own request, here for Windows Terminal
    // only, while the settings say every cmd window.
    function Host() {
      const [lastApply, setLastApply] = useState<LastApply | null>(null);
      const draftProblems = useDraftProblems();
      const state: AppState = {
        settings: { ...DEFAULT_SETTINGS, target: "all" },
        saveSettings: async () => {},
        lastApply,
        setLastApply,
        draftProblems,
        version: 0,
        bump: () => {},
      };
      return (
        <AppStateContext.Provider value={state}>
          <AfterApplyBar />
          <ApplyFlow
            request={{ ...REQUEST, target: "wt" }}
            onApplied={(outcome, request) => setLastApply({ outcome, target: request.target })}
          />
        </AppStateContext.Provider>
      );
    }
    const user = userEvent.setup();
    const { container } = mount(<Host />);
    await user.click(await screen.findByRole("button", { name: "Apply" }));
    await screen.findAllByText(/Theme "Nord" applied/);
    const bar = await waitFor(() => {
      const found = container.querySelector(".bar");
      expect(found).not.toBeNull();
      return found as HTMLElement;
    });
    expect(within(bar).getByText(/open a new cmd tab in Windows Terminal/)).toBeTruthy();
    expect(screen.queryByText(/open a new cmd window/)).toBeNull();
  });
});
