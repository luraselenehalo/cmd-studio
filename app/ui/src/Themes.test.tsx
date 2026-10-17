// The theme library and the editor against the mock IPC: Customize copies a preset into my
// themes and opens the editor; the filter and the "in use" badge; a plan shown and applied
// from a card; the after-apply bar's Undo; the editor's draft and save; a failed apply is said.
// Then the rulings this plan's Task 6 carries: the bar undoes the apply's own restore point and
// says how the undo went, in the target's words (R3, R4); the apply dialog stays open while an
// apply runs (R7); a rename keeps the draft (R8); a tone change and a readability fix are one
// undo step each (R9); the preview's sample is this computer's (R1); numbers are whole; a draft
// that cannot be saved is said; a library problem is in the chosen language; a route change
// puts focus on the screen's heading.
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ApplyOutcome } from "./bindings/ApplyOutcome";
import { App } from "./App";
import { IpcProvider } from "./ipc/context";
import { createMockIpc, type MockOptions } from "./ipc/mock";
import { PRESETS } from "./ipc/presets";
import { IpcError } from "./ipc/types";

type MockIpc = ReturnType<typeof createMockIpc>;

function mount(options: MockOptions = {}) {
  const ipc = createMockIpc({ ...options, settings: { wizardDone: true, ...options.settings } });
  const user = userEvent.setup();
  const view = render(
    <IpcProvider ipc={ipc}>
      <App />
    </IpcProvider>,
  );
  return { ipc, user, view };
}

/** Like `mount`, with the mock's library (or its answers) set up before the app starts. */
async function mountWith(seed: (ipc: MockIpc) => Promise<void> | void, options: MockOptions = {}) {
  const ipc = createMockIpc({ ...options, settings: { wizardDone: true, ...options.settings } });
  await seed(ipc);
  const user = userEvent.setup();
  const view = render(
    <IpcProvider ipc={ipc}>
      <App />
    </IpcProvider>,
  );
  return { ipc, user, view };
}

/** Opens the Themes page: the shell starts on it in the themes plan, on Home afterwards. */
async function openThemes(user: ReturnType<typeof userEvent.setup>) {
  await screen.findByRole("heading", { name: /^(Home|Themes)$/ });
  await user.click(screen.getByRole("button", { name: "Themes" }));
  await screen.findByRole("heading", { name: "Themes" }, { timeout: 3000 });
}

async function card(name: string): Promise<HTMLElement> {
  const labels = await screen.findAllByText(name);
  return labels[0]!.closest(".theme-card") as HTMLElement;
}

function presetId(name: string): string {
  const found = PRESETS.find((p) => p.name === name);
  if (!found) throw new Error(`no preset ${name}`);
  return found.id;
}

/** A copy of a preset in my themes, as Customize makes it; with a draft when `draft` is given. */
async function mineFrom(ipc: MockIpc, preset: string, name: string, draft?: { author: string }) {
  const copy = await ipc.themeDuplicate(presetId(preset), name);
  if (draft) {
    const { theme } = await ipc.themeGet(copy.id);
    await ipc.themeSave({ ...theme, author: draft.author }, true);
  }
  return copy;
}

function ipcError(code: string, details: string): IpcError {
  return new IpcError({ code, messageTh: `(${code})`, messageEn: `(${code})`, details });
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

/** Opens the editor on a copy of `preset` made by Customize. */
async function customize(user: ReturnType<typeof userEvent.setup>, preset: string) {
  await openThemes(user);
  await user.click(within(await card(preset)).getByRole("button", { name: "Customize" }));
  await screen.findByRole("heading", { name: "Edit theme" });
}

/** Applies a preset from its card and closes the dialog once it says the theme is applied. */
async function applyFromCard(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(within(await card(name)).getByRole("button", { name: "Apply" }));
  const dialog = await screen.findByRole("dialog");
  await user.click(await within(dialog).findByRole("button", { name: "Apply" }));
  await within(dialog).findByText(new RegExp(`Theme "${name}" applied`));
  await user.click(within(dialog).getByRole("button", { name: "Close" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
}

describe("the theme library", () => {
  it("lists the twelve presets and customizes one into a copy", async () => {
    const { ipc, user } = mount();
    await openThemes(user);
    expect((await screen.findAllByText("Built in")).length).toBe(12);
    await user.click(within(await card("Terracotta")).getByRole("button", { name: "Customize" }));
    const heading = await screen.findByRole("heading", { name: "Edit theme" });
    // A route change puts focus on the new screen's heading (spec §4.5).
    await waitFor(() => expect(document.activeElement).toBe(heading));
    expect(ipc.state.mine.map((t) => t.name)).toEqual(["My Terracotta"]);
    expect((screen.getByLabelText("Theme name") as HTMLInputElement).value).toBe("My Terracotta");
    // Advanced mode shows the 23 colors; a hand edit makes the recipe null.
    await user.click(screen.getByRole("tab", { name: "Advanced" }));
    expect(screen.getByLabelText("Frame")).toBeTruthy();
    expect(screen.getByLabelText("Bright white")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved.")).toBeTruthy();
    expect(ipc.state.mine[0]?.name).toBe("My Terracotta");
  });

  it("filters by tone, shows the plan before applying, and marks the theme in use", async () => {
    const { ipc, user } = mount();
    await openThemes(user);
    await user.click(screen.getByRole("button", { name: "Light" }));
    await waitFor(() => expect(screen.queryByText("Terracotta")).toBeNull());
    expect(screen.getByText("Paper")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "All" }));
    await user.click(within(await card("Paper")).getByRole("button", { name: "Apply" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Update the prompt and the welcome box")).toBeTruthy();
    expect(within(dialog).getByText("Use this theme in every cmd window")).toBeTruthy();
    expect(ipc.state.applied).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    await within(dialog).findByText(/Theme "Paper" applied/);
    expect(ipc.state.applied?.theme.name).toBe("Paper");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.getAllByText("In use").length).toBe(1));
    // The after-apply bar undoes it.
    await user.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(ipc.state.applied).toBeNull());
  });

  it("asks about another cmd add-on and goes on with the user's yes", async () => {
    const { ipc, user } = mount({ otherClink: ['"C:\\clink\\clink_x64.exe" inject --autorun'] });
    await openThemes(user);
    await user.click(within(await card("Nord")).getByRole("button", { name: "Apply" }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByRole("heading", { name: "Another cmd add-on is already set up" });
    expect(within(dialog).getByText(/clink_x64\.exe/)).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Use CMD Studio's instead" }));
    await within(dialog).findByText("Turn off the cmd add-on that was set up before (can be undone)");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    await within(dialog).findByText(/applied/);
    expect(ipc.state.applied?.request.replaceOtherClink).toBe(true);
  });

  it("says a failed apply was put back, with a way to copy the details", async () => {
    const { ipc, user } = mount({ failApplyWith: "E_VERIFY_OMP" });
    await openThemes(user);
    await user.click(within(await card("Dracula")).getByRole("button", { name: "Apply" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(await within(dialog).findByRole("button", { name: "Apply" }));
    expect(await within(dialog).findByText("The theme could not be applied. Everything was put back as it was.")).toBeTruthy();
    expect(within(dialog).getByText(/Code E_VERIFY_OMP/)).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Copy details for a problem report" })).toBeTruthy();
    expect(ipc.state.applied).toBeNull();
    // A rolled-back apply still carries a restore point, but there is nothing to undo.
    expect(screen.queryByRole("button", { name: "Undo" })).toBeNull();
  });

  it("makes a new theme in simple mode and keeps a draft", async () => {
    const { ipc, user } = mount();
    await openThemes(user);
    // The dictionary's button starts with a verb (spec §4.5): "Create a theme".
    await user.click(screen.getByRole("button", { name: "Create a theme" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    expect(screen.getByRole("tab", { name: "Simple", selected: true })).toBeTruthy();
    expect(ipc.state.drafts).toHaveLength(1);
    expect(ipc.state.drafts[0]?.simple).toEqual({ accent: "#d97757", tint: 0.1 });
    // Powerline switches the font to the Nerd Font and says so.
    await user.click(screen.getByLabelText("Powerline"));
    expect(await screen.findByText(/switched to CaskaydiaCove NF/)).toBeTruthy();
    expect((screen.getByLabelText("Font") as HTMLSelectElement).value).toBe("CaskaydiaCove NF");
    await user.click(screen.getByRole("button", { name: "Back to themes" }));
    expect((await screen.findAllByText("Draft")).length).toBe(1);
    // Leaving within the second saved the edit that was still waiting for its draft.
    expect(ipc.state.drafts[0]?.prompt.layout).toBe("powerline");
  });

  it("imports after a preview", async () => {
    const { ipc, user } = mount();
    await openThemes(user);
    await user.click(screen.getByRole("button", { name: "Import a theme file" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("You already have this theme; the import is added as a separate theme.")).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Add to my themes" }));
    await waitFor(() => expect(ipc.state.mine.map((t) => t.name)).toEqual(["Imported theme"]));
  });

  it("renames a theme and keeps its draft under the new name (R8)", async () => {
    const { ipc, user } = await mountWith((m) => mineFrom(m, "Nord", "My Nord", { author: "from the draft" }).then(() => {}));
    await openThemes(user);
    await user.click(within(await card("My Nord")).getByRole("button", { name: "Rename" }));
    const dialog = await screen.findByRole("dialog");
    const field = within(dialog).getByLabelText("New name");
    await user.clear(field);
    await user.type(field, "Fjord");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(ipc.state.mine.map((t) => t.name)).toEqual(["Fjord"]));
    expect(ipc.state.drafts.map((d) => [d.name, d.author])).toEqual([["Fjord", "from the draft"]]);
  });

  it("shows a library problem in the chosen language, after opening in English", async () => {
    const { view } = await mountWith(
      (m) => {
        const list = m.themesList.bind(m);
        vi.spyOn(m, "themesList").mockImplementation(async () => ({
          ...(await list()),
          problems: [
            {
              file: "broken.cmdtheme",
              error: {
                code: "E_THEME_INVALID",
                messageTh: "ไฟล์ธีมนี้ไม่ถูกต้อง",
                messageEn: "This theme file is not valid",
                details: "line 3: expected a comma",
              },
            },
          ],
        }));
      },
      { settings: { language: "th" } },
    );
    // The UI opens in English; the saved Thai takes over once the settings are read.
    expect(view.container.querySelector("div[lang]")?.getAttribute("lang")).toBe("en");
    expect(await screen.findByText("ไฟล์ธีมนี้ไม่ถูกต้อง")).toBeTruthy();
    expect(screen.queryByText("This theme file is not valid")).toBeNull();
    expect(screen.getByText("broken.cmdtheme")).toBeTruthy();
    expect(screen.getByText("E_THEME_INVALID")).toBeTruthy();
    expect(view.container.querySelector("div[lang]")?.getAttribute("lang")).toBe("th");
  });
});

describe("the after-apply bar", () => {
  it("undoes the apply's own restore point and keeps saying so after the bar is gone (R3, R4)", async () => {
    const { ipc, user } = mount();
    const restore = vi.spyOn(ipc, "snapshotRestore");
    await openThemes(user);
    await applyFromCard(user, "Paper");
    const snapshotId = ipc.state.applied?.snapshotId;
    expect(snapshotId).toBeTruthy();
    expect(screen.getByText(/open a new cmd window/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(await screen.findByText("Undone: everything is as it was before.")).toBeTruthy();
    expect(restore).toHaveBeenCalledWith(snapshotId);
    expect(ipc.state.applied).toBeNull();
    expect(screen.queryByRole("button", { name: "Undo" })).toBeNull();
    await waitFor(() => expect(screen.queryByText("In use")).toBeNull());
  });

  it("says an undo that was put back, with its error, and still offers Undo", async () => {
    const { ipc, user } = mount();
    await openThemes(user);
    await applyFromCard(user, "Nord");
    vi.spyOn(ipc, "snapshotRestore").mockResolvedValue({
      status: "stopped",
      undone: [],
      last: {
        status: "rolledBack",
        undid: ipc.state.applied?.snapshotId ?? null,
        error: { code: "E_WT_WRITE", messageTh: "(E_WT_WRITE)", messageEn: "(E_WT_WRITE)", details: "settings.json is locked" },
        unrestored: [],
        notes: [],
        fontsLeft: [],
      },
      notes: [],
    });
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(await screen.findByText("The undo did not go through, so nothing changed: the theme is still applied.")).toBeTruthy();
    expect(screen.getByText(/Code E_WT_WRITE/)).toBeTruthy();
    expect(screen.queryByText("Undone: everything is as it was before.")).toBeNull();
    expect(screen.getByRole("button", { name: "Undo" })).toBeTruthy();
  });

  it("lists what an undo could not put back, to fix by hand", async () => {
    const { ipc, user } = mount();
    await openThemes(user);
    await applyFromCard(user, "Nord");
    vi.spyOn(ipc, "snapshotRestore").mockResolvedValue({
      status: "stopped",
      undone: [],
      last: {
        status: "rollbackIncomplete",
        undid: ipc.state.applied?.snapshotId ?? null,
        error: { code: "E_WT_WRITE", messageTh: "(E_WT_WRITE)", messageEn: "(E_WT_WRITE)", details: "settings.json is locked" },
        unrestored: [
          {
            opIndex: 3,
            target: "Windows Terminal settings.json",
            code: "E_WT_WRITE",
            message: "the file is locked",
            fix: "Close Windows Terminal and copy settings.json back from the restore point.",
            fixTh: "ปิด Windows Terminal แล้วคัดลอก settings.json คืนจากจุดย้อนกลับ",
          },
        ],
        notes: [],
        fontsLeft: [],
      },
      notes: [],
    });
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(await screen.findByText("The undo did not go through, and not everything could be put back. Fix these by hand:")).toBeTruthy();
    expect(screen.getByText("Close Windows Terminal and copy settings.json back from the restore point.")).toBeTruthy();
  });

  it("says to open a new cmd tab in Windows Terminal when the target is Windows Terminal", async () => {
    const { user } = mount({ settings: { target: "wt" } });
    await openThemes(user);
    await applyFromCard(user, "Paper");
    expect(screen.getByText(/open a new cmd tab in Windows Terminal/)).toBeTruthy();
    expect(screen.queryByText(/open a new cmd window/)).toBeNull();
  });

  it("keeps the apply dialog open while the apply runs (R7)", async () => {
    const { ipc, user } = mount();
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const apply = ipc.apply.bind(ipc);
    vi.spyOn(ipc, "apply").mockImplementation(async (request, digest, onProgress): Promise<ApplyOutcome> => {
      await gate;
      return apply(request, digest, onProgress);
    });
    await openThemes(user);
    await user.click(within(await card("Paper")).getByRole("button", { name: "Apply" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(await within(dialog).findByRole("button", { name: "Apply" }));
    await within(dialog).findByText("Applying…");
    expect(within(dialog).queryByRole("button", { name: "Close" })).toBeNull();
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBe(dialog);
    release();
    await within(dialog).findByText(/Theme "Paper" applied/);
    expect(within(dialog).getByRole("button", { name: "Close" })).toBeTruthy();
  });
});

describe("the editor", () => {
  it("previews with this computer's sample and keeps numbers whole", async () => {
    const { ipc, user } = await mountWith((m) => {
      vi.spyOn(m, "previewRender");
    });
    const render = vi.mocked(ipc.previewRender);
    await openThemes(user);
    await user.click(within(await card("Terracotta")).getByRole("button", { name: "Customize" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    expect((screen.getByLabelText("Sample user name") as HTMLInputElement).value).toBe("tester");
    expect((screen.getByLabelText("Sample folder") as HTMLInputElement).value).toBe("C:\\Users\\tester\\projects\\cmd-studio");
    await waitFor(() => expect(render).toHaveBeenCalled());
    const context = render.mock.calls.at(-1)?.[1];
    expect(context).toMatchObject({ user: "tester", host: "TESTPC", home: "C:\\Users\\tester", cwd: "C:\\Users\\tester\\projects\\cmd-studio" });
    // A fraction would fail Tauri's u8/u16 arguments: it is rounded.
    fireEvent.change(screen.getByLabelText("Window width"), { target: { value: "99.6" } });
    await waitFor(() => expect(render.mock.calls.at(-1)?.[1].width).toBe(100));
    fireEvent.change(screen.getByLabelText("Size"), { target: { value: "12.5" } });
    await user.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByText("Saved.");
    expect(ipc.state.mine[0]?.terminal.fontSize).toBe(13);
  });

  it("changes the tone in one undo step and fixes readability in one more (R9)", async () => {
    const { user } = mount();
    await openThemes(user);
    await user.click(within(await card("Gruvbox")).getByRole("button", { name: "Customize" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    expect(await screen.findByText("Red is hard to read (2.7:1, needs 3.0:1)")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Fix automatically" }));
    expect(await screen.findByText("Every color is easy to read.")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(await screen.findByText("Red is hard to read (2.7:1, needs 3.0:1)")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Undo" }) as HTMLButtonElement).disabled).toBe(true);

    await user.click(screen.getByRole("button", { name: "Back to themes" }));
    await user.click(await screen.findByRole("button", { name: "Create a theme" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    await user.click(screen.getByRole("radio", { name: "Light" }));
    await waitFor(() => expect((screen.getByRole("radio", { name: "Light" }) as HTMLInputElement).checked).toBe(true));
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect((screen.getByRole("radio", { name: "Dark" }) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByRole("button", { name: "Undo" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("undoes with Ctrl+Z and redoes with Ctrl+Y, on a Thai keyboard layout too", async () => {
    const { user } = mount();
    await openThemes(user);
    await user.click(screen.getByRole("button", { name: "Create a theme" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    const light = screen.getByRole("radio", { name: "Light" });
    const dark = screen.getByRole("radio", { name: "Dark" }) as HTMLInputElement;
    await user.click(light);
    await waitFor(() => expect((light as HTMLInputElement).checked).toBe(true));
    await user.keyboard("{Control>}z{/Control}");
    expect(dark.checked).toBe(true);
    await user.keyboard("{Control>}y{/Control}");
    expect((light as HTMLInputElement).checked).toBe(true);
    // The Thai layout's letters on the Z and Y keys.
    fireEvent.keyDown(light, { key: "ผ", code: "KeyZ", ctrlKey: true });
    expect(dark.checked).toBe(true);
    fireEvent.keyDown(dark, { key: "ั", code: "KeyY", ctrlKey: true });
    expect((light as HTMLInputElement).checked).toBe(true);
  });

  it("says a draft that could not be saved", async () => {
    const { user } = mount();
    await openThemes(user);
    await user.click(screen.getByRole("button", { name: "Create a theme" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    await user.clear(screen.getByLabelText("Theme name"));
    expect(await screen.findByText("Draft not saved", undefined, { timeout: 3000 })).toBeTruthy();
    expect(screen.getByText(/Code E_THEME_INVALID/)).toBeTruthy();
    expect(screen.queryByText("Draft saved")).toBeNull();
  });

  it("offers the draft left from before and goes on from it", async () => {
    const { ipc, user } = await mountWith((m) => mineFrom(m, "Nord", "My Nord", { author: "from the draft" }).then(() => {}));
    await openThemes(user);
    await user.click(within(await card("My Nord")).getByRole("button", { name: "Edit" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("You have unsaved changes from before.")).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Keep editing them" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // What had focus before the offer is gone with the library: the heading takes it.
    expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Edit theme" }));
    expect((screen.getByLabelText("Author") as HTMLInputElement).value).toBe("from the draft");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByText("Saved.");
    expect(ipc.state.mine[0]?.author).toBe("from the draft");
    expect(ipc.state.drafts).toEqual([]);
  });

  it("discards the draft left from before when asked", async () => {
    const { ipc, user } = await mountWith((m) => mineFrom(m, "Nord", "My Nord", { author: "from the draft" }).then(() => {}));
    const stored = ipc.state.mine[0]!.author;
    await openThemes(user);
    await user.click(within(await card("My Nord")).getByRole("button", { name: "Edit" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Discard them" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(ipc.state.drafts).toEqual([]);
    expect((screen.getByLabelText("Author") as HTMLInputElement).value).toBe(stored);
    expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Edit theme" }));
  });

  it("writes the draft of an edit made just before leaving by the sidebar (fix 1)", async () => {
    const { ipc, user } = mount();
    await customize(user, "Terracotta");
    const author = screen.getByLabelText("Author");
    await user.clear(author);
    await user.type(author, "Q");
    // Well within the draft second: the editor goes away before its timer fires.
    await user.click(screen.getByRole("button", { name: "Themes" }));
    await screen.findByRole("heading", { name: "Themes" });
    await waitFor(() => expect(ipc.state.drafts.map((d) => d.author)).toEqual(["Q"]));
  });

  it("writes no draft after the user chose to go back without it", async () => {
    const { ipc, user } = await mountWith((m) => {
      const save = m.themeSave.bind(m);
      let refused = false;
      vi.spyOn(m, "themeSave").mockImplementation(async (theme, draft) => {
        // The first draft write fails; any later one would go through.
        if (draft && !refused) {
          refused = true;
          throw ipcError("E_DATA_WRITE", "drafts folder is read-only");
        }
        return save(theme, draft);
      });
    });
    await customize(user, "Terracotta");
    await user.type(screen.getByLabelText("Author"), "Q");
    expect(await screen.findByText("Draft not saved", undefined, { timeout: 3000 })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Go back without saving the draft" }));
    await screen.findByRole("heading", { name: "Themes" });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(ipc.state.drafts).toEqual([]);
  });

  it("saves after a draft write that is still running, so no draft is left behind (fix 5)", async () => {
    let writing = false;
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const { ipc, user } = await mountWith((m) => {
      const save = m.themeSave.bind(m);
      // A slow disk: each draft write waits for the gate.
      vi.spyOn(m, "themeSave").mockImplementation(async (theme, draft) => {
        if (draft) {
          writing = true;
          await gate;
        }
        return save(theme, draft);
      });
    });
    await customize(user, "Terracotta");
    await user.clear(screen.getByLabelText("Author"));
    await user.type(screen.getByLabelText("Author"), "Q");
    await waitFor(() => expect(writing).toBe(true), { timeout: 3000 });
    await user.click(screen.getByRole("button", { name: "Save" }));
    release();
    await screen.findByText("Saved.");
    expect(ipc.state.mine[0]?.author).toBe("Q");
    expect(ipc.state.drafts).toEqual([]);
  });

  it("leaves the editor's history alone for Ctrl+Z and Ctrl+Y in a text or a number field", async () => {
    const { user } = mount();
    await customize(user, "Terracotta");
    const author = screen.getByLabelText("Author") as HTMLInputElement;
    await user.clear(author);
    await user.type(author, "Q");
    const redo = screen.getByRole("button", { name: "Redo" }) as HTMLButtonElement;
    await user.keyboard("{Control>}z{/Control}");
    expect(author.value).toBe("Q");
    expect(redo.disabled).toBe(true);
    await user.click(screen.getByLabelText("Size"));
    await user.keyboard("{Control>}z{/Control}");
    expect(author.value).toBe("Q");
    expect(redo.disabled).toBe(true);
    // Something to redo (the Undo button takes the "Q" back), then Ctrl+Y in each field.
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(author.value).toBe("");
    await user.click(screen.getByLabelText("Size"));
    await user.keyboard("{Control>}y{/Control}");
    expect(author.value).toBe("");
    await user.click(author);
    await user.keyboard("{Control>}y{/Control}");
    expect(author.value).toBe("");
    expect(redo.disabled).toBe(false);
  });

  it("says when this computer's details could not be read, and previews with sample values (fix 3)", async () => {
    const { user } = await mountWith((m) => {
      vi.spyOn(m, "systemDetect").mockRejectedValue(ipcError("E_MACHINE_READ", "the registry could not be read"));
    });
    await customize(user, "Terracotta");
    expect(
      screen.getByText(
        "This computer's details could not be read, so the preview uses sample values and the list of installed fonts is not available.",
      ),
    ).toBeTruthy();
    expect(screen.getByText(/Code E_MACHINE_READ/)).toBeTruthy();
    expect((screen.getByLabelText("Sample user name") as HTMLInputElement).value).toBe("you");
  });

  it("saves the theme before planning in Save and apply, and keeps that dialog open while applying (R12, R7)", async () => {
    const { ipc, user } = mount();
    const release = holdApply(ipc);
    await customize(user, "Terracotta");
    const from = ipc.state.calls.length;
    await user.click(screen.getByRole("button", { name: "Save and apply" }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByRole("button", { name: "Apply" });
    expect(ipc.state.calls.slice(from).filter((c) => c === "themeSave" || c === "planApply")).toEqual(["themeSave", "planApply"]);
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    await within(dialog).findByText("Applying…");
    expect(within(dialog).queryByRole("button", { name: "Close" })).toBeNull();
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBe(dialog);
    release();
    await within(dialog).findByText(/Theme "My Terracotta" applied/);
    expect(within(dialog).getByRole("button", { name: "Close" })).toBeTruthy();
  });
});
