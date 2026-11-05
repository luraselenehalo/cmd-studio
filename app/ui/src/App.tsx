// The shell (spec §4.3, §4.4, §4.5): the settings loaded once, English until the saved language
// says Thai (founder 2026-10-16), the wizard until it is done, then the five pages behind a
// sidebar with the after-apply bar above them. What lives elsewhere is kept as the themes plan
// left it: the bar (components/AfterApplyBar, fed with the applied request's target by every
// apply), DraftProblemNotice above the screen, the editor keyed by its theme, useRouteFocus, and
// main.tsx's ErrorBoundary around it all. Everything sits in an element whose `lang` is the UI's
// language, so a screen reader reads Thai as Thai and the `:lang(th)` styles apply.
import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import type { AppSettings } from "./bindings/AppSettings";
import type { AppStatus } from "./bindings/AppStatus";
import type { CrashNotice } from "./bindings/CrashNotice";
import type { SettingsLoad } from "./bindings/SettingsLoad";
import type { Updated } from "./bindings/Updated";
import { AfterApplyBar } from "./components/AfterApplyBar";
import { DraftProblemNotice } from "./components/DraftProblemNotice";
import { ErrorNotice, Notice } from "./components/Notices";
import { WorkingBar } from "./components/WorkingBar";
import { formatTime, I18nProvider, useI18n, type Key, type Language } from "./i18n";
import { useIpc } from "./ipc/context";
import { taskOf } from "./ipc/work";
import { NavContext, useNav, type Nav, type Route } from "./router";
import { Advanced } from "./screens/Advanced";
import { Editor } from "./screens/Editor";
import { Help } from "./screens/Help";
import { Home } from "./screens/Home";
import { Recovery } from "./screens/Recovery";
import { Settings } from "./screens/Settings";
import { Themes } from "./screens/Themes";
import { Wizard } from "./screens/Wizard";
import { AppStateContext, DEFAULT_SETTINGS, useAppState, useDraftProblems, type AppState, type LastApply } from "./state";
import { useRouteFocus } from "./useRouteFocus";

/** `loaded` with what `next` changes put on top. `base` is what the caller built `next` from, so
 * a key is a change only where `next` differs from it. */
function withChanges(loaded: AppSettings, base: AppSettings, next: AppSettings): AppSettings {
  const merged: AppSettings = { ...loaded };
  for (const key of Object.keys(next) as (keyof AppSettings)[]) {
    if (next[key] !== base[key]) Object.assign(merged, { [key]: next[key] });
  }
  return merged;
}

/** How often the start asks `app_status` again while a change still runs (recovery spec §6.3). */
export const APP_STATUS_POLL_MS = 2000;

/** Recovery spec §6.3: a change of this process still runs; the start waits for it and names it
 * by the bar's task of that change (`taskOf`: the backend's snake_case name mapped once).
 * `recovery_entry` is RECOVERY_ENTRY_BUSY of backend/run.rs: the `--recovery` entry's own name. */
function StartWait({ busy }: { busy: string }) {
  const { t } = useI18n();
  return (
    <p role="status" style={{ padding: "2rem" }}>
      {busy === "recovery_entry" ? t("start.recovering") : t("start.busyWith", { task: t(taskOf(busy)) })}
    </p>
  );
}

export function App() {
  const ipc = useIpc();
  // English until the saved settings say otherwise (founder 2026-10-16).
  const [language, setLanguage] = useState<Language>("en");
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [route, setRoute] = useState<Route | null>(null);
  /** `app_status` as read first on this load (recovery spec §6.3); `null` until then. */
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [busyWith, setBusyWith] = useState<string | null>(null);
  /** The last committed apply, with the target of the request that made it (item 12). */
  const [lastApply, setLastApply] = useState<LastApply | null>(null);
  const draftProblems = useDraftProblems();
  const [version, setVersion] = useState(0);
  const [recoveryActed, setRecoveryActed] = useState(false);
  const [startError, setStartError] = useState<unknown>(null);
  const [resetFrom, setResetFrom] = useState<string | null>(null);
  /** The crash notice was closed: it stays closed whatever screen draws the notices next. */
  const [crashClosed, setCrashClosed] = useState(false);
  /** "Later" on the update offer: hidden for the rest of this run (spec §6.2). */
  const [updateLater, setUpdateLater] = useState(false);
  /** The "Updated to vX" notice was closed: it stays closed for the run. */
  const [updateClosed, setUpdateClosed] = useState(false);
  /** Why the wizard's settings could not be saved as it went home: they are kept in memory, the
   * next save that goes through writes them, and the shell says so until then (fix I1). */
  const [unsaved, setUnsaved] = useState<unknown>(null);

  /** What one read of settings.json gives, held as the app's settings: the language, the notice
   * of a file that was set aside, and no start problem any more. */
  const adopt = useCallback((loaded: SettingsLoad) => {
    setSettings(loaded.settings);
    setLanguage(loaded.settings.language);
    setResetFrom(loaded.resetFrom);
    setStartError(null);
  }, []);

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | null = null;
    // `safe`: the app is in safe mode (recovery spec §5.2): the first screen is the Recovery
    // page, whatever the settings say (the wizard and the library are not shown), and `app_ready`
    // follows its drawing like any first screen.
    const readSettings = (safe: boolean) =>
      ipc.settingsGet().then(
        (loaded) => {
          if (!alive) return;
          adopt(loaded);
          setRoute(safe ? { name: "recovery" } : loaded.settings.wizardDone ? { name: "home" } : { name: "wizard" });
        },
        (e: unknown) => {
          if (!alive) return;
          // Said on the home page; the wizard is not shown over settings that could not be read.
          // The settings held meanwhile are the defaults, not the file's: see `saveSettings`.
          setStartError(e);
          setRoute(safe ? { name: "recovery" } : { name: "home" });
        },
      );
    // Recovery spec §6.3: on every load, `app_status` first. While this process still runs a
    // change, say so and ask again every two seconds (K12: this includes the `--recovery` entry's
    // own busy, after which the answer is read again); a status that cannot be read is no reason
    // to hold the start.
    const readStatus = (): Promise<void> =>
      ipc.appStatus().then(
        (s) => {
          if (!alive) return;
          if (s.busy !== null) {
            setBusyWith(s.busy);
            timer = setTimeout(() => void readStatus(), APP_STATUS_POLL_MS);
            return;
          }
          setBusyWith(null);
          setStatus(s);
          return readSettings(s.safeMode);
        },
        () => {
          if (!alive) return;
          setBusyWith(null);
          return readSettings(false);
        },
      );
    void readStatus();
    return () => {
      alive = false;
      if (timer !== null) clearTimeout(timer);
    };
  }, [ipc, adopt]);

  // The first screen is drawn: the run is a good start from then on (recovery spec §5.1).
  const ready = useRef(false);
  useEffect(() => {
    if (route === null || ready.current) return;
    ready.current = true;
    ipc.appReady().catch(() => {});
  }, [ipc, route]);

  /** Reads settings.json again after the start read failed. A read that works replaces the
   * defaults held in memory with what the file holds and ends the problem; one that fails keeps
   * the problem, with the newest reason, and throws it. */
  const readAgain = useCallback(async (): Promise<SettingsLoad> => {
    try {
      const loaded = await ipc.settingsGet();
      adopt(loaded);
      return loaded;
    } catch (e) {
      setStartError(e);
      throw e;
    }
  }, [ipc, adopt]);

  /** Try again on the start problem: reads once more, and goes on as a start would have, which is
   * to the wizard for a first run (when the user is still on the home page). */
  const tryAgain = useCallback(async () => {
    try {
      const loaded = await readAgain();
      if (!loaded.settings.wizardDone) setRoute((r) => (r?.name === "home" ? { name: "wizard" } : r));
    } catch {
      // `readAgain` kept the problem, and the notice shows it.
    }
  }, [readAgain]);

  const saveSettings = useCallback(
    async (next: AppSettings) => {
      let toSave = next;
      if (startError !== null) {
        // settings.json could not be read as the app started, so what is held is the defaults and
        // `next` was built on them. Writing it as it is would put the defaults over a file that
        // may only have been held for a moment (an antivirus), losing the user's choices and
        // bringing the wizard back. So read it again first: if that fails nothing is written
        // (the error is thrown, and the shell's notice offers Try again); if it works, only what
        // `next` changes is put on top of what the file holds.
        const loaded = await readAgain();
        toSave = withChanges(loaded.settings, settings, next);
      }
      const saved = await ipc.settingsSet(toSave);
      setSettings(saved);
      setLanguage(saved.language);
      // Everything in memory is saved now, the wizard's choices included.
      setUnsaved(null);
    },
    [ipc, settings, startError, readAgain],
  );

  /** The user's own reset of the settings (the Recovery page): the defaults stand at once, and
   * the "could not be read" notice does not apply, so none is shown (the page says what it did). */
  const adoptReset = useCallback((loaded: SettingsLoad) => {
    setSettings(loaded.settings);
    setLanguage(loaded.settings.language);
    setResetFrom(null);
    setStartError(null);
  }, []);

  const keepUnsaved = useCallback((next: AppSettings, error: unknown) => {
    setSettings(next);
    setLanguage(next.language);
    setUnsaved(error);
  }, []);

  const bump = useCallback(() => setVersion((v) => v + 1), []);
  const markRecoveryActed = useCallback(() => setRecoveryActed(true), []);
  const markUpdateLater = useCallback(() => setUpdateLater(true), []);
  const appState = useMemo<AppState>(
    () => ({
      settings,
      saveSettings,
      lastApply,
      setLastApply,
      draftProblems,
      version,
      bump,
      recoveryActed,
      markRecoveryActed,
      appStatus: status ? { ...status, crashNotice: crashClosed ? null : status.crashNotice, updated: updateClosed ? null : status.updated } : null,
      adoptSettings: adoptReset,
      updateLater,
      markUpdateLater,
    }),
    [settings, saveSettings, lastApply, draftProblems, version, bump, recoveryActed, markRecoveryActed, status, crashClosed, adoptReset, updateLater, updateClosed, markUpdateLater],
  );
  const nav = useMemo<Nav>(() => ({ route: route ?? { name: "home" }, go: (r: Route) => setRoute(r) }), [route]);

  return (
    <I18nProvider language={language} setLanguage={setLanguage}>
      <AppStateContext.Provider value={appState}>
        <NavContext.Provider value={nav}>
          <div lang={language}>
            <WorkingBar />
            {busyWith !== null ? (
              <StartWait busy={busyWith} />
            ) : route === null ? (
              <p role="status" style={{ padding: "2rem" }}>
                …
              </p>
            ) : (
              <Screens
                startError={startError}
                onRetry={tryAgain}
                resetFrom={resetFrom}
                unsaved={unsaved}
                onCrashClosed={() => setCrashClosed(true)}
                onUpdateClosed={() => setUpdateClosed(true)}
                onUnsaved={keepUnsaved}
              />
            )}
          </div>
        </NavContext.Provider>
      </AppStateContext.Provider>
    </I18nProvider>
  );
}

/** What the shell says above every screen about the settings themselves. */
interface StartProblems {
  /** settings.json could not be read as the app started: nothing is saved over it until a read
   * goes through. */
  startError: unknown;
  /** Try again on that problem: reads settings.json once more. */
  onRetry: () => Promise<void>;
  /** settings.json was reset, and the old one kept as this file. */
  resetFrom: string | null;
  /** The wizard's settings could not be saved (fix I1). */
  unsaved: unknown;
  /** The crash notice was closed. */
  onCrashClosed: () => void;
  /** The "Updated to vX" notice was closed. */
  onUpdateClosed: () => void;
}

/**
 * The wizard or the shell, each with its `main` in one ref the route focus follows: a change of
 * screen puts focus on the new screen's heading (spec §4.5), the first Home after the wizard
 * included; the first screen of the app leaves it at the top of the page.
 */
function Screens({ onUnsaved, ...problems }: StartProblems & { onUnsaved: (settings: AppSettings, error: unknown) => void }) {
  const { route } = useNav();
  const main = useRef<HTMLElement>(null);
  useRouteFocus(route, main);
  if (route.name === "recovery") {
    return (
      <main id="content" className="recovery" ref={main}>
        <StartNotices {...problems} />
        <Recovery />
      </main>
    );
  }
  if (route.name === "wizard") {
    return (
      <main id="content" className="wizard" ref={main}>
        <StartNotices {...problems} />
        <Wizard onUnsaved={onUnsaved} />
      </main>
    );
  }
  return <Shell main={main} {...problems} />;
}

/** The settings could not be read as the app started, were reset, or could not be saved by the
 * wizard: then the wizard shows again at the next start, as the notice says. */
function StartNotices({ startError, onRetry, resetFrom, unsaved, onCrashClosed, onUpdateClosed }: StartProblems) {
  const { t } = useI18n();
  const [reading, setReading] = useState(false);
  const retry = async () => {
    setReading(true);
    try {
      await onRetry();
    } finally {
      setReading(false);
    }
  };
  const { appStatus } = useAppState();
  return (
    <>
      {appStatus?.crashNotice && <CrashClosed notice={appStatus.crashNotice} onClosed={onCrashClosed} />}
      {appStatus?.updated && <UpdatedNotice updated={appStatus.updated} onClosed={onUpdateClosed} />}
      {startError !== null && (
        <>
          <ErrorNotice error={startError} title={t("settings.startFailed")} />
          <div className="row-actions">
            <button type="button" disabled={reading} onClick={() => void retry()}>
              {t("common.retry")}
            </button>
          </div>
        </>
      )}
      {resetFrom && <Notice kind="warn">{t("settings.reset", { file: resetFrom })}</Notice>}
      {unsaved !== null && <ErrorNotice error={unsaved} title={t("wizard.notSaved")} />}
    </>
  );
}

/** The earlier run ended abnormally (recovery spec §5.3): when, and one click to the report with
 * the crash note. Closing it tells the backend, which forgets the notice. */
function CrashClosed({ notice, onClosed }: { notice: CrashNotice; onClosed: () => void }) {
  const { t, language } = useI18n();
  const ipc = useIpc();
  const { go, route } = useNav();
  const time = formatTime(notice.atMs, language);
  return (
    <Notice kind="warn">
      <p>{notice.hasReport ? t("start.crashed", { time }) : t("start.crashedNoReport", { time })}</p>
      <div className="row-actions">
        <button type="button" className="primary" onClick={() => {
            if (route.name !== "recovery") return go({ name: "help", tab: "report" });
            // Recovery mode stays on its page: the report section is on it.
            const section = document.getElementById("recovery-report");
            section?.scrollIntoView?.();
            section?.focus();
          }}
        >
          {t("home.paused.report")}
        </button>
        <button
          type="button"
          onClick={() => {
            onClosed();
            ipc.crashDismiss().catch(() => {});
          }}
        >
          {t("common.close")}
        </button>
      </div>
    </Notice>
  );
}

/** The last run's update went through (spec §2.1, §6.2): which version this is now, and one click
 * to the release page, which the backend opens for a SemVer version only. Close hides it for the
 * run; the backend forgot the record when the first screen drew (app_ready). */
function UpdatedNotice({ updated, onClosed }: { updated: Updated; onClosed: () => void }) {
  const { t } = useI18n();
  const ipc = useIpc();
  const [openError, setOpenError] = useState<unknown>(null);
  return (
    <Notice kind="ok">
      <p>{t("start.updated", { version: updated.to })}</p>
      <div className="row-actions">
        <button
          type="button"
          className="primary"
          onClick={() => {
            setOpenError(null);
            ipc.openExternal({ releaseNotes: { version: updated.to } }).catch(setOpenError);
          }}
        >
          {t("start.whatChanged")}
        </button>
        <button type="button" onClick={onClosed}>
          {t("common.close")}
        </button>
      </div>
      {openError !== null && <ErrorNotice error={openError} />}
    </Notice>
  );
}

const PAGES: { route: Route; label: Key }[] = [
  { route: { name: "home" }, label: "nav.home" },
  { route: { name: "themes" }, label: "nav.themes" },
  { route: { name: "settings" }, label: "nav.settings" },
  { route: { name: "help", tab: "doctor" }, label: "nav.help" },
  { route: { name: "advanced" }, label: "nav.advanced" },
];

function Shell({ main, ...problems }: StartProblems & { main: RefObject<HTMLElement | null> }) {
  const { t } = useI18n();
  const { route, go } = useNav();
  // The editor belongs to the Themes page; every Help tab to Help.
  const current = (page: Route) => page.name === route.name || (page.name === "themes" && route.name === "editor");

  return (
    <>
      <a className="skip-link" href="#content">
        {t("a11y.skipToContent")}
      </a>
      <AfterApplyBar />
      <div className="shell">
        <nav className="sidebar" aria-label={t("a11y.mainMenu")}>
          <div className="brand">{t("app.name")}</div>
          {PAGES.map((page) => (
            <button
              key={page.route.name}
              type="button"
              aria-current={current(page.route) ? "page" : undefined}
              onClick={() => go(page.route)}
            >
              {t(page.label)}
            </button>
          ))}
        </nav>
        <main id="content" ref={main}>
          <StartNotices {...problems} />
          <DraftProblemNotice />
          {route.name === "home" && <Home />}
          {route.name === "themes" && <Themes />}
          {route.name === "editor" && <Editor key={route.id} id={route.id} />}
          {route.name === "settings" && <Settings />}
          {route.name === "help" && <Help tab={route.tab} />}
          {route.name === "advanced" && <Advanced />}
        </main>
      </div>
    </>
  );
}
