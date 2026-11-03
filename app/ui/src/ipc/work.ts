// The wrapper every screen's IPC goes through (recovery spec §6.1–§6.3). It counts what waits;
// after ten seconds the bar (`WorkingBar`) names the oldest task; a command that changes
// nothing is given up after a minute with E_TIMEOUT and its late answer dropped; one that
// changes the machine is never given up (its transaction must end), but after three minutes
// the bar offers "Restart CMD Studio". The two commands that wait for the user's file dialog
// are shown but never timed; the restart itself is neither timed nor shown. One list says what each command is.
import type { Key } from "../i18n/types";
import { IpcError, type Ipc } from "./types";

export const STILL_WORKING_MS = 10_000;
export const READ_DEADLINE_MS = 60_000;
export const RESTART_AFTER_MS = 180_000;

/** What a command is: it only reads or computes (a deadline), changes the machine (a restart
 * after three minutes), waits for the user in a native dialog, or ends the process. */
export type Kind = "read" | "changes" | "user" | "exit";

export const KINDS: Record<keyof Ipc, Kind> = {
  systemDetect: "read",
  themesList: "read",
  themeGet: "read",
  themeSave: "read",
  themeDuplicate: "read",
  themeDelete: "read",
  themeImportPreview: "user",
  themeImportConfirm: "read",
  themeExport: "user",
  paletteGenerate: "read",
  contrastCheck: "read",
  contrastFix: "read",
  previewRender: "read",
  planApply: "read",
  apply: "changes",
  undoLast: "changes",
  snapshotsList: "read",
  snapshotRestore: "changes",
  revertAll: "changes",
  doctorRun: "read",
  doctorFix: "read",
  settingsGet: "read",
  settingsSet: "read",
  openCmd: "read",
  openExternal: "read",
  diagnosticsCopy: "read",
  advancedRead: "read",
  recoveryStatus: "read",
  themePause: "changes",
  themeResume: "read",
  appReady: "read",
  appStatus: "read",
  crashDismiss: "read",
  crashReportRead: "read",
  settingsReset: "read",
  logUiError: "read",
  appRestart: "exit",
};

/** The bar's words for each command (recovery spec §6.1): fifteen tasks. */
export const TASK: Record<keyof Ipc, Key> = {
  systemDetect: "work.reading",
  themesList: "work.readingThemes",
  themeGet: "work.readingThemes",
  themeSave: "work.savingTheme",
  themeDuplicate: "work.savingTheme",
  themeDelete: "work.savingTheme",
  themeImportPreview: "work.choosingFile",
  themeImportConfirm: "work.savingTheme",
  themeExport: "work.choosingFile",
  paletteGenerate: "work.drawingPreview",
  contrastCheck: "work.drawingPreview",
  contrastFix: "work.drawingPreview",
  previewRender: "work.drawingPreview",
  planApply: "work.planning",
  apply: "work.applying",
  undoLast: "work.undoing",
  snapshotsList: "work.reading",
  snapshotRestore: "work.undoing",
  revertAll: "work.undoing",
  doctorRun: "work.checking",
  doctorFix: "work.checking",
  settingsGet: "work.settings",
  settingsSet: "work.settings",
  openCmd: "work.opening",
  openExternal: "work.opening",
  diagnosticsCopy: "work.reading",
  advancedRead: "work.reading",
  recoveryStatus: "work.reading",
  themePause: "work.pausing",
  themeResume: "work.resuming",
  appReady: "work.noting",
  appStatus: "work.reading",
  crashDismiss: "work.noting",
  crashReportRead: "work.reading",
  settingsReset: "work.settings",
  logUiError: "work.noting",
  appRestart: "work.restarting",
};

/** The E_TIMEOUT row of spec §5.9, as `MEANINGS` in app/src-tauri/src/error.rs has it (a test
 * reads that row and compares). */
export const TIMEOUT_ROW = {
  th: "CMD Studio ไม่ตอบภายใน 60 วินาที สำหรับคำสั่งที่ไม่เปลี่ยนเครื่อง ให้ลองใหม่ ถ้ายังเป็นอยู่ให้เริ่ม CMD Studio ใหม่ (recovery spec §6.2)",
  en: "CMD Studio did not answer within 60 seconds for a command that changes nothing; try again, and if it stays, restart CMD Studio (recovery spec §6.2)",
};

export interface WorkState {
  /** The oldest task that has waited ten seconds or more; `null` when none has. */
  task: Key | null;
  /** A change has run for three minutes or more. */
  offerRestart: boolean;
}

export interface WorkStore {
  get(): WorkState;
  subscribe(listener: () => void): () => void;
}

/** The command names as the Rust side logs them, for E_TIMEOUT's details. */
function snake(method: string): string {
  return method.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
}

/** The bar's task of a change named as the backend names it (`app_status.busy`: snake_case,
 * one of `CHANGES_MACHINE`), for the start's wait (recovery spec §6.3). A name the list does
 * not know is said as an apply, which is what the start used to assume for every change. */
export function taskOf(busy: string): Key {
  const method = busy.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());
  return Object.hasOwn(TASK, method) ? TASK[method as keyof Ipc] : TASK.apply;
}

interface Pending {
  method: keyof Ipc;
  startedAt: number;
}

export function withWork(ipc: Ipc, now: () => number = Date.now): { ipc: Ipc; work: WorkStore } {
  const pending: Pending[] = [];
  const listeners = new Set<() => void>();
  let state: WorkState = { task: null, offerRestart: false };
  let ticker: ReturnType<typeof setInterval> | null = null;

  const compute = (): WorkState => {
    const t = now();
    const shown = pending.filter((p) => KINDS[p.method] !== "exit" && t - p.startedAt >= STILL_WORKING_MS);
    const oldest = shown.reduce<Pending | null>((a, p) => (a === null || p.startedAt < a.startedAt ? p : a), null);
    const offerRestart = pending.some((p) => KINDS[p.method] === "changes" && t - p.startedAt >= RESTART_AFTER_MS);
    return { task: oldest ? TASK[oldest.method] : null, offerRestart };
  };
  const publish = () => {
    const next = compute();
    if (next.task === state.task && next.offerRestart === state.offerRestart) return;
    state = next;
    listeners.forEach((l) => l());
  };
  const tick = () => {
    publish();
    if (pending.length === 0 && ticker !== null) {
      clearInterval(ticker);
      ticker = null;
    }
  };
  const begin = (method: keyof Ipc): Pending => {
    const p = { method, startedAt: now() };
    pending.push(p);
    if (ticker === null) ticker = setInterval(tick, 1000);
    return p;
  };
  const end = (p: Pending) => {
    const at = pending.indexOf(p);
    if (at >= 0) pending.splice(at, 1);
    tick();
  };

  const track = <T>(method: keyof Ipc, run: () => Promise<T>): Promise<T> => {
    if (KINDS[method] === "exit") return run();
    const p = begin(method);
    let started: Promise<T>;
    try {
      started = run();
    } catch (e) {
      // A throw before any promise exists: nothing is left waiting.
      end(p);
      throw e;
    }
    const settled = started.finally(() => end(p));
    if (KINDS[method] !== "read") return settled;
    return new Promise<T>((resolve, reject) => {
      let done = false;
      const timer = setTimeout(() => {
        if (done) return;
        done = true;
        end(p);
        reject(
          new IpcError({
            code: "E_TIMEOUT",
            messageTh: TIMEOUT_ROW.th,
            messageEn: TIMEOUT_ROW.en,
            details: `${snake(method)} did not answer within ${READ_DEADLINE_MS / 1000} s`,
          }),
        );
      }, READ_DEADLINE_MS);
      settled.then(
        (value) => {
          // A late answer, after the deadline, is dropped (recovery spec §6.2).
          if (done) return;
          done = true;
          clearTimeout(timer);
          resolve(value);
        },
        (error: unknown) => {
          if (done) return;
          done = true;
          clearTimeout(timer);
          reject(error);
        },
      );
    });
  };

  const wrapped = {} as Record<keyof Ipc, (...args: unknown[]) => Promise<unknown>>;
  const inner = ipc as unknown as Record<keyof Ipc, (...args: unknown[]) => Promise<unknown>>;
  for (const method of Object.keys(KINDS) as (keyof Ipc)[]) {
    wrapped[method] = (...args: unknown[]) => track(method, () => inner[method](...args));
  }
  const work: WorkStore = {
    get: () => state,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
  return { ipc: wrapped as unknown as Ipc, work };
}
