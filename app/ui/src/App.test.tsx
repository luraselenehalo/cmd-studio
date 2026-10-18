// The whole app against the mock IPC, through the final shell. The first run: English, Terracotta
// and every cmd window preselected, a theme applied in five real clicks with nothing typed (spec
// §1 criterion 1, §4.2); Thai chosen on page 1 is remembered and Skip lands on the Thai home page
// (Review Focus 2); page 2's four Windows Terminal states, the last read from the backend's flag;
// no Back while the apply runs (R7) and the apply said once; `wizardDone` saved when the apply
// commits (Q12). Home: the real checks with their time, the theme in use, the quick switch through
// the plan, the latest restore point, and a state.json that could not be read said as such.
// Settings keeps the founder's defaults and applies again through the plan. Help's fix really goes
// through the plan and "Undo everything" asks first (Review Focus 4). Every button on every page,
// tab and plan dialog has a name, in English and in Thai (Review Focus 5). Help's and Advanced's
// own tests are Task 1's (Help.test.tsx, ruling Q15).
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { AppSettings } from "./bindings/AppSettings";
import type { ApplyOutcome } from "./bindings/ApplyOutcome";
import type { ApplyRequest } from "./bindings/ApplyRequest";
import type { AppError } from "./bindings/AppError";
import type { PauseInfo } from "./bindings/PauseInfo";
import type { RevertOutcome } from "./bindings/RevertOutcome";
import type { SystemReport } from "./bindings/SystemReport";
import { App } from "./App";
import { Wizard } from "./screens/Wizard";
import { en } from "./i18n/en";
import { th } from "./i18n/th";
import type { Dictionary, Language } from "./i18n";
import { IpcProvider } from "./ipc/context";
import { createMockIpc, type MockOptions } from "./ipc/mock";
import { PRESETS } from "./ipc/presets";
import { IpcError } from "./ipc/types";

type MockIpc = ReturnType<typeof createMockIpc>;
type User = ReturnType<typeof userEvent.setup>;

const OTHER_CLINK = '"C:\\clink\\clink_x64.exe" inject --autorun';
const RECOMMENDED = ["Terracotta", "Tokyo Night", "Catppuccin Mocha", "Dracula", "Catppuccin Latte", "Paper"];

/** Mounts the app over a mock, after `seed` has set the mock up (a theme applied, a spy). */
async function mount(options: MockOptions = {}, seed?: (ipc: MockIpc) => Promise<void> | void) {
  const ipc = createMockIpc(options);
  if (seed) await seed(ipc);
  const user = userEvent.setup();
  const view = render(
    <IpcProvider ipc={ipc}>
      <App />
    </IpcProvider>,
  );
  return { ipc, user, view };
}

/** The app after the wizard, on its home page. */
async function home(options: MockOptions = {}, seed?: (ipc: MockIpc) => Promise<void> | void) {
  const mounted = await mount({ ...options, settings: { wizardDone: true, ...options.settings } }, seed);
  await screen.findByRole("heading", { name: options.settings?.language === "th" ? th["home.title"] : "Home" });
  return mounted;
}

function presetId(name: string): string {
  const found = PRESETS.find((p) => p.name === name);
  if (!found) throw new Error(`no preset ${name}`);
  return found.id;
}

function ipcError(code: string, details: string): IpcError {
  return new IpcError({ code, messageTh: `(${code})`, messageEn: `(${code})`, details });
}

/** Applies a preset in the mock before the app mounts, as an earlier run would have. */
async function applyIn(ipc: MockIpc, name: string, replaceOtherClink = false): Promise<void> {
  const request: ApplyRequest = { themeId: presetId(name), target: "all", setWtDefaultProfile: true, replaceOtherClink };
  const planned = await ipc.planApply(request);
  if (planned.kind !== "ready") throw new Error(`no plan for ${name}`);
  const outcome = await ipc.apply(request, planned.plan.digest, () => {});
  if (outcome.status !== "committed") throw new Error(`${name} was not applied`);
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

const settle = () =>
  act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });

/** Clicks as the user does, counting each one: criterion 1 counts clicks. */
function counting(user: User) {
  let count = 0;
  return {
    click: async (element: Element) => {
      count += 1;
      await user.click(element);
    },
    get count() {
      return count;
    },
  };
}

/** Nothing on the page asks for typing: no text field, number field or combo box. */
function expectNothingToType() {
  expect(screen.queryAllByRole("textbox")).toEqual([]);
  expect(screen.queryAllByRole("spinbutton")).toEqual([]);
  expect(screen.queryAllByRole("combobox")).toEqual([]);
}

/** The wizard from page 1 to page 5's plan with every default: Next four times. */
async function toPage5(user: User, dictionary: Dictionary = en) {
  await user.click(await screen.findByRole("button", { name: dictionary["common.next"] }));
  await screen.findByText(dictionary["wizard.step"].replace("{n}", "2"));
  await user.click(screen.getByRole("button", { name: dictionary["common.next"] }));
  await screen.findByRole("heading", { name: dictionary["wizard.choose.title"] });
  await user.click(screen.getByRole("button", { name: dictionary["common.next"] }));
  await screen.findByRole("heading", { name: dictionary["wizard.where.title"] });
  await user.click(screen.getByRole("button", { name: dictionary["common.next"] }));
  await screen.findByRole("heading", { name: dictionary["wizard.confirm.title"] });
}

/** Opens a sidebar page by its name. */
async function open(user: User, name: string) {
  await user.click(within(screen.getByRole("navigation")).getByRole("button", { name }));
}

describe("the first run", () => {
  it("opens in English and applies Terracotta in five clicks with nothing typed (criterion 1)", async () => {
    const { ipc, user, view } = await mount();
    const restore = vi.spyOn(ipc, "snapshotRestore");
    const clicks = counting(user);
    expect(await screen.findByRole("heading", { name: "Make your cmd window beautiful and easy to read" })).toBeTruthy();
    expect(view.container.querySelector("div[lang]")?.getAttribute("lang")).toBe("en");
    expect(screen.getByRole("button", { name: "English", pressed: true })).toBeTruthy();
    expectNothingToType();
    await clicks.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByText("Windows build 26100");
    expectNothingToType();
    await clicks.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByRole("heading", { name: "Choose a theme" });
    expect(await screen.findByRole("button", { name: "Terracotta", pressed: true })).toBeTruthy();
    expectNothingToType();
    await clicks.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByRole("heading", { name: "Where should the theme go?" });
    expect((screen.getByLabelText(/^Every cmd window/) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("Make Windows Terminal open cmd by default") as HTMLInputElement).checked).toBe(true);
    expectNothingToType();
    await clicks.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByRole("heading", { name: "Ready to apply" });
    await screen.findByText("What will change");
    expect(screen.getByText("We save a restore point first. You can undo at any time.")).toBeTruthy();
    expectNothingToType();
    await clicks.click(screen.getByRole("button", { name: "Apply" }));
    const done = await screen.findByRole("heading", { name: "Done" });
    expectNothingToType();
    // Five clicks from the first screen to an applied theme, the apply asked for once.
    expect(clicks.count).toBe(5);
    expect(ipc.state.calls.filter((c) => c === "apply")).toHaveLength(1);
    expect(ipc.state.applied?.theme.name).toBe("Terracotta");
    expect(ipc.state.applied?.request).toEqual({ themeId: presetId("Terracotta"), target: "all", setWtDefaultProfile: true, replaceOtherClink: false });
    // Saved as the apply committed (Q12): closing the app on page 6 does not bring the wizard back.
    expect(ipc.state.settings).toMatchObject({ wizardDone: true, language: "en", target: "all", setWtDefaultProfile: true });
    await waitFor(() => expect(document.activeElement).toBe(done));
    expect(await screen.findByRole("img", { name: "Preview of the cmd window" })).toBeTruthy();
    // Home, with its heading focused and the after-apply bar, whose Undo goes back to the
    // wizard's own restore point (R3).
    await user.click(screen.getByRole("button", { name: "Go to the home page" }));
    const heading = await screen.findByRole("heading", { name: "Home" });
    await waitFor(() => expect(document.activeElement).toBe(heading));
    expect(screen.getByText(/Theme "Terracotta" applied/)).toBeTruthy();
    const point = ipc.state.points.find((p) => p.kind === "apply")!.id;
    await user.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(ipc.state.applied).toBeNull());
    expect(restore).toHaveBeenCalledWith(point);
    expect(await screen.findByText("Undone: everything is as it was before.")).toBeTruthy();
  });

  it("switches to Thai on page 1 at once, remembers it, and Skip lands on the Thai home page", async () => {
    const { ipc, user, view } = await mount();
    const thai = await screen.findByRole("button", { name: "ไทย" });
    // Each language's name is marked as that language, for a screen reader.
    expect(thai.getAttribute("lang")).toBe("th");
    expect(screen.getByRole("button", { name: "English" }).getAttribute("lang")).toBe("en");
    await user.click(thai);
    expect(await screen.findByRole("heading", { name: th["wizard.welcome.title"] })).toBeTruthy();
    expect(view.container.querySelector("div[lang]")?.getAttribute("lang")).toBe("th");
    expect(screen.getByRole("button", { name: "ไทย", pressed: true })).toBeTruthy();
    await waitFor(() => expect(ipc.state.settings.language).toBe("th"));
    expect(ipc.state.settings.wizardDone).toBe(false);
    // The label is the dictionary's (ruling Q1).
    await user.click(screen.getByRole("button", { name: th["wizard.skip"] }));
    expect(await screen.findByRole("heading", { name: th["home.title"] })).toBeTruthy();
    expect(ipc.state.settings).toMatchObject({ wizardDone: true, language: "th" });
    expect(ipc.state.calls).not.toContain("apply");
  });

  it("goes home without changing anything when skipped on page 2, 3 or 4 (Q13)", async () => {
    for (const page of [2, 3, 4]) {
      const { ipc, user, view } = await mount();
      for (let n = 1; n < page; n++) {
        await user.click(await screen.findByRole("button", { name: "Next" }));
      }
      await screen.findByText(`Step ${page} of 6`);
      await user.click(screen.getByRole("button", { name: "Skip, set up myself" }));
      expect(await screen.findByRole("heading", { name: "Home" })).toBeTruthy();
      expect(ipc.state.settings.wizardDone, `page ${page}`).toBe(true);
      expect(ipc.state.calls.filter((c) => c === "planApply" || c === "apply"), `page ${page}`).toEqual([]);
      view.unmount();
    }
  });

  it("asks before replacing another cmd add-on, lists its commands, and asks again after Back", async () => {
    const { ipc, user } = await mount({ otherClink: [OTHER_CLINK] });
    await user.click(await screen.findByRole("button", { name: "Next" }));
    expect(await screen.findByText("Another cmd add-on is already set up; we will ask before replacing it")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Next" }));
    await user.click(await screen.findByRole("button", { name: "Next" }));
    await user.click(await screen.findByRole("button", { name: "Next" }));
    expect(await screen.findByRole("heading", { name: "Another cmd add-on is already set up" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Use CMD Studio's instead" }));
    await screen.findByText("What will change");
    expect(screen.getByText("Turning off the other cmd add-on takes these commands out of cmd's start-up:")).toBeTruthy();
    expect(screen.getByText(OTHER_CLINK)).toBeTruthy();
    // The yes holds for that plan only: back and on again, it is asked anew.
    await user.click(screen.getByRole("button", { name: "Back" }));
    await user.click(await screen.findByRole("button", { name: "Next" }));
    expect(await screen.findByRole("heading", { name: "Another cmd add-on is already set up" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Use CMD Studio's instead" }));
    await screen.findByText("What will change");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await screen.findByRole("heading", { name: "Done" });
    expect(ipc.state.applied?.request.replaceOtherClink).toBe(true);
  });

  it("shows a failed apply as put back, with a way to copy the details, and Back once it ended", async () => {
    const { ipc, user } = await mount({ failApplyWith: "E_VERIFY_OMP" });
    await toPage5(user);
    await screen.findByText("What will change");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByText("The theme could not be applied. Everything was put back as it was.")).toBeTruthy();
    expect(screen.getByText("Code E_VERIFY_OMP", { exact: false })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Copy details for a problem report" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Back" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Done" })).toBeNull();
    expect(ipc.state.applied).toBeNull();
    // Nothing was applied, so the wizard is not done.
    expect(ipc.state.settings.wizardDone).toBe(false);
  });

  it("has no Back while it applies, and says the apply once, in one live region (R7)", async () => {
    const { ipc, user } = await mount();
    const release = holdApply(ipc);
    await toPage5(user);
    await screen.findByText("What will change");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await screen.findByText("Applying…");
    expect(screen.queryByRole("button", { name: "Back" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Next" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Skip, set up myself" })).toBeNull();
    await act(async () => {
      release();
    });
    await screen.findByRole("heading", { name: "Done" });
    const said = () => screen.queryAllByRole("status").filter((el) => /Theme "Terracotta" applied/.test(el.textContent ?? ""));
    expect(said()).toHaveLength(1);
    // The step counter is no second announcement.
    expect(screen.getByText("Step 6 of 6").closest('[aria-live], [role="status"]')).toBeNull();
    // Nor is the bar on Home (2c fix 12).
    await user.click(screen.getByRole("button", { name: "Go to the home page" }));
    await screen.findByRole("heading", { name: "Home" });
    expect(screen.getByText(/Theme "Terracotta" applied/)).toBeTruthy();
    expect(said()).toHaveLength(0);
  });

  it("says on page 2 what Windows Terminal gets: colors, set up by the apply, the Store, too old, or not known", async () => {
    const texts = {
      present: "Windows Terminal is installed: your cmd windows get colors",
      neverOpened: "Windows Terminal is installed and was never opened: CMD Studio sets it up when the theme is applied",
      missing: en["wizard.check.wtMissing"],
      tooOld: en["wizard.check.wtTooOld"],
      unknown: en["wizard.check.wtUnknown"],
    };
    const cases: { options: MockOptions; says: keyof typeof texts; store: boolean }[] = [
      { options: {}, says: "present", store: false },
      { options: { wtNeverOpened: true }, says: "neverOpened", store: false },
      { options: { noWindowsTerminal: true }, says: "missing", store: true },
      { options: { noWindowsTerminal: true, build: 18363, storeAvailable: false }, says: "tooOld", store: false },
      { options: { noWindowsTerminal: true, build: null, storeAvailable: false }, says: "unknown", store: false },
    ];
    for (const c of cases) {
      const { ipc, user, view } = await mount(c.options);
      await user.click(await screen.findByRole("button", { name: "Next" }));
      expect(await screen.findByText(texts[c.says]), c.says).toBeTruthy();
      for (const [state, text] of Object.entries(texts)) {
        if (state !== c.says) expect(screen.queryByText(text), `${c.says}: ${state}`).toBeNull();
      }
      expect(screen.queryByRole("button", { name: "Install Windows Terminal" }) !== null, c.says).toBe(c.store);
      // A Terminal never opened needs nothing of the user's (founder 2026-10-17).
      expect(screen.queryByRole("button", { name: /Open Windows Terminal/ }), c.says).toBeNull();
      expect(screen.queryByRole("button", { name: "Check again" }), c.says).toBeNull();
      if (c.store) {
        await user.click(screen.getByRole("button", { name: "Install Windows Terminal" }));
        await waitFor(() => expect(ipc.state.calls).toContain("openExternal:windowsTerminalStore"));
      }
      view.unmount();
    }
    // The build that could not be read is said as well.
    const { user } = await mount({ noWindowsTerminal: true, build: null, storeAvailable: false });
    await user.click(await screen.findByRole("button", { name: "Next" }));
    expect(await screen.findByText("Windows version could not be read; the check goes on without it")).toBeTruthy();
  });

  it("takes a Terminal set up by the apply from the backend's flag, never from installed without settings", async () => {
    // An unpackaged folder left behind reads as installed with no settings.json, and no apply
    // sets it up: it is no Terminal, so page 2 offers the Store.
    const { ipc, user } = await mount({}, (m) => {
      const detect = m.systemDetect.bind(m);
      vi.spyOn(m, "systemDetect").mockImplementation(async (): Promise<SystemReport> => ({
        ...(await detect()),
        windowsTerminal: [{ kind: "unpackaged", installed: true, hasSettings: false, setUpByApply: false }],
        wtInstalled: true,
      }));
      vi.spyOn(m, "openExternal").mockRejectedValue(ipcError("E_INTERNAL", "ShellExecuteW failed: the Store is not there"));
    });
    await user.click(await screen.findByRole("button", { name: "Next" }));
    expect(await screen.findByText(en["wizard.check.wtMissing"])).toBeTruthy();
    expect(screen.queryByText(/was never opened/)).toBeNull();
    expect(screen.queryByText("Windows Terminal is installed: your cmd windows get colors")).toBeNull();
    // A Store that did not open is said (spec §5.9).
    await user.click(screen.getByRole("button", { name: "Install Windows Terminal" }));
    expect(await screen.findByText("ShellExecuteW failed: the Store is not there")).toBeTruthy();
    expect(ipc.openExternal).toHaveBeenCalledWith("windowsTerminalStore");
  });

  it("promises the colors on page 5 for a Terminal never opened: the plan's set-up line comes first", async () => {
    const { ipc, user } = await mount({ wtNeverOpened: true });
    await toPage5(user);
    expect(await screen.findByText("Set up Windows Terminal's settings so cmd gets this theme")).toBeTruthy();
    expect(screen.getByText("Colors and font of cmd in Windows Terminal")).toBeTruthy();
    expect(screen.queryByText(en["apply.warning.noWindowsTerminal"])).toBeNull();
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await screen.findByRole("heading", { name: "Done" });
    expect(ipc.state.applied?.theme.name).toBe("Terracotta");
  });

  it("lists page 3 from one listing, the six recommended first, and previews the chosen theme on page 6", async () => {
    const { ipc, user } = await mount({}, (m) => {
      vi.spyOn(m, "themeGet");
      vi.spyOn(m, "previewRender");
    });
    await user.click(await screen.findByRole("button", { name: "Next" }));
    await user.click(await screen.findByRole("button", { name: "Next" }));
    await screen.findByRole("button", { name: "Terracotta", pressed: true });
    const cards = () => Array.from(document.querySelectorAll(".theme-card button[aria-pressed]"), (b) => b.textContent);
    expect(cards()).toEqual(RECOMMENDED);
    // The cards come from the listing's colors (R10): no theme_get per card.
    expect(ipc.themeGet).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "See all 12 themes" }));
    expect(cards()).toHaveLength(12);
    await user.click(screen.getByRole("button", { name: "Gruvbox" }));
    expect(screen.getByRole("button", { name: "Gruvbox", pressed: true })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Terracotta", pressed: false })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Show the recommended 6" }));
    expect(cards()).toEqual(RECOMMENDED);
    await user.click(screen.getByRole("button", { name: "Next" }));
    await user.click(await screen.findByRole("button", { name: "Next" }));
    await screen.findByText("What will change");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await screen.findByRole("heading", { name: "Done" });
    expect(ipc.state.applied?.theme.name).toBe("Gruvbox");
    // One theme_get, for page 6's preview, drawn with this computer's sample (R1).
    await waitFor(() => expect(ipc.previewRender).toHaveBeenCalled());
    expect(vi.mocked(ipc.themeGet).mock.calls.map((call) => call[0])).toEqual([presetId("Gruvbox")]);
    expect(vi.mocked(ipc.previewRender).mock.calls.at(-1)?.[1].user).toBe("tester");
  });

  it("says what could not be read, with a way to try again, and goes on", async () => {
    const { user } = await mount({}, (m) => {
      vi.spyOn(m, "systemDetect").mockRejectedValueOnce(ipcError("E_MACHINE_READ", "the registry could not be read"));
      vi.spyOn(m, "themesList").mockRejectedValueOnce(ipcError("E_DATA_READ", "the themes folder could not be listed"));
    });
    await user.click(await screen.findByRole("button", { name: "Next" }));
    expect(await screen.findByText("the registry could not be read")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Check again" }));
    expect(await screen.findByText("Windows build 26100")).toBeTruthy();
    expect(screen.queryByText("the registry could not be read")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(await screen.findByText("the themes folder could not be listed")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Next" }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("button", { name: "Terracotta", pressed: true })).toBeTruthy();
    expect((screen.getByRole("button", { name: "Next" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("says a preview that could not be drawn and a cmd that did not open, and still goes home", async () => {
    const { ipc, user } = await mount({}, (m) => {
      vi.spyOn(m, "themeGet").mockRejectedValue(ipcError("E_THEME_INVALID", "the theme file is gone"));
      vi.spyOn(m, "openCmd").mockRejectedValue(ipcError("E_INTERNAL", "CreateProcessW failed: cmd.exe"));
    });
    await toPage5(user);
    await screen.findByText("What will change");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await screen.findByRole("heading", { name: "Done" });
    expect(await screen.findByText("the theme file is gone")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Try cmd" }));
    expect(await screen.findByText("CreateProcessW failed: cmd.exe")).toBeTruthy();
    expect(ipc.openCmd).toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Go to the home page" }));
    expect(await screen.findByRole("heading", { name: "Home" })).toBeTruthy();
  });

  it("goes home when the choice could not be saved, and says the setup comes back next time (I1)", async () => {
    let refuse = true;
    const { ipc, user } = await mount({}, (m) => {
      const set = m.settingsSet.bind(m);
      vi.spyOn(m, "settingsSet").mockImplementation(async (settings) => {
        if (refuse) throw ipcError("E_DATA_WRITE", "settings.json is read-only");
        return set(settings);
      });
    });
    await user.click(await screen.findByRole("button", { name: "ไทย" }));
    // The UI switched; the error says the choice was not kept.
    expect(await screen.findByRole("heading", { name: th["wizard.welcome.title"] })).toBeTruthy();
    expect(await screen.findByText("settings.json is read-only")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: th["wizard.skip"] }));
    // Home all the same, in the chosen language, saying what the failed save means.
    expect(await screen.findByRole("heading", { name: th["home.title"] })).toBeTruthy();
    expect(screen.getByText(th["wizard.notSaved"])).toBeTruthy();
    expect(screen.getByText("settings.json is read-only")).toBeTruthy();
    expect(ipc.state.settings.wizardDone).toBe(false);
    // The notice stays on the other pages until a save goes through, and that save keeps the
    // wizard done and the language chosen.
    await open(user, th["nav.settings"]);
    await screen.findByRole("heading", { name: th["settings.title"] });
    expect(screen.getByText(th["wizard.notSaved"])).toBeTruthy();
    refuse = false;
    await user.click(screen.getByLabelText(th["settings.updates.notify"]));
    await waitFor(() => expect(ipc.state.settings).toMatchObject({ updates: "notify", wizardDone: true, language: "th" }));
    expect(screen.queryByText(th["wizard.notSaved"])).toBeNull();
  });

  it("goes home after an apply whose settings could not be saved, with the bar's Undo there (I1)", async () => {
    const { ipc, user } = await mount({}, (m) => {
      vi.spyOn(m, "settingsSet").mockRejectedValue(ipcError("E_DATA_WRITE", "settings.json is read-only"));
    });
    await toPage5(user);
    await screen.findByText("What will change");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await screen.findByRole("heading", { name: "Done" });
    expect(await screen.findByText("settings.json is read-only")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Go to the home page" }));
    expect(await screen.findByRole("heading", { name: "Home" })).toBeTruthy();
    expect(screen.getByText("CMD Studio could not save your settings, so this setup shows again next time")).toBeTruthy();
    // Page 5's "You can undo at any time" holds: the bar's Undo is there.
    expect(screen.getByText(/Theme "Terracotta" applied/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(ipc.state.applied).toBeNull());
  });

  it("words page 6 by where the theme went (m1)", async () => {
    for (const target of ["all", "wt"] as const) {
      const { user, view } = await mount();
      await user.click(await screen.findByRole("button", { name: "Next" }));
      await user.click(await screen.findByRole("button", { name: "Next" }));
      await user.click(await screen.findByRole("button", { name: "Next" }));
      await screen.findByRole("heading", { name: "Where should the theme go?" });
      if (target === "wt") await user.click(screen.getByLabelText(/^Only in Windows Terminal/));
      await user.click(screen.getByRole("button", { name: "Next" }));
      await screen.findByText("What will change");
      await user.click(screen.getByRole("button", { name: "Apply" }));
      await screen.findByRole("heading", { name: "Done" });
      const [says, not] = target === "wt" ? [en["wizard.done.textWt"], en["wizard.done.text"]] : [en["wizard.done.text"], en["wizard.done.textWt"]];
      expect(screen.getByText(says), target).toBeTruthy();
      expect(screen.queryByText(not), target).toBeNull();
      if (target === "wt") expect(screen.queryByText(/Open a new cmd window/)).toBeNull();
      view.unmount();
    }
  });

  it("names page 2 by the finished check once its results are there, in both languages (V6)", async () => {
    let release: () => void = () => {};
    const { user, view } = await mount({}, (m) => {
      const detect = m.systemDetect.bind(m);
      vi.spyOn(m, "systemDetect").mockImplementation(async () => {
        await new Promise<void>((resolve) => {
          release = resolve;
        });
        return detect();
      });
    });
    await user.click(await screen.findByRole("button", { name: "Next" }));
    const heading = await screen.findByRole("heading", { name: "Checking this computer" });
    await waitFor(() => expect(document.activeElement).toBe(heading));
    await act(async () => {
      release();
    });
    await screen.findByText("Windows build 26100");
    expect(screen.getByRole("heading", { name: "Your computer" })).toBe(heading);
    expect(screen.queryByRole("heading", { name: "Checking this computer" })).toBeNull();
    // The heading is the same element, so it keeps the focus it took.
    expect(document.activeElement).toBe(heading);
    view.unmount();
    const thai = await mount({ settings: { language: "th" } });
    await thai.user.click(await screen.findByRole("button", { name: th["common.next"] }));
    await screen.findByText(th["wizard.check.windows"].replace("{build}", "26100"));
    expect(screen.getByRole("heading", { name: "เครื่องของคุณ" })).toBeTruthy();
  });
});

describe("the home page", () => {
  it("shows the checks with their time, the theme in use with its preview, and the latest restore point", async () => {
    const { user } = await home({}, async (m) => {
      await applyIn(m, "Terracotta");
      vi.spyOn(m, "previewRender");
    });
    // The mock's checks after an apply: D3 says no cmd window has loaded the theme yet.
    expect(await screen.findByText("1 thing(s) to look at")).toBeTruthy();
    expect(screen.getByText(/^Checked at /)).toBeTruthy();
    const card = (title: string) => screen.getByRole("heading", { name: title }).closest(".card") as HTMLElement;
    const active = card("Theme in use");
    expect(within(active).getByText("Terracotta")).toBeTruthy();
    expect(await within(active).findByRole("img", { name: "Preview of the cmd window" })).toBeTruthy();
    expect(within(card("Latest restore point")).getByText(/^Terracotta · /)).toBeTruthy();
    // The theme in use is not offered again (V5): with no other one used, the quick switch says
    // where its themes come from.
    expect(within(card("Switch quickly")).queryAllByRole("button")).toEqual([]);
    expect(within(card("Switch quickly")).getByText("Themes you apply show up here.")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "See the checks" }));
    await screen.findByRole("heading", { name: "Help" });
    expect(screen.getByRole("button", { name: "Health check", pressed: true })).toBeTruthy();
    await open(user, "Home");
    await user.click(await screen.findByRole("button", { name: "See all restore points" }));
    expect(await screen.findByRole("button", { name: "Restore points", pressed: true })).toBeTruthy();
  });

  it("draws the preview with this computer's sample, and says when it could not be read", async () => {
    const { ipc, view } = await home({}, async (m) => {
      await applyIn(m, "Terracotta");
      vi.spyOn(m, "previewRender");
    });
    await waitFor(() => expect(ipc.previewRender).toHaveBeenCalled());
    expect(vi.mocked(ipc.previewRender).mock.calls.at(-1)?.[1]).toMatchObject({ user: "tester", host: "TESTPC", cwd: "C:\\Users\\tester\\projects\\cmd-studio" });
    expect(screen.queryByText("This computer's details could not be read, so the preview uses sample values.")).toBeNull();
    view.unmount();
    const failed = await mount({ settings: { wizardDone: true } }, async (m) => {
      await applyIn(m, "Terracotta");
      vi.spyOn(m, "systemDetect").mockRejectedValue(ipcError("E_MACHINE_READ", "the registry could not be read"));
      vi.spyOn(m, "previewRender");
    });
    expect(await screen.findByText("This computer's details could not be read, so the preview uses sample values.")).toBeTruthy();
    await waitFor(() => expect(failed.ipc.previewRender).toHaveBeenCalled());
    expect(vi.mocked(failed.ipc.previewRender).mock.calls.at(-1)?.[1].user).toBe("you");
  });

  it("says when no restore point can be gone back to, rather than that there is none (m2)", async () => {
    await home({}, async (m) => {
      await applyIn(m, "Terracotta");
      // Undo everything leaves points that can no longer be restored.
      await m.revertAll();
    });
    const latest = screen.getByRole("heading", { name: "Latest restore point" }).closest(".card") as HTMLElement;
    expect(await within(latest).findByText(en["home.restore.noneRestorable"])).toBeTruthy();
    expect(within(latest).queryByText(en["home.restore.none"])).toBeNull();
    expect(within(latest).queryByText(/·/)).toBeNull();
  });

  it("says that in Thai as Help words going back to a point (Task 2 re-review)", async () => {
    await home({ settings: { language: "th" } }, async (m) => {
      await applyIn(m, "Terracotta");
      await m.revertAll();
    });
    const latest = screen.getByRole("heading", { name: th["home.restore.title"] }).closest(".card") as HTMLElement;
    expect(await within(latest).findByText("ตอนนี้ไม่มีจุดย้อนกลับที่กลับไปได้")).toBeTruthy();
    // Help's verb for going back to a point is the same.
    expect(th["help.restore.restore"]).toBe("กลับไปที่จุดนี้");
    expect(th["home.restore.noneRestorable"]).toContain("กลับไป");
  });

  it("needs the wizard's onUnsaved: without it a failed save would be swallowed (Task 2 re-review)", () => {
    // A type test: `npm run typecheck` fails on an unused directive, so it fails if the prop is optional.
    // @ts-expect-error onUnsaved is required
    const without = <Wizard />;
    expect(without.type).toBe(Wizard);
  });

  it("says when state.json could not be read, never that no theme is applied", async () => {
    await home({}, (m) => {
      const list = m.themesList.bind(m);
      vi.spyOn(m, "themesList").mockImplementation(async () => ({
        ...(await list()),
        applied: null,
        stateProblem: { code: "E_STATE_INVALID", messageTh: "(E_STATE_INVALID)", messageEn: "state.json can't be used", details: "line 1: expected value" },
      }));
    });
    expect(await screen.findByText("Which theme is in use could not be read")).toBeTruthy();
    expect(screen.getByText("state.json can't be used")).toBeTruthy();
    expect(screen.queryByText("No theme is applied yet.")).toBeNull();
  });

  it("loads each part on its own: what failed is said, and the rest is still there", async () => {
    await home({}, (m) => {
      vi.spyOn(m, "doctorRun").mockRejectedValue(ipcError("E_INTERNAL", "the checks could not run"));
      vi.spyOn(m, "snapshotsList").mockRejectedValue(ipcError("E_SNAPSHOT_READ", "the restore points could not be listed"));
    });
    expect(await screen.findByText("the checks could not run")).toBeTruthy();
    expect(await screen.findByText("the restore points could not be listed")).toBeTruthy();
    expect(screen.queryByText("Checking…")).toBeNull();
    expect(await screen.findByText("No theme is applied yet.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Choose a theme" })).toBeTruthy();
  });

  it("switches quickly to a theme used before through the plan, with no Close while it applies (R7)", async () => {
    const { ipc, user } = await home({}, async (m) => {
      await applyIn(m, "Terracotta");
      await applyIn(m, "Dracula");
      // An apply that rolled back was never used: it is not offered (a24).
      m.state.points.unshift({ id: "1791694709999-rolled", kind: "apply", createdMs: 1791694709999, theme: { id: presetId("Paper"), name: "Paper" }, target: "all", summary: [], undoes: null, undoneBy: null, restorable: false });
    });
    // The theme in use (Dracula) is left out (V5), and so is the apply that rolled back.
    expect(await screen.findByRole("button", { name: "Use Terracotta" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Use Dracula" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Use Paper" })).toBeNull();
    expect(screen.queryByText("Themes you apply show up here.")).toBeNull();
    // The latest restore point is the newest that can be gone back to, not the rolled-back one (m2).
    const latest = screen.getByRole("heading", { name: "Latest restore point" }).closest(".card") as HTMLElement;
    expect(within(latest).getByText(/^Dracula · /)).toBeTruthy();
    expect(within(latest).queryByText(/^Paper · /)).toBeNull();
    const release = holdApply(ipc);
    await user.click(screen.getByRole("button", { name: "Use Terracotta" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(await within(dialog).findByRole("button", { name: "Apply" }));
    await within(dialog).findByText("Applying…");
    expect(within(dialog).queryByRole("button", { name: "Close" })).toBeNull();
    await act(async () => {
      release();
    });
    // The dialog stays open on its outcome until it is closed.
    expect(await within(dialog).findByText(/Theme "Terracotta" applied/)).toBeTruthy();
    expect(ipc.state.applied?.request).toEqual({ themeId: presetId("Terracotta"), target: "all", setWtDefaultProfile: true, replaceOtherClink: false });
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByText(/Theme "Terracotta" applied/)).toBeTruthy();
  });

  it("customizes the theme in use into a copy, changes theme, and says a cmd that did not open", async () => {
    const { ipc, user } = await home({}, async (m) => {
      await applyIn(m, "Terracotta");
      vi.spyOn(m, "openCmd").mockRejectedValue(ipcError("E_INTERNAL", "CreateProcessW failed: cmd.exe"));
    });
    await user.click(await screen.findByRole("button", { name: "Open cmd" }));
    expect(await screen.findByText("CreateProcessW failed: cmd.exe")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Change theme" }));
    await screen.findByRole("heading", { name: "Themes" });
    await open(user, "Home");
    await user.click(await screen.findByRole("button", { name: "Customize" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    expect(ipc.state.mine.map((t) => t.name)).toEqual(["My Terracotta"]);
    expect((screen.getByLabelText("Theme name") as HTMLInputElement).value).toBe("My Terracotta");
    // The editor's modes are toggle buttons in their group (item 9), apart from the sidebar's Advanced.
    await user.click(within(screen.getByRole("group", { name: "Color mode" })).getByRole("button", { name: "Advanced" }));
    expect(screen.getByLabelText("Frame")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved.")).toBeTruthy();
  });
});

describe("Settings", () => {
  it("keeps the defaults: English, every window, automatic updates; a change is saved and said", async () => {
    const { ipc, user } = await home();
    await open(user, "Settings");
    await screen.findByRole("heading", { name: "Settings" });
    expect((screen.getByLabelText("English") as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText(/^Every cmd window/) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("Make Windows Terminal open cmd by default") as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("Automatic (recommended)") as HTMLInputElement).checked).toBe(true);
    expect(screen.getByText(/checked each time it starts/)).toBeTruthy();
    // "Saved." comes in a status region that was there before (2c item 10).
    const status = screen.getAllByRole("status").find((el) => el.textContent === "");
    expect(status).toBeTruthy();
    await user.click(screen.getByLabelText("Tell me first"));
    await waitFor(() => expect(ipc.state.settings.updates).toBe("notify"));
    expect(status?.textContent).toBe("Saved.");
    await user.click(screen.getByLabelText("ไทย"));
    expect(await screen.findByRole("heading", { name: "ตั้งค่า" })).toBeTruthy();
    expect(ipc.state.settings.language).toBe("th");
    expect(screen.getByLabelText("English").closest("label")?.querySelector('[lang="en"]')).toBeTruthy();
  });

  it("offers to apply the theme again only while one is applied, through the plan, with no Close while it applies (R7)", async () => {
    const first = await home();
    await open(first.user, "Settings");
    await screen.findByRole("heading", { name: "Settings" });
    await settle();
    expect(screen.queryByRole("button", { name: "Apply the current theme again with this choice" })).toBeNull();
    first.view.unmount();

    const { ipc, user } = await home({}, (m) => applyIn(m, "Terracotta"));
    await open(user, "Settings");
    await user.click(await screen.findByLabelText(/^Only in Windows Terminal/));
    await waitFor(() => expect(ipc.state.settings.target).toBe("wt"));
    const release = holdApply(ipc);
    await user.click(await screen.findByRole("button", { name: "Apply the current theme again with this choice" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(await within(dialog).findByRole("button", { name: "Apply" }));
    await within(dialog).findByText("Applying…");
    expect(within(dialog).queryByRole("button", { name: "Close" })).toBeNull();
    await act(async () => {
      release();
    });
    expect(await within(dialog).findByText(/open a new cmd tab in Windows Terminal/)).toBeTruthy();
    expect(ipc.state.applied?.request).toMatchObject({ themeId: presetId("Terracotta"), target: "wt", replaceOtherClink: false });
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    // The bar is worded by the request's own target (2c item 12).
    expect(await screen.findByText(/Theme "Terracotta" applied · open a new cmd tab in Windows Terminal/)).toBeTruthy();
  });
});

// The final review (A): when the start read of settings.json failed, the shell holds the defaults,
// and the first save used to write them over a file that was only unreadable for a moment (an
// antivirus held it). Approach (a): the first save reads the file again; if that works the change
// is put on top of what was read, if not nothing is written.
describe("a settings file that could not be read at the start", () => {
  const HELD = "settings.json is held by a scan";

  it("saves a Settings change on top of what the file holds, never the defaults over it", async () => {
    const onDisk: AppSettings = { language: "th", target: "wt", setWtDefaultProfile: true, updates: "auto", wizardDone: true };
    const { ipc, user } = await mount({}, (m) => {
      vi.spyOn(m, "settingsGet")
        .mockRejectedValueOnce(ipcError("E_MACHINE_READ", HELD))
        .mockResolvedValueOnce({ settings: onDisk, resetFrom: null });
    });
    // Home, in English, with the problem said: the wizard is not shown over settings not read.
    expect(await screen.findByRole("heading", { name: "Home" })).toBeTruthy();
    expect(screen.getByText(HELD)).toBeTruthy();
    await open(user, "Settings");
    await user.click(await screen.findByLabelText("Tell me first"));
    await waitFor(() => expect(ipc.state.calls).toContain("settingsSet"));
    expect(ipc.state.calls.filter((c) => c === "settingsSet")).toHaveLength(1);
    expect(ipc.state.settings).toEqual({ ...onDisk, updates: "notify" });
    // What the file holds is on screen now: Thai, and the problem is over.
    expect(await screen.findByRole("heading", { name: th["settings.title"] })).toBeTruthy();
    expect(screen.queryByText(HELD)).toBeNull();
    expect((screen.getByLabelText(th["settings.updates.notify"]) as HTMLInputElement).checked).toBe(true);
  });

  it("writes nothing while the file still cannot be read, and says so with Try again", async () => {
    const { ipc, user } = await mount({}, (m) => {
      vi.spyOn(m, "settingsGet")
        .mockRejectedValueOnce(ipcError("E_MACHINE_READ", HELD))
        .mockRejectedValueOnce(ipcError("E_MACHINE_READ", "settings.json is still held"));
    });
    await screen.findByRole("heading", { name: "Home" });
    await open(user, "Settings");
    await user.click(await screen.findByLabelText("Tell me first"));
    await settle();
    // Nothing is written: not the change, and not the defaults over a file that is only held.
    expect(ipc.state.calls).not.toContain("settingsSet");
    expect(ipc.state.settings).toEqual(createMockIpc().state.settings);
    // The second read's reason, on the page and in the shell's notice; the shell's has Try again.
    await waitFor(() => expect(screen.getAllByText("settings.json is still held").length).toBeGreaterThan(0));
    expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
    expect(screen.getByText(en["settings.startFailed"])).toBeTruthy();
    expect((screen.getByLabelText("Automatic (recommended)") as HTMLInputElement).checked).toBe(true);
  });

  it("reads again on Try again, shows what the file holds and drops the problem, writing nothing", async () => {
    const { ipc, user } = await mount({ settings: { language: "th", target: "wt", wizardDone: true } }, (m) => {
      vi.spyOn(m, "settingsGet").mockRejectedValueOnce(ipcError("E_MACHINE_READ", HELD));
    });
    await screen.findByRole("heading", { name: "Home" });
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("heading", { name: th["home.title"] })).toBeTruthy();
    expect(screen.queryByText(HELD)).toBeNull();
    expect(screen.queryByRole("button", { name: th["common.retry"] })).toBeNull();
    await open(user, th["nav.settings"]);
    const wt = (await screen.findByLabelText(new RegExp(`^${th["wizard.where.wt"]}`))) as HTMLInputElement;
    expect(wt.checked).toBe(true);
    expect(ipc.state.calls).not.toContain("settingsSet");
  });

  it("starts the wizard when Try again finds no settings yet, as a start would", async () => {
    const { user } = await mount({}, (m) => {
      vi.spyOn(m, "settingsGet").mockRejectedValueOnce(ipcError("E_MACHINE_READ", HELD));
    });
    await screen.findByRole("heading", { name: "Home" });
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("heading", { name: "Make your cmd window beautiful and easy to read" })).toBeTruthy();
    expect(screen.queryByText(HELD)).toBeNull();
  });

  it("says a damaged file that the second read set aside, as a start would", async () => {
    const { user } = await mount({}, (m) => {
      vi.spyOn(m, "settingsGet")
        .mockRejectedValueOnce(ipcError("E_MACHINE_READ", HELD))
        .mockResolvedValueOnce({ settings: { ...m.state.settings, wizardDone: true }, resetFrom: "settings.json.damaged-7" });
    });
    await screen.findByRole("heading", { name: "Home" });
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText(/The old file is kept as settings\.json\.damaged-7/)).toBeTruthy();
    expect(screen.queryByText(HELD)).toBeNull();
  });

  it("is not triggered by a missing settings file: that is a first run, and its saves write at once", async () => {
    const { ipc, user } = await mount({}, (m) => {
      vi.spyOn(m, "settingsGet");
    });
    expect(await screen.findByRole("heading", { name: "Make your cmd window beautiful and easy to read" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
    await user.click(screen.getByRole("button", { name: "ไทย" }));
    await waitFor(() => expect(ipc.state.settings.language).toBe("th"));
    expect(ipc.settingsGet).toHaveBeenCalledTimes(1);
    expect(ipc.state.calls.filter((c) => c === "settingsSet")).toHaveLength(1);
  });
});

describe("Help through the final shell", () => {
  it("runs the health check, applies a fix through the plan, and undoes everything after asking (Review Focus 4)", async () => {
    const { ipc, user } = await home({ otherClink: [OTHER_CLINK] }, (m) => applyIn(m, "Terracotta", true));
    await open(user, "Help");
    await screen.findByText(/D5/);
    await user.click(await screen.findByRole("button", { name: "Replace the other add-on" }));
    const dialog = await screen.findByRole("dialog");
    // The plan lists what goes before anything is applied.
    expect(await within(dialog).findByText("Turning off the other cmd add-on takes these commands out of cmd's start-up:")).toBeTruthy();
    expect(within(dialog).getByText(OTHER_CLINK)).toBeTruthy();
    expect(ipc.state.calls.filter((c) => c === "apply")).toHaveLength(1);
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect(await within(dialog).findByText(/Theme "Terracotta" applied/)).toBeTruthy();
    expect(ipc.state.calls.filter((c) => c === "apply")).toHaveLength(2);
    expect(ipc.state.applied?.request.replaceOtherClink).toBe(true);
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await user.click(screen.getByRole("button", { name: "Open a new cmd window" }));
    await waitFor(() => expect(ipc.state.calls).toContain("openCmd"));
    await user.click(within(screen.getByRole("group", { name: "Help topics" })).getByRole("button", { name: "Undo everything" }));
    const action = (await screen.findAllByRole("button", { name: "Undo everything" })).find((b) => !b.closest('[role="group"]'))!;
    await user.click(action);
    expect(ipc.state.calls).not.toContain("revertAll");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Undo everything" }));
    expect(await screen.findByText("Done: everything CMD Studio changed is back as it was.")).toBeTruthy();
    expect(ipc.state.applied).toBeNull();
    // The bar's apply is gone with it.
    expect(screen.queryByRole("button", { name: "Undo" })).toBeNull();
  });

  it("lists restore points and goes back to one after asking", async () => {
    const { ipc, user } = await home({}, (m) => applyIn(m, "Terracotta"));
    await open(user, "Help");
    await user.click(await screen.findByRole("button", { name: "Restore points" }));
    await user.click(await screen.findByRole("button", { name: "Go back to this point" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Go back to this point" }));
    expect(await screen.findByText("Done: everything is back to that point.")).toBeTruthy();
    expect(ipc.state.applied).toBeNull();
  });
});

describe("the theme's pause", () => {
  const PAUSED_AT = 1791694709000;
  /** The theme paused in cmd by init.lua after three windows in a row (recovery spec §3). */
  const PAUSED_IN_CMD: PauseInfo = { by: "clink", level: "cmd", reason: "cmdStarts", atMs: PAUSED_AT, count: 3, resume: null };

  /** Terracotta applied before the app mounts, then paused in cmd. */
  const pausedInCmd = async (m: MockIpc) => {
    await applyIn(m, "Terracotta");
    m.state.paused = structuredClone(PAUSED_IN_CMD);
  };

  /** Terracotta applied, then the given pause (an apply lifts any pause, so the pause comes last). */
  const pausedWith = (paused: PauseInfo) => async (m: MockIpc) => {
    await applyIn(m, "Terracotta");
    m.state.paused = structuredClone(paused);
  };

  /** The theme a pause of this computer applies again. */
  const resumeTerracotta = () => ({ themeId: presetId("Terracotta"), target: "all" as const, themeName: "Terracotta" });

  const appError = (code: string): AppError => ({ code, messageTh: `(${code})`, messageEn: `(${code})`, details: `why ${code}` });
  const committed: RevertOutcome = { ok: true, steps: [], notes: [], status: "committed", error: null, unrestored: [] };
  const rolledBack: RevertOutcome = { ok: false, steps: [], notes: [], status: "rolledBack", error: appError("E_AUTORUN_WRITE"), unrestored: [] };

  /** The words of the pause that did not go through (ruling R3), and the banner's (ruling R2). */
  const NOT_YET_DIALOG =
    "The theme is paused in cmd only for now: CMD Studio could not take its changes out of this computer, and tries again the next time the CMD Studio app starts.";
  const NOT_YET_BANNER =
    "It is paused in cmd only for now, not yet on this computer: CMD Studio could not take its changes out, and tries again the next time the CMD Studio app starts.";

  /** The banner, found by its heading. */
  const banner = async () => (await screen.findByRole("heading", { name: "The theme is paused" })).closest(".card") as HTMLElement;

  it("says on Home that cmd paused the theme, and turns it back on in one click (criterion 1)", async () => {
    const { ipc, user } = await home({}, pausedInCmd);
    const card = await banner();
    expect(within(card).getByText("cmd failed to reach its prompt 3 times in a row, so CMD Studio paused the theme in cmd.")).toBeTruthy();
    expect(within(card).getByText(/^Since /)).toBeTruthy();
    expect(within(card).getByText("Only cmd windows open without it: nothing on this computer was changed.")).toBeTruthy();
    expect(within(card).getByRole("button", { name: "Undo everything" })).toBeTruthy();
    expect(within(card).getByRole("button", { name: "Report a problem" })).toBeTruthy();
    // No "Pause the theme" while it is paused.
    expect(screen.queryByRole("button", { name: "Pause the theme" })).toBeNull();
    await user.click(within(card).getByRole("button", { name: "Turn the theme back on" }));
    expect(await screen.findByText("The theme is back on. Open a new cmd window to see it.")).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole("heading", { name: "The theme is paused" })).toBeNull());
    expect(ipc.state.paused).toBeNull();
    // Nothing on the computer was changed: no apply besides the one before the app started.
    expect(ipc.state.calls.filter((c) => c === "apply")).toHaveLength(1);
    expect(await screen.findByRole("button", { name: "Pause the theme" })).toBeTruthy();
  });

  it("pauses the theme on this computer after asking, and turns it back on through the plan", async () => {
    const { ipc, user } = await home({}, (m) => applyIn(m, "Terracotta"));
    const active = (await screen.findByRole("heading", { name: "Theme in use" })).closest(".card") as HTMLElement;
    await user.click(await within(active).findByRole("button", { name: "Pause the theme" }));
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByText(
        "CMD Studio takes its changes out of this computer, so cmd windows open without the theme. You can turn it back on from the home page at any time.",
      ),
    ).toBeTruthy();
    expect(ipc.state.calls.some((c) => c.startsWith("themePause"))).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: "Pause the theme" }));
    expect(await within(dialog).findByText("The theme is paused: cmd windows open without it.")).toBeTruthy();
    expect(ipc.state.calls).toContain("themePause:user");
    expect(ipc.state.applied).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    const card = await banner();
    expect(within(card).getByText("You paused the theme.")).toBeTruthy();
    expect(within(card).getByText("CMD Studio took its changes out of this computer. Turning it back on applies Terracotta again.")).toBeTruthy();
    expect(within(card).queryByRole("button", { name: "Undo everything" })).toBeNull();
    // Focus is on Home's heading, not on the page's body: the button that opened the dialog went.
    expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Home" }));
    await user.click(within(card).getByRole("button", { name: "Turn the theme back on" }));
    const plan = await screen.findByRole("dialog");
    await user.click(await within(plan).findByRole("button", { name: "Apply" }));
    expect(await within(plan).findByText(/Theme "Terracotta" applied/)).toBeTruthy();
    expect(ipc.state.applied?.theme.name).toBe("Terracotta");
    expect(ipc.state.paused).toBeNull();
    await user.click(within(plan).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("heading", { name: "The theme is paused" })).toBeNull());
  });

  it("does not leave the line that the theme is back on beside a new pause", async () => {
    const { user } = await home({}, pausedInCmd);
    await user.click(within(await banner()).getByRole("button", { name: "Turn the theme back on" }));
    expect(await screen.findByText("The theme is back on. Open a new cmd window to see it.")).toBeTruthy();
    await user.click(await screen.findByRole("button", { name: "Pause the theme" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Pause the theme" }));
    await within(dialog).findByText("The theme is paused: cmd windows open without it.");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await banner();
    expect(screen.queryByText("The theme is back on. Open a new cmd window to see it.")).toBeNull();
  });

  it("takes the user to the Themes page when the paused theme is no longer there", async () => {
    const gone: PauseInfo = {
      by: "app",
      level: "computer",
      reason: "clinkCrash",
      atMs: PAUSED_AT,
      count: 3,
      resume: { themeId: "00000000-0000-4000-8000-000000000000", target: "all", themeName: null },
    };
    const { user } = await home({ paused: gone });
    const card = await banner();
    expect(within(card).getByText("cmd crashed in its add-on 3 times, so CMD Studio paused the theme.")).toBeTruthy();
    expect(
      within(card).getByText("CMD Studio took its changes out of this computer. Turning it back on takes you to the Themes page to choose a theme."),
    ).toBeTruthy();
    await user.click(within(card).getByRole("button", { name: "Turn the theme back on" }));
    expect(await screen.findByRole("heading", { name: "Themes" })).toBeTruthy();
  });

  it("takes Undo everything of a pause in cmd to Help, and the pause goes with the theme", async () => {
    const { ipc, user } = await home({}, pausedInCmd);
    await user.click(within(await banner()).getByRole("button", { name: "Undo everything" }));
    expect(await screen.findByRole("button", { name: "Undo everything", pressed: true })).toBeTruthy();
    const action = (await screen.findAllByRole("button", { name: "Undo everything" })).find((b) => !b.closest('[role="group"]'))!;
    await user.click(action);
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Undo everything" }));
    expect(await screen.findByText("Done: everything CMD Studio changed is back as it was.")).toBeTruthy();
    expect(ipc.state.paused).toBeNull();
  });

  it("takes Report a problem of the banner to Help's report", async () => {
    const { user } = await home({}, pausedInCmd);
    await user.click(within(await banner()).getByRole("button", { name: "Report a problem" }));
    expect(await screen.findByRole("button", { name: "Report a problem", pressed: true })).toBeTruthy();
  });

  it("says when whether the theme is paused could not be read, and the rest of Home is still there", async () => {
    await home({ failRecoveryWith: "E_MACHINE_READ" }, (m) => applyIn(m, "Terracotta"));
    expect(await screen.findByText("Whether the theme is paused could not be read")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Theme in use" })).toBeTruthy();
    // Not known, so no Pause: it could pause a theme that is paused already.
    expect(screen.queryByRole("button", { name: "Pause the theme" })).toBeNull();
  });

  it("offers on the health check to pause after cmd crashed in its add-on, and to turn back on while paused", async () => {
    const { ipc, user } = await home({ cmdCrashes: 2 }, (m) => applyIn(m, "Terracotta"));
    await open(user, "Help");
    await screen.findByText(/D12/);
    await user.click(await screen.findByRole("button", { name: "Pause the theme" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Pause the theme" }));
    expect(await within(dialog).findByText("The theme is paused: cmd windows open without it.")).toBeTruthy();
    expect(ipc.state.calls).toContain("themePause:clinkCrash");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    // Checked again: D12 says it is paused, and turning it back on goes through the plan.
    await user.click(await screen.findByRole("button", { name: "Turn the theme back on" }));
    const plan = await screen.findByRole("dialog");
    expect(await within(plan).findByRole("button", { name: "Apply" })).toBeTruthy();
  });

  it("reads Home again when the start's crash check paused the theme while Home was reading", async () => {
    const { ipc } = await home({ cmdCrashes: 3 }, (m) => applyIn(m, "Terracotta"));
    const card = await banner();
    expect(within(card).getByText("cmd crashed in its add-on 3 times, so CMD Studio paused the theme.")).toBeTruthy();
    expect(within(card).getByText("CMD Studio took its changes out of this computer. Turning it back on applies Terracotta again.")).toBeTruthy();
    // What Home read before the pause is out of date: it reads again, and no theme is applied now.
    expect(await screen.findByText("No theme is applied yet.")).toBeTruthy();
    expect(ipc.state.applied).toBeNull();
    expect(ipc.state.calls.filter((c) => c === "themesList").length).toBeGreaterThanOrEqual(2);
  });

  it("reads Home again once, never in a loop (R5)", async () => {
    const { ipc } = await home({ cmdCrashes: 3 }, (m) => applyIn(m, "Terracotta"));
    await banner();
    for (let i = 0; i < 3; i++) await settle();
    expect(ipc.state.calls.filter((c) => c === "themesList")).toHaveLength(2);
    expect(ipc.state.calls.filter((c) => c === "recoveryStatus")).toHaveLength(2);
  });

  it("says a pause that has not gone through as in cmd only for now, and offers no Pause (ruling R2)", async () => {
    const notYet: PauseInfo = { by: "app", level: "computerNotYet", reason: "user", atMs: PAUSED_AT, count: 0, resume: resumeTerracotta() };
    await home({}, pausedWith(notYet));
    const card = await banner();
    expect(within(card).getByText("You paused the theme.")).toBeTruthy();
    expect(within(card).getByText(NOT_YET_BANNER)).toBeTruthy();
    expect(within(card).queryByRole("button", { name: "Undo everything" })).toBeNull();
    // Terracotta is still the theme in use, and "Pause the theme" is not offered again.
    expect(screen.getByRole("heading", { name: "Theme in use" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Pause the theme" })).toBeNull();
  });

  it("says the not-yet banner in Thai", async () => {
    const notYet: PauseInfo = { by: "app", level: "computerNotYet", reason: "user", atMs: PAUSED_AT, count: 0, resume: resumeTerracotta() };
    await home({ settings: { language: "th" } }, pausedWith(notYet));
    const card = (await screen.findByRole("heading", { name: "ธีมถูกหยุดไว้" })).closest(".card") as HTMLElement;
    expect(within(card).getByText("คุณหยุดธีมไว้")).toBeTruthy();
    expect(
      within(card).getByText(
        "ตอนนี้หยุดธีมไว้ใน cmd เท่านั้น ยังหยุดทั้งเครื่องไม่สำเร็จ CMD Studio เอาการเปลี่ยนแปลงออกไม่ได้ และจะลองใหม่ตอนเปิดแอป CMD Studio ครั้งหน้า",
      ),
    ).toBeTruthy();
    expect(within(card).getByRole("button", { name: "เปิดธีมกลับ" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "หยุดธีม" })).toBeNull();
  });

  it("says a pause record that cannot be read, and that turning it back on only removes it", async () => {
    const unreadable: PauseInfo = { by: "clink", level: "cmd", reason: null, atMs: null, count: 0, resume: null };
    const { ipc, user } = await home({}, pausedWith(unreadable));
    const card = await banner();
    expect(
      within(card).getByText("CMD Studio's note of the pause can't be read; cmd windows open without the theme until it is turned back on."),
    ).toBeTruthy();
    expect(within(card).getByText("Turning it back on only removes the note: it applies no theme.")).toBeTruthy();
    // Neither a time nor "nothing was changed": nothing of it is known.
    expect(within(card).queryByText(/^Since /)).toBeNull();
    expect(within(card).queryByText(/nothing on this computer was changed/)).toBeNull();
    expect(within(card).getByRole("button", { name: "Undo everything" })).toBeTruthy();
    await user.click(within(card).getByRole("button", { name: "Turn the theme back on" }));
    expect(await screen.findByText("The theme is back on. Open a new cmd window to see it.")).toBeTruthy();
    // Lifted at once: no plan, no apply.
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(ipc.state.paused).toBeNull();
    expect(ipc.state.calls.filter((c) => c === "planApply")).toHaveLength(1);
    expect(ipc.state.calls.filter((c) => c === "apply")).toHaveLength(1);
  });

  it("never says 0 times or 1 times for the crashes that paused the theme", async () => {
    const crashes = (count: number): PauseInfo => ({ by: "app", level: "computer", reason: "clinkCrash", atMs: PAUSED_AT, count, resume: resumeTerracotta() });
    const words = [
      [0, "cmd crashed in its add-on, so CMD Studio paused the theme."],
      [1, "cmd crashed in its add-on once, so CMD Studio paused the theme."],
      [3, "cmd crashed in its add-on 3 times, so CMD Studio paused the theme."],
    ] as const;
    for (const [count, text] of words) {
      const { view } = await home({ paused: crashes(count) });
      const card = await banner();
      expect(within(card).getByText(text)).toBeTruthy();
      expect(card.textContent).not.toMatch(/\b[01] times\b/);
      view.unmount();
    }
  });

  it("turns an older app record back on at once, and never names the theme it noted", async () => {
    // An app record older than the theme in use: a theme was applied after it, and its lift failed.
    const older: PauseInfo = { by: "app", level: "cmd", reason: "user", atMs: PAUSED_AT, count: 0, resume: resumeTerracotta() };
    const { ipc, user } = await home({}, pausedWith(older));
    const card = await banner();
    expect(
      within(card).getByText(
        "A theme was applied after this pause, but the pause could not be lifted, so cmd windows still open without it. Turning it back on lifts the pause.",
      ),
    ).toBeTruthy();
    expect(card.textContent).not.toContain("Terracotta");
    expect(within(card).queryByRole("button", { name: "Undo everything" })).toBeNull();
    await user.click(within(card).getByRole("button", { name: "Turn the theme back on" }));
    expect(await screen.findByText("The theme is back on. Open a new cmd window to see it.")).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(ipc.state.paused).toBeNull();
    expect(ipc.state.calls.filter((c) => c === "apply")).toHaveLength(1);
    await waitFor(() => expect(screen.queryByRole("heading", { name: "The theme is paused" })).toBeNull());
  });

  it("says when whether CMD Studio's changes are out could not be read", async () => {
    const unknown: PauseInfo = { by: "app", level: "computerUnknown", reason: "user", atMs: PAUSED_AT, count: 0, resume: resumeTerracotta() };
    await home({}, pausedWith(unknown));
    const card = await banner();
    expect(within(card).getByText("You paused the theme.")).toBeTruthy();
    expect(
      within(card).getByText("A pause of this computer may be in place, but CMD Studio could not read whether its changes are out (state.json)."),
    ).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Pause the theme" })).toBeNull();
  });

  it("says a pause that was refused as in cmd only for now, with why, and the banner says it after Close", async () => {
    const { ipc, user } = await home({ failPause: "refused" }, (m) => applyIn(m, "Terracotta"));
    await user.click(await screen.findByRole("button", { name: "Pause the theme" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Pause the theme" }));
    expect(await within(dialog).findByText(NOT_YET_DIALOG)).toBeTruthy();
    expect(within(dialog).getByText("(E_BUSY)")).toBeTruthy();
    expect(within(dialog).queryByText("The theme is paused: cmd windows open without it.")).toBeNull();
    // Nothing was taken out of this computer.
    expect(ipc.state.applied).not.toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    const card = await banner();
    expect(within(card).getByText("You paused the theme.")).toBeTruthy();
    expect(within(card).getByText(NOT_YET_BANNER)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Pause the theme" })).toBeNull();
  });

  it("says a pause whose note could not be saved: the changes are out, and applying again turns it back on", async () => {
    const { ipc, user } = await home({ failPause: "notSaved" }, (m) => applyIn(m, "Terracotta"));
    await user.click(await screen.findByRole("button", { name: "Pause the theme" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Pause the theme" }));
    expect(
      await within(dialog).findByText(
        "CMD Studio took its changes out, but could not note the pause: to turn the theme back on, apply it again from the Themes page.",
      ),
    ).toBeTruthy();
    expect(within(dialog).getByText("(E_DATA_WRITE)")).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    // Nothing is noted, so no banner: the theme is simply out.
    expect(await screen.findByText("No theme is applied yet.")).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "The theme is paused" })).toBeNull();
    expect(ipc.state.paused).toBeNull();
  });

  it("shows at once a pause the start tried that did not go through, and reads Home no more than the once", async () => {
    const notYet: PauseInfo = { by: "app", level: "computerNotYet", reason: "clinkCrash", atMs: PAUSED_AT, count: 3, resume: resumeTerracotta() };
    const { ipc } = await home(
      { crashCheck: { kind: "read", count: 3, pausedNow: false, pause: { pausedNow: false, revert: null, refused: appError("E_BUSY"), notSaved: null } } },
      pausedWith(notYet),
    );
    const card = await banner();
    expect(within(card).getByText("cmd crashed in its add-on 3 times, so CMD Studio paused the theme.")).toBeTruthy();
    expect(within(card).getByText(NOT_YET_BANNER)).toBeTruthy();
    const tried = (await screen.findByText("When CMD Studio started, it tried to pause the theme on this computer:")).closest(".card") as HTMLElement;
    expect(within(tried).getByText(NOT_YET_DIALOG)).toBeTruthy();
    expect(within(tried).getByText("(E_BUSY)")).toBeTruthy();
    await settle();
    expect(ipc.state.calls.filter((c) => c === "themesList")).toHaveLength(1);
  });

  it("shows a start pause whose note was not saved, by whether the theme is out", async () => {
    const notSaved = (revert: RevertOutcome) => ({
      kind: "read" as const,
      count: 3,
      pausedNow: false,
      pause: { pausedNow: false, revert, refused: null, notSaved: appError("E_DATA_WRITE") },
    });
    // The theme is out and nothing is noted: said, since no banner says why.
    let mounted = await home({ crashCheck: notSaved(committed) });
    const out = (await screen.findByText("When CMD Studio started, it tried to pause the theme on this computer:")).closest(".card") as HTMLElement;
    expect(
      within(out).getByText("CMD Studio took its changes out, but could not note the pause: to turn the theme back on, apply it again from the Themes page."),
    ).toBeTruthy();
    expect(within(out).getByText("(E_DATA_WRITE)")).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "The theme is paused" })).toBeNull();
    expect(mounted.ipc.state.calls.filter((c) => c === "themesList")).toHaveLength(1);
    mounted.view.unmount();
    // Nothing was taken out and nothing is noted: cmd still opens with the theme.
    mounted = await home({ crashCheck: notSaved(rolledBack) }, (m) => applyIn(m, "Terracotta"));
    const failed = (await screen.findByText("When CMD Studio started, it tried to pause the theme on this computer:")).closest(".card") as HTMLElement;
    expect(
      within(failed).getByText(
        "The theme could not be paused: CMD Studio could not note the pause or take its changes out, so cmd windows still open with it.",
      ),
    ).toBeTruthy();
    expect(mounted.ipc.state.calls.filter((c) => c === "themesList")).toHaveLength(1);
  });

  it("reads Home again once for a start pause that took the theme out and was not saved, and says nothing while the theme still reads as applied", async () => {
    const { ipc } = await home(
      { crashCheck: { kind: "read", count: 3, pausedNow: false, pause: { pausedNow: false, revert: committed, refused: null, notSaved: appError("E_DATA_WRITE") } } },
      (m) => applyIn(m, "Terracotta"),
    );
    await screen.findByRole("heading", { name: "Theme in use" });
    for (let i = 0; i < 3; i++) await settle();
    // The mock keeps Terracotta applied (it was given the check, not run it): once, never in a loop.
    expect(ipc.state.calls.filter((c) => c === "themesList")).toHaveLength(2);
    expect(screen.queryByText("When CMD Studio started, it tried to pause the theme on this computer:")).toBeNull();
  });

  it("asks for the pause once, never on a timer, while the start's crash check has not finished (R8)", async () => {
    const { ipc } = await home({ crashCheck: null }, (m) => applyIn(m, "Terracotta"));
    expect(await screen.findByRole("heading", { name: "Theme in use" })).toBeTruthy();
    expect(await screen.findByRole("button", { name: "Pause the theme" })).toBeTruthy();
    for (let i = 0; i < 10; i++) await settle();
    expect(ipc.state.calls.filter((c) => c === "recoveryStatus")).toHaveLength(1);
  });
});

describe("accessibility", () => {
  /** Every button has a name: its label, the text it is labelled by, or its own text. */
  function expectNamedButtons(where: string) {
    const buttons = screen.getAllByRole("button");
    expect(buttons.length, where).toBeGreaterThan(0);
    for (const button of buttons) {
      const by = button.getAttribute("aria-labelledby");
      const name = button.getAttribute("aria-label") ?? (by ? document.getElementById(by)?.textContent : null) ?? button.textContent ?? "";
      expect(name.trim(), `${where}: a button without a name: ${button.outerHTML}`).not.toBe("");
    }
  }

  it(
    "names every button of every wizard page, page, Help tab and dialog, in English and in Thai",
    async () => {
      for (const language of ["en", "th"] as Language[]) {
        const d = language === "th" ? th : en;
        // No Windows Terminal, so page 2 offers the Store; another add-on, so the plans ask
        // first; Dracula applied before, so Home has a theme to switch back to.
        const { user, view } = await mount({ otherClink: [OTHER_CLINK], noWindowsTerminal: true, settings: { language } }, (m) =>
          applyIn(m, "Dracula", true),
        );
        const page = async (n: number) => {
          await screen.findByText(d["wizard.step"].replace("{n}", String(n)));
          await settle();
          expectNamedButtons(`${language}: wizard page ${n}`);
        };
        /** Opens a dialog with `opener`, checks its buttons once `ready` is there, and closes it. */
        const inDialog = async (where: string, opener: HTMLElement, ready: string, close: string) => {
          await user.click(opener);
          const dialog = await screen.findByRole("dialog");
          await within(dialog).findByRole("button", { name: ready });
          expectNamedButtons(`${language}: ${where}`);
          await user.click(within(dialog).getByRole("button", { name: close }));
          await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
        };
        await screen.findByRole("heading", { name: d["wizard.welcome.title"] });
        await page(1);
        await user.click(screen.getByRole("button", { name: d["common.next"] }));
        await screen.findByRole("button", { name: d["wizard.check.wtInstall"] });
        await page(2);
        await user.click(screen.getByRole("button", { name: d["common.next"] }));
        await screen.findByRole("button", { name: "Terracotta", pressed: true });
        await page(3);
        await user.click(screen.getByRole("button", { name: d["wizard.choose.seeAll"] }));
        expectNamedButtons(`${language}: wizard page 3, all twelve`);
        await user.click(screen.getByRole("button", { name: d["common.next"] }));
        await page(4);
        await user.click(screen.getByRole("button", { name: d["common.next"] }));
        await screen.findByRole("button", { name: d["apply.otherClink.replace"] });
        await page(5);
        await user.click(screen.getByRole("button", { name: d["apply.otherClink.replace"] }));
        await screen.findByText(d["apply.planTitle"]);
        expectNamedButtons(`${language}: wizard page 5, the plan`);
        await user.click(screen.getByRole("button", { name: d["apply.start"] }));
        await screen.findByRole("heading", { name: d["wizard.done.title"] });
        await page(6);
        await user.click(screen.getByRole("button", { name: d["wizard.done.goHome"] }));

        // The pages, with the theme the wizard applied.
        await screen.findByRole("heading", { name: d["home.title"] });
        await settle();
        expectNamedButtons(`${language}: ${d["nav.home"]}`);
        for (const name of [d["nav.themes"], d["nav.settings"], d["nav.advanced"]]) {
          await open(user, name);
          await screen.findByRole("heading", { name });
          await settle();
          expectNamedButtons(`${language}: ${name}`);
        }
        // Home's quick-switch dialog.
        await open(user, d["nav.home"]);
        const switchBack = await screen.findByRole("button", { name: d["home.quick.use"].replace("{name}", "Dracula") });
        await inDialog("Home's quick-switch dialog", switchBack, d["apply.otherClink.replace"], d["common.cancel"]);
        // Settings' plan dialog.
        await open(user, d["nav.settings"]);
        const again = await screen.findByRole("button", { name: d["settings.target.applyAgain"] });
        await inDialog("Settings' plan dialog", again, d["apply.otherClink.replace"], d["common.cancel"]);
        // Every Help tab, with its fix and confirm dialogs.
        await open(user, d["nav.help"]);
        await screen.findByRole("heading", { name: d["help.title"] });
        const tabButton = (tab: string) => within(screen.getByRole("group", { name: d["help.tabs.label"] })).getByRole("button", { name: tab });
        for (const tab of ["doctor", "restore", "revert", "report", "guide", "credits"] as const) {
          await user.click(tabButton(d[`help.tabs.${tab}`]));
          await settle();
          expectNamedButtons(`${language}: Help, ${tab}`);
          if (tab === "doctor") {
            const fix = await screen.findByRole("button", { name: d["help.doctor.fix.replaceOtherClink"] });
            await inDialog("Help's fix dialog", fix, d["apply.start"], d["a11y.closeDialog"]);
          }
          if (tab === "restore") {
            const [goBack] = await screen.findAllByRole("button", { name: d["help.restore.restore"] });
            await inDialog("Help's restore confirm", goBack!, d["help.restore.restore"], d["a11y.closeDialog"]);
          }
          if (tab === "revert") {
            const action = screen.getAllByRole("button", { name: d["help.revert.button"] }).find((b) => !b.closest('[role="group"]'))!;
            await inDialog("Help's undo-everything confirm", action, d["help.revert.button"], d["a11y.closeDialog"]);
          }
        }
        // The editor of a copy.
        await open(user, d["nav.home"]);
        await user.click(await screen.findByRole("button", { name: d["home.active.customize"] }));
        await screen.findByRole("heading", { name: d["editor.title"] });
        await settle();
        expectNamedButtons(`${language}: the editor`);
        view.unmount();
      }
    },
    60000,
  );

  it("names every button of the pause banner and its dialogs, in English and in Thai", async () => {
    for (const language of ["en", "th"] as Language[]) {
      const d = language === "th" ? th : en;
      const { user, view } = await home({ settings: { language } }, (m) => applyIn(m, "Terracotta"));
      await user.click(await screen.findByRole("button", { name: d["home.active.pause"] }));
      const dialog = await screen.findByRole("dialog");
      expectNamedButtons(`${language}: the pause dialog`);
      await user.click(within(dialog).getByRole("button", { name: d["pause.button"] }));
      await within(dialog).findByText(d["pause.done"]);
      expectNamedButtons(`${language}: the pause dialog, done`);
      await user.click(within(dialog).getByRole("button", { name: d["a11y.closeDialog"] }));
      await screen.findByRole("heading", { name: d["home.paused.title"] });
      await settle();
      expectNamedButtons(`${language}: Home, paused`);
      await user.click(screen.getByRole("button", { name: d["resume.button"] }));
      const plan = await screen.findByRole("dialog");
      await within(plan).findByRole("button", { name: d["apply.start"] });
      expectNamedButtons(`${language}: the plan that turns the theme back on`);
      view.unmount();
    }
  });
});
