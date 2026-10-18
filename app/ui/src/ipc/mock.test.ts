import { describe, expect, it } from "vitest";
import type { PauseInfo } from "../bindings/PauseInfo";
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

  it("words its Thai as the Rust side does: พร้อมท์ for the prompt, ส่วนของ CMD Studio, never เรา (V4)", async () => {
    const ipc = createMockIpc();
    const themeId = (await ipc.themesList()).themes[0]!.id;
    const request = { themeId, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    const planned = await ipc.planApply(request);
    if (planned.kind !== "ready") throw new Error("not ready");
    expect(planned.plan.summaryTh[0]).toBe("ปรับพร้อมท์และกล่องต้อนรับตามธีม");
    expect((await ipc.apply(request, planned.plan.digest, () => {})).status).toBe("committed");
    const d4 = (await ipc.doctorRun()).checks.find((c) => c.id === "D4")!;
    expect(d4.messageTh).toBe("AutoRun มีส่วนของ CMD Studio อยู่หน้าสุด");
    // None of the Thai it gives says the English word, "we" or a backup (the guard over the app's
    // error table, error.rs, is the same).
    for (const text of [...planned.plan.summaryTh, d4.messageTh]) {
      for (const word of ["prompt", "เรา", "สำรอง", "รุ่น"]) expect(text, word).not.toContain(word);
    }
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

  it("pauses and turns the theme back on as the backend does: in cmd at once, on this computer through the plan", async () => {
    const ipc = createMockIpc();
    const terracotta = (await ipc.themesList()).themes[0]!;
    const request = { themeId: terracotta.id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    const planned = await ipc.planApply(request);
    if (planned.kind !== "ready") throw new Error("not ready");
    await ipc.apply(request, planned.plan.digest, () => {});
    expect(await ipc.recoveryStatus()).toEqual({ paused: null, crashes: { kind: "read", count: 0, pausedNow: false, pause: null } });
    const paused = await ipc.themePause("user");
    expect(paused).toMatchObject({ pausedNow: true, refused: null, notSaved: null });
    expect(paused.revert?.status).toBe("committed");
    expect(ipc.state.applied).toBeNull();
    expect((await ipc.recoveryStatus()).paused).toMatchObject({
      by: "app",
      level: "computer",
      reason: "user",
      count: 0,
      resume: { themeId: terracotta.id, target: "all", themeName: "Terracotta" },
    });
    const resumed = await ipc.themeResume();
    if (resumed.kind !== "apply" || resumed.plan.kind !== "ready") throw new Error(resumed.kind);
    expect(resumed.request).toEqual(request);
    // The pause stays until that apply commits.
    expect((await ipc.recoveryStatus()).paused).not.toBeNull();
    expect((await ipc.apply(resumed.request, resumed.plan.plan.digest, () => {})).status).toBe("committed");
    expect((await ipc.recoveryStatus()).paused).toBeNull();
    // A pause in cmd is lifted at once, and Undo everything takes it too.
    const inCmd = { by: "clink", level: "cmd", reason: "cmdStarts", atMs: 1, count: 3, resume: null } as const;
    ipc.state.paused = { ...inCmd };
    expect(await ipc.themeResume()).toEqual({ kind: "resumed" });
    expect(await ipc.themeResume()).toEqual({ kind: "notPaused" });
    ipc.state.paused = { ...inCmd };
    await ipc.revertAll();
    expect(ipc.state.paused).toBeNull();
    // A pause of this computer stays through Undo everything: it holds the theme to turn back on.
    ipc.state.paused = { by: "app", level: "computer", reason: "user", atMs: 1, count: 0, resume: { themeId: terracotta.id, target: "all", themeName: "Terracotta" } };
    await ipc.revertAll();
    expect(ipc.state.paused).not.toBeNull();
    // Only the user and cmd's crashes are the app's reasons.
    await expect(ipc.themePause("cmdStarts")).rejects.toMatchObject({ code: "E_INTERNAL" });
    await expect(ipc.themePause("recovery")).rejects.toMatchObject({ code: "E_INTERNAL" });
    expect(ipc.state.calls.filter((c) => c.startsWith("themePause") || c === "themeResume" || c === "recoveryStatus")).toEqual([
      "recoveryStatus",
      "themePause:user",
      "recoveryStatus",
      "themeResume",
      "recoveryStatus",
      "recoveryStatus",
      "themeResume",
      "themeResume",
      "themePause:cmdStarts",
      "themePause:recovery",
    ]);
  });

  it("says D12 as the core does: paused first, then the crashes it is given", async () => {
    const d12 = async (ipc: ReturnType<typeof createMockIpc>) => (await ipc.doctorRun()).checks.find((c) => c.id === "D12")!;
    const theme = PRESETS[0]!;
    const applied = async (options: Parameters<typeof createMockIpc>[0]) => {
      const ipc = createMockIpc(options);
      const request = { themeId: theme.id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
      const planned = await ipc.planApply(request);
      if (planned.kind !== "ready") throw new Error("not ready");
      await ipc.apply(request, planned.plan.digest, () => {});
      return ipc;
    };
    expect((await d12(createMockIpc())).status).toBe("skipped");
    expect(await d12(await applied({}))).toMatchObject({ status: "ok", code: null, fix: null });
    expect(await d12(await applied({ cmdCrashes: 2 }))).toMatchObject({ status: "warn", code: "E_CMD_CRASHED", fix: "pause" });
    expect(await d12(await applied({ cmdCrashes: 3 }))).toMatchObject({ status: "fail", code: "E_CMD_CRASHED", fix: "pause" });
    // Crash reports that can't be read are "not checked", never "no crash".
    expect(await d12(await applied({ crashCheck: { kind: "unreadable", why: "access is denied", pause: null } }))).toMatchObject({
      status: "skipped",
      code: null,
      fix: null,
    });
    const paused = createMockIpc({ paused: { by: "clink", level: "cmd", reason: "cmdStarts", atMs: 1, count: 3, resume: null } });
    expect(await d12(paused)).toMatchObject({ status: "warn", code: "E_THEME_PAUSED", fix: "resume" });
    for (const word of ["prompt", "เรา", "สำรอง", "รุ่น"]) expect((await d12(paused)).messageTh).not.toContain(word);
    // A pause comes before the crashes, and its fix is the way back on. (An apply lifts a pause, so
    // it is set after the apply.)
    const both = await applied({ cmdCrashes: 3 });
    both.state.paused = { by: "clink", level: "cmd", reason: "cmdStarts", atMs: 1, count: 3, resume: null };
    expect(await d12(both)).toMatchObject({ code: "E_THEME_PAUSED", fix: "resume" });
    expect((await both.doctorFix("D12")).kind).toBe("resume");
    expect((await (await applied({ cmdCrashes: 3 })).doctorFix("D12")).kind).toBe("pause");
  });

  it("runs the start's crash check once, at the first status, and pauses for three crashes as the backend does", async () => {
    const ipc = createMockIpc({ cmdCrashes: 3 });
    const theme = PRESETS[0]!;
    const request = { themeId: theme.id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    const planned = await ipc.planApply(request);
    if (planned.kind !== "ready") throw new Error("not ready");
    await ipc.apply(request, planned.plan.digest, () => {});
    const first = await ipc.recoveryStatus();
    expect(first.crashes).toMatchObject({ kind: "read", count: 3, pausedNow: true, pause: { pausedNow: true, refused: null, notSaved: null } });
    expect(first.paused).toMatchObject({ by: "app", level: "computer", reason: "clinkCrash", count: 3, resume: { themeId: theme.id, target: "all" } });
    expect(ipc.state.applied).toBeNull();
    // Once per start: the same answer, and nothing paused again.
    const pausedAt = first.paused?.atMs;
    const again = await ipc.recoveryStatus();
    expect(again.crashes).toEqual(first.crashes);
    expect(again.paused?.atMs).toBe(pausedAt);
    // Fewer than three crashes pause nothing; with no theme applied there is nothing to look for.
    const two = createMockIpc({ cmdCrashes: 2 });
    expect((await two.recoveryStatus()).crashes).toEqual({ kind: "noTheme" });
    const planned2 = await two.planApply(request);
    if (planned2.kind !== "ready") throw new Error("not ready");
    await two.apply(request, planned2.plan.digest, () => {});
    // The check ran at the first status, when no theme was applied: it is not run again.
    expect(await two.recoveryStatus()).toEqual({ paused: null, crashes: { kind: "noTheme" } });
    await two.themePause("clinkCrash");
    expect((await two.recoveryStatus()).paused).toMatchObject({ reason: "clinkCrash", count: 2 });
    const one = createMockIpc({ cmdCrashes: 2 });
    const planned1 = await one.planApply(request);
    if (planned1.kind !== "ready") throw new Error("not ready");
    await one.apply(request, planned1.plan.digest, () => {});
    expect(await one.recoveryStatus()).toEqual({ paused: null, crashes: { kind: "read", count: 2, pausedNow: false, pause: null } });
    // A status that fails, with the code it is told.
    const failing = createMockIpc({ failRecoveryWith: "E_MACHINE_READ" });
    await expect(failing.recoveryStatus()).rejects.toMatchObject({ code: "E_MACHINE_READ" });
    expect(failing.state.calls).toEqual(["recoveryStatus"]);
  });

  it("gives each shape of the pause as fixed data from its option, and words it as the core does", async () => {
    const terracotta = PRESETS[0]!;
    const resume = { themeId: terracotta.id, target: "all" as const, themeName: "Terracotta" };
    const crash = { by: "app", reason: "clinkCrash", atMs: 1791694709000, count: 3, resume } as const;
    const d12 = async (ipc: ReturnType<typeof createMockIpc>) => (await ipc.doctorRun()).checks.find((c) => c.id === "D12")!;
    const shape = async (paused: PauseInfo) => {
      const ipc = createMockIpc({ paused });
      expect((await ipc.recoveryStatus()).paused).toEqual(paused);
      return { ipc, check: await d12(ipc) };
    };
    // In effect: the theme is out. Never "not yet".
    let row = await shape({ ...crash, level: "computer" });
    expect(row.check).toMatchObject({ status: "warn", code: "E_THEME_PAUSED", fix: "resume" });
    expect(row.check.messageEn).toBe(
      "CMD Studio paused the theme on this computer: cmd crashed in its add-on 3 times. Turn it back on from CMD Studio's home page",
    );
    expect(row.check.messageTh).toBe("CMD Studio หยุดธีมของเครื่องนี้ไว้ เพราะ cmd แครชในส่วนเสริม 3 ครั้ง เปิดธีมกลับได้ที่หน้าแรกของ CMD Studio");
    // Not yet: CMD Studio could not take its changes out; the theme is paused in cmd only.
    row = await shape({ ...crash, level: "computerNotYet" });
    expect(row.check.messageEn).toBe(
      "The theme is paused in cmd only, not yet on this computer: CMD Studio could not take its changes out and tries again the next time the CMD Studio app starts",
    );
    expect(row.check.messageEn).not.toContain("paused on this computer");
    expect(row.check.detailsTh).toContain("ยังไม่สำเร็จ");
    // Not known: state.json can't be read.
    row = await shape({ ...crash, level: "computerUnknown" });
    expect(row.check.messageEn).toBe("A pause of this computer may be in place, but state.json could not be read");
    expect(row.check.messageTh).toBe("อาจมีการหยุดธีมของเครื่องนี้อยู่ แต่อ่าน state.json ไม่ได้");
    // A record that can't be read: no reason, no time; turning back on only lifts it.
    row = await shape({ by: "clink", level: "cmd", reason: null, atMs: null, count: 0, resume: null });
    expect(row.check.messageEn).toContain("can't read its pause record");
    expect(row.check.details).not.toContain("nothing on this computer was changed");
    expect(await row.ipc.themeResume()).toEqual({ kind: "resumed" });
    expect((await row.ipc.recoveryStatus()).paused).toBeNull();
    // A count of 0 is never said as "0 times": the crash reports could not be read.
    row = await shape({ ...crash, level: "computer", count: 0 });
    expect(row.check.messageEn).toBe(
      "CMD Studio paused the theme on this computer: cmd crashed in its add-on. Turn it back on from CMD Studio's home page",
    );
    expect(row.check.messageTh).not.toContain("0 ครั้ง");
    // And one time is said once.
    row = await shape({ ...crash, level: "computer", count: 1 });
    expect(row.check.messageEn).toContain("crashed in its add-on once.");
    // An app record older than the theme in use is a pause in cmd: the lift is at once, and the
    // theme it noted is not applied again.
    row = await shape({ by: "app", level: "cmd", reason: "user", atMs: 1, count: 0, resume });
    expect(row.check.messageEn).toContain("a theme was applied after this pause");
    expect(await row.ipc.themeResume()).toEqual({ kind: "resumed" });
    // Not yet or in effect, with a theme to apply: through the plan. Without one, or with one that
    // is no longer in the library: the Themes page.
    expect((await (await shape({ ...crash, level: "computerNotYet" })).ipc.themeResume()).kind).toBe("apply");
    expect((await (await shape({ ...crash, level: "computer", resume: null })).ipc.themeResume()).kind).toBe("pickTheme");
    const gone = { ...resume, themeId: "00000000-0000-4000-8000-000000000000", themeName: null };
    expect((await (await shape({ ...crash, level: "computerUnknown", resume: gone })).ipc.themeResume()).kind).toBe("pickTheme");
    // Every Thai text of the pause rows is free of the words the app never says.
    for (const level of ["computer", "computerNotYet", "computerUnknown", "cmd"] as const) {
      for (const reason of ["cmdStarts", "clinkCrash", "recovery", "user", null] as const) {
        for (const by of ["clink", "app"] as const) {
          const check = (await shape({ by, level, reason, atMs: 1, count: 3, resume: null })).check;
          for (const word of ["prompt", "เรา", "สำรอง", "รุ่น"]) {
            expect(check.messageTh, `${by}/${level}/${reason}`).not.toContain(word);
            expect(check.detailsTh ?? "", `${by}/${level}/${reason}`).not.toContain(word);
          }
        }
      }
    }
    // Taking the theme out makes a pause of the app's one in effect, as the backend's level says.
    const applied = createMockIpc({ paused: { ...crash, level: "computerNotYet" } });
    const request = { themeId: terracotta.id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    const planned = await applied.planApply(request);
    if (planned.kind !== "ready") throw new Error("not ready");
    await applied.apply(request, planned.plan.digest, () => {});
    // (An apply lifts any pause; the pause comes back as the test sets it.)
    applied.state.paused = { ...crash, level: "computerNotYet" };
    await applied.revertAll();
    expect(applied.state.paused).toMatchObject({ by: "app", level: "computer" });
  });

  it("gives the start's crash check as fixed data when it is told to, whatever shape", async () => {
    const terracotta = PRESETS[0]!;
    const refused = { pausedNow: false, revert: null, refused: { code: "E_BUSY", messageTh: "(E_BUSY)", messageEn: "(E_BUSY)", details: "x" }, notSaved: null };
    for (const crashCheck of [
      { kind: "read", count: 0, pausedNow: false, pause: refused },
      { kind: "unreadable", why: "the Application log is off", pause: null },
      { kind: "unreadable", why: "the Application log is off", pause: refused },
    ] as const) {
      const ipc = createMockIpc({ crashCheck });
      // As given at the first status, and the same at every later one; nothing else is worked out.
      expect((await ipc.recoveryStatus()).crashes).toEqual(crashCheck);
      expect((await ipc.recoveryStatus()).crashes).toEqual(crashCheck);
      expect(ipc.state.paused).toBeNull();
    }
    // Windows' crash reports that can't be read: D12 says it was not checked, with the reason.
    const ipc = createMockIpc({ crashCheck: { kind: "unreadable", why: "the Application log is off", pause: null } });
    const request = { themeId: terracotta.id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    const planned = await ipc.planApply(request);
    if (planned.kind !== "ready") throw new Error("not ready");
    await ipc.apply(request, planned.plan.digest, () => {});
    const d12 = (await ipc.doctorRun()).checks.find((c) => c.id === "D12")!;
    expect(d12.status).toBe("skipped");
    expect(d12.messageEn).toBe("Not checked: Windows' crash reports (the Application event log) could not be read: the Application log is off");
    expect(d12.messageTh).toContain("the Application log is off");
  });

  it("says a pause that did not go through the way the backend does: refused, rolled back, or not saved", async () => {
    const terracotta = PRESETS[0]!;
    const request = { themeId: terracotta.id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    const applied = async (failPause: "refused" | "rolledBack" | "notSaved") => {
      const ipc = createMockIpc({ failPause });
      const planned = await ipc.planApply(request);
      if (planned.kind !== "ready") throw new Error("not ready");
      await ipc.apply(request, planned.plan.digest, () => {});
      return ipc;
    };
    // Revert everything was refused: nothing changed but the record, and the theme is applied.
    let ipc = await applied("refused");
    let out = await ipc.themePause("user");
    expect(out).toMatchObject({ pausedNow: false, revert: null, refused: { code: "E_BUSY" }, notSaved: null });
    expect(ipc.state.applied).not.toBeNull();
    expect((await ipc.recoveryStatus()).paused).toMatchObject({ by: "app", level: "computerNotYet", reason: "user", resume: { themeId: terracotta.id } });
    expect((await ipc.doctorRun()).checks.find((c) => c.id === "D12")?.messageEn).toContain("not yet on this computer");
    // Rolled back: the revert ran and did not hold.
    ipc = await applied("rolledBack");
    out = await ipc.themePause("user");
    expect(out).toMatchObject({ pausedNow: false, refused: null, notSaved: null });
    expect(out.revert).toMatchObject({ ok: false, status: "rolledBack", error: { code: "E_AUTORUN_WRITE" } });
    expect(ipc.state.applied).not.toBeNull();
    expect((await ipc.recoveryStatus()).paused?.level).toBe("computerNotYet");
    // The record could not be written: the theme is out all the same, and no pause is there.
    ipc = await applied("notSaved");
    out = await ipc.themePause("user");
    expect(out).toMatchObject({ pausedNow: false, refused: null, notSaved: { code: "E_DATA_WRITE" } });
    expect(out.revert?.ok).toBe(true);
    expect(ipc.state.applied).toBeNull();
    expect((await ipc.recoveryStatus()).paused).toBeNull();
    // The start's own pause for three crashes goes the same way, and says so in the check.
    const crashing = createMockIpc({ cmdCrashes: 3, failPause: "refused" });
    const planned = await crashing.planApply(request);
    if (planned.kind !== "ready") throw new Error("not ready");
    await crashing.apply(request, planned.plan.digest, () => {});
    const status = await crashing.recoveryStatus();
    expect(status.crashes).toMatchObject({ kind: "read", count: 3, pausedNow: false, pause: { pausedNow: false, refused: { code: "E_BUSY" } } });
    expect(status.paused).toMatchObject({ level: "computerNotYet", reason: "clinkCrash", count: 3 });
  });
});
