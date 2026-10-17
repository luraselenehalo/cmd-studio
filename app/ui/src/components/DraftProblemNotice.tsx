// A draft the app could not write or take away, said on the screen that came after the one that
// tried (item 4): the editor that went away, or the library after a rename (ruling R8). Every
// shell shows it above its screen: this plan's interim one and phase 3's final one.
import { useState } from "react";
import { useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import { useAppState } from "../state";
import { ErrorNotice } from "./Notices";

/** The problem with its error, Try again (which writes the kept draft or takes the draft away)
 * and Dismiss. A success lists the library again, so its badges follow. */
export function DraftProblemNotice() {
  const ipc = useIpc();
  const { t } = useI18n();
  const { draftProblem, setDraftProblem, bump } = useAppState();
  const [busy, setBusy] = useState(false);
  if (!draftProblem) return null;

  const retry = async () => {
    const problem = draftProblem;
    setBusy(true);
    try {
      if (problem.kind === "write") await ipc.themeSave(problem.draft, true);
      else await ipc.themeDelete(problem.id, true);
      setDraftProblem(null);
      bump();
    } catch (error) {
      setDraftProblem({ ...problem, error });
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <ErrorNotice
        error={draftProblem.error}
        title={t(draftProblem.kind === "write" ? "editor.draftNotSaved" : "editor.draftNotRemoved")}
      />
      <div className="row-actions">
        <button type="button" disabled={busy} onClick={() => void retry()}>
          {t("common.retry")}
        </button>
        <button type="button" className="link" onClick={() => setDraftProblem(null)}>
          {t("common.dismiss")}
        </button>
      </div>
    </>
  );
}
