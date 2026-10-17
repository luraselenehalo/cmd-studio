// The shell as the themes plan leaves it: the settings loaded once, English unless the saved
// language is Thai, the Themes page and the editor behind the sidebar, and the after-apply bar
// (spec §4.4). The wizard, the home page, Settings, Help and Advanced come with the next plan,
// which replaces this file. Everything sits in an element whose `lang` is the UI's language, so a
// screen reader reads Thai as Thai and the `:lang(th)` styles apply.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AppSettings } from "./bindings/AppSettings";
import type { ApplyOutcome } from "./bindings/ApplyOutcome";
import type { Target } from "./bindings/Target";
import { AfterApplyBar } from "./components/AfterApplyBar";
import { ErrorNotice, Notice } from "./components/Notices";
import { I18nProvider, useI18n, type Language } from "./i18n";
import { useIpc } from "./ipc/context";
import { NavContext, useNav, type Nav, type Route } from "./router";
import { Editor } from "./screens/Editor";
import { Themes } from "./screens/Themes";
import { AppStateContext, DEFAULT_SETTINGS, type AppState } from "./state";
import { useRouteFocus } from "./useRouteFocus";

export function App() {
  const ipc = useIpc();
  // English until the saved settings say otherwise (founder 2026-10-16).
  const [language, setLanguage] = useState<Language>("en");
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [route, setRoute] = useState<Route | null>(null);
  /** The last committed apply, with the target it went to (the bar's words depend on it). */
  const [applied, setApplied] = useState<{ outcome: ApplyOutcome; target: Target } | null>(null);
  const [version, setVersion] = useState(0);
  const [startError, setStartError] = useState<unknown>(null);
  const [resetFrom, setResetFrom] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    ipc.settingsGet().then(
      (loaded) => {
        if (!alive) return;
        setSettings(loaded.settings);
        setLanguage(loaded.settings.language);
        setResetFrom(loaded.resetFrom);
        setRoute({ name: "themes" });
      },
      (e: unknown) => {
        if (!alive) return;
        setStartError(e);
        setRoute({ name: "themes" });
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc]);

  const saveSettings = useCallback(
    async (next: AppSettings) => {
      const saved = await ipc.settingsSet(next);
      setSettings(saved);
      setLanguage(saved.language);
    },
    [ipc],
  );

  // Every apply of this shell asks for `settings.target`, so that is where it went.
  const target = settings.target;
  const setLastApply = useCallback(
    (outcome: ApplyOutcome | null) => setApplied(outcome ? { outcome, target } : null),
    [target],
  );
  const bump = useCallback(() => setVersion((v) => v + 1), []);
  const appState = useMemo<AppState>(
    () => ({ settings, saveSettings, lastApply: applied?.outcome ?? null, setLastApply, version, bump }),
    [settings, saveSettings, applied, setLastApply, version, bump],
  );
  const nav = useMemo<Nav>(() => ({ route: route ?? { name: "themes" }, go: (r: Route) => setRoute(r) }), [route]);

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
              <Shell startError={startError} resetFrom={resetFrom} appliedTarget={applied?.target ?? target} />
            )}
          </div>
        </NavContext.Provider>
      </AppStateContext.Provider>
    </I18nProvider>
  );
}

function Shell({ startError, resetFrom, appliedTarget }: { startError: unknown; resetFrom: string | null; appliedTarget: Target }) {
  const { t } = useI18n();
  const { route, go } = useNav();
  const main = useRef<HTMLElement>(null);
  useRouteFocus(route, main);

  const onThemes = route.name === "themes" || route.name === "editor";

  return (
    <>
      <a className="skip-link" href="#content">
        {t("a11y.skipToContent")}
      </a>
      <AfterApplyBar target={appliedTarget} />
      <div className="shell">
        <nav className="sidebar" aria-label={t("a11y.mainMenu")}>
          <div className="brand">{t("app.name")}</div>
          <button type="button" aria-current={onThemes ? "page" : undefined} onClick={() => go({ name: "themes" })}>
            {t("nav.themes")}
          </button>
        </nav>
        <main id="content" ref={main}>
          {startError !== null && <ErrorNotice error={startError} />}
          {resetFrom && <Notice kind="warn">{t("settings.reset", { file: resetFrom })}</Notice>}
          {route.name === "editor" ? <Editor key={route.id} id={route.id} /> : <Themes />}
        </main>
      </div>
    </>
  );
}
