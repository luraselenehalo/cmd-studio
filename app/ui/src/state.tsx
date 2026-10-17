// App-wide state the screens share: the settings (language, where themes go), the last apply
// (for the after-apply bar, spec §4.4), the drafts that could not be written or taken away (for
// the next screen to say), and a counter that tells screens the library or the machine
// changed, so they reload.
import { createContext, useCallback, useContext, useMemo, useRef, useState } from "react";
import type { AppSettings } from "./bindings/AppSettings";
import type { ApplyOutcome } from "./bindings/ApplyOutcome";
import type { Target } from "./bindings/Target";
import type { Theme } from "./bindings/Theme";

/**
 * A draft that could not be written or taken away where the page that tried can no longer say
 * so: the editor as it went away (item 4), or a rename that saved the theme but not its draft
 * again (ruling R8). The next screen says it, with Try again; a draft to write is kept here.
 * `name` is the theme's stored name, so two problems can be told apart.
 */
export type DraftProblemReport =
  | { kind: "write"; id: string; name: string; draft: Theme; error: unknown }
  | { kind: "remove"; id: string; name: string; error: unknown };

/** A problem as it is kept: numbered, so a late Try again can tell it is still the same one. */
export type DraftProblem = DraftProblemReport & { seq: number };

/** The draft problems, at most one per theme (round 2, fix 2). */
export interface DraftProblems {
  list: DraftProblem[];
  /** Keeps `problem` in place of any older one of the same theme. */
  report: (problem: DraftProblemReport) => void;
  /** Takes the theme's problem away: whichever it is, or only `seq`, so a late Try again never
   * takes a newer one away. An editor clears its theme's once it wrote, removed or saved. */
  clear: (id: string, seq?: number) => void;
  /** Problem `seq`'s Try again failed: says the new error only while that problem is still kept,
   * so a problem dismissed meanwhile does not come back. */
  retryFailed: (id: string, seq: number, error: unknown) => void;
  /** Whether problem `seq` of theme `id` is still kept, as of now (not of the last render). */
  isCurrent: (id: string, seq: number) => boolean;
}

/** The draft problems' state, for the shell that provides the app state. */
export function useDraftProblems(): DraftProblems {
  const [list, setList] = useState<DraftProblem[]>([]);
  // The list as of now, so `isCurrent` is right before the next render.
  const current = useRef<DraftProblem[]>([]);
  const next = useRef(0);
  const change = useCallback((f: (list: DraftProblem[]) => DraftProblem[]) => {
    current.current = f(current.current);
    setList(current.current);
  }, []);
  const report = useCallback(
    (problem: DraftProblemReport) => {
      const seq = ++next.current;
      change((l) => [...l.filter((p) => p.id !== problem.id), { ...problem, seq }]);
    },
    [change],
  );
  const clear = useCallback(
    (id: string, seq?: number) => change((l) => l.filter((p) => p.id !== id || (seq !== undefined && p.seq !== seq))),
    [change],
  );
  const retryFailed = useCallback(
    (id: string, seq: number, error: unknown) => change((l) => l.map((p) => (p.id === id && p.seq === seq ? { ...p, error } : p))),
    [change],
  );
  const isCurrent = useCallback((id: string, seq: number) => current.current.some((p) => p.id === id && p.seq === seq), []);
  return useMemo(() => ({ list, report, clear, retryFailed, isCurrent }), [list, report, clear, retryFailed, isCurrent]);
}

/** The last committed apply and the target of the request that made it (the bar's words
 * depend on it): that request's own, not the settings', since a doctor fix or a later
 * Settings change can differ from them (item 12). */
export interface LastApply {
  outcome: ApplyOutcome;
  target: Target;
}

export interface AppState {
  settings: AppSettings;
  /** Saves and remembers the settings; `language` is applied to the UI at once. */
  saveSettings: (settings: AppSettings) => Promise<void>;
  lastApply: LastApply | null;
  setLastApply: (applied: LastApply | null) => void;
  /** Shown by `DraftProblemNotice` on whatever screen comes next. Its functions never change. */
  draftProblems: DraftProblems;
  /** Bumped after an apply, undo, restore or revert: screens that show the machine reload. */
  version: number;
  bump: () => void;
}

export const AppStateContext = createContext<AppState | null>(null);

export function useAppState(): AppState {
  const state = useContext(AppStateContext);
  if (!state) throw new Error("useAppState needs an AppStateContext");
  return state;
}

/** The settings before `settings_get` answers, the same as Rust's `AppSettings::default()`. The
 * UI opens in English (founder 2026-10-16): Thai comes only from the saved `language`. */
export const DEFAULT_SETTINGS: AppSettings = {
  language: "en",
  target: "all",
  setWtDefaultProfile: true,
  updates: "auto",
  wizardDone: false,
};
