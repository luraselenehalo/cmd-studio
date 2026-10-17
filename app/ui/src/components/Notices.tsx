// What the user reads when something is wrong or worth knowing (spec §5.9: the problem in
// their language, the code, and the technical detail behind a "Details" toggle).
import type { ReactNode } from "react";
import type { AppError } from "../bindings/AppError";
import type { Note } from "../bindings/Note";
import type { Unrestored } from "../bindings/Unrestored";
import { pick, useI18n } from "../i18n";
import { isAppError, toIpcError } from "../ipc/types";

/** An error in the UI's language with its code; anything that is not an `AppError` (all four
 * fields) is shown as `toIpcError` makes it: E_INTERNAL, with what was thrown as the details.
 * `live={false}` drops the alert role, for a box that takes focus and is read by that (item 11). */
export function ErrorNotice({ error, title, live = true }: { error: AppError | unknown; title?: string; live?: boolean }) {
  const { language, t } = useI18n();
  const e: AppError = isAppError(error) ? error : toIpcError(error).toAppError();
  return (
    <div className="notice error" role={live ? "alert" : undefined}>
      <strong>{title ?? t("error.title")}</strong>
      <p>{pick(language, e.messageTh, e.messageEn)}</p>
      <details>
        <summary>
          {t("common.details")} · {t("error.code", { code: e.code })}
        </summary>
        {e.details && <pre>{e.details}</pre>}
      </details>
    </div>
  );
}

export function Notice({ kind = "info", children }: { kind?: "info" | "ok" | "warn"; children: ReactNode }) {
  return (
    <div className={kind === "info" ? "notice" : `notice ${kind}`} role={kind === "warn" ? "alert" : "status"}>
      {children}
    </div>
  );
}

/** Notes from the start of a command (spec §5.5), in the UI's language. Each note's specific
 * reason (`details`, in the core's English) sits under its message behind a toggle, so a Thai
 * UI shows as much as an English one. */
export function NotesList({ notes }: { notes: Note[] }) {
  const { language, t } = useI18n();
  if (notes.length === 0) return null;
  return (
    <div className="notice warn">
      <strong>{t("apply.notesTitle")}</strong>
      <ul className="plain-list">
        {notes.map((n, i) => (
          <li key={i}>
            {pick(language, n.messageTh, n.messageEn)} <span className="badge">{n.code}</span>
            {n.details && (
              <details>
                <summary>{t("common.details")}</summary>
                <pre>{n.details}</pre>
              </details>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The steps a rollback could not put back, each with how to fix it by hand (spec §5.5).
 * `live={false}` as for `ErrorNotice`. */
export function UnrestoredList({ steps, live = true }: { steps: Unrestored[]; live?: boolean }) {
  const { language, t } = useI18n();
  if (steps.length === 0) return null;
  return (
    <div className="notice error" role={live ? "alert" : undefined}>
      <strong>{t("apply.unrestoredTitle")}</strong>
      <ul className="plain-list">
        {steps.map((s, i) => (
          <li key={i}>
            <div>
              {s.target} <span className="badge">{s.code}</span>
            </div>
            <div>{pick(language, s.fixTh, s.fix)}</div>
          </li>
        ))}
      </ul>
    </div>
  );
}
