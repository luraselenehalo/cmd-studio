# Phase 2b (3 of 3) — The Wizard, Home, Settings, Help and Advanced; the final shell; the VM check

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This is the last of the three sub-plans indexed by `2026-10-16-phase2b-app.md`; it needs the shell plan and the themes-UI plan done and CI's `app` job green.

**Goal:** Finish the app of spec §4: the first-run wizard (six pages, five clicks from start to a themed cmd, no typing), the Home page with its real status, the Settings page (where themes go, language, update mode), Help (health check with fixes, restore points, undo everything, report a problem, the short guide, credits and licenses), Advanced (versions, log, data folder, generated files), the final shell with the after-apply bar, the whole-app tests in both languages, the spec's appendix B rows, and the protocol for the controller's VM check of spec §1 criterion 1 (spec §8: phase 3 ends when it passes on the Windows 11 VM).

**Architecture:** The screens are plain components over the `Ipc`, `AppState` and `Nav` contexts of the themes-UI plan; nothing new in the plumbing. The final `App.tsx` loads the settings once, opens in English (the saved language takes over when it is Thai), shows the wizard until `wizardDone`, then the five pages behind a sidebar with the after-apply bar on top; the main heading takes focus on every route change (spec §4.5). Every change to the machine — the wizard's page 5, the home page's quick switch, Settings' "Apply again", the doctor's fixes — goes through the same `ApplyFlow`. Help's tabs are routes (`{name: "help", tab}`), so a "Restore points" button on Home lands on the right tab.

**Tech Stack:** as the themes-UI plan; no new package.

**Spec:** `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` §1 (criterion 1), §2.1, §4.1–§4.5, §5.8 (the doctor's fixes), §6.6 (credits), §7 (the VM tiers), §8 (phase 3's exit), appendix B; founder 2026-10-16 (English always; updates default automatic, stored and shown here, performed in Phase 4). Index plan Decisions 8, 11, 13.

**Prerequisite:** the two earlier sub-plans merged into the branch; `npm test` 20 passed; CI's `app` job green (the Tauri build of `ipc.rs`, `lib.rs`, `main.rs` and the config). The code of every task below was type-checked and run in the scratch copy: `App.test.tsx` 12 passed, the whole set 32.

## Global Constraints

Every task's requirements include these. Values are copied from the spec, the founder's decisions and the earlier plans' Global Constraints (`docs/superpowers/plans/2026-10-10-phase1a-core.md`, `2026-10-11-phase1b-system.md`, `2026-10-13-phase2a-preview.md`) where they bind here.

- Repository `C:\Users\<you>\cmd-studio` (written `<repo>` below), branch `phase2b-app`, cut from main `061668a`. Rust `1.99.0` (`rust-toolchain.toml`), edition 2021, license MIT. Pinned runtimes: Oh My Posh `31.5.0`, Clink `1.9.34`, Nerd Fonts `3.5.1` (`CaskaydiaCove NF`).
- **Language (founder 2026-10-13 and 2026-10-16; spec §1, §4.5, appendix B).** Nothing CMD Studio writes into a cmd window is Thai: the app writes nothing new into cmd, and `crates/core/tests/english_in_cmd.rs` must keep passing. The app's UI **always opens in English**; Thai is a choice on the wizard's first page and in Settings, and the app remembers it in `settings.json` (`language`, default `en`). Every UI string lives in both dictionaries (`app/ui/src/i18n/en.ts`, `th.ts`); `i18n.test.ts` fails on a missing key, an empty text, Thai in the English dictionary or English-only text in the Thai one.
- **One exe (spec §5.7).** `cmdstudio.exe` is the app; started with any argument it runs `cmdstudio_cli::run` and exits with its code. `crates/cli` is a library only.
- **IPC (spec §5.7, §5.9, §5.10).** The commands are exactly the §5.7 list plus `preview_render`, `open_external` and `advanced_read` (27 names, `app/src-tauri/src/command_names.rs`): thin wrappers over `Backend`, no logic of their own. Every failure crosses the IPC as `{code, messageTh, messageEn, details}` (`AppError`); the codes are the rows of spec §5.9, no other. `capabilities/default.json` allows exactly these commands and the two event permissions; the CSP is `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; font-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'self'; frame-ancestors 'none'` (no remote origin); the app is single-instance (a second start focuses the window).
- **No palette, contrast or prompt logic in TypeScript (spec §5.6).** The UI paints the spans the core returns and shows the colors `palette_generate` / `contrast_fix` return. The test mock (`app/ui/src/ipc/mock.ts`) answers with fixed data taken from the presets, never with a computation of its own.
- **`--cleanup` stays true.** Every entry the app writes under the data folder (`drafts\`, `logs\`, `webview\`, `settings.json`, the set-aside `settings.json.damaged-<ms>` and the temp file `.settings.json.<hex>.cmdstudio-tmp`) is known to `cmdstudio_cli`'s `put_there_by_cmd_studio`, so a folder the app used is still "every entry is CMD Studio's".
- **Production code** (everything outside `#[cfg(test)]` and `tests/`): no `unwrap`, `expect`, `panic!`, unchecked indexing or slicing, string slicing, or `serde_json::Value` IndexMut; the workspace denies clippy `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `string_slice` and `undocumented_unsafe_blocks` (ruling R1); the app crate inherits `[lints] workspace = true`. Errors are typed. Every `unsafe` block (the Win32 calls in `console.rs` and `open.rs`) carries a `// SAFETY:` comment.
- **Test safety.** No test touches this computer's AutoRun, fonts, Windows Terminal settings, `%LOCALAPPDATA%\ai.resincore.cmdstudio` or `%ProgramData%`: backend tests run over the core's `FakeMachine` (feature `fake`), UI tests over the in-memory mock in jsdom. The visual check is the Vite dev server with `VITE_MOCK_IPC=1` in the built-in browser pane. Never run the built app or `cmdstudio.exe` on the dev PC; real-app runs happen only in the VMs, by the controller. Never set `CMDSTUDIO_SYSTEM_TESTS`, `CMDSTUDIO_OMP_TESTS` or `CMDSTUDIO_ALLOW_PARITY_SKIP` on the dev PC; `cmd.exe` only with `/d`.
- **Windows Application Control.** This PC intermittently blocks a freshly built unsigned binary (a build script, a test exe) with `os error 4551`. Rerun the identical command once. If it is blocked again, do not work around it (no signing, no policy change, no other toolchain): mark the step "not compiled locally; CI is the gate" in the commit message and go on; CI runs every step of this plan.
- **Pins.** Every direct dependency is exact: `= "x.y.z"` in Cargo manifests, `"x.y.z"` in `package.json`; `Cargo.lock` and `app/package-lock.json` are committed; Node is pinned by `app/.nvmrc` (`24.21.0`) and `engines`. In the repository only `npm ci`, never `npm install`; experiments run in a scratch copy with `npm_config_cache` under the scratchpad.
- **JSON** CMD Studio writes or sends over the IPC uses camelCase (ruling P21): `#[serde(rename_all = "camelCase")]` on every DTO, and ts-rs follows serde. The bindings under `app/ui/src/bindings` are generated by `scripts/export-bindings.ps1` and committed; CI fails when they are stale.
- **Each Rust task ends clean:** `cargo fmt --all`, then `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings` and `cargo clippy -p cmdstudio-core --all-targets --features ts -- -D warnings`. Exception, stated where it applies: while the app crate is being built up (shell plan Tasks 8–13) its clippy reports dead code for items whose users come in a later task of that plan — exactly the items each task's Expected line names, nothing else; from Task 14 on the app crate is clean too. **Each UI task ends clean:** `npm run typecheck` and `npm test` in `app/`.
- **The spec** (`docs/superpowers/specs/2026-10-10-cmd-studio-design.md`) changes only in the tasks that name the change, and the working copy on the branch already holds the coordinator's uncommitted edits of 2026-10-16 (§2.1, §4.3, §4.5, §6.1, §6.2, §6.4, §8, appendix B): edit the file as it is, never from `061668a`'s version.
- **Commits:** write the English message to `.git/CMDSTUDIO_COMMIT_MSG` with the Write tool, then `git commit -F .git/CMDSTUDIO_COMMIT_MSG`. Stage only the task's files with `git add -- <files>` and check `git diff --cached --name-only`. Never add `Co-Authored-By`, "Generated with" or any Claude/claude.ai line; never `--no-verify`; never push. Follow the `auroraz-pr-workflow` skill.
- Scratch copies and the check crate use their own `CARGO_TARGET_DIR`, never the repository's `target/`. No user name of this PC appears in a committed file or a plan: tests use `C:\Users\tester\...`, docs use `<you>` or `%USERPROFILE%`.

## Review Focus

The five of the index plan that this plan's code owns, each with the test that pins it here.

1. **The first run must be five clicks and no typing** (spec §1 criterion 1, §4.2), starting in English, Terracotta preselected, "every cmd window" preselected. Task 2: `opens in English and applies Terracotta in five clicks with no typing` (it counts the `apply` calls and checks the request).
2. **Thai chosen on page 1 is remembered**, and the whole UI switches at once; "Skip, set up myself" lands on the Thai home page. Task 2: `switches to Thai on page 1 and remembers it`.
3. **The defaults are the founder's:** English, every window, automatic updates; a change is saved. Task 2: `keeps the defaults in Settings: English, every window, automatic updates`.
4. **A doctor finding offers the right fix and the fix goes through the plan;** "Undo everything" asks first and says what it did. Task 2: `runs the health check, offers a fix, and undoes everything`, `lists restore points and goes back to one`.
5. **Every button on every screen has a name, in English and in Thai** (spec §4.5: screen readers read the labels in both). Task 2: `names every button on every screen, in English and in Thai`.

## File map

```
app/ui/src/screens/Help.tsx, credits.ts, Advanced.tsx              (Task 1)
app/ui/src/screens/Wizard.tsx, Home.tsx, Settings.tsx              (Task 2)
app/ui/src/App.tsx (final, replaces the interim), App.test.tsx     (Task 2)
docs/superpowers/specs/2026-10-10-cmd-studio-design.md            appendix B rows (Task 3)
.superpowers/sdd/phase2b-vm-check.md                               the VM protocol for the controller (Task 4)
```

Commands run in `<repo>\app` unless said otherwise.

---

### Task 1: Help (six tabs) and Advanced

Spec §4.3's last two rows and §6.6. Help: the health check (`doctorRun`, each check with its status, bilingual message, the code, "Details" with `detailsTh` when Thai, and a fix button when the fix is one the UI can run — reapply and replace-other-Clink open the plan dialog, install Windows Terminal opens the Store, open a new cmd window opens cmd; the other kinds are shown as text), the restore points (newest first, "Go back to this point" after a confirmation, the outcome with what was undone), "Undo everything" (confirmation, then every step's result), "Report a problem" (the diagnostics text, "Copy details" to the clipboard, "Open GitHub Issues"), the guide (five paragraphs), credits and licenses (`credits.ts`: the bundled programs and font with versions and licenses, the color schemes the presets use, the SignPath line). Advanced: the versions table, the data folder with "Open the data folder" and "Open the logs folder", the log's tail, the generated files behind toggles — read only; the one page that names Oh My Posh and Clink (spec §4.5's words table).

**Files:**
- Create: `app/ui/src/screens/Help.tsx`, `app/ui/src/screens/credits.ts`, `app/ui/src/screens/Advanced.tsx`
- Test: `npm run typecheck` here; Task 2's `App.test.tsx` exercises them (`runs the health check, offers a fix, and undoes everything`, `lists restore points and goes back to one`, `copies the diagnostics for a problem report`, `shows the add-ons' versions on the Advanced page`, and the accessibility sweep over every tab).

**Interfaces:**
- Consumes: `useIpc`, `useAppState`, `useNav` + `HelpTab`, `ApplyFlow`, `Dialog`, `ErrorNotice`, `Notice`, `NotesList`, `UnrestoredList`, `formatTime`, `pick`, the bindings `CheckReport`, `DoctorReport`, `FixKind`, `RestoreOutcome`, `RestorePoints`, `RevertOutcome`, `AdvancedInfo`.
- Produces: `export function Help(props: { tab: HelpTab }): JSX.Element`; `export interface Credit { name; version?; license; copyright; source }`, `export const BUNDLED: Credit[]` (Clink 1.9.34 GPL-3.0, Oh My Posh 31.5.0 MIT, CaskaydiaCove Nerd Font / Nerd Fonts 3.5.1 SIL OFL 1.1), `export const SCHEMES: Credit[]` (Tokyo Night, Catppuccin, Dracula, Nord, Gruvbox, One Dark, Solarized Light, and CMD Studio's own four); `export function Advanced(): JSX.Element`.

- [ ] **Step 1: Write `app/ui/src/screens/credits.ts`** (the data is `docs/credits.md`'s, pinned by Phase 1b/2a)

```ts
// The credits page's data (spec §6.6), from docs/credits.md: the bundled programs and font,
// and the color schemes the built-in themes use, each with its license and copyright line.
export interface Credit {
  name: string;
  version?: string;
  license: string;
  copyright: string;
  source: string;
}

export const BUNDLED: Credit[] = [
  {
    name: "Clink",
    version: "1.9.34",
    license: "GPL-3.0",
    copyright: "Martin Ridgers and contributors",
    source: "https://github.com/chrisant996/clink/tree/v1.9.34",
  },
  {
    name: "Oh My Posh",
    version: "31.5.0",
    license: "MIT",
    copyright: "Copyright 2022 Jan De Dobbeleer",
    source: "https://github.com/JanDeDobbeleer/oh-my-posh",
  },
  {
    name: "CaskaydiaCove Nerd Font",
    version: "Nerd Fonts 3.5.1",
    license: "SIL OFL 1.1",
    copyright:
      "Copyright (c) 2019 - Present, Microsoft Corporation (Cascadia Code); Copyright (c) 2014 Ryan L McIntyre (Nerd Fonts)",
    source: "https://github.com/ryanoasis/nerd-fonts",
  },
];

export const SCHEMES: Credit[] = [
  {
    name: "Tokyo Night",
    license: "MIT",
    copyright: "Copyright (c) 2018-present Enkia",
    source: "https://github.com/tokyo-night/tokyo-night-vscode-theme",
  },
  {
    name: "Catppuccin Mocha, Catppuccin Latte",
    license: "MIT",
    copyright: "Copyright (c) 2021 Catppuccin",
    source: "https://github.com/catppuccin/windows-terminal",
  },
  {
    name: "Dracula",
    license: "MIT",
    copyright: "Copyright (c) 2019 thismat (the Windows Terminal scheme); Copyright (c) 2023 Dracula Theme (the palette's names)",
    source: "https://github.com/dracula/windows-terminal",
  },
  {
    name: "Nord",
    license: "MIT",
    copyright: "Copyright (c) 2016-present Sven Greb <development@svengreb.de> (https://www.svengreb.de)",
    source: "https://github.com/nordtheme/alacritty",
  },
  {
    name: "Gruvbox",
    license: "MIT",
    copyright: "Copyright (c) 2018 Pavel Pertsev",
    source: "https://github.com/gruvbox-community/gruvbox",
  },
  {
    name: "One Dark",
    license: "MIT",
    copyright: "Copyright (c) 2016 GitHub Inc.",
    source: "https://github.com/atom/one-dark-syntax",
  },
  {
    name: "Solarized Light",
    license: "MIT",
    copyright: "Copyright (c) 2011 Ethan Schoonover",
    source: "https://github.com/altercation/solarized",
  },
  {
    name: "Terracotta, Retro green, Daylight, Paper",
    license: "MIT",
    copyright: "CMD Studio contributors",
    source: "https://github.com/luraselenehalo/cmd-studio",
  },
];
```

- [ ] **Step 2: Write `app/ui/src/screens/Help.tsx`**

```tsx
// Help (spec §4.3): the health check with a fix for each finding, every restore point with
// "Go back to this point", "Undo everything", "Report a problem" (diagnostics copied with the
// user name redacted + GitHub Issues), the short guide, and the credits and licenses (spec §6.6).
import { useCallback, useEffect, useState } from "react";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { CheckReport } from "../bindings/CheckReport";
import type { DoctorReport } from "../bindings/DoctorReport";
import type { FixKind } from "../bindings/FixKind";
import type { RestoreOutcome } from "../bindings/RestoreOutcome";
import type { RestorePoints } from "../bindings/RestorePoints";
import type { RevertOutcome } from "../bindings/RevertOutcome";
import { ApplyFlow } from "../components/ApplyFlow";
import { Dialog } from "../components/Dialog";
import { ErrorNotice, Notice, NotesList, UnrestoredList } from "../components/Notices";
import { formatTime, pick, useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import { useNav, type HelpTab } from "../router";
import { useAppState } from "../state";
import { BUNDLED, SCHEMES, type Credit } from "./credits";

const TABS: HelpTab[] = ["doctor", "restore", "revert", "report", "guide", "credits"];

export function Help({ tab }: { tab: HelpTab }) {
  const { t } = useI18n();
  const { go } = useNav();
  return (
    <section>
      <h1 tabIndex={-1}>{t("help.title")}</h1>
      <div className="tabs" role="tablist">
        {TABS.map((name) => (
          <button key={name} type="button" role="tab" aria-selected={tab === name} onClick={() => go({ name: "help", tab: name })}>
            {t(`help.tabs.${name}`)}
          </button>
        ))}
      </div>
      <div role="tabpanel">
        {tab === "doctor" && <Doctor />}
        {tab === "restore" && <RestorePointsTab />}
        {tab === "revert" && <Revert />}
        {tab === "report" && <Report />}
        {tab === "guide" && <Guide />}
        {tab === "credits" && <Credits />}
      </div>
    </section>
  );
}

function Doctor() {
  const ipc = useIpc();
  const { t, language } = useI18n();
  const { version, bump, setLastApply } = useAppState();
  const [report, setReport] = useState<DoctorReport | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [applying, setApplying] = useState<ApplyRequest | null>(null);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let alive = true;
    setError(null);
    ipc
      .doctorRun()
      .then((r) => alive && setReport(r))
      .catch((e: unknown) => alive && setError(e));
    return () => {
      alive = false;
    };
  }, [ipc, version, tick]);

  const fix = async (check: CheckReport) => {
    setError(null);
    try {
      const proposal = await ipc.doctorFix(check.id);
      switch (proposal.kind) {
        case "reapply":
        case "replaceOtherClink":
          if (proposal.request) setApplying(proposal.request);
          break;
        case "installWindowsTerminal":
          await ipc.openExternal("windowsTerminalStore");
          break;
        case "openNewCmdWindow":
          await ipc.openCmd();
          break;
        default:
          break;
      }
    } catch (e) {
      setError(e);
    }
  };

  const fixLabel = (kind: FixKind) => t(`help.doctor.fix.${kind}`);
  const actionable = (kind: FixKind | null) =>
    kind === "reapply" || kind === "replaceOtherClink" || kind === "installWindowsTerminal" || kind === "openNewCmdWindow";

  return (
    <div>
      <div className="row-actions">
        <button type="button" onClick={() => setTick((n) => n + 1)}>
          {t("help.doctor.run")}
        </button>
        {report && <span>{t("help.doctor.checkedAt", { time: formatTime(report.checkedAtMs, language) })}</span>}
      </div>
      {error !== null && <ErrorNotice error={error} />}
      {!report && error === null && <p role="status">{t("common.loading")}</p>}
      {report && (
        <ul className="check-list">
          {report.checks.map((c) => (
            <li key={c.id}>
              <div>
                <strong className={`status-${c.status}`}>
                  {c.id} · {t(`help.doctor.status.${c.status}`)}
                </strong>{" "}
                {pick(language, c.messageTh, c.messageEn)}
                {c.code && <span className="badge">{c.code}</span>}
              </div>
              {(c.details || c.detailsTh) && (
                <details>
                  <summary>{t("common.details")}</summary>
                  <pre style={{ whiteSpace: "pre-wrap" }}>{language === "th" && c.detailsTh ? c.detailsTh : c.details}</pre>
                </details>
              )}
              {c.fix && (
                <div className="row-actions">
                  {actionable(c.fix) ? (
                    <button type="button" onClick={() => void fix(c)}>
                      {fixLabel(c.fix)}
                    </button>
                  ) : (
                    <span>{fixLabel(c.fix)}</span>
                  )}
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
      {applying && (
        <Dialog title={t("apply.planTitle")} onClose={() => setApplying(null)}>
          <ApplyFlow
            titled={false}
            request={applying}
            onCancel={() => setApplying(null)}
            onApplied={(o) => {
              setLastApply(o);
              bump();
              setApplying(null);
            }}
          />
        </Dialog>
      )}
    </div>
  );
}

function RestorePointsTab() {
  const ipc = useIpc();
  const { t, language } = useI18n();
  const { version, bump, setLastApply } = useAppState();
  const [points, setPoints] = useState<RestorePoints | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [confirming, setConfirming] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<RestoreOutcome | null>(null);

  const load = useCallback(() => {
    setError(null);
    ipc
      .snapshotsList()
      .then(setPoints)
      .catch((e: unknown) => setError(e));
  }, [ipc]);

  useEffect(() => {
    load();
  }, [load, version]);

  const restore = async (id: string) => {
    setConfirming(null);
    setError(null);
    try {
      const result = await ipc.snapshotRestore(id);
      setOutcome(result);
      if (result.undone.length > 0) {
        setLastApply(null);
        bump();
      }
    } catch (e) {
      setError(e);
    }
  };

  return (
    <div>
      <h2>{t("help.restore.title")}</h2>
      {error !== null && <ErrorNotice error={error} />}
      {outcome && (
        <Notice kind={outcome.status === "done" ? "ok" : "warn"}>
          {t(`help.restore.${outcome.status}`)}
          {outcome.last?.error && <ErrorNotice error={outcome.last.error} />}
          {outcome.last && <UnrestoredList steps={outcome.last.unrestored} />}
          {outcome.last && <NotesList notes={outcome.last.notes} />}
        </Notice>
      )}
      {points && points.problems.length > 0 && (
        <Notice kind="warn">
          {t("help.restore.problems", { n: points.problems.length })}
          <NotesList notes={points.problems} />
        </Notice>
      )}
      {points && points.points.length === 0 && <p>{t("help.restore.none")}</p>}
      {points && points.points.length > 0 && (
        <ul className="check-list">
          {points.points.map((p) => (
            <li key={p.id}>
              <div>
                <strong>{t(`help.restore.kind.${p.kind}`)}</strong>
                {p.theme && ` · ${p.theme.name}`} · {formatTime(p.createdMs, language)}
                {p.undoneBy && <span className="badge">{t("help.restore.undone")}</span>}
              </div>
              <small>{p.id}</small>
              {p.restorable && (
                <div className="row-actions">
                  <button type="button" onClick={() => setConfirming(p.id)}>
                    {t("help.restore.restore")}
                  </button>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
      {confirming && (
        <Dialog title={t("help.restore.restore")} onClose={() => setConfirming(null)}>
          <p>{t("help.restore.confirm")}</p>
          <div className="row-actions">
            <button type="button" className="primary" onClick={() => void restore(confirming)}>
              {t("help.restore.restore")}
            </button>
          </div>
        </Dialog>
      )}
    </div>
  );
}

function Revert() {
  const ipc = useIpc();
  const { t, language } = useI18n();
  const { bump, setLastApply } = useAppState();
  const [confirming, setConfirming] = useState(false);
  const [outcome, setOutcome] = useState<RevertOutcome | null>(null);
  const [error, setError] = useState<unknown>(null);

  const revert = async () => {
    setConfirming(false);
    setError(null);
    try {
      const result = await ipc.revertAll();
      setOutcome(result);
      setLastApply(null);
      bump();
    } catch (e) {
      setError(e);
    }
  };

  return (
    <div>
      <h2>{t("help.revert.title")}</h2>
      <p>{t("help.revert.text")}</p>
      <button type="button" className="danger" onClick={() => setConfirming(true)}>
        {t("help.revert.button")}
      </button>
      {error !== null && <ErrorNotice error={error} />}
      {outcome && (
        <div>
          <Notice kind={outcome.ok ? "ok" : "warn"}>{outcome.ok ? t("help.revert.done") : t("help.revert.notOk")}</Notice>
          {outcome.error && <ErrorNotice error={outcome.error} />}
          <ul className="check-list">
            {outcome.steps.map((s, i) => (
              <li key={i} className={s.ok ? "status-ok" : "status-fail"}>
                {s.what}: {s.detail}
                {!s.ok && (s.fixEn || s.fixTh) && <div>{pick(language, s.fixTh ?? "", s.fixEn ?? "")}</div>}
              </li>
            ))}
          </ul>
          <UnrestoredList steps={outcome.unrestored} />
          <NotesList notes={outcome.notes} />
        </div>
      )}
      {confirming && (
        <Dialog title={t("help.revert.title")} onClose={() => setConfirming(false)}>
          <p>{t("help.revert.confirm")}</p>
          <div className="row-actions">
            <button type="button" className="danger" onClick={() => void revert()}>
              {t("help.revert.button")}
            </button>
          </div>
        </Dialog>
      )}
    </div>
  );
}

function Report() {
  const ipc = useIpc();
  const { t } = useI18n();
  const [text, setText] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    let alive = true;
    ipc
      .diagnosticsCopy()
      .then((d) => alive && setText(d))
      .catch((e: unknown) => alive && setError(e));
    return () => {
      alive = false;
    };
  }, [ipc]);

  const copy = async () => {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch (e) {
      setError(e);
    }
  };

  return (
    <div>
      <h2>{t("help.report.title")}</h2>
      <p>{t("help.report.text")}</p>
      {error !== null && <ErrorNotice error={error} />}
      <div className="row-actions">
        <button type="button" className="primary" disabled={!text} onClick={() => void copy()}>
          {t("help.report.copy")}
        </button>
        <button type="button" onClick={() => void ipc.openExternal("gitHubIssues")}>
          {t("help.report.issues")}
        </button>
      </div>
      {copied && <Notice kind="ok">{t("help.report.copied")}</Notice>}
      {text && <pre className="log">{text}</pre>}
    </div>
  );
}

function Guide() {
  const { t } = useI18n();
  return (
    <div>
      <h2>{t("help.guide.title")}</h2>
      {(["p1", "p2", "p3", "p4", "p5"] as const).map((p) => (
        <p key={p}>{t(`help.guide.${p}`)}</p>
      ))}
    </div>
  );
}

function CreditList({ items }: { items: Credit[] }) {
  return (
    <ul className="check-list">
      {items.map((c) => (
        <li key={c.name}>
          <strong>{c.name}</strong>
          {c.version && ` ${c.version}`} · {c.license}
          <div>
            <small>{c.copyright}</small>
          </div>
          <div>
            <small>
              <code>{c.source}</code>
            </small>
          </div>
        </li>
      ))}
    </ul>
  );
}

function Credits() {
  const { t } = useI18n();
  return (
    <div>
      <h2>{t("help.credits.title")}</h2>
      <p>{t("help.credits.intro")}</p>
      <h3>{t("help.credits.bundled")}</h3>
      <CreditList items={BUNDLED} />
      <h3>{t("help.credits.schemes")}</h3>
      <CreditList items={SCHEMES} />
      <p>
        <small>{t("help.credits.signpath")}</small>
      </p>
    </div>
  );
}
```

- [ ] **Step 3: Write `app/ui/src/screens/Advanced.tsx`**

```tsx
// Advanced (spec §4.3): the add-ons' versions, the log, the data folder, and the generated
// files, read only. The only page where Oh My Posh and Clink are named (spec §4.5).
import { useEffect, useState } from "react";
import type { AdvancedInfo } from "../bindings/AdvancedInfo";
import { ErrorNotice } from "../components/Notices";
import { useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import { useAppState } from "../state";

export function Advanced() {
  const ipc = useIpc();
  const { t } = useI18n();
  const { version } = useAppState();
  const [info, setInfo] = useState<AdvancedInfo | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let alive = true;
    ipc
      .advancedRead()
      .then((i) => alive && setInfo(i))
      .catch((e: unknown) => alive && setError(e));
    return () => {
      alive = false;
    };
  }, [ipc, version, tick]);

  return (
    <section>
      <h1 tabIndex={-1}>{t("advanced.title")}</h1>
      {error !== null && <ErrorNotice error={error} />}
      {info && (
        <>
          <h2>{t("advanced.versions")}</h2>
          <table>
            <tbody>
              <tr>
                <th scope="row">{t("advanced.version.app")}</th>
                <td>{info.versions.app}</td>
              </tr>
              <tr>
                <th scope="row">{t("advanced.version.ohMyPosh")}</th>
                <td>{info.versions.ohMyPosh}</td>
              </tr>
              <tr>
                <th scope="row">{t("advanced.version.clink")}</th>
                <td>{info.versions.clink}</td>
              </tr>
              <tr>
                <th scope="row">{t("advanced.version.nerdFonts")}</th>
                <td>{info.versions.nerdFonts}</td>
              </tr>
            </tbody>
          </table>
          <h2>{t("advanced.dataDir")}</h2>
          <p>
            <code>{info.dataDir}</code>
          </p>
          <div className="row-actions">
            <button type="button" onClick={() => void ipc.openExternal("dataFolder")}>
              {t("advanced.openData")}
            </button>
            <button type="button" onClick={() => void ipc.openExternal("logsFolder")}>
              {t("advanced.openLogs")}
            </button>
            <button type="button" onClick={() => setTick((n) => n + 1)}>
              {t("common.refresh")}
            </button>
          </div>
          <h2>{t("advanced.log")}</h2>
          <pre className="log">{info.logTail}</pre>
          <h2>{t("advanced.generated")}</h2>
          {info.generated.length === 0 && <p>{t("advanced.none")}</p>}
          {info.generated.map((file) => (
            <details key={file.name}>
              <summary>
                <code>{file.name}</code>
              </summary>
              <pre className="log">{file.text}</pre>
            </details>
          ))}
        </>
      )}
    </section>
  );
}
```

- [ ] **Step 4: Type-check and test**

Run: `npm run typecheck` → clean. `npm test` → still `20 passed` (nothing routes here yet).

- [ ] **Step 5: Commit**

Stage `app/ui/src/screens/Help.tsx app/ui/src/screens/credits.ts app/ui/src/screens/Advanced.tsx`. Message: `ui: Help (health check with fixes, restore points, undo everything, report, guide, credits) and Advanced`.

---

### Task 2: The wizard, Home, Settings, the final shell, and the whole-app tests

Spec §4.2: six pages — (1) welcome and the language choice, remembered (founder 2026-10-16); (2) the machine check from `systemDetect` (Windows build, Terminal present / install from the Store / too old, the user's other AutoRun commands kept, another add-on will be asked about, ARM64 not supported, no admin rights needed); (3) choose a theme: the six recommended (Terracotta first and preselected) with "See all twelve"; (4) where it goes: every cmd window (default) or only in Windows Terminal, and "Open Windows Terminal with cmd" checked; (5) the real plan and Apply with progress ("we save a restore point first"); (6) done: the preview, "Try it: open cmd", "Go to the home page". "Skip, set up myself" on pages 1–4 goes home. Spec §4.3: Home — the status from the real checks with the time they ran, the theme in use with its preview and Customize / Change theme / Open cmd, the quick switch to the five most recently used themes, the latest restore point, and the slot for Phase 4's update notice; Settings — where themes go (changing it offers "Apply the current theme again" through the plan), the language, the update mode (automatic / tell me first / off, automatic by default, with the note that the app checks on every launch). The final `App.tsx` replaces the interim one.

**Files:**
- Create: `app/ui/src/screens/Wizard.tsx`, `app/ui/src/screens/Home.tsx`, `app/ui/src/screens/Settings.tsx`, `app/ui/src/App.test.tsx`
- Modify: `app/ui/src/App.tsx` (replace)
- Test: `App.test.tsx` (12 tests); `Themes.test.tsx` still passes (its helper clicks "Themes" from Home)

**Interfaces:**
- Consumes: everything of the themes-UI plan and Task 1.
- Produces: `export function Wizard(): JSX.Element`, `export const RECOMMENDED = ["Terracotta", "Tokyo Night", "Catppuccin Mocha", "Dracula", "Catppuccin Latte", "Paper"]`, `export function MachineCheck(props: { report: SystemReport }): JSX.Element`; `export function Home(): JSX.Element`; `export function Settings(): JSX.Element`; `export function App(): JSX.Element` — routes `wizard` (when `!settings.wizardDone`), `home`, `themes`, `editor`, `settings`, `help`, `advanced`; the after-apply bar.

- [ ] **Step 1: Write the failing tests** — `app/ui/src/App.test.tsx`:

```tsx
// The whole app against the mock IPC: the wizard applies a theme in five clicks and no typing
// (spec §1 criterion 1, §4.2), opens in English and switches to Thai, the after-apply bar
// undoes, the library customizes, Settings keeps the defaults, Help's doctor and revert work,
// and every button on every screen has a name in both languages (spec §4.5).
import { render, screen, waitFor, within, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";
import { IpcProvider } from "./ipc/context";
import { createMockIpc, type MockOptions } from "./ipc/mock";

function mount(options: MockOptions = {}) {
  const ipc = createMockIpc(options);
  const user = userEvent.setup();
  render(
    <IpcProvider ipc={ipc}>
      <App />
    </IpcProvider>,
  );
  return { ipc, user };
}

async function finishWizard(user: ReturnType<typeof userEvent.setup>) {
  // Page 1 → 2 → 3 → 4 → 5, then Apply: five clicks.
  await user.click(await screen.findByRole("button", { name: "Next" }));
  await screen.findByText(/Windows build 26100/);
  await user.click(screen.getByRole("button", { name: "Next" }));
  await screen.findByRole("heading", { name: "Choose a theme" });
  expect(screen.getByRole("button", { name: "Terracotta", pressed: true })).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Next" }));
  await screen.findByRole("heading", { name: "Where should the theme go?" });
  await user.click(screen.getByRole("button", { name: "Next" }));
  await screen.findByRole("heading", { name: "Ready to apply" });
  await screen.findByText("What will change");
  await user.click(screen.getByRole("button", { name: "Apply" }));
  await screen.findByRole("heading", { name: "Done" });
}

describe("the first run", () => {
  it("opens in English and applies Terracotta in five clicks with no typing", async () => {
    const { ipc, user } = mount();
    expect(await screen.findByRole("heading", { name: "Make your cmd window beautiful and easy to read" })).toBeTruthy();
    await finishWizard(user);
    expect(ipc.state.applied?.theme.name).toBe("Terracotta");
    expect(ipc.state.applied?.request).toEqual({ themeId: ipc.state.applied?.theme.id, target: "all", setWtDefaultProfile: true, replaceOtherClink: false });
    expect(ipc.state.calls.filter((c) => c === "apply")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Go to the home page" }));
    await screen.findByRole("heading", { name: "Home" });
    expect(ipc.state.settings.wizardDone).toBe(true);
    expect(ipc.state.settings.language).toBe("en");
    // The after-apply bar is up, and undoes.
    expect(screen.getByText(/Theme "Terracotta" applied/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(ipc.state.applied).toBeNull());
  });

  it("switches to Thai on page 1 and remembers it", async () => {
    const { ipc, user } = mount();
    await user.click(await screen.findByRole("button", { name: "ไทย" }));
    expect(await screen.findByRole("heading", { name: "แต่งหน้าต่าง cmd ให้สวยและอ่านง่าย" })).toBeTruthy();
    await waitFor(() => expect(ipc.state.settings.language).toBe("th"));
    await user.click(screen.getByRole("button", { name: "ข้าม ตั้งค่าเอง" }));
    expect(await screen.findByRole("heading", { name: "หน้าแรก" })).toBeTruthy();
    expect(ipc.state.settings.wizardDone).toBe(true);
  });

  it("asks before replacing another cmd add-on", async () => {
    const { ipc, user } = mount({ otherClink: ['"C:\\clink\\clink_x64.exe" inject --autorun'] });
    await user.click(await screen.findByRole("button", { name: "Next" }));
    expect(await screen.findByText("Another cmd add-on is already set up; we will ask before replacing it")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Next" }));
    await user.click(await screen.findByRole("button", { name: "Next" }));
    await user.click(await screen.findByRole("button", { name: "Next" }));
    expect(await screen.findByRole("heading", { name: "Another cmd add-on is already set up" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Use CMD Studio's instead" }));
    await screen.findByText("What will change");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    await screen.findByRole("heading", { name: "Done" });
    expect(ipc.state.applied?.request.replaceOtherClink).toBe(true);
  });

  it("shows a failed apply as put back, with a way to copy the details", async () => {
    const { ipc, user } = mount({ failApplyWith: "E_VERIFY_OMP" });
    await user.click(await screen.findByRole("button", { name: "Next" }));
    await user.click(await screen.findByRole("button", { name: "Next" }));
    await user.click(await screen.findByRole("button", { name: "Next" }));
    await user.click(await screen.findByRole("button", { name: "Next" }));
    await screen.findByText("What will change");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByText("The theme could not be applied. Everything was put back as it was.")).toBeTruthy();
    expect(screen.getByText("Code E_VERIFY_OMP", { exact: false })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Copy details for a problem report" })).toBeTruthy();
    expect(ipc.state.applied).toBeNull();
  });
});

describe("after the wizard", () => {
  async function home(options: MockOptions = {}) {
    const mounted = mount({ ...options, settings: { wizardDone: true, ...options.settings } });
    await screen.findByRole("heading", { name: "Home" });
    return mounted;
  }

  it("customizes a preset into a copy and edits it", async () => {
    const { ipc, user } = await home();
    await user.click(screen.getByRole("button", { name: "Themes" }));
    await screen.findByRole("heading", { name: "Themes" });
    const cards = await screen.findAllByText("Terracotta");
    const card = cards[0]!.closest(".theme-card")!;
    await user.click(within(card as HTMLElement).getByRole("button", { name: "Customize" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    expect(ipc.state.mine.map((t) => t.name)).toEqual(["My Terracotta"]);
    const name = screen.getByLabelText("Theme name") as HTMLInputElement;
    expect(name.value).toBe("My Terracotta");
    await user.click(screen.getByRole("tab", { name: "Advanced" }));
    expect(screen.getByLabelText("Frame")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved.")).toBeTruthy();
  });

  it("filters the library and shows the theme in use", async () => {
    const { user } = await home();
    await user.click(screen.getByRole("button", { name: "Themes" }));
    await screen.findByRole("heading", { name: "Themes" });
    await user.click(screen.getByRole("button", { name: "Light" }));
    await waitFor(() => expect(screen.queryByText("Terracotta")).toBeNull());
    expect(screen.getByText("Paper")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "All" }));
    const cards = await screen.findAllByText("Paper");
    const card = cards[0]!.closest(".theme-card") as HTMLElement;
    await user.click(within(card).getByRole("button", { name: "Apply" }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText("Steps", { exact: false });
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    await within(dialog).findByText(/Theme "Paper" applied/);
    await user.click(screen.getAllByRole("button", { name: "Close" })[0]!);
    await waitFor(() => expect(screen.getAllByText("In use").length).toBeGreaterThan(0));
  });

  it("keeps the defaults in Settings: English, every window, automatic updates", async () => {
    const { ipc, user } = await home();
    await user.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", { name: "Settings" });
    expect((screen.getByLabelText("English") as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText(/Every cmd window/) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("Automatic (recommended)") as HTMLInputElement).checked).toBe(true);
    await user.click(screen.getByLabelText("Tell me first"));
    await waitFor(() => expect(ipc.state.settings.updates).toBe("notify"));
    await user.click(screen.getByLabelText("ไทย"));
    expect(await screen.findByRole("heading", { name: "ตั้งค่า" })).toBeTruthy();
    expect(ipc.state.settings.language).toBe("th");
  });

  it("runs the health check, offers a fix, and undoes everything", async () => {
    const { ipc, user } = await home({ otherClink: ['"C:\\clink\\clink_x64.exe" inject --autorun'] });
    // Apply first so a fix that applies again has a theme.
    await user.click(screen.getByRole("button", { name: "Themes" }));
    const cards = await screen.findAllByText("Terracotta");
    await user.click(within(cards[0]!.closest(".theme-card") as HTMLElement).getByRole("button", { name: "Apply" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(await within(dialog).findByRole("button", { name: "Use CMD Studio's instead" }));
    await within(dialog).findByText("Steps", { exact: false });
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    await within(dialog).findByText(/applied/);
    await user.click(screen.getAllByRole("button", { name: "Close" })[0]!);
    await user.click(screen.getByRole("button", { name: "Help" }));
    await screen.findByText(/D5/);
    expect(screen.getByRole("button", { name: "Replace the other add-on" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open a new cmd window" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Open a new cmd window" }));
    await waitFor(() => expect(ipc.state.calls).toContain("openCmd"));
    await user.click(screen.getByRole("tab", { name: "Undo everything" }));
    await user.click(screen.getByRole("button", { name: "Undo everything" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Undo everything" }));
    expect(await screen.findByText("Done: everything CMD Studio changed is back as it was.")).toBeTruthy();
    expect(ipc.state.applied).toBeNull();
  });

  it("lists restore points and goes back to one", async () => {
    const { ipc, user } = await home();
    await user.click(screen.getByRole("button", { name: "Themes" }));
    const cards = await screen.findAllByText("Terracotta");
    await user.click(within(cards[0]!.closest(".theme-card") as HTMLElement).getByRole("button", { name: "Apply" }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText("Steps", { exact: false });
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    await within(dialog).findByText(/applied/);
    await user.click(screen.getAllByRole("button", { name: "Close" })[0]!);
    await user.click(screen.getByRole("button", { name: "Help" }));
    await user.click(await screen.findByRole("tab", { name: "Restore points" }));
    await user.click(await screen.findByRole("button", { name: "Go back to this point" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Go back to this point" }));
    expect(await screen.findByText("Done: everything is back to that point.")).toBeTruthy();
    expect(ipc.state.applied).toBeNull();
  });

  it("copies the diagnostics for a problem report", async () => {
    const { ipc, user } = await home();
    const written: string[] = [];
    Object.defineProperty(navigator, "clipboard", { value: { writeText: async (s: string) => void written.push(s) }, configurable: true });
    await user.click(screen.getByRole("button", { name: "Help" }));
    await user.click(await screen.findByRole("tab", { name: "Report a problem" }));
    await user.click(await screen.findByRole("button", { name: "Copy details" }));
    await waitFor(() => expect(written[0]).toContain("CMD Studio diagnostics"));
    expect(ipc.state.calls).toContain("diagnosticsCopy");
  });

  it("shows the add-ons' versions on the Advanced page", async () => {
    const { user } = await home();
    await user.click(screen.getByRole("button", { name: "Advanced" }));
    await screen.findByRole("heading", { name: "Advanced" });
    expect(await screen.findByText("31.5.0")).toBeTruthy();
    expect(screen.getByText("1.9.34")).toBeTruthy();
  });
});

describe("accessibility", () => {
  it("names every button on every screen, in English and in Thai", async () => {
    for (const language of ["en", "th"] as const) {
      const { user } = mount({ settings: { wizardDone: true, language } });
      const nav = await screen.findByRole("navigation");
      const pages = within(nav).getAllByRole("button");
      for (const page of pages) {
        await user.click(page);
        await act(async () => {
          await new Promise((r) => setTimeout(r, 20));
        });
        for (const button of screen.getAllByRole("button")) {
          const name = button.getAttribute("aria-label") ?? button.textContent ?? "";
          expect(name.trim(), `${language}: a button without a name on ${page.textContent}`).not.toBe("");
        }
      }
      // Unmount before the other language mounts.
      document.body.innerHTML = "";
    }
  });
});
```

Run: `npm test`
Expected: FAIL — `App.test.tsx` cannot find "Make your cmd window beautiful and easy to read" (the interim shell has no wizard); `Themes.test` still passes.

- [ ] **Step 2: Write `app/ui/src/screens/Wizard.tsx`**

```tsx
// The first-run wizard (spec §4.2): six pages, at least five clicks from start to a theme
// applied, no typing. Page 1 picks the language (remembered, spec §4.5); page 2 checks the
// machine; page 3 picks a theme (Terracotta first); page 4 where it goes; page 5 shows the real
// plan and applies with progress; page 6 shows the result. "Skip, set up myself" goes home.
import { useEffect, useMemo, useState } from "react";
import type { ApplyOutcome } from "../bindings/ApplyOutcome";
import type { Library } from "../bindings/Library";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { SystemReport } from "../bindings/SystemReport";
import type { Target } from "../bindings/Target";
import type { Theme } from "../bindings/Theme";
import { ApplyFlow } from "../components/ApplyFlow";
import { ErrorNotice } from "../components/Notices";
import { TerminalPreview } from "../components/TerminalPreview";
import { ThemeCard } from "../components/ThemeCard";
import { useI18n, type Language } from "../i18n";
import { useIpc } from "../ipc/context";
import { SAMPLE_CONTEXT } from "../ipc/mock";
import { useNav } from "../router";
import { useAppState } from "../state";

/** Spec §4.2 page 3: the six recommended presets, Terracotta first and preselected. */
export const RECOMMENDED = ["Terracotta", "Tokyo Night", "Catppuccin Mocha", "Dracula", "Catppuccin Latte", "Paper"];

export function Wizard() {
  const ipc = useIpc();
  const { t, language, setLanguage } = useI18n();
  const { settings, saveSettings, setLastApply, bump } = useAppState();
  const { go } = useNav();
  const [page, setPage] = useState(1);
  const [report, setReport] = useState<SystemReport | null>(null);
  const [reportError, setReportError] = useState<unknown>(null);
  const [library, setLibrary] = useState<Library | null>(null);
  const [themes, setThemes] = useState<Record<string, Theme>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [target, setTarget] = useState<Target>(settings.target);
  const [setDefault, setSetDefault] = useState(settings.setWtDefaultProfile);
  const [outcome, setOutcome] = useState<ApplyOutcome | null>(null);
  const [preview, setPreview] = useState<PreviewResult | null>(null);

  useEffect(() => {
    let alive = true;
    ipc.systemDetect().then((r) => alive && setReport(r)).catch((e: unknown) => alive && setReportError(e));
    ipc
      .themesList()
      .then(async (lib) => {
        if (!alive) return;
        setLibrary(lib);
        const presets = lib.themes.filter((s) => s.source === "preset");
        const full = await Promise.all(presets.map((s) => ipc.themeGet(s.id).then((d) => d.theme)));
        if (!alive) return;
        const map: Record<string, Theme> = {};
        for (const theme of full) map[theme.id] = theme;
        setThemes(map);
        const terracotta = presets.find((s) => s.name === "Terracotta") ?? presets[0];
        setSelected((current) => current ?? terracotta?.id ?? null);
      })
      .catch((e: unknown) => alive && setReportError(e));
    return () => {
      alive = false;
    };
  }, [ipc]);

  useEffect(() => {
    if (page !== 6 || !selected) return;
    const theme = themes[selected];
    if (!theme) return;
    let alive = true;
    ipc.previewRender(theme, SAMPLE_CONTEXT).then((r) => alive && setPreview(r)).catch(() => {});
    return () => {
      alive = false;
    };
  }, [ipc, page, selected, themes]);

  const pickLanguage = async (next: Language) => {
    setLanguage(next);
    try {
      await saveSettings({ ...settings, language: next });
    } catch {
      // The UI already switched; the choice is saved again when the wizard finishes.
    }
  };

  const finish = async () => {
    try {
      await saveSettings({ ...settings, language, target, setWtDefaultProfile: setDefault, wizardDone: true });
    } finally {
      go({ name: "home" });
    }
  };

  const skip = async () => {
    try {
      await saveSettings({ ...settings, language, wizardDone: true });
    } finally {
      go({ name: "home" });
    }
  };

  const presets = useMemo(() => {
    const all = library?.themes.filter((s) => s.source === "preset") ?? [];
    if (showAll) return all;
    return RECOMMENDED.map((name) => all.find((s) => s.name === name)).filter((s): s is NonNullable<typeof s> => s !== undefined);
  }, [library, showAll]);

  const canNext = page === 3 ? selected !== null : page !== 5;

  return (
    <main className="wizard" id="content">
      <p className="badge" aria-live="polite">
        {t("wizard.step", { n: page })}
      </p>
      {page === 1 && (
        <section>
          <h1 tabIndex={-1}>{t("wizard.welcome.title")}</h1>
          <p>{t("wizard.welcome.text")}</p>
          <fieldset>
            <legend>{t("language.label")}</legend>
            <div className="row-actions">
              <button type="button" aria-pressed={language === "en"} onClick={() => void pickLanguage("en")}>
                {t("language.en")}
              </button>
              <button type="button" aria-pressed={language === "th"} onClick={() => void pickLanguage("th")}>
                {t("language.th")}
              </button>
            </div>
          </fieldset>
        </section>
      )}
      {page === 2 && (
        <section>
          <h1 tabIndex={-1}>{t("wizard.check.title")}</h1>
          {reportError !== null && <ErrorNotice error={reportError} />}
          {!report && reportError === null && <p role="status">{t("wizard.check.checking")}</p>}
          {report && <MachineCheck report={report} />}
        </section>
      )}
      {page === 3 && (
        <section>
          <h1 tabIndex={-1}>{t("wizard.choose.title")}</h1>
          <p>{t("wizard.choose.canChangeLater")}</p>
          <div className="grid">
            {presets.map((s) => (
              <ThemeCard key={s.id} summary={s} theme={themes[s.id] ?? null} selected={selected === s.id} onSelect={() => setSelected(s.id)} />
            ))}
          </div>
          <button type="button" className="link" onClick={() => setShowAll((v) => !v)}>
            {showAll ? t("wizard.choose.seeFewer") : t("wizard.choose.seeAll")}
          </button>
        </section>
      )}
      {page === 4 && (
        <section>
          <h1 tabIndex={-1}>{t("wizard.where.title")}</h1>
          <fieldset>
            <legend>{t("wizard.where.title")}</legend>
            <label className="row">
              <input type="radio" name="target" checked={target === "all"} onChange={() => setTarget("all")} />
              <span>
                {t("wizard.where.all")} <br />
                <small>{t("wizard.where.allHint")}</small>
              </span>
            </label>
            <label className="row">
              <input type="radio" name="target" checked={target === "wt"} onChange={() => setTarget("wt")} />
              <span>
                {t("wizard.where.wt")} <br />
                <small>{t("wizard.where.wtHint")}</small>
              </span>
            </label>
          </fieldset>
          <label className="row">
            <input type="checkbox" checked={setDefault} onChange={(e) => setSetDefault(e.target.checked)} />
            {t("wizard.where.setDefault")}
          </label>
        </section>
      )}
      {page === 5 && selected && (
        <section>
          <h1 tabIndex={-1}>{t("wizard.confirm.title")}</h1>
          <ApplyFlow
            request={{ themeId: selected, target, setWtDefaultProfile: setDefault, replaceOtherClink: false }}
            showBackup
            onApplied={(o) => {
              setOutcome(o);
              setLastApply(o);
              bump();
              setPage(6);
            }}
          />
        </section>
      )}
      {page === 6 && selected && (
        <section>
          <h1 tabIndex={-1}>{t("wizard.done.title")}</h1>
          <p>{t("wizard.done.text")}</p>
          {themes[selected] && <TerminalPreview theme={themes[selected]!} result={preview} />}
          <div className="row-actions">
            <button type="button" onClick={() => void ipc.openCmd()}>
              {t("wizard.done.tryCmd")}
            </button>
            <button type="button" className="primary" onClick={() => void finish()}>
              {t("wizard.done.goHome")}
            </button>
          </div>
          {outcome === null && <span />}
        </section>
      )}
      <div className="wizard-nav">
        <div>
          {page > 1 && page < 6 && (
            <button type="button" onClick={() => setPage(page - 1)}>
              {t("common.back")}
            </button>
          )}
        </div>
        <div className="row-actions">
          {page < 5 && (
            <button type="button" className="link" onClick={() => void skip()}>
              {t("wizard.skip")}
            </button>
          )}
          {page < 5 && (
            <button type="button" className="primary" disabled={!canNext} onClick={() => setPage(page + 1)}>
              {t("common.next")}
            </button>
          )}
        </div>
      </div>
    </main>
  );
}

export function MachineCheck({ report }: { report: SystemReport }) {
  const ipc = useIpc();
  const { t } = useI18n();
  const otherCount = report.autorun.otherCommands.length;
  return (
    <ul className="check-list">
      <li>{report.build === null ? t("wizard.check.windowsUnknown") : t("wizard.check.windows", { build: report.build })}</li>
      <li>
        {report.wtInstalled ? (
          t("wizard.check.wtPresent")
        ) : report.storeAvailable ? (
          <>
            {t("wizard.check.wtMissing")}
            <div className="row-actions">
              <button type="button" onClick={() => void ipc.openExternal("windowsTerminalStore")}>
                {t("wizard.check.wtInstall")}
              </button>
              <small>{t("wizard.check.wtSkipHint")}</small>
            </div>
          </>
        ) : (
          t("wizard.check.wtTooOld")
        )}
      </li>
      <li>{otherCount === 0 ? t("wizard.check.autorunNone") : t("wizard.check.autorunKept", { n: otherCount })}</li>
      {report.autorun.foreignClink.length > 0 && <li>{t("wizard.check.otherClink")}</li>}
      {report.autorun.machineWideClink && <li>{t("wizard.check.machineWideClink")}</li>}
      {report.arch !== "x64" && <li className="status-fail">{t("wizard.check.arm64")}</li>}
      <li>{t("wizard.check.noAdmin")}</li>
    </ul>
  );
}
```

- [ ] **Step 3: Write `app/ui/src/screens/Home.tsx`**

```tsx
// The home page (spec §4.3): a status bar from the real checks with the time they ran, the
// theme in use with its preview, Customize / Change theme / Open cmd, a quick switch to the
// five themes used most recently, and the latest restore point. A line is reserved for Phase
// 4's "Updated to vX" notice (`home.update.placeholder`).
import { useEffect, useState } from "react";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { DoctorReport } from "../bindings/DoctorReport";
import type { Library } from "../bindings/Library";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { RestorePoints } from "../bindings/RestorePoints";
import type { Theme } from "../bindings/Theme";
import { ApplyFlow } from "../components/ApplyFlow";
import { Dialog } from "../components/Dialog";
import { ErrorNotice } from "../components/Notices";
import { TerminalPreview } from "../components/TerminalPreview";
import { formatTime, useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import { SAMPLE_CONTEXT } from "../ipc/mock";
import { useNav } from "../router";
import { useAppState } from "../state";

export function Home() {
  const ipc = useIpc();
  const { t, language } = useI18n();
  const { go } = useNav();
  const { settings, version, bump, setLastApply } = useAppState();
  const [library, setLibrary] = useState<Library | null>(null);
  const [doctor, setDoctor] = useState<DoctorReport | null>(null);
  const [points, setPoints] = useState<RestorePoints | null>(null);
  const [active, setActive] = useState<Theme | null>(null);
  const [preview, setPreview] = useState<PreviewResult | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [applying, setApplying] = useState<ApplyRequest | null>(null);

  useEffect(() => {
    let alive = true;
    setError(null);
    Promise.all([ipc.themesList(), ipc.doctorRun(), ipc.snapshotsList()])
      .then(async ([lib, report, snaps]) => {
        if (!alive) return;
        setLibrary(lib);
        setDoctor(report);
        setPoints(snaps);
        if (lib.applied) {
          const detail = await ipc.themeGet(lib.applied.themeId).catch(() => null);
          if (!alive) return;
          setActive(detail?.theme ?? null);
          if (detail) {
            const drawn = await ipc.previewRender(detail.theme, SAMPLE_CONTEXT).catch(() => null);
            if (alive) setPreview(drawn);
          }
        } else {
          setActive(null);
          setPreview(null);
        }
      })
      .catch((e: unknown) => alive && setError(e));
    return () => {
      alive = false;
    };
  }, [ipc, version]);

  const customize = async () => {
    if (!library?.applied) return;
    const summary = library.themes.find((s) => s.id === library.applied?.themeId);
    if (!summary) return;
    if (summary.source === "preset") {
      const copy = await ipc.themeDuplicate(summary.id, t("themes.copyName", { name: summary.name }));
      go({ name: "editor", id: copy.id });
    } else {
      go({ name: "editor", id: summary.id });
    }
  };

  const warns = doctor?.checks.filter((c) => c.status === "warn").length ?? 0;
  const fails = doctor?.checks.filter((c) => c.status === "fail").length ?? 0;
  const statusText = !doctor
    ? t("home.status.checking")
    : fails > 0
      ? t("home.status.fail", { n: fails })
      : warns > 0
        ? t("home.status.warn", { n: warns })
        : t("home.status.ok");
  const statusClass = !doctor ? "" : fails > 0 ? "status-fail" : warns > 0 ? "status-warn" : "status-ok";

  // Recently used: the newest apply of each theme, up to five, that still exists.
  const recent = (points?.points ?? [])
    .filter((p) => p.kind === "apply" && p.theme)
    .map((p) => p.theme!)
    .filter((th, i, all) => all.findIndex((o) => o.id === th.id) === i)
    .filter((th) => library?.themes.some((s) => s.id === th.id))
    .slice(0, 5);
  const latest = points?.points[0] ?? null;

  return (
    <section>
      <h1 tabIndex={-1}>{t("home.title")}</h1>
      {error !== null && <ErrorNotice error={error} />}
      <div className="card" aria-live="polite">
        <h2 style={{ marginTop: 0 }}>{t("home.status.title")}</h2>
        <p className={statusClass}>{statusText}</p>
        {doctor && <p>{t("home.status.checkedAt", { time: formatTime(doctor.checkedAtMs, language) })}</p>}
        <button type="button" onClick={() => go({ name: "help", tab: "doctor" })}>
          {t("home.status.open")}
        </button>
        {/* Phase 4's updater puts its "Updated to vX" notice here. */}
        <div data-slot="update-notice">{t("home.update.placeholder")}</div>
      </div>
      <div className="card">
        <h2 style={{ marginTop: 0 }}>{t("home.active.title")}</h2>
        {library && !library.applied && (
          <>
            <p>{t("home.active.none")}</p>
            <button type="button" className="primary" onClick={() => go({ name: "themes" })}>
              {t("home.active.pick")}
            </button>
          </>
        )}
        {library?.applied && (
          <>
            <p>
              <strong>{library.applied.themeName}</strong>
            </p>
            {active && <TerminalPreview theme={active} result={preview} />}
            <div className="row-actions">
              <button type="button" onClick={() => void customize()}>
                {t("home.active.customize")}
              </button>
              <button type="button" onClick={() => go({ name: "themes" })}>
                {t("home.active.change")}
              </button>
              <button type="button" onClick={() => void ipc.openCmd()}>
                {t("home.active.openCmd")}
              </button>
            </div>
          </>
        )}
      </div>
      <div className="card">
        <h2 style={{ marginTop: 0 }}>{t("home.quick.title")}</h2>
        {recent.length === 0 && <p>{t("home.quick.none")}</p>}
        <div className="row-actions">
          {recent.map((th) => (
            <button
              key={th.id}
              type="button"
              disabled={library?.applied?.themeId === th.id}
              onClick={() =>
                setApplying({ themeId: th.id, target: settings.target, setWtDefaultProfile: settings.setWtDefaultProfile, replaceOtherClink: false })
              }
            >
              {th.name}
            </button>
          ))}
        </div>
      </div>
      <div className="card">
        <h2 style={{ marginTop: 0 }}>{t("home.restore.title")}</h2>
        {!latest && <p>{t("home.restore.none")}</p>}
        {latest && (
          <p>
            {t("home.restore.latest", {
              name: latest.theme?.name ?? t(`help.restore.kind.${latest.kind}`),
              time: formatTime(latest.createdMs, language),
            })}
          </p>
        )}
        <button type="button" onClick={() => go({ name: "help", tab: "restore" })}>
          {t("home.restore.all")}
        </button>
      </div>
      {applying && (
        <Dialog title={t("apply.planTitle")} onClose={() => setApplying(null)}>
          <ApplyFlow
            titled={false}
            request={applying}
            onCancel={() => setApplying(null)}
            onApplied={(o) => {
              setLastApply(o);
              bump();
            }}
          />
        </Dialog>
      )}
    </section>
  );
}
```

- [ ] **Step 4: Write `app/ui/src/screens/Settings.tsx`**

```tsx
// Settings (spec §4.3): where themes go (the wizard's page 4 choice; changing it offers to
// apply the current theme again, since that changes the machine and must be shown first),
// the language (remembered), and the update mode (stored and shown here; Phase 4's updater
// reads it, automatic being the default, spec §6.2).
import { useState } from "react";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { AppSettings } from "../bindings/AppSettings";
import type { Target } from "../bindings/Target";
import type { UpdateMode } from "../bindings/UpdateMode";
import { ApplyFlow } from "../components/ApplyFlow";
import { Dialog } from "../components/Dialog";
import { ErrorNotice, Notice } from "../components/Notices";
import { useI18n, type Language } from "../i18n";
import { useIpc } from "../ipc/context";
import { useAppState } from "../state";

export function Settings() {
  const ipc = useIpc();
  const { t } = useI18n();
  const { settings, saveSettings, setLastApply, bump } = useAppState();
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [reapply, setReapply] = useState<ApplyRequest | null>(null);

  const change = async (next: Partial<AppSettings>) => {
    setError(null);
    setSaved(false);
    try {
      await saveSettings({ ...settings, ...next });
      setSaved(true);
    } catch (e) {
      setError(e);
    }
  };

  const applyAgain = async () => {
    setError(null);
    try {
      const lib = await ipc.themesList();
      if (!lib.applied) return;
      setReapply({
        themeId: lib.applied.themeId,
        target: settings.target,
        setWtDefaultProfile: settings.setWtDefaultProfile,
        replaceOtherClink: false,
      });
    } catch (e) {
      setError(e);
    }
  };

  return (
    <section>
      <h1 tabIndex={-1}>{t("settings.title")}</h1>
      {error !== null && <ErrorNotice error={error} />}
      {saved && <Notice kind="ok">{t("settings.saved")}</Notice>}
      <fieldset>
        <legend>{t("settings.target.title")}</legend>
        {(["all", "wt"] as Target[]).map((target) => (
          <label className="row" key={target}>
            <input type="radio" name="target" checked={settings.target === target} onChange={() => void change({ target })} />
            <span>
              {t(target === "all" ? "wizard.where.all" : "wizard.where.wt")}
              <br />
              <small>{t(target === "all" ? "wizard.where.allHint" : "wizard.where.wtHint")}</small>
            </span>
          </label>
        ))}
        <label className="row">
          <input
            type="checkbox"
            checked={settings.setWtDefaultProfile}
            onChange={(e) => void change({ setWtDefaultProfile: e.target.checked })}
          />
          {t("wizard.where.setDefault")}
        </label>
        <div className="row-actions">
          <button type="button" onClick={() => void applyAgain()}>
            {t("settings.target.applyAgain")}
          </button>
        </div>
      </fieldset>
      <fieldset>
        <legend>{t("settings.language.title")}</legend>
        {(["en", "th"] as Language[]).map((language) => (
          <label className="row" key={language}>
            <input type="radio" name="language" checked={settings.language === language} onChange={() => void change({ language })} />
            {t(`language.${language}`)}
          </label>
        ))}
      </fieldset>
      <fieldset>
        <legend>{t("settings.updates.title")}</legend>
        {(["auto", "notify", "off"] as UpdateMode[]).map((mode) => (
          <label className="row" key={mode}>
            <input type="radio" name="updates" checked={settings.updates === mode} onChange={() => void change({ updates: mode })} />
            {t(`settings.updates.${mode}`)}
          </label>
        ))}
        <p>
          <small>{t("settings.updates.note")}</small>
        </p>
      </fieldset>
      {reapply && (
        <Dialog title={t("apply.planTitle")} onClose={() => setReapply(null)}>
          <ApplyFlow
            titled={false}
            request={reapply}
            onCancel={() => setReapply(null)}
            onApplied={(o) => {
              setLastApply(o);
              bump();
            }}
          />
        </Dialog>
      )}
    </section>
  );
}
```

- [ ] **Step 5: Replace `app/ui/src/App.tsx`**

```tsx
// The shell: loads the settings once, opens in English (the saved language takes over when it
// is Thai), shows the wizard until it is done, then the five pages behind a sidebar, with the
// after-apply bar on top (spec §4.3, §4.4, §4.5).
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ApplyOutcome } from "./bindings/ApplyOutcome";
import type { AppSettings } from "./bindings/AppSettings";
import { ErrorNotice, Notice } from "./components/Notices";
import { I18nProvider, useI18n, type Language } from "./i18n";
import { useIpc } from "./ipc/context";
import { NavContext, type Route } from "./router";
import { Advanced } from "./screens/Advanced";
import { Editor } from "./screens/Editor";
import { Help } from "./screens/Help";
import { Home } from "./screens/Home";
import { Settings } from "./screens/Settings";
import { Themes } from "./screens/Themes";
import { Wizard } from "./screens/Wizard";
import { AppStateContext, DEFAULT_SETTINGS } from "./state";

export function App() {
  const ipc = useIpc();
  const [language, setLanguage] = useState<Language>("en");
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [route, setRoute] = useState<Route | null>(null);
  const [lastApply, setLastApply] = useState<ApplyOutcome | null>(null);
  const [version, setVersion] = useState(0);
  const [startError, setStartError] = useState<unknown>(null);
  const [resetFrom, setResetFrom] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    ipc
      .settingsGet()
      .then((loaded) => {
        if (!alive) return;
        setSettings(loaded.settings);
        setLanguage(loaded.settings.language);
        setResetFrom(loaded.resetFrom);
        setRoute(loaded.settings.wizardDone ? { name: "home" } : { name: "wizard" });
      })
      .catch((e: unknown) => {
        if (!alive) return;
        setStartError(e);
        setRoute({ name: "home" });
      });
    return () => {
      alive = false;
    };
  }, [ipc]);

  const saveSettings = useCallback(
    async (next: AppSettings) => {
      const saved = await ipc.settingsSet(next);
      setSettings(saved);
      setLanguage(saved.language);
    },
    [ipc],
  );

  const bump = useCallback(() => setVersion((v) => v + 1), []);
  const appState = useMemo(
    () => ({ settings, saveSettings, lastApply, setLastApply, version, bump }),
    [settings, saveSettings, lastApply, version, bump],
  );
  const nav = useMemo(() => ({ route: route ?? { name: "home" as const }, go: (r: Route) => setRoute(r) }), [route]);

  return (
    <I18nProvider language={language} setLanguage={setLanguage}>
      <AppStateContext.Provider value={appState}>
        <NavContext.Provider value={nav}>
          <div lang={language}>
            {route === null ? (
              <p role="status" style={{ padding: "2rem" }}>
                …
              </p>
            ) : route.name === "wizard" ? (
              <Wizard />
            ) : (
              <Shell startError={startError} resetFrom={resetFrom} />
            )}
          </div>
        </NavContext.Provider>
      </AppStateContext.Provider>
    </I18nProvider>
  );
}

function Shell({ startError, resetFrom }: { startError: unknown; resetFrom: string | null }) {
  const { t } = useI18n();
  const { route, go } = useNavSafe();
  const heading = useRef<HTMLHeadingElement>(null);
  const items: { route: Route; label: string }[] = [
    { route: { name: "home" }, label: t("nav.home") },
    { route: { name: "themes" }, label: t("nav.themes") },
    { route: { name: "settings" }, label: t("nav.settings") },
    { route: { name: "help", tab: "doctor" }, label: t("nav.help") },
    { route: { name: "advanced" }, label: t("nav.advanced") },
  ];
  const current = (r: Route) => r.name === route.name || (r.name === "themes" && route.name === "editor");

  useEffect(() => {
    heading.current?.focus();
  }, [route]);

  return (
    <>
      <a className="skip-link" href="#content">
        {t("a11y.skipToContent")}
      </a>
      <AfterApplyBar />
      <div className="shell">
        <nav className="sidebar" aria-label={t("a11y.mainMenu")}>
          <div className="brand">{t("app.name")}</div>
          {items.map((item) => (
            <button
              key={item.route.name}
              type="button"
              aria-current={current(item.route) ? "page" : undefined}
              onClick={() => go(item.route)}
            >
              {item.label}
            </button>
          ))}
        </nav>
        <main id="content">
          {startError !== null && <ErrorNotice error={startError} />}
          {resetFrom && <Notice kind="warn">{t("settings.reset", { file: resetFrom })}</Notice>}
          <div ref={heading} tabIndex={-1} className="visually-hidden">
            {t("app.name")}
          </div>
          {route.name === "home" && <Home />}
          {route.name === "themes" && <Themes />}
          {route.name === "editor" && <Editor id={route.id} />}
          {route.name === "settings" && <Settings />}
          {route.name === "help" && <Help tab={route.tab} />}
          {route.name === "advanced" && <Advanced />}
        </main>
      </div>
    </>
  );
}

function useNavSafe() {
  const nav = useContextNav();
  return nav;
}

import { useNav as useContextNav } from "./router";
import { useAppState } from "./state";

/** Spec §4.4: after every apply, "Theme X applied · open a new cmd window…" with Undo. */
function AfterApplyBar() {
  const ipc = useIpc();
  const { t } = useI18n();
  const { lastApply, setLastApply, bump } = useAppState();
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  if (!lastApply || lastApply.status !== "committed") return null;

  const undo = async () => {
    setError(null);
    try {
      const outcome = await ipc.undoLast();
      setMessage(outcome.status === "nothingToUndo" ? t("apply.undoNothing") : t("apply.undone"));
      if (outcome.status === "committed") {
        setLastApply(null);
        bump();
      } else if (outcome.error) {
        setError(outcome.error);
      }
    } catch (e) {
      setError(e);
    }
  };

  return (
    <div className="bar" role="status">
      <span>{t("apply.done", { name: lastApply.themeName })}</span>
      <button type="button" onClick={() => void undo()}>
        {t("apply.undo")}
      </button>
      <button type="button" className="link" onClick={() => setLastApply(null)}>
        {t("common.dismiss")}
      </button>
      {message && <span>{message}</span>}
      {error !== null && <ErrorNotice error={error} />}
    </div>
  );
}
```

- [ ] **Step 6: Run the tests**

Run: `npm test`
Expected: `Test Files 5 passed`, `Tests 32 passed` (7 + 2 + 5 + 6 + 12). The twelve of `App.test`: the first run opens in English and applies Terracotta in five clicks (`apply` called once, request `{target "all", setWtDefaultProfile true, replaceOtherClink false}`), lands on Home with `wizardDone` and `language "en"` saved, and the bar's Undo takes it back; page 1's "ไทย" switches the heading to "แต่งหน้าต่าง cmd ให้สวยและอ่านง่าย", saves `language "th"`, and "ข้าม ตั้งค่าเอง" lands on "หน้าแรก"; another add-on is announced on page 2 and asked about on page 5; a failed apply is put back with `Code E_VERIFY_OMP`; after the wizard: Customize → "My Terracotta" edited and saved; the Light filter and "In use"; Settings shows English / Every cmd window / Automatic (recommended) checked, "Tell me first" saves `notify`, "ไทย" switches to "ตั้งค่า"; Help shows D5 with "Replace the other add-on" and "Open a new cmd window" (which calls `openCmd`), "Undo everything" asks and says "Done: everything CMD Studio changed is back as it was."; a restore point goes back ("Done: everything is back to that point."); "Copy details" writes "CMD Studio diagnostics" to the clipboard; Advanced shows 31.5.0 and 1.9.34; every button on every page has a name in English and in Thai.
Run: `npm run typecheck` → clean; `npm run build` → `ui/dist/` written.

- [ ] **Step 7: See that the click count bites**

In `Wizard.tsx` make page 3 require an explicit click by starting `selected` at `null` without the Terracotta default (replace `setSelected((current) => current ?? terracotta?.id ?? null)` with `setSelected((current) => current)`); `npm test` FAILS `opens in English and applies Terracotta in five clicks with no typing` (`Terracotta` is not pressed and Next is disabled). Restore; passes.

- [ ] **Step 8: The visual check in the browser pane** — `$env:VITE_MOCK_IPC = "1"; npm run dev`, open `http://localhost:1420` in the built-in browser pane (never the Tauri window): the wizard's six pages with the keyboard only (Tab, Space, Enter: five activations from page 1 to the applied theme, no text field needed), the home page's status card and preview, Settings' three groups, Help's six tabs, Advanced's table and log; switch to ไทย on page 1 and confirm every page reads Thai with the same layout (Leelawadee UI / Noto Sans Thai from `styles.css`), including the dialogs. No console error. Stop the server; `Remove-Item Env:VITE_MOCK_IPC`.

- [ ] **Step 9: Commit**

Stage `app/ui/src/screens/Wizard.tsx app/ui/src/screens/Home.tsx app/ui/src/screens/Settings.tsx app/ui/src/App.tsx app/ui/src/App.test.tsx`. Message: `ui: the first-run wizard (five clicks), Home, Settings, and the final shell; whole-app tests in English and Thai`.

---

### Task 3: Appendix B records this phase's decisions

The spec's decision log gains the rows for the index plan's decisions, dated today, after the coordinator's rows of 2026-10-16 (the working copy has them; append at the end of the table).

**Files:**
- Modify: `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (appendix B)

- [ ] **Step 1: Append the rows** at the end of appendix B's table:

```markdown
| 2026-10-16 | Phase 2b (แอป) การตัดสินใจของ architect ที่ founder เปลี่ยนได้:<br>• exe เดียว `cmdstudio.exe` เป็น Windows-subsystem (ไม่มีหน้าต่าง console วูบตอนเปิดแอป) เปิดโดยไม่มี argument = แอป มี argument = CLI ของ phase 1b (เป็น library แล้ว) ซึ่งต่อ console ของ parent ก่อนพิมพ์ · ผลคือ cmd ไม่รอ `cmdstudio doctor` (ใช้ `\| more` หรือ `start /wait` ถ้าต้องการรอ ตัวถอนการติดตั้งใช้ `ExecWait` จึงรอ) รอ founder ตัดสินว่าจะยอมรับหรือให้ phase 4 เพิ่ม exe แบบ console อีกตัว<br>• `Backend<M: Machine>` ใน `app/src-tauri/src/backend` มีเมธอดละหนึ่งคำสั่ง IPC เทสต์ทั้งหมดกับ `FakeMachine` ของ core (97 เทสต์) ชั้น IPC (`ipc.rs`) เป็นตัวห่อบางๆ 27 คำสั่ง = §5.7 + `preview_render`, `open_external`, `advanced_read`<br>• `plan_apply` ปล่อยล็อกทันทีและคืน digest (sha256 ของสรุป คำเตือน และขั้นตอน) `apply` วางแผนใหม่ใต้ล็อกแล้วทำต่อเมื่อ digest ตรง ไม่ตรงคืน `planChanged` พร้อมแผนใหม่ให้ผู้ใช้อ่านอีกครั้ง<br>• การตั้งค่าของแอปอยู่ใน `settings.json` แยกจาก `state.json` (เพราะ undo คืน state.json ทั้งไฟล์) ค่าเริ่มต้น: ภาษา en, ให้ธีมไปทุกหน้าต่าง, ตั้งโปรไฟล์ cmd เป็นค่าเริ่มต้นของ WT, อัปเดต auto, วิซาร์ดยังไม่เสร็จ · ไฟล์เสียหรือรุ่นใหม่กว่าถูกกันไว้เป็น `.damaged-<ms>` แล้วใช้ค่าเริ่มต้นและบอกผู้ใช้<br>• WebView2 ใช้โฟลเดอร์ `<data>\webview` ที่แอปกำหนดเอง (หน้าต่างประกาศ `create: false` แล้วสร้างใน setup ด้วย `data_directory`) `--cleanup` รู้จัก `drafts\`, `logs\`, `webview\`, `settings.json` และสำเนา damaged/temp ของมัน<br>• core เพิ่ม `run_apply_with` ที่รายงานทุกขั้น (สำรอง / ทำ / ตรวจ / ย้อน) ให้ event `apply-progress` และ feature `ts` ที่ส่งออก type ด้วย ts-rs 12.0.1 (bindings 71 ไฟล์ commit ไว้ที่ `app/ui/src/bindings` CI ตรวจว่าไม่เก่า)<br>• รหัสใหม่ `E_INTERNAL` สำหรับปัญหาของแอปเอง (§5.9)<br>• ธีมของฉันเป็น `themes\<ชื่อ>.cmdtheme` ร่างเป็น `drafts\<id>.cmdtheme` (บันทึกหนึ่งวินาทีหลังแก้) การนำเข้าอ่านไฟล์ครั้งเดียวแล้วพรีวิวก่อนยืนยัน id ซ้ำได้ id ใหม่<br>• การแก้ของ doctor ทุกข้อผ่านแผนเดียวกับการใช้ธีม (`doctor_fix` คืนคำขอและแผน) · ข้อมูลวินิจฉัยแทนชื่อผู้ใช้ด้วย `<you>` ก่อนคัดลอก<br>• โหมดอัปเดต (auto/notify/off) เก็บและแสดงในหน้าตั้งค่าแล้ว แต่ตัวอัปเดตเป็นงาน phase 4 (plugin, คีย์, latest.json, จังหวะติดตั้ง, ข้อความ "อัปเดตเป็น vX แล้ว" ซึ่งหน้าแรกเว้นที่ไว้แล้ว)<br>• UI ไม่มี library สำหรับ routing, state หรือ i18n: `Route` union, พจนานุกรม en/th อย่างละ 319 คีย์พร้อมเทสต์ความครบ, `Ipc` interface เดียวมี implementation ของ Tauri และ mock ในหน่วยความจำ (ข้อมูลตายตัวจาก preset ไม่มีตรรกะสี) · CSP ไม่มี origin ภายนอก `style-src 'self'` (React ตั้ง inline style ผ่าน CSSOM) ตรวจบน VM<br>• เทสต์ UI 32 ข้อกับ mock รวมเกณฑ์สำเร็จข้อ 1 (5 คลิก ไม่พิมพ์) และปุ่มทุกปุ่มมีชื่อทั้งสองภาษา · ยังไม่ได้รันแอปจริงบนเครื่อง dev (กติกาความปลอดภัย) การตรวจบน VM-A/VM-C เป็นของ controller ตาม `.superpowers/sdd/phase2b-vm-check.md` |
```

- [ ] **Step 2: Commit**

Run (in `<repo>`): `cargo test -p cmdstudio-app --lib error::` → `4 passed` (the §5.9 table is untouched). Stage `docs/superpowers/specs/2026-10-10-cmd-studio-design.md`. Message: `spec: appendix B — the decisions of phase 2b (the app)`.

---

### Task 4: The VM check protocol for spec §1 criterion 1 (the controller runs it)

Spec §8: phase 3 ends when criterion 1 passes on the Windows 11 VM; §7 names VM-A (Windows 11, English, fresh), VM-B (Windows 10) and VM-C (Windows 11, Thai user name). The dev PC never runs the app (Global Constraints), so this task writes the protocol the controller follows in the VMs with the artifact CI built, and what counts as a pass. There is no installer yet (Phase 4): the exe runs from a folder with `resources\` beside it.

**Files:**
- Create: `.superpowers/sdd/phase2b-vm-check.md`

- [ ] **Step 1: Write the protocol**

```markdown
# Phase 2b — VM check of spec §1 criterion 1 (controller)

Artifact: `cmdstudio-exe` from the CI run of the phase-3 commit (`cmdstudio.exe` + `resources\`). Copy both into one folder on the VM (e.g. `C:\CMDStudio\`); the exe looks for `resources\` beside itself. WebView2 is part of Windows 11; the VM needs no Node, no Rust, no network (the app downloads nothing).

## VM-A — Windows 11, English, fresh snapshot

1. Start `cmdstudio.exe` by double-click. PASS: the window opens in **English** on the wizard's page 1 within a few seconds, no console window appears, no message box.
2. Open WebView2's devtools (a debug build: right-click → Inspect; a release build has no devtools — skip) and read the console: PASS: **no CSP violation** (`style-src`, `connect-src`) and no error.
3. Five clicks, no typing: Next → (page 2 shows the real Windows build and "Windows Terminal is installed") Next → (Terracotta is preselected) Next → (every cmd window, "Open Windows Terminal with cmd" checked) Next → **Apply**. PASS: the progress shows steps (backing up n/N, applying n/N, checking), then "Done" with the preview.
4. "Try it: open cmd". PASS: a new Windows Terminal tab (or cmd window) opens with the Terracotta welcome box and the box-layout prompt, in English; the frame's right edge lines up; `✻` shows in Windows Terminal.
5. Open a second cmd window from the Start menu. PASS: the same theme (AutoRun).
6. "Go to the home page". PASS: Home shows "Everything is fine" (or the warnings the checks found) with the time, the theme in use with its preview, and the latest restore point.
7. Start `cmdstudio.exe` a second time while it is open. PASS: the first window is focused; no second window.
8. In cmd: `C:\CMDStudio\cmdstudio.exe doctor`. PASS: the report prints (after the prompt came back — a Windows-subsystem exe; `… doctor | more` waits). `cmdstudio.exe --help` prints the CLI's help. Note the behaviour for open question 1.
9. The after-apply bar's **Undo** (or Help → Restore points → Go back to this point). PASS: a new cmd window is plain again; Home says no theme is in use; the restore point is marked undone.
10. Settings → ไทย. PASS: every page is Thai; close and start the app again: it opens in **Thai** (remembered). Settings → English → the app opens in English again.
11. Themes → New theme → edit a color → Back to themes. PASS: a Draft card; the editor offers the draft on reopening.
12. Close the app. In cmd: `C:\CMDStudio\cmdstudio.exe --cleanup --data-dir %LOCALAPPDATA%\ai.resincore.cmdstudio`. PASS: exit code 0, the folder is gone except `themes\` (it holds the draft's saved copy, if any): `drafts\`, `logs\`, `webview\` and `settings.json` were accepted as CMD Studio's own.
13. `dir %LOCALAPPDATA%\ai.resincore.cmdstudio` shows only `themes\` (or nothing). Restore the snapshot.

## VM-C — Windows 11, Thai user name (and Thai folder)

1. Steps 1–4 of VM-A. PASS: the welcome box shows the Thai user name and a Thai folder (`cd` into one first) with the frame lined up — the Clink cell count of Phase 2a Decision 6 — and the prompt's path shows Thai as typed.
2. Help → Report a problem → Copy details; paste into Notepad. PASS: the text holds `<you>` where the Thai user name would be, and no occurrence of the name.
3. Step 10 of VM-A (Thai UI).
4. Step 12 of VM-A.

## VM-B — Windows 10 (information only, not a gate for phase 3)

Steps 1, 3, 4 of VM-A. Note whether the Store link appears on page 2 when Windows Terminal is missing, and whether the box's `*` fallback shows in conhost.

## Result

Write PASS/FAIL per step with a screenshot of page 1, page 5 (the plan), the themed cmd window, Home, and the Thai home page into `docs/spikes/phase2b-vm-check/` (no user name of the dev PC in the text). Criterion 1 of spec §1 is met when VM-A steps 1, 3, 4, 5 and 9 pass. Anything that fails goes back to the plan's owning task before the PR.
```

- [ ] **Step 2: Commit**

Stage `.superpowers/sdd/phase2b-vm-check.md`. Message: `sdd: the VM check protocol for phase 2b (spec §1 criterion 1)`. The controller runs the protocol with CI's artifact; the plan is done when it reports PASS (or lists what must change).
