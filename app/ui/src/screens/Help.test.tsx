// Help's six tabs and the Advanced page against the mock IPC (this plan's Task 1 owns them,
// ruling Q15), in a small shell of this file's own with the shipped plumbing every shell keeps:
// the app state, the router, useRouteFocus and the after-apply bar. The tabs are toggle buttons
// in a named group: choosing one leaves focus on it, a real route change moves it to the heading
// (Q4). The doctor's fixes follow the F2 mapping: an apply goes through the plan, with no Close
// while it runs and its outcome kept (R7), and "Replace the other add-on" is the user's yes; a
// window that could not be opened is said. A restore says what it undid and what its start found
// (2c carry); the revert's steps say why in the chosen language, their English behind "Show
// details" (Q5); the copy says Copied; the credits have no SignPath line until Phase 4 (Q6); the
// Advanced page is read only; every button has a name in both languages.
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useCallback, useMemo, useRef, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import type { ApplyOutcome } from "../bindings/ApplyOutcome";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { CheckReport } from "../bindings/CheckReport";
import type { RestoreOutcome } from "../bindings/RestoreOutcome";
import type { RevertOutcome } from "../bindings/RevertOutcome";
import { AfterApplyBar } from "../components/AfterApplyBar";
import { I18nProvider, useT, type Dictionary, type Language } from "../i18n";
import { en } from "../i18n/en";
import { th } from "../i18n/th";
import { IpcProvider } from "../ipc/context";
import { createMockIpc } from "../ipc/mock";
import { PRESETS } from "../ipc/presets";
import { IpcError } from "../ipc/types";
import { NavContext, useNav, type Nav, type Route } from "../router";
import { AppStateContext, DEFAULT_SETTINGS, useDraftProblems, type AppState, type LastApply } from "../state";
import { useRouteFocus } from "../useRouteFocus";
import { Advanced } from "./Advanced";
import { BUNDLED, SCHEMES } from "./credits";
import { Help } from "./Help";

type MockIpc = ReturnType<typeof createMockIpc>;
type User = ReturnType<typeof userEvent.setup>;

const OTHER_CLINK = '"C:\\clink\\clink_x64.exe" inject --autorun';
const TABS = ["doctor", "restore", "revert", "report", "guide", "credits"] as const;

function presetId(name: string): string {
  const found = PRESETS.find((p) => p.name === name);
  if (!found) throw new Error(`no preset ${name}`);
  return found.id;
}

function ipcError(code: string, details: string): IpcError {
  return new IpcError({ code, messageTh: `(${code})`, messageEn: `(${code})`, details });
}

/** Applies a preset in the mock before the page mounts, as the Themes page would have. */
async function applyIn(ipc: MockIpc, name: string, replaceOtherClink = false): Promise<LastApply> {
  const request: ApplyRequest = { themeId: presetId(name), target: "all", setWtDefaultProfile: true, replaceOtherClink };
  const planned = await ipc.planApply(request);
  if (planned.kind !== "ready") throw new Error(`no plan for ${name}`);
  const outcome = await ipc.apply(request, planned.plan.digest, () => {});
  if (outcome.status !== "committed") throw new Error(`${name} was not applied`);
  return { outcome, target: request.target };
}

/** The shell around the page: the app state, the router and the route focus, as every shell has
 * them; a menu with Help and Advanced; the after-apply bar. */
function Shell({ language, start, applied }: { language: Language; start: Route; applied: LastApply | null }) {
  const [lang, setLang] = useState<Language>(language);
  const [route, setRoute] = useState<Route>(start);
  const [lastApply, setLastApply] = useState<LastApply | null>(applied);
  const draftProblems = useDraftProblems();
  const [version, setVersion] = useState(0);
  const bump = useCallback(() => setVersion((v) => v + 1), []);
  const saveSettings = useCallback(async () => {}, []);
  const settings = useMemo(() => ({ ...DEFAULT_SETTINGS, language: lang, wizardDone: true }), [lang]);
  const appState = useMemo<AppState>(
    () => ({ settings, saveSettings, lastApply, setLastApply, draftProblems, version, bump }),
    [settings, saveSettings, lastApply, draftProblems, version, bump],
  );
  const nav = useMemo<Nav>(() => ({ route, go: setRoute }), [route]);
  return (
    <I18nProvider language={lang} setLanguage={setLang}>
      <AppStateContext.Provider value={appState}>
        <NavContext.Provider value={nav}>
          <div lang={lang}>
            <Body />
          </div>
        </NavContext.Provider>
      </AppStateContext.Provider>
    </I18nProvider>
  );
}

function Body() {
  const t = useT();
  const { route, go } = useNav();
  const main = useRef<HTMLElement>(null);
  useRouteFocus(route, main);
  return (
    <>
      <AfterApplyBar />
      <nav aria-label={t("a11y.mainMenu")}>
        <button type="button" onClick={() => go({ name: "help", tab: "doctor" })}>
          {t("nav.help")}
        </button>
        <button type="button" onClick={() => go({ name: "advanced" })}>
          {t("nav.advanced")}
        </button>
      </nav>
      <main ref={main}>
        {route.name === "help" && <Help tab={route.tab} />}
        {route.name === "advanced" && <Advanced />}
      </main>
    </>
  );
}

function mount(
  ipc: MockIpc,
  { language = "en", start = { name: "help", tab: "doctor" }, applied = null }: { language?: Language; start?: Route; applied?: LastApply | null } = {},
) {
  const user = userEvent.setup();
  const view = render(
    <IpcProvider ipc={ipc}>
      <Shell language={language} start={start} applied={applied} />
    </IpcProvider>,
  );
  return { user, view };
}

const settle = () =>
  act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });

async function openTab(user: User, name: string, group: string = en["help.tabs.label"]) {
  await user.click(within(screen.getByRole("group", { name: group })).getByRole("button", { name }));
}

/** The page's own button of that name: not Help's tab of the same name, not a dialog's. */
function pageButton(name: string, group: string = en["help.tabs.label"]): HTMLElement {
  const tabs = screen.getByRole("group", { name: group });
  const found = screen.getAllByRole("button", { name }).filter((b) => !tabs.contains(b) && !b.closest('[role="dialog"]'));
  expect(found).toHaveLength(1);
  return found[0]!;
}

/** The health check's item whose message is `message`. */
async function check(message: string): Promise<HTMLElement> {
  return (await screen.findByText(message)).closest("li") as HTMLElement;
}

/** The after-apply bar's text, or null when there is no bar. */
function bar(): string | null {
  return document.querySelector(".bar")?.textContent ?? null;
}

/** Holds every `apply` of the mock until `release()` is called. */
function holdApply(ipc: MockIpc): () => void {
  let release: () => void = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const apply = ipc.apply.bind(ipc);
  vi.spyOn(ipc, "apply").mockImplementation(async (request, digest, onProgress): Promise<ApplyOutcome> => {
    await gate;
    return apply(request, digest, onProgress);
  });
  return () => release();
}

describe("Help's tabs", () => {
  it("are six toggle buttons in a named group; a tab leaves focus on it, a route change moves it to the heading (Q4)", async () => {
    const { user } = mount(createMockIpc());
    await screen.findByRole("heading", { level: 1, name: "Help" });
    expect(screen.queryAllByRole("tab")).toEqual([]);
    expect(screen.queryAllByRole("tablist")).toEqual([]);
    expect(screen.queryAllByRole("tabpanel")).toEqual([]);
    const group = screen.getByRole("group", { name: "Help topics" });
    expect(within(group).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Health check",
      "Restore points",
      "Undo everything",
      "Report a problem",
      "Short guide",
      "Credits and licenses",
    ]);
    expect(within(group).getByRole("button", { name: "Health check", pressed: true })).toBeTruthy();
    const restore = within(group).getByRole("button", { name: "Restore points" });
    await user.click(restore);
    await screen.findByRole("heading", { level: 2, name: "Restore points" });
    await settle();
    expect(within(group).getByRole("button", { name: "Restore points", pressed: true })).toBe(restore);
    expect(within(group).getByRole("button", { name: "Health check", pressed: false })).toBeTruthy();
    // Only the tab changed: focus stays on the button just pressed.
    expect(document.activeElement).toBe(restore);
    // A real route change moves focus to the new screen's heading, and back again.
    const menu = screen.getByRole("navigation");
    await user.click(within(menu).getByRole("button", { name: "Advanced" }));
    const advanced = await screen.findByRole("heading", { level: 1, name: "Advanced" });
    await waitFor(() => expect(document.activeElement).toBe(advanced));
    await user.click(within(menu).getByRole("button", { name: "Help" }));
    const help = await screen.findByRole("heading", { level: 1, name: "Help" });
    await waitFor(() => expect(document.activeElement).toBe(help));
  });
});

describe("the health check", () => {
  it("lists each check with its status, message and code, offers the fixes the page can run, and checks again", async () => {
    const ipc = createMockIpc({ otherClink: [OTHER_CLINK] });
    await applyIn(ipc, "Terracotta", true);
    const { user } = mount(ipc);
    const d5 = await check("Another Clink runs from AutoRun");
    expect(within(d5).getByText("D5 · Look")).toBeTruthy();
    expect(within(d5).getByText("E_AUTORUN_FOREIGN_CLINK")).toBeTruthy();
    expect(within(d5).getByRole("button", { name: "Replace the other add-on" })).toBeTruthy();
    // Its details are the commands that the page's yes is for.
    expect(within(d5).getByText(OTHER_CLINK)).toBeTruthy();
    const d6 = await check("Windows Terminal is installed and its settings parse");
    expect(within(d6).getByText("D6 · OK")).toBeTruthy();
    expect(within(d6).queryAllByRole("button")).toEqual([]);
    const d3 = await check("No cmd window has loaded the theme yet");
    await user.click(within(d3).getByRole("button", { name: "Open a new cmd window" }));
    await waitFor(() => expect(ipc.state.calls).toContain("openCmd"));
    const runs = ipc.state.calls.filter((c) => c === "doctorRun").length;
    await user.click(screen.getByRole("button", { name: "Check now" }));
    await waitFor(() => expect(ipc.state.calls.filter((c) => c === "doctorRun").length).toBe(runs + 1));
  });

  it("applies the other add-on's fix through the plan: the user's yes, no Close while it runs, the outcome kept (R7)", async () => {
    const ipc = createMockIpc({ otherClink: [OTHER_CLINK] });
    await applyIn(ipc, "Nord", true);
    const release = holdApply(ipc);
    const { user } = mount(ipc);
    const d5 = await check("Another Clink runs from AutoRun");
    await user.click(within(d5).getByRole("button", { name: "Replace the other add-on" }));
    const dialog = await screen.findByRole("dialog", { name: "What will change" });
    // The page was the yes: no question again, and the plan lists the commands it takes out.
    expect(await within(dialog).findByText(en["apply.replacesOtherClink"])).toBeTruthy();
    expect(within(dialog).getByText(OTHER_CLINK)).toBeTruthy();
    expect(within(dialog).queryByRole("button", { name: "Use CMD Studio's instead" })).toBeNull();
    expect(within(dialog).getByRole("button", { name: "Close" })).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(await within(dialog).findByText("Applying…")).toBeTruthy();
    expect(within(dialog).queryByRole("button", { name: "Close" })).toBeNull();
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBe(dialog);
    release();
    expect(await within(dialog).findByText(/Theme "Nord" applied/)).toBeTruthy();
    expect(ipc.state.applied?.request.replaceOtherClink).toBe(true);
    // The dialog stays with its outcome until Close; the bar says it too.
    await settle();
    expect(screen.getByRole("dialog")).toBe(dialog);
    expect(bar()).toMatch(/Theme "Nord" applied · open a new cmd window/);
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("applies the theme again through the plan, with the request's own target and no yes carried over", async () => {
    const ipc = createMockIpc();
    vi.spyOn(ipc, "doctorRun").mockResolvedValue({
      checks: [
        {
          id: "D9",
          status: "fail",
          code: "E_GENERATED_CHANGED",
          messageTh: "ไฟล์ที่ CMD Studio สร้างถูกแก้",
          messageEn: "A file CMD Studio wrote was changed",
          details: "init.lua differs from what was written",
          detailsTh: null,
          fix: "reapply",
        },
      ],
      checkedAtMs: 1791694709000,
    });
    vi.spyOn(ipc, "doctorFix").mockResolvedValue({
      kind: "reapply",
      request: { themeId: presetId("Paper"), target: "wt", setWtDefaultProfile: false, replaceOtherClink: true },
      plan: null,
    });
    const { user } = mount(ipc);
    const d9 = await check("A file CMD Studio wrote was changed");
    await user.click(within(d9).getByRole("button", { name: "Apply the theme again" }));
    const dialog = await screen.findByRole("dialog", { name: "What will change" });
    await user.click(await within(dialog).findByRole("button", { name: "Apply" }));
    expect(await within(dialog).findByText(/Theme "Paper" applied · open a new cmd tab in Windows Terminal/)).toBeTruthy();
    expect(ipc.state.applied?.request).toEqual({
      themeId: presetId("Paper"),
      target: "wt",
      setWtDefaultProfile: false,
      replaceOtherClink: false,
    });
    expect(bar()).toMatch(/open a new cmd tab in Windows Terminal/);
  });

  it("opens the Store for Windows Terminal, and says when a window could not be opened", async () => {
    const ipc = createMockIpc({ noWindowsTerminal: true });
    await applyIn(ipc, "Terracotta");
    const { user } = mount(ipc);
    const d6 = await check("Windows Terminal was not found");
    await user.click(within(d6).getByRole("button", { name: "Install Windows Terminal" }));
    await waitFor(() => expect(ipc.state.calls).toContain("openExternal:windowsTerminalStore"));
    vi.spyOn(ipc, "openExternal").mockRejectedValue(ipcError("E_INTERNAL", "ShellExecuteW failed: the Store is not there"));
    await user.click(within(d6).getByRole("button", { name: "Install Windows Terminal" }));
    expect(await screen.findByText("ShellExecuteW failed: the Store is not there")).toBeTruthy();
    vi.spyOn(ipc, "openCmd").mockRejectedValue(ipcError("E_INTERNAL", "CreateProcessW failed: cmd.exe"));
    const d3 = await check("No cmd window has loaded the theme yet");
    await user.click(within(d3).getByRole("button", { name: "Open a new cmd window" }));
    expect(await screen.findByText("CreateProcessW failed: cmd.exe")).toBeTruthy();
    expect(screen.queryByText("ShellExecuteW failed: the Store is not there")).toBeNull();
  });

  it("shows a fix only the user can take as words, in Thai, with the Thai details", async () => {
    const ipc = createMockIpc();
    const checks: CheckReport[] = [
      {
        id: "D1",
        status: "fail",
        code: "E_RUNTIME_MISSING",
        messageTh: "ไฟล์ที่มากับ CMD Studio เสียหาย",
        messageEn: "CMD Studio's own files are damaged",
        details: "runtime\\clink-1.9.34\\clink_x64.exe: the bundled file differs",
        detailsTh: null,
        fix: "reinstall",
      },
      {
        id: "D6",
        status: "warn",
        code: "E_WT_NOT_FOUND",
        messageTh: "มี Windows Terminal แต่ยังไม่เคยเปิด",
        messageEn: "Windows Terminal is installed but was never opened",
        details: "Windows Terminal's settings.json is not there yet",
        detailsTh: "ยังไม่มีไฟล์ settings.json ของ Windows Terminal",
        fix: "openWindowsTerminalOnce",
      },
      {
        id: "D2",
        status: "fail",
        code: "E_OMP_RUN",
        messageTh: "Oh My Posh รันไม่ได้",
        messageEn: "Oh My Posh does not run",
        details: "",
        detailsTh: null,
        fix: null,
      },
    ];
    vi.spyOn(ipc, "doctorRun").mockResolvedValue({ checks, checkedAtMs: 1791694709000 });
    mount(ipc, { language: "th" });
    const d1 = await check("ไฟล์ที่มากับ CMD Studio เสียหาย");
    expect(within(d1).getByText("D1 · มีปัญหา")).toBeTruthy();
    expect(within(d1).queryAllByRole("button")).toEqual([]);
    expect(within(d1).getByText(th["help.doctor.fix.reinstall"])).toBeTruthy();
    expect(within(d1).getByText(th["help.whatToDo"])).toBeTruthy();
    // No Thai details: the English ones.
    expect(within(d1).getByText("runtime\\clink-1.9.34\\clink_x64.exe: the bundled file differs")).toBeTruthy();
    const d6 = await check("มี Windows Terminal แต่ยังไม่เคยเปิด");
    expect(within(d6).queryAllByRole("button")).toEqual([]);
    expect(within(d6).getByText(th["help.doctor.fix.openWindowsTerminalOnce"])).toBeTruthy();
    expect(within(d6).getByText("ยังไม่มีไฟล์ settings.json ของ Windows Terminal")).toBeTruthy();
    expect(within(d6).queryByText("Windows Terminal's settings.json is not there yet")).toBeNull();
    // A finding with no fix is its message only.
    const d2 = await check("Oh My Posh รันไม่ได้");
    expect(within(d2).queryByText(th["help.whatToDo"])).toBeNull();
    expect(within(d2).queryByText(th["common.details"])).toBeNull();
    expect(screen.queryByText("CMD Studio's own files are damaged")).toBeNull();
  });

  it("checks again, and says so, when the computer changed since the check", async () => {
    const ipc = createMockIpc({ otherClink: [OTHER_CLINK] });
    await applyIn(ipc, "Terracotta", true);
    vi.spyOn(ipc, "doctorFix").mockResolvedValue({ kind: null, request: null, plan: null });
    const { user } = mount(ipc);
    const d5 = await check("Another Clink runs from AutoRun");
    const runs = ipc.state.calls.filter((c) => c === "doctorRun").length;
    await user.click(within(d5).getByRole("button", { name: "Replace the other add-on" }));
    expect(await screen.findByText(en["help.doctor.fixChanged"])).toBeTruthy();
    await waitFor(() => expect(ipc.state.calls.filter((c) => c === "doctorRun").length).toBe(runs + 1));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("restore points", () => {
  it("lists them newest first and goes back to one after asking, saying what it undid", async () => {
    const ipc = createMockIpc();
    await applyIn(ipc, "Nord");
    const applied = await applyIn(ipc, "Terracotta");
    const { user } = mount(ipc, { start: { name: "help", tab: "restore" }, applied });
    expect(bar()).toMatch(/Theme "Terracotta" applied/);
    expect(await screen.findAllByRole("button", { name: "Go back to this point" })).toHaveLength(2);
    // Newest first, each Go back named by its point.
    const terracotta = screen.getByRole("group", { name: /^Terracotta · / });
    const nord = screen.getByRole("group", { name: /^Nord · / });
    expect(terracotta.compareDocumentPosition(nord) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await user.click(within(nord).getByRole("button", { name: "Go back to this point" }));
    const dialog = screen.getByRole("dialog", { name: "Go back to this point" });
    expect(within(dialog).getByText(en["help.restore.confirm"])).toBeTruthy();
    expect(ipc.state.applied?.theme.name).toBe("Terracotta");
    await user.click(within(dialog).getByRole("button", { name: "Go back to this point" }));
    const done = await screen.findByText("Done: everything is back to that point.");
    await waitFor(() => expect(document.activeElement?.contains(done)).toBe(true));
    // What it undid, by theme, newest first.
    const undone = screen.getByText(en["help.restore.undoneTitle"]).parentElement as HTMLElement;
    const lines = within(undone).getAllByRole("listitem").map((li) => li.textContent);
    expect(lines).toHaveLength(2);
    expect(lines[0]).toMatch(/^Terracotta · /);
    expect(lines[1]).toMatch(/^Nord · /);
    expect(ipc.state.applied).toBeNull();
    // The bar's apply is undone with it, and the list shows both points undone.
    await waitFor(() => expect(bar()).toBeNull());
    await waitFor(() => expect(screen.queryAllByRole("button", { name: "Go back to this point" })).toEqual([]));
    expect(screen.getAllByText("Undone")).toHaveLength(2);
  });

  it("runs one restore at a time and says it is running", async () => {
    const ipc = createMockIpc();
    await applyIn(ipc, "Nord");
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const restore = ipc.snapshotRestore.bind(ipc);
    const spy = vi.spyOn(ipc, "snapshotRestore").mockImplementation(async (id): Promise<RestoreOutcome> => {
      await gate;
      return restore(id);
    });
    const { user } = mount(ipc, { start: { name: "help", tab: "restore" } });
    await user.click(await screen.findByRole("button", { name: "Go back to this point" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Go back to this point" }));
    expect(await screen.findByText("Going back to this point…")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Go back to this point" }) as HTMLButtonElement).disabled).toBe(true);
    release();
    expect(await screen.findByText("Done: everything is back to that point.")).toBeTruthy();
    expect(spy).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("Going back to this point…")).toBeNull();
  });

  it("says what the restore's start found with its details, why it stopped, and a font it left", async () => {
    const ipc = createMockIpc();
    await applyIn(ipc, "Nord");
    const stopped: RestoreOutcome = {
      status: "stopped",
      undone: [],
      last: {
        status: "rolledBack",
        undid: null,
        error: {
          code: "E_WT_WRITE",
          messageTh: "เขียน settings.json ของ Windows Terminal ไม่ได้",
          messageEn: "Windows Terminal's settings.json could not be written",
          details: "access is denied",
        },
        unrestored: [],
        notes: [
          {
            code: "E_JOURNAL_READ",
            messageTh: "อ่านบันทึกของการใช้ธีมครั้งก่อนไม่ได้",
            messageEn: "A record of an earlier apply could not be read",
            details: "journal.json of the undo: unexpected end of file",
          },
        ],
        fontsLeft: [],
      },
      notes: [
        {
          code: "E_SNAPSHOT_READ",
          messageTh: "อ่านจุดย้อนกลับไม่ได้",
          messageEn: "A restore point could not be read",
          details: "manifest.json of 1791694700000-0badc0de is not JSON",
        },
      ],
    };
    const done: RestoreOutcome = {
      status: "done",
      undone: [],
      last: {
        status: "committed",
        undid: null,
        error: null,
        unrestored: [],
        notes: [],
        fontsLeft: [
          {
            family: "CaskaydiaCove NF",
            messageEn: "CaskaydiaCove NF stays installed: you installed a font of that name since",
            messageTh: "ฟอนต์ CaskaydiaCove NF ยังติดตั้งอยู่ เพราะคุณติดตั้งฟอนต์ชื่อนี้ไว้ทีหลัง",
          },
        ],
      },
      notes: [],
    };
    vi.spyOn(ipc, "snapshotRestore").mockResolvedValueOnce(stopped).mockResolvedValueOnce(done);
    const { user } = mount(ipc, { start: { name: "help", tab: "restore" } });
    const goBack = async () => {
      await user.click(await screen.findByRole("button", { name: "Go back to this point" }));
      await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Go back to this point" }));
    };
    await goBack();
    expect(await screen.findByText("Stopped part-way; see the message.")).toBeTruthy();
    const note = screen.getByText("A restore point could not be read").closest("li") as HTMLElement;
    expect(within(note).getByText("E_SNAPSHOT_READ")).toBeTruthy();
    expect(within(note).getByText("manifest.json of 1791694700000-0badc0de is not JSON")).toBeTruthy();
    expect(screen.getByText("A record of an earlier apply could not be read")).toBeTruthy();
    expect(screen.getByText("Windows Terminal's settings.json could not be written")).toBeTruthy();
    await goBack();
    expect(await screen.findByText("Done: everything is back to that point.")).toBeTruthy();
    expect(screen.getByText("CaskaydiaCove NF stays installed: you installed a font of that name since")).toBeTruthy();
    expect(screen.queryByText("Stopped part-way; see the message.")).toBeNull();
  });

  it("says the restore points that could not be read, with their details", async () => {
    const ipc = createMockIpc();
    vi.spyOn(ipc, "snapshotsList").mockResolvedValue({
      points: [],
      problems: [
        {
          code: "E_SNAPSHOT_READ",
          messageTh: "อ่านจุดย้อนกลับไม่ได้",
          messageEn: "A restore point could not be read",
          details: "manifest.json of 1791694700000-0badc0de is not JSON",
        },
      ],
    });
    mount(ipc, { start: { name: "help", tab: "restore" } });
    expect(await screen.findByText("1 restore point(s) could not be read.")).toBeTruthy();
    expect(screen.getByText("manifest.json of 1791694700000-0badc0de is not JSON")).toBeTruthy();
    expect(screen.getByText("No restore points yet.")).toBeTruthy();
  });
});

describe("undo everything", () => {
  it("asks first, then says each step's result, its English behind Show details", async () => {
    const ipc = createMockIpc();
    const applied = await applyIn(ipc, "Terracotta");
    const { user } = mount(ipc, { start: { name: "help", tab: "revert" }, applied });
    await screen.findByRole("heading", { level: 2, name: "Undo everything" });
    await user.click(pageButton("Undo everything"));
    const dialog = screen.getByRole("dialog", { name: "Undo everything" });
    expect(within(dialog).getByText(en["help.revert.confirm"])).toBeTruthy();
    expect(ipc.state.calls).not.toContain("revertAll");
    await user.click(within(dialog).getByRole("button", { name: "Undo everything" }));
    const done = await screen.findByText("Done: everything CMD Studio changed is back as it was.");
    await waitFor(() => expect(document.activeElement?.contains(done)).toBe(true));
    expect(screen.getByText("Step 1: done")).toBeTruthy();
    expect(screen.getByText("Step 2: done")).toBeTruthy();
    const what = screen.getByText("AutoRun and Windows Terminal's settings: CMD Studio's part is taken out");
    expect((what.closest("details") as HTMLDetailsElement).open).toBe(false);
    expect(ipc.state.applied).toBeNull();
    await waitFor(() => expect(bar()).toBeNull());
  });

  it("says a failed step's reason and what to do in Thai, and keeps the bar after a rollback (Q5)", async () => {
    const ipc = createMockIpc();
    const applied = await applyIn(ipc, "Terracotta");
    const failed: RevertOutcome = {
      ok: false,
      steps: [
        { what: "AutoRun", ok: true, detail: "CMD Studio's part is taken out", code: null, fixEn: null, fixTh: null, error: null },
        {
          what: "Windows Terminal's settings",
          ok: false,
          detail: "E_MACHINE_READ: it could not be read: access is denied",
          code: "E_MACHINE_READ",
          fixEn: "Close Windows Terminal and try again",
          fixTh: "ปิด Windows Terminal แล้วลองใหม่",
          error: {
            code: "E_MACHINE_READ",
            messageTh: "อ่านข้อมูลบางอย่างในเครื่องนี้ไม่ได้",
            messageEn: "Something on this computer could not be read",
            details: "it could not be read: access is denied",
          },
        },
      ],
      notes: [],
      status: "rolledBack",
      error: null,
      unrestored: [],
    };
    vi.spyOn(ipc, "revertAll").mockResolvedValue(failed);
    const { user } = mount(ipc, { language: "th", start: { name: "help", tab: "revert" }, applied });
    await screen.findByRole("heading", { level: 2, name: th["help.revert.title"] });
    await user.click(pageButton(th["help.revert.button"], th["help.tabs.label"]));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: th["help.revert.button"] }));
    expect(await screen.findByText(th["help.revert.notOk"])).toBeTruthy();
    expect(screen.getByText("ขั้นที่ 1: เสร็จแล้ว")).toBeTruthy();
    const step = screen.getByText("ขั้นที่ 2: ไม่สำเร็จ").closest("li") as HTMLElement;
    expect(within(step).getByText("อ่านข้อมูลบางอย่างในเครื่องนี้ไม่ได้")).toBeTruthy();
    expect(within(step).getByText("รหัส E_MACHINE_READ", { exact: false })).toBeTruthy();
    expect(within(step).getByText("ปิด Windows Terminal แล้วลองใหม่")).toBeTruthy();
    expect(within(step).queryByText("Something on this computer could not be read")).toBeNull();
    expect(within(step).queryByText("Close Windows Terminal and try again")).toBeNull();
    // What and how, in the core's English, wait behind ดูรายละเอียด.
    const what = within(step).getByText("Windows Terminal's settings: E_MACHINE_READ: it could not be read: access is denied");
    expect((what.closest("details") as HTMLDetailsElement).open).toBe(false);
    // Rolled back: the theme is still applied, so its bar stays.
    await settle();
    expect(bar()).not.toBeNull();
  });
});

describe("report a problem", () => {
  it("shows the diagnostics, copies them with a visible Copied, and opens GitHub Issues", async () => {
    const ipc = createMockIpc();
    const { user } = mount(ipc, { start: { name: "help", tab: "report" } });
    expect(await screen.findByText(/^CMD Studio diagnostics/)).toBeTruthy();
    expect(screen.queryByText(en["help.report.copied"])).toBeNull();
    // The status region is there before the copy, its text toggled (2c item 10).
    const regions = Array.from(document.querySelectorAll('[role="status"]'));
    await user.click(screen.getByRole("button", { name: "Copy details" }));
    const said = await screen.findByText(en["help.report.copied"]);
    expect(regions).toContain(said);
    expect(await navigator.clipboard.readText()).toMatch(/^CMD Studio diagnostics/);
    expect(ipc.state.calls).toContain("diagnosticsCopy");
    await user.click(screen.getByRole("button", { name: "Open GitHub Issues" }));
    await waitFor(() => expect(ipc.state.calls).toContain("openExternal:gitHubIssues"));
  });

  it("says when the details could not be copied, or GitHub Issues could not be opened", async () => {
    const ipc = createMockIpc();
    const { user } = mount(ipc, { start: { name: "help", tab: "report" } });
    await screen.findByText(/^CMD Studio diagnostics/);
    vi.spyOn(ipc, "diagnosticsCopy").mockRejectedValue(new Error("the log could not be read"));
    await user.click(screen.getByRole("button", { name: "Copy details" }));
    expect(await screen.findByText("The details could not be copied")).toBeTruthy();
    expect(screen.getByText("the log could not be read")).toBeTruthy();
    expect(screen.queryByText(en["help.report.copied"])).toBeNull();
    vi.spyOn(ipc, "openExternal").mockRejectedValue(ipcError("E_INTERNAL", "no browser is set up"));
    await user.click(screen.getByRole("button", { name: "Open GitHub Issues" }));
    expect(await screen.findByText("no browser is set up")).toBeTruthy();
  });
});

describe("the guide and the credits", () => {
  it("has the five paragraphs of the guide", async () => {
    const { user } = mount(createMockIpc());
    await openTab(user, "Short guide");
    for (const p of ["p1", "p2", "p3", "p4", "p5"] as const) {
      expect(screen.getByText(en[`help.guide.${p}`])).toBeTruthy();
    }
  });

  it("lists the bundled programs, the font and the color schemes with their licenses, and no SignPath line yet (Q6)", async () => {
    expect(BUNDLED.map((c) => [c.name, c.version, c.license])).toEqual([
      ["Clink", "1.9.34", "GPL-3.0"],
      ["Oh My Posh", "31.5.0", "MIT"],
      ["CaskaydiaCove Nerd Font", "Nerd Fonts 3.5.1", "SIL OFL 1.1"],
    ]);
    expect(SCHEMES.map((c) => c.name)).toEqual([
      "Tokyo Night",
      "Catppuccin Mocha, Catppuccin Latte",
      "Dracula",
      "Nord",
      "Gruvbox",
      "One Dark",
      "Solarized Light",
      "Terracotta, Retro green, Daylight, Paper",
    ]);
    expect(SCHEMES.every((c) => c.license === "MIT" && c.version === undefined)).toBe(true);
    const { user } = mount(createMockIpc());
    await openTab(user, "Credits and licenses");
    for (const c of [...BUNDLED, ...SCHEMES]) {
      const item = screen.getByText(c.name).closest("li") as HTMLElement;
      expect(within(item).getByText(c.copyright)).toBeTruthy();
      expect(within(item).getByText(c.source)).toBeTruthy();
      expect(item.textContent).toContain(c.license);
      if (c.version) expect(item.textContent).toContain(c.version);
    }
    expect(screen.queryByText(/SignPath/)).toBeNull();
  });
});

describe("Advanced", () => {
  it("shows the add-ons' versions, the data folder, the log and the generated files, read only", async () => {
    const ipc = createMockIpc();
    await applyIn(ipc, "Terracotta");
    const { user } = mount(ipc, { start: { name: "advanced" } });
    await screen.findByRole("heading", { level: 1, name: "Advanced" });
    const row = async (name: string) => within((await screen.findByRole("rowheader", { name })).closest("tr") as HTMLElement);
    expect((await row("CMD Studio")).getByText("0.1.0")).toBeTruthy();
    expect((await row("Oh My Posh")).getByText("31.5.0")).toBeTruthy();
    expect((await row("Clink")).getByText("1.9.34")).toBeTruthy();
    expect((await row("CaskaydiaCove NF (Nerd Fonts)")).getByText("3.5.1")).toBeTruthy();
    expect(screen.getByText("C:\\Users\\tester\\AppData\\Local\\ai.resincore.cmdstudio")).toBeTruthy();
    expect(screen.getByText("2026-10-16T12:00:00Z cmd=themes_list ms=3 ok")).toBeTruthy();
    // The generated files behind toggles, closed.
    for (const name of ["theme.omp.json", "init.lua", "banner.lua", "clink_settings"]) {
      expect((screen.getByText(name).closest("details") as HTMLDetailsElement).open).toBe(false);
    }
    expect(screen.getByText("clink.logo = none").closest("details")).toBeTruthy();
    expect(screen.queryAllByRole("textbox")).toEqual([]);
    await user.click(screen.getByRole("button", { name: "Open the data folder" }));
    await user.click(screen.getByRole("button", { name: "Open the log folder" }));
    await waitFor(() =>
      expect(ipc.state.calls.filter((c) => c.startsWith("openExternal:"))).toEqual(["openExternal:dataFolder", "openExternal:logsFolder"]),
    );
  });

  it("says a failed read with Check again, nothing generated before an apply, and a folder that did not open", async () => {
    const ipc = createMockIpc();
    vi.spyOn(ipc, "advancedRead").mockRejectedValueOnce(ipcError("E_MACHINE_READ", "logs\\cmdstudio.log: access is denied"));
    const { user } = mount(ipc, { start: { name: "advanced" } });
    expect(await screen.findByText("logs\\cmdstudio.log: access is denied")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Check again" }));
    expect(await screen.findByText(en["advanced.none"])).toBeTruthy();
    expect(screen.queryByText("logs\\cmdstudio.log: access is denied")).toBeNull();
    vi.spyOn(ipc, "openExternal").mockRejectedValue(ipcError("E_INTERNAL", "explorer.exe could not start"));
    await user.click(screen.getByRole("button", { name: "Open the log folder" }));
    expect(await screen.findByText("explorer.exe could not start")).toBeTruthy();
  });
});

describe("accessibility", () => {
  it("names every button of every Help tab, the fix dialog and Advanced, in English and in Thai", async () => {
    for (const language of ["en", "th"] as const) {
      const dict: Dictionary = language === "en" ? en : th;
      const ipc = createMockIpc({ otherClink: [OTHER_CLINK] });
      const applied = await applyIn(ipc, "Terracotta", true);
      const { user, view } = mount(ipc, { language, applied });
      const named = (where: string) => {
        for (const button of screen.getAllByRole("button")) {
          const name = button.getAttribute("aria-label") ?? button.textContent ?? "";
          expect(name.trim(), `${language}: a button without a name on ${where}`).not.toBe("");
        }
      };
      await screen.findByRole("button", { name: dict["help.doctor.fix.replaceOtherClink"] });
      for (const tab of TABS) {
        await openTab(user, dict[`help.tabs.${tab}`], dict["help.tabs.label"]);
        await settle();
        named(tab);
      }
      await openTab(user, dict["help.tabs.doctor"], dict["help.tabs.label"]);
      await user.click(await screen.findByRole("button", { name: dict["help.doctor.fix.replaceOtherClink"] }));
      const dialog = await screen.findByRole("dialog");
      await within(dialog).findByRole("button", { name: dict["apply.start"] });
      named("the fix dialog");
      await user.click(within(dialog).getByRole("button", { name: dict["a11y.closeDialog"] }));
      await user.click(within(screen.getByRole("navigation")).getByRole("button", { name: dict["nav.advanced"] }));
      await screen.findByRole("button", { name: dict["advanced.openData"] });
      named("Advanced");
      view.unmount();
    }
  });
});
