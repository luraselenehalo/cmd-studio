// The first-run wizard (spec §4.2): six pages, a theme applied in five clicks with nothing typed
// (spec §1 criterion 1). Page 1 picks the language, saved at once (spec §4.5); page 2 checks the
// machine; page 3 picks a theme from one listing, Terracotta first and chosen; page 4 says where it
// goes; page 5 shows the real plan and applies with progress, with no Back while it runs (ruling
// R7); page 6 shows the result. The wizard is done once the apply commits (ruling Q12), or when
// the user skips it on pages 1 to 4 (Q13), which changes nothing else. Each page's heading takes
// focus as the page comes, and the apply is said once, in a status region that is always there.
// The shell around it (App.tsx) puts it in the `main` the route focus follows.
import { useEffect, useMemo, useRef, useState } from "react";
import type { AppSettings } from "../bindings/AppSettings";
import type { ApplyOutcome } from "../bindings/ApplyOutcome";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { Library } from "../bindings/Library";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { SystemReport } from "../bindings/SystemReport";
import type { Target } from "../bindings/Target";
import type { Theme } from "../bindings/Theme";
import { ApplyFlow } from "../components/ApplyFlow";
import { ErrorNotice } from "../components/Notices";
import { TerminalPreview } from "../components/TerminalPreview";
import { ThemeCard } from "../components/ThemeCard";
import { useI18n, type Key, type Language } from "../i18n";
import { useIpc } from "../ipc/context";
import { useNav } from "../router";
import { CORE_SAMPLE_IDENTITY, sampleContext } from "../sample";
import { useAppState } from "../state";

/** Spec §4.2 page 3: the six recommended presets, Terracotta first and chosen. */
export const RECOMMENDED = ["Terracotta", "Tokyo Night", "Catppuccin Mocha", "Dracula", "Catppuccin Latte", "Paper"];

const LANGUAGES: Language[] = ["en", "th"];

export function Wizard() {
  const ipc = useIpc();
  const { t, language, setLanguage } = useI18n();
  const { settings, saveSettings, setLastApply, bump } = useAppState();
  const { go } = useNav();
  const [page, setPage] = useState(1);
  const [report, setReport] = useState<SystemReport | null>(null);
  const [reportError, setReportError] = useState<unknown>(null);
  const [detectTick, setDetectTick] = useState(0);
  const [library, setLibrary] = useState<Library | null>(null);
  const [libraryError, setLibraryError] = useState<unknown>(null);
  const [listTick, setListTick] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [target, setTarget] = useState<Target>(settings.target);
  const [setDefault, setSetDefault] = useState(settings.setWtDefaultProfile);
  /** Page 5's apply is running: Back is gone until it has ended (R7). */
  const [busy, setBusy] = useState(false);
  /** The apply that committed, as the status region says it. */
  const [applied, setApplied] = useState<{ name: string; target: Target } | null>(null);
  const [preview, setPreview] = useState<{ theme: Theme; result: PreviewResult | null } | null>(null);
  const [previewError, setPreviewError] = useState<unknown>(null);
  /** A choice that could not be saved: the language, or the wizard being done. */
  const [saveError, setSaveError] = useState<unknown>(null);
  const [openError, setOpenError] = useState<unknown>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const shownPage = useRef(page);

  // Page 2's machine check, from the start and on Check again.
  useEffect(() => {
    let alive = true;
    ipc.systemDetect().then(
      (r) => {
        if (alive) setReport(r);
      },
      (e: unknown) => {
        if (alive) setReportError(e);
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc, detectTick]);

  // Page 3's cards: one listing, which carries each card's colors (R10), and Terracotta chosen.
  useEffect(() => {
    let alive = true;
    ipc.themesList().then(
      (lib) => {
        if (!alive) return;
        setLibrary(lib);
        const presets = lib.themes.filter((s) => s.source === "preset");
        const first = presets.find((s) => s.name === RECOMMENDED[0]) ?? presets[0];
        setSelected((current) => current ?? first?.id ?? null);
      },
      (e: unknown) => {
        if (alive) setLibraryError(e);
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc, listTick]);

  // Page 6's preview: the one theme it needs in full, drawn with this computer's sample (ruling
  // R1, sample.ts). A failure loses the preview only.
  const identity = report?.previewSample ?? CORE_SAMPLE_IDENTITY;
  useEffect(() => {
    if (page !== 6 || !selected) return;
    let alive = true;
    setPreviewError(null);
    ipc
      .themeGet(selected)
      .then(async ({ theme }) => {
        if (!alive) return;
        setPreview({ theme, result: null });
        const result = await ipc.previewRender(theme, sampleContext(identity));
        if (alive) setPreview({ theme, result });
      })
      .catch((e: unknown) => {
        if (alive) setPreviewError(e);
      });
    return () => {
      alive = false;
    };
  }, [ipc, page, selected, identity]);

  // Each page's heading takes focus as the page comes (not the first, which the app leaves alone).
  useEffect(() => {
    if (shownPage.current === page) return;
    shownPage.current = page;
    heading.current?.focus();
  }, [page]);

  const checkAgain = () => {
    setReport(null);
    setReportError(null);
    setDetectTick((n) => n + 1);
  };

  const listAgain = () => {
    setLibraryError(null);
    setListTick((n) => n + 1);
  };

  const pickLanguage = async (next: Language) => {
    // The whole UI switches at once; the choice is kept in settings.json (spec §4.5).
    setLanguage(next);
    setSaveError(null);
    try {
      await saveSettings({ ...settings, language: next });
    } catch (e) {
      setSaveError(e);
    }
  };

  /** The wizard is done: saved with the language on screen and whatever else is given. */
  const saveDone = (more: Partial<AppSettings> = {}) => saveSettings({ ...settings, language, ...more, wizardDone: true });

  /** Skip (pages 1 to 4) and "Go to the home page": saved as done unless the apply's commit did
   * it already, then home. A save that failed is said, and the wizard stays: it would come back
   * at the next start. */
  const leave = async (more: Partial<AppSettings> = {}) => {
    setSaveError(null);
    try {
      if (!settings.wizardDone) await saveDone(more);
      go({ name: "home" });
    } catch (e) {
      setSaveError(e);
    }
  };

  const onApplied = (outcome: ApplyOutcome, request: ApplyRequest) => {
    setLastApply({ outcome, target: request.target });
    bump();
    setApplied({ name: outcome.themeName, target: request.target });
    setPage(6);
    // Done as the apply committed (Q12): an app closed on page 6 does not bring the wizard back.
    setSaveError(null);
    saveDone({ target: request.target, setWtDefaultProfile: request.setWtDefaultProfile }).catch((e: unknown) => setSaveError(e));
  };

  const tryCmd = async () => {
    setOpenError(null);
    try {
      await ipc.openCmd();
    } catch (e) {
      setOpenError(e);
    }
  };

  const presets = useMemo(() => {
    const all = library?.themes.filter((s) => s.source === "preset") ?? [];
    if (showAll) return all;
    return RECOMMENDED.flatMap((name) => all.filter((s) => s.name === name));
  }, [library, showAll]);

  const canNext = page !== 3 || selected !== null;

  return (
    <>
      <p className="badge">{t("wizard.step", { n: page })}</p>
      {page === 1 && (
        <section>
          <h1 ref={heading} tabIndex={-1}>
            {t("wizard.welcome.title")}
          </h1>
          <p>{t("wizard.welcome.text")}</p>
          <fieldset>
            <legend>{t("language.label")}</legend>
            <div className="row-actions">
              {LANGUAGES.map((l) => (
                // Each language's name in that language, for a screen reader (spec §4.5).
                <button key={l} type="button" lang={l} aria-pressed={language === l} onClick={() => void pickLanguage(l)}>
                  {t(`language.${l}`)}
                </button>
              ))}
            </div>
          </fieldset>
        </section>
      )}
      {page === 2 && (
        <section>
          <h1 ref={heading} tabIndex={-1}>
            {t("wizard.check.title")}
          </h1>
          {reportError !== null && (
            <>
              <ErrorNotice error={reportError} />
              <div className="row-actions">
                <button type="button" onClick={checkAgain}>
                  {t("common.refresh")}
                </button>
              </div>
            </>
          )}
          {!report && reportError === null && <p role="status">{t("wizard.check.checking")}</p>}
          {report && <MachineCheck report={report} />}
        </section>
      )}
      {page === 3 && (
        <section>
          <h1 ref={heading} tabIndex={-1}>
            {t("wizard.choose.title")}
          </h1>
          <p>{t("wizard.choose.canChangeLater")}</p>
          {libraryError !== null && (
            <>
              <ErrorNotice error={libraryError} />
              <div className="row-actions">
                <button type="button" onClick={listAgain}>
                  {t("common.retry")}
                </button>
              </div>
            </>
          )}
          {!library && libraryError === null && <p role="status">{t("common.loading")}</p>}
          {library && (
            <>
              <div className="grid">
                {presets.map((s) => (
                  <ThemeCard key={s.id} summary={s} selected={selected === s.id} onSelect={() => setSelected(s.id)} />
                ))}
              </div>
              <button type="button" className="link" onClick={() => setShowAll((v) => !v)}>
                {showAll ? t("wizard.choose.seeFewer") : t("wizard.choose.seeAll")}
              </button>
            </>
          )}
        </section>
      )}
      {page === 4 && (
        <section>
          <h1 ref={heading} tabIndex={-1}>
            {t("wizard.where.title")}
          </h1>
          <fieldset>
            <legend className="visually-hidden">{t("wizard.where.title")}</legend>
            {(["all", "wt"] as Target[]).map((option) => (
              <label className="row" key={option}>
                <input type="radio" name="wizard-target" checked={target === option} onChange={() => setTarget(option)} />
                <span>
                  {t(option === "all" ? "wizard.where.all" : "wizard.where.wt")}
                  <br />
                  <small>{t(option === "all" ? "wizard.where.allHint" : "wizard.where.wtHint")}</small>
                </span>
              </label>
            ))}
          </fieldset>
          <label className="row">
            <input type="checkbox" checked={setDefault} onChange={(e) => setSetDefault(e.target.checked)} />
            {t("wizard.where.setDefault")}
          </label>
        </section>
      )}
      {page === 5 && selected && (
        <section>
          <h1 ref={heading} tabIndex={-1}>
            {t("wizard.confirm.title")}
          </h1>
          {/* A new flow, and a new plan asking about another add-on anew, each time the page
              comes (2c: replaceOtherClink cleared for each new plan). */}
          <ApplyFlow
            request={{ themeId: selected, target, setWtDefaultProfile: setDefault, replaceOtherClink: false }}
            showBackup
            onBusy={setBusy}
            onApplied={onApplied}
          />
        </section>
      )}
      {page === 6 && (
        <section>
          <h1 ref={heading} tabIndex={-1}>
            {t("wizard.done.title")}
          </h1>
          <p>{t("wizard.done.text")}</p>
          {preview && <TerminalPreview theme={preview.theme} result={preview.result} />}
          {previewError !== null && <ErrorNotice error={previewError} />}
          <div className="row-actions">
            <button type="button" onClick={() => void tryCmd()}>
              {t("wizard.done.tryCmd")}
            </button>
            <button type="button" className="primary" onClick={() => void leave({ target, setWtDefaultProfile: setDefault })}>
              {t("wizard.done.goHome")}
            </button>
          </div>
          {openError !== null && <ErrorNotice error={openError} />}
        </section>
      )}
      {/* The apply said once, here: the flow that would say it went with page 5, and a region
          that is there before its text comes is announced (2c item 10). Page 6 shows it in its
          own words; the heading's focus reads "Done". */}
      <p role="status" className="visually-hidden">
        {applied ? t(applied.target === "wt" ? "apply.doneWt" : "apply.done", { name: applied.name }) : ""}
      </p>
      {saveError !== null && <ErrorNotice error={saveError} />}
      <div className="wizard-nav">
        <div>
          {page > 1 && page < 6 && !busy && (
            <button type="button" onClick={() => setPage(page - 1)}>
              {t("common.back")}
            </button>
          )}
        </div>
        <div className="row-actions">
          {page < 5 && (
            <button type="button" className="link" onClick={() => void leave()}>
              {t("wizard.skip")}
            </button>
          )}
          {page < 5 && (
            <button type="button" className="primary" disabled={!canNext} onClick={() => setPage(page + 1)}>
              {t("common.next")}
            </button>
          )}
        </div>
      </div>
    </>
  );
}

/**
 * What Windows Terminal gets (2c carry R11, and the founder's decision of 2026-10-17): colors when
 * an installed one has its settings; set up by the apply when the backend says an apply sets one
 * up (its `setUpByApply`, never `installed` without settings, which an unpackaged leftover matches
 * too); otherwise none is usable, and the Store is offered where it applies, or this Windows is
 * too old for it, or its version could not be read.
 */
type TerminalState = "present" | "neverOpened" | "install" | "tooOld" | "unknown";

function terminalState(report: SystemReport): TerminalState {
  if (report.windowsTerminal.some((w) => w.installed && w.hasSettings)) return "present";
  if (report.windowsTerminal.some((w) => w.setUpByApply)) return "neverOpened";
  if (report.storeAvailable) return "install";
  return report.build === null ? "unknown" : "tooOld";
}

const TERMINAL_TEXT: Record<Exclude<TerminalState, "install">, Key> = {
  present: "wizard.check.wtPresent",
  neverOpened: "wizard.check.wtNeverOpened",
  tooOld: "wizard.check.wtTooOld",
  unknown: "wizard.check.wtUnknown",
};

/** Page 2's findings (spec §4.2): the Windows build, Windows Terminal, the user's other AutoRun
 * commands (kept), another add-on (asked about before it is replaced), an add-on set up for the
 * whole computer, a computer that is not x64, and that no administrator is needed. */
export function MachineCheck({ report }: { report: SystemReport }) {
  const ipc = useIpc();
  const { t } = useI18n();
  const [openError, setOpenError] = useState<unknown>(null);
  const otherCount = report.autorun.otherCommands.length;
  const terminal = terminalState(report);

  const install = async () => {
    setOpenError(null);
    try {
      await ipc.openExternal("windowsTerminalStore");
    } catch (e) {
      setOpenError(e);
    }
  };

  return (
    <>
      <ul className="check-list">
        <li>{report.build === null ? t("wizard.check.windowsUnknown") : t("wizard.check.windows", { build: report.build })}</li>
        <li>
          {terminal === "install" ? (
            <>
              {t("wizard.check.wtMissing")}
              <div className="row-actions">
                <button type="button" onClick={() => void install()}>
                  {t("wizard.check.wtInstall")}
                </button>
                <small>{t("wizard.check.wtSkipHint")}</small>
              </div>
            </>
          ) : (
            t(TERMINAL_TEXT[terminal])
          )}
        </li>
        <li>{otherCount === 0 ? t("wizard.check.autorunNone") : t("wizard.check.autorunKept", { n: otherCount })}</li>
        {report.autorun.foreignClink.length > 0 && <li>{t("wizard.check.otherClink")}</li>}
        {report.autorun.machineWideClink && <li>{t("wizard.check.machineWideClink")}</li>}
        {report.arch !== "x64" && <li className="status-fail">{t("wizard.check.arm64")}</li>}
        <li>{t("wizard.check.noAdmin")}</li>
      </ul>
      {openError !== null && <ErrorNotice error={openError} />}
    </>
  );
}
