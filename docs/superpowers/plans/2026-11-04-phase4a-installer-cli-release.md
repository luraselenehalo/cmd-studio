# Phase 4a Implementation Plan — The console CLI exe, the NSIS installer and its hooks, the user Path, the licenses, one version, the release workflow to a draft pre-release, the README, and the install/uninstall VM check

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Task 9's run is the controller's (VirtualBox VMs); an implementer never runs it. Nothing in this plan needs a founder step: every task runs and passes without the updater key, a published release, SignPath or winget.

**Goal:** CMD Studio becomes a thing a person installs: one NSIS installer (per user, no admin, x64 only) that puts `cmdstudio.exe`, a console `cmdstudio.com`, the bundled runtimes with their licenses and two Start-menu shortcuts in `%LOCALAPPDATA%\CMD Studio`, adds that folder to the user's Path so `cmdstudio doctor` typed in cmd prints before the prompt returns, starts the app, and whose uninstaller runs `--cleanup` (never on an update), takes the Path entry back, keeps the themes unless the box is ticked, and leaves the registry, Windows Terminal's settings and the fonts byte-equal to before; CI builds that installer on every push, and a tag `v*` builds it again, checks everything, and files a **draft** pre-release that only the founder publishes.

**Architecture:** The CLI gets a second binary in the app crate, `cmdstudio-cli.exe`, a console-subsystem program over the same `cmdstudio_cli::run`, staged as `cmdstudio.com` beside `cmdstudio.exe` by Tauri's `beforeBundleCommand` and bundled as a resource: cmd's PATHEXT tries `.COM` before `.EXE`, so a bare `cmdstudio` is the console program and the shortcuts stay on the exe (the `devenv.com` pattern). The user's Path is edited by Rust, not by NSIS script (NSIS strings hold 1024 characters; a longer Path would be cut): a pure module `user_path` in the core, a `UserPath` adapter (fake and Windows), and two CLI commands `--add-to-path` / `--remove-from-path` the hooks call. `app/src-tauri/windows/hooks.nsh` carries the four NSIS hooks: ARM64 refused before any file, the Recovery shortcut and the Path after the files, `cmdstudio.com --cleanup [--remove-themes]` through `nsExec` (no console window, output captured) and the Path removal before any file is deleted and only when `$UpdateMode <> 1`. One version lives in `Cargo.toml`'s workspace and a test pins the rest to it. `release.yml` repeats CI's gates, builds the bundle, checks the tag against the version and creates a draft pre-release with the installer and `SHA256SUMS.txt`; a commented SignPath step and a winget note wait for the founder.

**Tech Stack:** Rust 1.99.0, Tauri 2.12.4 (`@tauri-apps/cli` 2.12.3, tauri-bundler's NSIS 3.x with `nsExec`, `x64.nsh`, LogicLib), winreg 0.56.0, windows-sys 0.61.2 (`Win32_UI_WindowsAndMessaging` for `SendMessageTimeoutW`, already on in the core), React 19.3.1 / Vitest 5.0.5 for the UI's words, GitHub Actions on `windows-latest` (`gh`, `7z`, `python3` preinstalled), VirtualBox 7.2 VMs per `spikes/vm/README.md`.

**Spec:** `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` §1 (distribution), §2.1, §5.7 (the CLI table, `cmdstudio.com`, the Path commands), §5.9 (`E_PATH_WRITE`, Task 2), §5.10, §6.1, §6.3, §6.4, §6.5, §6.6, §7, §8 (the phase 4 row: install, update and uninstall pass on every tier; this plan covers install and uninstall, 4b update), §10, appendix B (the 2026-11-04 row); `docs/superpowers/specs/2026-10-17-recovery-mode-design.md` §4.4 (the "CMD Studio Recovery" shortcut); the founder's brief `.superpowers/sdd/phase4-plan/brief.md`; the facts of `docs/spikes/G10-G11-tauri.md` (`$UpdateMode`, `$DeleteAppDataCheckboxState`, the hook names, `%LOCALAPPDATA%\CMD Studio`); the 3b-2 rulings `docs/superpowers/plans/2026-11-03-phase3b-2-recovery-app-rulings.md` (M7). The controller's draft `.superpowers/sdd/phase4-design-brief.md` is superseded where it says "no updater in v0.1.0" (that is 4b's); its installer, hooks, licenses and README content stand where the spec agrees.

**Prerequisite:** branch `phase4`, cut from main `8255e96` (PR #11 merged). Measure the baselines at the start of Task 1 on this PC (`cargo test --workspace` per crate's `lib` count, `npm test`'s file and test count) and write them into Task 1's commit message; every count a later task states is a delta on them.

## Why 4a and 4b

Phase 4 splits in two along the one line the founder's steps draw: everything in 4a (this plan) builds, tests and ships without the updater key, without a published release and without SignPath, and its done-criterion is the half of spec §8's that needs no key, "install and uninstall pass on every tier"; 4b (`docs/superpowers/plans/2026-11-04-phase4b-updater-upstream.md`) is the updater, whose code and UI run over a fake first, whose install-moment spike runs on a VM with an ephemeral key, and whose last tasks wire the founder's key and check "update passes on every tier" against a real draft. 4a must come first because 4b's spike needs an installer that already carries the hooks (the `$UpdateMode` skip of `--cleanup` is what an update exercises), and because the first draft release is what SignPath asks for. The upstream-watch workflow is 4b's too: it only matters once updates reach users.

## Global Constraints

Every task's requirements include these. Values are copied from the spec, the brief and the founder's decisions where they bind here.

- **No Thai in cmd (spec §1, §5.7, appendix B 2026-10-13).** Nothing CMD Studio writes into a cmd window and nothing the CLI prints is Thai, on every Windows: `cmdstudio.com`'s output, `--add-to-path`'s and `--remove-from-path`'s words, the installer's message boxes (NSIS, English). `crates/core/tests/english_in_cmd.rs` and the CLI's `thai(…) == ""` tests keep passing.
- **The UI's languages (founder 2026-10-16).** The app opens in English; Thai is chosen in Settings. Every UI text is a key of both `app/ui/src/i18n/en.ts` and `th.ts`, in `th.ts`'s words (the prompt "พร้อมท์", the add-on "ส่วนเสริม", a version "เวอร์ชัน", the app "CMD Studio", a restore point "จุดย้อนกลับ"); `i18n.test.ts` fails on a missing key, an empty text, Thai in English or English-only Thai.
- **Rust production code** (outside `#[cfg(test)]` and `tests/`): no `unwrap`, `expect`, `panic!`, unchecked indexing or slicing, string slicing (the workspace denies clippy `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `string_slice`, `undocumented_unsafe_blocks`); typed errors; every `unsafe` block carries a `// SAFETY:` comment. Never `panic = "abort"` in any profile.
- **The names, each in one place:** the app exe `cmdstudio.exe` (`mainBinaryName` in `tauri.conf.json`, `console::APP_EXE`), the console exe `cmdstudio.com` (`console::CONSOLE_EXE`, the hooks file, the stage script), the install folder `%LOCALAPPDATA%\CMD Studio` (`$INSTDIR` of `currentUser`, G10), the Start-menu folder `CMD Studio` (`startMenuFolder` in `tauri.conf.json` and the hooks file, pinned equal by a test), the Recovery shortcut `CMD Studio Recovery.lnk` with the argument `--recovery` and nothing else (recovery spec §4.4), the data folder `%LOCALAPPDATA%\ai.resincore.cmdstudio` (unchanged), the user Path value `HKCU\Environment` `Path` (`registry::ENVIRONMENT`, `registry::PATH_VALUE`).
- **The CLI's contract (spec §5.7)** is unchanged for `apply`, `doctor`, `undo`, `--cleanup`, `set-aside`, `--help`: the same words, the same exit codes `0`/`1`/`2`/`3`, `--data-dir` and `--resources` on every command. The two new commands take no other argument and exit `0` when there is nothing to do.
- **The installer (spec §6.1, §6.3, G10):** NSIS, `installMode: "currentUser"`, no admin, `webviewInstallMode` left at Tauri's default (`downloadBootstrapper`), one file, every runtime and license inside (`resources\` as `scripts/fetch-resources.ps1` writes it, checked against `resources/manifest.json` by doctor D1), the app's own `LICENSE` as `LICENSE.txt`; ARM64 refused in `NSIS_HOOK_PREINSTALL`; the app started after install (Tauri's finish page, "Run CMD Studio" ticked); `--cleanup` only when `$UpdateMode <> 1`, with `--remove-themes` only when `$DeleteAppDataCheckboxState = 1`, through `cmdstudio.com` so the uninstaller waits and reads the exit code and the output; a failed cleanup never lets NSIS delete the data folder (the hook sets `$DeleteAppDataCheckboxState` to 0 and says so), because the restore points may be the only copy of the user's settings (spec §6.3).
- **The user Path (founder 2026-11-04, spec §5.7):** only the install folder is ever added or removed; every other entry stays byte for byte; the value's type (`REG_SZ` or `REG_EXPAND_SZ`) stays; a missing value is created as `REG_EXPAND_SZ`; a value that empties is deleted; a value that is not plain text (`RegText::NotText`) is never edited and never taken for absent, and is reported as `E_PATH_WRITE` with the hand fix; after every write `WM_SETTINGCHANGE` (`lParam` `"Environment"`) is broadcast with `SendMessageTimeoutW` (`SMTO_ABORTIFHUNG`, 5000 ms); the whole thing is idempotent.
- **One version (spec §6.4, brief):** `[workspace.package] version` in `Cargo.toml` is the source; `tauri.conf.json` has no `version` key (Tauri reads Cargo's; fallback F-V in Task 5 if the CLI does not resolve a workspace-inherited version); `app/package.json` and `app/package-lock.json` carry the same string and a test says so; the release workflow refuses a tag that is not `v<that version>`.
- **Release (spec §6.4, §6.5, brief):** the workflow runs on a pushed tag `v*` (the founder tags and pushes; agents never tag, never push) or by hand (`workflow_dispatch`, a dry run that uploads an artifact and creates nothing); it creates a **draft pre-release**, never publishes; the SignPath step is present, commented, disabled; winget is a founder step noted in the workflow and the README, not a job.
- **IPC, capability, CSP:** unchanged in 4a (the 37 commands, `apply-progress` the only event, the CSP of `config_guard.rs`). No new dependency in `Cargo.lock` and `package-lock.json` for 4a; the NSIS plugins come with tauri-bundler's NSIS.
- **Bindings.** No binding changes in 4a. If a task nevertheless changes an exported type, regenerate by the manual steps only: delete the generated `.ts` files that change, then `cargo test -p cmdstudio-app --lib export_bindings` with `TS_RS_EXPORT_DIR` set to `<repo>\app\ui\src\bindings`. Never run `scripts/export-bindings.ps1`; never pass `-ExecutionPolicy`.
- **The checks.** Each Rust task ends with `cargo fmt --all`, then `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features ts -- -D warnings`, and `cargo fmt --all -- --check`. Each UI task ends with `npm run typecheck` and `npm test`, then `TZ=UTC npm test` (Git Bash: `TZ=UTC npm test`; PowerShell: `$env:TZ='UTC'; npm test`), and, when it touches the bundle (Task 4), `npm run build` and `bash scripts/check-dist.sh`, all in `<repo>\app`. Workflow files are checked with `python3 -c "import yaml,sys; yaml.safe_load(open(sys.argv[1]))" <file>` (or `node -e` with the `yaml` package absent: then `npx` is not used; PyYAML is on this PC's Python or the check is marked "parsed in CI") and by CI itself.
- **PC safety (the brief, binding).** The dev PC is the founder's own. Never run the built app, `cmdstudio.exe`, `cmdstudio-cli.exe`/`cmdstudio.com`, the installer, the uninstaller, a VM guest script, a Lua probe or a stand-in binary; never run `npx tauri dev`, Oh My Posh or Clink. Run `cmd.exe` only with `/d`. Never touch this PC's AutoRun, fonts, Windows Terminal settings, `%LOCALAPPDATA%\ai.resincore.cmdstudio`, `%ProgramData%` or the user Path: the Path code is tested over the fake, and its one real-registry test is gated on `CMDSTUDIO_SYSTEM_TESTS` exactly `1`, which CI sets and this PC never does. Never read this PC's event log, registry or windows. Never pass `-ExecutionPolicy`. Never write a file outside the repo and the session scratchpad. Never set `CMDSTUDIO_SYSTEM_TESTS`, `CMDSTUDIO_OMP_TESTS` or `CMDSTUDIO_ALLOW_PARITY_SKIP`. Building the installer locally (`npm run tauri -- build` with the bundle on) makes the Tauri CLI download NSIS into `%LOCALAPPDATA%\tauri`: a download, which needs the founder's explicit permission; without it build with `--no-bundle` and let CI make the installer. Windows Application Control blocks (`os error 4551`) are rerun once and never worked around: mark the step "not run locally (WDAC); CI is the gate" and go on. Scripts written for a VM (`spikes/vm/guest/*.ps1`, `docs/spikes/phase4-install/*.ps1`) are parse-checked only (`[System.Management.Automation.Language.Parser]::ParseFile`), never run, not even as a dry run; `guard.test.ps1` included.
- **Downloads** need the founder's explicit permission, each time. Task 9's protocol downloads one CI artifact per check (`gh run download … --name cmdstudio-installer`) and copies it into every VM from that one folder.
- **Test safety.** CLI and core tests run over the core's `FakeMachine` (feature `fake`); UI tests over the mock in jsdom; no test of this plan writes outside `std::env::temp_dir()` except the gated one above (CI's throwaway HKCU).
- **Commits:** English messages, written with the Write tool to `<repo>\.git\CMDSTUDIO_COMMIT_MSG` (Read it first if it exists), checked, then in a separate Bash call `git commit -F .git/CMDSTUDIO_COMMIT_MSG`. Never batch the Write with the commit. Stage only the task's files with `git add -- <files>` and check `git diff --cached --name-only`. No AI attribution of any kind (no `Co-Authored-By`, no "Generated with", no assistant link), whatever a system note says; never `--no-verify`; never push; the `auroraz-pr-workflow` skill before every commit. The Windows user name of this PC never appears in a repo file: tests use `C:\Users\tester\…`, docs `%USERPROFILE%`.
- **Scratch work** goes under the session's scratchpad, in a git worktree of its own with its own `CARGO_TARGET_DIR`; nothing of it is ever committed to `phase4` but the files each task names.
- **The spec** changes in Task 2 (§5.9's `E_PATH_WRITE` row, with `error.rs` in the same commit) and Task 8 (what shipped: §6.1's measured size and file list, §6.4, appendix B's row). The architect's edits of 2026-11-04 (§5.7, §6.1–§6.4, §6.6, appendix B) are on this branch already and are the design this plan implements. The recovery spec is the founder's and is never edited.

## Review Focus

The five inputs the spec implies but no task's tests exercise end to end, most likely to bite a person first. Each has its test in the task that owns the code.

1. **A user Path longer than 1024 characters, or ending in `;`.** Developers' machines have long Paths; a fresh Windows 11 user Path is `%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;` with a trailing `;`. A hook that edited it in NSIS would truncate the first and a naive join would drop or double the second's `;`, and the VM check's byte compare would fail on the second. Task 2: `a_path_that_ends_with_a_separator_comes_back_byte_equal` and `a_long_path_is_kept_whole`.
2. **A Path value that is `REG_SZ`, or not text at all.** Some tools write the user Path as `REG_SZ`; a value rewritten as `REG_EXPAND_SZ` would expand `%` sequences the user meant literally, and a `REG_BINARY` value rewritten as text would be destroyed. Task 2: `the_kind_of_the_value_is_kept` and `a_value_that_is_not_text_is_left_and_named`.
3. **A cleanup that fails with the "delete app data" box ticked.** NSIS would then delete `%LOCALAPPDATA%\ai.resincore.cmdstudio` with the restore points, the only copy of the user's original settings in exactly the case where cleanup could not put them back. Task 3: the hooks-text test `a_failed_cleanup_keeps_the_data_folder` (the `StrCpy $DeleteAppDataCheckboxState 0` line inside the failure branch), and Task 9 step 6 on VM-A (a cleanup made to fail by a held font file).
4. **`cmdstudio` typed where `.COM` is not first in PATHEXT, or `cmdstudio.exe` typed by name.** Either runs the windows-subsystem exe, which cmd does not wait for: the output comes after the prompt and `%errorlevel%` is 0. Not a defect, but a surprise the README and `--help` must name. Task 1: `usage_names_the_two_programs`; Task 7: the README's CLI section.
5. **An update's uninstaller run (`$UpdateMode = 1`) removing the Path entry or the Recovery shortcut for good.** The old uninstaller runs during an update; if it took the Path entry out and the new installer did not add it back, `cmdstudio` would stop resolving after the first update, and the shortcut would vanish. Task 3: the hooks-text test `the_path_and_the_cleanup_run_only_outside_update_mode` and `the_recovery_shortcut_is_made_after_every_install`; 4b's update VM check reads the Path and the shortcut after an update.

## Decisions the founder may change

Each is the architect's choice of 2026-11-04 where the brief left room; appendix B records them. The report to the founder lists them one line each.

- **`cmdstudio.com`, not a differently named exe.** With a second exe named, say, `cmdstudio-cli.exe`, every document, the spec's CLI table and the usage text would have to say a name nobody expects, and `cmdstudio doctor` would still run the window's exe. `.com` keeps the one name; the cost is that Explorer calls the file "MS-DOS Application" (it is a normal PE; Windows runs it by its header) and that a PATHEXT edited to put `.EXE` first gives the windows-subsystem exe (Review Focus 4). To change: rename `CONSOLE_EXE`, the stage script's target, the hooks file and the resource key; the Rust is the same.
- **The install folder goes on the user Path.** Without it the CLI promise of §2.1 ("a CLI that works without a screen") needs a full path typed. The entry is the literal folder (`C:\Users\<name>\AppData\Local\CMD Studio`), not `%LOCALAPPDATA%\CMD Studio`, so that a `REG_SZ` Path stays `REG_SZ` and the entry means the same in both kinds.
- **Rust edits the Path; the hooks only call it.** Two more CLI commands instead of NSIS string work, for Review Focus 1 and 2. They are listed in `--help` under "for the installer".
- **`nsExec::ExecToStack` for the hooks' calls.** No console window flashes, the exit code and the output come back, and the uninstaller's box can show what `--cleanup` printed. If tauri-bundler's NSIS lacks `nsExec` (Task 3's gate G-NSIS), the fallback is `ExecWait` of `cmdstudio.exe --cleanup` (no window, the code only).
- **The Start-menu folder `CMD Studio` holds both shortcuts** ("CMD Studio", "CMD Studio Recovery"); Tauri's default would put the app's shortcut straight in Programs and the Recovery one would stand alone.
- **CI builds the installer on every push** (ci.yml's `app` job, bundle on) so a hooks-file mistake fails a pull request, not a release; `release.yml` builds it again on a tag. The VM checks use CI's artifact.
- **The draft is a pre-release named `CMD Studio v0.1.0`** with the SmartScreen text of §6.5 step 1; the founder publishes.

## Implementation Gate (spec §0) for this plan

Before the task that uses each, the implementer opens the real thing and reads the shape, not the name; a mismatch stops the task and goes to the founder, and the spec is changed to the real thing.

| # | Confirm | Where | Used by |
|---|---|---|---|
| G-NSI | In the `installer.nsi` template of the tauri-bundler that `@tauri-apps/cli` 2.12.3 ships (`node_modules/@tauri-apps/cli-win32-x64-msvc` embeds it; or `https://raw.githubusercontent.com/tauri-apps/tauri/<tauri-cli tag>/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi`): the four hook names and their places (G10), `Var UpdateMode`, `Var DeleteAppDataCheckboxState`, `!include LogicLib.nsh`, the finish page's run checkbox, how `startMenuFolder` names the folder (`$SMPROGRAMS\<folder>\`), and that resources are installed under `$INSTDIR` at their target path | tauri repo at the cli's tag | Task 3 |
| G-NSIS | tauri-bundler's NSIS download (`nsis.rs`: the `NSIS_URL` zip) holds `Plugins\x86-unicode\nsExec.dll` and an `Include\x64.nsh` with `IsNativeARM64` | the zip's listing | Task 3 (fallbacks in the task) |
| G-RES | `bundle.resources` as a map (`{"source": "target"}`) exists in tauri-utils 2.x's `BundleResources`, a source path that does not exist at `cargo build` time (tauri-build's resource copy) does not fail the build, and `build.beforeBundleCommand` runs after the cargo build and before bundling | `tauri-utils` `config.rs`, `tauri-build` `lib.rs`, `tauri-cli` `build.rs` | Task 3 (fallback F-A) |
| G-VER | The Tauri CLI reads `version.workspace = true` from the workspace `Cargo.toml` | `tauri-cli` `interface/rust/manifest.rs` or the installer's file name in CI | Task 5 (fallback F-V) |
| G-GH | `gh release create --draft --prerelease` with `GH_TOKEN: ${{ github.token }}` and `permissions: contents: write` creates a draft nobody but collaborators sees | GitHub's docs for `gh release create` | Task 6 |

## File map

New files, one responsibility each:

- `app/src-tauri/src/bin/cli.rs` — the console exe's `main` (Task 1).
- `app/src-tauri/src/console.rs` — `ConsoleStart`, `console_start`, `start_app_beside`, `APP_EXE`, `CONSOLE_EXE` (Task 1).
- `scripts/stage-console-exe.mjs` — copies `cmdstudio-cli.exe` to `app/src-tauri/target-stage/cmdstudio.com` before bundling (Task 3).
- `crates/core/src/user_path.rs` — the pure Path rules and `add`/`remove` over the adapter (Task 2).
- `crates/core/src/adapters/mod.rs` — `UserPath` trait, `System.user_path` (Task 2).
- `crates/core/src/adapters/fake.rs` — `FakeUserPath`, `FakeMachine.user_path` (Task 2).
- `crates/core/src/adapters/windows/registry.rs` — `WinUserPath`, `ENVIRONMENT`, `PATH_VALUE`, the broadcast (Task 2).
- `crates/cli/src/args.rs`, `crates/cli/src/lib.rs` — `Command::AddToPath`, `Command::RemoveFromPath`, `Defaults.exe_dir`, `Say::add_to_path`, `Say::remove_from_path` (Task 2).
- `app/src-tauri/src/error.rs` — the `E_PATH_WRITE` row (Task 2).
- `app/src-tauri/windows/hooks.nsh` — the four hooks (Task 3).
- `app/src-tauri/tauri.conf.json`, `app/src-tauri/src/config_guard.rs` — the bundle, pinned (Tasks 3, 5).
- `.github/workflows/ci.yml` — the console exe checks, the bundle, the installer's listing, the artifact (Tasks 1, 3).
- `.github/workflows/release.yml`, `.github/release-notes.md` — the tag build and the draft (Task 6).
- `app/ui/src/i18n/en.ts`, `th.ts`, `i18n.test.ts`, `app/ui/src/screens/Help.tsx`, `Help.test.tsx`, `docs/credits.md` — the license-files sentence, M7's sentence (Task 4).
- `README.md` — the English README (Task 7).
- `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` — §5.9 (Task 2), §6.1, §6.4, appendix B (Task 8).
- `docs/spikes/phase4-install/protocol.md`, `helpers.ps1`, `spikes/vm/guest/install-check.ps1`, `spikes/vm/guest/guard.test.ps1` — the VM check (Task 9).

---

### Task 1: The console exe `cmdstudio.com`

The founder's decision of 2026-11-04 ("ทำ exe แยกในเฟส 4"). A second binary of the app crate, console subsystem, over `cmdstudio_cli::run`; `--recovery` alone is handed to `cmdstudio.exe` beside it. CI runs it (`--help`, a bad command) and checks both binaries' PE subsystem; nothing runs on this PC.

**Files:**
- Modify: `app/src-tauri/Cargo.toml` (a second `[[bin]]`)
- Create: `app/src-tauri/src/bin/cli.rs`
- Modify: `app/src-tauri/src/console.rs` (`ConsoleStart`, `console_start`, `start_app_beside`, `APP_EXE`, `CONSOLE_EXE`)
- Modify: `app/src-tauri/src/main.rs` (the module doc: the `.com` is the way to type it)
- Modify: `crates/cli/src/args.rs` (`USAGE` names the two programs)
- Modify: `.github/workflows/ci.yml` (`app` job: the subsystem check, the `.com` answers; the stage holds `cmdstudio.com`)

**Interfaces:**
- Consumes: `cmdstudio_cli::run(&[OsString], &mut Io) -> i32`, `cmdstudio_cli::{EXIT_OK, EXIT_FAILED, EXIT_INVALID, now_ms}`, `cmdstudio_app::machine::real_backend()`, `cmdstudio_app::crash::install_hook(logs, redaction, now_ms)`, `cmdstudio_app::RECOVERY_ARG`.
- Produces: `cmdstudio_app::console::{ConsoleStart, console_start(&[OsString]) -> ConsoleStart, start_app_beside(&[OsString]) -> Result<(), String>, APP_EXE: &str = "cmdstudio.exe", CONSOLE_EXE: &str = "cmdstudio.com"}`; the binary `target/release/cmdstudio-cli.exe` (Task 3 stages it as `cmdstudio.com`).

- [ ] **Step 1: Write the failing tests in `console.rs`**

Add to `app/src-tauri/src/console.rs`'s test module (it is `#[cfg(all(test, windows))]`; add a second, unconditional `#[cfg(test)] mod start_tests`):

```rust
#[cfg(test)]
mod start_tests {
    use super::{console_start, ConsoleStart, APP_EXE, CONSOLE_EXE};
    use std::ffi::OsString;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    /// `cmdstudio.com --recovery` opens the window (the app's job): it is handed to
    /// `cmdstudio.exe`. Everything else, `--after-pid` and a `--recovery` with company
    /// included, is the CLI's, which refuses what it does not know with exit code 2.
    #[test]
    fn only_a_lone_recovery_flag_is_handed_to_the_app() {
        assert_eq!(console_start(&args(&["--recovery"])), ConsoleStart::OpenRecovery);
        for other in [
            vec![],
            vec!["doctor"],
            vec!["--recovery", "--data-dir", r"C:\d"],
            vec!["--after-pid", "1234"],
            vec!["--cleanup"],
        ] {
            assert_eq!(console_start(&args(&other)), ConsoleStart::Cli, "{other:?}");
        }
    }

    /// The two names, in one place each; the hooks file and the stage script spell them too.
    #[test]
    fn the_two_programs_have_their_names() {
        assert_eq!(APP_EXE, "cmdstudio.exe");
        assert_eq!(CONSOLE_EXE, "cmdstudio.com");
    }
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p cmdstudio-app --lib console::start_tests`
Expected: compile error, `console_start` and `ConsoleStart` not found.

- [ ] **Step 3: `console.rs`: the names, `ConsoleStart`, `console_start`, `start_app_beside`**

Add after the `Stdout` impl:

```rust
/// The app's exe, which the Start-menu shortcuts and the uninstaller name.
pub const APP_EXE: &str = "cmdstudio.exe";
/// The console exe beside it (spec §5.7): the same CLI as a console-subsystem program, which cmd
/// waits for and gets an exit code from. `cmdstudio` typed in cmd or PowerShell is this one, as
/// PATHEXT tries `.COM` before `.EXE`.
pub const CONSOLE_EXE: &str = "cmdstudio.com";

/// What `cmdstudio.com` does with its arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleStart {
    /// `--recovery` alone: the window in recovery mode is the app's job (recovery spec §4.4);
    /// the console exe starts `cmdstudio.exe --recovery` beside itself and ends.
    OpenRecovery,
    /// Everything else is `cmdstudio_cli::run`'s. `--after-pid <pid>` included: it is
    /// `app_restart`'s own and never typed, and the CLI refuses it as it refuses any unknown
    /// option (exit code 2).
    Cli,
}

pub fn console_start(args: &[std::ffi::OsString]) -> ConsoleStart {
    match args {
        [one] if one == crate::RECOVERY_ARG => ConsoleStart::OpenRecovery,
        _ => ConsoleStart::Cli,
    }
}

/// Starts [`APP_EXE`] from this program's own folder with `args`, without waiting: the window
/// outlives the console program. `Err` says what failed, in English.
#[cfg(windows)]
pub fn start_app_beside(args: &[std::ffi::OsString]) -> Result<(), String> {
    let me = std::env::current_exe()
        .map_err(|e| format!("this program's own path could not be read: {e}"))?;
    let folder = me
        .parent()
        .ok_or_else(|| "this program has no folder".to_string())?;
    let app = folder.join(APP_EXE);
    std::process::Command::new(&app)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{} could not be started: {e}", app.display()))
}
```

`crate::RECOVERY_ARG` is `pub const RECOVERY_ARG: &str = "--recovery"` in `lib.rs:24`; `OsString == &str` compares through `PartialEq<str> for OsString` as `main.rs` already does (`one == cmdstudio_app::RECOVERY_ARG`).

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p cmdstudio-app --lib console::`
Expected: the two new tests and the existing `the_parent_console_is_attached_only_when_the_output_goes_nowhere` pass.

- [ ] **Step 5: The second binary**

In `app/src-tauri/Cargo.toml`, after the existing `[[bin]]`:

```toml
# The console exe (spec §5.7, phase 4a): the same CLI, built as a console-subsystem program and
# put beside cmdstudio.exe as cmdstudio.com by `tauri build` (scripts/stage-console-exe.mjs,
# build.beforeBundleCommand), so `cmdstudio doctor` typed in cmd prints before the prompt returns
# and hands cmd its exit code. cmd's PATHEXT tries .COM before .EXE for a bare `cmdstudio`.
[[bin]]
name = "cmdstudio-cli"
path = "src/bin/cli.rs"
```

Create `app/src-tauri/src/bin/cli.rs`:

```rust
//! `cmdstudio.com`: the CLI as a console-subsystem program (spec §5.7). cmd waits for it, shows
//! what it prints as it prints, and gets its exit code; `cmdstudio.exe` stays the window, a
//! Windows-subsystem exe cmd does not wait for. Built as `cmdstudio-cli.exe`, staged beside the
//! app as `cmdstudio.com` by `tauri build`, where cmd's PATHEXT (`.COM` before `.EXE`) picks it
//! for a bare `cmdstudio`. The uninstaller runs `cmdstudio.com --cleanup` (spec §6.3) and the
//! installer `cmdstudio.com --add-to-path`.
//!
//! `--recovery` alone belongs to the window: it is handed to `cmdstudio.exe` in the same folder
//! (recovery spec §4.4) and this process ends with 0 once that one has started. Everything else
//! is `cmdstudio_cli::run`, as in `main.rs`, and the crash-note hook comes first, as there.
#[cfg(windows)]
fn main() {
    use cmdstudio_app::console::{console_start, start_app_beside, ConsoleStart};
    if let Ok(backend) = cmdstudio_app::machine::real_backend() {
        cmdstudio_app::crash::install_hook(
            backend.paths().logs(),
            backend.redaction(),
            cmdstudio_cli::now_ms,
        );
    }
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let code = match console_start(&args) {
        ConsoleStart::OpenRecovery => match start_app_beside(&args) {
            Ok(()) => cmdstudio_cli::EXIT_OK,
            Err(why) => {
                eprintln!("cmdstudio: E_INTERNAL: {why}");
                cmdstudio_cli::EXIT_FAILED
            }
        },
        ConsoleStart::Cli => {
            let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
            cmdstudio_cli::run(
                &args,
                &mut cmdstudio_cli::Io {
                    out: &mut out,
                    err: &mut err,
                },
            )
        }
    };
    std::process::exit(code);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("CMD Studio runs on Windows only.");
    std::process::exit(cmdstudio_cli::EXIT_INVALID);
}
```

No `windows_subsystem` attribute: that is what makes it a console program. `E_INTERNAL` is a row of spec §5.9 already (`crates/core/tests/error_codes.rs` counts production literals: this one is used, so the test still passes).

In `app/src-tauri/src/main.rs`, replace the module doc's third paragraph ("cmd does not wait for a Windows-subsystem program, …") with:

```rust
//! cmd does not wait for a Windows-subsystem program, so `cmdstudio.exe doctor` typed by name
//! prints after the prompt came back and leaves `%errorlevel%` at 0; `cmdstudio doctor` without
//! the extension runs `cmdstudio.com` beside this exe (`src/bin/cli.rs`, console subsystem,
//! phase 4a), which cmd waits for. This CLI mode stays for `cmdstudio.exe` typed by name,
//! `start /wait`, and a PATHEXT that puts `.EXE` first. The uninstaller runs the `.com`, and
//! tests call the library: both wait.
```

- [ ] **Step 6: `USAGE` names the two programs**

In `crates/cli/src/args.rs`, after the "Options for every command" block and before "Exit codes", add to `USAGE`:

```
Two programs:
  cmdstudio.com   this console program: type `cmdstudio`, cmd waits and gets the exit code
  cmdstudio.exe   the app's window; `cmdstudio.exe doctor` prints after the prompt returns

For the installer (it calls these; they take no other argument):
  cmdstudio --add-to-path         put this program's folder on your Path (HKCU\\Environment)
  cmdstudio --remove-from-path    take it off again; other entries are never touched
```

(Task 2 makes the two commands real; until then they are words in the usage text only, and `parse` still answers "unknown option" for them.) Add to the `usage` test's `for command in [...]` list in `args.rs` the strings `"cmdstudio.com"`, `"cmdstudio.exe"`, `"--add-to-path"`, `"--remove-from-path"`; the test is named `usage_names_the_two_programs` if it has no name yet (keep the existing name otherwise and add the four strings).

- [ ] **Step 7: Run the CLI's tests**

Run: `cargo test -p cmdstudio-cli`
Expected: all pass (the usage test with its four new strings).

- [ ] **Step 8: CI runs the console exe**

In `.github/workflows/ci.yml`, `app` job, after the step "The UI the exe embeds holds no mock data and no preset file" and before "Stage the exe with its resources", add:

```yaml
      # The two binaries of the app crate (spec §5.7, phase 4a): cmdstudio.exe is a windows
      # program (PE subsystem 2) and cmdstudio-cli.exe, bundled as cmdstudio.com, a console one
      # (subsystem 3), which answers --help with its usage and exit 0, and an unknown command with
      # exit 2. The subsystem is the 16-bit field at offset 0x5C of the optional header, found
      # through e_lfanew (the 32-bit field at 0x3C). Nothing here touches the runner beyond
      # reading its %LOCALAPPDATA% for the crash-note hook, which writes only on a panic.
      - name: The console exe answers in a console; the app exe is a windows program
        shell: bash
        run: |
          subsystem() {
            python3 -c "import struct,sys; d=open(sys.argv[1],'rb').read(); off=struct.unpack_from('<I',d,0x3C)[0]; print(struct.unpack_from('<H',d,off+0x5C)[0])" "$1"
          }
          test "$(subsystem target/release/cmdstudio.exe)" = 2
          test "$(subsystem target/release/cmdstudio-cli.exe)" = 3
          ./target/release/cmdstudio-cli.exe --help | head -1 | grep -q '^Usage:'
          code=0; ./target/release/cmdstudio-cli.exe frobnicate 2>/dev/null || code=$?
          test "$code" -eq 2
```

In the step "Stage the exe with its resources, as the app finds them", after `cp target/release/cmdstudio.exe stage/cmdstudio/`, add `cp target/release/cmdstudio-cli.exe stage/cmdstudio/cmdstudio.com` and extend the comment: "and the console exe under its bundled name".

- [ ] **Step 9: The checks, then commit**

Run the four clippy commands, `cargo fmt --all` and `cargo fmt --all -- --check`, `cargo test -p cmdstudio-app --lib`, `cargo test -p cmdstudio-cli`. Parse `ci.yml` (`python3 -c "import yaml,sys; yaml.safe_load(open(sys.argv[1]))" .github/workflows/ci.yml`; if PyYAML is absent on this PC, say "parsed in CI").

Write `.git/CMDSTUDIO_COMMIT_MSG`:

```
app, cli: the console exe cmdstudio.com beside cmdstudio.exe (spec §5.7, phase 4a)

A second binary of the app crate, src/bin/cli.rs, console subsystem,
over the same cmdstudio_cli::run: cmd waits for it and gets its exit
code; `--recovery` alone is handed to cmdstudio.exe beside it.
console.rs: ConsoleStart, console_start, start_app_beside, the two
names. USAGE names the two programs and the installer's two commands.
CI checks both binaries' PE subsystem, runs the console exe's --help
and an unknown command, and stages it as cmdstudio.com.

Baselines at 8255e96 on this PC: <the counts measured>.
```

Then `git add -- app/src-tauri/Cargo.toml app/src-tauri/src/bin/cli.rs app/src-tauri/src/console.rs app/src-tauri/src/main.rs crates/cli/src/args.rs .github/workflows/ci.yml`, check `git diff --cached --name-only`, then `git commit -F .git/CMDSTUDIO_COMMIT_MSG`.

---

### Task 2: The user Path: the pure rules, the `UserPath` adapter, `--add-to-path` and `--remove-from-path`, `E_PATH_WRITE`

Rust edits `HKCU\Environment` `Path` (Global Constraints "The user Path"); the hooks of Task 3 call these two commands. Pure rules in the core, tested on their own; the adapter's Windows half runs only on CI's throwaway HKCU.

**Files:**
- Create: `crates/core/src/user_path.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod user_path;`)
- Modify: `crates/core/src/adapters/mod.rs` (`UserPath` trait; `System.user_path`)
- Modify: `crates/core/src/adapters/fake.rs` (`FakeUserPath`; `FakeMachine.user_path`; `system()`)
- Modify: `crates/core/src/adapters/windows/registry.rs` (`ENVIRONMENT`, `PATH_VALUE`, `WinUserPath`, the broadcast)
- Modify: `crates/cli/src/args.rs` (`Command::AddToPath`, `Command::RemoveFromPath`)
- Modify: `crates/cli/src/lib.rs` (`Defaults.exe_dir`; `Say::add_to_path`, `Say::remove_from_path`; `run` and `run_with`; every `System { … }` and `Defaults { … }` literal)
- Modify: `crates/cli/tests/system.rs` (the `System` and `Defaults` literals; the gated real-Path test)
- Modify: `app/src-tauri/src/machine.rs`, `app/src-tauri/src/library.rs` (the `System` literals)
- Modify: `app/src-tauri/src/error.rs` (`E_PATH_WRITE` in `MEANINGS`)
- Modify: `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§5.9: the `E_PATH_WRITE` row, same commit)

**Interfaces:**
- Consumes: `RegValue { kind, bytes }`, `RegValue::from_text`, `RegValue::text`, `RegText::of`, `REG_SZ`, `REG_EXPAND_SZ`, `AdapterError::new`, `RegLocation::{user, read_raw, write_raw, delete_value}`, `adapters::windows::wide`.
- Produces: `cmdstudio_core::user_path::{has_entry, with_entry, without_entry, add, remove, PathChange, PathError}`; `cmdstudio_core::adapters::UserPath` (`read`, `write`, `delete`, `broadcast_change`) and `System.user_path`; `FakeUserPath { value, broadcasts, fail_reads, fail_writes }`; `WinUserPath::{real, at}`; `Command::{AddToPath, RemoveFromPath}`; `Defaults.exe_dir: PathBuf`; the CLI's words below; the §5.9 row `E_PATH_WRITE`.

- [ ] **Step 1: The pure rules: failing tests**

Create `crates/core/src/user_path.rs` with the tests first (the module body comes in Step 3):

```rust
//! The user's `Path` (`HKCU\Environment`, spec §5.7, phase 4a): the install folder goes on it
//! so that cmd finds `cmdstudio`, and comes off it at uninstall. The text rules here are pure;
//! [`add`] and [`remove`] run them over a [`UserPath`] adapter. Every other entry stays byte for
//! byte: the value is split at `;` only, entries are compared without case and without trailing
//! `\`, and CMD Studio's entry takes exactly one separator with it, so that taking it out gives
//! the old text back (a value that ended with `;` gets `<dir>;` appended before that end, one
//! that did not gets `;<dir>`, an empty or absent one becomes `<dir>`). Any text that is not
//! plain `REG_SZ`/`REG_EXPAND_SZ` text is never edited (spec §5.4's rule for AutoRun).
use crate::adapters::{AdapterError, RegText, RegValue, UserPath, REG_EXPAND_SZ};
use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeUserPath;
    use crate::adapters::REG_SZ;
    use std::path::PathBuf;

    const DIR: &str = r"C:\Users\tester\AppData\Local\CMD Studio";
    const APPS: &str = r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps";

    fn dir() -> PathBuf {
        PathBuf::from(DIR)
    }

    /// Review Focus 1: a fresh Windows 11 user Path ends with `;`. Adding and removing gives the
    /// old text back exactly, and so for a value without the trailing `;`, an empty one, and
    /// one where the user appended something after ours.
    #[test]
    fn a_path_that_ends_with_a_separator_comes_back_byte_equal() {
        for (before, after_add) in [
            (format!("{APPS};"), format!("{APPS};{DIR};")),
            (APPS.to_string(), format!("{APPS};{DIR}")),
            (String::new(), DIR.to_string()),
            (";".to_string(), format!(";{DIR};")),
        ] {
            assert_eq!(with_entry(Some(&before), &dir()).as_deref(), Some(after_add.as_str()), "{before:?}");
            assert_eq!(without_entry(&after_add, &dir()).as_deref(), Some(before.as_str()), "{after_add:?}");
        }
        assert_eq!(with_entry(None, &dir()).as_deref(), Some(DIR));
        // The user added an entry after ours: ours goes, theirs stays.
        let later = format!("{APPS};{DIR};C:\\theirs");
        assert_eq!(without_entry(&later, &dir()).as_deref(), Some(&*format!("{APPS};C:\\theirs")));
    }

    /// Case and a trailing backslash do not make a different entry; nothing else is touched,
    /// not even an empty entry (`;;`) or a quoted one.
    #[test]
    fn the_entry_is_found_without_case_and_trailing_backslash_and_nothing_else_moves() {
        let value = format!("C:\\a;;\"C:\\q;r\";{};C:\\b\\", DIR.to_uppercase() + "\\");
        assert!(has_entry(Some(&value), &dir()));
        assert_eq!(with_entry(Some(&value), &dir()), None, "already there: no change");
        assert_eq!(without_entry(&value, &dir()).as_deref(), Some("C:\\a;;\"C:\\q;r\";C:\\b\\"));
        assert_eq!(without_entry("C:\\a;C:\\b", &dir()), None, "not there: no change");
        assert!(!has_entry(None, &dir()));
        assert!(!has_entry(Some("C:\\Users\\tester\\AppData\\Local\\CMD Studio 2"), &dir()));
    }

    /// Review Focus 1: a Path far longer than NSIS's 1024-character strings is kept whole.
    #[test]
    fn a_long_path_is_kept_whole() {
        let long: String = (0..200).map(|i| format!("C:\\tools\\entry-{i:03};")).collect();
        assert!(long.len() > 3000);
        let added = with_entry(Some(&long), &dir()).unwrap();
        assert_eq!(added, format!("{long}{DIR};"));
        assert_eq!(without_entry(&added, &dir()).unwrap(), long);
    }

    /// Review Focus 2: the kind of the value is kept (`REG_SZ` stays `REG_SZ`); a missing value
    /// is made as `REG_EXPAND_SZ`; a value that empties is deleted; the broadcast follows every
    /// write and no non-write.
    #[test]
    fn the_kind_of_the_value_is_kept() {
        let p = FakeUserPath::new();
        assert_eq!(add(&p, &dir()).unwrap(), PathChange::Added);
        assert_eq!(p.value(), Some(RegValue::from_text(REG_EXPAND_SZ, DIR)));
        assert_eq!(p.broadcasts(), 1);
        assert_eq!(add(&p, &dir()).unwrap(), PathChange::AlreadyThere);
        assert_eq!(p.broadcasts(), 1, "nothing written, nothing broadcast");
        assert_eq!(remove(&p, &dir()).unwrap(), PathChange::Removed);
        assert_eq!(p.value(), None, "an empty Path is deleted, as it was absent before");
        assert_eq!(p.broadcasts(), 2);
        assert_eq!(remove(&p, &dir()).unwrap(), PathChange::NotThere);
        assert_eq!(p.broadcasts(), 2);

        p.set(Some(RegValue::from_text(REG_SZ, "C:\\a")));
        assert_eq!(add(&p, &dir()).unwrap(), PathChange::Added);
        assert_eq!(p.value(), Some(RegValue::from_text(REG_SZ, &format!("C:\\a;{DIR}"))));
        assert_eq!(remove(&p, &dir()).unwrap(), PathChange::Removed);
        assert_eq!(p.value(), Some(RegValue::from_text(REG_SZ, "C:\\a")));
    }

    /// Review Focus 2: a value that is not plain text (a `REG_BINARY`, or text with bytes after
    /// its NUL) is left alone and named, never read as absent and never written over.
    #[test]
    fn a_value_that_is_not_text_is_left_and_named() {
        let p = FakeUserPath::new();
        p.set(Some(RegValue { kind: 3, bytes: vec![1, 2, 3] }));
        for change in [add(&p, &dir()), remove(&p, &dir())] {
            match change {
                Err(PathError::NotText { kind }) => assert_eq!(kind, 3),
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(p.value(), Some(RegValue { kind: 3, bytes: vec![1, 2, 3] }));
        assert_eq!(p.broadcasts(), 0);
        assert_eq!(PathError::NotText { kind: 3 }.code(), "E_PATH_WRITE");
    }

    /// A registry that cannot be read is `E_MACHINE_READ`; one that cannot be written is
    /// `E_PATH_WRITE`, and the broadcast is not sent for a write that failed.
    #[test]
    fn read_and_write_failures_have_their_codes() {
        let p = FakeUserPath::new();
        p.fail_reads.set(true);
        assert_eq!(add(&p, &dir()).unwrap_err().code(), "E_MACHINE_READ");
        p.fail_reads.set(false);
        p.fail_writes.set(true);
        let e = add(&p, &dir()).unwrap_err();
        assert_eq!(e.code(), "E_PATH_WRITE");
        assert!(e.to_string().contains("access denied (injected)"), "{e}");
        assert_eq!(p.broadcasts(), 0);
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Add `pub mod user_path;` to `crates/core/src/lib.rs` (in alphabetical order, after `txn`). Run: `cargo test -p cmdstudio-core --features fake --lib user_path`
Expected: compile errors (`UserPath`, `FakeUserPath`, `with_entry`… not found).

- [ ] **Step 3: The adapter trait, the fake, the pure module**

In `crates/core/src/adapters/mod.rs`, after the `AutoRun` trait:

```rust
/// The user's `Path` value under `HKCU\Environment` (spec §5.7, phase 4a), read and written raw
/// like AutoRun so that its kind and every byte survive. Only `user_path::{add, remove}` call it.
pub trait UserPath {
    /// The value as stored; `None` when absent.
    fn read(&self) -> Result<Option<RegValue>, AdapterError>;
    /// Writes the value exactly as given, kind included; opens the existing `Environment` key
    /// and creates it only when missing, never recreates it (spec §2.2 lesson 9).
    fn write(&self, value: &RegValue) -> Result<(), AdapterError>;
    /// Deletes the value; `Ok` when it is absent.
    fn delete(&self) -> Result<(), AdapterError>;
    /// Tells every top-level window that the environment changed (`WM_SETTINGCHANGE` with
    /// `"Environment"`, `SendMessageTimeoutW`, abort if hung, 5 s), so that a cmd window opened
    /// from Explorer or the Start menu afterwards sees the new Path. Best effort: a window that
    /// does not answer is not an error.
    fn broadcast_change(&self);
}
```

Add `pub user_path: &'a dyn UserPath,` to `System` (after `event_log`).

In `crates/core/src/adapters/fake.rs`, after `FakeAutoRun`'s impl:

```rust
/// The user's Path in memory (phase 4a).
#[derive(Default)]
pub struct FakeUserPath {
    value: RefCell<Option<RegValue>>,
    broadcasts: Cell<u32>,
    pub fail_reads: Cell<bool>,
    pub fail_writes: Cell<bool>,
}

impl FakeUserPath {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn value(&self) -> Option<RegValue> {
        self.value.borrow().clone()
    }
    pub fn set(&self, value: Option<RegValue>) {
        *self.value.borrow_mut() = value;
    }
    /// How many times the change was broadcast.
    pub fn broadcasts(&self) -> u32 {
        self.broadcasts.get()
    }
}

impl UserPath for FakeUserPath {
    fn read(&self) -> Result<Option<RegValue>, AdapterError> {
        if self.fail_reads.get() {
            return Err(AdapterError::new("read Path", "access denied (injected)"));
        }
        Ok(self.value())
    }
    fn write(&self, value: &RegValue) -> Result<(), AdapterError> {
        if self.fail_writes.get() {
            return Err(AdapterError::new("write Path", "access denied (injected)"));
        }
        self.set(Some(value.clone()));
        Ok(())
    }
    fn delete(&self) -> Result<(), AdapterError> {
        if self.fail_writes.get() {
            return Err(AdapterError::new("delete Path", "access denied (injected)"));
        }
        self.set(None);
        Ok(())
    }
    fn broadcast_change(&self) {
        self.broadcasts.set(self.broadcasts.get() + 1);
    }
}
```

(`use std::cell::{Cell, RefCell}` is in the file already for the other fakes; add `UserPath` to the `use crate::adapters::{…}` line.) Add `pub user_path: FakeUserPath,` to `FakeMachine`, `user_path: FakeUserPath::new(),` in `new()`, and `user_path: &self.user_path,` in `system()`.

The pure module and the two operations, in `user_path.rs` above the tests:

```rust
/// Two entries are the same folder when they match without case and without trailing
/// backslashes; an empty entry is never a match.
fn same_entry(entry: &str, dir: &str) -> bool {
    let norm = |s: &str| s.trim_end_matches('\\').to_lowercase();
    !entry.trim().is_empty() && norm(entry) == norm(dir)
}

/// Whether `value` (absent, or its text) holds `dir`.
pub fn has_entry(value: Option<&str>, dir: &Path) -> bool {
    let dir = dir.to_string_lossy();
    value.is_some_and(|v| v.split(';').any(|e| same_entry(e, &dir)))
}

/// The value with `dir` added at its end, or `None` when it is already there. A value that ends
/// with `;` keeps ending with `;` (the entry goes before that end); one that does not gets `;`
/// and the entry; an empty or absent one becomes the entry alone.
pub fn with_entry(value: Option<&str>, dir: &Path) -> Option<String> {
    if has_entry(value, dir) {
        return None;
    }
    let dir = dir.to_string_lossy().into_owned();
    let value = value.unwrap_or_default();
    if value.is_empty() {
        return Some(dir);
    }
    let mut entries: Vec<&str> = value.split(';').collect();
    let ends_with_separator = entries.last().is_some_and(|last| last.is_empty());
    if ends_with_separator {
        entries.pop();
        entries.push(&dir);
        entries.push("");
    } else {
        entries.push(&dir);
    }
    Some(entries.join(";"))
}

/// The value with every entry that is `dir` taken out, each with one `;`, or `None` when none is
/// there. `Some("")` means nothing is left: the caller deletes the value.
pub fn without_entry(value: &str, dir: &Path) -> Option<String> {
    let dir = dir.to_string_lossy();
    let entries: Vec<&str> = value.split(';').collect();
    if !entries.iter().any(|e| same_entry(e, &dir)) {
        return None;
    }
    let kept: Vec<&str> = entries
        .into_iter()
        .filter(|e| !same_entry(e, &dir))
        .collect();
    Some(kept.join(";"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathChange {
    Added,
    AlreadyThere,
    Removed,
    NotThere,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// The value is there but is not plain text (registry type `kind`): it is left alone.
    #[error("the Path value of HKEY_CURRENT_USER\\Environment is not plain text (registry type {kind}), so CMD Studio did not change it")]
    NotText { kind: u32 },
    #[error("{0}")]
    Read(AdapterError),
    #[error("{0}")]
    Write(AdapterError),
}

impl PathError {
    /// Stable code (spec §5.9): a registry that cannot be read is `E_MACHINE_READ`; a value that
    /// cannot be edited or written is `E_PATH_WRITE`.
    pub fn code(&self) -> &'static str {
        match self {
            PathError::Read(_) => "E_MACHINE_READ",
            PathError::NotText { .. } | PathError::Write(_) => "E_PATH_WRITE",
        }
    }
}

/// The value's text, or the error for a value that is not text.
fn current(user_path: &dyn UserPath) -> Result<(Option<RegValue>, RegText), PathError> {
    let value = user_path.read().map_err(PathError::Read)?;
    let text = RegText::of(value.as_ref());
    if let (RegText::NotText, Some(v)) = (&text, &value) {
        return Err(PathError::NotText { kind: v.kind });
    }
    Ok((value, text))
}

/// Puts `dir` on the user's Path, keeping the value's kind (a missing value is made as
/// `REG_EXPAND_SZ`), and broadcasts the change when something was written.
pub fn add(user_path: &dyn UserPath, dir: &Path) -> Result<PathChange, PathError> {
    let (value, text) = current(user_path)?;
    let (kind, old) = match (&value, &text) {
        (Some(v), RegText::Text(t)) => (v.kind, Some(t.as_str())),
        _ => (REG_EXPAND_SZ, None),
    };
    let Some(new) = with_entry(old, dir) else {
        return Ok(PathChange::AlreadyThere);
    };
    user_path
        .write(&RegValue::from_text(kind, &new))
        .map_err(PathError::Write)?;
    user_path.broadcast_change();
    Ok(PathChange::Added)
}

/// Takes `dir` off the user's Path; a value left empty is deleted. Broadcasts when something
/// was written or deleted.
pub fn remove(user_path: &dyn UserPath, dir: &Path) -> Result<PathChange, PathError> {
    let (value, text) = current(user_path)?;
    let (Some(v), RegText::Text(old)) = (&value, &text) else {
        return Ok(PathChange::NotThere);
    };
    let Some(new) = without_entry(old, dir) else {
        return Ok(PathChange::NotThere);
    };
    if new.is_empty() {
        user_path.delete().map_err(PathError::Write)?;
    } else {
        user_path
            .write(&RegValue::from_text(v.kind, &new))
            .map_err(PathError::Write)?;
    }
    user_path.broadcast_change();
    Ok(PathChange::Removed)
}
```

`AdapterError` must implement `Display` for `#[error("{0}")]`; check `adapters/mod.rs` (it does: the CLI prints adapter errors with `{e}`). If it does not, write `#[error("{}: {}", .0.context, .0.message)]` instead.

- [ ] **Step 4: Run the pure tests**

Run: `cargo test -p cmdstudio-core --features fake --lib user_path`
Expected: the six tests pass. Then `cargo build -p cmdstudio-core` fails on the `System` literals that lack `user_path` (the CLI, `system.rs`, `machine.rs`, `library.rs`): that is Step 6.

- [ ] **Step 5: The Windows adapter**

In `crates/core/src/adapters/windows/registry.rs`, after `impl AutoRun for WinAutoRun`:

```rust
/// `HKCU\Environment`, where the user's `Path` lives.
pub const ENVIRONMENT: &str = "Environment";
pub const PATH_VALUE: &str = "Path";

/// The user's Path (spec §5.7, phase 4a): `HKCU\Environment` `Path`, read and written raw.
#[derive(Clone, Debug)]
pub struct WinUserPath {
    key: RegLocation,
}

impl WinUserPath {
    pub fn real() -> Self {
        Self::at(RegLocation::user(ENVIRONMENT))
    }
    /// At another key, for a test's throwaway key.
    pub fn at(key: RegLocation) -> Self {
        WinUserPath { key }
    }
}

impl UserPath for WinUserPath {
    fn read(&self) -> Result<Option<RegValue>, AdapterError> {
        self.key.read_raw(PATH_VALUE)
    }
    fn write(&self, value: &RegValue) -> Result<(), AdapterError> {
        self.key.write_raw(PATH_VALUE, value)
    }
    fn delete(&self) -> Result<(), AdapterError> {
        self.key.delete_value(PATH_VALUE)
    }
    fn broadcast_change(&self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        };
        let section = super::wide(std::ffi::OsStr::new(ENVIRONMENT));
        // SAFETY: `section` is a NUL-terminated UTF-16 buffer that outlives the call; the
        // message goes to every top-level window with a 5 s limit per window and
        // SMTO_ABORTIFHUNG, so a hung window cannot block us; the result pointer is null, which
        // the function allows, and the return value (0 on timeout) is deliberately ignored:
        // the broadcast is best effort.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                section.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5000,
                std::ptr::null_mut(),
            );
        }
    }
}
```

Add `UserPath` to the `use crate::adapters::{…}` line. `wide` is `pub(crate)` in `adapters/windows/mod.rs`. The `LPARAM` type of windows-sys 0.61 is `isize`; the `as isize` cast of a `*const u16` is what `fonts.rs`'s `WM_FONTCHANGE` broadcast does (read it and match its form exactly; if it passes `section.as_ptr() as LPARAM`, do the same). `Win32_UI_WindowsAndMessaging` is already in the core's windows-sys features.

- [ ] **Step 6: Every `System` and `Defaults` literal; the CLI's two commands**

`System { … }` literals that gain `user_path:` (grep `event_log: &`): `crates/cli/src/lib.rs` (`run`: `let user_path = WinUserPath::real();` with the other adapters, then `user_path: &user_path`), `crates/cli/tests/system.rs` (the same), `app/src-tauri/src/machine.rs` (`RealMachine` gains `user_path: WinUserPath` and `system()` hands `&self.user_path`), `app/src-tauri/src/library.rs` (its test's literal: the fake's `user_path`), and `fake.rs` (done in Step 3).

`Defaults` gains `pub exe_dir: PathBuf,` ("the folder of the program that runs: the install folder, which `--add-to-path` puts on the Path"). Literals: `run` in `crates/cli/src/lib.rs` (`exe_dir: exe_dir.clone()`, computed already as `exe_dir`), the test helper `machine()` there (`exe_dir: PathBuf::from(r"C:\Program Files\CMD Studio")`), and `crates/cli/tests/system.rs`'s (`exe_dir: std::env::temp_dir().join("cmdstudio-system-test-exe")`).

In `crates/cli/src/args.rs`:

```rust
    /// `--add-to-path` / `--remove-from-path` (spec §5.7): the installer's and the uninstaller's.
    AddToPath,
    RemoveFromPath,
```

in `parse`, before the `--cleanup` arm:

```rust
    } else if take_flag(&mut flags, "--add-to-path") | take_flag(&mut flags, "--remove-from-path") {
        // Both flags taken above so that giving both is "takes no other argument" below, not
        // "unknown option"; which one it was is read again here.
        let add = args.iter().any(|a| a == "--add-to-path");
        let remove = args.iter().any(|a| a == "--remove-from-path");
        if (add && remove) || !words.is_empty() || !values.is_empty() {
            return Err(bad("--add-to-path and --remove-from-path take no other argument"));
        }
        if add {
            Command::AddToPath
        } else {
            Command::RemoveFromPath
        }
```

(`a == "--add-to-path"` compares `&OsString` with `&str`: write `a.to_str() == Some("--add-to-path")`.) `--data-dir`/`--resources` are still accepted (every command takes them) and unused.

Tests in `args.rs`'s module: `p(&["--add-to-path"]).unwrap().command == Command::AddToPath`, the same for remove, and each of `["--add-to-path", "doctor"]`, `["--add-to-path", "--remove-from-path"]`, `["--remove-from-path", "--target", "wt"]`, `["--add-to-path", "--json"]` is `Err` (the last as "unknown option --json").

In `crates/cli/src/lib.rs`, `run_with`'s match gains:

```rust
        Command::AddToPath => say.path_change(sys, &defaults.exe_dir, true),
        Command::RemoveFromPath => say.path_change(sys, &defaults.exe_dir, false),
```

and `Say`:

```rust
    /// `--add-to-path` and `--remove-from-path` (spec §5.7): the install folder on or off the
    /// user's Path. Nothing to do is still done (exit 0): the installer runs it on every
    /// install, an update's included.
    fn path_change(&mut self, sys: System<'_>, dir: &Path, add: bool) -> i32 {
        use cmdstudio_core::user_path::{self, PathChange, PathError};
        let shown = dir.display();
        let result = if add {
            user_path::add(sys.user_path, dir)
        } else {
            user_path::remove(sys.user_path, dir)
        };
        match result {
            Ok(PathChange::Added) => {
                self.out(format!(
                    "Added {shown} to your Path. New cmd windows will find cmdstudio; windows \
                     that are open now will not."
                ));
                EXIT_OK
            }
            Ok(PathChange::AlreadyThere) => {
                self.out(format!("Your Path already has {shown}."));
                EXIT_OK
            }
            Ok(PathChange::Removed) => {
                self.out(format!("Removed {shown} from your Path."));
                EXIT_OK
            }
            Ok(PathChange::NotThere) => {
                self.out(format!("Your Path does not have {shown}."));
                EXIT_OK
            }
            Err(e) => {
                let text = self.coded(e.code(), &e.to_string());
                self.err(text);
                if !matches!(e, PathError::Read(_)) {
                    let verb = if add { "add" } else { "remove" };
                    self.err(format!(
                        "    fix: {verb} {shown} by hand in Settings > System > About > Advanced \
                         system settings > Environment Variables, under User variables > Path."
                    ));
                }
                EXIT_FAILED
            }
        }
    }
```

Tests in `lib.rs`'s module (over `machine()`):

```rust
    /// The installer's two commands: the install folder on and off the Path, each said once,
    /// nothing to do still exit 0, and a value that is not text named with the hand fix.
    #[test]
    fn add_to_path_and_remove_from_path_say_what_they_did() {
        let (m, d) = machine();
        let dir = d.exe_dir.display().to_string();
        let (code, out, err) = cli(&m, &d, &["--add-to-path"]);
        assert_eq!((code, err.as_str()), (EXIT_OK, ""), "{out}");
        assert!(out.starts_with(&format!("Added {dir} to your Path.")), "{out}");
        assert_eq!(m.user_path.value().unwrap().text().unwrap(), dir);
        let (code, out, _) = cli(&m, &d, &["--add-to-path"]);
        assert_eq!(code, EXIT_OK);
        assert_eq!(out.trim(), format!("Your Path already has {dir}."));
        let (code, out, _) = cli(&m, &d, &["--remove-from-path"]);
        assert_eq!(code, EXIT_OK);
        assert_eq!(out.trim(), format!("Removed {dir} from your Path."));
        assert_eq!(m.user_path.value(), None);
        let (code, out, _) = cli(&m, &d, &["--remove-from-path"]);
        assert_eq!(code, EXIT_OK);
        assert_eq!(out.trim(), format!("Your Path does not have {dir}."));
        assert_eq!(m.user_path.broadcasts(), 2);

        m.user_path.set(Some(RegValue { kind: 3, bytes: vec![0] }));
        let (code, out, err) = cli(&m, &d, &["--add-to-path"]);
        assert_eq!((code, out.as_str()), (EXIT_FAILED, ""));
        assert!(err.starts_with("E_PATH_WRITE: the Path value of HKEY_CURRENT_USER\\Environment is not plain text (registry type 3)"), "{err}");
        assert!(err.contains("fix: add "), "{err}");
        assert_eq!(thai(&err), "");
        assert_eq!(cli(&m, &d, &["--add-to-path", "doctor"]).0, EXIT_INVALID);
    }
```

- [ ] **Step 7: The gated real-registry test**

In `crates/cli/tests/system.rs`, a second test, gated like the first (`enabled()` prints SKIPPED and returns false unless `CMDSTUDIO_SYSTEM_TESTS` is exactly `1`; CI's grep for SKIPPED fails the run if it skipped there):

```rust
/// `--add-to-path` and `--remove-from-path` against this runner's real HKCU\Environment, at a
/// throwaway subkey (never the real `Environment`): the value, its kind and its bytes come back
/// as they were, including a trailing `;` and a REG_SZ kind. Runs only where the first test runs.
#[test]
fn system_user_path_add_and_remove_round_trip() {
    if !enabled() {
        return;
    }
    use cmdstudio_core::adapters::windows::registry::{RegLocation, WinUserPath, PATH_VALUE};
    use cmdstudio_core::adapters::{RegValue, UserPath, REG_EXPAND_SZ, REG_SZ};
    use cmdstudio_core::user_path::{add, remove, PathChange};
    let subkey = format!(r"Software\CMD Studio tests\{}\Environment", now_ms());
    let key = RegLocation::user(&subkey);
    let path = WinUserPath::at(key.clone());
    let dir = Path::new(r"C:\Users\tester\AppData\Local\CMD Studio");
    for before in [
        None,
        Some(RegValue::from_text(REG_EXPAND_SZ, r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;")),
        Some(RegValue::from_text(REG_SZ, r"C:\a;C:\b")),
    ] {
        match &before {
            Some(v) => path.write(v).unwrap(),
            None => path.delete().unwrap(),
        }
        assert_eq!(add(&path, dir).unwrap(), PathChange::Added);
        assert!(path.read().unwrap().unwrap().text().unwrap().contains(r"\CMD Studio"));
        assert_eq!(remove(&path, dir).unwrap(), PathChange::Removed);
        assert_eq!(path.read().unwrap(), before, "byte-equal after the round trip");
    }
    // The throwaway key goes; `delete_value` first so the key is empty.
    path.delete().unwrap();
    let _ = PATH_VALUE;
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .delete_subkey_all(r"Software\CMD Studio tests")
        .unwrap();
}
```

`winreg` is a dependency of the core, not the CLI: add `winreg = "=0.56.0"` under `[target.'cfg(windows)'.dev-dependencies]` in `crates/cli/Cargo.toml` (the same pin; `Cargo.lock` gains no new crate). Run `cargo test -p cmdstudio-cli --test system` on this PC: Expected: both tests print `SKIPPED …` (the variable is never set here).

- [ ] **Step 8: `E_PATH_WRITE` in the spec and in `error.rs`, one commit**

In the spec's §5.9 table, after the `E_MACHINE_READ` row, add:

```
| `E_PATH_WRITE` | แก้ค่า `Path` ของผู้ใช้ (`HKCU\Environment`) ไม่ได้: ค่าไม่ใช่ข้อความธรรมดา หรือเขียนไม่ได้ (`--add-to-path`, `--remove-from-path` §5.7) ให้ใส่หรือเอาโฟลเดอร์ติดตั้งออกด้วยมือใน Environment Variables | The user's Path (HKCU\Environment) could not be changed: it is not plain text, or could not be written (`--add-to-path`, `--remove-from-path` §5.7); add or remove the install folder by hand in Environment Variables |
```

In `app/src-tauri/src/error.rs`'s `MEANINGS`, a row with exactly the Thai and English of that spec row (the test `the_meanings_are_exactly_the_rows_of_spec_5_9` compares them cell for cell). Run: `cargo test -p cmdstudio-app --lib error::` and `cargo test -p cmdstudio-core --test error_codes`. Expected: pass (the code is said in `user_path.rs`'s production code, `PathError::code`).

- [ ] **Step 9: The checks, then commit**

The four clippy runs, `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo test --workspace` (this PC: the system tests print SKIPPED), `cargo test -p cmdstudio-core`. Nothing of this task touches this PC's registry: `cargo test` never sets the gate.

Commit message:

```
core, cli, app, spec: the user Path (--add-to-path, --remove-from-path, E_PATH_WRITE)

user_path.rs: the pure rules (split at `;` only, entries compared
without case and trailing `\`, one separator with our entry so a
value that ended with `;` comes back byte-equal, a long Path kept
whole) and add/remove over the UserPath adapter (kind kept, a missing
value made REG_EXPAND_SZ, an emptied one deleted, a value that is not
text left and named, WM_SETTINGCHANGE after every write). The fake and
the Windows adapter (HKCU\Environment, SendMessageTimeoutW); System
and FakeMachine carry user_path. The CLI's two commands with their
words; a gated real-registry round trip at a throwaway key. §5.9's
E_PATH_WRITE row and error.rs's, together.
```

Files: `crates/core/src/user_path.rs crates/core/src/lib.rs crates/core/src/adapters/mod.rs crates/core/src/adapters/fake.rs crates/core/src/adapters/windows/registry.rs crates/cli/Cargo.toml crates/cli/src/args.rs crates/cli/src/lib.rs crates/cli/tests/system.rs app/src-tauri/src/machine.rs app/src-tauri/src/library.rs app/src-tauri/src/error.rs docs/superpowers/specs/2026-10-10-cmd-studio-design.md Cargo.lock` (only if `Cargo.lock` changed by the dev-dependency; it must not gain a new crate).

---

### Task 3: The NSIS installer: the bundle, `hooks.nsh`, the Recovery shortcut, ARM64, the cleanup and the Path through the hooks, the stage script, the config guards, and CI's bundle

Spec §6.1 and §6.3 as the architect's edits of 2026-11-04 describe them. The installer is built by CI on every push from now on; nothing of it runs on this PC.

**Files:**
- Modify: `app/src-tauri/tauri.conf.json` (the `bundle` object, `build.beforeBundleCommand`)
- Create: `app/src-tauri/windows/hooks.nsh`
- Create: `scripts/stage-console-exe.mjs`
- Modify: `.gitignore` (`/app/src-tauri/target-stage/`)
- Modify: `app/src-tauri/src/config_guard.rs` (the bundle's guards; the hooks-text guards)
- Modify: `.github/workflows/ci.yml` (`app` job: the bundle, the installer's listing and size, the artifact `cmdstudio-installer`)

**Interfaces:**
- Consumes: `console::{APP_EXE, CONSOLE_EXE}` (Task 1), `target/release/cmdstudio-cli.exe` (Task 1), the CLI's `--cleanup [--remove-themes]`, `--add-to-path`, `--remove-from-path` (Task 2), the exit codes `0`–`3`.
- Produces: `target/release/bundle/nsis/CMD Studio_<version>_x64-setup.exe`; CI artifact `cmdstudio-installer` (the installer and `SHA256SUMS.txt`); `$INSTDIR` = `%LOCALAPPDATA%\CMD Studio` with `cmdstudio.exe`, `cmdstudio.com`, `resources\…`, `LICENSE.txt`, `uninstall.exe`; the Start-menu folder `CMD Studio` with `CMD Studio.lnk` and `CMD Studio Recovery.lnk`.

- [ ] **Step 0: The gates G-NSI, G-NSIS, G-RES**

Read the three (Implementation Gate table) and write what was found as a comment block at the top of `hooks.nsh` ("Checked on <date> against tauri-bundler <version>, NSIS <version>: …"). A mismatch stops here and goes to the founder; the fallbacks below cover the two foreseen ones.

- [ ] **Step 1: The failing config guards**

In `app/src-tauri/src/config_guard.rs`:

```rust
const HOOKS: &str = include_str!("../windows/hooks.nsh");

/// Spec §6.1, §6.3, G10: a per-user NSIS installer with the hooks file, both exes, the
/// resources with their licenses and the app's LICENSE, both shortcuts in one Start-menu folder,
/// Tauri's default WebView2 bootstrapper, and no updater artifacts until 4b.
#[test]
fn the_bundle_is_a_per_user_nsis_installer_with_the_hooks_and_the_resources() {
    let config: Value = serde_json::from_str(CONFIG).unwrap();
    let bundle = config.get("bundle").and_then(Value::as_object).unwrap();
    assert_eq!(bundle.get("active"), Some(&Value::Bool(true)));
    assert_eq!(bundle.get("targets"), Some(&serde_json::json!(["nsis"])));
    assert!(bundle.get("createUpdaterArtifacts").is_none(), "4b turns the updater artifacts on");
    let resources = bundle.get("resources").and_then(Value::as_object).unwrap();
    assert_eq!(resources.get("../../resources").and_then(Value::as_str), Some("resources"));
    assert_eq!(resources.get("../../LICENSE").and_then(Value::as_str), Some("LICENSE.txt"));
    assert_eq!(
        resources.get("target-stage/cmdstudio.com").and_then(Value::as_str),
        Some(crate::console::CONSOLE_EXE)
    );
    let nsis = bundle.pointer("/windows/nsis").and_then(Value::as_object).unwrap();
    assert_eq!(nsis.get("installMode").and_then(Value::as_str), Some("currentUser"));
    assert_eq!(nsis.get("installerHooks").and_then(Value::as_str), Some("windows/hooks.nsh"));
    assert_eq!(nsis.get("startMenuFolder").and_then(Value::as_str), Some("CMD Studio"));
    assert!(bundle.pointer("/windows/webviewInstallMode").is_none(), "Tauri's default: downloadBootstrapper (G10)");
    assert_eq!(config.get("mainBinaryName").and_then(Value::as_str), Some("cmdstudio"));
    assert_eq!(
        config.pointer("/build/beforeBundleCommand").and_then(Value::as_str),
        Some("node ../scripts/stage-console-exe.mjs")
    );
}

/// The hooks file says what spec §6.1 and §6.3 require, line by line: ARM64 refused before any
/// file; the Recovery shortcut (`--recovery` and nothing else) in the same Start-menu folder as
/// the app's; the Path added after every install; before any file is deleted and only outside
/// update mode, `--cleanup` through the console exe with `--remove-themes` on the ticked box,
/// a failed cleanup keeping the data folder, then the Path removed; the shortcut and the
/// console exe deleted. Each line is a pin: the words below are the ones the file must hold.
#[test]
fn the_hooks_refuse_arm64_make_the_recovery_shortcut_and_clean_up_only_outside_update_mode() {
    let hooks = HOOKS;
    let folder = "$SMPROGRAMS\\CMD Studio\\";
    let recovery = format!("CreateShortcut \"{folder}CMD Studio Recovery.lnk\" \"$INSTDIR\\cmdstudio.exe\" \"--recovery\"");
    let order = |a: &str, b: &str| {
        let (ia, ib) = (hooks.find(a).unwrap_or_else(|| panic!("missing: {a}")), hooks.find(b).unwrap_or_else(|| panic!("missing: {b}")));
        assert!(ia < ib, "{a:?} must come before {b:?}");
    };
    // The four hooks, each once.
    for name in ["NSIS_HOOK_PREINSTALL", "NSIS_HOOK_POSTINSTALL", "NSIS_HOOK_PREUNINSTALL", "NSIS_HOOK_POSTUNINSTALL"] {
        assert_eq!(hooks.matches(&format!("!macro {name}")).count(), 1, "{name}");
    }
    // ARM64, before any file.
    order("!macro NSIS_HOOK_PREINSTALL", "${If} ${IsNativeARM64}");
    order("${If} ${IsNativeARM64}", "Abort");
    order("Abort", "!macro NSIS_HOOK_POSTINSTALL");
    // The shortcut and the Path after the files.
    order("!macro NSIS_HOOK_POSTINSTALL", &recovery);
    order(&recovery, "nsExec::ExecToStack '\"$INSTDIR\\cmdstudio.com\" --add-to-path'");
    // Only outside update mode: the cleanup, then the Path; the box's flag passed and un-ticked on a failure.
    order("!macro NSIS_HOOK_PREUNINSTALL", "${If} $UpdateMode <> 1");
    order("${If} $UpdateMode <> 1", "${If} $DeleteAppDataCheckboxState = 1");
    order("${If} $DeleteAppDataCheckboxState = 1", "nsExec::ExecToStack '\"$INSTDIR\\cmdstudio.com\" --cleanup --remove-themes'");
    order("--cleanup --remove-themes'", "nsExec::ExecToStack '\"$INSTDIR\\cmdstudio.com\" --cleanup'");
    order("--cleanup'", "StrCpy $DeleteAppDataCheckboxState 0");
    order("StrCpy $DeleteAppDataCheckboxState 0", "nsExec::ExecToStack '\"$INSTDIR\\cmdstudio.com\" --remove-from-path'");
    order("--remove-from-path'", &format!("Delete \"{folder}CMD Studio Recovery.lnk\""));
    order("--remove-from-path'", "Delete \"$INSTDIR\\cmdstudio.com\"");
    assert_eq!(hooks.matches("$UpdateMode").count(), 2, "the guard, and its comment");
    // No Thai, no user name, no other exe called.
    assert!(!hooks.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)));
    assert!(!hooks.contains("cmdstudio.exe\" --cleanup"), "the cleanup goes through the console exe");
}

/// The Start-menu folder of the config and of the hooks file are the same words.
#[test]
fn the_start_menu_folder_is_one_name() {
    let config: Value = serde_json::from_str(CONFIG).unwrap();
    let folder = config.pointer("/bundle/windows/nsis/startMenuFolder").and_then(Value::as_str).unwrap();
    assert!(HOOKS.contains(&format!("$SMPROGRAMS\\{folder}\\CMD Studio Recovery.lnk")));
}
```

Run: `cargo test -p cmdstudio-app --lib config_guard`. Expected: compile error (`hooks.nsh` missing) — then after an empty file, the three fail.

- [ ] **Step 2: `tauri.conf.json`**

Replace the `build` and `bundle` objects:

```json
  "build": {
    "frontendDist": "../ui/dist",
    "beforeBuildCommand": "npm run build",
    "beforeBundleCommand": "node ../scripts/stage-console-exe.mjs"
  },
```

```json
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/icon.ico"],
    "publisher": "CMD Studio contributors",
    "resources": {
      "../../resources": "resources",
      "../../LICENSE": "LICENSE.txt",
      "target-stage/cmdstudio.com": "cmdstudio.com"
    },
    "windows": {
      "nsis": {
        "installMode": "currentUser",
        "installerHooks": "windows/hooks.nsh",
        "startMenuFolder": "CMD Studio"
      }
    }
  }
```

`beforeBundleCommand` runs in the app directory (`app/`, where `package.json` is; G-RES confirms the cwd); the script finds the repo from its own location either way.

- [ ] **Step 3: The stage script**

`scripts/stage-console-exe.mjs`:

```js
// Before `tauri build` bundles (build.beforeBundleCommand): copies the console exe cargo built,
// target/release/cmdstudio-cli.exe, to app/src-tauri/target-stage/cmdstudio.com, which
// tauri.conf.json lists as a resource under that name (spec §5.7, phase 4a). The cargo target
// folder is CARGO_TARGET_DIR when set, else <repo>/target. Node only: no package is needed.
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const target = process.env.CARGO_TARGET_DIR ? resolve(process.env.CARGO_TARGET_DIR) : join(repo, "target");
const from = join(target, "release", "cmdstudio-cli.exe");
const stage = join(repo, "app", "src-tauri", "target-stage");
const to = join(stage, "cmdstudio.com");
if (!existsSync(from)) {
  console.error(`stage-console-exe: ${from} is not there; the release build of the app crate makes it`);
  process.exit(1);
}
mkdirSync(stage, { recursive: true });
copyFileSync(from, to);
console.log(`stage-console-exe: ${to}`);
```

`.gitignore` gains, after the `/app/src-tauri/target/` line:

```
# The console exe staged under its bundled name before `tauri build` bundles (scripts/stage-console-exe.mjs).
/app/src-tauri/target-stage/
```

- [ ] **Step 4: `hooks.nsh`**

```nsis
; CMD Studio's installer hooks (spec §6.1, §6.3, phase 4a). Tauri's installer.nsi inserts each
; macro at its point: PREINSTALL before any file is written, POSTINSTALL after the files and
; Tauri's own shortcuts, PREUNINSTALL before any file is deleted (in update mode too: the
; installer runs the old uninstaller with /UPDATE, and $UpdateMode is 1 then, G10),
; POSTUNINSTALL after. Everything typed here is English: it is what a person sees in the
; installer's boxes. The program calls go through the console exe cmdstudio.com with nsExec, so
; no console window flashes and the exit code and the output come back on the stack
; (ExecToStack pushes the exit code, then the output; "error" when the program could not start).
; Rust edits the user's Path (--add-to-path, --remove-from-path), never this script: NSIS strings
; hold 1024 characters and a longer Path would be cut.
;
; Checked on <date> against tauri-bundler <version> (installer.nsi: the four hook names,
; Var UpdateMode, Var DeleteAppDataCheckboxState, LogicLib) and its NSIS <version>
; (Plugins\x86-unicode\nsExec.dll, Include\x64.nsh with IsNativeARM64).
!include "x64.nsh"

!macro NSIS_HOOK_PREINSTALL
  ; x64 only (spec §1, §6.1): Clink x64 cannot inject into an ARM64 cmd. Before any file.
  ${If} ${IsNativeARM64}
    MessageBox MB_OK|MB_ICONSTOP "CMD Studio supports x64 Windows only. This computer runs Windows on ARM64, and the add-ons CMD Studio bundles (Clink) cannot run in an ARM64 cmd window. Nothing was installed." /SD IDOK
    SetErrorLevel 3
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; The Recovery shortcut beside the app's, in the same Start-menu folder (spec §6.1, recovery
  ; spec §4.4): cmdstudio.exe with --recovery and nothing else.
  CreateDirectory "$SMPROGRAMS\CMD Studio"
  CreateShortcut "$SMPROGRAMS\CMD Studio\CMD Studio Recovery.lnk" "$INSTDIR\cmdstudio.exe" "--recovery" "$INSTDIR\cmdstudio.exe" 0 SW_SHOWNORMAL "" "Opens CMD Studio in recovery mode: puts back a cut-off change and pauses the theme"
  ; The install folder on the user's Path, so that cmd finds `cmdstudio` (spec §5.7). On an
  ; update it is there already and the command changes nothing.
  nsExec::ExecToStack '"$INSTDIR\cmdstudio.com" --add-to-path'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "CMD Studio is installed, but its folder could not be added to your Path (exit code $0). What it said:$\r$\n$1$\r$\nTo type cmdstudio in cmd, add $INSTDIR to your Path by hand (Settings > System > About > Advanced system settings > Environment Variables)." /SD IDOK
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Before any file is deleted. The cleanup and the Path only on a real uninstall: this hook
  ; runs on an update too ($UpdateMode = 1), and a cleanup then would undo the user's theme (G10).
  ${If} $UpdateMode <> 1
    ; spec §6.3: --cleanup puts the machine back; --remove-themes when the "delete app data"
    ; box is ticked (then NSIS deletes the data folder itself).
    ${If} $DeleteAppDataCheckboxState = 1
      nsExec::ExecToStack '"$INSTDIR\cmdstudio.com" --cleanup --remove-themes'
    ${Else}
      nsExec::ExecToStack '"$INSTDIR\cmdstudio.com" --cleanup'
    ${EndIf}
    Pop $0
    Pop $1
    ${If} $0 != 0
      ; Not everything is back (exit code 3: run it again later; 1 or 2: it was refused). The
      ; data folder stays whatever the box said: its restore points may be the only copy of the
      ; user's original settings (spec §6.3).
      StrCpy $DeleteAppDataCheckboxState 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "CMD Studio could not put everything back (cleanup exit code $0). What it said:$\r$\n$1$\r$\nThe uninstall goes on, and your CMD Studio data folder is kept so that this can be fixed: open a new cmd window once every cmd window and Windows Terminal are closed and run `cmdstudio --cleanup` again, or fix by hand what it lists; then delete %LOCALAPPDATA%\ai.resincore.cmdstudio yourself." /SD IDOK
    ${EndIf}
    nsExec::ExecToStack '"$INSTDIR\cmdstudio.com" --remove-from-path'
    Pop $0
    Pop $1
    ${If} $0 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "CMD Studio's folder could not be taken off your Path (exit code $0). What it said:$\r$\n$1$\r$\nRemove $INSTDIR from your Path by hand (Settings > System > About > Advanced system settings > Environment Variables)." /SD IDOK
    ${EndIf}
  ${EndIf}
  ; Ours, which Tauri's own list may not hold: the Recovery shortcut and the console exe. On an
  ; update the new installer writes both again (POSTINSTALL).
  Delete "$SMPROGRAMS\CMD Studio\CMD Studio Recovery.lnk"
  Delete "$INSTDIR\cmdstudio.com"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Nothing: Tauri removes its shortcut, the Start-menu folder when empty, the files and
  ; $INSTDIR, and the data folder when the box was ticked (and the cleanup succeeded).
!macroend
```

`cmdstudio --cleanup` in the message is what `--help` calls it; the `.com` is what that resolves to once a new cmd window has the Path (the uninstall has just removed it, so the text names the full command the README gives for that case: `"%LOCALAPPDATA%\CMD Studio\cmdstudio.com" --cleanup` is gone with the folder; a leftover data folder is cleaned by the next install's app, or by hand as the box says). Keep the message as written; Task 7's README has the hand steps.

**Fallbacks.** F-A (G-RES: a missing `target-stage/cmdstudio.com` fails `cargo build` through tauri-build): drop the `target-stage/cmdstudio.com` entry from `resources`, and in `NSIS_HOOK_POSTINSTALL`, first line, `File "/oname=$INSTDIR\cmdstudio.com" "${__FILEDIR__}\..\target-stage\cmdstudio.com"`; the config test's resource assertion becomes a hooks-text assertion on that `File` line. F-B (G-NSIS: no `nsExec`): `ExecWait '"$INSTDIR\cmdstudio.exe" --cleanup' $0` (and the other two calls alike), `$1` set to `"(output not captured: ExecWait)"`, the hooks-text test's `nsExec::ExecToStack '"$INSTDIR\cmdstudio.com"` strings replaced by `ExecWait '"$INSTDIR\cmdstudio.exe"` and the "cleanup goes through the console exe" assertion dropped; record it in Task 8's appendix B row. F-C (G-NSIS: no `IsNativeARM64`): `ReadEnvStr $0 PROCESSOR_ARCHITEW6432` then `ReadEnvStr $1 PROCESSOR_ARCHITECTURE`, `${If} $0 == "ARM64"` `${OrIf} $1 == "ARM64"` in place of `${If} ${IsNativeARM64}`, the test's string changed alike.

- [ ] **Step 5: Run the guards**

Run: `cargo test -p cmdstudio-app --lib config_guard`
Expected: the three new tests pass with the two old ones (the CSP and the window are unchanged).

- [ ] **Step 6: CI builds the bundle**

In `.github/workflows/ci.yml`, `app` job: the step "Tauri build (no bundle)" becomes "Tauri build (with the NSIS bundle)", its command `npm run tauri -- build` (no `--no-bundle`), its comment's last sentence "No bundle: Phase 4 turns it on." replaced by "The bundle is on (phase 4a): the NSIS installer lands in target/release/bundle/nsis/, after `beforeBundleCommand` has staged the console exe as cmdstudio.com." Then after "The UI the exe embeds holds no mock data and no preset file", add:

```yaml
      # The installer (spec §6.1): one file, named with the one version (Task 5 pins the sources;
      # the name proves the Tauri CLI read Cargo.toml's). 7-Zip lists NSIS installers, so the
      # files inside are checked here without running it: both exes, the runtimes' manifest, the
      # three licenses and Clink's source notice, the app's LICENSE. Its size is printed for the
      # spec's §6.1 (20–30 MB estimated). SHA256SUMS.txt goes with it.
      - name: The installer holds both exes, the resources and the licenses
        shell: bash
        run: |
          version="$(grep -m1 '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')"
          installer="target/release/bundle/nsis/CMD Studio_${version}_x64-setup.exe"
          test -f "$installer"
          ls -l "$installer"
          7z l "$installer" > installer-files.txt
          for f in cmdstudio.exe cmdstudio.com 'resources\manifest.json' 'resources\omp\oh-my-posh.exe' 'resources\clink\clink_x64.exe' 'resources\clink\clink_dll_x64.dll' 'resources\fonts\CaskaydiaCoveNerdFont-Regular.ttf' 'resources\LICENSES\oh-my-posh-MIT.txt' 'resources\LICENSES\clink-GPL-3.0.txt' 'resources\LICENSES\clink-SOURCE.txt' 'resources\LICENSES\CaskaydiaCove-NF-OFL-1.1.txt' LICENSE.txt; do
            grep -q -F -- "$f" installer-files.txt || { echo "::error::$f is not in the installer"; exit 1; }
          done
          mkdir -p stage/installer
          cp "$installer" stage/installer/
          (cd stage/installer && sha256sum -- *.exe > SHA256SUMS.txt && cat SHA256SUMS.txt)
      # For the VM check of phase 4a (install, uninstall; docs/spikes/phase4-install/protocol.md).
      - uses: actions/upload-artifact@v4
        with:
          name: cmdstudio-installer
          path: stage/installer
          if-no-files-found: error
```

`grep -m1 '^version = ' Cargo.toml` finds `[workspace.package]`'s line (the only `version =` at the start of a line in the root manifest). The existing `cmdstudio-exe` artifact stays (3b's VM scripts use it).

- [ ] **Step 7: Local checks, then commit**

The four clippy runs, fmt, `cargo test -p cmdstudio-app --lib`. Do not build the bundle on this PC without the founder's yes (the NSIS download). `node scripts/stage-console-exe.mjs` may be run on this PC only after a `cargo build --release -p cmdstudio-app` in the scratch worktree with its own `CARGO_TARGET_DIR` — it copies a file; it runs nothing. Parse `ci.yml`.

Commit message:

```
app, ci: the NSIS installer with its hooks (spec §6.1, §6.3, phase 4a)

tauri.conf.json: the bundle on, nsis per user, windows/hooks.nsh, the
Start-menu folder "CMD Studio", resources (the runtimes and licenses,
the app's LICENSE as LICENSE.txt, the console exe staged as
cmdstudio.com by scripts/stage-console-exe.mjs before bundling).
hooks.nsh: ARM64 refused before any file; the Recovery shortcut
(--recovery, nothing else) beside the app's and --add-to-path after
the files; before any file is deleted and only outside update mode,
cmdstudio.com --cleanup [--remove-themes] through nsExec (no window,
output captured), a failed cleanup keeping the data folder, then
--remove-from-path; the shortcut and the .com deleted. config_guard
pins the config and the hooks' lines. CI builds the installer on every
push, lists what is inside with 7z, prints its size and uploads it
with SHA256SUMS.txt as cmdstudio-installer.
```

Files: `app/src-tauri/tauri.conf.json app/src-tauri/windows/hooks.nsh scripts/stage-console-exe.mjs .gitignore app/src-tauri/src/config_guard.rs .github/workflows/ci.yml`.

- [ ] **Step 8: CI is the gate**

After the push (the founder's or the controller's), read the `app` job's log: the installer's name and size, the listing's twelve files, both artifacts. Write the size into Task 8's note. A failure here goes back to this task.

---

### Task 4: The licenses in the installer named on the credits page, `docs/credits.md`'s pin, and M7's sentence

Spec §6.6 against the credits page: everything is there already (`credits.ts`, `Help.test.tsx` "names the versions the app bundles…" checks `docs/credits.md`'s lines) except where the license texts are; and the held sentence of 3b-2's M7.

**Files:**
- Modify: `app/ui/src/i18n/en.ts`, `app/ui/src/i18n/th.ts` (`help.credits.files`; `help.guide.cmdWontOpen`)
- Modify: `app/ui/src/i18n/i18n.test.ts` (M7's test)
- Modify: `app/ui/src/screens/Help.tsx` (the sentence under the credits' intro)
- Modify: `app/ui/src/screens/Help.test.tsx`
- Modify: `docs/credits.md` (the font's version)

**Interfaces:**
- Consumes: the installer's layout of Task 3 (`resources\LICENSES\`, `LICENSE.txt`).
- Produces: the keys `help.credits.files`; `help.guide.cmdWontOpen` with the Start-menu sentence.

- [ ] **Step 1: The failing tests**

In `app/ui/src/i18n/i18n.test.ts`, replace the M7 test with:

```ts
  it("the guide says the Start-menu shortcut first, now that the installer makes it (M7, phase 4a)", () => {
    expect(en["help.guide.cmdWontOpen"]).toBe(
      "If cmd won't open: open the Start menu and choose CMD Studio Recovery, or open CMD Studio. When cmd keeps failing to start because of the theme, CMD Studio pauses the theme by itself, and you can also press Pause the theme on the home page.",
    );
    expect(th["help.guide.cmdWontOpen"]).toMatch(/^ถ้า cmd เปิดไม่ขึ้น: เปิดเมนู Start แล้วเลือก CMD Studio Recovery หรือเปิด CMD Studio /);
  });
```

In `app/ui/src/screens/Help.test.tsx`, in the test "lists the bundled programs, the font and the color schemes with their licenses, and no SignPath line yet (Q6)", after the SignPath assertion:

```ts
    expect(screen.getByText(en["help.credits.files"])).toBeTruthy();
    expect(en["help.credits.files"]).toContain("resources\\LICENSES");
    expect(en["help.credits.files"]).toContain("LICENSE.txt");
```

Run: `npm test -- Help i18n` (in `app/`). Expected: the three assertions fail (the key is missing; the guide's old text).

- [ ] **Step 2: The words**

`en.ts`:

```ts
  "help.guide.cmdWontOpen": "If cmd won't open: open the Start menu and choose CMD Studio Recovery, or open CMD Studio. When cmd keeps failing to start because of the theme, CMD Studio pauses the theme by itself, and you can also press Pause the theme on the home page.",
  "help.credits.files": "The license texts are installed with CMD Studio: resources\\LICENSES beside cmdstudio.exe holds Clink's, Oh My Posh's and the font's, with a note of where Clink's source for this exact release is, and LICENSE.txt is CMD Studio's own.",
```

`th.ts`:

```ts
  "help.guide.cmdWontOpen": 'ถ้า cmd เปิดไม่ขึ้น: เปิดเมนู Start แล้วเลือก CMD Studio Recovery หรือเปิด CMD Studio เมื่อ cmd เปิดไม่ขึ้นซ้ำๆ เพราะธีม CMD Studio จะหยุดธีมให้เอง และกด "หยุดธีม" ที่หน้าแรกเองก็ได้',
  "help.credits.files": "ข้อความสัญญาอนุญาตติดตั้งมาพร้อม CMD Studio: โฟลเดอร์ resources\\LICENSES ข้าง cmdstudio.exe มีของ Clink, Oh My Posh และฟอนต์ พร้อมบันทึกว่าซอร์สของ Clink รุ่นนี้อยู่ที่ไหน ส่วน LICENSE.txt เป็นของ CMD Studio เอง",
```

In `Help.tsx`'s `Credits`, after `<p>{t("help.credits.intro")}</p>`: `<p>{t("help.credits.files")}</p>`.

In `docs/credits.md`, the font row's version cell: `3.5.1 (Nerd Fonts release; bundled unmodified, the G12 probe used the same)`.

- [ ] **Step 3: Run the UI's checks**

In `app/`: `npm run typecheck`, `npm test`, `TZ=UTC npm test`, `npm run build`, `bash scripts/check-dist.sh` (from the repo root). Expected: all pass; the i18n test's placeholder and language rules hold (`th.ts` has no English-only text: "CMD Studio Recovery", "Clink", "Oh My Posh", "LICENSE.txt" are names, which the test allows as the existing credits keys show).

- [ ] **Step 4: Commit**

```
ui, docs: where the license texts are, and the guide's Start-menu sentence (M7, phase 4a)

help.credits.files names resources\LICENSES and LICENSE.txt in the
install folder (spec §6.6); help.guide.cmdWontOpen says the Start
menu's CMD Studio Recovery first, now that the installer makes the
shortcut (3b-2's M7). docs/credits.md pins the font at 3.5.1.
```

Files: `app/ui/src/i18n/en.ts app/ui/src/i18n/th.ts app/ui/src/i18n/i18n.test.ts app/ui/src/screens/Help.tsx app/ui/src/screens/Help.test.tsx docs/credits.md`.

---

### Task 5: One version source

`Cargo.toml`'s `[workspace.package] version` is the one; everything else is pinned to it by a test, and the release workflow (Task 6) refuses a tag that differs.

**Files:**
- Modify: `app/src-tauri/src/config_guard.rs`

- [ ] **Step 1: The failing test**

```rust
/// Spec §6.4, phase 4a: the version is `[workspace.package] version` in the root Cargo.toml and
/// nowhere else. tauri.conf.json carries none (Tauri reads Cargo's: the installer's file name
/// and the updater's own version come from it); the UI's package.json and its lockfile say the
/// same string; every crate inherits it.
#[test]
fn the_version_has_one_source() {
    let version = env!("CARGO_PKG_VERSION");
    let config: Value = serde_json::from_str(CONFIG).unwrap();
    assert!(config.get("version").is_none(), "tauri.conf.json must not carry a version");
    let package: Value = serde_json::from_str(include_str!("../../package.json")).unwrap();
    assert_eq!(package.get("version").and_then(Value::as_str), Some(version), "app/package.json");
    let lock: Value = serde_json::from_str(include_str!("../../package-lock.json")).unwrap();
    assert_eq!(lock.get("version").and_then(Value::as_str), Some(version), "app/package-lock.json");
    assert_eq!(lock.pointer("/packages//version").and_then(Value::as_str), Some(version), "the lockfile's root package");
    let root = include_str!("../../../Cargo.toml");
    assert!(root.contains(&format!("\nversion = \"{version}\"\n")), "the workspace's version");
    for (name, toml) in [
        ("app", include_str!("../Cargo.toml")),
        ("core", include_str!("../../../crates/core/Cargo.toml")),
        ("cli", include_str!("../../../crates/cli/Cargo.toml")),
    ] {
        assert!(toml.contains("version.workspace = true"), "{name} inherits the version");
    }
}
```

Run: `cargo test -p cmdstudio-app --lib config_guard::the_version_has_one_source`. Expected: passes already if the three files agree at `0.1.0` (they do at `8255e96`); if it fails, the mismatched file is wrong, not the test. Either way the test is the deliverable: it is what fails the next time a version is bumped in one place only.

- [ ] **Step 2: G-VER and the fallback**

CI's listing step of Task 3 proves G-VER: the installer is named `CMD Studio_0.1.0_x64-setup.exe`. If the name carries another version (the Tauri CLI did not resolve the workspace's), F-V: in `ci.yml` and `release.yml` the build step becomes `npm run tauri -- build --config "{\"version\":\"$version\"}"` with `version` read from `Cargo.toml` as the listing step reads it, and this test keeps refusing a `version` key in the committed file. Record in Task 8.

- [ ] **Step 3: Commit**

```
app: one version source, pinned (spec §6.4, phase 4a)

config_guard: tauri.conf.json carries no version (Tauri reads
Cargo.toml's), app/package.json and its lockfile say the workspace's
string, every crate inherits it.
```

File: `app/src-tauri/src/config_guard.rs`.

---

### Task 6: The release workflow, up to a draft pre-release

Spec §6.4 as edited: on a tag `v*` (the founder's push) the same gates as CI, the bundle, the tag checked against the version, a draft pre-release with the installer and `SHA256SUMS.txt`; `workflow_dispatch` for a dry run. A commented SignPath step (§6.5) and the winget note.

**Files:**
- Create: `.github/workflows/release.yml`
- Create: `.github/release-notes.md`

**Interfaces:**
- Consumes: Task 3's bundle and listing, Task 5's version.
- Produces: the draft release `v<version>` with assets `CMD Studio_<version>_x64-setup.exe` and `SHA256SUMS.txt`; the artifact `cmdstudio-installer` on every run.

- [ ] **Step 1: The notes**

`.github/release-notes.md` (the workflow substitutes `@VERSION@` and `@SHA256@`):

```markdown
CMD Studio @VERSION@ — a pre-release for testers.

**This build is not code-signed.** Windows SmartScreen will warn when you run the installer: choose "More info", then "Run anyway". CMD Studio will be signed through SignPath Foundation once the project qualifies; even then SmartScreen's trust builds up over time (see the README).

**What the installer does:** installs CMD Studio for your user only (no administrator rights) into `%LOCALAPPDATA%\CMD Studio`, with Oh My Posh, Clink and the CaskaydiaCove Nerd Font inside; adds that folder to your Path so `cmdstudio` works in cmd; puts "CMD Studio" and "CMD Studio Recovery" in the Start menu; starts the app. x64 Windows 10 (version 2004 and later) and Windows 11 only; ARM64 is refused.

**Uninstalling** puts everything back (AutoRun, Windows Terminal's settings, the font, your Path) and keeps your themes unless you tick the box to delete the app's data.

**Bundled:** Clink 1.9.34 (GPL-3.0; source: https://github.com/chrisant996/clink/tree/v1.9.34), Oh My Posh 31.5.0 (MIT), CaskaydiaCove Nerd Font from Nerd Fonts 3.5.1 (SIL OFL 1.1). CMD Studio itself is MIT. The credits page in the app lists the color schemes.

SHA-256 of the installer: `@SHA256@`
```

The three versions are pinned words; Task 7's README and `credits.ts` hold the same (4b's upstream-watch PR changes all three places together: its body says so).

- [ ] **Step 2: The workflow**

`.github/workflows/release.yml`:

```yaml
# The release (spec §6.4, phase 4a): on a pushed tag v<version> (the founder's push; nothing here
# tags), the same gates as ci.yml, the NSIS installer, the tag checked against Cargo.toml's
# version, then a DRAFT pre-release with the installer and SHA256SUMS.txt. Publishing is the
# founder's click (a public post). workflow_dispatch is a dry run: the same build, uploaded as
# an artifact, and no release.
name: release
on:
  push:
    tags: ['v*']
  workflow_dispatch:
permissions:
  contents: read
concurrency:
  group: release-${{ github.ref }}
  cancel-in-progress: false
jobs:
  build:
    runs-on: windows-latest
    timeout-minutes: 90
    env:
      CARGO_TERM_COLOR: always
    outputs:
      version: ${{ steps.version.outputs.version }}
    steps:
      - uses: actions/checkout@v4
      - name: Install the toolchain pinned in rust-toolchain.toml
        shell: bash
        run: |
          rustup --version
          rustup toolchain install
          cargo --version
      - name: Install the Node pinned in app/.nvmrc
        uses: actions/setup-node@v4
        with:
          node-version-file: app/.nvmrc
          cache: npm
          cache-dependency-path: app/package-lock.json
      - name: Install the UI's packages exactly as locked
        working-directory: app
        run: npm ci
      # The one version (Task 5): Cargo.toml's. A tag that is not v<version> stops here, before
      # anything is built: the release page must never say one number and the installer another.
      - name: The version, and the tag that must match it
        id: version
        shell: bash
        run: |
          version="$(grep -m1 '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')"
          echo "version=$version" >> "$GITHUB_OUTPUT"
          echo "Cargo.toml version: $version"
          if [[ "$GITHUB_REF" == refs/tags/* ]]; then
            test "$GITHUB_REF_NAME" = "v$version" || { echo "::error::tag $GITHUB_REF_NAME is not v$version"; exit 1; }
          fi
      - name: Format
        run: cargo fmt --all -- --check
      - name: Clippy of the workspace
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: Clippy of the core alone
        run: cargo clippy -p cmdstudio-core --all-targets -- -D warnings
      - name: Clippy of the core alone, with --features fake
        run: cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings
      - name: Clippy of the core alone, with --features ts
        run: cargo clippy -p cmdstudio-core --all-targets --features ts -- -D warnings
      - name: Cache the pinned runtimes and font
        uses: actions/cache@v4
        with:
          path: resources
          key: resources-${{ runner.os }}-${{ hashFiles('scripts/fetch-resources.ps1') }}
      - name: Fetch the pinned runtimes and font
        shell: powershell
        run: ./scripts/fetch-resources.ps1
      - name: Tests
        run: cargo test --workspace
      - name: Tests of the core alone (no fake adapters)
        run: cargo test -p cmdstudio-core
      - name: Type-check the UI
        working-directory: app
        run: npm run typecheck
      - name: UI tests
        working-directory: app
        run: npm test
      - name: UI tests under TZ=UTC
        working-directory: app
        shell: bash
        run: TZ=UTC npm test
      - name: Build the UI
        working-directory: app
        run: npm run build
      - name: The built UI holds no mock data and no preset file
        shell: bash
        run: bash ./scripts/check-dist.sh
      - name: Bindings are fresh
        shell: bash
        run: |
          pwsh -NoProfile -File ./scripts/export-bindings.ps1
          stale="$(git status --porcelain -- app/ui/src/bindings)"
          if [ -n "$stale" ]; then echo "$stale"; echo "::error::app/ui/src/bindings is stale"; exit 1; fi
      - name: Tauri build (with the NSIS bundle)
        shell: bash
        working-directory: app
        run: npm run tauri -- build
      - name: The UI the exe embeds holds no mock data and no preset file
        shell: bash
        run: bash ./scripts/check-dist.sh
      - name: The console exe answers in a console; the app exe is a windows program
        shell: bash
        run: |
          subsystem() {
            python3 -c "import struct,sys; d=open(sys.argv[1],'rb').read(); off=struct.unpack_from('<I',d,0x3C)[0]; print(struct.unpack_from('<H',d,off+0x5C)[0])" "$1"
          }
          test "$(subsystem target/release/cmdstudio.exe)" = 2
          test "$(subsystem target/release/cmdstudio-cli.exe)" = 3
          ./target/release/cmdstudio-cli.exe --help | head -1 | grep -q '^Usage:'
      - name: The installer holds both exes, the resources and the licenses
        shell: bash
        run: |
          version="${{ steps.version.outputs.version }}"
          installer="target/release/bundle/nsis/CMD Studio_${version}_x64-setup.exe"
          test -f "$installer"
          ls -l "$installer"
          7z l "$installer" > installer-files.txt
          for f in cmdstudio.exe cmdstudio.com 'resources\manifest.json' 'resources\omp\oh-my-posh.exe' 'resources\clink\clink_x64.exe' 'resources\clink\clink_dll_x64.dll' 'resources\fonts\CaskaydiaCoveNerdFont-Regular.ttf' 'resources\LICENSES\oh-my-posh-MIT.txt' 'resources\LICENSES\clink-GPL-3.0.txt' 'resources\LICENSES\clink-SOURCE.txt' 'resources\LICENSES\CaskaydiaCove-NF-OFL-1.1.txt' LICENSE.txt; do
            grep -q -F -- "$f" installer-files.txt || { echo "::error::$f is not in the installer"; exit 1; }
          done
          mkdir -p stage/installer
          cp "$installer" stage/installer/
          (cd stage/installer && sha256sum -- *.exe > SHA256SUMS.txt && cat SHA256SUMS.txt)
      - uses: actions/upload-artifact@v4
        with:
          name: cmdstudio-installer
          path: stage/installer
          if-no-files-found: error
  # The draft, on a tag only. GitHub shows a draft to collaborators alone; the founder reads it,
  # downloads the installer once more if wanted, and presses Publish (spec §6.4: a public post).
  draft:
    needs: build
    if: startsWith(github.ref, 'refs/tags/v')
    runs-on: ubuntu-latest
    timeout-minutes: 15
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          name: cmdstudio-installer
          path: installer
      - name: The notes, with the version and the hash filled in
        shell: bash
        run: |
          version="${{ needs.build.outputs.version }}"
          sha="$(cut -d' ' -f1 installer/SHA256SUMS.txt | head -1)"
          sed -e "s/@VERSION@/v$version/g" -e "s/@SHA256@/$sha/g" .github/release-notes.md > notes.md
          cat notes.md
      - name: Create the draft pre-release
        env:
          GH_TOKEN: ${{ github.token }}
        shell: bash
        run: |
          version="${{ needs.build.outputs.version }}"
          gh release create "v$version" --draft --prerelease --title "CMD Studio v$version" --notes-file notes.md installer/*.exe installer/SHA256SUMS.txt
      # SignPath Foundation (spec §6.5): applied for after the first release exists and the
      # project has "reputation" (G15); every release then needs the founder's approval on
      # SignPath's side. Until the application is accepted this step stays commented out. When it
      # is: the organization id, project slug and signing policy slug come from SignPath, the
      # API token is the secret SIGNPATH_API_TOKEN (the founder's; never read here), the signed
      # installer replaces the unsigned asset, and the credits page shows "Free code signing
      # provided by SignPath.io, certificate by SignPath Foundation" (docs/credits.md).
      # - name: Sign the installer through SignPath
      #   uses: signpath/github-action-submit-signing-request@v1
      #   with:
      #     api-token: ${{ secrets.SIGNPATH_API_TOKEN }}
      #     organization-id: <from SignPath>
      #     project-slug: cmd-studio
      #     signing-policy-slug: release-signing
      #     github-artifact-id: ${{ steps.upload.outputs.artifact-id }}
      #     wait-for-completion: true
      #     output-artifact-directory: signed
      # winget (spec §6.4): a pull request to microsoft/winget-pkgs is a public post and a
      # founder step, after a signed release: `wingetcreate new <installer url>` or the
      # winget-releaser action, with the manifest's publisher and identifier chosen by the
      # founder. Not a job here.
```

- [ ] **Step 3: Parse and review**

Parse both YAML files (`python3 -c "import yaml,sys; yaml.safe_load(open(sys.argv[1]))" .github/workflows/release.yml`), then read the file against G-GH. A `workflow_dispatch` run is the controller's to start after the push (it is not a public post; it uploads an artifact only) and proves the `build` job; the `draft` job runs first on the founder's tag.

- [ ] **Step 4: Commit**

```
ci: the release workflow to a draft pre-release (spec §6.4, §6.5, phase 4a)

release.yml: on a pushed tag v*, the gates of ci.yml, the installer,
the tag checked against Cargo.toml's version, SHA256SUMS.txt, then a
draft pre-release the founder publishes; workflow_dispatch is a dry
run that uploads the artifact only. The SignPath step is present and
commented out until the application is accepted; winget is noted as
a founder step. release-notes.md: the unsigned/SmartScreen text, what
the installer does, the bundled components and the hash.
```

Files: `.github/workflows/release.yml .github/release-notes.md`.

---

### Task 7: The English README

The founder's 2026-10-11 request and the controller's draft's content list, as the spec agrees. No Thai section (a nice-to-have the founder writes).

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Write it**

Replace `README.md` with the sections below, each as written here (the words that are pinned elsewhere — the versions, the folder names, the commands — copied from Tasks 1–6):

```markdown
# CMD Studio

Theme the Windows command prompt (cmd) without typing a command: pick one of twelve built-in themes or design your own, preview it live, apply it, and undo it at any time. Made for people who have never opened cmd, with a CLI for those who have.

Status: pre-release (unsigned builds for testers). The design is in `docs/superpowers/specs/`.

## What you need

- Windows 11, or Windows 10 version 2004 (build 19041) and later. x64 only; the installer refuses ARM64.
- Windows Terminal for the colors (Windows 11 has it; CMD Studio offers the Store page on Windows 10). Without it you still get the prompt and the welcome box.
- No administrator rights. No Node, no npm, nothing to download at first start: Oh My Posh, Clink and the font come inside the installer.

## Install

1. Download `CMD Studio_<version>_x64-setup.exe` from [Releases](https://github.com/luraselenehalo/cmd-studio/releases).
2. Run it. **Windows SmartScreen will warn** ("Windows protected your PC"): choose *More info*, then *Run anyway*. The build is not code-signed yet; signing through SignPath Foundation is planned once the project qualifies, and even a signed program earns SmartScreen's trust only over time (since 2024 an EV certificate no longer skips the warning either). Check the SHA-256 in `SHA256SUMS.txt` on the release page if you want to be sure of the file.
3. The installer puts CMD Studio in `%LOCALAPPDATA%\CMD Studio` for your user only, adds that folder to your Path, makes two Start-menu entries, "CMD Studio" and "CMD Studio Recovery", and starts the app. The first run is a short wizard: pick a theme, choose where it goes, apply.

## What it changes on your PC, and how to undo it

Every change is listed before it is made, and a restore point is taken first. CMD Studio touches only: cmd's AutoRun value under `HKEY_CURRENT_USER` (so every cmd window loads the theme; or only Windows Terminal's cmd profile, your choice), Windows Terminal's `settings.json` (a color scheme named "CMD Studio" and the cmd profile), a per-user font (CaskaydiaCove Nerd Font, installed only when a theme needs it), and its own data folder `%LOCALAPPDATA%\ai.resincore.cmdstudio`. Nothing needs administrator rights; nothing is sent anywhere (no telemetry; diagnostics leave your PC only when you copy them yourself).

- *Undo* on the home page returns to the restore point of the last apply.
- *Undo everything* in Help puts back everything CMD Studio ever changed.
- Uninstalling (Settings > Apps, or the Start menu) does the same and removes the program; your themes stay unless you tick the box to delete the app's data.
- If cmd will not open: Start menu > *CMD Studio Recovery* puts back any cut-off change, pauses the theme, and opens CMD Studio in recovery mode.

## Theme files

A theme is a `.cmdtheme` file: open JSON with a published schema (`schema/cmdtheme-1.schema.json`; the file's `$schema` line points at it, and `uuid` needs `ajv-formats` in Ajv). Export from the app, import on another PC, and it looks the same. The built-in themes are in `presets/`.

## The command line

The installer puts `cmdstudio` on your Path. In a new cmd or PowerShell window:

| Command | Does |
|---|---|
| `cmdstudio doctor [--json]` | checks the install and says what is wrong |
| `cmdstudio apply <file.cmdtheme> [--target all\|wt] [--replace-other-clink]` | applies a theme |
| `cmdstudio undo` | undoes the last apply |
| `cmdstudio --cleanup [--remove-themes]` | what the uninstaller runs: puts everything back |
| `cmdstudio --help` | the full usage, with `--data-dir` and `--resources` |

Exit codes: 0 done, 1 failed and rolled back, 2 invalid input, 3 rollback incomplete (it lists what to fix by hand). `cmdstudio` is `cmdstudio.com`, a console program cmd waits for; `cmdstudio.exe` is the app's window, and typed by name it prints after the prompt returns.

## Building from source

Rust (`rust-toolchain.toml` pins it), Node (`app/.nvmrc`), and the pinned runtimes: `./scripts/fetch-resources.ps1` downloads Oh My Posh, Clink and the font, checks their SHA-256 and writes `resources/`. Then `cargo test --workspace`, and in `app/`: `npm ci`, `npm test`, `npm run tauri -- build` (the installer lands in `target/release/bundle/nsis/`). CI runs the same on every push (`.github/workflows/ci.yml`); a tag `v<version>` builds the draft release (`release.yml`).

## Credits and licenses

CMD Studio is MIT (`LICENSE`). It bundles, unmodified: [Clink](https://github.com/chrisant996/clink) 1.9.34 (GPL-3.0; the source of that exact release is linked from `resources\LICENSES\clink-SOURCE.txt`), [Oh My Posh](https://github.com/JanDeDobbeleer/oh-my-posh) 31.5.0 (MIT), and the CaskaydiaCove Nerd Font from [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts) 3.5.1 (SIL OFL 1.1). The built-in themes' colors come from Tokyo Night, Catppuccin, Dracula, Nord, Gruvbox, One Dark and Solarized, all MIT; `docs/credits.md` has every copyright line, and so does the app's credits page.

## Contributing and security

Issues and pull requests at [GitHub](https://github.com/luraselenehalo/cmd-studio). For a security problem, open an issue with "security" in the title and no details, and the maintainer will reply with a private channel.
```

- [ ] **Step 2: Check the pins**

`grep -n "1.9.34\|31.5.0\|3.5.1" README.md` must show the three versions as `crates/core/src/plan.rs` and `resources.rs` pin them; `grep -n "mrgun" README.md` must print nothing. Render check: GitHub renders the table and the links; nothing to run.

- [ ] **Step 3: Commit**

```
docs: the English README (phase 4a)

What it is, what it needs, how to install past SmartScreen and why
the build is unsigned, what it changes and how to undo it, the theme
file and its schema, the CLI and its exit codes, building from
source, credits and licenses, contributing and security.
```

File: `README.md`.

---

### Task 8: The main spec records what shipped

The architect's edits of 2026-11-04 said the design; this task writes the facts of the build into the same places: §6.1's measured installer size and the file list as CI listed them, §6.4's workflow as it stands (and F-V if it was taken), appendix B's row (the fallbacks taken, the CI run id of the installer the VM check used once Task 9 is in — Task 9 adds that line). Thai, in the spec's register; code names in English.

**Files:**
- Modify: `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§6.1, §6.4, appendix B's 2026-11-04 row)

- [ ] **Step 1: §6.1**

Replace the bullet "ตัวติดตั้งเป็นไฟล์เดียว ประเมินขนาดไว้ราว 20–30 MB" and its two sub-bullets with: "ตัวติดตั้งเป็นไฟล์เดียว ขนาดที่วัดจริง (phase 4a, CI run <id>): <n> MB ✅ (OMP 14.4 MB และ Clink x64 2.9 MB อยู่ในนั้น)".

- [ ] **Step 2: §6.4 and appendix B**

In §6.4's phase 4a bullet, replace "(เขียนตอนวางแผน)" if present with the run id of the first green `release.yml` dry run. In appendix B's 2026-11-04 row, append one bullet per fallback taken (F-A, F-B, F-C, F-V) or "ไม่ได้ใช้ fallback ใด" when none was.

- [ ] **Step 3: The spec's tests, then commit**

`cargo test -p cmdstudio-core --test error_codes` and `cargo test -p cmdstudio-app --lib error::` still pass (§5.9 is unchanged here). `git diff docs/superpowers/specs/2026-10-10-cmd-studio-design.md` touches only §6.1, §6.4 and appendix B.

```
spec: phase 4a as shipped in §6.1, §6.4 and appendix B
```

---

### Task 9: The VM check of install and uninstall (the protocol and the scripts here; the run is the controller's)

Spec §8's phase 4 criterion, the half this plan owns: install and uninstall pass on VM-A, VM-B and VM-C; after uninstall the registry, Windows Terminal's settings and the fonts are byte-equal to before; the themes stay unless the box is ticked; `cmdstudio doctor` typed in cmd prints before the prompt returns and gives cmd its exit code; the two shortcuts; the Path on and off.

**PC safety, for this whole task:** every script here is for the VMs and is parse-checked only on this PC (Global Constraints). The controller runs the protocol against CI's `cmdstudio-installer` artifact of this branch's head after the final review, downloaded once with the founder's explicit yes.

**Files:**
- Create: `docs/spikes/phase4-install/protocol.md`
- Create: `docs/spikes/phase4-install/helpers.ps1`
- Create: `spikes/vm/guest/install-check.ps1`
- Modify: `spikes/vm/guest/guard.test.ps1` (`'install-check.ps1' = @('-Snapshot', 'before')`)

**Interfaces:**
- Consumes: `spikes/vm/vm.ps1` (`Invoke-Guest`, `Invoke-GuestPs`, `Invoke-GuestDesktop`, `Copy-ToGuest`, `Send-Run`, `Save-Shot`, `Wait-GuestReady`), `docs/spikes/phase3-vm-check/helpers.ps1` (`Start-CleanVm`, `Send-Key`, `Send-Text`, `Get-GuestAutoRun`, `Get-GuestWindows`, `Get-GuestCmdWindows`, `Invoke-GuestButton`, `Get-GuestFocus`), the words of `en.ts`/`th.ts`, the hooks' message texts (Task 3), the CLI's words (Task 2).
- Produces: `install-check.ps1 -Snapshot before|after [-Out <dir>]` (writes the machine's state: the registry values, WT's settings bytes, the user fonts folder, the shortcuts, the Path, the install and data folders), `install-check.ps1 -Compare [-Out <dir>] [-ThemesKept|-ThemesGone]` (prints `SAME`/`DIFFERENT` per item and a verdict line), `install-check.ps1 -Cli` (runs `cmdstudio doctor` in a cmd and a PowerShell and prints the order of output and prompt, and the exit codes), `install-check.ps1 -HoldFont` / `-ReleaseFont` (opens the installed font file for the failed-cleanup step); the protocol with PASS lines.

- [ ] **Step 1: `install-check.ps1`**

```powershell
# Runs inside a test VM (plan 4a Task 9): the state a CMD Studio install and uninstall must leave
# byte-equal, written to files before and compared after. -Snapshot before|after records, under
# -Out, the user's AutoRun value (type and bytes), the user's Path value (type and bytes), the
# user's font registrations (HKCU\...\Fonts, every value), the user fonts folder's files with
# their hashes, Windows Terminal's settings.json (bytes, or ABSENT), the Start-menu folder "CMD
# Studio" (the .lnk files, each one's target and arguments), the uninstall registry key of CMD
# Studio, the install folder's file list, and the data folder's top-level entries. -Compare says
# SAME or DIFFERENT per item against the two snapshots, requires the install folder, the
# shortcuts and the uninstall key to be gone, and, with -ThemesKept, the data folder to hold
# themes\ and nothing of CMD Studio's own files (state.json, snapshots\ …), with -ThemesGone the
# data folder to be gone. -Cli runs `cmdstudio doctor` in cmd and in PowerShell and prints,
# for each, the lines in the order they came and the exit code (the console exe prints before
# the prompt returns and hands cmd the code). -HoldFont opens the installed font file and keeps
# it open for -HoldSeconds; -ReleaseFont is the note that the hold ended (the holder is a
# process this script starts, which ends by itself).
param(
  [ValidateSet('before', 'after', '')] [string] $Snapshot = '',
  [switch] $Compare, [switch] $ThemesKept, [switch] $ThemesGone, [switch] $Cli,
  [switch] $HoldFont, [int] $HoldSeconds = 600,
  [string] $Out = 'C:\cmdstudio\out\phase4'
)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script reads the user's registry and starts programs" }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$local = $env:LOCALAPPDATA
$data = Join-Path $local 'ai.resincore.cmdstudio'
$inst = Join-Path $local 'CMD Studio'
$sm = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\CMD Studio'
$wt = Join-Path $local 'Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json'

function RegRaw([string] $key, [string] $name) {
  # Type and bytes, so that REG_SZ and REG_EXPAND_SZ, and every byte, are compared.
  $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($key)
  if ($null -eq $k) { return 'KEY ABSENT' }
  try {
    if ($k.GetValueNames() -notcontains $name) { return 'ABSENT' }
    $kind = $k.GetValueKind($name)
    $v = $k.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $bytes = if ($v -is [byte[]]) { $v } elseif ($v -is [string]) { [Text.Encoding]::Unicode.GetBytes($v) } else { [Text.Encoding]::Unicode.GetBytes("$v") }
    "$kind " + [BitConverter]::ToString($bytes)
  } finally { $k.Close() }
}
function FontValues {
  $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Microsoft\Windows NT\CurrentVersion\Fonts')
  if ($null -eq $k) { return 'KEY ABSENT' }
  try { ($k.GetValueNames() | Sort-Object | ForEach-Object { "$_ = $($k.GetValue($_))" }) -join "`n" } finally { $k.Close() }
}
function Files([string] $dir) {
  if (-not (Test-Path -LiteralPath $dir)) { return 'ABSENT' }
  (Get-ChildItem -LiteralPath $dir -Recurse -File | Sort-Object FullName | ForEach-Object { "$($_.FullName.Substring($dir.Length)) $((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash)" }) -join "`n"
}
function Shortcuts {
  if (-not (Test-Path -LiteralPath $sm)) { return 'ABSENT' }
  $sh = New-Object -ComObject WScript.Shell
  (Get-ChildItem -LiteralPath $sm -Filter *.lnk | Sort-Object Name | ForEach-Object { $l = $sh.CreateShortcut($_.FullName); "$($_.Name) -> $($l.TargetPath) [$($l.Arguments)]" }) -join "`n"
}
function State {
  [ordered]@{
    autorun   = RegRaw 'Software\Microsoft\Command Processor' 'AutoRun'
    path      = RegRaw 'Environment' 'Path'
    fontsReg  = FontValues
    fontsDir  = Files (Join-Path $local 'Microsoft\Windows\Fonts')
    wt        = if (Test-Path -LiteralPath $wt) { (Get-FileHash -LiteralPath $wt -Algorithm SHA256).Hash } else { 'ABSENT' }
    shortcuts = Shortcuts
    uninstall = if (Test-Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CMD Studio') { 'PRESENT' } else { 'ABSENT' }
    install   = Files $inst
    dataTop   = if (Test-Path -LiteralPath $data) { (Get-ChildItem -LiteralPath $data -Force | Sort-Object Name | ForEach-Object Name) -join "`n" } else { 'ABSENT' }
  }
}
if ($Snapshot) {
  $s = State
  foreach ($k in $s.Keys) { [IO.File]::WriteAllText((Join-Path $Out "$Snapshot-$k.txt"), "$($s[$k])", (New-Object Text.UTF8Encoding $false)) }
  "snapshot $Snapshot written: $($s.Keys -join ', ')"
  "path: $($s.path)"
  "shortcuts:`n$($s.shortcuts)"
  "data folder: $($s.dataTop -replace "`n", ', ')"
}
if ($Compare) {
  $fail = 0
  foreach ($k in 'autorun', 'path', 'fontsReg', 'fontsDir', 'wt') {
    $b = [IO.File]::ReadAllText((Join-Path $Out "before-$k.txt")); $a = [IO.File]::ReadAllText((Join-Path $Out "after-$k.txt"))
    if ($b -eq $a) { "SAME $k" } else { "DIFFERENT $k`n  before: $b`n  after:  $a"; $fail++ }
  }
  $after = State
  foreach ($k in 'shortcuts', 'uninstall', 'install') { if ($after[$k] -eq 'ABSENT') { "GONE $k" } else { "STILL THERE $k`n  $($after[$k])"; $fail++ } }
  if ($ThemesKept) {
    $top = $after.dataTop -split "`n"
    if ($top -contains 'themes' -and -not ($top -contains 'state.json') -and -not ($top -contains 'snapshots')) { 'THEMES KEPT, the rest gone' } else { "DATA FOLDER WRONG: $($after.dataTop -replace "`n", ', ')"; $fail++ }
  }
  if ($ThemesGone) { if ($after.dataTop -eq 'ABSENT') { 'DATA FOLDER GONE' } else { "DATA FOLDER STILL THERE: $($after.dataTop -replace "`n", ', ')"; $fail++ } }
  if ($fail) { "VERDICT: FAIL ($fail)" } else { 'VERDICT: PASS' }
}
if ($Cli) {
  # cmd: the output must come before the prompt, and %errorlevel% must be doctor's code. cmd runs
  # without /d here on purpose: AutoRun is under test in these VMs.
  $cmdOut = Join-Path $Out 'cli-cmd.txt'
  & "$env:SystemRoot\System32\cmd.exe" /c "cmdstudio doctor > `"$cmdOut`" 2>&1 & call echo EXIT=%^errorlevel% >> `"$cmdOut`" & echo PROMPT-RETURNED >> `"$cmdOut`""
  '--- cmd:'; Get-Content -LiteralPath $cmdOut
  '--- powershell:'
  $psOut = & "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -Command "cmdstudio doctor; 'EXIT=' + `$LASTEXITCODE; (Get-Command cmdstudio).Source"
  $psOut
}
if ($HoldFont) {
  $font = Join-Path $local 'Microsoft\Windows\Fonts\CaskaydiaCoveNerdFont-Regular.ttf'
  if (-not (Test-Path -LiteralPath $font)) { throw "no installed font at $font" }
  $holder = Start-Process -PassThru -WindowStyle Hidden "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -ArgumentList @('-NoProfile', '-Command', "`$f=[IO.File]::Open('$font','Open','Read','None'); Start-Sleep -Seconds $HoldSeconds; `$f.Close()")
  "holding $font for $HoldSeconds s in pid $($holder.Id)"
}
```

Parse-check on this PC: `[System.Management.Automation.Language.Parser]::ParseFile('<repo>\spikes\vm\guest\install-check.ps1', [ref]$null, [ref]$errors); $errors.Count` must print `0`. Add `'install-check.ps1' = @('-Snapshot', 'before')` to `guard.test.ps1`'s `$changers` (the guard is the script's first statement after `$ErrorActionPreference`, as the test's regex expects: keep the two lines in that order). The guard total becomes `2 * 21 + 3 = 45`.

- [ ] **Step 2: `helpers.ps1` for the host**

```powershell
# Host-side helpers for the phase 4a VM check (protocol.md). Dot-source after spikes\vm\vm.ps1
# and docs\spikes\phase3-vm-check\helpers.ps1. Only the CMD Studio test VMs get anything from
# them; this PC's installer is never run.
$P4 = @{
  Setup = 'C:\cmdstudio\phase4\setup'    # the installer and SHA256SUMS.txt
  Guest = 'C:\cmdstudio\spikes\vm\guest'  # install-check.ps1, run-on-desktop.ps1
  Out   = 'C:\cmdstudio\out\phase4'       # the snapshots and the CLI's outputs
}
function Install-Phase4Setup($Vm, [string] $User, [string] $Artifact) {
  $repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
  Invoke-GuestPs $Vm "New-Item -ItemType Directory -Force -Path '$($P4.Out)' | Out-Null" $User 60 | Out-Null
  Copy-ToGuest $Vm @((Join-Path $repo 'spikes\vm\guest\run-on-desktop.ps1'), (Join-Path $repo 'spikes\vm\guest\install-check.ps1'), (Join-Path $repo 'spikes\vm\guest\guard.test.ps1')) ($P4.Guest + '\') $User | Out-Null
  $exe = Get-ChildItem -LiteralPath $Artifact -Filter '*-setup.exe' | Select-Object -First 1
  Copy-ToGuest $Vm @($exe.FullName, (Join-Path $Artifact 'SHA256SUMS.txt')) ($P4.Setup + '\') $User | Out-Null
  $hostHash = (Get-FileHash -LiteralPath $exe.FullName -Algorithm SHA256).Hash
  $inVm = "$(Invoke-GuestPs $Vm "(Get-FileHash -LiteralPath '$($P4.Setup)\$($exe.Name)' -Algorithm SHA256).Hash" $User 180)"
  'installer: ' + $exe.Name + ' ' + $exe.Length + ' bytes'
  'sha256 on the host: ' + $hostHash
  'sha256 in the VM:   ' + $inVm.Trim()
  'same file: ' + ($inVm -match $hostHash)
  'in SHA256SUMS.txt:  ' + ((Get-Content -LiteralPath (Join-Path $Artifact 'SHA256SUMS.txt')) -match $hostHash.ToLowerInvariant()).Count
  $exe.Name
}
function Get-Phase4Snapshot($Vm, [string] $Which, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" "-Snapshot $Which" $User 300 }
function Compare-Phase4($Vm, [string] $Themes, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" "-Compare -$Themes" $User 300 }
function Get-Phase4Cli($Vm, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" '-Cli' $User 300 }
function Get-Phase4Installed($Vm, [string] $User = 'tester') {
  Invoke-GuestPs $Vm "Get-ChildItem -LiteralPath '$env:LOCALAPPDATA' -Directory -Filter 'CMD Studio' | ForEach-Object FullName; Get-ChildItem -LiteralPath (Join-Path `$env:LOCALAPPDATA 'CMD Studio') -Force -ErrorAction SilentlyContinue | ForEach-Object Name; Get-Process cmdstudio -ErrorAction SilentlyContinue | ForEach-Object { 'running: ' + `$_.Id }" $User 120
}
```

Parse-check it as the guest script.

- [ ] **Step 3: `protocol.md`**

```markdown
# Phase 4a — VM check of install and uninstall (controller)

- **What it checks:** spec §8's phase 4 criterion, the install and uninstall half (update is 4b's): on VM-A, VM-B and VM-C the installer installs for the user, starts the app, makes the two shortcuts and the Path entry, `cmdstudio doctor` typed in cmd prints before the prompt returns and gives cmd its exit code, and the uninstaller puts AutoRun, the user's Path, the font registrations and files, and Windows Terminal's settings back byte for byte, keeps the themes unless the box is ticked, and leaves no install folder, shortcut or uninstall key.
- **The build:** CI's artifact `cmdstudio-installer` of this branch's head (ci.yml's `app` job): `CMD Studio_<version>_x64-setup.exe` and `SHA256SUMS.txt`. One download per check, with the founder's explicit yes first: `gh run download <run id> --name cmdstudio-installer --dir <scratchpad>\p4a-installer`; then `git diff --stat <headSha> HEAD -- app crates presets schema scripts Cargo.toml Cargo.lock .github` must print nothing. Record the commit and the run id below.
- **VMs** (`spikes/vm/README.md`; each from `clean`): VM-A `cmdstudio-win11` (`tester`), VM-B `cmdstudio-win10` (`tester`, no Windows Terminal), VM-C `cmdstudio-th` (the Thai user with a space; `$u` as `docs/spikes/phase3-vm-check/protocol.md` writes it in code points).
- **Setup (once per host session):** Windows PowerShell 5.1 on the host, outside any sandbox, unlocked session, from the repo root: `. .\spikes\vm\vm.ps1`, `. .\docs\spikes\phase3-vm-check\helpers.ps1`, `. .\docs\spikes\phase4-install\helpers.ps1`; `$artifact = '<scratchpad>\p4a-installer'`. Per VM: `$vm`, `$u` as in phase 3's protocol; `Start-CleanVm $vm $u`; `$setup = Install-Phase4Setup $vm $u $artifact` (prints `same file: True`; `$setup` is the installer's file name); then `Invoke-GuestDesktop $vm "$($P4.Guest)\guard.test.ps1" '' $u` must end with `guest guards: 45/45 checks pass`.
- **Rules:** nothing here runs on the dev PC, not even as a dry run; every guest script refuses a machine whose name does not start with `CMDSTUDIO-`; cmd runs without `/d` inside the VMs; do not repair a VM: record the step and go on; the installer is driven with the keyboard (`Send-Key`), and a screenshot after every page; the words are `en.ts`'s (`th.ts`'s on VM-C once Thai is chosen in the wizard, as phase 3's protocol says) and the hooks' message texts of `app/src-tauri/windows/hooks.nsh`.
- **How results are judged:** by `install-check.ps1`'s printed lines (`SAME`, `GONE`, `VERDICT: PASS`), by the `-Cli` output's order (the doctor lines, then `EXIT=<n>`, then `PROMPT-RETURNED`), and by pixel counts or words on a screenshot where nothing prints (the installer's pages, SmartScreen).
- **Date / commit / run:** written by the controller.

## Steps (VM-A all; VM-B 1–5, 7, 9; VM-C 1–5, 7, 8, 9)

| # | Do | PASS when | Result |
|---|---|---|---|
| 1 | **Before.** `Get-Phase4Snapshot $vm before $u`. | Prints `snapshot before written`, `path: ExpandString …WindowsApps;` (VM-A, VM-C) or the VM's own, `shortcuts: ABSENT`, `data folder: ABSENT`. | |
| 2 | **Install, interactive (VM-A, VM-C).** `Send-Run $vm "$($P4.Setup)\$setup"`; wait 5 s; `Save-Shot $vm 'p4a-smartscreen'`. If SmartScreen shows ("Windows protected your PC"): it is the unsigned build (§6.5 step 1); record it, then `Send-Key $vm Tab 1` to "More info" is not reachable by keyboard on every build: use `Invoke-GuestButton $vm 'More info' $u` then `Invoke-GuestButton $vm 'Run anyway' $u`. The installer's pages: `Save-Shot` each; `Send-Key $vm Enter` for Next/Install/Finish ("Run CMD Studio" ticked on the finish page: zoom the shot). Wait 15 s. `Get-Phase4Installed $vm $u`; `Save-Shot $vm 'p4a-app-started'`. **VM-B:** `Invoke-GuestDesktop $vm "$($P4.Setup)\$setup" '/S' $u 300` (silent; the hooks run all the same; no app start), then `Get-Phase4Installed`. | `Get-Phase4Installed` lists `cmdstudio.exe`, `cmdstudio.com`, `resources`, `LICENSE.txt`, `uninstall.exe` and (VM-A, VM-C) `running: <pid>`; the app's window is up (the wizard's page 1). No ARM64 box. | |
| 3 | **The shortcuts and the Path.** `Get-Phase4Snapshot $vm mid $u` (a third snapshot, read only). | `shortcuts:` has `CMD Studio.lnk -> …\CMD Studio\cmdstudio.exe []` and `CMD Studio Recovery.lnk -> …\CMD Studio\cmdstudio.exe [--recovery]`; `path:` ends with `…\CMD Studio;` (VM-A, VM-C: the fresh value ended with `;`) or `;…\CMD Studio` (a value without the trailing `;`), the kind unchanged (`ExpandString`). | |
| 4 | **The CLI in cmd.** `Get-Phase4Cli $vm $u`. | Under `--- cmd:` the doctor's lines (`D1 …` to `D12 …`, English) come first, then `EXIT=<0 or 1>` (1 when no theme is applied yet and a check warns: record the code), then `PROMPT-RETURNED`. Under `--- powershell:` the doctor's lines, `EXIT=<the same>`, and `(Get-Command cmdstudio).Source` ending in `\CMD Studio\cmdstudio.com`. No Thai on VM-C. | |
| 5 | **Apply a theme and save one of your own.** The wizard to Terracotta applied (phase 3's protocol Run 2 step 4: five activations), "Go to the home page"; Themes > Terracotta > "Customize" > Save (a "My Terracotta" in `themes\`); `Get-GuestAutoRun $vm 'applied' $u`; `Send-Run $vm 'cmd'`, wait 10 s, `Save-Shot $vm 'p4a-cmd-themed'`, `& $closeCmd` (phase 3's). Close the app (`& $closeApp`). `Get-Phase4Snapshot $vm applied $u`. | AutoRun holds CMD Studio's guarded part; the cmd window shows the welcome box (pixel count of Terracotta's background `#1F1E1D` > 100,000 on VM-A, VM-C; VM-B conhost: the prompt's `❯` region only); `data folder:` lists `themes`, `state.json`, `snapshots`, `generated`, `clink-profile`, `runtime`. | |
| 6 | **A cleanup that cannot finish (VM-A only; Review Focus 3).** `Invoke-GuestDesktop $vm "$($P4.Guest)\install-check.ps1" '-HoldFont' $u` (the font file held open). `Send-Run $vm "$env:LOCALAPPDATA\CMD Studio\uninstall.exe"` (the user's own `%LOCALAPPDATA%` expands on the guest: use `Send-Text` with the full path `C:\Users\tester\AppData\Local\CMD Studio\uninstall.exe`). On the uninstaller's page tick "Delete the application data" (`Send-Key $vm Tab` to the box, `Space`; zoom the shot), then Uninstall. Wait 60 s; `Save-Shot $vm 'p4a-cleanup-failed'`. `Send-Key $vm Enter` on the box. `Get-Phase4Installed $vm $u`; `Get-Phase4Snapshot $vm after $u`. | The box "CMD Studio could not put everything back (cleanup exit code 3). What it said:" with a `FAILED` step naming the font, and "your CMD Studio data folder is kept". After it: the install folder gone, `data folder:` still lists `state.json` and `snapshots` (NSIS did not delete it: the hook un-ticked the box). AutoRun `(absent)`. | |
| 6b | **The second cleanup by hand (VM-A).** After the hold ends (10 min) or `Invoke-GuestPs $vm 'Get-Process powershell | Where-Object { $_.StartTime -gt (Get-Date).AddMinutes(-15) } | Stop-Process' $u`: there is no `cmdstudio.com` any more (uninstalled), so run the artifact's: `Copy-ToGuest` is not needed, the exe of ci's `cmdstudio-exe` artifact is not here; instead reinstall silently (`'/S'`, step 2's VM-B form), then `Invoke-GuestDesktop $vm "$($P4.Setup)\..\..\..\Users\$u\AppData\Local\CMD Studio\cmdstudio.com" '--cleanup --remove-themes' $u` is wrong too (run-on-desktop takes a .ps1): use `Invoke-GuestPs $vm "& 'C:\Users\$u\AppData\Local\CMD Studio\cmdstudio.com' --cleanup --remove-themes; 'EXIT=' + `$LASTEXITCODE" $u 300`, then uninstall again with the box ticked. `Compare-Phase4 $vm ThemesGone $u`. | The hand cleanup prints its steps and `EXIT=0`; the uninstall shows no box; `VERDICT: PASS` with `DATA FOLDER GONE` and `SAME` for the five items. | |
| 7 | **Uninstall, themes kept (VM-B, VM-C; VM-A after a fresh `Start-CleanVm` and steps 1, 2 (silent is fine), 5).** VM-B: `Invoke-GuestDesktop $vm 'C:\Users\tester\AppData\Local\CMD Studio\uninstall.exe' '/S' $u 300` is not a .ps1: use `Invoke-GuestPs $vm "Start-Process -Wait -FilePath 'C:\Users\$u\AppData\Local\CMD Studio\uninstall.exe' -ArgumentList '/S'" $u 300`. VM-C and VM-A: interactive as in 6 without ticking the box. Wait 30 s. `Get-Phase4Snapshot $vm after $u`; `Compare-Phase4 $vm ThemesKept $u`; `Get-GuestAutoRun $vm 'after-uninstall' $u`. | `VERDICT: PASS`: `SAME autorun`, `SAME path`, `SAME fontsReg`, `SAME fontsDir`, `SAME wt`, `GONE shortcuts`, `GONE uninstall`, `GONE install`, `THEMES KEPT, the rest gone`. No box. | |
| 8 | **Thai (VM-C).** In step 2 the installer's text is English (Tauri's NSIS, one language); step 4's output has no Thai; in step 5 the wizard is walked in Thai (page 1: ไทย) and the cmd window's welcome box is English under the Thai user folder. | As phase 3's protocol for the words; `Get-Phase4Cli`'s lines all ASCII or the user's own folder name. | |
| 9 | **Install again over an install (the update path's hook, without an update).** After 7 on VM-B: install silently again (step 2's VM-B form) and once more over it. `Get-Phase4Snapshot $vm reinstall $u`. | The second install over the first runs the old uninstaller with `/UPDATE` (`$UpdateMode = 1`): AutoRun is untouched by it (no theme was applied on this fresh run, so `(absent)` both times; on VM-A, with Terracotta applied before the reinstall, AutoRun keeps CMD Studio's part), `path:` has exactly one `\CMD Studio` entry, `shortcuts:` has the two. | |

### Results

Written by the controller: the date, the commit, the CI run id, the installer's name and size, each step's Result (PASS with the printed lines, or the finding), the screenshots `spikes\vm\tmp\<vm>-p4a-*.png` cited by label. Findings go to the task that owns them before the PR.
```

Rewrite step 6b's "Do" cell to the clean form before committing: "After the hold ends, reinstall silently (`Invoke-GuestPs … Start-Process -Wait … '/S'`), run `Invoke-GuestPs $vm "& 'C:\Users\$u\AppData\Local\CMD Studio\cmdstudio.com' --cleanup --remove-themes; 'EXIT=' + $LASTEXITCODE" $u 300`, then uninstall interactively with the box ticked, then `Compare-Phase4 $vm ThemesGone $u`." (the draft above thinks aloud; the committed table does not). Same for step 7's VM-B cell: only the `Invoke-GuestPs … Start-Process -Wait` form.

- [ ] **Step 4: Parse checks, the guard test's count, then commit**

Parse-check the two `.ps1` files. `guard.test.ps1`'s total line is computed (`2 * $changers.Count + 3`), so no number changes by hand.

```
docs, spikes: the phase 4a VM check of install and uninstall (protocol and scripts)

install-check.ps1 (VM only): before/after snapshots of AutoRun, the
user Path (type and bytes), the font registrations and files, WT's
settings, the shortcuts, the uninstall key, the install and data
folders; -Compare with the themes kept or gone; -Cli for `cmdstudio
doctor` in cmd and PowerShell (output before the prompt, the exit
code); -HoldFont for a cleanup that cannot finish. helpers.ps1 on the
host; protocol.md with the steps for VM-A, VM-B and VM-C and the
one-download rule. Parse-checked only; the controller runs it.
```

Files: `docs/spikes/phase4-install/protocol.md docs/spikes/phase4-install/helpers.ps1 spikes/vm/guest/install-check.ps1 spikes/vm/guest/guard.test.ps1`.

- [ ] **Step 5: The run (controller)**

After the final review: the download with the founder's yes, the three VMs, the Results section, the appendix B line in Task 8's row ("VM check ของ phase 4a: <date>, run <id>, ผล …"). A FAIL goes back to its task; the PR waits.

## Self-review (done while writing; the executor need not repeat it)

- **Spec coverage.** §6.1: the installer, per user, x64, ARM64 refused, the app started, the Recovery shortcut (Task 3); the file list and size (Tasks 3, 8). §6.3: the hook order, `$UpdateMode`, `--remove-themes`, the kept data folder on a failed cleanup (Task 3), `--cleanup` itself unchanged (phase 1b). §6.4: the tag workflow to a draft, one version, SignPath commented, winget noted (Tasks 5, 6). §6.5: the unsigned text in the notes and the README (Tasks 6, 7). §6.6: the licenses inside and named (Tasks 3, 4). §5.7: `cmdstudio.com`, the two Path commands, `--recovery` through the `.com` (Tasks 1, 2). §5.9: `E_PATH_WRITE` (Task 2). §8: install and uninstall on every tier (Task 9). Recovery spec §4.4: the shortcut's argument (Task 3) and the guide's sentence (Task 4). The brief's "one version source", "English README", "M7", "VM check" each have a task. Update, the updater key, the upstream watch: 4b.
- **Placeholders.** `<date>`, `<version>`, `<id>`, `<n>` stand for values the implementer or the controller measures and are named as such; `<from SignPath>` sits in a commented step the founder fills. No "TBD".
- **Types.** `console_start(&[OsString]) -> ConsoleStart` (Task 1) is what `cli.rs` calls; `user_path::{add, remove}(&dyn UserPath, &Path) -> Result<PathChange, PathError>` (Task 2) is what `Say::path_change` calls, with `PathError::code()`; `Defaults.exe_dir` is read by `run_with` and set by `run`, the tests and `system.rs`; `System.user_path` is set in the five literals named; `CONSOLE_EXE` is what `config_guard.rs` compares the resource target to.
- **Review Focus.** 1 and 2 in Task 2's tests, 3 in Task 3's hooks-text test and Task 9 step 6, 4 in Task 1's usage test and Task 7, 5 in Task 3's hooks-text test and Task 9 step 9 (and 4b's update check).
