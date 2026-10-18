// The start of the app: when the IPC cannot be picked (its module fails to load, or `pickIpc`
// itself rejects) the window shows the E_INTERNAL notice in English and Thai with Reload, never a
// blank window; when it can, the app is mounted. `start` is what main.tsx calls, and main.tsx is
// imported for real below, so the wiring is checked too.
import { act, cleanup, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockIpc } from "./ipc/mock";
import { start } from "./start";

const roots: Root[] = [];

beforeEach(() => {
  document.body.innerHTML = '<div id="root"></div>';
});

afterEach(async () => {
  await act(async () => {
    for (const root of roots.splice(0)) root.unmount();
  });
  cleanup();
  vi.unstubAllEnvs();
  vi.doUnmock("./ipc/tauri");
  vi.doUnmock("./ipc/mock");
  vi.resetModules();
  vi.restoreAllMocks();
});

function theRoot(): HTMLElement {
  const root = document.getElementById("root");
  if (!root) throw new Error("no #root in the test page");
  return root;
}

/** The English and the Thai notice are both there, with the code and what was thrown. */
async function expectFailureNotice(details: string | RegExp) {
  const notice = await screen.findByRole("alert");
  expect(within(notice).getByText("Something went wrong")).toBeTruthy();
  expect(within(notice).getByText("มีบางอย่างผิดพลาด")).toBeTruthy();
  expect(
    within(notice).getByText(
      "CMD Studio itself ran into a problem; the details say where. Try again, and if it stays, report it with the diagnostics",
    ),
  ).toBeTruthy();
  expect(
    within(notice).getByText("CMD Studio เองมีปัญหา รายละเอียดบอกว่าตรงไหน ลองใหม่อีกครั้ง ถ้ายังเป็นอยู่ให้แจ้งปัญหาพร้อมข้อมูลวินิจฉัย"),
  ).toBeTruthy();
  expect(within(notice).getByText(/Code E_INTERNAL/)).toBeTruthy();
  expect(within(notice).getByText(/รหัส E_INTERNAL/)).toBeTruthy();
  expect(within(notice).getByText(details)).toBeTruthy();
}

describe("start", () => {
  const failures: [string, () => Promise<never>, string][] = [
    [
      "the IPC's module fails to load",
      () => Promise.reject(new TypeError("Failed to fetch dynamically imported module: http://tauri.localhost/assets/tauri-1a2b3c.js")),
      "Failed to fetch dynamically imported module: http://tauri.localhost/assets/tauri-1a2b3c.js",
    ],
    ["pickIpc rejects with a text", () => Promise.reject("no IPC here"), "no IPC here"],
    [
      "pickIpc throws before it returns a promise",
      () => {
        throw new Error("pickIpc broke at once");
      },
      "pickIpc broke at once",
    ],
  ];

  it.each(failures)("shows the E_INTERNAL notice in English and Thai with Reload, not a blank window, when %s", async (_name, pick, details) => {
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    const reload = vi.fn();
    await act(async () => {
      roots.push(await start(theRoot(), pick, reload));
    });
    await expectFailureNotice(details);
    expect(theRoot().textContent).not.toBe("");
    // What went wrong is on the console as well.
    expect(logged).toHaveBeenCalledWith("CMD Studio: the app could not start", expect.anything());
    // The way out: Reload, in both languages.
    await userEvent.setup().click(screen.getByRole("button", { name: "Reload · โหลดใหม่" }));
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("mounts the app when the IPC is there", async () => {
    await act(async () => {
      roots.push(await start(theRoot(), () => Promise.resolve(createMockIpc())));
    });
    expect(await screen.findByRole("heading", { name: "Make your cmd window beautiful and easy to read" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("main.tsx", () => {
  // A module whose factory throws is a dynamic import that rejects; vitest words that error itself.
  const MOCKING_ERROR = /There was an error when mocking a module/;

  it("shows the notice when the dynamic import of the Tauri IPC rejects", async () => {
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    vi.doMock("./ipc/tauri", () => {
      throw new Error("the tauri chunk could not be loaded");
    });
    await act(async () => {
      await import("./main");
    });
    await expectFailureNotice(MOCKING_ERROR);
    expect(screen.getByRole("button", { name: "Reload · โหลดใหม่" })).toBeTruthy();
    expect(logged).toHaveBeenCalledWith("CMD Studio: the app could not start", expect.anything());
  });

  it("shows the notice when pickIpc itself rejects after the import went through", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.doMock("./ipc/tauri", () => ({
      get tauriIpc(): never {
        throw new Error("the Tauri IPC could not be set up");
      },
    }));
    await act(async () => {
      await import("./main");
    });
    await expectFailureNotice("the Tauri IPC could not be set up");
  });

  it("shows the notice when the mock IPC module fails to load (the visual check's build)", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.stubEnv("VITE_MOCK_IPC", "1");
    vi.doMock("./ipc/mock", () => {
      throw new Error("the mock chunk could not be loaded");
    });
    await act(async () => {
      await import("./main");
    });
    await expectFailureNotice(MOCKING_ERROR);
  });

  it("mounts the app with the Tauri IPC when it loads", async () => {
    vi.doMock("./ipc/tauri", async () => {
      const { createMockIpc: create } = await import("./ipc/mock");
      return { tauriIpc: create() };
    });
    await act(async () => {
      await import("./main");
    });
    expect(await screen.findByRole("heading", { name: "Make your cmd window beautiful and easy to read" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
