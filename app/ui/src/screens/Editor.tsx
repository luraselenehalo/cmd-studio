// The theme editor (spec §3.4): simple mode (tone, main color, tint → every color from the
// Rust palette), advanced mode (all 23 colors), the readability panel with "Fix automatically",
// font and cursor, the prompt's look and segments (order by buttons, keyboard-reachable), the
// welcome box, the live preview painted from the core's spans with this computer's sample
// (ruling R1), drafts saved one second after an edit, Ctrl+Z / Ctrl+Y with 100 steps, and "Save
// and apply" as the only action that changes the machine. Presets never open here: Customize
// copies one first.
import { useEffect, useLayoutEffect, useReducer, useRef, useState, type KeyboardEvent } from "react";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { ContrastIssue } from "../bindings/ContrastIssue";
import type { CursorShape } from "../bindings/CursorShape";
import type { Layout } from "../bindings/Layout";
import type { PreviewContext } from "../bindings/PreviewContext";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { Segment } from "../bindings/Segment";
import type { SystemReport } from "../bindings/SystemReport";
import type { Theme } from "../bindings/Theme";
import type { Tone } from "../bindings/Tone";
import { ApplyFlow } from "../components/ApplyFlow";
import { Dialog } from "../components/Dialog";
import { ErrorNotice, Notice } from "../components/Notices";
import { TerminalPreview } from "../components/TerminalPreview";
import { useI18n, type Key } from "../i18n";
import { useIpc } from "../ipc/context";
import { useNav } from "../router";
import { CORE_SAMPLE_IDENTITY, sampleContext } from "../sample";
import { useAppState } from "../state";
import { getSlot, initialState, reduce, type ColorSlot, type EditorAction, type EditorState } from "./editor/reducer";
import { clamp, NumberField } from "./editor/fields";
import { useDebounced } from "./editor/hooks";

export const NERD_FONT = "CaskaydiaCove NF";
export const DRAFT_DELAY_MS = 1000;
/** The preview follows an edit once it settles, so dragging a slider does not flood the IPC. */
const PREVIEW_DELAY_MS = 150;
/** The tint simple mode shows for a theme whose recipe a hand edit removed (spec §3.1's default). */
const DEFAULT_TINT = 0.1;
const SEGMENTS: Segment[] = ["path", "git", "time", "user", "host", "status", "duration"];
const BASIC_SLOTS: ColorSlot[] = ["background", "foreground", "cursor", "selection", "accent", "muted", "frame"];
const ANSI_SLOTS: ColorSlot[] = Array.from({ length: 16 }, (_, i) => `ansi${i}` as ColorSlot);
const CURSOR_SHAPES: CursorShape[] = ["filledBox", "bar", "underscore"];
const LAYOUTS: Layout[] = ["box", "line", "powerline"];

/** The reducer before a theme is loaded: only `load` makes a state. */
function reduceNullable(state: EditorState | null, action: EditorAction): EditorState | null {
  if (state === null) return action.type === "load" ? reduce(initialState(action.theme, action.savedTheme), action) : null;
  return reduce(state, action);
}

export function Editor({ id }: { id: string }) {
  const ipc = useIpc();
  const { t } = useI18n();
  const { go } = useNav();
  const { settings, setLastApply, bump } = useAppState();
  const [state, dispatch] = useReducer(reduceNullable, null);
  const [loadError, setLoadError] = useState<unknown>(null);
  const [error, setError] = useState<unknown>(null);
  /** A draft left from before, offered when the theme opens (spec §3.4). */
  const [draftOffer, setDraftOffer] = useState<{ draft: Theme; stored: Theme } | null>(null);
  const [offerError, setOfferError] = useState<unknown>(null);
  /** The last draft save failed: said, never swallowed. */
  const [draftError, setDraftError] = useState<unknown>(null);
  const [preview, setPreview] = useState<PreviewResult | null>(null);
  const [previewError, setPreviewError] = useState<unknown>(null);
  const [issues, setIssues] = useState<ContrastIssue[]>([]);
  const [checkError, setCheckError] = useState<unknown>(null);
  const [report, setReport] = useState<SystemReport | null>(null);
  /** `system_detect` failed: the sample is the core's and there is no font list (fix 3). */
  const [detectError, setDetectError] = useState<unknown>(null);
  const [sample, setSample] = useState<PreviewContext | null>(null);
  const [customFont, setCustomFont] = useState(false);
  const [powerlineNotice, setPowerlineNotice] = useState(false);
  const [status, setStatus] = useState<Key | null>(null);
  const [applying, setApplying] = useState<{ key: number; request: ApplyRequest } | null>(null);
  const [applyBusy, setApplyBusy] = useState(false);
  /** The theme the last draft save wrote (or the draft that was taken up): not written again. */
  const lastDraft = useRef<Theme | null>(null);
  const applies = useRef(0);
  /** Counts the regenerates of simple mode: only the newest one's answer is used (item 5). */
  const regenerations = useRef(0);
  const heading = useRef<HTMLHeadingElement>(null);
  const offered = useRef(false);
  /** Every draft write, one after the other; a save waits for the last (fix 5). */
  const pendingDraft = useRef<Promise<void>>(Promise.resolve());
  /** A save of the theme is running: no draft is started meanwhile (it would land after it). */
  const saving = useRef(false);
  /** The user chose to go back without the draft: leaving writes none. */
  const abandoned = useRef(false);
  /** What the page holds as of its last render, for the write on leaving (fix 1). */
  const latest = useRef<{ state: EditorState | null; draftOffer: unknown }>({ state: null, draftOffer: null });
  useLayoutEffect(() => {
    latest.current = { state, draftOffer };
  });

  /** Writes `draft` after any draft write still running, so they land in order. */
  const writeDraft = (draft: Theme): Promise<void> => {
    const write = pendingDraft.current
      .then(() => ipc.themeSave(draft, true))
      .then(() => {
        lastDraft.current = draft;
      });
    pendingDraft.current = write.catch(() => {});
    return write;
  };

  // Leaving by any way (the sidebar, another theme, a later screen) unmounts the page before
  // the draft second has passed: the edit still waiting is written then (spec §3.4). The page
  // that could say a failure is gone; the theme then opens without that draft. In StrictMode's
  // trial unmount nothing is loaded yet, so nothing is written.
  useEffect(
    () => () => {
      const { state: last, draftOffer: offer } = latest.current;
      if (!last || offer || abandoned.current || !last.dirty || last.theme === lastDraft.current) return;
      writeDraft(last.theme).catch(() => {});
    },
    // Once, on unmount; `writeDraft` reads only refs and the IPC.
    [ipc],
  );

  // The theme, its draft if any, and this computer's facts (fonts, the preview's sample).
  useEffect(() => {
    let alive = true;
    const detect = ipc.systemDetect().then(
      (report) => ({ report, error: null as unknown }),
      (error: unknown) => ({ report: null, error }),
    );
    Promise.all([ipc.themeGet(id), detect]).then(
      ([detail, sys]) => {
        if (!alive) return;
        setReport(sys.report);
        setDetectError(sys.error);
        setSample(sampleContext(sys.report?.previewSample ?? CORE_SAMPLE_IDENTITY));
        dispatch({ type: "load", theme: detail.theme });
        if (detail.draft) setDraftOffer({ draft: detail.draft, stored: detail.theme });
      },
      (e: unknown) => {
        if (alive) setLoadError(e);
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc, id]);

  // The draft offer opened as the page did, so what had focus before it is gone: once it is
  // answered, focus goes to the page's heading (after the dialog's own clean-up has run).
  useEffect(() => {
    if (draftOffer) offered.current = true;
    else if (offered.current) {
      offered.current = false;
      heading.current?.focus();
    }
  }, [draftOffer]);

  const theme = state?.theme ?? null;
  const previewTheme = useDebounced(theme, PREVIEW_DELAY_MS);
  const draftTheme = useDebounced(theme, DRAFT_DELAY_MS);

  // The preview and the readability check, from the core, after each edit settles.
  useEffect(() => {
    if (!previewTheme || !sample) return;
    let alive = true;
    ipc.previewRender(previewTheme, sample).then(
      (r) => {
        if (!alive) return;
        setPreview(r);
        setPreviewError(null);
      },
      (e: unknown) => {
        if (alive) setPreviewError(e);
      },
    );
    ipc.contrastCheck(previewTheme.colors).then(
      (list) => {
        if (!alive) return;
        setIssues(list);
        setCheckError(null);
      },
      (e: unknown) => {
        if (alive) setCheckError(e);
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc, previewTheme, sample]);

  // The draft, a second after the last edit (spec §3.4): only while the theme on screen is that
  // settled one, differs from the stored theme, and was not written already, and while no save
  // of the theme runs. A failure is said.
  useEffect(() => {
    if (!draftTheme || !state || draftTheme !== state.theme) return;
    if (!state.dirty || draftOffer || saving.current || draftTheme === lastDraft.current) return;
    let alive = true;
    writeDraft(draftTheme).then(
      () => {
        if (!alive) return;
        setDraftError(null);
        setStatus("editor.draftSaved");
      },
      (e: unknown) => {
        if (!alive) return;
        setDraftError(e);
        setStatus(null);
      },
    );
    return () => {
      alive = false;
    };
    // Runs for a newly settled theme only; `state` and `draftOffer` are read as they are then.
  }, [ipc, draftTheme]);

  if (loadError !== null) {
    return (
      <section>
        <h1 tabIndex={-1}>{t("editor.title")}</h1>
        <ErrorNotice error={loadError} />
        <div className="row-actions">
          <button type="button" onClick={() => go({ name: "themes" })}>
            {t("editor.back")}
          </button>
        </div>
      </section>
    );
  }
  if (!state || !theme || !sample) return <p role="status">{t("common.loading")}</p>;

  const edit = (next: Theme) => dispatch({ type: "edit", theme: next });
  const recipe = theme.simple ?? { accent: theme.colors.accent, tint: DEFAULT_TINT };

  const onKey = (e: KeyboardEvent<HTMLElement>) => {
    const target = e.target as HTMLElement;
    // A text field keeps its own undo; a dialog's keys are its own.
    if (target instanceof HTMLTextAreaElement) return;
    if (target instanceof HTMLInputElement && (target.type === "text" || target.type === "number")) return;
    if (target.closest('[role="dialog"]')) return;
    if (!e.ctrlKey || e.altKey || e.metaKey) return;
    // With a Thai keyboard layout `key` is a Thai letter: the physical key then tells.
    const typed = e.key.toLowerCase();
    const latin = /^[a-z]$/.test(typed);
    const isKey = (letter: "z" | "y") => typed === letter || (!latin && e.code === `Key${letter.toUpperCase()}`);
    if (isKey("z") && !e.shiftKey) {
      e.preventDefault();
      dispatch({ type: "undo" });
    } else if (isKey("y") || (isKey("z") && e.shiftKey)) {
      e.preventDefault();
      dispatch({ type: "redo" });
    }
  };

  // Simple mode: every color from the Rust palette, the tone with them, as ONE undo step (R9).
  // An answer that comes late is dropped when another regenerate started after it, or when the
  // theme on screen changed meanwhile (a color set by hand above all): it would overwrite them
  // (item 5).
  const regenerate = async (tone: Tone, accent: string, tint: number) => {
    const n = ++regenerations.current;
    const from = theme;
    setError(null);
    try {
      const colors = await ipc.paletteGenerate(tone, accent, tint);
      if (n !== regenerations.current || latest.current.state?.theme !== from) return;
      dispatch({ type: "setColors", colors, simple: { accent, tint }, tone });
    } catch (e) {
      if (n === regenerations.current) setError(e);
    }
  };

  const fixReadability = async () => {
    setError(null);
    try {
      const colors = await ipc.contrastFix(theme.colors);
      dispatch({ type: "setColors", colors, simple: null, tone: theme.tone });
    } catch (e) {
      setError(e);
    }
  };

  // Powerline needs the Nerd Font's glyphs: the font switches to it, and the page says so.
  const setLayout = (layout: Layout) => {
    let next: Theme = { ...theme, prompt: { ...theme.prompt, layout } };
    if (layout === "powerline" && theme.terminal.font !== NERD_FONT) {
      next = { ...next, terminal: { ...next.terminal, font: NERD_FONT } };
      setPowerlineNotice(true);
      setCustomFont(false);
    }
    edit(next);
  };

  const toggleSegment = (segment: Segment, on: boolean) => {
    const segments = on ? [...theme.prompt.segments, segment] : theme.prompt.segments.filter((s) => s !== segment);
    edit({ ...theme, prompt: { ...theme.prompt, segments } });
  };

  const moveSegment = (segment: Segment, by: -1 | 1) => {
    const list = [...theme.prompt.segments];
    const i = list.indexOf(segment);
    const j = i + by;
    const other = list[j];
    if (i < 0 || other === undefined) return;
    list[j] = segment;
    list[i] = other;
    edit({ ...theme, prompt: { ...theme.prompt, segments: list } });
  };

  const save = async (): Promise<Theme | null> => {
    setError(null);
    // The very object sent: `dirty` compares the stored theme by identity.
    const sent = theme;
    saving.current = true;
    try {
      // A draft write still running would land after the save and be offered again (fix 5).
      await pendingDraft.current;
      await ipc.themeSave(sent, false);
      dispatch({ type: "saved", theme: sent });
      // The save took the draft away (library.rs), so the next edit writes a new one.
      lastDraft.current = null;
      setDraftError(null);
      setStatus("editor.saved");
      return sent;
    } catch (e) {
      setError(e);
      return null;
    } finally {
      saving.current = false;
    }
  };

  // R12: the theme itself is saved first; then the plan of applying it is shown.
  const saveAndApply = async () => {
    const saved = await save();
    if (!saved) return;
    applies.current += 1;
    setApplyBusy(false);
    setApplying({
      key: applies.current,
      request: {
        themeId: saved.id,
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

  // Back to the library: an edit still waiting for its draft second is written first, so
  // leaving loses nothing; when that fails, the page says so and stays.
  const leave = async () => {
    if (state.dirty && !draftOffer && theme !== lastDraft.current) {
      try {
        await writeDraft(theme);
      } catch (e) {
        setDraftError(e);
        setStatus(null);
        return;
      }
    }
    go({ name: "themes" });
  };

  const keepDraft = () => {
    if (!draftOffer) return;
    // Starts dirty: the draft differs from the stored theme until it is saved.
    dispatch({ type: "load", theme: draftOffer.draft, savedTheme: draftOffer.stored });
    lastDraft.current = draftOffer.draft;
    setDraftOffer(null);
  };

  const discardDraft = async () => {
    setOfferError(null);
    try {
      await ipc.themeDelete(id, true);
      setDraftOffer(null);
    } catch (e) {
      setOfferError(e);
    }
  };

  const fonts = report?.fonts.monospace ?? [];
  const knownFont = fonts.includes(theme.terminal.font) || theme.terminal.font === NERD_FONT;
  const segmentName = (segment: Segment) => t(`editor.prompt.segment.${segment}`);

  return (
    <section onKeyDown={onKey}>
      <h1 tabIndex={-1} ref={heading}>
        {t("editor.title")}
      </h1>
      <div className="row-actions">
        <button type="button" onClick={() => void leave()}>
          {t("editor.back")}
        </button>
        <button type="button" disabled={state.past.length === 0} onClick={() => dispatch({ type: "undo" })}>
          {t("editor.undo")}
        </button>
        <button type="button" disabled={state.future.length === 0} onClick={() => dispatch({ type: "redo" })}>
          {t("editor.redo")}
        </button>
        <button type="button" onClick={() => void save()}>
          {t("editor.save")}
        </button>
        <button type="button" className="primary" onClick={() => void saveAndApply()}>
          {t("editor.saveApply")}
        </button>
        <span role="status">{status ? t(status) : ""}</span>
      </div>
      {error !== null && <ErrorNotice error={error} />}
      {detectError !== null && <ErrorNotice error={detectError} title={t("editor.systemUnknown")} />}
      {draftError !== null && (
        <>
          <ErrorNotice error={draftError} title={t("editor.draftNotSaved")} />
          <div className="row-actions">
            <button
              type="button"
              className="link"
              onClick={() => {
                abandoned.current = true;
                go({ name: "themes" });
              }}
            >
              {t("editor.leaveWithoutDraft")}
            </button>
          </div>
        </>
      )}
      <div className="editor">
        <div>
          <fieldset>
            <legend>{t("common.name")}</legend>
            <label>
              {t("editor.name")}
              <input type="text" value={theme.name} maxLength={64} onChange={(e) => edit({ ...theme, name: e.target.value })} />
            </label>
            <label>
              {t("editor.author")}
              <input type="text" value={theme.author} maxLength={64} onChange={(e) => edit({ ...theme, author: e.target.value })} />
            </label>
          </fieldset>

          {/* Two toggle buttons in a named group, as the library's filter is (item 9): the tab
              pattern would need arrow keys and tab panels, which this switch does not have. */}
          <div className="tabs" role="group" aria-label={t("editor.mode.label")}>
            <button type="button" aria-pressed={state.mode === "simple"} onClick={() => dispatch({ type: "setMode", mode: "simple" })}>
              {t("editor.mode.simple")}
            </button>
            <button type="button" aria-pressed={state.mode === "advanced"} onClick={() => dispatch({ type: "setMode", mode: "advanced" })}>
              {t("editor.mode.advanced")}
            </button>
          </div>

          {state.mode === "simple" && (
            <fieldset>
              <legend>{t("editor.mode.simple")}</legend>
              {state.keptEditsNotice && <Notice>{t("editor.simple.keptNotice")}</Notice>}
              <div role="radiogroup" aria-label={t("editor.simple.tone")}>
                {(["dark", "light"] as Tone[]).map((tone) => (
                  <label className="row" key={tone}>
                    <input
                      type="radio"
                      name="tone"
                      checked={theme.tone === tone}
                      onChange={() => void regenerate(tone, recipe.accent, recipe.tint)}
                    />
                    {t(tone === "dark" ? "editor.simple.dark" : "editor.simple.light")}
                  </label>
                ))}
              </div>
              <label className="row">
                {t("editor.simple.accent")}
                <input type="color" value={recipe.accent} onChange={(e) => void regenerate(theme.tone, e.target.value, recipe.tint)} />
                <code>{recipe.accent}</code>
              </label>
              <label className="row">
                {t("editor.simple.tint")}
                <input
                  type="range"
                  min={0}
                  max={0.3}
                  step={0.01}
                  value={recipe.tint}
                  onChange={(e) => void regenerate(theme.tone, recipe.accent, Number(e.target.value))}
                />
                <span>{recipe.tint.toFixed(2)}</span>
              </label>
            </fieldset>
          )}

          {state.mode === "advanced" && (
            <fieldset>
              <legend>{t("editor.colors.title")}</legend>
              <div className="color-grid">
                {BASIC_SLOTS.map((slot) => (
                  <label key={slot}>
                    <input type="color" value={getSlot(theme.colors, slot)} onChange={(e) => dispatch({ type: "setColor", slot, value: e.target.value })} />
                    {t(`editor.colors.${slot}` as Key)}
                  </label>
                ))}
              </div>
              <h3>{t("editor.colors.ansi")}</h3>
              <div className="color-grid">
                {ANSI_SLOTS.map((slot) => (
                  <label key={slot}>
                    <input type="color" value={getSlot(theme.colors, slot)} onChange={(e) => dispatch({ type: "setColor", slot, value: e.target.value })} />
                    {t(`editor.colors.${slot}` as Key)}
                  </label>
                ))}
              </div>
            </fieldset>
          )}

          <fieldset>
            <legend>{t("editor.readability.title")}</legend>
            {checkError !== null && <ErrorNotice error={checkError} />}
            {issues.length === 0 ? (
              checkError === null && <p className="status-ok">{t("editor.readability.ok")}</p>
            ) : (
              <>
                <ul className="plain-list">
                  {issues.map((issue) => (
                    <li key={issue.slot} className="status-warn">
                      {t("editor.readability.issue", {
                        slot: t(`editor.colors.${issue.slot}` as Key),
                        ratio: issue.ratio.toFixed(1),
                        required: issue.required.toFixed(1),
                      })}
                    </li>
                  ))}
                </ul>
                <button type="button" onClick={() => void fixReadability()}>
                  {t("editor.readability.fix")}
                </button>
              </>
            )}
          </fieldset>

          <fieldset>
            <legend>{t("editor.font.title")}</legend>
            {powerlineNotice && <Notice>{t("editor.font.powerlineNotice")}</Notice>}
            <label>
              {t("editor.font.family")}
              <select
                value={customFont || !knownFont ? "__custom" : theme.terminal.font}
                onChange={(e) => {
                  if (e.target.value === "__custom") setCustomFont(true);
                  else {
                    setCustomFont(false);
                    edit({ ...theme, terminal: { ...theme.terminal, font: e.target.value } });
                  }
                }}
              >
                {[NERD_FONT, ...fonts.filter((f) => f !== NERD_FONT)].map((f) => (
                  <option key={f} value={f}>
                    {f}
                  </option>
                ))}
                <option value="__custom">{t("editor.font.custom")}</option>
              </select>
            </label>
            {(customFont || !knownFont) && (
              <label>
                {t("editor.font.custom")}
                <input
                  type="text"
                  value={theme.terminal.font}
                  maxLength={64}
                  onChange={(e) => edit({ ...theme, terminal: { ...theme.terminal, font: e.target.value } })}
                />
              </label>
            )}
            <NumberField
              label={t("editor.font.size")}
              value={theme.terminal.fontSize}
              min={8}
              max={32}
              onChange={(fontSize) => edit({ ...theme, terminal: { ...theme.terminal, fontSize } })}
            />
            <label>
              {t("editor.font.cursor")}
              <select
                value={theme.terminal.cursorShape}
                onChange={(e) => edit({ ...theme, terminal: { ...theme.terminal, cursorShape: e.target.value as CursorShape } })}
              >
                {CURSOR_SHAPES.map((shape) => (
                  <option key={shape} value={shape}>
                    {t(`editor.font.cursor.${shape}`)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("editor.font.opacity")}
              <input
                type="range"
                min={30}
                max={100}
                step={1}
                value={theme.terminal.opacity}
                onChange={(e) => edit({ ...theme, terminal: { ...theme.terminal, opacity: clamp(Number(e.target.value), 30, 100) } })}
              />
              <span>{theme.terminal.opacity}%</span>
            </label>
            <label className="row">
              <input type="checkbox" checked={theme.terminal.acrylic} onChange={(e) => edit({ ...theme, terminal: { ...theme.terminal, acrylic: e.target.checked } })} />
              {t("editor.font.acrylic")}
            </label>
            <NumberField
              label={t("editor.font.paddingX")}
              value={theme.terminal.padding[0]}
              min={0}
              max={40}
              onChange={(x) => edit({ ...theme, terminal: { ...theme.terminal, padding: [x, theme.terminal.padding[1]] } })}
            />
            <NumberField
              label={t("editor.font.paddingY")}
              value={theme.terminal.padding[1]}
              min={0}
              max={40}
              onChange={(y) => edit({ ...theme, terminal: { ...theme.terminal, padding: [theme.terminal.padding[0], y] } })}
            />
          </fieldset>

          <fieldset>
            <legend>{t("editor.prompt.title")}</legend>
            <div role="radiogroup" aria-label={t("editor.prompt.layout")}>
              {LAYOUTS.map((layout) => (
                <label className="row" key={layout}>
                  <input type="radio" name="layout" checked={theme.prompt.layout === layout} onChange={() => setLayout(layout)} />
                  {t(`editor.prompt.layout.${layout}`)}
                </label>
              ))}
            </div>
            <h3>{t("editor.prompt.segments")}</h3>
            <ul className="plain-list segments">
              {theme.prompt.segments.map((segment, i) => (
                <li key={segment}>
                  <label className="row">
                    <input type="checkbox" checked onChange={() => toggleSegment(segment, false)} />
                    {segmentName(segment)}
                  </label>
                  <button
                    type="button"
                    disabled={i === 0}
                    aria-label={t("editor.prompt.moveUp", { segment: segmentName(segment) })}
                    onClick={() => moveSegment(segment, -1)}
                  >
                    ↑
                  </button>
                  <button
                    type="button"
                    disabled={i === theme.prompt.segments.length - 1}
                    aria-label={t("editor.prompt.moveDown", { segment: segmentName(segment) })}
                    onClick={() => moveSegment(segment, 1)}
                  >
                    ↓
                  </button>
                </li>
              ))}
              {SEGMENTS.filter((s) => !theme.prompt.segments.includes(s)).map((segment) => (
                <li key={segment}>
                  <label className="row">
                    <input type="checkbox" checked={false} onChange={() => toggleSegment(segment, true)} />
                    {segmentName(segment)}
                  </label>
                </li>
              ))}
            </ul>
            <label className="row">
              <input type="checkbox" checked={theme.prompt.transient} onChange={(e) => edit({ ...theme, prompt: { ...theme.prompt, transient: e.target.checked } })} />
              {t("editor.prompt.transient")}
            </label>
          </fieldset>

          <fieldset>
            <legend>{t("editor.banner.title")}</legend>
            <label className="row">
              <input type="checkbox" checked={theme.banner.enabled} onChange={(e) => edit({ ...theme, banner: { ...theme.banner, enabled: e.target.checked } })} />
              {t("editor.banner.enabled")}
            </label>
            <label>
              {t("editor.banner.titleField")}
              <input type="text" value={theme.banner.title} maxLength={80} onChange={(e) => edit({ ...theme, banner: { ...theme.banner, title: e.target.value } })} />
            </label>
            <p>
              <small>{t("editor.banner.variables")}</small>
            </p>
            <div>{t("editor.banner.lines")}</div>
            {[0, 1, 2, 3].map((i) => (
              <label key={i}>
                {t("editor.banner.line", { n: i + 1 })}
                <input
                  type="text"
                  value={theme.banner.lines[i] ?? ""}
                  maxLength={80}
                  onChange={(e) => {
                    const lines = [...theme.banner.lines];
                    while (lines.length <= i) lines.push("");
                    lines[i] = e.target.value;
                    while (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
                    edit({ ...theme, banner: { ...theme.banner, lines } });
                  }}
                />
              </label>
            ))}
            <label className="row">
              <input type="checkbox" checked={theme.banner.tips} onChange={(e) => edit({ ...theme, banner: { ...theme.banner, tips: e.target.checked } })} />
              {t("editor.banner.tips")}
            </label>
            <label className="row">
              <input type="checkbox" checked={theme.banner.clearLogo} onChange={(e) => edit({ ...theme, banner: { ...theme.banner, clearLogo: e.target.checked } })} />
              {t("editor.banner.clearLogo")}
            </label>
          </fieldset>
        </div>

        <div className="preview-column">
          <h2 style={{ marginTop: 0 }}>{t("editor.preview.title")}</h2>
          <TerminalPreview theme={theme} result={preview} />
          {previewError !== null && <ErrorNotice error={previewError} />}
          <label>
            {t("editor.preview.user")}
            <input type="text" value={sample.user} maxLength={64} onChange={(e) => setSample({ ...sample, user: e.target.value })} />
          </label>
          <label>
            {t("editor.preview.folder")}
            <input type="text" value={sample.cwd} maxLength={200} onChange={(e) => setSample({ ...sample, cwd: e.target.value })} />
          </label>
          <NumberField label={t("editor.preview.width")} value={sample.width} min={20} max={200} onChange={(width) => setSample({ ...sample, width })} />
        </div>
      </div>
      {draftOffer && (
        // A choice, not a notice: an edit made around it would overwrite the draft it offers.
        <Dialog title={t("editor.draftFound")}>
          {offerError !== null && <ErrorNotice error={offerError} />}
          <div className="row-actions">
            <button type="button" className="primary" onClick={keepDraft}>
              {t("editor.useDraft")}
            </button>
            <button type="button" onClick={() => void discardDraft()}>
              {t("editor.discardDraft")}
            </button>
          </div>
        </Dialog>
      )}
      {applying && (
        // No way to close it while the apply runs (ruling R7).
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
