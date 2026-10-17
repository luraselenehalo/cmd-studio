// The one way a theme reaches the machine (spec §2.1, §4.2 page 5, §4.4): plan → show what
// will change → the user presses Apply → progress step by step → the outcome. Used by the
// wizard, the editor's "Save and apply", the home page's quick switch, the Settings page and
// the doctor's fixes. A plan that changed while the user read it is shown again, never applied.
import { useCallback, useEffect, useId, useRef, useState, type ReactNode } from "react";
import type { ApplyOutcome } from "../bindings/ApplyOutcome";
import type { ApplyPlan } from "../bindings/ApplyPlan";
import type { ApplyProgress } from "../bindings/ApplyProgress";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { PlanWarning } from "../bindings/PlanWarning";
import type { StepKind } from "../bindings/StepKind";
import { useI18n, useT, type Key, type Translate } from "../i18n";
import { useIpc } from "../ipc/context";
import { IpcError, toIpcError } from "../ipc/types";
import { ErrorNotice, NotesList, UnrestoredList } from "./Notices";

const STEP_KEY: Record<StepKind, Key> = {
  installOhMyPosh: "apply.step.installOhMyPosh",
  installClink: "apply.step.installClink",
  writeGenerated: "apply.step.writeGenerated",
  windowsTerminal: "apply.step.windowsTerminal",
  autoRun: "apply.step.autoRun",
  installFont: "apply.step.installFont",
  saveState: "apply.step.saveState",
};

function warningText(w: PlanWarning, t: Translate): string {
  switch (w.kind) {
    case "noWindowsTerminal":
      return t("apply.warning.noWindowsTerminal");
    case "machineWideClink":
      return t("apply.warning.machineWideClink");
    case "fontFallback":
      return t("apply.warning.fontFallback", { wanted: w.wanted });
  }
}

export function PlanView({ plan, titled = true }: { plan: ApplyPlan; titled?: boolean }) {
  const { language, t } = useI18n();
  const summary = language === "th" ? plan.summaryTh : plan.summaryEn;
  return (
    <div>
      {titled && <h3>{t("apply.planTitle")}</h3>}
      <ul className="check-list">
        {summary.map((line, i) => (
          <li key={i}>{line}</li>
        ))}
      </ul>
      {plan.replacesOtherClink.length > 0 && (
        // Exactly the commands of the other add-on that this apply takes out of AutoRun, as
        // the user agreed (spec §5.4).
        <div>
          <p>{t("apply.replacesOtherClink")}</p>
          <ul className="check-list">
            {plan.replacesOtherClink.map((c, i) => (
              <li key={i}>
                <code>{c}</code>
              </li>
            ))}
          </ul>
        </div>
      )}
      {plan.warnings.length > 0 && (
        <>
          <h3>{t("apply.warnings")}</h3>
          <ul className="check-list">
            {plan.warnings.map((w, i) => (
              <li key={i}>{warningText(w, t)}</li>
            ))}
          </ul>
        </>
      )}
      <details>
        <summary>
          {t("apply.steps")} ({plan.steps.length})
        </summary>
        <ol>
          {plan.steps.map((s, i) => (
            <li key={i}>{t(STEP_KEY[s.kind], { detail: s.detail })}</li>
          ))}
        </ol>
      </details>
      <NotesList notes={plan.notes} />
      <UnrestoredList steps={plan.unrestored} />
    </div>
  );
}

/**
 * The text and the bar of one `apply-progress` event. Backing up and applying count the
 * transaction's steps, the state.json write included, as the plan lists them. Putting things
 * back is no fraction: its `step` counts the puts back, newest first, and stops below `total`
 * after a step failed part-way. So it has no numbers, and its bar has no value (indeterminate).
 */
function progressOf(progress: ApplyProgress, t: Translate): { text: string; max?: number; value?: number } {
  switch (progress.phase) {
    case "backingUp":
    case "applying": {
      const max = Math.max(progress.total, 1);
      return {
        text: t(`apply.phase.${progress.phase}`, { step: progress.step, total: progress.total }),
        max,
        value: Math.min(progress.step, max),
      };
    }
    case "verifying":
      return { text: t("apply.phase.verifying"), max: 1, value: 1 };
    case "rollingBack":
      return { text: t("apply.phase.rollingBack") };
  }
}

/** One status region from before the first event to the last, so a screen reader hears each
 * change in place; the bar is named by the text above it. */
export function ProgressView({ progress }: { progress: ApplyProgress | null }) {
  const t = useT();
  const textId = useId();
  const { text, max, value } = progress ? progressOf(progress, t) : { text: t("apply.working"), max: undefined, value: undefined };
  return (
    <div role="status">
      <p id={textId}>{text}</p>
      <progress aria-labelledby={textId} max={max} value={value} />
    </div>
  );
}

/** "Copy details for a problem report", saying whether the details reached the clipboard. Help's
 * "Report a problem" copies through it too, so both say it the same way. */
export function useCopyDiagnostics() {
  const ipc = useIpc();
  const [copied, setCopied] = useState<"yes" | IpcError | null>(null);
  const copy = async () => {
    setCopied(null);
    try {
      const text = await ipc.diagnosticsCopy();
      await navigator.clipboard.writeText(text);
      setCopied("yes");
    } catch (thrown) {
      setCopied(toIpcError(thrown));
    }
  };
  return { copied, copy };
}

/** An apply that rolled back: what went wrong, what to fix by hand, and the two ways on. */
function Failed({ outcome, onRetry }: { outcome: ApplyOutcome; onRetry: () => void }) {
  const t = useT();
  const { copied, copy } = useCopyDiagnostics();
  return (
    <div>
      <div className="notice error" role="alert">
        {outcome.status === "rollbackIncomplete" ? t("apply.failedIncomplete") : t("apply.failed")}
      </div>
      {/* Right after the header, whose "…Fix these by hand:" points at it. */}
      <UnrestoredList steps={outcome.unrestored} />
      {outcome.error && <ErrorNotice error={outcome.error} />}
      <NotesList notes={outcome.notes} />
      <div className="row-actions">
        <button type="button" onClick={() => void copy()}>
          {t("apply.copyDiagnostics")}
        </button>
        <button type="button" onClick={onRetry}>
          {t("common.retry")}
        </button>
        {/* Always there, its text toggled (item 10): a region mounted with its text is not
            reliably announced. */}
        <span role="status">{copied === "yes" ? t("common.copied") : ""}</span>
      </div>
      {copied instanceof IpcError && <ErrorNotice error={copied} title={t("apply.copyFailed")} />}
    </div>
  );
}

type Stage =
  | { kind: "planning" }
  | { kind: "otherClink"; commands: string[] }
  | { kind: "ready"; plan: ApplyPlan; changed: boolean }
  | { kind: "applying"; progress: ApplyProgress | null }
  | { kind: "done"; outcome: ApplyOutcome }
  | { kind: "error"; error: IpcError };

/**
 * The flow plans for `request` as it was when the flow mounted; to plan another request, mount
 * a new flow (a new `key`). The user's "Use CMD Studio's instead" holds for the plan it answers
 * and that plan's apply only: Try again plans from that first request with
 * `replaceOtherClink: false`, so the question about the other add-on is asked again (ruling R5).
 *
 * Each stage is a new container that takes focus when the stage changes (the button pressed
 * is gone with the old stage), unless focus is somewhere else the user moved it to.
 */
export function ApplyFlow({
  request: initial,
  onApplied,
  onCancel,
  onBusy,
  showBackup = false,
  titled = true,
}: {
  request: ApplyRequest;
  /** Whether the plan carries its own "What will change" heading (a dialog has it as its title). */
  titled?: boolean;
  /** Called once an apply committed (the after-apply bar and the caller's refresh), with the
   * request that was applied, whose `target` words the bar; never for an apply that rolled
   * back, though its outcome carries a `snapshotId` too. */
  onApplied?: (outcome: ApplyOutcome, request: ApplyRequest) => void;
  onCancel?: () => void;
  /** `true` just before the apply starts and `false` once it has ended, however it ended: a
   * dialog around the flow cannot be closed in between (ruling R7). */
  onBusy?: (busy: boolean) => void;
  /** The wizard's "we save a restore point first" line. */
  showBackup?: boolean;
}) {
  const ipc = useIpc();
  const t = useT();
  const first = useRef(initial);
  const [request, setRequest] = useState(initial);
  const [stage, setStage] = useState<Stage>({ kind: "planning" });
  const busy = useRef(false);
  const stageBox = useRef<HTMLDivElement>(null);
  const shownKind = useRef(stage.kind);

  const plan = useCallback(
    async (req: ApplyRequest) => {
      setStage({ kind: "planning" });
      try {
        const outcome = await ipc.planApply(req);
        if (outcome.kind === "otherClink") setStage({ kind: "otherClink", commands: outcome.commands });
        else setStage({ kind: "ready", plan: outcome.plan, changed: false });
      } catch (thrown) {
        setStage({ kind: "error", error: toIpcError(thrown) });
      }
    },
    [ipc],
  );

  // Every plan of the flow goes through here: a new request object is a new plan.
  useEffect(() => {
    void plan(request);
  }, [plan, request]);

  // The first stage leaves focus where it is (a dialog puts it on its box). A later one takes
  // it when focus went with the old stage (to the page's body) or sits on what holds the flow.
  useEffect(() => {
    if (shownKind.current === stage.kind) return;
    shownKind.current = stage.kind;
    const box = stageBox.current;
    const active = document.activeElement;
    if (box && (active === null || active === document.body || active.contains(box))) box.focus();
  }, [stage.kind]);

  const replace = () => setRequest({ ...request, replaceOtherClink: true });
  const retry = () => setRequest({ ...first.current, replaceOtherClink: false });

  const run = async (current: ApplyPlan) => {
    // One apply at a time, even if Apply were pressed twice before the page changed.
    if (busy.current) return;
    busy.current = true;
    onBusy?.(true);
    setStage({ kind: "applying", progress: null });
    try {
      const outcome = await ipc.apply(request, current.digest, (progress) =>
        setStage({ kind: "applying", progress }),
      );
      if (outcome.status === "planChanged") {
        // Nothing was done. The backend always sends the new plan; without one, plan again
        // rather than say the apply failed.
        if (outcome.plan) setStage({ kind: "ready", plan: outcome.plan, changed: true });
        else void plan(request);
        return;
      }
      setStage({ kind: "done", outcome });
      if (outcome.status === "committed") onApplied?.(outcome, request);
    } catch (thrown) {
      setStage({ kind: "error", error: toIpcError(thrown) });
    } finally {
      busy.current = false;
      onBusy?.(false);
    }
  };

  let body: ReactNode;
  switch (stage.kind) {
    case "planning":
      body = <p role="status">{t("common.loading")}</p>;
      break;
    case "otherClink":
      body = (
        <>
          <h3>{t("apply.otherClink.title")}</h3>
          <p>{t("apply.otherClink.text")}</p>
          <ul className="check-list">
            {stage.commands.map((c, i) => (
              <li key={i}>
                <code>{c}</code>
              </li>
            ))}
          </ul>
          <div className="row-actions">
            <button type="button" className="primary" onClick={replace}>
              {t("apply.otherClink.replace")}
            </button>
            {onCancel && (
              <button type="button" onClick={onCancel}>
                {t("common.cancel")}
              </button>
            )}
          </div>
        </>
      );
      break;
    case "ready": {
      const ready = stage.plan;
      body = (
        <>
          {stage.changed && (
            <div className="notice warn" role="alert">
              {t("apply.planChanged")}
            </div>
          )}
          <PlanView plan={ready} titled={titled} />
          {showBackup && <p>{t("wizard.confirm.backup")}</p>}
          <div className="row-actions">
            <button type="button" className="primary" onClick={() => void run(ready)}>
              {t("apply.start")}
            </button>
            {onCancel && (
              <button type="button" onClick={onCancel}>
                {t("common.cancel")}
              </button>
            )}
          </div>
        </>
      );
      break;
    }
    case "applying":
      body = <ProgressView progress={stage.progress} />;
      break;
    case "done": {
      const o = stage.outcome;
      body =
        o.status === "committed" ? (
          <div className="notice ok" role="status">
            {t(request.target === "wt" ? "apply.doneWt" : "apply.done", { name: o.themeName })}
            <NotesList notes={o.notes} />
          </div>
        ) : (
          <Failed outcome={o} onRetry={retry} />
        );
      break;
    }
    case "error":
      body = (
        <>
          <ErrorNotice error={stage.error} />
          <div className="row-actions">
            <button type="button" onClick={retry}>
              {t("common.retry")}
            </button>
            {onCancel && (
              <button type="button" onClick={onCancel}>
                {t("common.cancel")}
              </button>
            )}
          </div>
        </>
      );
      break;
  }

  // Keyed by the stage, so each stage is a new element: focusing it is announced.
  return (
    <div key={stage.kind} ref={stageBox} tabIndex={-1} className="apply-stage">
      {body}
    </div>
  );
}
