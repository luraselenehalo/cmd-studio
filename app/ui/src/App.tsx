// The shell (spec §4.3, §4.4, §4.5): the settings loaded once, English until the saved language
// says Thai (founder 2026-10-16), the wizard until it is done, then the five pages behind a
// sidebar with the after-apply bar above them. What lives elsewhere is kept as the themes plan
// left it: the bar (components/AfterApplyBar, fed with the applied request's target by every
// apply), DraftProblemNotice above the screen, the editor keyed by its theme, useRouteFocus, and
// main.tsx's ErrorBoundary around it all. Everything sits in an element whose `lang` is the UI's
// language, so a screen reader reads Thai as Thai and the `:lang(th)` styles apply.
import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import type { AppSettings } from "./bindings/AppSettings";
import type { SettingsLoad } from "./bindings/SettingsLoad";
import { AfterApplyBar } from "./components/AfterApplyBar";
import { DraftProblemNotice } from "./components/DraftProblemNotice";
import { ErrorNotice, Notice } from "./components/Notices";
import { I18nProvider, useI18n, type Key, type Language } from "./i18n";
import { useIpc } from "./ipc/context";
import { NavContext, useNav, type Nav, type Route } from "./router";
import { Advanced } from "./screens/Advanced";
import { Editor } from "./screens/Editor";
import { Help } from "./screens/Help";
import { Home } from "./screens/Home";
import { Settings } from "./screens/Settings";
import { Themes } from "./screens/Themes";
import { Wizard } from "./screens/Wizard";
import { AppStateContext, DEFAULT_SETTINGS, useDraftProblems, type AppState, type LastApply } from "./state";
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

export function App() {
  const ipc = useIpc();
  // English until the saved settings say otherwise (founder 2026-10-16).
  const [language, setLanguage] = useState<Language>("en");
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [route, setRoute] = useState<Route | null>(null);
  /** The last committed apply, with the target of the request that made it (item 12). */
  const [lastApply, setLastApply] = useState<LastApply | null>(null);
  const draftProblems = useDraftProblems();
  const [version, setVersion] = useState(0);
  const [startError, setStartError] = useState<unknown>(null);
  const [resetFrom, setResetFrom] = useState<string | null>(null);
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
    ipc.settingsGet().then(
      (loaded) => {
        if (!alive) return;
        adopt(loaded);
        setRoute(loaded.settings.wizardDone ? { name: "home" } : { name: "wizard" });
      },
      (e: unknown) => {
        if (!alive) return;
        // Said on the home page; the wizard is not shown over settings that could not be read.
        // The settings held meanwhile are the defaults, not the file's: see `saveSettings`.
        setStartError(e);
        setRoute({ name: "home" });
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc, adopt]);

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

  const keepUnsaved = useCallback((next: AppSettings, error: unknown) => {
    setSettings(next);
    setLanguage(next.language);
    setUnsaved(error);
  }, []);

  const bump = useCallback(() => setVersion((v) => v + 1), []);
  const appState = useMemo<AppState>(
    () => ({ settings, saveSettings, lastApply, setLastApply, draftProblems, version, bump }),
    [settings, saveSettings, lastApply, draftProblems, version, bump],
  );
  const nav = useMemo<Nav>(() => ({ route: route ?? { name: "home" }, go: (r: Route) => setRoute(r) }), [route]);

  return (
    <I18nProvider language={language} setLanguage={setLanguage}>
      <AppStateContext.Provider value={appState}>
        <NavContext.Provider value={nav}>
          <div lang={language}>
            {route === null ? (
              <p role="status" style={{ padding: "2rem" }}>
                …
              </p>
            ) : (
              <Screens
                startError={startError}
                onRetry={tryAgain}
                resetFrom={resetFrom}
                unsaved={unsaved}
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
function StartNotices({ startError, onRetry, resetFrom, unsaved }: StartProblems) {
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
  return (
    <>
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
