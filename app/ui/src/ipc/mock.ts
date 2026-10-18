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
import type { CrashCheck } from "../bindings/CrashCheck";
import type { DoctorReport } from "../bindings/DoctorReport";
import type { FixProposal } from "../bindings/FixProposal";
import type { ImportPreview } from "../bindings/ImportPreview";
import type { Library } from "../bindings/Library";
import type { Line } from "../bindings/Line";
import type { PauseInfo } from "../bindings/PauseInfo";
import type { PauseOutcome } from "../bindings/PauseOutcome";
import type { PauseReason } from "../bindings/PauseReason";
import type { PlanOutcome } from "../bindings/PlanOutcome";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { RecoveryStatus } from "../bindings/RecoveryStatus";
import type { RestoreOutcome } from "../bindings/RestoreOutcome";
import type { RestorePoint } from "../bindings/RestorePoint";
import type { RestorePoints } from "../bindings/RestorePoints";
import type { ResumeOutcome } from "../bindings/ResumeOutcome";
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

/** The cmd windows in a row that fail to start, and the crashes of cmd in Clink, that pause the
 * theme: the core's `recovery::PAUSE_AFTER`, copied as fixed data (D12 says `warn` below it and
 * `fail` from it). */
const PAUSE_AFTER = 3;

/**
 * D12's words for a pause, copied from the core's `paused_check` as fixed rows (crates/core/src/
 * doctor.rs), in the order the core tries them. A row is chosen by what the pause is given as (its
 * `level`, who paused it and why); nothing is worked out. Thai then English. A pause "not yet" or
 * "not known" never says "paused": the theme is paused in cmd only, or may be, and the words say so.
 */
function pausedWords(p: PauseInfo): { th: string; en: string } {
  const home = "เปิดธีมกลับได้ที่หน้าแรกของ CMD Studio";
  if (p.level === "computerNotYet") {
    return {
      th: "ตอนนี้หยุดธีมไว้ใน cmd เท่านั้น ยังหยุดทั้งเครื่องไม่สำเร็จ: CMD Studio เอาการเปลี่ยนแปลงออกไม่ได้ และจะลองใหม่ตอนเปิดแอป CMD Studio ครั้งหน้า",
      en: "The theme is paused in cmd only, not yet on this computer: CMD Studio could not take its changes out and tries again the next time the CMD Studio app starts",
    };
  }
  if (p.level === "computerUnknown") {
    return {
      th: "อาจมีการหยุดธีมของเครื่องนี้อยู่ แต่อ่าน state.json ไม่ได้",
      en: "A pause of this computer may be in place, but state.json could not be read",
    };
  }
  if (p.reason === null) {
    return {
      th: "CMD Studio อ่านบันทึกการหยุดธีม (paused.json) ไม่ได้ จึงถือว่าธีมถูกหยุดไว้ใน cmd การเปิดธีมกลับที่หน้าแรกของ CMD Studio จะยกเลิกการหยุดนี้",
      en: "CMD Studio can't read its pause record (paused.json), so it treats the theme as paused in cmd. Turning the theme back on from CMD Studio's home page lifts it",
    };
  }
  if (p.level === "cmd" && p.by === "app") {
    return {
      th: `ธีมถูกหยุดไว้ใน cmd เท่านั้น: มีการใช้ธีมหลังการหยุดนี้ แต่ลบบันทึกการหยุดไม่ได้ ${home}`,
      en: "The theme is paused in cmd only: a theme was applied after this pause, but its record could not be cleared. Turn it back on from CMD Studio's home page",
    };
  }
  if (p.by === "clink" && p.reason === "cmdStarts") {
    return {
      th: `ธีมถูกหยุดไว้ใน cmd เพราะ cmd เปิดไม่ถึงพร้อมท์ ${p.count} ครั้งติดกัน ${home}`,
      en: `The theme is paused in cmd: cmd failed to reach its prompt ${p.count} times in a row. Turn it back on from CMD Studio's home page`,
    };
  }
  if (p.by === "app" && p.reason === "clinkCrash") {
    // A count of 0 is a pause saved when the crash reports could not be read: no number is true.
    const times =
      p.count === 0
        ? { th: "", en: "" }
        : p.count === 1
          ? { th: " 1 ครั้ง", en: " once" }
          : { th: ` ${p.count} ครั้ง`, en: ` ${p.count} times` };
    return {
      th: `CMD Studio หยุดธีมของเครื่องนี้ไว้ เพราะ cmd แครชในส่วนเสริม${times.th} ${home}`,
      en: `CMD Studio paused the theme on this computer: cmd crashed in its add-on${times.en}. Turn it back on from CMD Studio's home page`,
    };
  }
  if (p.by === "app" && p.reason === "recovery") {
    return {
      th: `CMD Studio หยุดธีมของเครื่องนี้ไว้จากทางลัด CMD Studio Recovery ${home}`,
      en: "CMD Studio paused the theme on this computer from the CMD Studio Recovery shortcut. Turn it back on from CMD Studio's home page",
    };
  }
  if (p.by === "app" && p.reason === "user") {
    return {
      th: `ธีมของเครื่องนี้ถูกหยุดไว้ตามที่คุณสั่ง ${home}`,
      en: "The theme is paused on this computer, as you asked. Turn it back on from CMD Studio's home page",
    };
  }
  return { th: `ธีมถูกหยุดไว้ ${home}`, en: "The theme is paused. Turn it back on from CMD Studio's home page" };
}

/** The level line of D12's details, as the core words it (the Thai only for the two levels that
 * are not plain). The core's "since" line is left out: it is a time worked out from `atMs`. */
function pausedLevel(p: PauseInfo): { en: string; th: string | null } {
  if (p.level === "computerNotYet") {
    return {
      en: "this computer, not yet (in cmd only for now): CMD Studio could not take its changes out and tries again the next time the CMD Studio app starts",
      th: "เครื่องนี้ ยังไม่สำเร็จ (ตอนนี้หยุดไว้ใน cmd เท่านั้น): CMD Studio เอาการเปลี่ยนแปลงออกไม่ได้ และจะลองใหม่ตอนเปิดแอป CMD Studio ครั้งหน้า",
    };
  }
  if (p.level === "computerUnknown") {
    return {
      en: "this computer, not known: state.json can't be read, so whether CMD Studio took its changes out is not known",
      th: "เครื่องนี้ ยังบอกไม่ได้: อ่าน state.json ไม่ได้ จึงไม่รู้ว่า CMD Studio เอาการเปลี่ยนแปลงออกแล้วหรือยัง",
    };
  }
  if (p.level === "computer") return { en: "this computer (CMD Studio took its changes out)", th: null };
  if (p.reason === null) {
    return { en: "treated as in cmd (the record can't be read: it may be damaged, or written by a newer CMD Studio)", th: null };
  }
  if (p.by === "app") {
    return { en: "cmd only (a theme was applied after this pause, but its record could not be cleared)", th: null };
  }
  return { en: "cmd only (nothing on this computer was changed)", th: null };
}

/** D12 while the theme is paused: the core's row for the pause it is given. */
function pausedCheck(p: PauseInfo): CheckReport {
  const words = pausedWords(p);
  const level = pausedLevel(p);
  const reason = p.reason ?? "not known (paused.json can't be read as a pause record)";
  return {
    id: "D12",
    status: "warn",
    code: "E_THEME_PAUSED",
    messageTh: words.th,
    messageEn: words.en,
    details: `paused: ${level.en}\nreason: ${reason}`,
    detailsTh: level.th === null ? null : `หยุดไว้: ${level.th}\nเหตุผล: ${reason}`,
    fix: "resume",
  };
}

export interface MockOptions {
  /** Another Clink in AutoRun: `planApply` answers `otherClink` until `replaceOtherClink`. */
  otherClink?: string[];
  /** Every apply fails its check and rolls back with this code. */
  failApplyWith?: string;
  /** Windows Terminal is not installed. */
  noWindowsTerminal?: boolean;
  /**
   * Windows Terminal (the Store one) is installed and was never opened: it has no settings.json
   * yet, and `system_detect` says an apply sets it up (`setUpByApply`). The plan says so in the
   * core's words, and D6 says so until a theme is applied.
   */
  wtNeverOpened?: boolean;
  /** Windows' build as `system_detect` reports it, `null` when it could not be read; 26100 when
   * not given. */
  build?: number | null;
  /** `system_detect`'s `storeAvailable` as given, never worked out from `build` (the backend's
   * rule, build 19041 or later, is Rust's); `true` when not given. D6 offers the Store only
   * when it is `true`, as the backend's `fix_for` does. */
  storeAvailable?: boolean;
  settings?: Partial<AppSettings>;
  /**
   * The theme is paused when the mock starts, as `recovery_status` says it (recovery spec §4.2):
   * any shape of `PauseInfo`, given whole and never worked out. Its `level` says how far it
   * reaches (in effect `computer`, `computerNotYet`, `computerUnknown`, or in `cmd`), `reason: null`
   * is a record that can't be read, and a `count` of 0 is a pause saved with no crash count.
   * D12 and `themeResume` go by it.
   */
  paused?: PauseInfo;
  /**
   * cmd's crashes in its add-on since the theme was applied, as the backend's D12 and its start
   * check count them: a fixed number given here, never read from anything. D12 says `warn` for 1
   * and 2 and `fail` from 3, the core's rows (`recovery::PAUSE_AFTER`) copied as fixed data, and
   * the start check pauses the theme on this computer for 3 or more.
   */
  cmdCrashes?: number;
  /** `recovery_status` fails with this code. */
  failRecoveryWith?: string;
  /**
   * The start's crash check answers this at the first `recovery_status`, as given, and nothing is
   * worked out or paused (give `paused` to match): for the shapes `cmdCrashes` does not make, such
   * as crash reports that can't be read (`unreadable`, which D12 then says as not checked), a pause
   * the check tried that did not go through, or an earlier one it tried again. `null` is the
   * answer while the check has not finished or did not run: `recovery_status` then says
   * `crashes: null` every time, whatever else the mock is given. Not given keeps the check that
   * runs once.
   */
  crashCheck?: CrashCheck | null;
  /**
   * "Pause the theme" does not go through, as the backend says it, and so does the start check's
   * own pause for 3 crashes: `refused` (Revert everything is refused with `E_BUSY`, so the theme
   * stays applied and the pause is "not yet"), `rolledBack` (it ran and rolled back, the same),
   * `notSaved` (the record could not be written, `E_DATA_WRITE`, though the theme is out) or
   * `partial` (Revert everything committed with one step failed, `E_FONT_INSTALL`: the theme is
   * out and the record is saved at level `computer`, but `pausedNow` is false, as the backend's
   * `revert.ok && saved` has it).
   */
  failPause?: "refused" | "rolledBack" | "notSaved" | "partial";
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
  /** The theme's pause, as `paused.json` would hold it, with how far it reaches now. */
  paused: PauseInfo | null;
}

/**
 * A frame-and-text preview in the theme's own colors: fixed data, not a rendering. The welcome
 * box's middle row is written out as the core draws the presets' title ("Welcome back, {user}!")
 * for the sample user tester, padded to the frame's 44 columns so its right border lines up; the
 * theme's own title is not read (the mock fills in no variable).
 */
function fakeLines(theme: Theme): PreviewResult {
  const c = theme.colors;
  const span = (text: string, fg: string, bold = false) => ({ text, fg, bg: null, bold });
  const banner: Line[] = [
    [span("╭──────────────────────────────────────────╮", c.accent)],
    [span("│ ", c.accent), span("✻ Welcome back, tester!", c.foreground), span("                  │", c.accent)],
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
    paused: options.paused ? structuredClone(options.paused) : null,
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
    const summaryTh = ["ปรับพร้อมท์และกล่องต้อนรับตามธีม"];
    const warnings: ApplyPlan["warnings"] = [];
    if (options.noWindowsTerminal) {
      warnings.push({ kind: "noWindowsTerminal" });
    } else {
      steps.push({ kind: "windowsTerminal", detail: "settings.json" });
      if (options.wtNeverOpened) {
        // The core's line for a Terminal that was never opened (crates/core/src/plan.rs).
        summaryEn.push("Set up Windows Terminal's settings so cmd gets this theme");
        summaryTh.push("ตั้งค่า Windows Terminal ให้ cmd ได้ธีมนี้");
      }
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
      digest: `digest-${theme.id}-${request.target}-${request.setWtDefaultProfile}-${request.replaceOtherClink}-${options.noWindowsTerminal ? "nowt" : options.wtNeverOpened ? "newwt" : "wt"}`,
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
    /** The core's D12 in fixed words: a pause first, then the crashes it is given (cmdCrashes). */
    const d12 = (): CheckReport => {
      if (state.paused) return pausedCheck(state.paused);
      if (!applied) return skip("D12");
      if (options.crashCheck?.kind === "unreadable") {
        const why = options.crashCheck.why;
        return {
          id: "D12",
          status: "skipped",
          code: null,
          messageTh: `ตรวจข้อนี้ไม่ได้ เพราะอ่านรายงานแครชของ Windows (event log ของ Application) ไม่ได้: ${why}`,
          messageEn: `Not checked: Windows' crash reports (the Application event log) could not be read: ${why}`,
          details: "",
          detailsTh: null,
          fix: null,
        };
      }
      const crashes = options.cmdCrashes ?? 0;
      if (crashes === 0) {
        return ok(
          "D12",
          "cmd has not crashed in its add-on since the theme was applied, and the theme is not paused",
          "cmd ไม่ได้แครชในส่วนเสริมตั้งแต่ใช้ธีม และธีมไม่ได้ถูกหยุดไว้",
        );
      }
      const fail = crashes >= PAUSE_AFTER;
      // The count words of the pause rows (the core's `crashed_times`): "once", else "{n} times".
      const times = crashes === 1 ? { th: " 1 ครั้ง", en: " once" } : { th: ` ${crashes} ครั้ง`, en: ` ${crashes} times` };
      return {
        id: "D12",
        status: fail ? "fail" : "warn",
        code: "E_CMD_CRASHED",
        messageTh: fail
          ? `cmd แครชในส่วนเสริม${times.th}ตั้งแต่ใช้ธีม และธีมยังเปิดอยู่ ให้หยุดธีมใน CMD Studio เพื่อให้ cmd เปิดได้`
          : `cmd แครชในส่วนเสริม${times.th}ตั้งแต่ใช้ธีม ถ้ายังเกิดอีก ให้หยุดธีมใน CMD Studio เพื่อให้ cmd เปิดได้`,
        messageEn: fail
          ? `cmd crashed in its add-on${times.en} since the theme was applied, and the theme is still on. Pause the theme in CMD Studio so that cmd opens`
          : `cmd crashed in its add-on${times.en} since the theme was applied. If it happens again, pause the theme in CMD Studio so that cmd opens`,
        details: "",
        detailsTh: null,
        fix: "pause",
      };
    };
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
      applied ? ok("D4", "AutoRun holds CMD Studio's entry first", "AutoRun มีส่วนของ CMD Studio อยู่หน้าสุด") : skip("D4"),
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
            // As the backend's fix_for: the Store page only where it applies.
            fix: (options.storeAvailable ?? true) ? "installWindowsTerminal" : null,
          }
        : options.wtNeverOpened && !applied
          ? {
              // The core's D6 for it (crates/core/src/doctor.rs): before any apply there is
              // nothing to offer, since the apply sets it up; after one, it has its settings.
              id: "D6",
              status: "warn",
              code: "E_WT_NOT_FOUND",
              messageTh: "Windows Terminal ยังไม่เคยเปิด จึงยังไม่มีการตั้งค่า ใช้ธีมแล้ว CMD Studio จะตั้งค่าให้เอง",
              messageEn: "Windows Terminal has never been opened, so it has no settings yet. Applying a theme sets them up",
              details: "settings.json does not exist yet",
              detailsTh: null,
              fix: null,
            }
          : ok("D6", "Windows Terminal is installed and its settings parse", "มี Windows Terminal และอ่าน settings.json ได้"),
      applied ? ok("D7", "The cmd profile matches the theme", "โปรไฟล์ cmd ตรงกับธีม") : skip("D7"),
      applied ? ok("D8", "The theme's font is installed", "ฟอนต์ของธีมติดตั้งอยู่") : skip("D8"),
      applied ? ok("D9", "The generated files are as written", "ไฟล์ที่สร้างตรงกับที่บันทึกไว้") : skip("D9"),
      ok("D10", "The app runs unpackaged", "แอปไม่ได้อยู่ในแพ็กเกจ"),
      ok("D11", "Windows Terminal is the default terminal", "แอปเทอร์มินัลเริ่มต้นคือ Windows Terminal"),
      d12(),
    ];
    return list;
  };

  /** What Revert everything does to the mock: every apply undone by a new revertAll point. */
  const revertState = () => {
    const id = `${now()}-${uuid().slice(0, 8)}`;
    for (const p of state.points) if (p.kind === "apply" && p.undoneBy === null) { p.undoneBy = id; p.restorable = false; }
    state.points.unshift({ id, kind: "revertAll", createdMs: Number(id.split("-")[0]), theme: null, target: null, summary: ["Revert everything"], undoes: null, undoneBy: null, restorable: false });
    state.applied = null;
    // The core's Revert everything never touches paused.json, so it lifts no pause in cmd.
    afterThemeLeft(false);
  };
  /**
   * What a pause becomes once no theme is applied, as the backend has it. The app's pause is one of
   * this computer in effect (`Paused::level`), however far it reached before; "not known" stays
   * not known. A pause in cmd (init.lua's) has nothing left to pause, and `liftInCmd` lifts it, as
   * Undo everything, and an undo or a restore that leaves no theme, do (`lift_pause_in_cmd`). The
   * pause of the app's own Revert everything (`pauseHere`) does not.
   */
  const afterThemeLeft = (liftInCmd: boolean) => {
    if (state.applied !== null || !state.paused) return;
    if (state.paused.by === "clink") {
      if (liftInCmd) state.paused = null;
    } else if (state.paused.level !== "computerUnknown") {
      state.paused.level = "computer";
    }
  };
  const revertOutcome = (): RevertOutcome => ({
    ok: true,
    steps: [
      { what: "AutoRun and Windows Terminal's settings", ok: true, detail: "CMD Studio's part is taken out", code: null, fixEn: null, fixTh: null, error: null },
      { what: "Fonts", ok: true, detail: "nothing to uninstall", code: null, fixEn: null, fixTh: null, error: null },
    ],
    notes: [],
    status: "committed",
    error: null,
    unrestored: [],
  });
  /** A Revert everything that ran and rolled back (`failPause: "rolledBack"`). */
  const revertRolledBack = (): RevertOutcome => {
    const failed = error("E_AUTORUN_WRITE", "the mock was told to fail").toAppError();
    return {
      ok: false,
      steps: [{ what: "AutoRun and Windows Terminal's settings", ok: false, detail: "AutoRun could not be written", code: failed.code, fixEn: null, fixTh: null, error: failed }],
      notes: [],
      status: "rolledBack",
      error: failed,
      unrestored: [],
    };
  };
  /** A Revert everything that committed with one step failed (`failPause: "partial"`): the theme
   * is out, the fonts step is not. */
  const revertPartial = (): RevertOutcome => {
    const failed = error("E_FONT_INSTALL", "the mock was told to fail").toAppError();
    const done = revertOutcome();
    return {
      ...done,
      ok: false,
      steps: [
        done.steps[0]!,
        { what: "Fonts", ok: false, detail: "a font could not be uninstalled", code: failed.code, fixEn: null, fixTh: null, error: failed },
      ],
    };
  };
  /**
   * As the backend's pause_machine: the record first, with the theme to apply again (the one
   * applied now, else the one an earlier pause of this computer noted), then Revert everything.
   * Whether that goes through is `failPause`'s fixed answer, never worked out: a refused or
   * rolled-back revert leaves the record with the theme still applied ("not yet"), and a record
   * that was not saved leaves the theme out with no pause.
   */
  const pauseHere = (reason: PauseReason, count: number): PauseOutcome => {
    const resume = state.applied
      ? { themeId: state.applied.theme.id, target: state.applied.request.target, themeName: state.applied.theme.name }
      : state.paused?.by === "app"
        ? state.paused.resume
        : null;
    const record = (level: PauseInfo["level"]): PauseInfo => ({ by: "app", level, reason, atMs: now(), count, resume });
    const fail = options.failPause;
    if (fail === "notSaved") {
      revertState();
      return { pausedNow: false, revert: revertOutcome(), refused: null, notSaved: error("E_DATA_WRITE", "the mock was told to fail").toAppError() };
    }
    if (fail === "partial") {
      revertState();
      state.paused = record("computer");
      return { pausedNow: false, revert: revertPartial(), refused: null, notSaved: null };
    }
    if (fail) {
      state.paused = record(state.applied ? "computerNotYet" : "computer");
      return fail === "refused"
        ? { pausedNow: false, revert: null, refused: error("E_BUSY", "the mock was told to fail").toAppError(), notSaved: null }
        : { pausedNow: false, revert: revertRolledBack(), refused: null, notSaved: null };
    }
    revertState();
    state.paused = record("computer");
    return { pausedNow: true, revert: revertOutcome(), refused: null, notSaved: null };
  };
  /**
   * The start's crash check, run once at the first `recoveryStatus`, as the backend runs it once per
   * start (`CrashSlot`): `crashCheck` as given (`null`: never finished, so `crashes: null`), else
   * with no theme applied `noTheme`, else `cmdCrashes` read, and the pause of this computer for 3
   * or more (the core's PAUSE_AFTER). An earlier pause it tried again is not worked out: give
   * `crashCheck`.
   */
  let startCheck: CrashCheck | null = null;
  const recovery = (): RecoveryStatus => {
    if (startCheck === null && options.crashCheck !== null) {
      if (options.crashCheck) {
        startCheck = structuredClone(options.crashCheck);
      } else if (!state.applied) {
        startCheck = { kind: "noTheme" };
      } else {
        const count = options.cmdCrashes ?? 0;
        const pause = count >= PAUSE_AFTER ? pauseHere("clinkCrash", count) : null;
        startCheck = { kind: "read", count, pausedNow: pause?.pausedNow ?? false, pause };
      }
    }
    return { paused: structuredClone(state.paused), crashes: structuredClone(startCheck) };
  };

  const ipc: Ipc & { state: MockState } = {
    state,
    async systemDetect(): Promise<SystemReport> {
      state.calls.push("systemDetect");
      return {
        build: options.build === undefined ? 26100 : options.build,
        arch: "x64",
        packaged: false,
        unreadable: [],
        // Fixed answers, one per option: whether an apply sets a Terminal up is the backend's
        // flag (the core's `detect::never_opened`), given here, never worked out.
        windowsTerminal: options.noWindowsTerminal
          ? [{ kind: "store", installed: false, hasSettings: false, setUpByApply: false }]
          : options.wtNeverOpened
            ? [{ kind: "store", installed: true, hasSettings: false, setUpByApply: true }]
            : [{ kind: "store", installed: true, hasSettings: true, setUpByApply: false }],
        wtInstalled: !options.noWindowsTerminal,
        storeAvailable: options.storeAvailable ?? true,
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
      // As the backend: a theme applied is the theme on again.
      state.paused = null;
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
      // As the backend: an undo that leaves no theme lifts a pause in cmd (a restore goes through here).
      afterThemeLeft(true);
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
      revertState();
      // As the backend: everything is out, so a pause by init.lua (in cmd) goes too; one of the
      // app's stays, since it holds the theme to turn back on.
      afterThemeLeft(true);
      return revertOutcome();
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
    async recoveryStatus(): Promise<RecoveryStatus> {
      state.calls.push("recoveryStatus");
      if (options.failRecoveryWith) throw error(options.failRecoveryWith, "the mock was told to fail");
      return recovery();
    },
    async themePause(reason): Promise<PauseOutcome> {
      state.calls.push(`themePause:${reason}`);
      // As the backend: only the user's and cmd's crashes are the app's reasons to give.
      if (reason !== "user" && reason !== "clinkCrash") {
        throw error("E_INTERNAL", `the app does not pause the theme for ${reason}`);
      }
      return pauseHere(reason, reason === "clinkCrash" ? (options.cmdCrashes ?? 0) : 0);
    },
    async themeResume(): Promise<ResumeOutcome> {
      state.calls.push("themeResume");
      const paused = state.paused;
      if (!paused) return { kind: "notPaused" };
      // By level, as the backend: a pause in cmd (init.lua's, a record that can't be read, or an
      // app record older than the theme in use) is lifted at once.
      if (paused.level === "cmd") {
        state.paused = null;
        return { kind: "resumed" };
      }
      const theme = paused.resume ? all().find((t) => t.id === paused.resume?.themeId) : undefined;
      if (!paused.resume || !theme) return { kind: "pickTheme" };
      const request: ApplyRequest = {
        themeId: theme.id,
        target: paused.resume.target,
        setWtDefaultProfile: state.settings.setWtDefaultProfile,
        replaceOtherClink: false,
      };
      return { kind: "apply", request, plan: await ipc.planApply(request) };
    },
  };
  return ipc;
}
