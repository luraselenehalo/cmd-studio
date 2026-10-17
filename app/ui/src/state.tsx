// App-wide state the screens share: the settings (language, where themes go), the last apply
// (for the after-apply bar, spec §4.4), a draft that could not be written or taken away (for
// the next screen to say), and a counter that tells screens the library or the machine
// changed, so they reload.
import { createContext, useContext } from "react";
import type { AppSettings } from "./bindings/AppSettings";
import type { ApplyOutcome } from "./bindings/ApplyOutcome";
import type { Target } from "./bindings/Target";
import type { Theme } from "./bindings/Theme";

/**
 * A draft that could not be written or taken away where the page that tried can no longer say
 * so: the editor as it went away (item 4), or a rename that saved the theme but not its draft
 * again (ruling R8). The next screen says it, with Try again; a draft to write is kept here.
 */
export type DraftProblem =
  | { kind: "write"; draft: Theme; error: unknown }
  | { kind: "remove"; id: string; error: unknown };

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
  /** Shown by `DraftProblemNotice` on whatever screen comes next. */
  draftProblem: DraftProblem | null;
  setDraftProblem: (problem: DraftProblem | null) => void;
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
