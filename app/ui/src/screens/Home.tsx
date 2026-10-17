// The home page (spec §4.3): the status from the real checks with the time they ran, the theme in
// use with its preview and Customize / Change theme / Open cmd, a quick switch to the five themes
// used most recently (through the plan, as every apply), and the latest restore point. A line is
// kept for Phase 4's "Updated to vX" notice (`home.update.placeholder`). Each part loads on its
// own, so one that fails is said and the others still show; a state.json that could not be read
// is said as such, never as "no theme is applied" (spec §5.8 D9).
import { useEffect, useRef, useState } from "react";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { SampleIdentity } from "../bindings/SampleIdentity";
import type { Theme } from "../bindings/Theme";
import type { ThemeSummary } from "../bindings/ThemeSummary";
import { ApplyFlow } from "../components/ApplyFlow";
import { Dialog } from "../components/Dialog";
import { ErrorNotice } from "../components/Notices";
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

export function Home() {
  const ipc = useIpc();
  const { t, language } = useI18n();
  const { go } = useNav();
  const { settings, version, bump, setLastApply } = useAppState();
  const library = useRead(readLibrary, version);
  const checks = useRead(readChecks, version);
  const points = useRead(readPoints, version);
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

  const lib = library.value;
  const appliedId = lib?.applied?.themeId ?? null;

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

  // The dialog gives focus back to the button that opened it; when that button is now disabled
  // (its theme is in use), focus goes to the page's heading rather than to the page's body.
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
  // restore point left and was never undone), still in the library, up to five.
  const recent: ThemeSummary[] = [];
  for (const p of points.value?.points ?? []) {
    if (recent.length === RECENT) break;
    if (p.kind !== "apply" || !p.theme || !(p.restorable || p.undoneBy !== null)) continue;
    const id = p.theme.id;
    const summary = lib?.themes.find((s) => s.id === id && s.source !== "draft");
    if (summary && !recent.some((r) => r.id === id)) recent.push(summary);
  }
  const latest = points.value?.points[0] ?? null;

  return (
    <section>
      <h1 ref={heading} tabIndex={-1}>
        {t("home.title")}
      </h1>
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
            {systemError !== null && <ErrorNotice error={systemError} title={t("home.active.systemUnknown")} />}
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
            <button key={s.id} type="button" disabled={appliedId === s.id} onClick={() => startApply(s)}>
              {t("home.quick.use", { name: s.name })}
            </button>
          ))}
        </div>
      </div>
      <div className="card">
        <h2 style={{ marginTop: 0 }}>{t("home.restore.title")}</h2>
        {points.error !== null && <ErrorNotice error={points.error} />}
        {points.value && !latest && <p>{t("home.restore.none")}</p>}
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
