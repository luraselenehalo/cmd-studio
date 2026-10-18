// Help (spec §4.3, §6.6): six tabs, each a route of its own, shown as toggle buttons in a named
// group rather than the tab pattern, so choosing one leaves focus on it (ruling Q4, as the
// editor's modes). The health check with the fix the backend offers for each finding (the F2
// mapping: an apply goes through the plan, the Store and a new cmd window open from here, and
// the step only the user can take is said in words); every restore point with "Go back to
// this point"; "Undo everything"; "Report a problem" (the diagnostics, the user name replaced,
// copied for a GitHub issue); the short guide; the credits and licenses. Every failure, a window
// that could not be opened included, is shown (spec §5.9).
import { useEffect, useRef, useState } from "react";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { CheckReport } from "../bindings/CheckReport";
import type { DoctorReport } from "../bindings/DoctorReport";
import type { FixKind } from "../bindings/FixKind";
import type { RestoreOutcome } from "../bindings/RestoreOutcome";
import type { RestorePoint } from "../bindings/RestorePoint";
import type { RestorePoints } from "../bindings/RestorePoints";
import type { RevertOutcome } from "../bindings/RevertOutcome";
import { ApplyFlow, useCopyDiagnostics } from "../components/ApplyFlow";
import { Dialog } from "../components/Dialog";
import { ErrorNotice, Notice, NotesList, UnrestoredList } from "../components/Notices";
import { formatTime, pick, pickedLanguage, useI18n, type Key, type Language, type Translate } from "../i18n";
import { useIpc } from "../ipc/context";
import { IpcError } from "../ipc/types";
import { useNav, type HelpTab } from "../router";
import { useAppState } from "../state";
import { BUNDLED, SCHEMES, type Credit } from "./credits";

const TABS: HelpTab[] = ["doctor", "restore", "revert", "report", "guide", "credits"];

/** The fixes this page runs (F2): an apply through the plan, the Store, a new cmd window.
 * Reinstalling only the user can do: it is said, not a button. A Windows Terminal that was
 * never opened needs nothing of the user's, since an apply sets it up (founder 2026-10-17). */
const RUNNABLE: ReadonlySet<FixKind> = new Set<FixKind>([
  "reapply",
  "replaceOtherClink",
  "installWindowsTerminal",
  "openNewCmdWindow",
]);

export function Help({ tab }: { tab: HelpTab }) {
  const { t } = useI18n();
  const { go } = useNav();
  return (
    <section>
      <h1 tabIndex={-1}>{t("help.title")}</h1>
      <div className="tabs" role="group" aria-label={t("help.tabs.label")}>
        {TABS.map((name) => (
          <button key={name} type="button" aria-pressed={tab === name} onClick={() => go({ name: "help", tab: name })}>
            {t(`help.tabs.${name}`)}
          </button>
        ))}
      </div>
      {tab === "doctor" && <Doctor />}
      {tab === "restore" && <RestorePointsTab />}
      {tab === "revert" && <Revert />}
      {tab === "report" && <Report />}
      {tab === "guide" && <Guide />}
      {tab === "credits" && <Credits />}
    </section>
  );
}

function Doctor() {
  const ipc = useIpc();
  const { t, language } = useI18n();
  const { version, bump, setLastApply } = useAppState();
  const [report, setReport] = useState<DoctorReport | null>(null);
  const [loadError, setLoadError] = useState<unknown>(null);
  /** A check is running: said in a status region that is always there (2c item 10). */
  const [checking, setChecking] = useState(true);
  /** What the last fix said went wrong: a window that could not be opened, a failed proposal. */
  const [fixError, setFixError] = useState<unknown>(null);
  /** A proposal no longer fitted its check: the computer changed, so the checks ran again. */
  const [changed, setChanged] = useState(false);
  const [applying, setApplying] = useState<{ key: number; request: ApplyRequest } | null>(null);
  const [applyBusy, setApplyBusy] = useState(false);
  const [tick, setTick] = useState(0);
  const applies = useRef(0);
  const heading = useRef<HTMLHeadingElement>(null);
  /** The fix's dialog was just closed: where focus lands is checked once it is gone. */
  const closing = useRef(false);

  useEffect(() => {
    let alive = true;
    setChecking(true);
    setLoadError(null);
    ipc.doctorRun().then(
      (r) => {
        if (!alive) return;
        setReport(r);
        setChecking(false);
      },
      (e: unknown) => {
        if (!alive) return;
        // No list from an earlier check under this failure: it would read as this check's.
        setReport(null);
        setLoadError(e);
        setChecking(false);
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc, version, tick]);

  // The dialog gives focus back to the fix's button; when that button went with its finding
  // (the apply fixed it), focus goes to this tab's heading rather than to the page's body.
  useEffect(() => {
    if (applying !== null || !closing.current) return;
    closing.current = false;
    const active = document.activeElement;
    if (active === null || active === document.body || !active.isConnected) heading.current?.focus();
  }, [applying]);

  const checkAgain = () => {
    setChanged(false);
    setFixError(null);
    setTick((n) => n + 1);
  };

  /** The checks as they are now, said: the fix proposed for this one is not the one shown. */
  const changedSince = () => {
    setChanged(true);
    setTick((n) => n + 1);
  };

  // Each proposal mounts a flow of its own (a new key), and the dialog starts closable.
  const startApply = (request: ApplyRequest) => {
    applies.current += 1;
    setApplyBusy(false);
    setApplying({ key: applies.current, request });
  };
  const closeApply = () => {
    closing.current = true;
    setApplying(null);
    setApplyBusy(false);
  };

  const fix = async (check: CheckReport) => {
    setFixError(null);
    setChanged(false);
    try {
      // doctor_fix checks again: what it proposes is the fix for the computer as it is now.
      const { kind, request } = await ipc.doctorFix(check.id);
      if (kind !== check.fix) {
        changedSince();
        return;
      }
      if (kind === "reapply" || kind === "replaceOtherClink") {
        if (!request) {
          changedSince();
          return;
        }
        // The page is the user's yes to replacing the other add-on: it lists its commands, and
        // the plan lists them again before Apply. Applying again asks anew (2c: cleared for
        // each new plan).
        startApply({ ...request, replaceOtherClink: kind === "replaceOtherClink" });
      } else if (kind === "installWindowsTerminal") {
        await ipc.openExternal("windowsTerminalStore");
      } else if (kind === "openNewCmdWindow") {
        await ipc.openCmd();
      }
    } catch (e) {
      setFixError(e);
    }
  };

  return (
    <div>
      <h2 ref={heading} tabIndex={-1}>
        {t("help.tabs.doctor")}
      </h2>
      <div className="row-actions">
        <button type="button" onClick={checkAgain}>
          {t("help.doctor.run")}
        </button>
        {report && <span>{t("help.doctor.checkedAt", { time: formatTime(report.checkedAtMs, language) })}</span>}
        {/* Always there, their text toggled (2c item 10). */}
        <span role="status">{checking ? t("help.doctor.checking") : ""}</span>
        <span role="status">{changed ? t("help.doctor.fixChanged") : ""}</span>
      </div>
      {loadError !== null && <ErrorNotice error={loadError} />}
      {fixError !== null && <ErrorNotice error={fixError} />}
      {report && (
        <ul className="check-list">
          {report.checks.map((c) => (
            <CheckItem key={c.id} check={c} onFix={() => void fix(c)} />
          ))}
        </ul>
      )}
      {applying && (
        // No Close while the apply runs (ruling R7), and the dialog stays open once it committed:
        // its outcome is read there, since the after-apply bar is no live region.
        <Dialog title={t("apply.planTitle")} onClose={applyBusy ? undefined : closeApply}>
          <ApplyFlow
            key={applying.key}
            titled={false}
            request={applying.request}
            onBusy={setApplyBusy}
            onCancel={closeApply}
            onApplied={(outcome, request) => {
              setLastApply({ outcome, target: request.target });
              bump();
            }}
          />
        </Dialog>
      )}
    </div>
  );
}

/** One check: its id and status, its message in the UI's language, its code, the details (Thai
 * when there are Thai ones and the UI is Thai, else the core's English, marked as English so a
 * screen reader reads it in that voice, spec §4.5), and its fix as a button or in words. */
function CheckItem({ check: c, onFix }: { check: CheckReport; onFix: () => void }) {
  const { t, language } = useI18n();
  const thai = language === "th" && c.detailsTh ? c.detailsTh : null;
  const details = thai ?? c.details;
  return (
    <li>
      <div>
        <strong className={`status-${c.status}`}>
          {c.id} · {t(`help.doctor.status.${c.status}`)}
        </strong>{" "}
        <span lang={pickedLanguage(language, c.messageTh)}>{pick(language, c.messageTh, c.messageEn)}</span>
        {c.code && (
          <>
            {" "}
            <span className="badge">{c.code}</span>
          </>
        )}
      </div>
      {details && (
        <details>
          <summary>{t("common.details")}</summary>
          <pre lang={thai ? undefined : "en"}>{details}</pre>
        </details>
      )}
      {c.fix &&
        (RUNNABLE.has(c.fix) ? (
          <div className="row-actions">
            <button type="button" onClick={onFix}>
              {t(`help.doctor.fix.${c.fix}`)}
            </button>
          </div>
        ) : (
          <p>
            <strong>{t("help.whatToDo")}</strong> <span>{t(`help.doctor.fix.${c.fix}`)}</span>
          </p>
        ))}
    </li>
  );
}

/** A restore point in words: its theme (or what made it) and when. */
function pointLabel(p: RestorePoint, t: Translate, language: Language): string {
  return `${p.theme ? p.theme.name : t(`help.restore.kind.${p.kind}`)} · ${formatTime(p.createdMs, language)}`;
}

/** What a restore did, and the points as they were before it, to name the applies it undid. */
type RestoreResult = { kind: "restore"; outcome: RestoreOutcome; before: RestorePoint[] } | { kind: "error"; error: unknown };

function RestorePointsTab() {
  const ipc = useIpc();
  const { t, language } = useI18n();
  const { version, bump, lastApply, setLastApply } = useAppState();
  const [points, setPoints] = useState<RestorePoints | null>(null);
  const [loadError, setLoadError] = useState<unknown>(null);
  const [confirming, setConfirming] = useState<RestorePoint | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<RestoreResult | null>(null);
  const running = useRef(false);
  const resultBox = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let alive = true;
    ipc.snapshotsList().then(
      (p) => {
        if (!alive) return;
        setPoints(p);
        setLoadError(null);
      },
      (e: unknown) => {
        if (alive) setLoadError(e);
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc, version]);

  // What the restore did takes focus: the button pressed went with the dialog, or with its point.
  useEffect(() => {
    if (result) resultBox.current?.focus();
  }, [result]);

  const restore = async (point: RestorePoint) => {
    setConfirming(null);
    // One restore at a time: the backend's lock would answer a second with E_BUSY.
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setResult(null);
    const before = points?.points ?? [];
    const barApply = lastApply?.outcome.snapshotId ?? null;
    try {
      const outcome = await ipc.snapshotRestore(point.id);
      setResult({ kind: "restore", outcome, before });
      // The after-apply bar's apply is gone from it once this restore undid it.
      if (barApply && outcome.undone.includes(barApply)) setLastApply(null);
    } catch (e) {
      setResult({ kind: "error", error: e });
    } finally {
      running.current = false;
      setBusy(false);
      bump();
    }
  };

  return (
    <div>
      <h2>{t("help.restore.title")}</h2>
      {loadError !== null && <ErrorNotice error={loadError} />}
      <div role="status">{busy ? t("help.restore.working") : ""}</div>
      {result && (
        <div ref={resultBox} tabIndex={-1}>
          {result.kind === "error" ? (
            <ErrorNotice error={result.error} live={false} />
          ) : (
            <RestoreResultView outcome={result.outcome} before={result.before} />
          )}
        </div>
      )}
      {points && points.problems.length > 0 && (
        <Notice kind="warn">
          {t("help.restore.problems", { n: points.problems.length })}
          <NotesList notes={points.problems} />
        </Notice>
      )}
      {!points && loadError === null && <p role="status">{t("common.loading")}</p>}
      {points && points.points.length === 0 && <p>{t("help.restore.none")}</p>}
      {points && points.points.length > 0 && (
        <ul className="check-list">
          {points.points.map((p) => (
            <li key={p.id}>
              <div>
                <strong>{t(`help.restore.kind.${p.kind}`)}</strong>
                {p.theme && <span> · {p.theme.name}</span>} · <span>{formatTime(p.createdMs, language)}</span>
                {p.undoneBy && (
                  <>
                    {" "}
                    <span className="badge">{t("help.restore.undone")}</span>
                  </>
                )}
              </div>
              <small>{p.id}</small>
              {p.restorable && (
                // Named by its point, so a screen reader says which point this Go back is for.
                <div className="row-actions" role="group" aria-label={pointLabel(p, t, language)}>
                  <button type="button" disabled={busy} onClick={() => setConfirming(p)}>
                    {t("help.restore.restore")}
                  </button>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
      {confirming && (
        <Dialog title={t("help.restore.restore")} onClose={() => setConfirming(null)}>
          <p>{t("help.restore.confirm")}</p>
          <p>
            <strong>{pointLabel(confirming, t, language)}</strong>
          </p>
          <div className="row-actions">
            <button type="button" className="primary" onClick={() => void restore(confirming)}>
              {t("help.restore.restore")}
            </button>
          </div>
        </Dialog>
      )}
    </div>
  );
}

/**
 * A restore's headline, by what it did, as the bar's undo words it (2c fix 8): a stop whose
 * rollback did not finish has things to fix by hand; one that undid nothing changed nothing (the
 * first undo refused or rolled back); "part-way" only once something was undone.
 */
function restoreHeadline(outcome: RestoreOutcome): { tone: "ok" | "warn" | "error"; key: Key } {
  if (outcome.status === "done") return { tone: "ok", key: "help.restore.done" };
  if (outcome.status === "notRestorable") return { tone: "warn", key: "help.restore.notRestorable" };
  if (outcome.last?.status === "rollbackIncomplete") return { tone: "error", key: "help.restore.incomplete" };
  if (outcome.undone.length === 0) return { tone: "warn", key: "help.restore.nothingChanged" };
  return { tone: "warn", key: "help.restore.stopped" };
}

/**
 * How a restore went, worded for this page, with what the bar's undo shows as well (2c carry):
 * the fonts it left, what a rollback could not put back (right after the headline, whose "Fix
 * these by hand:" points at it), the applies it undid, why it stopped, and the notes of its start
 * and of its last undo with their details. No live region: the box around it takes focus, which
 * reads it (item 11).
 */
function RestoreResultView({ outcome, before }: { outcome: RestoreOutcome; before: RestorePoint[] }) {
  const { t, language } = useI18n();
  const last = outcome.last;
  const notes = [...outcome.notes, ...(last?.notes ?? [])];
  const { tone, key } = restoreHeadline(outcome);
  return (
    <>
      <div className={`notice ${tone}`}>
        {t(key)}
        {last && last.fontsLeft.length > 0 && (
          <ul className="plain-list">
            {last.fontsLeft.map((f, i) => (
              <li key={i} lang={pickedLanguage(language, f.messageTh)}>
                {pick(language, f.messageTh, f.messageEn)}
              </li>
            ))}
          </ul>
        )}
      </div>
      {last && <UnrestoredList steps={last.unrestored} live={false} />}
      {outcome.undone.length > 0 && (
        <div>
          <p>{t("help.restore.undoneTitle")}</p>
          <ul className="plain-list">
            {outcome.undone.map((id) => {
              const p = before.find((b) => b.id === id);
              return <li key={id}>{p ? pointLabel(p, t, language) : id}</li>;
            })}
          </ul>
        </div>
      )}
      {last?.error && <ErrorNotice error={last.error} live={false} />}
      <NotesList notes={notes} />
    </>
  );
}

type RevertResult = { kind: "revert"; outcome: RevertOutcome } | { kind: "error"; error: unknown };

function Revert() {
  const ipc = useIpc();
  const { t } = useI18n();
  const { bump, setLastApply } = useAppState();
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<RevertResult | null>(null);
  const running = useRef(false);
  const resultBox = useRef<HTMLDivElement>(null);

  // What the revert did takes focus, as the restore's does.
  useEffect(() => {
    if (result) resultBox.current?.focus();
  }, [result]);

  const revert = async () => {
    setConfirming(false);
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setResult(null);
    try {
      const outcome = await ipc.revertAll();
      setResult({ kind: "revert", outcome });
      // Only a revert that committed took the theme away: a rolled-back one left it applied.
      if (outcome.status === "committed") setLastApply(null);
    } catch (e) {
      setResult({ kind: "error", error: e });
    } finally {
      running.current = false;
      setBusy(false);
      bump();
    }
  };

  return (
    <div>
      <h2>{t("help.revert.title")}</h2>
      <p>{t("help.revert.text")}</p>
      <div className="row-actions">
        <button type="button" className="danger" disabled={busy} onClick={() => setConfirming(true)}>
          {t("help.revert.button")}
        </button>
        <span role="status">{busy ? t("help.revert.working") : ""}</span>
      </div>
      {result && (
        <div ref={resultBox} tabIndex={-1}>
          {result.kind === "error" ? (
            <ErrorNotice error={result.error} live={false} />
          ) : (
            <RevertResultView outcome={result.outcome} />
          )}
        </div>
      )}
      {confirming && (
        <Dialog title={t("help.revert.title")} onClose={() => setConfirming(false)}>
          <p>{t("help.revert.confirm")}</p>
          <div className="row-actions">
            <button type="button" className="danger" onClick={() => void revert()}>
              {t("help.revert.button")}
            </button>
          </div>
        </Dialog>
      )}
    </div>
  );
}

/**
 * A revert's headline, by how it ended: a rolled-back one put everything back, so nothing
 * changed and the theme stays; one whose rollback did not finish has things to fix by hand; one
 * that committed with a step that failed did not undo everything.
 */
function revertHeadline(outcome: RevertOutcome): { tone: "ok" | "warn" | "error"; key: Key } {
  if (outcome.ok) return { tone: "ok", key: "help.revert.done" };
  if (outcome.status === "rolledBack") return { tone: "error", key: "help.revert.rolledBack" };
  if (outcome.status === "rollbackIncomplete") return { tone: "error", key: "help.revert.incomplete" };
  return { tone: "warn", key: "help.revert.notOk" };
}

/**
 * Each step done or not in the UI's language; a failed one says why through its error (its
 * code's row of spec §5.9 in Thai or English) and what to do. What the step was about and what
 * happened are the core's English, behind "Show details" (ruling Q5) and marked as English.
 */
function RevertResultView({ outcome }: { outcome: RevertOutcome }) {
  const { t, language } = useI18n();
  const { tone, key } = revertHeadline(outcome);
  return (
    <>
      <div className={`notice ${tone}`}>{t(key)}</div>
      {/* Right after the headline, whose "Fix these by hand:" points at it. */}
      <UnrestoredList steps={outcome.unrestored} live={false} />
      {outcome.error && <ErrorNotice error={outcome.error} live={false} />}
      <ol className="check-list">
        {outcome.steps.map((s, i) => (
          <li key={i}>
            <span className={s.ok ? "status-ok" : "status-fail"}>
              {t(s.ok ? "help.revert.stepOk" : "help.revert.stepFailed", { n: i + 1 })}
            </span>
            {!s.ok && s.error && <ErrorNotice error={s.error} live={false} />}
            {!s.ok && (s.fixEn || s.fixTh) && (
              <p>
                <strong>{t("help.whatToDo")}</strong> <span lang={pickedLanguage(language, s.fixTh ?? "")}>{pick(language, s.fixTh ?? "", s.fixEn ?? "")}</span>
              </p>
            )}
            <details>
              <summary>{t("common.details")}</summary>
              <pre lang="en">{`${s.what}: ${s.detail}`}</pre>
            </details>
          </li>
        ))}
      </ol>
      <NotesList notes={outcome.notes} />
    </>
  );
}

function Report() {
  const ipc = useIpc();
  const { t } = useI18n();
  const [text, setText] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<unknown>(null);
  const [openError, setOpenError] = useState<unknown>(null);
  // The same copy as the apply's "Copy details", saying the same way whether it copied.
  const { copied, copy } = useCopyDiagnostics();

  useEffect(() => {
    let alive = true;
    ipc.diagnosticsCopy().then(
      (d) => {
        if (alive) setText(d);
      },
      (e: unknown) => {
        if (alive) setLoadError(e);
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc]);

  const openIssues = async () => {
    setOpenError(null);
    try {
      await ipc.openExternal("gitHubIssues");
    } catch (e) {
      setOpenError(e);
    }
  };

  // The page then shows exactly what went to the clipboard, newer than what it read at first.
  const copyShown = async () => {
    const sent = await copy();
    if (sent === null) return;
    setText(sent);
    setLoadError(null);
  };

  return (
    <div>
      <h2>{t("help.report.title")}</h2>
      <p>{t("help.report.text")}</p>
      <div className="row-actions">
        <button type="button" className="primary" onClick={() => void copyShown()}>
          {t("help.report.copy")}
        </button>
        <button type="button" onClick={() => void openIssues()}>
          {t("help.report.issues")}
        </button>
        {/* Always there, its text toggled (2c item 10). */}
        <span role="status">{copied === "yes" ? t("help.report.copied") : ""}</span>
      </div>
      {copied instanceof IpcError && <ErrorNotice error={copied} title={t("apply.copyFailed")} />}
      {openError !== null && <ErrorNotice error={openError} />}
      {loadError !== null && <ErrorNotice error={loadError} />}
      {text === null && loadError === null && <p role="status">{t("common.loading")}</p>}
      {text !== null && (
        <pre className="log" lang="en">
          {text}
        </pre>
      )}
    </div>
  );
}

function Guide() {
  const { t } = useI18n();
  return (
    <div>
      <h2>{t("help.guide.title")}</h2>
      {(["p1", "p2", "p3", "p4", "p5"] as const).map((p) => (
        <p key={p}>{t(`help.guide.${p}`)}</p>
      ))}
    </div>
  );
}

/** Names, licenses and copyright lines as their projects write them: English in both UIs. */
function CreditList({ items }: { items: Credit[] }) {
  return (
    <ul className="check-list" lang="en">
      {items.map((c) => (
        <li key={c.name}>
          <strong>{c.name}</strong>
          {c.version && ` ${c.version}`} · {c.license}
          <div>
            <small>{c.copyright}</small>
          </div>
          <div>
            <small>
              <code>{c.source}</code>
            </small>
          </div>
        </li>
      ))}
    </ul>
  );
}

function Credits() {
  const { t } = useI18n();
  return (
    <div>
      <h2>{t("help.credits.title")}</h2>
      <p>{t("help.credits.intro")}</p>
      <h3>{t("help.credits.bundled")}</h3>
      <CreditList items={BUNDLED} />
      <h3>{t("help.credits.schemes")}</h3>
      <CreditList items={SCHEMES} />
      {/* No SignPath line until Phase 4 signs the builds (ruling Q6; docs/credits.md: show it
          once signing through SignPath Foundation is set up). Its text stays in the
          dictionaries as help.credits.signpath. */}
    </div>
  );
}
