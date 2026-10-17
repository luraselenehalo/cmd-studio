// App-wide state the screens share: the settings (language, where themes go), the last apply
// (for the after-apply bar, spec §4.4) and a counter that tells screens the library or the
// machine changed, so they reload.
import { createContext, useContext } from "react";
import type { AppSettings } from "./bindings/AppSettings";
import type { ApplyOutcome } from "./bindings/ApplyOutcome";

export interface AppState {
  settings: AppSettings;
  /** Saves and remembers the settings; `language` is applied to the UI at once. */
  saveSettings: (settings: AppSettings) => Promise<void>;
  lastApply: ApplyOutcome | null;
  setLastApply: (outcome: ApplyOutcome | null) => void;
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
