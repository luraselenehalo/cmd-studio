// The updater's offer (spec §6.2): on the home page, a card when a newer version waits for "Update
// now" (notify mode) or is downloaded and installs at close (automatic mode); on the Recovery page,
// one line that says Update now when a version waits, else that none waits (recovery spec §5.2).
// The check of a run starts only after app_ready, after this page's first read, so the offer reads
// update_status again every two seconds while the check has not run (`checkedAtMs` null) or is under
// way, for at most a minute (Q5); Off and a settled check end the reads. "Update now" hands the
// install to the backend, which ends the process and brings the app back by itself as a normal
// start; the card says so while that happens. An E_BUSY whose re-read finds the update downloading
// or installing is a second click while one is under way (Task 3's review): said as "already", not
// as an error; any other refusal is shown as an error. "Later" (home only) hides the offer for the
// run.
import { useEffect, useState } from "react";
import type { UpdateStatus } from "../bindings/UpdateStatus";
import { useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import { IpcError } from "../ipc/types";
import { useAppState } from "../state";
import { ErrorNotice } from "./Notices";

/** How often the offer reads `update_status` while the check of this run may still change it. */
export const UPDATE_POLL_MS = 2000;
/** How many reads at most: a minute, then the offer stays as it last read it. */
export const UPDATE_POLL_LIMIT = 30;

/** Whether the updater may still change: the check has not run, or is checking or downloading. */
function stillComing(status: UpdateStatus): boolean {
  if (status.state.kind === "off") return false;
  return status.checkedAtMs === null || status.state.kind === "checking" || status.state.kind === "downloading";
}

/** Whether the updater is busy with an update already: an E_BUSY then is a second click. */
function underWay(status: UpdateStatus): boolean {
  return status.state.kind === "downloading" || status.state.kind === "installing";
}

export function UpdateOffer({ where }: { where: "home" | "recovery" }) {
  const { t } = useI18n();
  const ipc = useIpc();
  const { version: changes, updateLater, markUpdateLater } = useAppState();
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);
  const [already, setAlready] = useState(false);
  const [error, setError] = useState<unknown>(null);

  // One read, and more while the check may still change the answer; a read that fails is silent
  // (the updater says nothing until it has something) and read again on the next tick.
  useEffect(() => {
    let alive = true;
    let reads = 0;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const again = () => {
      if (alive && reads < UPDATE_POLL_LIMIT) timer = setTimeout(read, UPDATE_POLL_MS);
    };
    const read = () => {
      reads += 1;
      ipc.updateStatus().then(
        (s) => {
          if (!alive) return;
          setStatus(s);
          if (stillComing(s)) again();
        },
        () => again(),
      );
    };
    read();
    return () => {
      alive = false;
      if (timer !== null) clearTimeout(timer);
    };
  }, [ipc, changes]);

  const state = status?.state ?? null;
  const version = state && "version" in state ? state.version : null;
  const updateNow = (v: string) => {
    setError(null);
    setAlready(false);
    setInstalling(v);
    ipc.updateInstall().catch(async (e: unknown) => {
      if (e instanceof IpcError && e.code === "E_BUSY") {
        const now = await ipc.updateStatus().catch(() => null);
        if (now !== null) setStatus(now);
        if (now !== null && underWay(now)) {
          setInstalling(null);
          setAlready(true);
          return;
        }
      }
      setInstalling(null);
      setError(e);
    });
  };
  if (already) {
    return (
      <div className="notice" role="status">
        {t("home.update.already")}
      </div>
    );
  }
  if (installing !== null || state?.kind === "installing") {
    const v = installing ?? version ?? "";
    return (
      <div className="notice" role="status">
        {t("home.update.installing", { version: v })}
      </div>
    );
  }
  const offered = state?.kind === "available" || state?.kind === "ready";
  if (!offered || version === null || (where === "home" && updateLater)) {
    return where === "recovery" ? <p className="update-line">{t("recovery.update.none")}</p> : null;
  }
  const text =
    where === "recovery"
      ? t("recovery.update.available", { version })
      : state.kind === "ready"
        ? t("home.update.ready", { version })
        : t("home.update.available", { version });
  return (
    <div className={where === "home" ? "notice" : "update-line"} role="status">
      <p>{text}</p>
      <div className="row-actions">
        <button type="button" className="primary" onClick={() => updateNow(version)}>
          {t("home.update.now")}
        </button>
        {where === "home" && (
          <button type="button" onClick={markUpdateLater}>
            {t("home.update.later")}
          </button>
        )}
      </div>
      {error !== null && <ErrorNotice error={error} />}
    </div>
  );
}
