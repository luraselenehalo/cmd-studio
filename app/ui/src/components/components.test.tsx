// The shared components on their own, against the mock IPC: what the screens' tests
// (Themes.test.tsx) do not pin. The dialog keeps every key typed into it and keeps focus
// inside; notes show their details; one predicate tells an AppError; the rollback has no
// step numbers; the plan lists the other add-on's commands it turns off; Try again asks about
// that add-on again; "Copy details" says whether it copied.
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { ThemeSummary } from "../bindings/ThemeSummary";
import { I18nProvider, type Language } from "../i18n";
import { IpcProvider } from "../ipc/context";
import { createMockIpc } from "../ipc/mock";
import { PRESETS } from "../ipc/presets";
import type { Ipc } from "../ipc/types";
import { ApplyFlow, ProgressView } from "./ApplyFlow";
import { Dialog } from "./Dialog";
import { ErrorNotice, NotesList } from "./Notices";
import { ThemeCard } from "./ThemeCard";

function mount(children: ReactNode, { ipc = createMockIpc(), language = "en" }: { ipc?: Ipc; language?: Language } = {}) {
  return render(
    <I18nProvider language={language} setLanguage={() => {}}>
      <IpcProvider ipc={ipc}>{children}</IpcProvider>
    </I18nProvider>,
  );
}

function presetId(name: string): string {
  const found = PRESETS.find((p) => p.name === name);
  if (!found) throw new Error(`no preset ${name}`);
  return found.id;
}

const REQUEST: ApplyRequest = {
  themeId: presetId("Nord"),
  target: "all",
  setWtDefaultProfile: true,
  replaceOtherClink: false,
};

const OTHER_CLINK = '"C:\\clink\\clink_x64.exe" inject --autorun';

/** A rename dialog whose parent re-renders on every key, with a new `onClose` each time. */
function Rename({ startOpen = true }: { startOpen?: boolean }) {
  const [open, setOpen] = useState(startOpen);
  const [name, setName] = useState("My Terracotta");
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Rename
      </button>
      {open && (
        <Dialog title="Rename the theme" onClose={() => setOpen(false)}>
          <label>
            New name
            <input value={name} onChange={(e) => setName(e.target.value)} />
          </label>
        </Dialog>
      )}
    </>
  );
}

describe("Dialog", () => {
  it("keeps both characters typed into a field while its parent re-renders", async () => {
    const user = userEvent.setup();
    mount(<Rename />);
    const field = screen.getByLabelText("New name") as HTMLInputElement;
    await user.type(field, "XY");
    expect(field.value).toBe("My TerracottaXY");
    expect(document.activeElement).toBe(field);
  });

  it("keeps Tab and Shift+Tab inside, closes on Escape and gives focus back", async () => {
    const user = userEvent.setup();
    mount(<Rename startOpen={false} />);
    const opener = screen.getByRole("button", { name: "Rename" });
    await user.click(opener);
    const dialog = screen.getByRole("dialog", { name: "Rename the theme" });
    const field = within(dialog).getByLabelText("New name");
    const close = within(dialog).getByRole("button", { name: "Close" });
    // The dialog itself has focus first; Shift+Tab from it goes to its last control.
    expect(document.activeElement).toBe(dialog);
    await user.tab({ shift: true });
    expect(document.activeElement).toBe(close);
    await user.tab();
    expect(document.activeElement).toBe(field);
    await user.tab({ shift: true });
    expect(document.activeElement).toBe(close);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });
});

describe("Notices", () => {
  it("shows a note's details under its message, in Thai as in English", () => {
    mount(
      <NotesList
        notes={[
          {
            code: "E_SNAPSHOT_READ",
            messageTh: "อ่านจุดย้อนกลับไม่ได้",
            messageEn: "A restore point could not be read",
            details: "restore point 1791694709000-abcd1234: manifest.json is not valid JSON",
          },
          { code: "E_RECOVERED", messageTh: "คืนค่าที่ค้างไว้แล้ว", messageEn: "An unfinished apply was put back", details: "" },
        ]}
      />,
      { language: "th" },
    );
    const first = screen.getByText("อ่านจุดย้อนกลับไม่ได้").closest("li") as HTMLElement;
    expect(within(first).getByText(/manifest\.json is not valid JSON/)).toBeTruthy();
    expect(within(first).getByText("ดูรายละเอียด")).toBeTruthy();
    // A note with no details has no empty toggle.
    const second = screen.getByText("คืนค่าที่ค้างไว้แล้ว").closest("li") as HTMLElement;
    expect(within(second).queryByText("ดูรายละเอียด")).toBeNull();
  });

  it("tells an AppError by all four fields, so a partial one is an internal problem, not a crash", () => {
    mount(<ErrorNotice error={{ code: "E_WT_PARSE", messageEn: "Windows Terminal's settings could not be read" }} />, {
      language: "th",
    });
    expect(screen.getByText(/รหัส E_INTERNAL/)).toBeTruthy();
    expect(screen.getByText(/CMD Studio เองมีปัญหา/)).toBeTruthy();
    // What was thrown is kept in the details.
    expect(screen.getByText(/"code":"E_WT_PARSE"/)).toBeTruthy();
  });
});

describe("ProgressView", () => {
  it("counts an apply's steps, the state.json write included", () => {
    const { container } = mount(<ProgressView progress={{ phase: "applying", step: 3, total: 7, kind: "writeGenerated" }} />);
    expect(screen.getByText("Applying (3 of 7)")).toBeTruthy();
    const bar = container.querySelector("progress") as HTMLProgressElement;
    expect(bar.getAttribute("value")).toBe("3");
    expect(bar.getAttribute("max")).toBe("7");
  });

  it("draws putting things back without step numbers and with a bar that claims no fraction", () => {
    const progress = { phase: "rollingBack", step: 2, total: 7, kind: null } as const;
    const en = mount(<ProgressView progress={progress} />);
    expect(screen.getByText("Putting things back…")).toBeTruthy();
    expect(screen.queryByText(/2/)).toBeNull();
    expect(en.container.querySelector("progress")?.hasAttribute("value")).toBe(false);
    en.unmount();
    const th = mount(<ProgressView progress={progress} />, { language: "th" });
    expect(screen.getByText("กำลังคืนค่าเดิม…")).toBeTruthy();
    expect(th.container.querySelector("progress")?.hasAttribute("value")).toBe(false);
  });
});

describe("ThemeCard", () => {
  it("draws the eight swatches from the summary's own colors", () => {
    const preset = PRESETS.find((p) => p.name === "Nord")!;
    const summary: ThemeSummary = {
      id: preset.id,
      name: preset.name,
      tone: preset.tone,
      layout: preset.prompt.layout,
      colors: preset.colors,
      source: "preset",
      inUse: true,
      hasDraft: false,
    };
    const { container } = mount(<ThemeCard summary={summary} />);
    expect(container.querySelectorAll(".swatches span")).toHaveLength(8);
    expect(screen.getByText("In use")).toBeTruthy();
    expect(screen.getByText("Built in")).toBeTruthy();
  });
});

describe("ApplyFlow", () => {
  it("lists in the plan the other add-on's commands it turns off, then applies", async () => {
    const ipc = createMockIpc({ otherClink: [OTHER_CLINK] });
    const onApplied = vi.fn();
    const user = userEvent.setup();
    mount(<ApplyFlow request={REQUEST} onApplied={onApplied} />, { ipc });
    await screen.findByRole("heading", { name: "Another cmd add-on is already set up" });
    await user.click(screen.getByRole("button", { name: "Use CMD Studio's instead" }));
    await screen.findByRole("button", { name: "Apply" });
    const intro = screen.getByText("Turning off the other cmd add-on takes these commands out of cmd's start-up:");
    expect(within(intro.parentElement as HTMLElement).getByText(OTHER_CLINK)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await screen.findByText(/Theme "Nord" applied/);
    expect(onApplied).toHaveBeenCalledTimes(1);
    expect(onApplied.mock.calls[0]?.[0]).toMatchObject({ status: "committed", themeName: "Nord" });
  });

  it("asks about the other add-on again when a failed apply is tried again", async () => {
    const ipc = createMockIpc({ otherClink: [OTHER_CLINK], failApplyWith: "E_VERIFY_OMP" });
    const onApplied = vi.fn();
    const user = userEvent.setup();
    mount(<ApplyFlow request={REQUEST} onApplied={onApplied} />, { ipc });
    await user.click(await screen.findByRole("button", { name: "Use CMD Studio's instead" }));
    await user.click(await screen.findByRole("button", { name: "Apply" }));
    await screen.findByText("The theme could not be applied. Everything was put back as it was.");
    // A rolled-back outcome still carries its snapshotId; it is not an apply anyone may undo.
    expect(onApplied).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await screen.findByRole("heading", { name: "Another cmd add-on is already set up" });
  });

  it("asks about the other add-on again when a plan that failed is tried again", async () => {
    const ipc = createMockIpc({ otherClink: [OTHER_CLINK] });
    const planApply = ipc.planApply.bind(ipc);
    let failNext = false;
    ipc.planApply = async (request) => {
      if (request.replaceOtherClink && !failNext) {
        failNext = true;
        throw new Error("the plan could not be made");
      }
      return planApply(request);
    };
    const user = userEvent.setup();
    mount(<ApplyFlow request={REQUEST} />, { ipc });
    await user.click(await screen.findByRole("button", { name: "Use CMD Studio's instead" }));
    await screen.findByText(/Code E_INTERNAL/);
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await screen.findByRole("heading", { name: "Another cmd add-on is already set up" });
  });

  it("says the details were copied, and says so when they could not be", async () => {
    const ipc = createMockIpc({ failApplyWith: "E_VERIFY_OMP" });
    const user = userEvent.setup();
    mount(<ApplyFlow request={REQUEST} />, { ipc });
    await user.click(await screen.findByRole("button", { name: "Apply" }));
    const copy = await screen.findByRole("button", { name: "Copy details for a problem report" });
    await user.click(copy);
    expect(await screen.findByText("Copied")).toBeTruthy();
    expect(await navigator.clipboard.readText()).toMatch(/^CMD Studio diagnostics/);
    ipc.diagnosticsCopy = async () => {
      throw new Error("the log could not be read");
    };
    await user.click(copy);
    expect(await screen.findByText("The details could not be copied")).toBeTruthy();
    expect(screen.queryByText("Copied")).toBeNull();
  });
});
