// The theme library (spec §3.1): presets (Customize copies one into my themes), my themes
// (edit, rename, duplicate, delete, export, apply), drafts of unsaved themes, a filter
// all / dark / light, the "in use" badge, import (file → check → preview → confirm) and a new
// theme (simple mode, dark, #d97757, tint 0.10). One `themes_list` draws every card: the
// swatches come with the summaries (ruling R10), so there is no `theme_get` per card.
import { useCallback, useEffect, useRef, useState } from "react";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { ImportPreview } from "../bindings/ImportPreview";
import type { Library } from "../bindings/Library";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { SampleIdentity } from "../bindings/SampleIdentity";
import type { Theme } from "../bindings/Theme";
import type { ThemeSummary } from "../bindings/ThemeSummary";
import type { Tone } from "../bindings/Tone";
import { ApplyFlow } from "../components/ApplyFlow";
import { Dialog } from "../components/Dialog";
import { ErrorNotice, Notice } from "../components/Notices";
import { TerminalPreview } from "../components/TerminalPreview";
import { ThemeCard } from "../components/ThemeCard";
import { pick, useI18n, type Key } from "../i18n";
import { useIpc } from "../ipc/context";
import { useNav } from "../router";
import { CORE_SAMPLE_IDENTITY, sampleContext } from "../sample";
import { useAppState } from "../state";

export const NEW_THEME_ACCENT = "#d97757";
export const NEW_THEME_TINT = 0.1;
/** The most characters a theme's name may have, counted as the core counts them: code points
 * (spec §3.3, `Theme::validate`). */
export const NAME_MAX_CHARS = 64;

/** `text`'s first `max` code points: never a cut inside a UTF-16 surrogate pair. */
function firstCodePoints(text: string, max: number): string {
  const points = Array.from(text);
  return points.length <= max ? text : points.slice(0, Math.max(max, 0)).join("");
}

/** A random version 4 UUID, 8-4-4-4-12 hexadecimal digits (the theme file's `id`). */
function uuidV4(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

type Filter = "all" | Tone;

const FILTERS: Filter[] = ["all", "dark", "light"];

export function Themes() {
  const ipc = useIpc();
  const { t, language } = useI18n();
  const { go } = useNav();
  const { settings, version, bump, setLastApply, draftProblems } = useAppState();
  const [library, setLibrary] = useState<Library | null>(null);
  const [listError, setListError] = useState<unknown>(null);
  const [filter, setFilter] = useState<Filter>("all");
  /** What the last action on the page said went wrong (a dialog shows its own). */
  const [error, setError] = useState<unknown>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [applying, setApplying] = useState<{ key: number; request: ApplyRequest } | null>(null);
  const [applyBusy, setApplyBusy] = useState(false);
  const [deleting, setDeleting] = useState<ThemeSummary | null>(null);
  const [renaming, setRenaming] = useState<ThemeSummary | null>(null);
  const [newName, setNewName] = useState("");
  const [importing, setImporting] = useState<ImportPreview | null>(null);
  const [importPreview, setImportPreview] = useState<PreviewResult | null>(null);
  const [importPreviewError, setImportPreviewError] = useState<unknown>(null);
  /** `system_detect` failed for the import's preview, which then uses the core's sample. */
  const [importDetectError, setImportDetectError] = useState<unknown>(null);
  const [dialogError, setDialogError] = useState<unknown>(null);
  const loads = useRef(0);
  const applies = useRef(0);
  const identity = useRef<SampleIdentity | null>(null);

  // The newest listing wins: an older answer that comes in late is dropped.
  const load = useCallback(async () => {
    const n = ++loads.current;
    try {
      const lib = await ipc.themesList();
      if (n !== loads.current) return;
      setLibrary(lib);
      setListError(null);
    } catch (e) {
      if (n === loads.current) setListError(e);
    }
  }, [ipc]);

  useEffect(() => {
    void load();
  }, [load, version]);

  /** An action of the page: its failure is shown, and the library is listed again. */
  const act = async (action: () => Promise<void>, reload = true) => {
    setError(null);
    setMessage(null);
    try {
      await action();
    } catch (e) {
      setError(e);
    }
    if (reload) await load();
  };

  /** An action confirmed in a dialog: its failure is shown in the dialog, which stays open. */
  const actInDialog = async (action: () => Promise<void>) => {
    setDialogError(null);
    try {
      await action();
    } catch (e) {
      setDialogError(e);
    }
    await load();
  };

  /**
   * A copy's name, "My <name>" / "<name> ของฉัน", within NAME_MAX_CHARS code points (item 6): a
   * name of 64 would otherwise be refused. The name inside is shortened, so the words around it
   * stay whole (a cut at the end of the Thai one would leave "… ของ").
   */
  const copyName = (name: string): string => {
    const room = NAME_MAX_CHARS - Array.from(t("themes.copyName", { name: "" })).length;
    return t("themes.copyName", { name: firstCodePoints(name, room) });
  };

  // Customize (spec §3.1): a copy named "My <name>" in my themes, opened in the editor.
  const customize = (s: ThemeSummary) =>
    act(async () => {
      const copy = await ipc.themeDuplicate(s.id, copyName(s.name));
      go({ name: "editor", id: copy.id });
    }, false);

  const duplicate = (s: ThemeSummary) =>
    act(async () => {
      await ipc.themeDuplicate(s.id, copyName(s.name));
    });

  // A new theme starts in simple mode: dark, #d97757, tint 0.10 (spec §3.1). Its colors come
  // from the Rust palette; the font, the prompt and the welcome box from the first preset. It
  // is a draft until the user saves it.
  const createNew = () =>
    act(async () => {
      const first = library?.themes.find((s) => s.source === "preset");
      if (!first) return;
      const { theme: base } = await ipc.themeGet(first.id);
      const colors = await ipc.paletteGenerate("dark", NEW_THEME_ACCENT, NEW_THEME_TINT);
      const theme: Theme = {
        ...base,
        id: uuidV4(),
        name: t("themes.newName"),
        author: "",
        tone: "dark",
        simple: { accent: NEW_THEME_ACCENT, tint: NEW_THEME_TINT },
        colors,
      };
      const saved = await ipc.themeSave(theme, true);
      go({ name: "editor", id: saved.id });
    }, false);

  const exportTheme = (s: ThemeSummary) =>
    act(async () => {
      const path = await ipc.themeExport(s.id);
      if (path) setMessage(t("themes.exported", { path }));
    }, false);

  /**
   * This computer's user, host and home for the import's preview (ruling R1). An answer is kept
   * for the page; a failure is not, so the next import asks again (fix 3), and meanwhile the
   * preview uses the core's sample and says so.
   */
  const sampleIdentity = async (): Promise<{ identity: SampleIdentity; error: unknown }> => {
    if (identity.current) return { identity: identity.current, error: null };
    try {
      identity.current = (await ipc.systemDetect()).previewSample;
      return { identity: identity.current, error: null };
    } catch (error) {
      return { identity: CORE_SAMPLE_IDENTITY, error };
    }
  };

  const startImport = () =>
    act(async () => {
      const preview = await ipc.themeImportPreview();
      if (!preview) {
        setMessage(t("themes.import.cancelled"));
        return;
      }
      setDialogError(null);
      setImportPreview(null);
      setImportPreviewError(null);
      setImportDetectError(null);
      setImporting(preview);
      const sample = await sampleIdentity();
      setImportDetectError(sample.error);
      try {
        setImportPreview(await ipc.previewRender(preview.theme, sampleContext(sample.identity)));
      } catch (e) {
        setImportPreviewError(e);
      }
    }, false);

  const confirmImport = () =>
    actInDialog(async () => {
      if (!importing) return;
      await ipc.themeImportConfirm(importing.token);
      setImporting(null);
    });

  const confirmDelete = () =>
    actInDialog(async () => {
      if (!deleting) return;
      await ipc.themeDelete(deleting.id, deleting.source === "draft");
      setDeleting(null);
    });

  // A rename saves the theme, which takes its draft away (library.rs `save_locked`); a draft
  // that was waiting is saved again under the new name, so no unsaved edit is lost (ruling R8).
  // When that fails, the draft is kept in the app state, which says so with Try again (fix 2):
  // the same notice as for a draft the editor could not write as it went (item 4).
  const confirmRename = () =>
    actInDialog(async () => {
      if (!renaming) return;
      const { theme, draft } = await ipc.themeGet(renaming.id);
      await ipc.themeSave({ ...theme, name: newName }, false);
      setRenaming(null);
      if (draft) {
        const renamed: Theme = { ...draft, name: newName };
        try {
          await ipc.themeSave(renamed, true);
        } catch (e) {
          draftProblems.report({ kind: "write", id: renamed.id, name: newName, draft: renamed, error: e });
        }
      }
    });

  const openRename = (s: ThemeSummary) => {
    setDialogError(null);
    setNewName(s.name);
    setRenaming(s);
  };

  const openDelete = (s: ThemeSummary) => {
    setDialogError(null);
    setDeleting(s);
  };

  const startApply = (s: ThemeSummary) => {
    applies.current += 1;
    setApplyBusy(false);
    setApplying({
      key: applies.current,
      request: {
        themeId: s.id,
        target: settings.target,
        setWtDefaultProfile: settings.setWtDefaultProfile,
        replaceOtherClink: false,
      },
    });
  };
  const closeApply = () => {
    setApplying(null);
    setApplyBusy(false);
  };

  const visible = (library?.themes ?? []).filter((s) => filter === "all" || s.tone === filter);
  const groups: { key: "preset" | "mine" | "draft"; title: Key }[] = [
    { key: "preset", title: "themes.presets" },
    { key: "mine", title: "themes.mine" },
    { key: "draft", title: "themes.drafts" },
  ];

  return (
    <section>
      <h1 tabIndex={-1}>{t("themes.title")}</h1>
      {listError !== null && <ErrorNotice error={listError} />}
      {error !== null && <ErrorNotice error={error} />}
      {message && <Notice kind="ok">{message}</Notice>}
      {library?.stateProblem && <ErrorNotice error={library.stateProblem} />}
      {library && library.problems.length > 0 && (
        <Notice kind="warn">
          {t("themes.problems", { n: library.problems.length })}
          <ul className="plain-list">
            {library.problems.map((p) => (
              <li key={p.file}>
                <code>{p.file}</code>: <span>{pick(language, p.error.messageTh, p.error.messageEn)}</span>{" "}
                <span className="badge">{p.error.code}</span>
                {p.error.details && (
                  <details>
                    <summary>{t("common.details")}</summary>
                    <pre>{p.error.details}</pre>
                  </details>
                )}
              </li>
            ))}
          </ul>
        </Notice>
      )}
      <div className="row-actions">
        <div className="row-actions" role="group" aria-label={t("themes.filter.label")}>
          {FILTERS.map((f) => (
            <button key={f} type="button" aria-pressed={filter === f} onClick={() => setFilter(f)}>
              {t(`themes.filter.${f}`)}
            </button>
          ))}
        </div>
        <span style={{ flex: 1 }} />
        <button type="button" disabled={!library} onClick={() => void createNew()}>
          {t("themes.new")}
        </button>
        <button type="button" onClick={() => void startImport()}>
          {t("themes.import")}
        </button>
      </div>
      {!library && listError === null && <p role="status">{t("common.loading")}</p>}
      {library && visible.length === 0 && <p>{t("themes.empty")}</p>}
      {groups.map((g) => {
        const items = visible.filter((s) => s.source === g.key);
        if (items.length === 0) return null;
        return (
          <div key={g.key}>
            <h2>{t(g.title)}</h2>
            <div className="grid">
              {items.map((s) => (
                <ThemeCard key={s.id} summary={s}>
                  {/* Named by the theme, so a screen reader says whose Apply this is. */}
                  <div className="row-actions" role="group" aria-label={s.name}>
                    {/* A draft was never saved: Edit is how it gets saved and applied, since only
                        Save and apply changes the machine for it (spec §3.4). A theme of mine
                        with a draft applies its saved version, and says so (fix 4). */}
                    {s.source !== "draft" && (
                      <button
                        type="button"
                        className="primary"
                        aria-label={s.source === "mine" && s.hasDraft ? t("themes.applySaved") : undefined}
                        onClick={() => startApply(s)}
                      >
                        {t("themes.apply")}
                      </button>
                    )}
                    {s.source === "preset" && (
                      <button type="button" onClick={() => void customize(s)}>
                        {t("common.customize")}
                      </button>
                    )}
                    {s.source !== "preset" && (
                      <button type="button" onClick={() => go({ name: "editor", id: s.id })}>
                        {t("common.edit")}
                      </button>
                    )}
                    {s.source === "mine" && (
                      <>
                        <button type="button" onClick={() => openRename(s)}>
                          {t("common.rename")}
                        </button>
                        <button type="button" onClick={() => void duplicate(s)}>
                          {t("common.duplicate")}
                        </button>
                        <button type="button" onClick={() => void exportTheme(s)}>
                          {t("common.export")}
                        </button>
                        <button type="button" className="danger" onClick={() => openDelete(s)}>
                          {t("common.delete")}
                        </button>
                      </>
                    )}
                    {s.source === "draft" && (
                      <button type="button" className="danger" onClick={() => openDelete(s)}>
                        {t("themes.discardDraft")}
                      </button>
                    )}
                  </div>
                </ThemeCard>
              ))}
            </div>
          </div>
        );
      })}
      {applying && (
        // No way to close it while the apply runs (ruling R7): its outcome, a rollback with
        // its error and "Copy details" included, must be seen.
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
      {deleting && (
        <Dialog title={deleting.source === "draft" ? t("themes.discardDraft") : t("common.delete")} onClose={() => setDeleting(null)}>
          <p>{t("themes.deleteConfirm", { name: deleting.name })}</p>
          {deleting.inUse && <p>{t("themes.deleteInUse")}</p>}
          {dialogError !== null && <ErrorNotice error={dialogError} />}
          <div className="row-actions">
            <button type="button" className="danger" onClick={() => void confirmDelete()}>
              {deleting.source === "draft" ? t("themes.discardDraft") : t("common.delete")}
            </button>
          </div>
        </Dialog>
      )}
      {renaming && (
        <Dialog title={t("common.rename")} onClose={() => setRenaming(null)}>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (newName.trim().length > 0) void confirmRename();
            }}
          >
            <label>
              {t("themes.renamePrompt")}
              <input type="text" value={newName} maxLength={64} onChange={(e) => setNewName(e.target.value)} />
            </label>
            {dialogError !== null && <ErrorNotice error={dialogError} />}
            <div className="row-actions">
              <button type="submit" className="primary" disabled={newName.trim().length === 0}>
                {t("common.save")}
              </button>
            </div>
          </form>
        </Dialog>
      )}
      {importing && (
        <Dialog title={t("themes.import.title")} onClose={() => setImporting(null)}>
          <p>
            <strong>{importing.theme.name}</strong>
            {importing.theme.author && ` · ${importing.theme.author}`}
          </p>
          {importing.idTaken && <p>{t("themes.import.idTaken")}</p>}
          {importing.nameTaken && <p>{t("themes.import.nameTaken")}</p>}
          <TerminalPreview theme={importing.theme} result={importPreview} />
          {importDetectError !== null && <ErrorNotice error={importDetectError} title={t("themes.import.systemUnknown")} />}
          {importPreviewError !== null && <ErrorNotice error={importPreviewError} />}
          {dialogError !== null && <ErrorNotice error={dialogError} />}
          <div className="row-actions">
            <button type="button" className="primary" onClick={() => void confirmImport()}>
              {t("themes.import.confirm")}
            </button>
          </div>
        </Dialog>
      )}
    </section>
  );
}
