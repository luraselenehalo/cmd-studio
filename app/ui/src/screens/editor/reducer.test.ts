import { describe, expect, it } from "vitest";
import { PRESETS } from "../../ipc/presets";
import { HISTORY, initialState, reduce, type EditorState } from "./reducer";

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
    expect(reduce(s, { type: "saved" }).dirty).toBe(false);
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
