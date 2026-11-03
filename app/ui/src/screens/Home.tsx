// The home page (spec §4.3): the status from the real checks with the time they ran, the theme in
// use with its preview and Customize / Change theme / Open cmd, a quick switch to the five themes
// used most recently (through the plan, as every apply), and the latest restore point. A line is
// kept for Phase 4's "Updated to vX" notice (`home.update.placeholder`). Each part loads on its
// own, so one that fails is said and the others still show; a state.json that could not be read
// is said as such, never as "no theme is applied" (spec §5.8 D9). While the theme is paused a
// banner says why, since when and how far it reaches, with "Turn the theme back on"; under the
// theme in use, "Pause the theme" asks first (recovery spec §4.2). A pause the start tried that did
// not go through is shown at once, and a start that took the theme out while this page was reading
// makes it read again, once (§4.3). The pause is asked for once, never on a timer.
import { useEffect, useRef, useState } from "react";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { SampleIdentity } from "../bindings/SampleIdentity";
import type { Theme } from "../bindings/Theme";
import type { ThemeSummary } from "../bindings/ThemeSummary";
import { ApplyFlow } from "../components/ApplyFlow";
import { Dialog } from "../components/Dialog";
import { ErrorNotice } from "../components/Notices";
import { PausedBanner, PauseResult, usePause, useResume } from "../components/Pause";
import { TerminalPreview } from "../components/TerminalPreview";
import { formatTime, useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import type { Ipc } from "../ipc/types";
import { useNav } from "../router";
import { CORE_SAMPLE_IDENTITY, sampleContext } from "../sample";
import { useAppState } from "../state";
import { copyName } from "./Themes";

/** How many themes the quick switch offers (spec §4.3). */
const RECENT = 5;

/** One read of the page, done again whenever `version` changes: its answer or what failed. An
 * answer that comes after a newer read started is dropped. */
function useRead<T>(read: (ipc: Ipc) => Promise<T>, version: number): { value: T | null; error: unknown } {
  const ipc = useIpc();
  const [state, setState] = useState<{ value: T | null; error: unknown }>({ value: null, error: null });
  useEffect(() => {
    let alive = true;
    read(ipc).then(
      (value) => {
        if (alive) setState({ value, error: null });
      },
      (error: unknown) => {
        if (alive) setState({ value: null, error });
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc, read, version]);
  return state;
}

// Module-level, so each read keeps one identity across renders.
const readLibrary = (ipc: Ipc) => ipc.themesList();
const readChecks = (ipc: Ipc) => ipc.doctorRun();
const readPoints = (ipc: Ipc) => ipc.snapshotsList();
const readRecovery = (ipc: Ipc) => ipc.recoveryStatus();

export function Home() {
  const ipc = useIpc();
  const { t, language } = useI18n();
  const { go } = useNav();
  const { settings, version, bump, setLastApply, recoveryActed } = useAppState();
  const library = useRead(readLibrary, version);
  const checks = useRead(readChecks, version);
  const points = useRead(readPoints, version);
  const recovery = useRead(readRecovery, version);
  const [preview, setPreview] = useState<{ theme: Theme; result: PreviewResult | null } | null>(null);
  const [previewError, setPreviewError] = useState<unknown>(null);
  /** `system_detect` failed for the preview, which then uses the core's sample. */
  const [systemError, setSystemError] = useState<unknown>(null);
  const identity = useRef<SampleIdentity | null>(null);
  /** What the last button of the theme card said went wrong: Customize, Open cmd. */
  const [actionError, setActionError] = useState<unknown>(null);
  const [applying, setApplying] = useState<{ key: number; request: ApplyRequest } | null>(null);
  const [applyBusy, setApplyBusy] = useState(false);
  const applies = useRef(0);
  const heading = useRef<HTMLHeadingElement>(null);
  const closing = useRef(false);
  const pause = usePause(heading);

  const lib = library.value;
  const appliedId = lib?.applied?.themeId ?? null;
  const paused = recovery.value?.paused ?? null;
  const resume = useResume(heading, paused);
  // The pause the start's crash check tried, when it tried one (§4.3).
  const crashes = recovery.value?.crashes ?? null;
  const startPause = crashes && crashes.kind !== "noTheme" ? crashes.pause : null;
  const startTookThemeOut = startPause?.revert?.status === "committed";

  // The start's crash check took the theme out of this computer while this page was reading: what
  // it read before says a theme is applied, so it reads again, once. It is decided on the first
  // answers only (the reads that follow are the right ones), so it is never a loop.
  const decidedReread = useRef(false);
  useEffect(() => {
    if (decidedReread.current || lib === null || recovery.value === null) return;
    decidedReread.current = true;
    if (startTookThemeOut && lib.applied) bump();
  }, [lib, recovery.value, startTookThemeOut, bump]);

  // A pause the start tried that did not go through (ruling R4, with its correction):
  // - the pause of this computer is not yet in effect;
  // - it took the theme out with a step that failed, and the record is saved (`computer`);
  // - or it could not be noted, and either did not take the theme out or the theme is out now (a
  //   theme still read as applied is the read above, about to be done again).
  // It is the start's own record, so it goes once the user has paused or turned the theme back on:
  // a later pause that is not yet is said by the banner, not by this card.
  const themeIsOut = lib ? lib.applied === null : library.error !== null;
  const startPauseShown =
    startPause !== null &&
    !startPause.pausedNow &&
    !recoveryActed &&
    (paused?.level === "computerNotYet" ||
      (paused?.level === "computer" && startTookThemeOut && startPause.revert?.ok === false && startPause.notSaved === null) ||
      (paused === null && startPause.notSaved !== null && (!startTookThemeOut || themeIsOut)))
      ? startPause
      : null;

  /**
   * This computer's user, host and home for the preview (ruling R1), read when a preview needs
   * them. An answer is kept for the page; a failure is not, so the next preview asks again, and
   * meanwhile the core's sample stands in and the page says so (as the Themes page does).
   */
  const sampleIdentity = async (): Promise<{ sample: SampleIdentity; error: unknown }> => {
    if (identity.current) return { sample: identity.current, error: null };
    try {
      identity.current = (await ipc.systemDetect()).previewSample;
      return { sample: identity.current, error: null };
    } catch (error) {
      return { sample: CORE_SAMPLE_IDENTITY, error };
    }
  };

  // The theme in use, drawn with this computer's sample.
  useEffect(() => {
    if (!appliedId) {
      setPreview(null);
      setSystemError(null);
      return;
    }
    let alive = true;
    setPreviewError(null);
    Promise.all([sampleIdentity(), ipc.themeGet(appliedId)])
      .then(async ([{ sample, error }, { theme }]) => {
        if (!alive) return;
        setSystemError(error);
        setPreview({ theme, result: null });
        const result = await ipc.previewRender(theme, sampleContext(sample));
        if (alive) setPreview({ theme, result });
      })
      .catch((e: unknown) => {
        if (alive) setPreviewError(e);
      });
    return () => {
      alive = false;
    };
    // `sampleIdentity` reads only the ipc and a ref.
  }, [ipc, appliedId, version]);

  // The dialog gives focus back to the button that opened it; when that button went (its theme
  // is in use now, so it is no longer offered), focus goes to the page's heading rather than to
  // the page's body.
  useEffect(() => {
    if (applying !== null || !closing.current) return;
    closing.current = false;
    const active = document.activeElement;
    if (active === null || active === document.body || !active.isConnected) heading.current?.focus();
  }, [applying]);

  const act = async (action: () => Promise<void>) => {
    setActionError(null);
    try {
      await action();
    } catch (e) {
      setActionError(e);
    }
  };

  // Customize (spec §3.1): a preset is copied into my themes first, as the Themes page does.
  const customize = (s: ThemeSummary) =>
    act(async () => {
      const id = s.source === "preset" ? (await ipc.themeDuplicate(s.id, copyName(s.name, t))).id : s.id;
      go({ name: "editor", id });
    });

  const openCmd = () => act(() => ipc.openCmd());

  // Each switch mounts a flow of its own (a new key), and the dialog starts closable.
  const startApply = (s: ThemeSummary) => {
    applies.current += 1;
    setApplyBusy(false);
    setApplying({
      key: applies.current,
      request: { themeId: s.id, target: settings.target, setWtDefaultProfile: settings.setWtDefaultProfile, replaceOtherClink: false },
    });
  };
  const closeApply = () => {
    closing.current = true;
    setApplying(null);
    setApplyBusy(false);
  };

  const report = checks.value;
  const warns = report?.checks.filter((c) => c.status === "warn").length ?? 0;
  const fails = report?.checks.filter((c) => c.status === "fail").length ?? 0;
  const statusText = report
    ? fails > 0
      ? t("home.status.fail", { n: fails })
      : warns > 0
        ? t("home.status.warn", { n: warns })
        : t("home.status.ok")
    : checks.error === null
      ? t("home.status.checking")
      : "";
  const statusClass = !report ? "" : fails > 0 ? "status-fail" : warns > 0 ? "status-warn" : "status-ok";

  const inUse = lib?.applied ? (lib.themes.find((s) => s.id === lib.applied?.themeId) ?? null) : null;

  // Used recently: the newest apply of each theme that committed (a rolled-back one has no
  // restore point left and was never undone), still in the library, up to five. The theme in use
  // is not offered: switching to it would change nothing (V5).
  const recent: ThemeSummary[] = [];
  for (const p of points.value?.points ?? []) {
    if (recent.length === RECENT) break;
    if (p.kind !== "apply" || !p.theme || !(p.restorable || p.undoneBy !== null)) continue;
    const id = p.theme.id;
    if (id === appliedId) continue;
    const summary = lib?.themes.find((s) => s.id === id && s.source !== "draft");
    if (summary && !recent.some((r) => r.id === id)) recent.push(summary);
  }
  // The newest point there is to go back to (m2): a rolled-back apply or an undone one is listed
  // in Help, but cannot be gone back to.
  const latest = points.value?.points.find((p) => p.restorable) ?? null;
  const anyPoint = (points.value?.points.length ?? 0) > 0;

  return (
    <section>
      <h1 ref={heading} tabIndex={-1}>
        {t("home.title")}
      </h1>
      {recovery.error !== null && <ErrorNotice error={recovery.error} title={t("home.paused.unreadable")} />}
      {paused && <PausedBanner paused={paused} onResume={() => void resume.resume()} />}
      {startPauseShown && (
        <div className="card">
          <p>{t("home.paused.startTried")}</p>
          <PauseResult outcome={startPauseShown} live />
        </div>
      )}
      {resume.view}
      {pause.view}
      <div className="card">
        <h2 style={{ marginTop: 0 }}>{t("home.status.title")}</h2>
        {/* Always there, its text toggled (2c item 10): checking, then what the checks found. */}
        <p role="status" className={statusClass}>
          {statusText}
        </p>
        {report && <p>{t("home.status.checkedAt", { time: formatTime(report.checkedAtMs, language) })}</p>}
        {checks.error !== null && <ErrorNotice error={checks.error} />}
        <button type="button" onClick={() => go({ name: "help", tab: "doctor" })}>
          {t("home.status.open")}
        </button>
        {/* Phase 4's updater puts its "Updated to vX" notice here. */}
        <div data-slot="update-notice">{t("home.update.placeholder")}</div>
      </div>
      <div className="card">
        <h2 style={{ marginTop: 0 }}>{t("home.active.title")}</h2>
        {library.error !== null && <ErrorNotice error={library.error} />}
        {!lib && library.error === null && <p>{t("common.loading")}</p>}
        {lib?.stateProblem && <ErrorNotice error={lib.stateProblem} title={t("home.active.unknown")} />}
        {lib && !lib.applied && (
          <>
            {!lib.stateProblem && <p>{t("home.active.none")}</p>}
            <button type="button" className="primary" onClick={() => go({ name: "themes" })}>
              {t("home.active.pick")}
            </button>
          </>
        )}
        {lib?.applied && (
          <>
            <p>
              <strong>{inUse?.name ?? lib.applied.themeName}</strong>
            </p>
            {preview && <TerminalPreview theme={preview.theme} result={preview.result} />}
            {systemError !== null && <ErrorNotice error={systemError} title={t("themes.import.systemUnknown")} />}
            {previewError !== null && <ErrorNotice error={previewError} />}
            <div className="row-actions">
              {inUse && inUse.source !== "draft" && (
                <button type="button" onClick={() => void customize(inUse)}>
                  {t("home.active.customize")}
                </button>
              )}
              <button type="button" onClick={() => go({ name: "themes" })}>
                {t("home.active.change")}
              </button>
              <button type="button" onClick={() => void openCmd()}>
                {t("home.active.openCmd")}
              </button>
              {/* Only once it is known that the theme is not paused already. */}
              {recovery.value !== null && recovery.value.paused === null && (
                <button
                  type="button"
                  onClick={() => {
                    resume.reset();
                    pause.ask("user");
                  }}
                >
                  {t("home.active.pause")}
                </button>
              )}
            </div>
          </>
        )}
        {actionError !== null && <ErrorNotice error={actionError} />}
      </div>
      <div className="card">
        <h2 style={{ marginTop: 0 }}>{t("home.quick.title")}</h2>
        {recent.length === 0 && <p>{t("home.quick.none")}</p>}
        <div className="row-actions">
          {recent.map((s) => (
            <button key={s.id} type="button" onClick={() => startApply(s)}>
              {t("home.quick.use", { name: s.name })}
            </button>
          ))}
        </div>
      </div>
      <div className="card">
        <h2 style={{ marginTop: 0 }}>{t("home.restore.title")}</h2>
        {points.error !== null && <ErrorNotice error={points.error} />}
        {points.value && !latest && <p>{t(anyPoint ? "home.restore.noneRestorable" : "home.restore.none")}</p>}
        {latest && (
          <p>
            {t("home.restore.latest", {
              name: latest.theme?.name ?? t(`help.restore.kind.${latest.kind}`),
              time: formatTime(latest.createdMs, language),
            })}
          </p>
        )}
        <button type="button" onClick={() => go({ name: "help", tab: "restore" })}>
          {t("home.restore.all")}
        </button>
      </div>
      {applying && (
        // No way to close it while the apply runs (ruling R7), and it stays open once the apply
        // committed: its outcome is read there, since the after-apply bar is no live region.
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
    </section>
  );
}
