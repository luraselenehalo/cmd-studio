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

export function setSlot(colors: Colors, slot: ColorSlot, value: string): Colors {
  if (slot.startsWith("ansi")) {
    const index = Number(slot.slice(4));
    const ansi = [...colors.ansi];
    if (index >= 0 && index < ansi.length) ansi[index] = value;
    return { ...colors, ansi };
  }
  return { ...colors, [slot]: value };
}

export function getSlot(colors: Colors, slot: ColorSlot): string {
  if (slot.startsWith("ansi")) return colors.ansi[Number(slot.slice(4))] ?? "#000000";
  return colors[slot as Exclude<keyof Colors, "ansi">];
}

export function reduce(state: EditorState, action: EditorAction): EditorState {
  switch (action.type) {
    case "load":
      return { ...initialState(action.theme), mode: action.mode ?? initialState(action.theme).mode };
    case "edit":
      return push(state, action.theme);
    case "setColor": {
      const theme: Theme = { ...state.theme, colors: setSlot(state.theme.colors, action.slot, action.value), simple: null };
      return push(state, theme);
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
