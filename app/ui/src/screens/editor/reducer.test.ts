import { describe, expect, it } from "vitest";
import { PRESETS } from "../../ipc/presets";
import { HISTORY, getSlot, initialState, reduce, type EditorState } from "./reducer";

const terracotta = () => structuredClone(PRESETS.find((p) => p.name === "Terracotta")!);

describe("the editor's history", () => {
  it("undoes and redoes, and keeps at most 100 steps", () => {
    let s = initialState(terracotta());
    expect(s.mode).toBe("advanced"); // Terracotta's simple is null (hand-tuned)
    for (let i = 0; i < HISTORY + 20; i++) {
      s = reduce(s, { type: "edit", theme: { ...s.theme, name: `Name ${i}` } });
    }
    expect(s.past.length).toBe(HISTORY);
    expect(s.theme.name).toBe(`Name ${HISTORY + 19}`);
    for (let i = 0; i < HISTORY; i++) s = reduce(s, { type: "undo" });
    expect(s.theme.name).toBe("Name 19");
    expect(s.past.length).toBe(0);
    // One more undo does nothing.
    expect(reduce(s, { type: "undo" })).toBe(s);
    s = reduce(s, { type: "redo" });
    expect(s.theme.name).toBe("Name 20");
    // A new edit clears the redo stack.
    s = reduce(s, { type: "edit", theme: { ...s.theme, name: "fresh" } });
    expect(s.future).toEqual([]);
    expect(reduce(s, { type: "redo" })).toBe(s);
    expect(s.dirty).toBe(true);
    expect(reduce(s, { type: "saved", theme: s.theme }).dirty).toBe(false);
  });
});

describe("dirty is derived from the saved theme, by identity", () => {
  it("is clean again after an edit is undone back to the saved theme", () => {
    let s = initialState(terracotta());
    expect(s.dirty).toBe(false);
    s = reduce(s, { type: "edit", theme: { ...s.theme, name: "Changed" } });
    expect(s.dirty).toBe(true);
    s = reduce(s, { type: "undo" });
    expect(s.dirty).toBe(false);
    s = reduce(s, { type: "redo" });
    expect(s.dirty).toBe(true);
  });

  it("is dirty after undoing past the saved theme, and clean again on redo", () => {
    let s = initialState(terracotta());
    s = reduce(s, { type: "edit", theme: { ...s.theme, name: "Saved name" } });
    s = reduce(s, { type: "saved", theme: s.theme });
    expect(s.dirty).toBe(false);
    s = reduce(s, { type: "undo" }); // back before the save
    expect(s.dirty).toBe(true);
    s = reduce(s, { type: "redo" });
    expect(s.dirty).toBe(false);
  });

  it("stays dirty when an edit was made while the save was awaited", () => {
    let s = initialState(terracotta());
    s = reduce(s, { type: "edit", theme: { ...s.theme, name: "A" } });
    const saving = s.theme; // the save starts with this theme
    s = reduce(s, { type: "edit", theme: { ...s.theme, name: "B" } }); // an edit during the await
    s = reduce(s, { type: "saved", theme: saving });
    expect(s.savedTheme).toBe(saving);
    expect(s.dirty).toBe(true);
    // Undoing the edit made during the save brings back exactly what was saved.
    s = reduce(s, { type: "undo" });
    expect(s.dirty).toBe(false);
  });

  it("starts dirty for a loaded draft and clean for a fresh load", () => {
    const stored = terracotta();
    const draft = { ...stored, name: "A draft" };
    let s = reduce(initialState(stored), { type: "load", theme: draft, savedTheme: stored });
    expect(s.theme).toBe(draft);
    expect(s.savedTheme).toBe(stored);
    expect(s.dirty).toBe(true);
    expect(s.past).toEqual([]);
    s = reduce(s, { type: "saved", theme: draft });
    expect(s.dirty).toBe(false);
    // A fresh load: the theme and the saved theme are the same.
    s = reduce(s, { type: "load", theme: stored });
    expect(s.savedTheme).toBe(stored);
    expect(s.dirty).toBe(false);
    // The mode can be chosen; the saved theme is still the one given.
    s = reduce(s, { type: "load", theme: draft, savedTheme: stored, mode: "simple" });
    expect(s.mode).toBe("simple");
    expect(s.dirty).toBe(true);
    expect(initialState(draft, stored).dirty).toBe(true);
    expect(initialState(draft).dirty).toBe(false);
  });
});

describe("simple and advanced mode (spec §3.4)", () => {
  const simpleTheme = () => ({ ...terracotta(), simple: { accent: "#d97757", tint: 0.1 } });

  it("opens in simple mode when the theme has a recipe", () => {
    expect(initialState(simpleTheme()).mode).toBe("simple");
  });

  it("makes simple null as soon as one color is edited by hand", () => {
    let s: EditorState = initialState(simpleTheme());
    s = reduce(s, { type: "setMode", mode: "advanced" });
    s = reduce(s, { type: "setColor", slot: "ansi3", value: "#ffff00" });
    expect(s.theme.simple).toBeNull();
    expect(s.theme.colors.ansi[3]).toBe("#ffff00");
    s = reduce(s, { type: "setColor", slot: "frame", value: "#123456" });
    expect(s.theme.colors.frame).toBe("#123456");
  });

  it("keeps the hand edits when going back to simple mode, with a notice, until a regenerate", () => {
    let s: EditorState = initialState(simpleTheme());
    s = reduce(s, { type: "setMode", mode: "advanced" });
    s = reduce(s, { type: "setColor", slot: "muted", value: "#777777" });
    s = reduce(s, { type: "setMode", mode: "simple" });
    expect(s.keptEditsNotice).toBe(true);
    expect(s.theme.colors.muted).toBe("#777777");
    // Tone, accent or tint changed: the Rust palette comes back and replaces the edits.
    const regenerated = { ...s.theme.colors, muted: "#8a8780" };
    s = reduce(s, {
      type: "setColors",
      colors: regenerated,
      simple: { accent: "#00ff00", tint: 0.2 },
      tone: s.theme.tone,
    });
    expect(s.keptEditsNotice).toBe(false);
    expect(s.theme.simple).toEqual({ accent: "#00ff00", tint: 0.2 });
    expect(s.theme.colors.muted).toBe("#8a8780");
    // A readability fix is a hand edit: simple goes null again, the tone stays, and the notice
    // is not shown until the mode is switched again.
    s = reduce(s, { type: "setColors", colors: regenerated, simple: null, tone: s.theme.tone });
    expect(s.theme.simple).toBeNull();
    expect(s.theme.tone).toBe("dark");
    expect(s.keptEditsNotice).toBe(false);
  });

  it("does not show the notice when the theme still has its recipe", () => {
    let s: EditorState = initialState(simpleTheme());
    s = reduce(s, { type: "setMode", mode: "advanced" });
    s = reduce(s, { type: "setMode", mode: "simple" });
    expect(s.keptEditsNotice).toBe(false);
  });
});

describe("a color edit that changes nothing is no edit", () => {
  // The recipe must never be wiped by accident, whichever input dispatches (the guard lives in
  // the reducer, not in the callers).
  const advancedWithRecipe = () => {
    const s = initialState({ ...terracotta(), simple: { accent: "#d97757", tint: 0.1 } });
    return reduce(s, { type: "setMode", mode: "advanced" });
  };

  it("ignores a value equal to the current one: no undo step, simple stays", () => {
    const s = advancedWithRecipe();
    const sameNamed = reduce(s, { type: "setColor", slot: "frame", value: s.theme.colors.frame });
    expect(sameNamed).toBe(s);
    const sameAnsi = reduce(s, { type: "setColor", slot: "ansi3", value: s.theme.colors.ansi[3]! });
    expect(sameAnsi).toBe(s);
    expect(sameAnsi.theme.simple).toEqual({ accent: "#d97757", tint: 0.1 });
    expect(sameAnsi.past).toEqual([]);
    expect(sameAnsi.dirty).toBe(false);
  });

  it("ignores an ansi index outside 0 to 15: no undo step, simple stays", () => {
    const s = advancedWithRecipe();
    for (const slot of ["ansi16", "ansi99", "ansi-1", "ansi3.5"] as const) {
      const next = reduce(s, { type: "setColor", slot, value: "#abcdef" });
      expect(next, slot).toBe(s);
    }
    expect(s.theme.simple).toEqual({ accent: "#d97757", tint: 0.1 });
    expect(s.theme.colors.ansi.length).toBe(16);
    expect(s.past).toEqual([]);
  });

  it("still takes a real change, and the two ends of the ansi range", () => {
    let s = advancedWithRecipe();
    s = reduce(s, { type: "setColor", slot: "ansi0", value: "#010101" });
    s = reduce(s, { type: "setColor", slot: "ansi15", value: "#fefefe" });
    expect(s.theme.colors.ansi[0]).toBe("#010101");
    expect(s.theme.colors.ansi[15]).toBe("#fefefe");
    expect(s.theme.simple).toBeNull();
    expect(s.past.length).toBe(2);
  });
});

describe("the kept-edits notice follows the theme on screen through undo and redo (spec §3.4)", () => {
  const simpleTheme = () => ({ ...terracotta(), simple: { accent: "#d97757", tint: 0.1 } });
  // Simple mode, hand edit in advanced mode, back to simple mode: the notice is shown.
  const withNoticeShown = (): EditorState => {
    let s = initialState(simpleTheme());
    s = reduce(s, { type: "setMode", mode: "advanced" });
    s = reduce(s, { type: "setColor", slot: "muted", value: "#777777" });
    s = reduce(s, { type: "setMode", mode: "simple" });
    expect(s.keptEditsNotice).toBe(true);
    return s;
  };

  it("A: undoing back to a recipe ends the notice, and redo brings it back", () => {
    let s = withNoticeShown();
    s = reduce(s, { type: "undo" }); // the theme with its recipe again
    expect(s.theme.simple).not.toBeNull();
    expect(s.keptEditsNotice).toBe(false);
    s = reduce(s, { type: "redo" }); // the hand edit again, shown in simple mode
    expect(s.theme.simple).toBeNull();
    expect(s.keptEditsNotice).toBe(true);
  });

  it("B: undoing a regenerate back to the hand edits shows the notice", () => {
    let s = withNoticeShown();
    const regenerated = { ...s.theme.colors, muted: "#8a8780" };
    s = reduce(s, {
      type: "setColors",
      colors: regenerated,
      simple: { accent: "#00ff00", tint: 0.2 },
      tone: s.theme.tone,
    });
    expect(s.keptEditsNotice).toBe(false);
    s = reduce(s, { type: "undo" }); // the kept hand edits are on screen again
    expect(s.theme.simple).toBeNull();
    expect(s.theme.colors.muted).toBe("#777777");
    expect(s.keptEditsNotice).toBe(true);
    s = reduce(s, { type: "redo" }); // regenerated again
    expect(s.theme.simple).not.toBeNull();
    expect(s.keptEditsNotice).toBe(false);
  });

  it("shows no notice in advanced mode, even when the theme put back has no recipe", () => {
    let s = initialState(simpleTheme());
    s = reduce(s, { type: "setMode", mode: "advanced" });
    s = reduce(s, { type: "setColor", slot: "muted", value: "#777777" });
    s = reduce(s, { type: "setColor", slot: "frame", value: "#123456" });
    s = reduce(s, { type: "undo" }); // a theme with no recipe, in advanced mode
    expect(s.theme.simple).toBeNull();
    expect(s.mode).toBe("advanced");
    expect(s.keptEditsNotice).toBe(false);
    s = reduce(s, { type: "redo" });
    expect(s.theme.simple).toBeNull();
    expect(s.keptEditsNotice).toBe(false);
  });
});

describe("a color edit does not touch the history's themes", () => {
  it("brings the original ansi color back on undo (the palette array is copied, not shared)", () => {
    const start = initialState(terracotta());
    const snapshot = structuredClone(start.theme);
    const original = start.theme.colors.ansi[3]!;
    let s = reduce(start, { type: "setColor", slot: "ansi3", value: "#123456" });
    expect(s.theme.colors.ansi[3]).toBe("#123456");
    expect(s.theme.colors.ansi).not.toBe(start.theme.colors.ansi);
    s = reduce(s, { type: "undo" });
    expect(s.theme.colors.ansi[3]).toBe(original);
    expect(s.theme).toEqual(snapshot);
  });
});

describe("getSlot", () => {
  const colors = () => terracotta().colors;

  it("reads a named slot and an ansi slot", () => {
    const c = colors();
    expect(getSlot(c, "frame")).toBe(c.frame);
    expect(getSlot(c, "background")).toBe(c.background);
    expect(getSlot(c, "ansi3")).toBe(c.ansi[3]);
    expect(getSlot(c, "ansi0")).toBe(c.ansi[0]);
    expect(getSlot(c, "ansi15")).toBe(c.ansi[15]);
  });

  it("answers black for an ansi index outside the palette or not a whole number", () => {
    const c = colors();
    for (const slot of ["ansi16", "ansi99", "ansi-1", "ansi3.5"] as const) {
      expect(getSlot(c, slot), slot).toBe("#000000");
    }
  });
});

describe("setColors with nothing new is no change", () => {
  const simpleTheme = () => ({ ...terracotta(), simple: { accent: "#d97757", tint: 0.1 } });

  it("returns the same state when the colors object, the recipe and the tone are all as they are", () => {
    const s = initialState(simpleTheme());
    const same = reduce(s, {
      type: "setColors",
      colors: s.theme.colors,
      simple: { accent: "#d97757", tint: 0.1 }, // a new object with the same recipe
      tone: s.theme.tone,
    });
    expect(same).toBe(s);
    const hand = reduce(s, { type: "setColor", slot: "frame", value: "#123456" });
    expect(hand.theme.simple).toBeNull();
    const sameNull = reduce(hand, { type: "setColors", colors: hand.theme.colors, simple: null, tone: hand.theme.tone });
    expect(sameNull).toBe(hand);
  });

  it("is a change as soon as the colors, the recipe or the tone differ", () => {
    const s = initialState(simpleTheme());
    const colors = s.theme.colors;
    const recipe = s.theme.simple;
    const newColors = reduce(s, { type: "setColors", colors: { ...colors }, simple: recipe, tone: s.theme.tone });
    expect(newColors).not.toBe(s);
    expect(newColors.past.length).toBe(1);
    const newRecipe = reduce(s, { type: "setColors", colors, simple: { accent: "#00ff00", tint: 0.1 }, tone: s.theme.tone });
    expect(newRecipe.past.length).toBe(1);
    const newTint = reduce(s, { type: "setColors", colors, simple: { accent: "#d97757", tint: 0.2 }, tone: s.theme.tone });
    expect(newTint.past.length).toBe(1);
    const noRecipe = reduce(s, { type: "setColors", colors, simple: null, tone: s.theme.tone });
    expect(noRecipe.past.length).toBe(1);
    const newTone = reduce(s, { type: "setColors", colors, simple: recipe, tone: "light" });
    expect(newTone.past.length).toBe(1);
    expect(newTone.theme.tone).toBe("light");
  });
});

describe("a change of tone (ruling R9)", () => {
  it("changes the tone and the palette together, so one undo brings both back", () => {
    const before = initialState({ ...terracotta(), simple: { accent: "#d97757", tint: 0.1 } });
    expect(before.theme.tone).toBe("dark");
    // Any fixed palette will do: the reducer only stores what the backend returns, so this one
    // is a preset's colors, not something computed here (and not the mock's echo of the accent).
    const light = structuredClone(PRESETS.find((p) => p.name === "Daylight")!.colors);
    expect(light).not.toEqual(before.theme.colors);

    let s = reduce(before, {
      type: "setColors",
      colors: light,
      simple: { accent: "#0969da", tint: 0 },
      tone: "light",
    });
    // One step: the new tone and the new palette arrive together, never one without the other.
    expect(s.past.length).toBe(1);
    expect(s.theme.tone).toBe("light");
    expect(s.theme.colors).toEqual(light);
    expect(s.theme.simple).toEqual({ accent: "#0969da", tint: 0 });

    s = reduce(s, { type: "undo" });
    expect(s.theme.tone).toBe("dark");
    expect(s.theme.colors).toEqual(before.theme.colors);
    expect(s.theme.simple).toEqual({ accent: "#d97757", tint: 0.1 });
    expect(s.past.length).toBe(0);

    // And redo brings the pair forward again, still in one step.
    s = reduce(s, { type: "redo" });
    expect(s.theme.tone).toBe("light");
    expect(s.theme.colors).toEqual(light);
    expect(s.future.length).toBe(0);
  });
});
