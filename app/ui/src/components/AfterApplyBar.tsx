// The after-apply bar (spec §4.4), for every shell: this plan's interim one and phase 3's final
// one. "Theme X applied · open a new cmd window…" (a new cmd tab in Windows Terminal when that is
// where the theme went) with Undo, which goes back to that apply's own restore point (ruling R3).
// Where the theme went is the target of the request that was applied, kept with the outcome in
// the app state (item 12), so an apply with its own request (a doctor fix) is worded right.
import { useEffect, useRef, useState } from "react";
import type { RestoreOutcome } from "../bindings/RestoreOutcome";
import { pick, useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import { useAppState } from "../state";
import { ErrorNotice, NotesList, UnrestoredList } from "./Notices";

export type UndoResult = { kind: "restore"; outcome: RestoreOutcome } | { kind: "error"; error: unknown };

/**
 * Undo is offered for a committed apply only: a rolled-back one carries a restore point too, but
 * nothing of it is left to undo. What the undo did stays on the bar after the apply it undid is
 * gone from it (ruling R4), and takes focus, since the Undo button pressed may be gone with it.
 *
 * The applied text is not a live region: every apply ends in an `ApplyFlow`, whose outcome is
 * the one announcement of it (fix 12).
 */
export function AfterApplyBar() {
  const ipc = useIpc();
  const { t } = useI18n();
  const { lastApply, setLastApply, bump } = useAppState();
  const [result, setResult] = useState<UndoResult | null>(null);
  const [busy, setBusy] = useState(false);
  const resultBox = useRef<HTMLDivElement>(null);

  // A new apply replaces what the last undo said.
  useEffect(() => {
    if (lastApply) setResult(null);
  }, [lastApply]);

  // What the undo said takes focus (fix 9).
  useEffect(() => {
    if (result) resultBox.current?.focus();
  }, [result]);

  const applied = lastApply?.outcome;
  const offer = applied && applied.status === "committed" && applied.snapshotId ? applied : null;
  const target = lastApply?.target;
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
        {offer && <span>{t(target === "wt" ? "apply.doneWt" : "apply.done", { name: offer.themeName })}</span>}
        {result && (
          <div ref={resultBox} tabIndex={-1}>
            <UndoResultView result={result} />
          </div>
        )}
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
export function UndoResultView({ result }: { result: UndoResult }) {
  const { t, language } = useI18n();
  if (result.kind === "error") return <ErrorNotice error={result.error} />;
  const { outcome } = result;
  const last = outcome.last;
  const notes = [...outcome.notes, ...(last?.notes ?? [])];
  if (outcome.status === "done") {
    // No live region: the notice takes focus, which reads it.
    return (
      <>
        <div className="notice ok">
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
        {/* The very first undo was refused (fix 8): nothing was undone, rather than "part-way". */}
        {outcome.undone.length === 0 ? t("apply.undoRefused") : t("apply.undoStopped")}
      </div>
      {last?.error && <ErrorNotice error={last.error} />}
      <NotesList notes={notes} />
    </>
  );
}
