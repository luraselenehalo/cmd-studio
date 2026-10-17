// The editor's state (spec §3.4): the theme being edited, 100 steps of undo and redo, the
// mode, and the rule that ties them: any color edited by hand makes `simple` null; going back
// to simple mode keeps those edits, with a notice, until the tone, the main color or the tint
// change, which regenerates every color (through `palette_generate`, in Rust).
//
// This module holds no palette, contrast or prompt logic (spec §5.6): it only stores the
// colors it is given.
import type { Colors } from "../../bindings/Colors";
import type { SimpleRecipe } from "../../bindings/SimpleRecipe";
import type { Theme } from "../../bindings/Theme";
import type { Tone } from "../../bindings/Tone";

export const HISTORY = 100;

export type Mode = "simple" | "advanced";

export interface EditorState {
  theme: Theme;
  past: Theme[];
  future: Theme[];
  mode: Mode;
  /** Simple mode is shown while `simple` is null: the hand edits are kept (spec §3.4). */
  keptEditsNotice: boolean;
  /** The theme as it is stored: what the last load or the last save held. */
  savedTheme: Theme;
  /**
   * Changed since it was stored. DERIVED, never set by a case: `theme !== savedTheme`, by
   * identity (history restores the same object references, so an undo back to the stored
   * theme is clean). `reduce` computes it in one place, after every action.
   */
  dirty: boolean;
}

export type ColorSlot = Exclude<keyof Colors, "ansi"> | `ansi${number}`;

export type EditorAction =
  /**
   * Start editing `theme`. `savedTheme` is the theme as it is stored; it defaults to `theme`
   * (a fresh load, which starts clean). For a draft it is the stored theme, so a loaded draft
   * starts dirty.
   */
  | { type: "load"; theme: Theme; savedTheme?: Theme; mode?: Mode }
  /** A field other than a color (name, font, prompt, banner…): the new theme. */
  | { type: "edit"; theme: Theme }
  /**
   * One color by hand (advanced mode): `simple` becomes null.
   *
   * Known limit (v0.1): each change is one undo step, so dragging the color picker can take
   * many steps to undo. Folding a drag into one step is left for a later version.
   */
  | { type: "setColor"; slot: ColorSlot; value: string }
  /**
   * Every color at once: simple mode's recipe (the Rust palette), or a readability fix
   * (`simple: null`). It carries the tone too (ruling R9), so a change of tone is ONE undo
   * step: before, the tone and the palette changed in two steps and the step between paired
   * the new palette with the old tone. A caller that does not change the tone passes the
   * current one.
   */
  | { type: "setColors"; colors: Colors; simple: SimpleRecipe | null; tone: Tone }
  | { type: "setMode"; mode: Mode }
  | { type: "undo" }
  | { type: "redo" }
  /**
   * `theme` was saved: the very object that was sent (keep a reference to it when the save
   * starts, do not pass a copy the backend returned: dirty compares by identity). An edit made
   * while the save was awaited stays dirty.
   */
  | { type: "saved"; theme: Theme };

export function initialState(theme: Theme, savedTheme: Theme = theme): EditorState {
  return {
    theme,
    savedTheme,
    past: [],
    future: [],
    mode: theme.simple ? "simple" : "advanced",
    keptEditsNotice: false,
    dirty: theme !== savedTheme,
  };
}

function push(state: EditorState, theme: Theme): EditorState {
  const past = [...state.past, state.theme];
  if (past.length > HISTORY) past.shift();
  return { ...state, theme, past, future: [] };
}

/** The notice (spec §3.4) is shown in simple mode while the theme on screen has no recipe. */
function keptEdits(mode: Mode, theme: Theme): boolean {
  return mode === "simple" && theme.simple === null;
}

function sameRecipe(a: SimpleRecipe | null, b: SimpleRecipe | null): boolean {
  if (a === null || b === null) return a === b;
  return a.accent === b.accent && a.tint === b.tint;
}

/** The index of an `ansiN` slot, or null when `N` is not a whole number (`ansi`, `ansi-1`, `ansi3.5`). */
function ansiIndex(slot: string): number | null {
  const match = /^ansi(0|[1-9]\d*)$/.exec(slot);
  return match?.[1] === undefined ? null : Number(match[1]);
}

/**
 * The colors with one slot set. When nothing would change (the value is the one already there,
 * or the slot does not exist: an ansi index outside the palette, a name that is not a color),
 * it returns the SAME `colors` object, so the caller can tell and treat it as no edit.
 */
export function setSlot(colors: Colors, slot: ColorSlot, value: string): Colors {
  if (slot.startsWith("ansi")) {
    const index = ansiIndex(slot);
    if (index === null || index >= colors.ansi.length || colors.ansi[index] === value) return colors;
    const ansi = [...colors.ansi];
    ansi[index] = value;
    return { ...colors, ansi };
  }
  const key = slot as Exclude<keyof Colors, "ansi">;
  if (typeof colors[key] !== "string" || colors[key] === value) return colors;
  return { ...colors, [key]: value };
}

export function getSlot(colors: Colors, slot: ColorSlot): string {
  if (slot.startsWith("ansi")) {
    const index = ansiIndex(slot);
    return (index === null ? undefined : colors.ansi[index]) ?? "#000000";
  }
  return colors[slot as Exclude<keyof Colors, "ansi">];
}

/** One action applied, without the derived `dirty` (see `reduce`). */
function step(state: EditorState, action: EditorAction): EditorState {
  switch (action.type) {
    case "load": {
      const fresh = initialState(action.theme, action.savedTheme);
      return action.mode ? { ...fresh, mode: action.mode } : fresh;
    }
    case "edit":
      return push(state, action.theme);
    case "setColor": {
      // An edit that changes nothing is no edit: no undo step, and the recipe is not wiped.
      // The guard is here, not in the callers, so no input can null `simple` by accident.
      const colors = setSlot(state.theme.colors, action.slot, action.value);
      if (colors === state.theme.colors) return state;
      return push(state, { ...state.theme, colors, simple: null });
    }
    case "setColors": {
      // Nothing new (the same colors object, recipe and tone) is no change: no undo step.
      if (
        action.colors === state.theme.colors &&
        action.tone === state.theme.tone &&
        sameRecipe(action.simple, state.theme.simple)
      ) {
        return state;
      }
      const theme: Theme = { ...state.theme, colors: action.colors, simple: action.simple, tone: action.tone };
      // A regenerate from the recipe replaces the hand edits: the notice is over.
      return { ...push(state, theme), keptEditsNotice: action.simple ? false : state.keptEditsNotice };
    }
    case "setMode":
      if (action.mode === state.mode) return state;
      return { ...state, mode: action.mode, keptEditsNotice: keptEdits(action.mode, state.theme) };
    // Undo and redo put back a theme that may or may not have its recipe, so the notice is
    // worked out again for the theme now on screen, by the rule `setMode` uses.
    case "undo": {
      const previous = state.past[state.past.length - 1];
      if (!previous) return state;
      return {
        ...state,
        theme: previous,
        past: state.past.slice(0, -1),
        future: [state.theme, ...state.future],
        keptEditsNotice: keptEdits(state.mode, previous),
      };
    }
    case "redo": {
      const [next, ...rest] = state.future;
      if (!next) return state;
      return {
        ...state,
        theme: next,
        past: [...state.past, state.theme],
        future: rest,
        keptEditsNotice: keptEdits(state.mode, next),
      };
    }
    case "saved":
      return { ...state, savedTheme: action.theme };
  }
}

export function reduce(state: EditorState, action: EditorAction): EditorState {
  const next = step(state, action);
  // `dirty` is derived in this one place. A no-op action returns the same state object.
  const dirty = next.theme !== next.savedTheme;
  return dirty === next.dirty ? next : { ...next, dirty };
}
