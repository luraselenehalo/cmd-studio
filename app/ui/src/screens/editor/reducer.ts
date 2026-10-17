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
  /** Changed since it was loaded or saved. */
  dirty: boolean;
}

export type ColorSlot = Exclude<keyof Colors, "ansi"> | `ansi${number}`;

export type EditorAction =
  | { type: "load"; theme: Theme; mode?: Mode }
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
  | { type: "saved" };

export function initialState(theme: Theme): EditorState {
  return {
    theme,
    past: [],
    future: [],
    mode: theme.simple ? "simple" : "advanced",
    keptEditsNotice: false,
    dirty: false,
  };
}

function push(state: EditorState, theme: Theme): EditorState {
  const past = [...state.past, state.theme];
  if (past.length > HISTORY) past.shift();
  return { ...state, theme, past, future: [], dirty: true };
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

export function reduce(state: EditorState, action: EditorAction): EditorState {
  switch (action.type) {
    case "load":
      return { ...initialState(action.theme), mode: action.mode ?? initialState(action.theme).mode };
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
      const theme: Theme = { ...state.theme, colors: action.colors, simple: action.simple, tone: action.tone };
      // A regenerate from the recipe replaces the hand edits: the notice is over.
      return { ...push(state, theme), keptEditsNotice: action.simple ? false : state.keptEditsNotice };
    }
    case "setMode":
      if (action.mode === state.mode) return state;
      return {
        ...state,
        mode: action.mode,
        keptEditsNotice: action.mode === "simple" && state.theme.simple === null,
      };
    case "undo": {
      const previous = state.past[state.past.length - 1];
      if (!previous) return state;
      return { ...state, theme: previous, past: state.past.slice(0, -1), future: [state.theme, ...state.future], dirty: true };
    }
    case "redo": {
      const [next, ...rest] = state.future;
      if (!next) return state;
      return { ...state, theme: next, past: [...state.past, state.theme], future: rest, dirty: true };
    }
    case "saved":
      return { ...state, dirty: false };
  }
}
