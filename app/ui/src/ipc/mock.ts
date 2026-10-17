// An in-memory `Ipc` for the tests and the visual check (`VITE_MOCK_IPC=1`). It never reaches
// Tauri or this computer. It is a stand-in, not a second implementation of CMD Studio: the
// palette, the readability answers and the preview it gives are fixed data taken from the
// presets and from the core's own answers, never computed (spec §5.6: no palette, contrast or
// prompt logic in TypeScript; the real answers come from Rust). It echoes back only what it is
// given: the accent (as the accent and the cursor) and colors it has no answer for.
import type { AdvancedInfo } from "../bindings/AdvancedInfo";
import type { AppError } from "../bindings/AppError";
import type { AppSettings } from "../bindings/AppSettings";
import type { ApplyOutcome } from "../bindings/ApplyOutcome";
import type { ApplyPlan } from "../bindings/ApplyPlan";
import type { ApplyProgress } from "../bindings/ApplyProgress";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { CheckReport } from "../bindings/CheckReport";
import type { Colors } from "../bindings/Colors";
import type { ContrastIssue } from "../bindings/ContrastIssue";
import type { DoctorReport } from "../bindings/DoctorReport";
import type { FixProposal } from "../bindings/FixProposal";
import type { ImportPreview } from "../bindings/ImportPreview";
import type { Library } from "../bindings/Library";
import type { Line } from "../bindings/Line";
import type { PlanOutcome } from "../bindings/PlanOutcome";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { RestoreOutcome } from "../bindings/RestoreOutcome";
import type { RestorePoint } from "../bindings/RestorePoint";
import type { RestorePoints } from "../bindings/RestorePoints";
import type { RevertOutcome } from "../bindings/RevertOutcome";
import type { SettingsLoad } from "../bindings/SettingsLoad";
import type { SystemReport } from "../bindings/SystemReport";
import type { Theme } from "../bindings/Theme";
import type { ThemeDetail } from "../bindings/ThemeDetail";
import type { ThemeSource } from "../bindings/ThemeSource";
import type { ThemeSummary } from "../bindings/ThemeSummary";
import type { Tone } from "../bindings/Tone";
import type { UndoOutcome } from "../bindings/UndoOutcome";
import { IpcError, type Ipc } from "./types";
import { PRESETS } from "./presets";

function error(code: string, details: string): IpcError {
  return new IpcError({ code, messageTh: `(${code})`, messageEn: `(${code})`, details });
}

function uuid(): string {
  const hex = () => Math.floor(Math.random() * 16).toString(16);
  const part = (n: number) => Array.from({ length: n }, hex).join("");
  const variant = ["8", "9", "a", "b"][Math.floor(Math.random() * 4)];
  return `${part(8)}-${part(4)}-4${part(3)}-${variant}${part(3)}-${part(12)}`;
}

/** A preset by name, as its file has it. */
function presetNamed(name: string): Theme {
  const found = PRESETS.find((p) => p.name === name);
  if (!found) throw new Error(`the mock has no preset ${name}`);
  return found;
}

/** Code-unit order, as Rust compares strings (UTF-8 byte order gives the same below U+E000). */
function order(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

const NAMED_SLOTS = ["background", "foreground", "cursor", "selection", "accent", "muted", "frame"] as const;

/** The same 23 color texts: how the mock finds a preset's palette among the colors it is given. */
function sameColors(a: Colors, b: Colors): boolean {
  return (
    NAMED_SLOTS.every((slot) => a[slot] === b[slot]) &&
    a.ansi.length === b.ansi.length &&
    a.ansi.every((color, i) => color === b.ansi[i])
  );
}

/**
 * The core's readability answers for the three upstream presets with known exceptions
 * (crates/core/src/presets.rs, `every_contrast_exception_is_a_known_upstream_one`): what
 * `contrast_check` says of the preset's own colors, in the core's order, and the colors
 * `contrast_fix` gives back for them, copied from the core's answers. For any other colors the
 * mock knows no issue, and "Fix automatically" gives them back as they are (which is also the
 * core's answer for the other nine presets).
 */
const KNOWN_CONTRAST: { preset: string; issues: ContrastIssue[]; fixed: Colors }[] = [
  {
    preset: "Gruvbox",
    issues: [{ slot: "ansi1", ratio: 2.6942006289764127, required: 3.0 }],
    fixed: {
      background: "#282828",
      foreground: "#ebdbb2",
      cursor: "#ebdbb2",
      selection: "#665c54",
      accent: "#fe8019",
      muted: "#928374",
      frame: "#665c54",
      ansi: [
        "#282828", "#d73128", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984",
        "#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2",
      ],
    },
  },
  {
    preset: "Catppuccin Latte",
    issues: [
      { slot: "ansi2", ratio: 2.956563533142448, required: 3.0 },
      { slot: "ansi3", ratio: 2.314249076660964, required: 3.0 },
      { slot: "ansi5", ratio: 2.335295417042641, required: 3.0 },
      { slot: "ansi10", ratio: 2.956563533142448, required: 3.0 },
      { slot: "ansi11", ratio: 2.314249076660964, required: 3.0 },
      { slot: "ansi13", ratio: 2.335295417042641, required: 3.0 },
    ],
    fixed: {
      background: "#eff1f5",
      foreground: "#4c4f69",
      cursor: "#dc8a78",
      selection: "#acb0be",
      accent: "#8839ef",
      muted: "#7c7f93",
      frame: "#9ca0b0",
      ansi: [
        "#5c5f77", "#d20f39", "#3e9e29", "#c57a00", "#1e66f5", "#d260b4", "#179299", "#acb0be",
        "#acb0be", "#d20f39", "#3e9e29", "#c57a00", "#1e66f5", "#d260b4", "#179299", "#bcc0cc",
      ],
    },
  },
  {
    preset: "Solarized Light",
    issues: [
      { slot: "foreground", ratio: 4.129605890896795, required: 4.5 },
      { slot: "muted", ratio: 2.4789959188576267, required: 3.0 },
      { slot: "ansi2", ratio: 2.9704965004249213, required: 3.0 },
      { slot: "ansi3", ratio: 2.975393600563635, required: 3.0 },
      { slot: "ansi6", ratio: 2.9276158246163706, required: 3.0 },
      { slot: "ansi12", ratio: 2.9313067732855314, required: 3.0 },
      { slot: "ansi14", ratio: 2.4789959188576267, required: 3.0 },
    ],
    fixed: {
      background: "#fdf6e3",
      foreground: "#5e747c",
      cursor: "#657b83",
      selection: "#eee8d5",
      accent: "#268bd2",
      muted: "#829090",
      frame: "#839496",
      ansi: [
        "#073642", "#dc322f", "#849700", "#b38800", "#268bd2", "#d33682", "#259e95", "#eee8d5",
        "#002b36", "#cb4b16", "#586e75", "#657b83", "#829294", "#6c71c4", "#829090", "#fdf6e3",
      ],
    },
  },
];

function knownContrast(colors: Colors) {
  return KNOWN_CONTRAST.find((known) => sameColors(colors, presetNamed(known.preset).colors));
}

/**
 * The palette simple mode answers for a tone: one of CMD Studio's own presets (the core's
 * `presets::OWN`), whose colors pass the readability check as `palette::generate`'s do.
 */
const SIMPLE_BASE: Record<Tone, string> = { dark: "Terracotta", light: "Daylight" };

export interface MockOptions {
  /** Another Clink in AutoRun: `planApply` answers `otherClink` until `replaceOtherClink`. */
  otherClink?: string[];
  /** Every apply fails its check and rolls back with this code. */
  failApplyWith?: string;
  /** Windows Terminal is not installed. */
  noWindowsTerminal?: boolean;
  settings?: Partial<AppSettings>;
}

/** The mock's own state, open to the tests. */
export interface MockState {
  mine: Theme[];
  drafts: Theme[];
  applied: { theme: Theme; request: ApplyRequest; snapshotId: string } | null;
  points: RestorePoint[];
  settings: AppSettings;
  calls: string[];
  clipboardSuggestion: string;
}

/**
 * A frame-and-text preview in the theme's own colors: fixed data, not a rendering. The welcome
 * box's title is shown as written, its variables (`{user}`…) left as they are.
 */
function fakeLines(theme: Theme): PreviewResult {
  const c = theme.colors;
  const span = (text: string, fg: string, bold = false) => ({ text, fg, bg: null, bold });
  const banner: Line[] = [
    [span("╭──────────────────────────────────────────╮", c.accent)],
    [span("│ ", c.accent), span(`✻ ${theme.banner.title}`, c.foreground), span(" │", c.accent)],
    [span("╰──────────────────────────────────────────╯", c.accent)],
    [span(" Tips: help for commands · cls to clear · ↑ for history", c.muted)],
    [span(" ", c.foreground)],
  ];
  const prompt: Line[] = [
    [span("> ", c.accent), span("npm test", c.foreground)],
    [span(" ", c.foreground)],
    [span("╭─ ", c.frame), span("~\\projects\\cmd-studio", c.accent), span(" · main *", c.ansi[3] ?? c.foreground), span(" ──────── ", c.frame), span("16:47", c.muted), span(" ─╮", c.frame)],
    [span("│ ", c.frame), span(">", c.accent), span(" ", c.foreground)],
  ];
  return { banner, prompt };
}

export function createMockIpc(options: MockOptions = {}): Ipc & { state: MockState } {
  const presets: Theme[] = PRESETS.map((t) => structuredClone(t));
  const byTone = (tone: Tone) => presets.find((p) => p.tone === tone) ?? presets[0]!;
  const state: MockState = {
    mine: [],
    drafts: [],
    applied: null,
    points: [],
    settings: {
      language: "en",
      target: "all",
      setWtDefaultProfile: true,
      updates: "auto",
      wizardDone: false,
      ...options.settings,
    },
    calls: [],
    clipboardSuggestion: "",
  };
  let nextMs = 1791694709000;
  const now = () => nextMs++;

  // The file each theme of mine has in `themes\`, named as library.rs names it when the theme
  // is first saved: its name, then " (2)", " (3)"… while a file of that name (ASCII letter case
  // aside) is there; `safe_file_stem`'s replacement of characters a file name cannot hold is
  // left out. The listing orders two themes of one name by it, as Rust's stable sort does over
  // `themes\` in file-name order. A rename keeps the file.
  const files = new Map<string, string>();
  const fileOf = (t: Theme) => files.get(t.id) ?? `${t.name}.cmdtheme`;
  const asciiLower = (s: string) => s.replace(/[A-Z]/g, (c) => c.toLowerCase());
  const newThemeFile = (name: string): string => {
    const taken = (file: string) => state.mine.some((t) => asciiLower(fileOf(t)) === asciiLower(file));
    for (let n = 1; ; n++) {
      const file = n === 1 ? `${name}.cmdtheme` : `${name} (${n}).cmdtheme`;
      if (!taken(file)) return file;
    }
  };
  /** Rust's order for the listing: by `name.to_lowercase()`, a tie by file name. */
  const listed = (themes: Theme[], file: (t: Theme) => string): Theme[] =>
    [...themes].sort((a, b) => order(a.name.toLowerCase(), b.name.toLowerCase()) || order(file(a), file(b)));

  const all = (): Theme[] => [...presets, ...state.mine];
  const find = (id: string): [Theme, ThemeSource] => {
    const preset = presets.find((t) => t.id === id);
    if (preset) return [preset, "preset"];
    const mine = state.mine.find((t) => t.id === id);
    if (mine) return [mine, "mine"];
    const draft = state.drafts.find((t) => t.id === id);
    if (draft) return [draft, "draft"];
    throw error("E_THEME_INVALID", `there is no theme with the id ${id}`);
  };
  const summary = (t: Theme, source: ThemeSource): ThemeSummary => ({
    id: t.id,
    name: t.name,
    tone: t.tone,
    layout: t.prompt.layout,
    colors: structuredClone(t.colors),
    source,
    inUse: state.applied?.theme.id === t.id,
    hasDraft: state.drafts.some((d) => d.id === t.id),
  });
  const plan = (request: ApplyRequest, theme: Theme): ApplyPlan => {
    const steps: ApplyPlan["steps"] = [
      { kind: "installOhMyPosh", detail: "runtime\\omp-31.5.0" },
      { kind: "installClink", detail: "runtime\\clink-1.9.34" },
      { kind: "writeGenerated", detail: "theme.omp.json" },
      { kind: "writeGenerated", detail: "init.lua" },
      { kind: "writeGenerated", detail: "banner.lua" },
      { kind: "writeGenerated", detail: "clink_settings" },
    ];
    const summaryEn = ["Update the prompt and the welcome box"];
    const summaryTh = ["ปรับ prompt และกล่องต้อนรับตามธีม"];
    const warnings: ApplyPlan["warnings"] = [];
    if (options.noWindowsTerminal) {
      warnings.push({ kind: "noWindowsTerminal" });
    } else {
      steps.push({ kind: "windowsTerminal", detail: "settings.json" });
      summaryEn.push("Colors and font of cmd in Windows Terminal");
      summaryTh.push("สีและตัวอักษรของหน้าต่าง cmd ใน Windows Terminal");
      if (request.setWtDefaultProfile) {
        summaryEn.push("Open Windows Terminal with cmd");
        summaryTh.push("เปิด Windows Terminal มาเป็น cmd");
      }
    }
    if (request.replaceOtherClink && options.otherClink?.length) {
      summaryEn.push("Turn off the cmd add-on that was set up before (can be undone)");
      summaryTh.push("ปิดส่วนเสริมของ cmd ตัวเดิมที่ติดตั้งไว้ก่อน (ย้อนกลับได้)");
    }
    if (request.target === "all") {
      steps.push({ kind: "autoRun", detail: "" });
      summaryEn.push("Use this theme in every cmd window");
      summaryTh.push("ให้ cmd ทุกหน้าต่างใช้ธีมนี้");
    }
    steps.push({ kind: "saveState", detail: "state.json" });
    return {
      digest: `digest-${theme.id}-${request.target}-${request.setWtDefaultProfile}-${request.replaceOtherClink}-${options.noWindowsTerminal ? "nowt" : "wt"}`,
      themeName: theme.name,
      summaryEn,
      summaryTh,
      warnings,
      steps,
      // As the backend: the other Clink's commands when the user agreed to turn them off.
      replacesOtherClink: request.replaceOtherClink ? [...(options.otherClink ?? [])] : [],
      notes: [],
      unrestored: [],
    };
  };
  const checks = (): CheckReport[] => {
    const ok = (id: string, en: string, th: string): CheckReport => ({
      id,
      status: "ok",
      code: null,
      messageTh: th,
      messageEn: en,
      details: "",
      detailsTh: null,
      fix: null,
    });
    const applied = state.applied !== null;
    const skip = (id: string): CheckReport => ({
      ...ok(id, "No theme has been applied yet", "ยังไม่ได้ใช้ธีม"),
      status: "skipped",
    });
    const list: CheckReport[] = [
      applied ? ok("D1", "Runtime files are complete", "ไฟล์ runtime ครบ") : skip("D1"),
      ok("D2", "Oh My Posh 31.5.0 runs", "Oh My Posh 31.5.0 รันได้"),
      applied
        ? {
            id: "D3",
            status: "warn",
            code: "E_THEME_NOT_LOADED",
            messageTh: "ยังไม่มีหน้าต่าง cmd ที่โหลดธีม",
            messageEn: "No cmd window has loaded the theme yet",
            details: "",
            detailsTh: null,
            fix: "openNewCmdWindow",
          }
        : skip("D3"),
      applied ? ok("D4", "AutoRun holds CMD Studio's entry first", "AutoRun มีส่วนของเราอยู่หน้าสุด") : skip("D4"),
      options.otherClink?.length
        ? {
            id: "D5",
            status: "warn",
            code: "E_AUTORUN_FOREIGN_CLINK",
            messageTh: "เจอ Clink ตัวอื่นใน AutoRun",
            messageEn: "Another Clink runs from AutoRun",
            details: options.otherClink.join("\n"),
            detailsTh: null,
            fix: applied ? "replaceOtherClink" : null,
          }
        : ok("D5", "No other Clink in AutoRun", "ไม่มี Clink ตัวอื่นใน AutoRun"),
      options.noWindowsTerminal
        ? {
            id: "D6",
            status: "warn",
            code: "E_WT_NOT_FOUND",
            messageTh: "ไม่พบ Windows Terminal",
            messageEn: "Windows Terminal was not found",
            details: "",
            detailsTh: null,
            fix: "installWindowsTerminal",
          }
        : ok("D6", "Windows Terminal is installed and its settings parse", "มี Windows Terminal และอ่าน settings.json ได้"),
      applied ? ok("D7", "The cmd profile matches the theme", "โปรไฟล์ cmd ตรงกับธีม") : skip("D7"),
      applied ? ok("D8", "The theme's font is installed", "ฟอนต์ของธีมติดตั้งอยู่") : skip("D8"),
      applied ? ok("D9", "The generated files are as written", "ไฟล์ที่สร้างตรงกับที่บันทึกไว้") : skip("D9"),
      ok("D10", "The app runs unpackaged", "แอปไม่ได้อยู่ในแพ็กเกจ"),
      ok("D11", "Windows Terminal is the default terminal", "แอปเทอร์มินัลเริ่มต้นคือ Windows Terminal"),
    ];
    return list;
  };

  const ipc: Ipc & { state: MockState } = {
    state,
    async systemDetect(): Promise<SystemReport> {
      state.calls.push("systemDetect");
      return {
        build: 26100,
        arch: "x64",
        packaged: false,
        unreadable: [],
        windowsTerminal: options.noWindowsTerminal
          ? [{ kind: "store", installed: false, hasSettings: false }]
          : [{ kind: "store", installed: true, hasSettings: true }],
        wtInstalled: !options.noWindowsTerminal,
        storeAvailable: true,
        autorun: {
          otherCommands: ['if exist "C:\\conda\\conda_hook.bat" "C:\\conda\\conda_hook.bat"'],
          foreignClink: options.otherClink ?? [],
          oursPresent: state.applied !== null && state.applied.request.target === "all",
          machineWideClink: false,
          notText: false,
        },
        fonts: { nerdFontInstalled: false, monospace: ["Cascadia Mono", "Consolas"] },
        dataDir: "C:\\Users\\tester\\AppData\\Local\\ai.resincore.cmdstudio",
        versions: { app: "0.1.0", ohMyPosh: "31.5.0", clink: "1.9.34", nerdFonts: "3.5.1" },
        // Fixed and unlike any fallback, so a test can see the UI sends these back.
        previewSample: { user: "tester", host: "TESTPC", home: "C:\\Users\\tester" },
      };
    },
    async themesList(): Promise<Library> {
      state.calls.push("themesList");
      const mine = listed(state.mine, fileOf);
      const known = new Set(all().map((t) => t.id));
      // A draft's file is `drafts\<id>.cmdtheme`.
      const drafts = listed(
        state.drafts.filter((d) => !known.has(d.id)),
        (d) => `${d.id}.cmdtheme`,
      );
      return {
        themes: [
          ...presets.map((t) => summary(t, "preset")),
          ...mine.map((t) => summary(t, "mine")),
          ...drafts.map((t) => summary(t, "draft")),
        ],
        applied: state.applied
          ? {
              themeId: state.applied.theme.id,
              themeName: state.applied.theme.name,
              target: state.applied.request.target,
              appliedAtMs: 1791694709000,
              snapshotId: state.applied.snapshotId,
            }
          : null,
        problems: [],
        stateProblem: null,
      };
    },
    async themeGet(id): Promise<ThemeDetail> {
      const [theme, source] = find(id);
      const draft = source === "draft" ? null : (state.drafts.find((d) => d.id === id) ?? null);
      return { theme: structuredClone(theme), source, draft: draft ? structuredClone(draft) : null };
    },
    async themeSave(theme, draft): Promise<ThemeSummary> {
      state.calls.push(draft ? "themeSave:draft" : "themeSave");
      if (presets.some((p) => p.id === theme.id)) {
        throw error("E_THEME_INVALID", `"${theme.name}" is a built-in preset`);
      }
      // Kept as written, as the backend writes it: the name trimmed (the backend also cleans
      // out control characters and escape sequences).
      const copy = structuredClone(theme);
      copy.name = theme.name.trim();
      if (copy.name.length === 0) throw error("E_THEME_INVALID", "invalid name: must be 1–64 characters");
      if (draft) {
        state.drafts = [...state.drafts.filter((d) => d.id !== theme.id), copy];
        return summary(copy, state.mine.some((t) => t.id === theme.id) ? "mine" : "draft");
      }
      const i = state.mine.findIndex((t) => t.id === theme.id);
      if (i >= 0) {
        state.mine[i] = copy;
      } else {
        files.set(copy.id, newThemeFile(copy.name));
        state.mine.push(copy);
      }
      state.drafts = state.drafts.filter((d) => d.id !== theme.id);
      return summary(copy, "mine");
    },
    async themeDuplicate(id, name): Promise<ThemeSummary> {
      const [theme] = find(id);
      const copy = structuredClone(theme);
      copy.id = uuid();
      copy.name = name;
      return ipc.themeSave(copy, false);
    },
    async themeDelete(id, draftOnly): Promise<void> {
      // Found before anything is removed, as library.rs does: a preset or an id nobody has is
      // refused with nothing touched, and a theme that is only a draft is still there to find.
      if (!draftOnly) {
        const [theme, source] = find(id);
        if (source === "preset") throw error("E_THEME_INVALID", `"${theme.name}" is a built-in preset`);
      }
      state.drafts = state.drafts.filter((d) => d.id !== id);
      if (draftOnly) return;
      state.mine = state.mine.filter((t) => t.id !== id);
      files.delete(id);
    },
    async themeImportPreview(): Promise<ImportPreview | null> {
      state.calls.push("themeImportPreview");
      const theme = structuredClone(byTone("dark"));
      theme.name = "Imported theme";
      return { token: "mock-token", theme, idTaken: true, nameTaken: false };
    },
    async themeImportConfirm(token): Promise<ThemeSummary> {
      if (token !== "mock-token") throw error("E_THEME_INVALID", "the import preview is no longer there");
      const theme = structuredClone(byTone("dark"));
      theme.id = uuid();
      theme.name = "Imported theme";
      return ipc.themeSave(theme, false);
    },
    async themeExport(id): Promise<string | null> {
      const [theme] = find(id);
      state.calls.push("themeExport");
      return `C:\\Users\\tester\\Desktop\\${theme.name}.cmdtheme`;
    },
    // Fixed data, not generated: the palette of the tone's own preset (SIMPLE_BASE), with the
    // accent put back as the accent and the cursor. The tint is not used. The real answer is
    // palette::generate in Rust.
    async paletteGenerate(tone, accent): Promise<Colors> {
      if (!/^#[0-9a-fA-F]{6}$/.test(accent)) throw error("E_THEME_INVALID", `accent: not a #rrggbb color: "${accent}"`);
      const colors = structuredClone(presetNamed(SIMPLE_BASE[tone]).colors);
      // An echo (ruling R6), unlike the backend: the real palette_generate moves an accent
      // whose contrast is too low (and the cursor with it) and writes lowercase hex. So a UI
      // test must not assert the accent that comes back from here.
      colors.accent = accent;
      colors.cursor = accent;
      return colors;
    },
    async contrastCheck(colors): Promise<ContrastIssue[]> {
      return structuredClone(knownContrast(colors)?.issues ?? []);
    },
    async contrastFix(colors): Promise<Colors> {
      return structuredClone(knownContrast(colors)?.fixed ?? colors);
    },
    async previewRender(theme): Promise<PreviewResult> {
      return fakeLines(theme);
    },
    async planApply(request): Promise<PlanOutcome> {
      state.calls.push("planApply");
      const [theme] = find(request.themeId);
      if (options.otherClink?.length && !request.replaceOtherClink) {
        return { kind: "otherClink", commands: options.otherClink };
      }
      return { kind: "ready", plan: plan(request, theme) };
    },
    async apply(request, digest, onProgress): Promise<ApplyOutcome> {
      state.calls.push("apply");
      const [theme] = find(request.themeId);
      const expected = plan(request, theme);
      if (expected.digest !== digest) {
        return { status: "planChanged", themeName: theme.name, snapshotId: null, error: null, unrestored: [], notes: [], plan: expected };
      }
      const total = expected.steps.length;
      const tell = (progress: ApplyProgress) => onProgress(progress);
      for (let step = 1; step <= total; step++) tell({ phase: "backingUp", step, total, kind: null });
      for (let step = 1; step <= total; step++) tell({ phase: "applying", step, total, kind: expected.steps[step - 1]?.kind ?? null });
      tell({ phase: "verifying", step: 0, total: 0, kind: null });
      const snapshotId = `${now()}-${uuid().slice(0, 8)}`;
      if (options.failApplyWith) {
        for (let step = 1; step <= total; step++) tell({ phase: "rollingBack", step, total, kind: null });
        const appError: AppError = {
          code: options.failApplyWith,
          messageTh: `(${options.failApplyWith})`,
          messageEn: `(${options.failApplyWith})`,
          details: "the mock was told to fail",
        };
        return { status: "rolledBack", themeName: theme.name, snapshotId, error: appError, unrestored: [], notes: [], plan: null };
      }
      state.points.unshift({
        id: snapshotId,
        kind: "apply",
        createdMs: Number(snapshotId.split("-")[0]),
        theme: { id: theme.id, name: theme.name },
        target: request.target,
        summary: expected.summaryEn,
        undoes: null,
        undoneBy: null,
        restorable: true,
      });
      state.applied = { theme: structuredClone(theme), request, snapshotId };
      return { status: "committed", themeName: theme.name, snapshotId, error: null, unrestored: [], notes: [], plan: null };
    },
    async undoLast(): Promise<UndoOutcome> {
      state.calls.push("undoLast");
      const target = state.points.find((p) => p.kind === "apply" && p.undoneBy === null);
      if (!target) {
        return { status: "nothingToUndo", undid: null, error: null, unrestored: [], notes: [], fontsLeft: [] };
      }
      const id = `${now()}-${uuid().slice(0, 8)}`;
      target.undoneBy = id;
      target.restorable = false;
      state.points.unshift({ ...target, id, kind: "undo", undoes: target.id, undoneBy: null, restorable: false, summary: [`Undo ${target.id}`] });
      const previous = state.points.find((p) => p.kind === "apply" && p.undoneBy === null);
      state.applied = previous && previous.theme ? { theme: structuredClone(find(previous.theme.id)[0]), request: { themeId: previous.theme.id, target: previous.target ?? "all", setWtDefaultProfile: true, replaceOtherClink: false }, snapshotId: previous.id } : null;
      return { status: "committed", undid: target.id, error: null, unrestored: [], notes: [], fontsLeft: [] };
    },
    async snapshotsList(): Promise<RestorePoints> {
      return { points: structuredClone(state.points), problems: [] };
    },
    async snapshotRestore(id): Promise<RestoreOutcome> {
      const point = state.points.find((p) => p.id === id);
      if (!point) throw error("E_SNAPSHOT_READ", `there is no restore point ${id}`);
      if (!point.restorable) return { status: "notRestorable", undone: [], last: null, notes: [] };
      const undone: string[] = [];
      let last: UndoOutcome | null = null;
      while (state.points.find((p) => p.id === id)?.undoneBy === null) {
        last = await ipc.undoLast();
        if (last.undid) undone.push(last.undid);
        if (last.status !== "committed") return { status: "stopped", undone, last, notes: [] };
      }
      return { status: "done", undone, last, notes: [] };
    },
    async revertAll(): Promise<RevertOutcome> {
      state.calls.push("revertAll");
      const id = `${now()}-${uuid().slice(0, 8)}`;
      for (const p of state.points) if (p.kind === "apply" && p.undoneBy === null) { p.undoneBy = id; p.restorable = false; }
      state.points.unshift({ id, kind: "revertAll", createdMs: Number(id.split("-")[0]), theme: null, target: null, summary: ["Revert everything"], undoes: null, undoneBy: null, restorable: false });
      state.applied = null;
      return {
        ok: true,
        steps: [
          { what: "AutoRun and Windows Terminal's settings", ok: true, detail: "CMD Studio's part is taken out", code: null, fixEn: null, fixTh: null, error: null },
          { what: "Fonts", ok: true, detail: "nothing to uninstall", code: null, fixEn: null, fixTh: null, error: null },
        ],
        notes: [],
        status: "committed",
        error: null,
        unrestored: [],
      };
    },
    async doctorRun(): Promise<DoctorReport> {
      state.calls.push("doctorRun");
      return { checks: checks(), checkedAtMs: now() };
    },
    async doctorFix(checkId): Promise<FixProposal> {
      const check = checks().find((c) => c.id === checkId);
      if (!check) throw error("E_INTERNAL", `there is no check ${checkId}`);
      const kind = check.fix;
      if ((kind === "reapply" || kind === "replaceOtherClink") && state.applied) {
        const request: ApplyRequest = { ...state.applied.request, replaceOtherClink: kind === "replaceOtherClink" };
        return { kind, request, plan: await ipc.planApply(request) };
      }
      return { kind, request: null, plan: null };
    },
    async settingsGet(): Promise<SettingsLoad> {
      return { settings: { ...state.settings }, resetFrom: null };
    },
    async settingsSet(settings): Promise<AppSettings> {
      state.calls.push("settingsSet");
      state.settings = { ...settings };
      return { ...settings };
    },
    async openCmd(): Promise<void> {
      state.calls.push("openCmd");
    },
    async openExternal(target): Promise<void> {
      state.calls.push(`openExternal:${target}`);
    },
    async diagnosticsCopy(): Promise<string> {
      state.calls.push("diagnosticsCopy");
      return "CMD Studio diagnostics\napp 0.1.0 · Oh My Posh 31.5.0 · Clink 1.9.34 · Nerd Fonts 3.5.1\n(mock)";
    },
    async advancedRead(): Promise<AdvancedInfo> {
      return {
        dataDir: "C:\\Users\\tester\\AppData\\Local\\ai.resincore.cmdstudio",
        logTail: "2026-10-16T12:00:00Z cmd=themes_list ms=3 ok",
        generated: state.applied
          ? [
              { name: "theme.omp.json", text: '{\n  "version": 4\n}' },
              { name: "init.lua", text: "-- init.lua" },
              { name: "banner.lua", text: "-- banner.lua" },
              { name: "clink_settings", text: "clink.logo = none" },
            ]
          : [],
        versions: { app: "0.1.0", ohMyPosh: "31.5.0", clink: "1.9.34", nerdFonts: "3.5.1" },
      };
    },
  };
  return ipc;
}
