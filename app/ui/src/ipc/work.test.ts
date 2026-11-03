// The wrapper of recovery spec §6: it counts what waits, says so after ten seconds, gives a
// read-only command up after a minute with E_TIMEOUT, drops the answer that comes after that,
// and offers a restart once a change runs for three minutes. Fake timers throughout.
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { en } from "../i18n/en";
import { createMockIpc } from "./mock";
import { IpcError, type Ipc } from "./types";
import { KINDS, READ_DEADLINE_MS, RESTART_AFTER_MS, STILL_WORKING_MS, TASK, TIMEOUT_ROW, withWork } from "./work";

/** The E_TIMEOUT row of `MEANINGS` in error.rs (spec §5.9), as the Rust source has it. */
function timeoutRow(): { th: string; en: string } {
  const here = dirname(fileURLToPath(import.meta.url));
  const source = readFileSync(resolve(here, "../../../src-tauri/src/error.rs"), "utf8");
  const row = /\(\s*"E_TIMEOUT",\s*r#"([^]*?)"#,\s*r#"([^]*?)"#,\s*\)/.exec(source);
  if (!row?.[1] || !row[2]) throw new Error("error.rs has no E_TIMEOUT row");
  return { th: row[1], en: row[2] };
}

/** A promise the test settles by hand. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

describe("the one list of commands", () => {
  it("names every method of Ipc once, with its kind and its task", () => {
    const methods = Object.keys(createMockIpc()).filter((k) => k !== "state").sort();
    expect(Object.keys(KINDS).sort()).toEqual(methods);
    expect(Object.keys(TASK).sort()).toEqual(methods);
    expect(KINDS.apply).toBe("changes");
    expect(KINDS.undoLast).toBe("changes");
    expect(KINDS.snapshotRestore).toBe("changes");
    expect(KINDS.revertAll).toBe("changes");
    expect(KINDS.themePause).toBe("changes");
    expect(KINDS.themeImportPreview).toBe("user");
    expect(KINDS.themeExport).toBe("user");
    expect(KINDS.appRestart).toBe("exit");
    expect(Object.values(KINDS).filter((k) => k === "changes")).toHaveLength(5);
    for (const key of Object.values(TASK)) expect(key in en, key).toBe(true);
  });

  it("carries the E_TIMEOUT row of spec §5.9 as error.rs has it", () => {
    expect(TIMEOUT_ROW).toEqual(timeoutRow());
  });
});

describe("the wrapper", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("says what still works after ten seconds and clears when it answers", async () => {
    const inner = createMockIpc();
    const held = deferred<Awaited<ReturnType<Ipc["themesList"]>>>();
    vi.spyOn(inner, "themesList").mockReturnValue(held.promise);
    const { ipc, work } = withWork(inner, () => Date.now());
    const seen: string[] = [];
    work.subscribe(() => seen.push(String(work.get().task)));
    const call = ipc.themesList();
    expect(work.get().task).toBeNull();
    await vi.advanceTimersByTimeAsync(STILL_WORKING_MS - 1000);
    expect(work.get().task).toBeNull();
    await vi.advanceTimersByTimeAsync(1000);
    expect(work.get()).toEqual({ task: "work.readingThemes", offerRestart: false });
    held.resolve(await createMockIpc().themesList());
    await call;
    expect(work.get().task).toBeNull();
    expect(seen).toContain("work.readingThemes");
  });

  it("gives a read-only command up after a minute with E_TIMEOUT, both texts from the row", async () => {
    const inner = createMockIpc();
    vi.spyOn(inner, "doctorRun").mockReturnValue(new Promise(() => {}));
    const { ipc, work } = withWork(inner, () => Date.now());
    const call = ipc.doctorRun();
    const settled = call.then(
      () => "resolved",
      (e: unknown) => e,
    );
    await vi.advanceTimersByTimeAsync(READ_DEADLINE_MS);
    const e = await settled;
    expect(e).toBeInstanceOf(IpcError);
    expect((e as IpcError).code).toBe("E_TIMEOUT");
    expect((e as IpcError).messageEn).toBe(TIMEOUT_ROW.en);
    expect((e as IpcError).messageTh).toBe(TIMEOUT_ROW.th);
    expect((e as IpcError).details).toBe("doctor_run did not answer within 60 s");
    expect(work.get().task).toBeNull();
  });

  it("a late answer after the deadline is dropped and the retry's answer is shown", async () => {
    // Review Focus 3.
    const inner = createMockIpc();
    const first = deferred<string>();
    const second = deferred<string>();
    vi.spyOn(inner, "diagnosticsCopy").mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { ipc, work } = withWork(inner, () => Date.now());
    const shown: string[] = [];
    const show = (p: Promise<string>) =>
      p.then(
        (text) => shown.push(text),
        () => shown.push("E_TIMEOUT"),
      );
    const one = show(ipc.diagnosticsCopy());
    await vi.advanceTimersByTimeAsync(READ_DEADLINE_MS);
    await one;
    expect(shown).toEqual(["E_TIMEOUT"]);
    const two = show(ipc.diagnosticsCopy());
    first.resolve("late");
    await vi.advanceTimersByTimeAsync(0);
    second.resolve("fresh");
    await two;
    expect(shown).toEqual(["E_TIMEOUT", "fresh"]);
    expect(work.get().task).toBeNull();
  });

  it("never gives up a change, a dialog or the restart, and offers the restart after three minutes of a change", async () => {
    const inner = createMockIpc();
    vi.spyOn(inner, "apply").mockReturnValue(new Promise(() => {}));
    vi.spyOn(inner, "themeExport").mockReturnValue(new Promise(() => {}));
    const { ipc, work } = withWork(inner, () => Date.now());
    const change = ipc.apply({ themeId: "x", target: "all", setWtDefaultProfile: true, replaceOtherClink: false }, "d", () => {});
    const dialog = ipc.themeExport("x");
    let settled = false;
    void change.finally(() => (settled = true));
    void dialog.finally(() => (settled = true));
    await vi.advanceTimersByTimeAsync(READ_DEADLINE_MS + 1000);
    expect(settled).toBe(false);
    expect(work.get()).toEqual({ task: "work.applying", offerRestart: false });
    await vi.advanceTimersByTimeAsync(RESTART_AFTER_MS - READ_DEADLINE_MS);
    expect(work.get()).toEqual({ task: "work.applying", offerRestart: true });
  });

  it("the restart itself is never counted as work", async () => {
    const inner = createMockIpc();
    vi.spyOn(inner, "appRestart").mockReturnValue(new Promise(() => {}));
    const { ipc, work } = withWork(inner, () => Date.now());
    void ipc.appRestart();
    await vi.advanceTimersByTimeAsync(STILL_WORKING_MS * 2);
    expect(work.get().task).toBeNull();
  });

  it("an Ok answer of the restart passes through, a rejection too, and neither is a timeout", async () => {
    const inner = createMockIpc();
    const { ipc, work } = withWork(inner, () => Date.now());
    await expect(ipc.appRestart()).resolves.toBeUndefined();
    expect(inner.state.restarted).toBe(true);
    const failure = new IpcError({ code: "E_INTERNAL", messageTh: "t", messageEn: "e", details: "spawn failed" });
    vi.spyOn(inner, "appRestart").mockRejectedValue(failure);
    await expect(ipc.appRestart()).rejects.toBe(failure);
    expect(work.get().task).toBeNull();
  });
});
