// Recovery mode's one page (recovery spec §5.2): why the app is here, what the --recovery entry
// did (§4.4), the pause's state with "Turn the theme back on", and the ways out, each a part Help
// already has (Undo everything, Restore points, Health check, Report a problem), plus the log
// folder, "Reset the app's settings" (settings.json set aside) and "Start normally" (a restart:
// the first screen of this run said ready, so the next start is a normal one). No library, no
// drafts, no preview, no quick switch are read here. A line is kept for phase 4's "Update now".
import { useEffect, useRef, useState } from "react";
import { Dialog } from "../components/Dialog";
import { ErrorNotice, NotesList, UnrestoredList } from "../components/Notices";
import { PausedBanner, PauseResult, useResume } from "../components/Pause";
import { useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import type { Ipc } from "../ipc/types";
import { useAppState } from "../state";
import { useRead } from "../useRead";
import { Doctor, Report, RestorePointsTab, Revert } from "./Help";

const readRecovery = (ipc: Ipc) => ipc.recoveryStatus();

export function Recovery() {
  const { t } = useI18n();
  const ipc = useIpc();
  const { appStatus, version, adoptSettings } = useAppState();
  const heading = useRef<HTMLHeadingElement>(null);
  const recovery = useRead(readRecovery, version);
  const paused = recovery.value?.paused ?? null;
  const resume = useResume(heading, paused);
  const [asking, setAsking] = useState<"reset" | null>(null);
  const [resetTo, setResetTo] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const reason = appStatus?.safeModeReason ?? null;
  const entry = appStatus?.recoveryEntry ?? null;

  // The page's heading takes focus when the page opens (spec §9).
  useEffect(() => {
    heading.current?.focus();
  }, []);

  const act = async (run: () => Promise<void>) => {
    setError(null);
    try {
      await run();
    } catch (e) {
      setError(e);
    }
  };
  const reset = () =>
    act(async () => {
      const done = await ipc.settingsReset();
      setAsking(null);
      // The defaults stand at once, the language too (no reload: the notice below is read in
      // the language the defaults give).
      adoptSettings(done);
      setResetTo(done.resetFrom);
    });

  return (
    <section>
      <h1 tabIndex={-1} ref={heading}>
        {t("recovery.title")}
      </h1>
      <p>
        {reason?.kind === "failedStarts" && t("recovery.why.failedStarts", { n: reason.count })}
        {reason?.kind === "recovery" && t("recovery.why.shortcut")}
      </p>
      {/* Phase 4's "Update now" goes here (recovery spec §5.2). */}
      <p className="update-line">{t("recovery.update.placeholder")}</p>
      {entry && (
        <div className="card">
          <p>{t("recovery.entry.did")}</p>
          <NotesList notes={entry.notes} />
          <UnrestoredList steps={entry.unrestored} live={false} />
          {entry.pause && <PauseResult outcome={entry.pause} />}
          {entry.skipped && (
            <p>
              {t("recovery.entry.skipped")} <span lang="en">{entry.skipped}</span>
            </p>
          )}
        </div>
      )}
      {recovery.error !== null && <ErrorNotice error={recovery.error} title={t("home.paused.unreadable")} />}
      {paused && <PausedBanner paused={paused} onResume={() => void resume.resume()} />}
      {resume.view}
      {error !== null && <ErrorNotice error={error} />}
      <Revert />
      <RestorePointsTab />
      <Doctor />
      <h2>{t("recovery.logs.title")}</h2>
      <div className="row-actions">
        <button type="button" onClick={() => void act(() => ipc.openExternal("logsFolder"))}>
          {t("advanced.openLogs")}
        </button>
      </div>
      <div id="recovery-report" tabIndex={-1}>
        <Report />
      </div>
      <h2>{t("recovery.reset.title")}</h2>
      <p>{t("recovery.reset.text")}</p>
      {/* Always there, its text toggled (2c item 10). */}
      <div role="status">{resetTo && <div className="notice ok">{t("recovery.reset.done", { file: resetTo })}</div>}</div>
      <div className="row-actions">
        <button type="button" onClick={() => setAsking("reset")}>
          {t("recovery.reset.button")}
        </button>
      </div>
      <h2>{t("recovery.normal.title")}</h2>
      <p>{t("recovery.normal.text")}</p>
      <div className="row-actions">
        <button type="button" className="primary" onClick={() => void act(() => ipc.appRestart())}>
          {t("recovery.normal.button")}
        </button>
      </div>
      {asking === "reset" && (
        <Dialog title={t("recovery.reset.button")} onClose={() => setAsking(null)}>
          <p>{t("recovery.reset.confirm")}</p>
          <div className="row-actions">
            <button type="button" className="primary" onClick={() => void reset()}>
              {t("recovery.reset.button")}
            </button>
            <button type="button" onClick={() => setAsking(null)}>
              {t("common.cancel")}
            </button>
          </div>
        </Dialog>
      )}
    </section>
  );
}
