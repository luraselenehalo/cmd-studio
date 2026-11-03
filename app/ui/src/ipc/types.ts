// The IPC surface the screens use (spec §5.7, with preview_render, open_external and
// advanced_read, and the theme's pause of recovery spec §8). One interface, two
// implementations: `tauri.ts` over Tauri's invoke, and `mock.ts` in memory for tests and the
// visual check. The types are the ts-rs bindings the Rust side exports (app/ui/src/bindings,
// committed; CI fails when they are stale).
import type { AdvancedInfo } from "../bindings/AdvancedInfo";
import type { AppError } from "../bindings/AppError";
import type { AppSettings } from "../bindings/AppSettings";
import type { ApplyOutcome } from "../bindings/ApplyOutcome";
import type { ApplyProgress } from "../bindings/ApplyProgress";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { Colors } from "../bindings/Colors";
import type { ContrastIssue } from "../bindings/ContrastIssue";
import type { DoctorReport } from "../bindings/DoctorReport";
import type { ExternalTarget } from "../bindings/ExternalTarget";
import type { FixProposal } from "../bindings/FixProposal";
import type { ImportPreview } from "../bindings/ImportPreview";
import type { Library } from "../bindings/Library";
import type { PauseOutcome } from "../bindings/PauseOutcome";
import type { PauseReason } from "../bindings/PauseReason";
import type { PlanOutcome } from "../bindings/PlanOutcome";
import type { PreviewContext } from "../bindings/PreviewContext";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { RecoveryStatus } from "../bindings/RecoveryStatus";
import type { RestoreOutcome } from "../bindings/RestoreOutcome";
import type { RestorePoints } from "../bindings/RestorePoints";
import type { ResumeOutcome } from "../bindings/ResumeOutcome";
import type { RevertOutcome } from "../bindings/RevertOutcome";
import type { SettingsLoad } from "../bindings/SettingsLoad";
import type { SystemReport } from "../bindings/SystemReport";
import type { Theme } from "../bindings/Theme";
import type { ThemeDetail } from "../bindings/ThemeDetail";
import type { ThemeSummary } from "../bindings/ThemeSummary";
import type { Tone } from "../bindings/Tone";
import type { UndoOutcome } from "../bindings/UndoOutcome";

export interface Ipc {
  systemDetect(): Promise<SystemReport>;
  themesList(): Promise<Library>;
  themeGet(id: string): Promise<ThemeDetail>;
  themeSave(theme: Theme, draft: boolean): Promise<ThemeSummary>;
  themeDuplicate(id: string, name: string): Promise<ThemeSummary>;
  themeDelete(id: string, draftOnly: boolean): Promise<void>;
  /** Opens the native Open dialog; `null` when the user cancelled. */
  themeImportPreview(): Promise<ImportPreview | null>;
  themeImportConfirm(token: string): Promise<ThemeSummary>;
  /** Opens the native Save dialog; the path written, or `null` when cancelled. */
  themeExport(id: string): Promise<string | null>;
  paletteGenerate(tone: Tone, accent: string, tint: number): Promise<Colors>;
  contrastCheck(colors: Colors): Promise<ContrastIssue[]>;
  contrastFix(colors: Colors): Promise<Colors>;
  previewRender(theme: Theme, context: PreviewContext): Promise<PreviewResult>;
  planApply(request: ApplyRequest): Promise<PlanOutcome>;
  /** Applies, calling `onProgress` for each `apply-progress` event while it runs. */
  apply(
    request: ApplyRequest,
    digest: string,
    onProgress: (progress: ApplyProgress) => void,
  ): Promise<ApplyOutcome>;
  undoLast(): Promise<UndoOutcome>;
  snapshotsList(): Promise<RestorePoints>;
  snapshotRestore(id: string): Promise<RestoreOutcome>;
  revertAll(): Promise<RevertOutcome>;
  doctorRun(): Promise<DoctorReport>;
  doctorFix(checkId: string): Promise<FixProposal>;
  settingsGet(): Promise<SettingsLoad>;
  settingsSet(settings: AppSettings): Promise<AppSettings>;
  openCmd(): Promise<void>;
  openExternal(target: ExternalTarget): Promise<void>;
  diagnosticsCopy(): Promise<string>;
  advancedRead(): Promise<AdvancedInfo>;
  /** Whether the theme is paused, and what the start's check of Windows' crash reports found
   * (recovery spec §4.2, §4.3). Not for polling: while the start's check runs, one call can wait
   * for it (up to 30 s), so it is read when Home loads and after a pause, a resume or an apply. */
  recoveryStatus(): Promise<RecoveryStatus>;
  /** Pauses the theme on this computer (Revert everything underneath): `user` from Home,
   * `clinkCrash` from the health check's D12. The backend refuses any other reason with
   * `E_INTERNAL`; they are in the type because it is the generated one. */
  themePause(reason: PauseReason): Promise<PauseOutcome>;
  /** Turns the theme back on: a pause in cmd at once, one of this computer as a request to
   * apply through the plan. */
  themeResume(): Promise<ResumeOutcome>;
}

/** A command failed: the Rust side's `AppError` (spec §5.9), as a JavaScript error. */
export class IpcError extends Error {
  readonly code: string;
  readonly messageTh: string;
  readonly messageEn: string;
  readonly details: string;

  constructor(error: AppError) {
    super(error.messageEn);
    this.name = "IpcError";
    this.code = error.code;
    this.messageTh = error.messageTh;
    this.messageEn = error.messageEn;
    this.details = error.details;
  }

  toAppError(): AppError {
    return {
      code: this.code,
      messageTh: this.messageTh,
      messageEn: this.messageEn,
      details: this.details,
    };
  }
}

/** The shape of an `AppError`: a code and three texts. */
export function isAppError(value: unknown): value is AppError {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.code === "string" &&
    typeof v.messageTh === "string" &&
    typeof v.messageEn === "string" &&
    typeof v.details === "string"
  );
}

/** What was thrown, as text for the details: a string as it is, an Error's message, else JSON. */
function original(thrown: unknown): string {
  if (typeof thrown === "string") return thrown;
  if (thrown instanceof Error) return thrown.message;
  try {
    return JSON.stringify(thrown) ?? String(thrown);
  } catch {
    return String(thrown);
  }
}

/**
 * Whatever a command threw, as an `IpcError`. Tauri rejects with the serialized `AppError`;
 * anything else (a deserialization message from Tauri, a plain Error) becomes `E_INTERNAL`
 * with its text as the details. The texts are the E_INTERNAL row of spec §5.9, as `MEANINGS`
 * in app/src-tauri/src/error.rs has it (a test reads that row and compares).
 */
export function toIpcError(thrown: unknown): IpcError {
  if (thrown instanceof IpcError) return thrown;
  if (isAppError(thrown)) return new IpcError(thrown);
  return new IpcError({
    code: "E_INTERNAL",
    messageTh:
      "CMD Studio เองมีปัญหา รายละเอียดบอกว่าตรงไหน ลองใหม่อีกครั้ง ถ้ายังเป็นอยู่ให้แจ้งปัญหาพร้อมข้อมูลวินิจฉัย",
    messageEn:
      "CMD Studio itself ran into a problem; the details say where. Try again, and if it stays, report it with the diagnostics",
    details: original(thrown),
  });
}
