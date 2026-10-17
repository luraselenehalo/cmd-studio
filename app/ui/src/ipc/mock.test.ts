import { describe, expect, it } from "vitest";
import type { PreviewContext } from "../bindings/PreviewContext";
import type { Theme } from "../bindings/Theme";
import { createMockIpc } from "./mock";
import { PRESETS } from "./presets";

/** A preset by name, as its file in `presets/` has it. */
function preset(name: string): Theme {
  const found = PRESETS.find((p) => p.name === name);
  if (!found) throw new Error(`no preset ${name}`);
  return found;
}

/** A sample for the preview, made here: the production one is the app's (`sample.ts`). */
const SAMPLE: PreviewContext = {
  user: "tester",
  host: "TESTPC",
  home: "C:\\Users\\tester",
  cwd: "C:\\Users\\tester\\projects\\cmd-studio",
  clock: { year: 2026, month: 10, day: 9, hour: 16, minute: 47 },
  git: { branch: "main", dirty: true },
  exitCode: 1,
  durationMs: 3500,
  width: 80,
  windowsTerminal: true,
  command: "npm test",
};

describe("the mock IPC", () => {
  it("lists the twelve presets and applies, undoes and restores like the backend", async () => {
    const ipc = createMockIpc();
    const lib = await ipc.themesList();
    expect(lib.themes).toHaveLength(12);
    expect(lib.themes[0]?.name).toBe("Terracotta");
    const request = { themeId: lib.themes[0]!.id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    const plan = await ipc.planApply(request);
    expect(plan.kind).toBe("ready");
    if (plan.kind !== "ready") throw new Error("not ready");
    const progress: string[] = [];
    const done = await ipc.apply(request, plan.plan.digest, (p) => progress.push(p.phase));
    expect(done.status).toBe("committed");
    expect(progress[0]).toBe("backingUp");
    expect(progress[progress.length - 1]).toBe("verifying");
    expect((await ipc.themesList()).themes[0]?.inUse).toBe(true);
    const stale = await ipc.apply(request, "stale", () => {});
    expect(stale.status).toBe("planChanged");
    const points = await ipc.snapshotsList();
    expect(points.points).toHaveLength(1);
    expect(points.points[0]?.restorable).toBe(true);
    const restored = await ipc.snapshotRestore(points.points[0]!.id);
    expect(restored.status).toBe("done");
    expect((await ipc.undoLast()).status).toBe("nothingToUndo");
  });

  it("refuses to save or delete a preset and keeps drafts apart", async () => {
    const ipc = createMockIpc();
    const [preset] = (await ipc.themesList()).themes;
    const detail = await ipc.themeGet(preset!.id);
    await expect(ipc.themeSave(detail.theme, false)).rejects.toMatchObject({ code: "E_THEME_INVALID" });
    await expect(ipc.themeDelete(preset!.id, false)).rejects.toMatchObject({ code: "E_THEME_INVALID" });
    const copy = await ipc.themeDuplicate(preset!.id, "Mine");
    await ipc.themeSave({ ...detail.theme, id: copy.id, name: "Mine, drafted" }, true);
    expect((await ipc.themeGet(copy.id)).draft?.name).toBe("Mine, drafted");
    expect((await ipc.themesList()).themes.find((t) => t.id === copy.id)?.hasDraft).toBe(true);
    await ipc.themeDelete(copy.id, true);
    expect((await ipc.themeGet(copy.id)).draft).toBeNull();
    // As library.rs: the theme is found before anything is removed. A refused delete of a
    // preset leaves a draft under its id; a theme that is only a draft is deleted, not "gone".
    ipc.state.drafts.push({ ...detail.theme });
    await expect(ipc.themeDelete(preset!.id, false)).rejects.toMatchObject({ code: "E_THEME_INVALID" });
    expect(ipc.state.drafts.map((d) => d.id)).toEqual([preset!.id]);
    ipc.state.drafts = [];
    const fresh = { ...detail.theme, id: "0b5a2f1e-6c3d-4e7f-8a9b-0c1d2e3f4a5b", name: "Brand new" };
    await ipc.themeSave(fresh, true);
    await ipc.themeDelete(fresh.id, false);
    expect((await ipc.themesList()).themes.some((t) => t.id === fresh.id)).toBe(false);
    await expect(ipc.themeDelete(fresh.id, false)).rejects.toMatchObject({ code: "E_THEME_INVALID" });
  });

  it("stores and lists themes as the backend does: trimmed names, by lowercase name, ties by file", async () => {
    const ipc = createMockIpc();
    const terracotta = (await ipc.themesList()).themes[0]!.id;
    // The name is trimmed when the theme is written, in the answer and in what is kept.
    const padded = await ipc.themeDuplicate(terracotta, "  Padded  ");
    expect(padded.name).toBe("Padded");
    expect(ipc.state.mine.map((t) => t.name)).toEqual(["Padded"]);
    const drafted = await ipc.themeSave({ ...(await ipc.themeGet(padded.id)).theme, name: " Padded, drafted " }, true);
    expect(drafted.name).toBe("Padded, drafted");
    // Mine by the lowercase name in code-unit order (Rust's `to_lowercase` key, not the
    // locale's), and two of one name in the order of their files: the second copy's file,
    // "My Terracotta (2).cmdtheme", comes before "My Terracotta.cmdtheme".
    const first = await ipc.themeDuplicate(terracotta, "My Terracotta");
    const second = await ipc.themeDuplicate(terracotta, "My Terracotta");
    await ipc.themeDuplicate(terracotta, "Éclair");
    await ipc.themeDuplicate(terracotta, "fudge");
    const mine = (await ipc.themesList()).themes.filter((t) => t.source === "mine");
    expect(mine.map((t) => t.name)).toEqual(["fudge", "My Terracotta", "My Terracotta", "Padded", "Éclair"]);
    expect(mine.filter((t) => t.name === "My Terracotta").map((t) => t.id)).toEqual([second.id, first.id]);
    // Drafts of themes never saved: the same key, and ties in the order of their files
    // (`<id>.cmdtheme`).
    const base = (await ipc.themeGet(terracotta)).theme;
    await ipc.themeSave({ ...base, id: "1aaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", name: "Same" }, true);
    await ipc.themeSave({ ...base, id: "0bbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", name: "Same" }, true);
    await ipc.themeSave({ ...base, id: "2ccccccc-cccc-4ccc-8ccc-cccccccccccc", name: "alpha" }, true);
    const drafts = (await ipc.themesList()).themes.filter((t) => t.source === "draft");
    expect(drafts.map((t) => t.id)).toEqual([
      "2ccccccc-cccc-4ccc-8ccc-cccccccccccc",
      "0bbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
      "1aaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    ]);
    // A deleted theme's file name is free again.
    await ipc.themeDelete(first.id, false);
    const third = await ipc.themeDuplicate(terracotta, "My Terracotta");
    const again = (await ipc.themesList()).themes.filter((t) => t.name === "My Terracotta").map((t) => t.id);
    expect(again).toEqual([second.id, third.id]);
  });

  it("answers the colors, readability and preview from fixed data, computing nothing", async () => {
    const ipc = createMockIpc();
    // Each card's colors come with the listing: the preset files' own.
    expect((await ipc.themesList()).themes.map((t) => t.colors)).toEqual(PRESETS.map((p) => p.colors));
    // Simple mode: a palette of CMD Studio's own for the tone, with the accent (and the
    // cursor) put back as given; the tint changes nothing here.
    expect(await ipc.paletteGenerate("light", "#123456", 0.3)).toEqual({
      ...preset("Daylight").colors,
      accent: "#123456",
      cursor: "#123456",
    });
    expect(await ipc.paletteGenerate("dark", "#123456", 0)).toEqual({
      ...preset("Terracotta").colors,
      accent: "#123456",
      cursor: "#123456",
    });
    // Readability: the core's known exceptions (crates/core/src/presets.rs) for the three
    // upstream presets that have them, in the core's order; nothing for any other colors.
    const slots = async (name: string) => (await ipc.contrastCheck(preset(name).colors)).map((i) => i.slot);
    expect(await slots("Gruvbox")).toEqual(["ansi1"]);
    expect(await slots("Catppuccin Latte")).toEqual(["ansi2", "ansi3", "ansi5", "ansi10", "ansi11", "ansi13"]);
    expect(await slots("Solarized Light")).toEqual(["foreground", "muted", "ansi2", "ansi3", "ansi6", "ansi12", "ansi14"]);
    expect(await slots("Terracotta")).toEqual([]);
    // "Fix automatically": the core's answer for those three, the colors as given otherwise.
    const fixed = await ipc.contrastFix(preset("Solarized Light").colors);
    expect([fixed.foreground, fixed.muted]).toEqual(["#5e747c", "#829090"]);
    expect(await ipc.contrastCheck(fixed)).toEqual([]);
    expect(await ipc.contrastFix(preset("Terracotta").colors)).toEqual(preset("Terracotta").colors);
    // The preview's welcome box is a fixed row drawn for the sample user tester (fixed data, not
    // a rendering): no variable is left in it, and its right border lines up with the frame.
    const preview = await ipc.previewRender(preset("Terracotta"), SAMPLE);
    const rows = preview.banner.map((line) => line.map((s) => s.text).join(""));
    expect(rows.join("\n")).toContain("Welcome back, tester!");
    expect(rows.join("\n")).not.toMatch(/\{\w+\}/);
    const box = rows.slice(0, 3);
    expect(box.map((row) => [Array.from(row)[0], Array.from(row).at(-1)])).toEqual([
      ["╭", "╮"],
      ["│", "│"],
      ["╰", "╯"],
    ]);
    expect(new Set(box.map((row) => Array.from(row).length)).size).toBe(1);
  });

  it("answers in the shipped shapes: the sample's identity and what a plan turns off", async () => {
    const other = ['"C:\\clink\\clink_x64.exe" inject --autorun'];
    const ipc = createMockIpc({ otherClink: other });
    expect((await ipc.systemDetect()).previewSample).toEqual({
      user: "tester",
      host: "TESTPC",
      home: "C:\\Users\\tester",
    });
    const themeId = (await ipc.themesList()).themes[0]!.id;
    const request = { themeId, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    expect(await ipc.planApply(request)).toEqual({ kind: "otherClink", commands: other });
    const agreed = { ...request, replaceOtherClink: true };
    const plan = await ipc.planApply(agreed);
    if (plan.kind !== "ready") throw new Error("not ready");
    expect(plan.plan.replacesOtherClink).toEqual(other);
    expect((await ipc.apply(agreed, plan.plan.digest, () => {})).status).toBe("committed");
    const point = (await ipc.snapshotsList()).points[0]!;
    expect(await ipc.snapshotRestore(point.id)).toMatchObject({ status: "done", notes: [] });
    const reverted = await ipc.revertAll();
    expect(reverted.steps.map((s) => s.error)).toEqual([null, null]);
  });

  it("reports the build, the Store and Windows Terminal as it is told, working nothing out (Q3)", async () => {
    let report = await createMockIpc().systemDetect();
    expect([report.build, report.storeAvailable, report.wtInstalled]).toEqual([26100, true, true]);
    expect(report.windowsTerminal).toEqual([{ kind: "store", installed: true, hasSettings: true, setUpByApply: false }]);
    // Given values come back as given, even a pair the backend would never send: the mock
    // derives `storeAvailable` from nothing.
    report = await createMockIpc({ build: 18363, storeAvailable: true }).systemDetect();
    expect([report.build, report.storeAvailable]).toEqual([18363, true]);
    report = await createMockIpc({ build: null, storeAvailable: false, noWindowsTerminal: true }).systemDetect();
    expect([report.build, report.storeAvailable, report.wtInstalled]).toEqual([null, false, false]);
    expect(report.windowsTerminal).toEqual([{ kind: "store", installed: false, hasSettings: false, setUpByApply: false }]);
    report = await createMockIpc({ wtNeverOpened: true }).systemDetect();
    expect(report.wtInstalled).toBe(true);
    expect(report.windowsTerminal).toEqual([{ kind: "store", installed: true, hasSettings: false, setUpByApply: true }]);
  });

  it("plans a Terminal that was never opened as set up by the apply, and D6 says so until one is applied", async () => {
    const ipc = createMockIpc({ wtNeverOpened: true });
    const request = { themeId: preset("Terracotta").id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    const planned = await ipc.planApply(request);
    if (planned.kind !== "ready") throw new Error("not ready");
    // The core's own lines (crates/core/src/plan.rs): the set-up first, then the colors.
    const en = planned.plan.summaryEn;
    expect(en.indexOf("Set up Windows Terminal's settings so cmd gets this theme")).toBeGreaterThanOrEqual(0);
    expect(en.indexOf("Set up Windows Terminal's settings so cmd gets this theme")).toBeLessThan(en.indexOf("Colors and font of cmd in Windows Terminal"));
    expect(planned.plan.summaryTh).toContain("ตั้งค่า Windows Terminal ให้ cmd ได้ธีมนี้");
    expect(planned.plan.warnings).toEqual([]);
    expect(planned.plan.steps.map((s) => s.kind)).toContain("windowsTerminal");
    const d6 = async () => (await ipc.doctorRun()).checks.find((c) => c.id === "D6")!;
    // The core's D6 for it (crates/core/src/doctor.rs); nothing to offer before an apply.
    expect(await d6()).toMatchObject({
      status: "warn",
      code: "E_WT_NOT_FOUND",
      messageEn: "Windows Terminal has never been opened, so it has no settings yet. Applying a theme sets them up",
      fix: null,
    });
    expect((await ipc.apply(request, planned.plan.digest, () => {})).status).toBe("committed");
    expect((await d6()).status).toBe("ok");
  });

  it("offers the Store for a missing Windows Terminal only where the Store applies", async () => {
    const d6 = async (options: Parameters<typeof createMockIpc>[0]) =>
      (await createMockIpc(options).doctorRun()).checks.find((c) => c.id === "D6")!;
    expect((await d6({ noWindowsTerminal: true })).fix).toBe("installWindowsTerminal");
    expect((await d6({ noWindowsTerminal: true, build: 18363, storeAvailable: false })).fix).toBeNull();
    expect((await d6({ noWindowsTerminal: true, build: null, storeAvailable: false })).fix).toBeNull();
  });
});
