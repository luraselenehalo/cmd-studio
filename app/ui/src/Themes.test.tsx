// The theme library and the editor against the mock IPC: Customize copies a preset into my
// themes and opens the editor; the filter and the "in use" badge; a plan shown and applied
// from a card; the after-apply bar's Undo; the editor's draft and save; a failed apply is said.
// Then the rulings this plan's Task 6 carries: the bar undoes the apply's own restore point and
// says how the undo went, in the target's words (R3, R4); the apply dialog stays open while an
// apply runs (R7); a rename keeps the draft (R8); a tone change and a readability fix are one
// undo step each (R9); the preview's sample is this computer's (R1); numbers are whole; a draft
// that cannot be saved is said; a library problem is in the chosen language; a route change
// puts focus on the screen's heading.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ApplyOutcome } from "./bindings/ApplyOutcome";
import type { RestoreOutcome } from "./bindings/RestoreOutcome";
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

/** A mount whose `paletteGenerate` answers only when its gate is opened, once `held.on` is set:
 * each held call adds its gate to `held.gates`, in the order the calls came. */
async function mountHoldingPalette() {
  const held = { on: false, gates: [] as (() => void)[] };
  const mounted = await mountWith((m) => {
    const generate = m.paletteGenerate.bind(m);
    vi.spyOn(m, "paletteGenerate").mockImplementation(async (tone, accent, tint) => {
      if (held.on) await new Promise<void>((resolve) => held.gates.push(resolve));
      return generate(tone, accent, tint);
    });
  });
  return { ...mounted, held };
}

/**
 * A mount whose draft writes are refused while `control.refuse` is set, with `control.details`
 * as the reason. With `control.holdNext` set, the next draft write waits until
 * `control.release(refuse)`, and `control.held` tells it is waiting.
 */
async function mountRefusingDrafts() {
  const control = {
    refuse: true,
    details: "drafts folder is read-only",
    holdNext: false,
    held: false,
    release: (_refuse: boolean) => {},
  };
  const mounted = await mountWith((m) => {
    const save = m.themeSave.bind(m);
    vi.spyOn(m, "themeSave").mockImplementation(async (theme, draft) => {
      let refuse = control.refuse;
      if (draft && control.holdNext) {
        control.holdNext = false;
        control.held = true;
        refuse = await new Promise<boolean>((resolve) => {
          control.release = resolve;
        });
      }
      if (draft && refuse) throw ipcError("E_DATA_WRITE", control.details);
      return save(theme, draft);
    });
  });
  return { ...mounted, control };
}

/** Customizes `preset`, types into Author, and leaves by the sidebar at once. */
async function editAndLeave(user: ReturnType<typeof userEvent.setup>, preset: string) {
  await customize(user, preset);
  await user.type(screen.getByLabelText("Author"), "Q");
  await user.click(screen.getByRole("button", { name: "Themes" }));
  await screen.findByRole("heading", { name: "Themes" });
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
    // Advanced mode shows the 23 colors; a hand edit makes the recipe null. The modes are two
    // toggle buttons in a named group, not tabs (item 9).
    const modes = screen.getByRole("group", { name: "Color mode" });
    expect(screen.queryAllByRole("tab")).toEqual([]);
    await user.click(within(modes).getByRole("button", { name: "Simple" }));
    expect(within(modes).getByRole("button", { name: "Simple", pressed: true })).toBeTruthy();
    expect(within(modes).getByRole("button", { name: "Advanced", pressed: false })).toBeTruthy();
    await user.click(within(modes).getByRole("button", { name: "Advanced" }));
    expect(within(modes).getByRole("button", { name: "Advanced", pressed: true })).toBeTruthy();
    expect(within(modes).getByRole("button", { name: "Simple", pressed: false })).toBeTruthy();
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
    expect(screen.getByRole("button", { name: "Simple", pressed: true })).toBeTruthy();
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

  it("keeps a copy's name within 64 characters, counted as code points (item 6)", async () => {
    // 64 code points, 128 UTF-16 units: a cut by units would split a pair, and the core counts
    // code points (model.rs validate).
    const long = "👋".repeat(64);
    const cases = [
      { language: "en", heading: "Themes", duplicate: "Duplicate", sent: `My ${"👋".repeat(61)}` },
      // The name is shortened, not the words around it: a cut at the end would leave "… ของ".
      { language: "th", heading: "ธีม", duplicate: "ทำสำเนา", sent: `${"👋".repeat(57)} ของฉัน` },
    ] as const;
    for (const c of cases) {
      const { ipc, user, view } = await mountWith(
        async (m) => {
          await mineFrom(m, "Nord", long);
          vi.spyOn(m, "themeDuplicate");
        },
        { settings: { language: c.language } },
      );
      await screen.findByRole("heading", { name: c.heading });
      await user.click(within(await card(long)).getByRole("button", { name: c.duplicate }));
      await waitFor(() => expect(ipc.themeDuplicate).toHaveBeenCalledTimes(1));
      const sent = vi.mocked(ipc.themeDuplicate).mock.calls[0]?.[1] ?? "";
      expect(sent, c.language).toBe(c.sent);
      expect(Array.from(sent), c.language).toHaveLength(64);
      view.unmount();
    }
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

  it("keeps a draft that could not be saved again after a rename, and saves it on Try again (fix 2)", async () => {
    let refuse = true;
    const { ipc, user } = await mountWith(async (m) => {
      await mineFrom(m, "Nord", "My Nord", { author: "from the draft" });
      const save = m.themeSave.bind(m);
      vi.spyOn(m, "themeSave").mockImplementation(async (theme, draft) => {
        if (draft && refuse) throw ipcError("E_DATA_WRITE", "drafts folder is read-only");
        return save(theme, draft);
      });
    });
    await openThemes(user);
    await user.click(within(await card("My Nord")).getByRole("button", { name: "Rename" }));
    const dialog = await screen.findByRole("dialog");
    const field = within(dialog).getByLabelText("New name");
    await user.clear(field);
    await user.type(field, "Fjord");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await screen.findByText('The draft of "Fjord" could not be saved')).toBeTruthy();
    expect(screen.getByText(/Code E_DATA_WRITE/)).toBeTruthy();
    expect(ipc.state.mine.map((t) => t.name)).toEqual(["Fjord"]);
    expect(ipc.state.drafts).toEqual([]);
    refuse = false;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(ipc.state.drafts.map((d) => [d.name, d.author])).toEqual([["Fjord", "from the draft"]]));
    expect(screen.queryByText('The draft of "Fjord" could not be saved')).toBeNull();
  });

  it("offers no Apply on a draft, and Apply of the saved version on a theme with unsaved changes (fix 4)", async () => {
    const { user } = await mountWith(async (m) => {
      await mineFrom(m, "Nord", "My Nord", { author: "from the draft" });
      const base = (await m.themeGet(presetId("Paper"))).theme;
      await m.themeSave({ ...base, id: "0b4a9a3c-6f0e-4c1e-9d7a-2f5b8e1c3d4a", name: "Never saved" }, true);
    });
    await openThemes(user);
    const draft = await card("Never saved");
    expect(within(draft).getByText("Draft")).toBeTruthy();
    expect(within(draft).queryByRole("button", { name: /Apply/ })).toBeNull();
    expect(within(draft).getByRole("button", { name: "Edit" })).toBeTruthy();
    const mine = await card("My Nord");
    expect(within(mine).getByText("Unsaved changes")).toBeTruthy();
    expect(within(mine).getByRole("button", { name: "Apply the saved version" })).toBeTruthy();
    expect(within(await card("Nord")).getByRole("button", { name: "Apply" })).toBeTruthy();
  });

  it("says when the import's preview could not use this computer's details, and asks again next time (fix 3)", async () => {
    const { ipc, user } = await mountWith((m) => {
      const detect = m.systemDetect.bind(m);
      vi.spyOn(m, "systemDetect")
        .mockRejectedValueOnce(ipcError("E_MACHINE_READ", "the registry could not be read"))
        .mockImplementation(detect);
      vi.spyOn(m, "previewRender");
    });
    const render = vi.mocked(ipc.previewRender);
    await openThemes(user);
    await user.click(screen.getByRole("button", { name: "Import a theme file" }));
    let dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText("This computer's details could not be read, so the preview uses sample values.")).toBeTruthy();
    expect(render.mock.calls.at(-1)?.[1].user).toBe("you");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await user.click(screen.getByRole("button", { name: "Import a theme file" }));
    dialog = await screen.findByRole("dialog");
    await waitFor(() => expect(render.mock.calls.at(-1)?.[1].user).toBe("tester"));
    expect(within(dialog).queryByText("This computer's details could not be read, so the preview uses sample values.")).toBeNull();
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
    const undone = await screen.findByText("Undone: everything is as it was before.");
    expect(restore).toHaveBeenCalledWith(snapshotId);
    // The Undo button is gone with the apply: focus goes to what the undo said (fix 9).
    await waitFor(() => expect(document.activeElement).not.toBe(document.body));
    expect(document.activeElement?.contains(undone)).toBe(true);
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

  it("says nothing was undone when the very first undo was refused (fix 8)", async () => {
    const { ipc, user } = mount();
    await openThemes(user);
    await applyFromCard(user, "Nord");
    vi.spyOn(ipc, "snapshotRestore").mockResolvedValue({
      status: "stopped",
      undone: [],
      last: null,
      notes: [
        {
          code: "E_BUSY",
          messageTh: "(E_BUSY)",
          messageEn: "(E_BUSY)",
          details: "the next undo was refused, so the restore stopped there: another command is running",
        },
      ],
    });
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(await screen.findByText("Nothing was undone: the undo was refused. See why below.")).toBeTruthy();
    expect(screen.queryByText("The undo stopped part-way. See why below.")).toBeNull();
    expect(screen.getByText("(E_BUSY)")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Undo" })).toBeTruthy();
  });

  it("says how an undo went wrong once, by moving focus to it, with no live region in it (item 11)", async () => {
    const failure = { code: "E_WT_WRITE", messageTh: "(E_WT_WRITE)", messageEn: "(E_WT_WRITE)", details: "settings.json is locked" };
    const cases: { text: string; answer: (undid: string | null) => Promise<RestoreOutcome> }[] = [
      {
        text: "The undo did not go through, so nothing changed: the theme is still applied.",
        answer: async (undid) => ({
          status: "stopped",
          undone: [],
          last: { status: "rolledBack", undid, error: failure, unrestored: [], notes: [], fontsLeft: [] },
          notes: [],
        }),
      },
      {
        text: "The undo did not go through, and not everything could be put back. Fix these by hand:",
        answer: async (undid) => ({
          status: "stopped",
          undone: [],
          last: {
            status: "rollbackIncomplete",
            undid,
            error: failure,
            unrestored: [{ opIndex: 3, target: "Windows Terminal settings.json", code: "E_WT_WRITE", message: "locked", fix: "Close Windows Terminal.", fixTh: "ปิด Windows Terminal" }],
            notes: [],
            fontsLeft: [],
          },
          notes: [],
        }),
      },
      {
        text: "Nothing was undone: the undo was refused. See why below.",
        answer: async () => ({ status: "stopped", undone: [], last: null, notes: [{ ...failure, code: "E_BUSY" }] }),
      },
      {
        text: "This apply can no longer be undone.",
        answer: async () => ({ status: "notRestorable", undone: [], last: null, notes: [] }),
      },
      {
        text: "(E_SNAPSHOT_READ)",
        answer: async () => {
          throw ipcError("E_SNAPSHOT_READ", "manifest.json is not valid JSON");
        },
      },
    ];
    for (const c of cases) {
      const { ipc, user, view } = mount();
      await openThemes(user);
      await applyFromCard(user, "Nord");
      const undid = ipc.state.applied?.snapshotId ?? null;
      vi.spyOn(ipc, "snapshotRestore").mockImplementation(() => c.answer(undid));
      await user.click(screen.getByRole("button", { name: "Undo" }));
      const said = await screen.findByText(c.text);
      await waitFor(() => expect(document.activeElement?.contains(said), c.text).toBe(true));
      const box = document.activeElement as HTMLElement;
      expect(box.querySelectorAll('[role="alert"], [role="status"], [aria-live]'), c.text).toHaveLength(0);
      view.unmount();
    }
  });

  it("announces an apply in one live region only (fix 12)", async () => {
    const { user } = mount();
    await openThemes(user);
    await user.click(within(await card("Paper")).getByRole("button", { name: "Apply" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(await within(dialog).findByRole("button", { name: "Apply" }));
    await within(dialog).findByText(/Theme "Paper" applied/);
    // The bar is there too, with the same words, but only the dialog's outcome is live.
    expect(screen.getAllByText(/Theme "Paper" applied/)).toHaveLength(2);
    const live = Array.from(document.querySelectorAll('[role="status"], [role="alert"], [aria-live]')).filter((el) =>
      /Theme "Paper" applied/.test(el.textContent ?? ""),
    );
    expect(live).toHaveLength(1);
    expect(dialog.contains(live[0]!)).toBe(true);
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

  it("keeps a hand edit made while a palette answer was on its way (item 5)", async () => {
    const { user, held } = await mountHoldingPalette();
    await openThemes(user);
    await user.click(screen.getByRole("button", { name: "Create a theme" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    held.on = true;
    await user.click(screen.getByRole("radio", { name: "Light" }));
    await waitFor(() => expect(held.gates).toHaveLength(1));
    // Meanwhile the user sets a color by hand.
    await user.click(screen.getByRole("button", { name: "Advanced" }));
    fireEvent.change(screen.getByLabelText("Background"), { target: { value: "#123456" } });
    await act(async () => {
      held.gates[0]?.();
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    expect((screen.getByLabelText("Background") as HTMLInputElement).value).toBe("#123456");
    await user.click(screen.getByRole("button", { name: "Simple" }));
    expect((screen.getByRole("radio", { name: "Dark" }) as HTMLInputElement).checked).toBe(true);
  });

  it("lets the newest palette request win, whichever answer comes first (item 5)", async () => {
    for (const order of [
      [0, 1],
      [1, 0],
    ]) {
      const { user, held, view } = await mountHoldingPalette();
      await openThemes(user);
      await user.click(screen.getByRole("button", { name: "Create a theme" }));
      await screen.findByRole("heading", { name: "Edit theme" });
      held.on = true;
      // The label also holds the value shown beside the slider.
      const tint = screen.getByLabelText(/^Background tint/);
      fireEvent.change(tint, { target: { value: "0.2" } });
      fireEvent.change(tint, { target: { value: "0.25" } });
      await waitFor(() => expect(held.gates).toHaveLength(2));
      // One answer at a time, the page drawn after each.
      for (const i of order) {
        await act(async () => {
          held.gates[i]?.();
          await new Promise((resolve) => setTimeout(resolve, 20));
        });
      }
      expect(screen.queryByText("0.25"), `answers in the order ${order.join(", ")}`).not.toBeNull();
      expect(screen.queryByText("0.20"), `answers in the order ${order.join(", ")}`).toBeNull();
      view.unmount();
    }
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

  it("offers no draft back after the edit that made it was undone, and leaves no badge (item 1)", async () => {
    const { ipc, user } = mount();
    await customize(user, "Terracotta");
    await user.type(screen.getByLabelText("Author"), "Q");
    await waitFor(() => expect(ipc.state.drafts).toHaveLength(1), { timeout: 3000 });
    // Ctrl+Z outside the text field takes the editor back to the saved theme; then the user
    // leaves at once, before the draft second.
    await user.click(screen.getByRole("heading", { name: "Edit theme" }));
    await user.keyboard("{Control>}z{/Control}");
    await user.click(screen.getByRole("button", { name: "Back to themes" }));
    await screen.findByRole("heading", { name: "Themes" });
    await waitFor(() => expect(ipc.state.drafts).toEqual([]));
    const mine = await card("My Terracotta");
    await waitFor(() => expect(within(mine).queryByText("Unsaved changes")).toBeNull());
    // Opening it again offers nothing.
    await user.click(within(mine).getByRole("button", { name: "Edit" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("takes the draft away a second after its edit was undone, and writes it again on redo (item 1)", async () => {
    const { ipc, user } = mount();
    await customize(user, "Terracotta");
    await user.type(screen.getByLabelText("Author"), "Q");
    await waitFor(() => expect(ipc.state.drafts).toHaveLength(1), { timeout: 3000 });
    await user.click(screen.getByRole("heading", { name: "Edit theme" }));
    await user.keyboard("{Control>}z{/Control}");
    await waitFor(() => expect(ipc.state.drafts).toEqual([]), { timeout: 3000 });
    expect(screen.queryByText("Draft saved")).toBeNull();
    await user.keyboard("{Control>}y{/Control}");
    await waitFor(() => expect(ipc.state.drafts.map((d) => d.author.endsWith("Q"))).toEqual([true]), { timeout: 3000 });
  });

  it("keeps a new theme never saved when its edit is undone, with the content it was made with (round 2, fix 1)", async () => {
    const { ipc, user } = mount();
    await openThemes(user);
    await user.click(screen.getByRole("button", { name: "Create a theme" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    // The draft is the theme: there is no other copy of it.
    const created = structuredClone(ipc.state.drafts[0]!);
    await user.type(screen.getByLabelText("Author"), "Q");
    await waitFor(() => expect(ipc.state.drafts[0]?.author).toBe("Q"), { timeout: 3000 });
    await user.click(screen.getByRole("heading", { name: "Edit theme" }));
    await user.keyboard("{Control>}z{/Control}");
    await user.click(screen.getByRole("button", { name: "Back to themes" }));
    await screen.findByRole("heading", { name: "Themes" });
    await waitFor(() => expect(ipc.state.drafts).toEqual([created]));
    expect(within(await card(created.name)).getByText("Draft")).toBeTruthy();
  });

  it("keeps a reopened theme never saved as it was reopened, when its edit is undone (round 2, fix 1)", async () => {
    const { ipc, user } = await mountWith(async (m) => {
      const base = (await m.themeGet(presetId("Paper"))).theme;
      await m.themeSave({ ...base, id: "0b4a9a3c-6f0e-4c1e-9d7a-2f5b8e1c3d4a", name: "Never saved", author: "as reopened" }, true);
    });
    const reopened = structuredClone(ipc.state.drafts[0]!);
    await openThemes(user);
    await user.click(within(await card("Never saved")).getByRole("button", { name: "Edit" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    await user.type(screen.getByLabelText("Author"), "Q");
    await waitFor(() => expect(ipc.state.drafts[0]?.author).toBe("as reopenedQ"), { timeout: 3000 });
    await user.click(screen.getByRole("heading", { name: "Edit theme" }));
    await user.keyboard("{Control>}z{/Control}");
    // A second later the draft is the theme as reopened again: written back, never removed.
    await waitFor(() => expect(ipc.state.drafts).toEqual([reopened]), { timeout: 3000 });
    await user.click(screen.getByRole("button", { name: "Back to themes" }));
    await screen.findByRole("heading", { name: "Themes" });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(ipc.state.drafts).toEqual([reopened]);
    expect(within(await card("Never saved")).getByText("Draft")).toBeTruthy();
  });

  it("writes the loaded theme back when an undo comes while the autosave is still being written (round 3, C)", async () => {
    const { ipc, user, control } = await mountRefusingDrafts();
    control.refuse = false;
    await openThemes(user);
    await user.click(screen.getByRole("button", { name: "Create a theme" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    const created = structuredClone(ipc.state.drafts[0]!);
    control.holdNext = true;
    await user.type(screen.getByLabelText("Author"), "Q");
    // The autosave of "Q" has started and not landed; the user undoes and leaves meanwhile.
    await waitFor(() => expect(control.held).toBe(true), { timeout: 3000 });
    await user.click(screen.getByRole("heading", { name: "Edit theme" }));
    await user.keyboard("{Control>}z{/Control}");
    await user.click(screen.getByRole("button", { name: "Back to themes" }));
    await act(async () => {
      control.release(false);
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    await screen.findByRole("heading", { name: "Themes" });
    await waitFor(() => expect(ipc.state.drafts).toEqual([created]));
  });

  it("says a draft that could not be taken away, on the page and on the next screen, and takes it on Try again", async () => {
    let refuse = true;
    const { ipc, user } = await mountWith((m) => {
      const remove = m.themeDelete.bind(m);
      vi.spyOn(m, "themeDelete").mockImplementation(async (id, draftOnly) => {
        if (refuse) throw ipcError("E_DATA_WRITE", "drafts folder is read-only");
        return remove(id, draftOnly);
      });
    });
    await customize(user, "Terracotta");
    await user.type(screen.getByLabelText("Author"), "Q");
    await waitFor(() => expect(ipc.state.drafts).toHaveLength(1), { timeout: 3000 });
    await user.click(screen.getByRole("heading", { name: "Edit theme" }));
    await user.keyboard("{Control>}z{/Control}");
    expect(await screen.findByText("The old draft could not be removed", undefined, { timeout: 3000 })).toBeTruthy();
    // Nothing is lost by leaving: no "go back without the draft" here.
    expect(screen.queryByRole("button", { name: "Go back without saving the draft" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Themes" }));
    await screen.findByRole("heading", { name: "Themes" });
    expect(await screen.findByText('The old draft of "My Terracotta" could not be removed')).toBeTruthy();
    expect(screen.getByText(/Code E_DATA_WRITE/)).toBeTruthy();
    refuse = false;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(ipc.state.drafts).toEqual([]));
    expect(screen.queryByText('The old draft of "My Terracotta" could not be removed')).toBeNull();
    await waitFor(async () => expect(within(await card("My Terracotta")).queryByText("Unsaved changes")).toBeNull());
  });

  it("writes no draft over a save that is still running, whichever way the editor is left (item 2)", async () => {
    for (const way of ["Back to themes", "Themes"] as const) {
      let hold = false;
      let release: () => void = () => {};
      const gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      const { ipc, user, view } = await mountWith((m) => {
        const save = m.themeSave.bind(m);
        // A slow disk, and one command at a time in the order they came, as the backend.
        let queue: Promise<unknown> = Promise.resolve();
        vi.spyOn(m, "themeSave").mockImplementation((theme, draft) => {
          if (!hold) return save(theme, draft);
          const run = queue.then(() => gate).then(() => save(theme, draft));
          queue = run.catch(() => {});
          return run;
        });
      });
      await customize(user, "Terracotta");
      await user.type(screen.getByLabelText("Author"), "Q");
      hold = true;
      await user.click(screen.getByRole("button", { name: "Save" }));
      await user.click(screen.getByRole("button", { name: way }));
      release();
      await screen.findByRole("heading", { name: "Themes" });
      await new Promise((resolve) => setTimeout(resolve, 50));
      expect(ipc.state.mine[0]?.author.endsWith("Q"), way).toBe(true);
      expect(ipc.state.drafts, way).toEqual([]);
      view.unmount();
    }
  });

  it("refreshes the library once the draft written on a sidebar leave has landed (item 3)", async () => {
    const { ipc, user } = mount();
    await customize(user, "Terracotta");
    await user.type(screen.getByLabelText("Author"), "Q");
    await user.click(screen.getByRole("button", { name: "Themes" }));
    await screen.findByRole("heading", { name: "Themes" });
    await waitFor(() => expect(ipc.state.drafts).toHaveLength(1));
    expect(await within(await card("My Terracotta")).findByText("Unsaved changes")).toBeTruthy();
  });

  it("says on the next screen that a sidebar leave's draft could not be written, and writes it on Try again (item 4)", async () => {
    let refuse = true;
    const { ipc, user } = await mountWith((m) => {
      const save = m.themeSave.bind(m);
      vi.spyOn(m, "themeSave").mockImplementation(async (theme, draft) => {
        if (draft && refuse) throw ipcError("E_DATA_WRITE", "drafts folder is read-only");
        return save(theme, draft);
      });
    });
    await customize(user, "Terracotta");
    await user.type(screen.getByLabelText("Author"), "Q");
    await user.click(screen.getByRole("button", { name: "Themes" }));
    await screen.findByRole("heading", { name: "Themes" });
    expect(await screen.findByText('The draft of "My Terracotta" could not be saved')).toBeTruthy();
    expect(screen.getByText(/Code E_DATA_WRITE/)).toBeTruthy();
    expect(ipc.state.drafts).toEqual([]);
    // The edit is kept meanwhile: Try again writes it.
    refuse = false;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(ipc.state.drafts.map((d) => d.author.endsWith("Q"))).toEqual([true]));
    expect(screen.queryByText('The draft of "My Terracotta" could not be saved')).toBeNull();
    expect(await within(await card("My Terracotta")).findByText("Unsaved changes")).toBeTruthy();
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

describe("a draft problem said on the next screen (round 2, fix 2)", () => {
  it("is taken away once the editor of that theme writes a newer draft, so Try again cannot overwrite it", async () => {
    const { ipc, user, control } = await mountRefusingDrafts();
    await editAndLeave(user, "Terracotta");
    await screen.findByRole("button", { name: "Try again" });
    const stored = ipc.state.mine[0]!.author;
    control.refuse = false;
    await user.click(within(await card("My Terracotta")).getByRole("button", { name: "Edit" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    await user.type(screen.getByLabelText("Author"), "Z");
    await waitFor(() => expect(ipc.state.drafts.map((d) => d.author)).toEqual([`${stored}Z`]), { timeout: 3000 });
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
    expect(screen.queryByText(/Code E_DATA_WRITE/)).toBeNull();
  });

  it("stays dismissed when the Try again that ran meanwhile fails", async () => {
    const { user, control } = await mountRefusingDrafts();
    await editAndLeave(user, "Terracotta");
    const retry = await screen.findByRole("button", { name: "Try again" });
    control.holdNext = true;
    await user.click(retry);
    await waitFor(() => expect(control.held).toBe(true));
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    await act(async () => {
      control.release(true);
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
    expect(screen.queryByText(/Code E_DATA_WRITE/)).toBeNull();
  });

  it("keeps a newer problem of the same theme when an older Try again succeeds late", async () => {
    const { user, control } = await mountRefusingDrafts();
    await editAndLeave(user, "Terracotta");
    control.holdNext = true;
    await user.click(await screen.findByRole("button", { name: "Try again" }));
    await waitFor(() => expect(control.held).toBe(true));
    // Meanwhile the theme is opened again, edited and left, and that draft fails too.
    control.details = "the disk is full";
    await user.click(within(await card("My Terracotta")).getByRole("button", { name: "Edit" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    await user.type(screen.getByLabelText("Author"), "Z");
    await user.click(screen.getByRole("button", { name: "Themes" }));
    await screen.findByRole("heading", { name: "Themes" });
    expect(await screen.findByText("the disk is full")).toBeTruthy();
    await act(async () => {
      control.release(false);
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    expect(screen.getByText("the disk is full")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
  });

  it("keeps one problem per theme, each named, and Try again of one leaves the other", async () => {
    const { ipc, user, control } = await mountRefusingDrafts();
    await editAndLeave(user, "Terracotta");
    await editAndLeave(user, "Nord");
    const terracotta = await screen.findByRole("group", { name: 'The draft of "My Terracotta" could not be saved' });
    const nord = screen.getByRole("group", { name: 'The draft of "My Nord" could not be saved' });
    expect(screen.getAllByRole("button", { name: "Try again" })).toHaveLength(2);
    control.refuse = false;
    await user.click(within(nord).getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(ipc.state.drafts.map((d) => d.name)).toEqual(["My Nord"]));
    expect(screen.queryByRole("group", { name: 'The draft of "My Nord" could not be saved' })).toBeNull();
    expect(terracotta.isConnected).toBe(true);
  });

  it("is over once the user keeps the offered draft, so Try again cannot take it away (round 3, A)", async () => {
    let refuse = true;
    const { ipc, user } = await mountWith((m) => {
      const remove = m.themeDelete.bind(m);
      vi.spyOn(m, "themeDelete").mockImplementation(async (id, draftOnly) => {
        if (refuse) throw ipcError("E_DATA_WRITE", "drafts folder is read-only");
        return remove(id, draftOnly);
      });
    });
    await customize(user, "Terracotta");
    await user.type(screen.getByLabelText("Author"), "Q");
    await waitFor(() => expect(ipc.state.drafts).toHaveLength(1), { timeout: 3000 });
    // Undo, and leave at once: the old draft cannot be taken away.
    await user.click(screen.getByRole("heading", { name: "Edit theme" }));
    await user.keyboard("{Control>}z{/Control}");
    await user.click(screen.getByRole("button", { name: "Themes" }));
    await screen.findByRole("heading", { name: "Themes" });
    await screen.findByRole("button", { name: "Try again" });
    refuse = false;
    // The user opens the theme again and keeps the draft it offers.
    await user.click(within(await card("My Terracotta")).getByRole("button", { name: "Edit" }));
    const offer = await screen.findByRole("dialog");
    await user.click(within(offer).getByRole("button", { name: "Keep editing them" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const retry = screen.queryByRole("button", { name: "Try again" });
    if (retry) await user.click(retry);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(ipc.state.drafts.map((d) => d.author.endsWith("Q"))).toEqual([true]);
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  });

  it("does not make a theme deleted since again on Try again", async () => {
    const { ipc, user, control } = await mountRefusingDrafts();
    await editAndLeave(user, "Terracotta");
    const retry = await screen.findByRole("button", { name: "Try again" });
    control.refuse = false;
    // The theme goes away meanwhile, as from another window.
    await ipc.themeDelete(ipc.state.mine[0]!.id, false);
    await user.click(retry);
    await waitFor(() => expect(screen.queryByRole("button", { name: "Try again" })).toBeNull());
    expect(ipc.state.drafts).toEqual([]);
    expect(ipc.state.mine).toEqual([]);
  });
});
