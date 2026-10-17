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
import type { ApplyOutcome } from "./bindings/ApplyOutcome";
import type { ApplyRequest } from "./bindings/ApplyRequest";
import type { SystemReport } from "./bindings/SystemReport";
import { App } from "./App";
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
});
