// The drafts the app could not write or take away, said on the screen that came after the one
// that tried (item 4): the editor that went away, or the library after a rename (ruling R8).
// One notice per theme, named by it (round 2, fix 2). Every shell shows them above its screen:
// this plan's interim one and phase 3's final one.
import { useState } from "react";
import { useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import { useAppState, type DraftProblem } from "../state";
import { ErrorNotice } from "./Notices";

export function DraftProblemNotice() {
  const { draftProblems } = useAppState();
  return (
    <>
      {draftProblems.list.map((problem) => (
        <DraftProblemItem key={`${problem.id}-${problem.seq}`} problem={problem} />
      ))}
    </>
  );
}

/**
 * One problem with its error, Try again and Dismiss, in a group named by its title. Try again
 * first lists the library: a theme deleted since is not made again by its draft, and a draft that
 * is gone needs no removal; either way the problem is over. It writes only while the problem is
 * still kept (not dismissed, not replaced by a newer one, not cleared by an editor that wrote a
 * newer draft), and its answer touches that problem only.
 */
function DraftProblemItem({ problem }: { problem: DraftProblem }) {
  const ipc = useIpc();
  const { t } = useI18n();
  const { draftProblems, bump } = useAppState();
  const [busy, setBusy] = useState(false);
  const title = t(problem.kind === "write" ? "editor.draftNotSavedOf" : "editor.draftNotRemovedOf", { name: problem.name });

  const retry = async () => {
    setBusy(true);
    try {
      const { themes } = await ipc.themesList();
      if (!draftProblems.isCurrent(problem.id, problem.seq)) return;
      const theme = themes.find((s) => s.id === problem.id);
      if (problem.kind === "write" && theme) await ipc.themeSave(problem.draft, true);
      // Only a theme of mine has a draft apart from itself to take away.
      if (problem.kind === "remove" && theme?.source === "mine" && theme.hasDraft) await ipc.themeDelete(problem.id, true);
      draftProblems.clear(problem.id, problem.seq);
      bump();
    } catch (error) {
      draftProblems.retryFailed(problem.id, problem.seq, error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div role="group" aria-label={title}>
      <ErrorNotice error={problem.error} title={title} />
      <div className="row-actions">
        <button type="button" disabled={busy} onClick={() => void retry()}>
          {t("common.retry")}
        </button>
        <button type="button" className="link" onClick={() => draftProblems.clear(problem.id, problem.seq)}>
          {t("common.dismiss")}
        </button>
      </div>
    </div>
  );
}
