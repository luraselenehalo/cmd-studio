// The "still working" bar of recovery spec §6.1 and the restart of §6.3: a status region at the
// top of the page, empty until a command has waited ten seconds, then the task's name; after
// three minutes of a change, "Restart CMD Studio", which asks first (the change that has not
// finished is put back by the journal when the app starts again) and then calls `appRestart`.
import { createContext, useContext, useState, useSyncExternalStore } from "react";
import { useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import type { WorkStore } from "../ipc/work";
import { Dialog } from "./Dialog";
import { ErrorNotice } from "./Notices";

export const WorkContext = createContext<WorkStore | null>(null);

const IDLE = { task: null, offerRestart: false };
const NONE: WorkStore = { get: () => IDLE, subscribe: () => () => {} };

export function WorkingBar() {
  const store = useContext(WorkContext) ?? NONE;
  const ipc = useIpc();
  const { t } = useI18n();
  const state = useSyncExternalStore(store.subscribe, store.get, store.get);
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState<unknown>(null);
  // Set before the call and kept on an Ok answer or none (the app is restarting either way);
  // only a rejection clears it.
  const [restarting, setRestarting] = useState(false);
  const restart = async () => {
    if (restarting) return;
    setError(null);
    setRestarting(true);
    try {
      await ipc.appRestart();
    } catch (e) {
      setError(e);
      setRestarting(false);
      setAsking(false);
    }
  };
  return (
    <>
      {/* Always there, its text toggled, so a screen reader hears the change. */}
      <div role="status" aria-label={t("work.label")} className={state.task ? "working-bar" : "working-bar empty"}>
        {state.task && <span>{t("work.still", { task: t(state.task) })}</span>}
        {state.offerRestart && (
          <button type="button" onClick={() => setAsking(true)}>
            {t("work.restart")}
          </button>
        )}
      </div>
      {error !== null && <ErrorNotice error={error} />}
      {asking && (
        <Dialog title={t("work.restart")} onClose={restarting ? undefined : () => setAsking(false)}>
          <p>{t("work.restartConfirm")}</p>
          {restarting && <p role="status">{t("work.restarting")}…</p>}
          <div className="row-actions">
            <button type="button" className="primary" disabled={restarting} onClick={() => void restart()}>
              {t("work.restart")}
            </button>
            <button type="button" disabled={restarting} onClick={() => setAsking(false)}>
              {t("common.cancel")}
            </button>
          </div>
        </Dialog>
      )}
    </>
  );
}
