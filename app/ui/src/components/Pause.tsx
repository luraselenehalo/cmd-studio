// The theme's pause (recovery spec §4.1, §4.2), shared by the home page (its banner and the
// "Pause the theme" button under the theme in use) and the health check's D12 fixes. Pausing
// takes CMD Studio's changes out of this computer after a confirmation (`theme_pause`, Revert
// everything underneath), and cmd windows then open without the theme. Turning it back on lifts
// a pause in cmd at once, and for a pause of this computer applies the paused theme again
// through the plan (`theme_resume`, then the ApplyFlow every apply goes through).
import { useEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { PauseInfo } from "../bindings/PauseInfo";
import type { PauseOutcome } from "../bindings/PauseOutcome";
import { formatTime, useI18n, type Key, type Translate } from "../i18n";
import { useIpc } from "../ipc/context";
import { useNav } from "../router";
import { useAppState } from "../state";
import { ApplyFlow } from "./ApplyFlow";
import { Dialog } from "./Dialog";
import { ErrorNotice, NotesList, UnrestoredList } from "./Notices";

/** Why the user pauses: from the home page, or from the health check after cmd's crashes. */
export type PauseAsk = "user" | "clinkCrash";

/**
 * Why the theme is paused, from the record's reason. A record that can't be read says so. A pause
 * for cmd's crashes never says "0 times" or "1 times": a count of 0 is a pause saved when the
 * crash reports could not be read, so no number is true (ruling R2).
 */
function reasonText(paused: PauseInfo, t: Translate): string {
  switch (paused.reason) {
    case null:
      return t("home.paused.reason.unknown");
    case "cmdStarts":
      return t("home.paused.reason.cmdStarts", { n: paused.count });
    case "clinkCrash":
      if (paused.count >= 2) return t("home.paused.reason.clinkCrash", { n: paused.count });
      return t(paused.count === 1 ? "home.paused.reason.clinkCrashOnce" : "home.paused.reason.clinkCrashNoCount");
    case "recovery":
      return t("home.paused.reason.recovery");
    case "user":
      return t("home.paused.reason.user");
  }
}

/**
 * How far the pause reaches now, by its level (ruling R2). A record that can't be read has no
 * reach to tell, only that turning it back on removes the note; an app record older than the theme
 * in use is lifted at once and never names the theme it noted.
 */
function reachText(paused: PauseInfo, t: Translate): string {
  if (paused.reason === null) return t("home.paused.unknownResume");
  switch (paused.level) {
    case "cmd":
      return t(paused.by === "clink" ? "home.paused.levelClink" : "home.paused.levelCmdOlder");
    case "computer":
      return paused.resume?.themeName
        ? t("home.paused.levelApp", { name: paused.resume.themeName })
        : t("home.paused.levelAppGone");
    case "computerNotYet":
      return t("home.paused.levelNotYet");
    case "computerUnknown":
      return t("home.paused.levelUnknown");
  }
}

/**
 * The banner over the home page while the theme is paused: why, since when, how far it reaches,
 * and the ways on. "Undo everything" is offered for a pause by init.lua (a record that can't be
 * read included) and not for the app's, which has taken everything out already or is about to.
 */
export function PausedBanner({ paused, onResume }: { paused: PauseInfo; onResume: () => void }) {
  const { t, language } = useI18n();
  const { go } = useNav();
  return (
    <div className="card notice warn">
      <h2 style={{ marginTop: 0 }}>{t("home.paused.title")}</h2>
      <p>{reasonText(paused, t)}</p>
      {paused.atMs !== null && <p>{t("home.paused.since", { time: formatTime(paused.atMs, language) })}</p>}
      <p>{reachText(paused, t)}</p>
      <div className="row-actions">
        <button type="button" className="primary" onClick={onResume}>
          {t("resume.button")}
        </button>
        {paused.by === "clink" && (
          <button type="button" onClick={() => go({ name: "help", tab: "revert" })}>
            {t("home.paused.revert")}
          </button>
        )}
        <button type="button" onClick={() => go({ name: "help", tab: "report" })}>
          {t("home.paused.report")}
        </button>
      </div>
    </div>
  );
}

/** Focus is nowhere: on the page's body, or on an element that has left the page. */
function focusLost(): boolean {
  const active = document.activeElement;
  return active === null || active === document.body || !active.isConnected;
}

/** Once a dialog is gone with the button that opened it (the theme it paused is out), focus goes
 * to `fallback`, the page's heading, rather than to the page's body. */
function useFocusAfterClose(open: boolean, fallback: RefObject<HTMLElement | null>) {
  const wasOpen = useRef(false);
  useEffect(() => {
    if (open) {
      wasOpen.current = true;
      return;
    }
    if (!wasOpen.current) return;
    wasOpen.current = false;
    if (focusLost()) fallback.current?.focus();
  }, [open, fallback]);
}

/** "Pause the theme": `ask(reason)` opens the confirmation; render `view` where the dialog goes. */
export function usePause(fallback: RefObject<HTMLElement | null>): { ask: (reason: PauseAsk) => void; view: ReactNode } {
  const [asked, setAsked] = useState<{ key: number; reason: PauseAsk } | null>(null);
  const keys = useRef(0);
  useFocusAfterClose(asked !== null, fallback);
  const view = asked && <PauseDialog key={asked.key} reason={asked.reason} onClose={() => setAsked(null)} />;
  return {
    ask: (reason) => {
      keys.current += 1;
      setAsked({ key: keys.current, reason });
    },
    view,
  };
}

/** The confirmation, then what the pause did. No Close while it runs (ruling R7, as every change
 * of the machine); its outcome stays in the dialog, whose box takes focus. */
function PauseDialog({ reason, onClose }: { reason: PauseAsk; onClose: () => void }) {
  const ipc = useIpc();
  const { t } = useI18n();
  const { bump, setLastApply, markRecoveryActed } = useAppState();
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ kind: "done"; outcome: PauseOutcome } | { kind: "error"; error: unknown } | null>(null);
  const running = useRef(false);
  const resultBox = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (result) resultBox.current?.focus();
  }, [result]);

  const pause = async () => {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    // From here the pause the start tried is out of date (Home stops showing it).
    markRecoveryActed();
    try {
      const outcome = await ipc.themePause(reason);
      setResult({ kind: "done", outcome });
      // The after-apply bar's theme is gone with it.
      if (outcome.revert?.status === "committed") setLastApply(null);
    } catch (e) {
      setResult({ kind: "error", error: e });
    } finally {
      running.current = false;
      setBusy(false);
      bump();
    }
  };

  return (
    <Dialog title={t("pause.title")} onClose={busy ? undefined : onClose}>
      {result === null && (
        <>
          <p>{t("pause.confirm")}</p>
          <div className="row-actions">
            <button type="button" className="danger" disabled={busy} onClick={() => void pause()}>
              {t("pause.button")}
            </button>
          </div>
        </>
      )}
      {/* Always there, its text toggled (2c item 10). */}
      <div role="status">{busy ? t("pause.working") : ""}</div>
      {result && (
        <div ref={resultBox} tabIndex={-1}>
          {result.kind === "error" ? <ErrorNotice error={result.error} live={false} /> : <PauseResult outcome={result.outcome} />}
        </div>
      )}
    </Dialog>
  );
}

/**
 * The pause's headline (ruling R3, corrected). The backend's `pausedNow` is "Revert everything
 * ended well and the record was saved", so it alone is "paused". Otherwise: the record could not
 * be saved (the changes are out when Revert everything committed, else nothing was changed and cmd
 * still opens with the theme); or Revert everything committed with a step that failed (the theme
 * is out, the record is saved, and the pause is one of this computer; a step needs a look); or the
 * changes are not out (refused, rolled back, or a rollback not complete): the record is there, cmd
 * opens without the theme, and the app tries again at its next start.
 */
function pauseHeadline(outcome: PauseOutcome): { tone: "ok" | "warn"; key: Key } {
  if (outcome.pausedNow) return { tone: "ok", key: "pause.done" };
  const out = outcome.revert?.status === "committed";
  if (outcome.notSaved) return { tone: "warn", key: out ? "pause.notSaved" : "pause.failed" };
  return { tone: "warn", key: out ? "pause.notOk" : "pause.notYet" };
}

/**
 * What a pause did: the headline, then why it did not go through or what could not be put back
 * (the unrestored steps right after the headline, whose "see below" points at them). `live` makes
 * the error notices alerts, for a place where no box around them takes focus (the home page).
 */
export function PauseResult({ outcome, live = false }: { outcome: PauseOutcome; live?: boolean }) {
  const { t } = useI18n();
  const { tone, key } = pauseHeadline(outcome);
  const failed = outcome.revert?.steps.filter((s) => !s.ok && s.error) ?? [];
  return (
    <>
      <div className={`notice ${tone}`}>{t(key)}</div>
      {outcome.revert && <UnrestoredList steps={outcome.revert.unrestored} live={live} />}
      {failed.map((s, i) => (
        <ErrorNotice key={i} error={s.error} live={live} />
      ))}
      {outcome.revert?.error && <ErrorNotice error={outcome.revert.error} live={live} />}
      {outcome.refused && <ErrorNotice error={outcome.refused} live={live} />}
      {outcome.notSaved && <ErrorNotice error={outcome.notSaved} live={live} />}
      {outcome.revert && <NotesList notes={outcome.revert.notes} />}
    </>
  );
}

/**
 * "Turn the theme back on": `resume()` asks the backend, which lifts a pause in cmd at once
 * (said in the status line of `view`), gives a request for a pause of this computer (applied
 * through the plan in a dialog of `view`), or sends the user to the Themes page when the paused
 * theme is gone. `reset()` takes the status line and an error away, for when something else is
 * done on the page (a new pause), so "the theme is back on" is never left beside it. After a lift
 * at once, the button that was pressed goes with its banner on the next read: focus then goes to
 * `fallback`, as it does when a dialog goes with the button that opened it.
 */
export function useResume(fallback: RefObject<HTMLElement | null>): { resume: () => Promise<void>; reset: () => void; view: ReactNode } {
  const ipc = useIpc();
  const { t } = useI18n();
  const { go } = useNav();
  const { bump, setLastApply, markRecoveryActed } = useAppState();
  const [applying, setApplying] = useState<{ key: number; request: ApplyRequest } | null>(null);
  const [applyBusy, setApplyBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [resumed, setResumed] = useState(false);
  const keys = useRef(0);
  const running = useRef(false);
  /** The element that had focus when the pause was lifted at once, until it has left the page. */
  const liftedFrom = useRef<Element | null>(null);
  useFocusAfterClose(applying !== null, fallback);
  useEffect(() => {
    const from = liftedFrom.current;
    if (from === null || from.isConnected) return;
    liftedFrom.current = null;
    if (focusLost()) fallback.current?.focus();
  });

  const reset = () => {
    setError(null);
    setResumed(false);
  };

  const resume = async () => {
    // One at a time: a second click while the first is asked would find nothing paused.
    if (running.current) return;
    running.current = true;
    reset();
    // From here the pause the start tried is out of date (Home stops showing it).
    markRecoveryActed();
    try {
      const outcome = await ipc.themeResume();
      if (outcome.kind === "resumed") {
        const active = document.activeElement;
        liftedFrom.current = active === document.body ? null : active;
        setResumed(true);
        bump();
      } else if (outcome.kind === "apply") {
        keys.current += 1;
        setApplyBusy(false);
        setApplying({ key: keys.current, request: outcome.request });
      } else if (outcome.kind === "pickTheme") {
        go({ name: "themes" });
      } else {
        bump();
      }
    } catch (e) {
      setError(e);
    } finally {
      running.current = false;
    }
  };
  const close = () => {
    setApplying(null);
    setApplyBusy(false);
  };

  const view = (
    <>
      {/* Always there, its text toggled (2c item 10). */}
      <div role="status">{resumed && <p>{t("resume.done")}</p>}</div>
      {error !== null && <ErrorNotice error={error} />}
      {applying && (
        // No Close while the apply runs (ruling R7), and the outcome is read in the dialog.
        <Dialog title={t("resume.title")} onClose={applyBusy ? undefined : close}>
          <ApplyFlow
            key={applying.key}
            titled={false}
            request={applying.request}
            onBusy={setApplyBusy}
            onCancel={close}
            onApplied={(outcome, request) => {
              setLastApply({ outcome, target: request.target });
              bump();
            }}
          />
        </Dialog>
      )}
    </>
  );
  return { resume, reset, view };
}
