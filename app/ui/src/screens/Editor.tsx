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
import { useAppState, type DraftProblemReport } from "../state";
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
  const { settings, setLastApply, draftProblems, bump } = useAppState();
  // Both never change: the unmount below may call them.
  const { report: reportDraftProblem, clear: clearDraftProblem } = draftProblems;
  const [state, dispatch] = useReducer(reduceNullable, null);
  const [loadError, setLoadError] = useState<unknown>(null);
  const [error, setError] = useState<unknown>(null);
  /** A draft left from before, offered when the theme opens (spec §3.4). */
  const [draftOffer, setDraftOffer] = useState<{ draft: Theme; stored: Theme } | null>(null);
  const [offerError, setOfferError] = useState<unknown>(null);
  /** The last draft write, or the last removal of a draft an undo made needless (item 1),
   * failed: said, never swallowed. */
  const [draftError, setDraftError] = useState<{ error: unknown; removal: boolean } | null>(null);
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
  /** The theme the last draft write wrote (or the draft that was taken up), as landed. Null while
   * no draft of this page's is stored (none yet, or a save or removal took it). */
  const lastDraft = useRef<Theme | null>(null);
  /** What the draft file will hold once the writes queued so far land: the theme of the last
   * write queued, set when it is queued, not when it lands. What "written already" compares with,
   * so an undo made while a write is on its way is written after it (round 3, C). */
  const queuedDraft = useRef<Theme | null>(null);
  const applies = useRef(0);
  /** Counts the requests for a whole set of colors, the regenerates of simple mode and the
   * readability fixes alike: only the newest one's answer is used (item 5). */
  const colorRequests = useRef(0);
  const heading = useRef<HTMLHeadingElement>(null);
  const offered = useRef(false);
  /** Every draft write and removal, one after the other; a save waits for the last (fix 5). */
  const pendingDraft = useRef<Promise<void>>(Promise.resolve());
  /** The save of the theme that is running, if any (it never rejects): no draft is started
   * meanwhile, since it would land after the save, and leaving waits for it (item 2). */
  const saveRun = useRef<Promise<void> | null>(null);
  /** The theme the last save stored, set the moment the save lands: the page may go before it
   * draws that save, so leaving compares with this, not with the last render (item 2). */
  const lastSaved = useRef<Theme | null>(null);
  /** The theme was loaded as an unsaved draft (source "draft"): its draft file IS the theme
   * (library.rs `theme_get`), the only copy of it until a save (round 2, fix 1). */
  const draftOnly = useRef(false);
  /** The user chose to go back without the draft: leaving writes none. */
  const abandoned = useRef(false);
  /** What the page holds as of its last render, for leaving (fix 1). */
  const latest = useRef<{ state: EditorState | null; draftOffer: unknown }>({ state: null, draftOffer: null });
  useLayoutEffect(() => {
    latest.current = { state, draftOffer };
  });

  /** The theme as it is stored: the last save's, else the one the page was loaded with. */
  const storedTheme = (s: EditorState): Theme => lastSaved.current ?? s.savedTheme;

  /**
   * Whether `theme` belongs in the draft file: when it differs from the stored theme, and always
   * while the theme is draft-only and never saved, since that file is then the theme itself.
   * Back at the loaded theme it is written back, never taken away (round 2, fix 1).
   */
  const wantsDraft = (s: EditorState, theme: Theme): boolean =>
    theme !== storedTheme(s) || (draftOnly.current && lastSaved.current === null);

  /** Writes `draft` after any draft write or removal still running, so they land in order. A
   * problem said about this theme's draft before is over once a newer draft is written (round
   * 2, fix 2): its Try again would write an older one over it. So is it after a removal or a
   * save, below. */
  const writeDraft = (draft: Theme): Promise<void> => {
    queuedDraft.current = draft;
    const write = pendingDraft.current
      .then(() => ipc.themeSave(draft, true))
      .then(
        () => {
          lastDraft.current = draft;
          clearDraftProblem(id);
        },
        (e: unknown) => {
          // Not written: unless a later write was queued, the file holds what last landed.
          if (queuedDraft.current === draft) queuedDraft.current = lastDraft.current;
          throw e;
        },
      );
    pendingDraft.current = write.catch(() => {});
    return write;
  };

  /**
   * Takes the theme's draft away, after any draft write still running, when this page wrote one:
   * an undo brought the stored theme back, so the draft would be offered for nothing (item 1).
   * Resolves to whether it took one away.
   */
  const dropDraft = (): Promise<boolean> => {
    const queued = queuedDraft.current;
    const drop = pendingDraft.current.then(async () => {
      // A draft that is the theme is never taken away (round 2, fix 1).
      if (lastDraft.current === null || (draftOnly.current && lastSaved.current === null)) return false;
      await ipc.themeDelete(id, true);
      lastDraft.current = null;
      // The file holds nothing now, unless a write was queued after this removal.
      if (queuedDraft.current === queued) queuedDraft.current = null;
      clearDraftProblem(id);
      return true;
    });
    pendingDraft.current = drop.then(
      () => {},
      () => {},
    );
    return drop;
  };

  /**
   * Brings the stored draft in line with `last` as the page goes, once any save still running
   * has ended (item 2): writes `last.theme` when it belongs in the draft file (`wantsDraft`) and
   * was not written yet, or takes the page's draft away when it is the stored theme again (item
   * 1). Says whether the library changed, or what could not be done.
   */
  const settleDraft = async (last: EditorState): Promise<{ changed: boolean; problem: DraftProblemReport | null }> => {
    await saveRun.current;
    const write = wantsDraft(last, last.theme);
    try {
      if (!write) return { changed: await dropDraft(), problem: null };
      if (last.theme === queuedDraft.current) return { changed: false, problem: null };
      await writeDraft(last.theme);
      return { changed: true, problem: null };
    } catch (error) {
      // Named as the library lists it: the stored theme's name.
      const name = storedTheme(last).name;
      const problem: DraftProblemReport = write
        ? { kind: "write", id, name, draft: last.theme, error }
        : { kind: "remove", id, name, error };
      return { changed: false, problem };
    }
  };

  // Leaving by any way (the sidebar, another theme, a later screen) unmounts the page, maybe
  // before the draft second has passed or a save has landed: the draft is settled then (spec
  // §3.4). Once that lands the library is listed again, so its badges follow (item 3); a failure
  // is kept in the app state, and the next screen says it with Try again (item 4). In
  // StrictMode's trial unmount nothing is loaded yet, so nothing is done.
  useEffect(
    () => () => {
      const { state: last, draftOffer: offer } = latest.current;
      if (!last || offer || abandoned.current) return;
      void settleDraft(last).then(({ changed, problem }) => {
        if (problem) reportDraftProblem(problem);
        else if (changed) bump();
      });
    },
    // Once, on unmount: the helpers read only refs, `id` and the IPC; the app state's
    // `reportDraftProblem`, `clearDraftProblem` and `bump` never change.
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
        // A draft-only theme's draft file holds the theme as loaded: that is the draft written.
        draftOnly.current = detail.source === "draft";
        if (draftOnly.current) lastDraft.current = queuedDraft.current = detail.theme;
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

  // The draft, a second after the last edit (spec §3.4), while the theme on screen is that
  // settled one and no save of it runs: written when it differs from the stored theme and was
  // not written already; taken away when an undo brought the stored theme back (item 1). A
  // failure is said.
  useEffect(() => {
    if (!draftTheme || !state || draftTheme !== state.theme) return;
    if (draftOffer || saveRun.current) return;
    const dirty = wantsDraft(state, draftTheme);
    if (dirty && draftTheme === queuedDraft.current) return;
    let alive = true;
    (dirty ? writeDraft(draftTheme).then(() => true) : dropDraft()).then(
      (changed) => {
        if (!alive) return;
        setDraftError(null);
        if (changed) setStatus(dirty ? "editor.draftSaved" : null);
      },
      (e: unknown) => {
        if (!alive) return;
        setDraftError({ error: e, removal: !dirty });
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
  // An answer that comes late is dropped when another request for colors started after it, or
  // when the theme on screen changed meanwhile (a color set by hand above all): it would
  // overwrite them (item 5).
  const regenerate = async (tone: Tone, accent: string, tint: number) => {
    const n = ++colorRequests.current;
    const from = theme;
    setError(null);
    try {
      const colors = await ipc.paletteGenerate(tone, accent, tint);
      if (n !== colorRequests.current || latest.current.state?.theme !== from) return;
      dispatch({ type: "setColors", colors, simple: { accent, tint }, tone });
    } catch (e) {
      if (n === colorRequests.current) setError(e);
    }
  };

  // The readability fix is answered for the colors it was asked about, so it follows the same
  // rule as `regenerate`: a late answer is dropped when a newer request for colors started after
  // it, or when the theme on screen is no longer the one it was asked about.
  const fixReadability = async () => {
    const n = ++colorRequests.current;
    const from = theme;
    setError(null);
    try {
      const colors = await ipc.contrastFix(from.colors);
      if (n !== colorRequests.current || latest.current.state?.theme !== from) return;
      dispatch({ type: "setColors", colors, simple: null, tone: from.tone });
    } catch (e) {
      if (n === colorRequests.current) setError(e);
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
    const run = (async () => {
      // A draft write still running would land after the save and be offered again (fix 5).
      await pendingDraft.current;
      await ipc.themeSave(sent, false);
      // At once, not at the next render: the page may be left first (item 2). The save took
      // the draft away (library.rs), so the next edit writes a new one.
      lastSaved.current = sent;
      lastDraft.current = queuedDraft.current = null;
      clearDraftProblem(id);
    })();
    const ended = run.then(
      () => {},
      () => {},
    );
    saveRun.current = ended;
    try {
      await run;
      dispatch({ type: "saved", theme: sent });
      setDraftError(null);
      setStatus("editor.saved");
      return sent;
    } catch (e) {
      setError(e);
      return null;
    } finally {
      if (saveRun.current === ended) saveRun.current = null;
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

  // Back to the library, once any save still running has ended (item 2): an edit still waiting
  // for its draft second is written first, so leaving loses nothing; when that fails, the page
  // says so and stays. A draft an undo made needless is taken away as the page goes (the
  // unmount above), and a failure of that is said on the next screen: leaving loses nothing.
  const leave = async () => {
    await saveRun.current;
    const { state: last, draftOffer: offer } = latest.current;
    if (last && !offer && wantsDraft(last, last.theme) && last.theme !== queuedDraft.current) {
      try {
        await writeDraft(last.theme);
      } catch (e) {
        setDraftError({ error: e, removal: false });
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
    lastDraft.current = queuedDraft.current = draftOffer.draft;
    // The user keeps this draft: a problem said about it before is over, so its Try again
    // cannot take the draft away (round 3, A).
    clearDraftProblem(id);
    setDraftOffer(null);
  };

  const discardDraft = async () => {
    setOfferError(null);
    try {
      await ipc.themeDelete(id, true);
      clearDraftProblem(id);
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
          <ErrorNotice error={draftError.error} title={t(draftError.removal ? "editor.draftNotRemoved" : "editor.draftNotSaved")} />
          {/* An old draft that could not be taken away loses nothing by leaving: no way out to offer. */}
          {!draftError.removal && (
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
          )}
        </>
      )}
      <div className="editor-box">
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
            {/* What the stacked layout lifts above the form (styles.css): the heading, the preview
                and its error notice. The sample fields below stay in the page's order. */}
            <div className="preview-top">
              <h2 style={{ marginTop: 0 }}>{t("editor.preview.title")}</h2>
              <TerminalPreview theme={theme} result={preview} />
              {previewError !== null && <ErrorNotice error={previewError} />}
            </div>
            <div className="preview-sample">
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
