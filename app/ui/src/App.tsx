// The shell as the themes plan leaves it: the settings loaded once, English unless the saved
// language is Thai, the Themes page and the editor behind the sidebar, and the after-apply bar
// (spec §4.4). The wizard, the home page, Settings, Help and Advanced come with the next plan,
// which replaces this file. Everything sits in an element whose `lang` is the UI's language, so a
// screen reader reads Thai as Thai and the `:lang(th)` styles apply.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AppSettings } from "./bindings/AppSettings";
import type { ApplyOutcome } from "./bindings/ApplyOutcome";
import type { RestoreOutcome } from "./bindings/RestoreOutcome";
import type { Target } from "./bindings/Target";
import { ErrorNotice, Notice, NotesList, UnrestoredList } from "./components/Notices";
import { I18nProvider, pick, useI18n, type Language } from "./i18n";
import { useIpc } from "./ipc/context";
import { NavContext, useNav, type Nav, type Route } from "./router";
import { Editor } from "./screens/Editor";
import { Themes } from "./screens/Themes";
import { AppStateContext, DEFAULT_SETTINGS, useAppState, type AppState } from "./state";

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
  const started = useRef(false);

  // A route change puts focus on the new screen's heading (spec §4.5), so a keyboard or
  // screen-reader user lands on its title. A screen that is still loading gets it once its
  // heading appears; an open dialog keeps the focus it took. The first screen leaves focus at
  // the top of the page.
  useEffect(() => {
    if (!started.current) {
      started.current = true;
      return;
    }
    const box = main.current;
    if (!box) return;
    const focusHeading = (): boolean => {
      if (box.querySelector('[role="dialog"]')) return true;
      const heading = box.querySelector<HTMLElement>("h1");
      if (!heading) return false;
      heading.focus();
      return true;
    };
    if (focusHeading()) return;
    const observer = new MutationObserver(() => {
      if (focusHeading()) observer.disconnect();
    });
    observer.observe(box, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [route]);

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

type UndoResult = { kind: "restore"; outcome: RestoreOutcome } | { kind: "error"; error: unknown };

/**
 * Spec §4.4: after every apply, "Theme X applied · open a new cmd window…" (a new cmd tab in
 * Windows Terminal when that is where the theme went) with Undo, which goes back to that apply's
 * own restore point (ruling R3). Undo is offered for a committed apply only: a rolled-back one
 * carries a restore point too, but nothing of it is left to undo. What the undo did stays on the
 * bar after the apply it undid is gone from it (ruling R4).
 */
function AfterApplyBar({ target }: { target: Target }) {
  const ipc = useIpc();
  const { t } = useI18n();
  const { lastApply, setLastApply, bump } = useAppState();
  const [result, setResult] = useState<UndoResult | null>(null);
  const [busy, setBusy] = useState(false);

  // A new apply replaces what the last undo said.
  useEffect(() => {
    if (lastApply) setResult(null);
  }, [lastApply]);

  const offer = lastApply && lastApply.status === "committed" && lastApply.snapshotId ? lastApply : null;
  if (!offer && !result) return null;

  const undo = async () => {
    const snapshotId = offer?.snapshotId;
    if (!snapshotId) return;
    setBusy(true);
    setResult(null);
    try {
      const outcome = await ipc.snapshotRestore(snapshotId);
      setResult({ kind: "restore", outcome });
      // The apply is gone from the bar once it is undone, or can no longer be.
      if (outcome.status === "notRestorable" || outcome.undone.includes(snapshotId)) setLastApply(null);
      bump();
    } catch (e) {
      setResult({ kind: "error", error: e });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="bar">
      <div style={{ flex: 1 }}>
        {offer && <span role="status">{t(target === "wt" ? "apply.doneWt" : "apply.done", { name: offer.themeName })}</span>}
        {result && <UndoResultView result={result} />}
      </div>
      {offer && (
        <button type="button" disabled={busy} onClick={() => void undo()}>
          {t("apply.undo")}
        </button>
      )}
      <button
        type="button"
        className="link"
        onClick={() => {
          setLastApply(null);
          setResult(null);
        }}
      >
        {t("common.dismiss")}
      </button>
    </div>
  );
}

/** How the bar's Undo went, in its own words for each outcome (ruling R4). */
function UndoResultView({ result }: { result: UndoResult }) {
  const { t, language } = useI18n();
  if (result.kind === "error") return <ErrorNotice error={result.error} />;
  const { outcome } = result;
  const last = outcome.last;
  const notes = [...outcome.notes, ...(last?.notes ?? [])];
  if (outcome.status === "done") {
    return (
      <>
        <div className="notice ok" role="status">
          {t("apply.undone")}
          {last && last.fontsLeft.length > 0 && (
            <ul className="plain-list">
              {last.fontsLeft.map((f, i) => (
                <li key={i}>{pick(language, f.messageTh, f.messageEn)}</li>
              ))}
            </ul>
          )}
        </div>
        <NotesList notes={notes} />
      </>
    );
  }
  if (outcome.status === "notRestorable") {
    return (
      <>
        <div className="notice warn" role="alert">
          {t("apply.undoNotRestorable")}
        </div>
        <NotesList notes={notes} />
      </>
    );
  }
  // Stopped: the undo that ran last says why.
  if (last?.status === "rolledBack") {
    return (
      <>
        <div className="notice error" role="alert">
          {t("apply.undoFailed")}
        </div>
        {last.error && <ErrorNotice error={last.error} />}
        <NotesList notes={notes} />
      </>
    );
  }
  if (last?.status === "rollbackIncomplete") {
    return (
      <>
        <div className="notice error" role="alert">
          {t("apply.undoIncomplete")}
        </div>
        <UnrestoredList steps={last.unrestored} />
        {last.error && <ErrorNotice error={last.error} />}
        <NotesList notes={notes} />
      </>
    );
  }
  return (
    <>
      <div className="notice warn" role="alert">
        {t("apply.undoStopped")}
      </div>
      {last?.error && <ErrorNotice error={last.error} />}
      <NotesList notes={notes} />
    </>
  );
}
