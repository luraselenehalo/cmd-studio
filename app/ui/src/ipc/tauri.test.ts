// Tauri's IPC with `invoke` stubbed: what a command's rejection becomes. Tauri rejects with the
// serialized AppError, or with its own text when it could not even call the command (arguments
// that do not deserialize); the second must still reach the UI as an error of §5.9.
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { tauriIpc } from "./tauri";
import { IpcError, toIpcError } from "./types";

/**
 * The E_INTERNAL row of `MEANINGS` in error.rs (spec §5.9), as the Rust source has it. (A path,
 * not a URL: jsdom's `URL` replaces Node's here, and `fs` refuses it.)
 */
function internalRow(): { th: string; en: string } {
  const here = dirname(fileURLToPath(import.meta.url));
  const source = readFileSync(resolve(here, "../../../src-tauri/src/error.rs"), "utf8");
  const row = /\(\s*"E_INTERNAL",\s*r#"([^]*?)"#,\s*r#"([^]*?)"#,\s*\)/.exec(source);
  if (!row?.[1] || !row[2]) throw new Error("error.rs has no E_INTERNAL row");
  return { th: row[1], en: row[2] };
}

async function rejectionOf(promise: Promise<unknown>): Promise<IpcError> {
  const thrown: unknown = await promise.then(
    () => undefined,
    (e: unknown) => e,
  );
  expect(thrown).toBeInstanceOf(IpcError);
  return thrown as IpcError;
}

describe("Tauri's IPC", () => {
  it("makes a rejection that is not an AppError E_INTERNAL, with both texts and the original as details", async () => {
    const { th, en } = internalRow();
    const internal = (details: string) => ({ code: "E_INTERNAL", messageTh: th, messageEn: en, details });
    const text = "invalid args `context` for command `preview_render`: invalid type: floating point `12.5`, expected u16";
    invoke.mockRejectedValueOnce(text);
    expect((await rejectionOf(tauriIpc.systemDetect())).toAppError()).toEqual(internal(text));
    invoke.mockRejectedValueOnce(new Error("the window is gone"));
    const fromError = await rejectionOf(tauriIpc.themesList());
    expect(fromError.toAppError()).toEqual(internal("the window is gone"));
    expect(fromError.message).toBe(en);
    // Half an AppError is not one; what it held is kept.
    const half = { code: "E_BUSY", messageEn: "busy" };
    expect(toIpcError(half).toAppError()).toEqual(internal(JSON.stringify(half)));
  });

  it("passes an AppError through unchanged", async () => {
    const appError = {
      code: "E_BUSY",
      messageTh: "มี CMD Studio อีกตัวกำลังเปลี่ยนการตั้งค่าของเครื่องนี้",
      messageEn: "Another CMD Studio is changing this computer right now",
      details: "cmdstudio.lock is held",
    };
    invoke.mockRejectedValueOnce(appError);
    const passed = await rejectionOf(tauriIpc.themeGet("7d3c5f0e-2b1a-4c8e-9f6d-1a2b3c4d5e6f"));
    expect(passed.toAppError()).toEqual(appError);
    expect(passed.message).toBe(appError.messageEn);
    expect(toIpcError(passed)).toBe(passed);
  });
});
