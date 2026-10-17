// Settings (spec §4.3): where themes go (the wizard's page 4 choice), the language (remembered,
// spec §4.5) and the update mode (stored and shown here; Phase 4's updater reads it, automatic by
// default, spec §6.2). A change of where themes go changes nothing on the machine by itself:
// "Apply the current theme again" does, through the plan like every apply, and is offered only
// while a theme is applied. Each change is saved at once and said in a status region that is
// always there.
import { useEffect, useRef, useState } from "react";
import type { AppSettings } from "../bindings/AppSettings";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { Library } from "../bindings/Library";
import type { Target } from "../bindings/Target";
import type { UpdateMode } from "../bindings/UpdateMode";
import { ApplyFlow } from "../components/ApplyFlow";
import { Dialog } from "../components/Dialog";
import { ErrorNotice } from "../components/Notices";
import { useI18n, type Language } from "../i18n";
import { useIpc } from "../ipc/context";
import { useAppState } from "../state";

const TARGETS: Target[] = ["all", "wt"];
const LANGUAGES: Language[] = ["en", "th"];
const UPDATES: UpdateMode[] = ["auto", "notify", "off"];

export function Settings() {
  const ipc = useIpc();
  const { t } = useI18n();
  const { settings, saveSettings, setLastApply, bump, version } = useAppState();
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [library, setLibrary] = useState<Library | null>(null);
  const [libraryError, setLibraryError] = useState<unknown>(null);
  const [applying, setApplying] = useState<{ key: number; request: ApplyRequest } | null>(null);
  const [applyBusy, setApplyBusy] = useState(false);
  const applies = useRef(0);

  // Which theme is applied, for "Apply the current theme again".
  useEffect(() => {
    let alive = true;
    ipc.themesList().then(
      (lib) => {
        if (!alive) return;
        setLibrary(lib);
        setLibraryError(null);
      },
      (e: unknown) => {
        if (alive) setLibraryError(e);
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc, version]);

  const change = async (next: Partial<AppSettings>) => {
    setError(null);
    setSaved(false);
    try {
      await saveSettings({ ...settings, ...next });
      setSaved(true);
    } catch (e) {
      setError(e);
    }
  };

  const applied = library?.applied ?? null;

  // A flow of its own each time (a new key), with the choice as it is saved now.
  const applyAgain = () => {
    if (!applied) return;
    applies.current += 1;
    setApplyBusy(false);
    setApplying({
      key: applies.current,
      request: {
        themeId: applied.themeId,
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

  return (
    <section>
      <h1 tabIndex={-1}>{t("settings.title")}</h1>
      {error !== null && <ErrorNotice error={error} />}
      {/* Always there, its text toggled (2c item 10). */}
      <p role="status">{saved ? t("settings.saved") : ""}</p>
      <fieldset>
        <legend>{t("settings.target.title")}</legend>
        {TARGETS.map((target) => (
          <label className="row" key={target}>
            <input type="radio" name="settings-target" checked={settings.target === target} onChange={() => void change({ target })} />
            <span>
              {t(target === "all" ? "wizard.where.all" : "wizard.where.wt")}
              <br />
              <small>{t(target === "all" ? "wizard.where.allHint" : "wizard.where.wtHint")}</small>
            </span>
          </label>
        ))}
        {/* On a line of its own below the two choices, as on the wizard's page 4: it belongs to
            neither of them (V3). */}
        <div>
          <label className="row">
            <input
              type="checkbox"
              checked={settings.setWtDefaultProfile}
              onChange={(e) => void change({ setWtDefaultProfile: e.target.checked })}
            />
            {t("wizard.where.setDefault")}
          </label>
        </div>
        {libraryError !== null && <ErrorNotice error={libraryError} />}
        {applied && (
          <div className="row-actions">
            <button type="button" onClick={applyAgain}>
              {t("settings.target.applyAgain")}
            </button>
          </div>
        )}
      </fieldset>
      <fieldset>
        <legend>{t("settings.language.title")}</legend>
        {LANGUAGES.map((language) => (
          <label className="row" key={language}>
            <input type="radio" name="settings-language" checked={settings.language === language} onChange={() => void change({ language })} />
            {/* Each language's name in that language, for a screen reader (spec §4.5). */}
            <span lang={language}>{t(`language.${language}`)}</span>
          </label>
        ))}
      </fieldset>
      <fieldset>
        <legend>{t("settings.updates.title")}</legend>
        {UPDATES.map((mode) => (
          <label className="row" key={mode}>
            <input type="radio" name="settings-updates" checked={settings.updates === mode} onChange={() => void change({ updates: mode })} />
            {t(`settings.updates.${mode}`)}
          </label>
        ))}
        <p>
          <small>{t("settings.updates.note")}</small>
        </p>
      </fieldset>
      {applying && (
        // No way to close it while the apply runs (ruling R7); it stays open on its outcome.
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
