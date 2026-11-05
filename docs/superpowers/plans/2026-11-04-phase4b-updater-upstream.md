# Phase 4b Implementation Plan — The updater: its code and UI over a fake, the install-moment spike, the key-dependent wiring, the upstream-watch workflow, and the update VM check

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. The tasks keep the outline's numbers; they run in this order: **Task 2, Task 3, Task 1 (its CI half by the implementer, its VM half by the controller), Task 4, Task 5, Task 7, Task 8 (Steps 1–3), Task 6 (waits for the founder's key), Task 9 (waits for a published release with updater assets; the controller runs it), Task 8 Step 4 (the VM facts).** The first six of those run and pass without the updater key. Task 1's VM run and Task 9's run are the controller's (VirtualBox VMs); an implementer never runs them. No founder step is needed before Task 6.

**Goal:** CMD Studio updates itself from GitHub Releases: it checks on every launch, in the mode of `settings.json` (automatic by default, notify first, off), never interrupts a transaction, never steals focus, stays silent offline, always says "Updated to vX" with a link to what changed, and after an update installs the new runtime folder, re-applies the current theme through a transaction and deletes the old runtime when nothing holds it; OMP, Clink and the font move only with the app, through a daily workflow that opens a PR when upstream has a new version; update passes on every tier (spec §8).

**Architecture:** `tauri-plugin-updater` (2.13.x, pinned) is used from Rust only, behind an `Updater` trait that the app's `backend/update.rs` drives (`check`, `download`, `install_now`), with a fake for every test; no JS permission is granted to the window and no npm package is added. The state machine is pure: `UpdateMode` × network outcome × lock state → `UpdateState`, exposed by a new read-only IPC `update_status` and driven by `update_install` (the notify-mode button, the Recovery page's "Update now"). Two install paths, both production code: **at close** (automatic mode: the verified installer, written to `<data>\updates\`, is started by the app's own spawn with `/S /UPDATE` at `RunEvent::Exit`, no relaunch) and **now** (the user's click: the plugin's own `Update::install`, which runs the installer with `/S /R /UPDATE /ARGS` and ends the process, so the app comes back by itself). Before either, the app writes `<data>\update.json` (`{version, from, to, notesUrl}`) and ends its run record; the next start reads it, shows the notice through `app_status.updated`, re-applies the theme if the runtime pins changed, prunes old runtime folders, and `app_ready` deletes the file. The moment of the automatic install is the spike's decision (Task 1): the primary is "at close, silent, no relaunch"; Task 3 is written for it with the fallbacks named. The release workflow signs the installer with the founder's key and uploads `latest.json`; a daily workflow rewrites `scripts/fetch-resources.ps1`'s pins and opens a PR, never merging, never tagging.

**Tech Stack:** Rust 1.99.0, Tauri 2.12.4 (`@tauri-apps/cli` 2.12.3, `@tauri-apps/api` 2.12.4), `tauri-plugin-updater` **=2.13.3** (the newest 2.13.x on the crates.io index on 2026-11-05: tauri `^2.12`, tauri-plugin `^2.7.1`, minisign-verify `^0.3`, reqwest `^0.13`; G10's table lists 2.13.2, which has the same tauri requirement; see G-UPD), `semver` =1.0.28 (already in the tree through tauri-utils), minisign through the plugin (the key pair from `tauri signer generate`), ts-rs 12.0.1, React 19.3.1 / Vitest 5.0.5, GitHub Actions (`windows-latest`, `ubuntu-latest`; `gh`, `jq`, `python3`, `7z`), PowerShell 5.1 and 7 for the upstream script (on this PC only in `-WhatIf` and `-Fixture` mode), VirtualBox VMs; `python3 -m http.server` on the host for the spike's `latest.json`.

**Spec:** `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` §2.1 (no silent change), §5.3 (`update.json`, the log lines), §5.5 (the lock), §5.7 (the two commands, `--cleanup`'s list), §5.9 (no new code: `E_BUSY` and `E_INTERNAL` cover the updater), §5.10 (the one program run from outside `runtime\`: the signed installer), §6.2 (all of it; the ⬜ install moment), §6.4 (the updater manifest, the upstream-watch workflow), §8; `docs/superpowers/specs/2026-10-17-recovery-mode-design.md` §5.2 (the "Update now" line on the Recovery page), §6.3 (the restart's journal rule; the busy wait after a reload). Phase 4a's plan and its rulings (`docs/superpowers/plans/2026-11-04-phase4a-installer-cli-release-rulings.md`) are the prerequisite and bind this plan where they name 4b: the carries "the spike checks whether a manual over-install can keep the theme" and "the 4b plan's `latest.json` URL must use the release asset name `CMD-Studio_<version>_x64-setup.exe`", and the VM finding "step 6 moves to 4b's VM run".

**Prerequisite:** branch `phase4b`, cut from main `ea91974` (PR #12, phase 4a, merged). Measure the baselines at the start of Task 2 on this PC (`cargo test --workspace` per crate's `lib` count, `cargo test -p cmdstudio-app --lib`'s count, `npm test`'s file and test count) and write them into Task 2's commit message; every count a later task states is a delta on them.

## What the sources settled (read on 2026-11-05)

The outline assumed a few behaviours; the plan below reads as the sources say.

- **The plugin's quiet install relaunches the app.** `tauri-plugin-updater`'s Windows `install_inner` (`~/.cargo/registry/src/*/tauri-plugin-updater-2.10.1/src/updater.rs:787-866`; 2.13.3 is reread in G-UPD) builds the NSIS arguments as `install_mode.nsis_args()` + `/UPDATE` + `/ARGS` + the app's own arguments + `installer_args`, where `nsis_args()` is `["/S", "/R"]` for `quiet` and `["/P", "/R"]` for `passive` (`src/config.rs:36-47`), runs `on_before_exit`, starts the file with `ShellExecuteW`, and calls `std::process::exit(0)`. The template's `.onInstSuccess` (`installer.nsi` l.27442-27452 of the text in `app/node_modules/@tauri-apps/cli-win32-x64-msvc/cli.win32-x64-msvc.node`) reads `/R` in silent and passive mode and runs `cmdstudio.exe` again. So the outline's F1 ("quiet `install()` does not relaunch") cannot hold, and the plugin's `install` is exactly the "now" path a click asks for: silent, and the app comes back. The automatic at-close install is the app's own spawn with `/S /UPDATE` and no `/R` (no relaunch), which Task 1's U3 confirms.
- **In update mode the old uninstaller never runs, and under `/S` the reinstall page is never reached.** `PageLeaveReinstall` (l.26995-27006) goes to `reinst_done` when `$UpdateMode = 1` before any uninstall; the `/UPDATE` append at l.27042 is on the non-WiX branch of `reinst_uninstall`, reached only when the new installer itself was not started with `/UPDATE`; and NSIS calls no custom page function in silent mode (the template's own comment at l.27220: "the modes that skip the reinstall page choice: silent, passive and update"). So the updater path exercises no `$UpdateMode` guard of `hooks.nsh`: the new installer writes over the files (`Section Install` l.27340-27362, `CheckIfAppIsRunning` first, which in silent mode ends a running app through the Restart Manager, l.27847-27860), and POSTINSTALL renames the `.com` and rewrites the Recovery shortcut as on every install. U2 and U3 check the theme and AutoRun after the update, not an uninstaller's run.
- **A by-hand upgrade keeps the theme only through the page's second choice.** For a newer version the "already installed" page (l.26927-26931, 26955-26990) offers "Uninstall before installing" (the first radio, checked by default) and "Do not uninstall"; the first runs the old uninstaller **without** `/UPDATE` (full cleanup, the theme off, the wizard again, as 4a's VM-B step 9b showed), the second installs over the files and keeps everything. A silent `/S` install keeps everything too. There is no other signal from the newer installer to the old uninstaller. The release notes say so (Task 6); U6 confirms the page's words.
- **The plugin's `check()` never reads the pubkey.** Only `download()` verifies (`verify_signature`, `updater.rs:713`). An empty `pubkey` would not make `check` fail, so the app's `TauriUpdater::check` refuses before any network request when the config's pubkey is empty (Task 3): one log line, `update check=manifest`, and nothing else, until Task 6 fills it.
- **The plugin registers JS commands, which the capability does not grant.** `Builder::build()` adds `check`, `download`, `install` and `download_and_install` under the `updater` plugin (`src/lib.rs:232-236`); `capabilities/default.json` lists no `updater:` permission and `ipc.rs`'s capability test pins that, so the window cannot call them. No `@tauri-apps/plugin-updater` package is installed: `app/package-lock.json` does not change in 4b.
- **The release endpoint.** `https://github.com/<owner>/<repo>/releases/latest/download/<asset>` is served from the newest published release that is neither a draft nor flagged pre-release; a `.sig` and `latest.json` on a draft are not reachable, so Task 9 waits for a published release.

## Global Constraints

Every task's requirements include these. Values are copied from the spec, the brief, the founder's decisions and 4a's Global Constraints (copied here in full, as the outline asked, with 4b's changes marked).

- **No Thai in cmd (spec §1, §5.7, appendix B 2026-10-13).** Nothing CMD Studio writes into a cmd window and nothing the CLI prints is Thai, on every Windows: `cmdstudio.com`'s output, `--add-to-path`'s and `--remove-from-path`'s words, the installer's message boxes (NSIS, English), the updater's log lines. `crates/core/tests/english_in_cmd.rs` and the CLI's `thai(…) == ""` tests keep passing.
- **The UI's languages (founder 2026-10-16).** The app opens in English; Thai is chosen in Settings. Every UI text is a key of both `app/ui/src/i18n/en.ts` and `th.ts`, in `th.ts`'s words (the prompt "พร้อมท์", the add-on "ส่วนเสริม", a version "เวอร์ชัน", the app "CMD Studio", a restore point "จุดย้อนกลับ", an update "อัปเดต"); `i18n.test.ts` fails on a missing key, an empty text, Thai in English or English-only Thai. 4b removes the two reserved empty keys (`home.update.placeholder`, `recovery.update.placeholder`) and their exceptions in that test.
- **Rust production code** (outside `#[cfg(test)]` and `tests/`): no `unwrap`, `expect`, `panic!`, unchecked indexing or slicing, string slicing (the workspace denies clippy `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `string_slice`, `undocumented_unsafe_blocks`); typed errors; every `unsafe` block carries a `// SAFETY:` comment. Never `panic = "abort"` in any profile.
- **The names, each in one place:** the app exe `cmdstudio.exe` (`mainBinaryName` in `tauri.conf.json`, `console::APP_EXE`), the console exe `cmdstudio.com` (`console::CONSOLE_EXE`), the install folder `%LOCALAPPDATA%\CMD Studio`, the Start-menu folder `CMD Studio`, the Recovery shortcut `CMD Studio Recovery.lnk` with `--recovery`, the data folder `%LOCALAPPDATA%\ai.resincore.cmdstudio`, the user Path value `HKCU\Environment` `Path`; 4b adds the update record `<data>\update.json` (`Paths::update_file`), the download folder `<data>\updates\` (`Paths::updates_dir`), the downloaded installer `updates\cmdstudio-<version>-setup.exe`, and the busy name `update_reapply` (`backend::update::UPDATE_REAPPLY_BUSY`).
- **The CLI's contract (spec §5.7)** is unchanged for every command; `--cleanup --data-dir`'s list of CMD Studio's own entries grows by `update.json`, `updates\` and `.update.json.<hex 32>.cmdstudio-tmp` (Task 2, and the spec's sentence in Task 8).
- **The installer (spec §6.1, §6.3, G10):** unchanged in 4b: NSIS, `installMode: "currentUser"`, `app/src-tauri/windows/hooks.nsh` as 4a shipped it (no line of it changes in 4b; `config_guard.rs`'s hooks tests keep passing). The updater runs the new installer with `/S` (silent) and never shows a page; the hooks' `$UpdateMode` guards stay as they are.
- **The user Path (founder 2026-11-04, spec §5.7):** unchanged; an update's POSTINSTALL runs `--add-to-path` again, which changes nothing when the entry is there (Task 9 reads the Path after the update: one entry).
- **One version (spec §6.4, brief):** `[workspace.package] version` in `Cargo.toml` is the source; `tauri.conf.json` has no `version` key; `app/package.json` and `app/package-lock.json` carry the same string; `config_guard::the_version_has_one_source` pins it; the release workflow refuses a tag that is not `v<that version>`. The app's running version for the updater is `tauri::Context::package_info().version` (Cargo's, or a `--config` override's in the spike), never a second constant.
- **Release (spec §6.4, §6.5, brief):** the workflow runs on a pushed tag `v*` (the founder tags and pushes; agents never tag, never push) or by hand (`workflow_dispatch`, a dry run that uploads an artifact and creates nothing); it creates a **draft**, never publishes; from Task 6 on the draft is a normal release named "pre-release for testers" in its title and notes (not flagged pre-release: see "The endpoint" under Decisions); the SignPath step stays commented; winget is a founder step.
- **The modes (spec §6.2, founder 2026-10-16):** `UpdateMode::Auto` (default), `Notify`, `Off`, read from `settings.json` at every launch through `Backend::settings_or_default` (a damaged `settings.json` means the defaults, phase 2b, so Auto).
- **Never interrupts a transaction:** the install step runs only after `Fs::try_lock` of `<data>\cmdstudio.lock` answers `Some` (`None` is `E_BUSY`: the at-close install is postponed to the next launch with one log line `update postponed=busy`; "Update now" answers `E_BUSY` to the window); the lock is held into the installer's start and released by the process's end.
- **Never steals focus:** no window, dialog or relaunch comes from the updater without the user's click; the automatic install runs at close with `/S /UPDATE` and no `/R`; the "Updated to vX" notice is a `StartNotices` card on the next start, not a popup; the notify card is a card on the home page.
- **Always says it (spec §2.1):** every completed update shows "Updated to vX" with a link, once, until closed; the card goes with `update.json`'s deletion, which `app_ready` does after the UI has shown it (no `update_dismiss` command: Close hides the card for this run).
- **Offline is silent:** a check that fails (no network, DNS, GitHub 5xx, a bad `latest.json`, a signature mismatch, an empty pubkey) writes one log line `update check=<kind>` (or `update download=<kind>`) and nothing else; the state is `idle`; nothing is shown.
- **Add-ons move with the app only (spec §6.2):** no code path downloads OMP, Clink or a font; the runtime folder comes from `resources\` of the new install, checked against `manifest.json` as today.
- **The one program run from outside `runtime\` (spec §5.10):** the installer the plugin verified (minisign, the pubkey in `tauri.conf.json`), from `<data>\updates\<file>` whose SHA-256 the app compares with the hash of the verified bytes before it starts it, or the plugin's own temp file inside `Update::install`; with `/S`; nothing else. Nothing from `latest.json` but the version and the signature reaches a command line.
- **IPC, capability, CSP:** 37 + `update_status` + `update_install` = **39** names in `command_names.rs`, the capability, `ipc.rs`'s counts (39 commands, 35 logging); `app_status` gains `updated: Option<Updated>`; `ExternalTarget` gains `ReleaseNotes { version: String }` whose URL is `https://github.com/luraselenehalo/cmd-studio/releases/tag/v<version>` for a `version` that parses as SemVer and nothing else; the capability grants no `updater:` permission; `apply-progress` stays the only event; the CSP of `config_guard.rs` is unchanged (the updater's requests are Rust's, not the window's).
- **The new crate.** `tauri-plugin-updater = "=2.13.3"` in `app/src-tauri/Cargo.toml` (and `semver = "=1.0.28"`); `Cargo.lock` gains the plugin's tree (reqwest 0.13 with rustls, minisign-verify, tempfile, infer, zip …) and is committed with Task 3. No npm package: `app/package.json` and `app/package-lock.json` do not change in 4b (the version bump of Task 9's Step 0 excepted).
- **Secrets:** agents never read, write, print or echo `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` or `UPSTREAM_WATCH_TOKEN`; workflows reference them as `${{ secrets.NAME }}` only, and never in a step that prints its environment; the spike's ephemeral key is generated inside the CI run, written only under `$RUNNER_TEMP`, and never uploaded (its `.pub` is).
- **The updater's endpoint** is `https://github.com/luraselenehalo/cmd-studio/releases/latest/download/latest.json`, which GitHub serves from the newest published, non-pre-release release; see "The endpoint" under Decisions for what that asks of the founder. `config_guard.rs` pins it, `windows.installMode: "quiet"`, and the absence of `dangerousInsecureTransportProtocol` in the committed config.
- **Bindings.** 4b changes exported types (`AppStatus`, `ExternalTarget`) and adds `UpdateState`, `UpdateStatus`, `Updated`. Regenerate by the manual steps only: delete the generated `.ts` files that change, then `cargo test -p cmdstudio-app --lib export_bindings` with `TS_RS_EXPORT_DIR` set to `<repo>\app\ui\src\bindings`. Never run `scripts/export-bindings.ps1`; never pass `-ExecutionPolicy`. CI's "Bindings are fresh" step is the gate.
- **The checks.** Each Rust task ends with `cargo fmt --all`, then `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features ts -- -D warnings`, and `cargo fmt --all -- --check`. Each UI task ends with `npm run typecheck` and `npm test`, then `TZ=UTC npm test` (Git Bash: `TZ=UTC npm test`; PowerShell: `$env:TZ='UTC'; npm test`), and, when it touches the bundle, `npm run build` and `bash scripts/check-dist.sh`, all in `<repo>\app`. Workflow files are checked with `python3 -c "import yaml,sys; yaml.safe_load(open(sys.argv[1]))" <file>` (if PyYAML is absent on this PC, say "parsed in CI") and by CI itself. PowerShell scripts of this plan are parse-checked with `[System.Management.Automation.Language.Parser]::ParseFile`; the upstream script additionally runs its own test in fixture mode (Task 7), which touches the network never and the repo never.
- **PC safety (the brief, binding).** The dev PC is the founder's own. Never run the built app, `cmdstudio.exe`, `cmdstudio-cli.exe`/`cmdstudio.com`, an installer, an uninstaller, a VM guest script, a Lua probe or a stand-in binary; never run `npx tauri dev`, Oh My Posh or Clink; never run a spike build. Run `cmd.exe` only with `/d`. Never touch this PC's AutoRun, fonts, Windows Terminal settings, `%LOCALAPPDATA%\ai.resincore.cmdstudio`, `%ProgramData%` or the user Path: every updater test runs over the fake machine and the fake updater; no test of this plan opens a network connection. Never read this PC's event log, registry or windows. Never pass `-ExecutionPolicy`. Never write a file outside the repo and the session scratchpad. Never set `CMDSTUDIO_SYSTEM_TESTS`, `CMDSTUDIO_OMP_TESTS` or `CMDSTUDIO_ALLOW_PARITY_SKIP`. Building the installer locally (`npm run tauri -- build` with the bundle on) makes the Tauri CLI download NSIS into `%LOCALAPPDATA%\tauri`: a download, which needs the founder's explicit permission; without it build with `--no-bundle` and let CI make the installer. Windows Application Control blocks (`os error 4551`) are rerun once and never worked around: mark the step "not run locally (WDAC); CI is the gate" and go on. Scripts written for a VM (`spikes/vm/guest/*.ps1`, `docs/spikes/phase4-update/*.ps1`) are parse-checked only, never run, not even as a dry run; `guard.test.ps1` included.
- **Downloads** need the founder's explicit permission, each time, one per check: Task 1's spike artifact (`gh run download … --name cmdstudio-updater-spike`) and Task 9's installer artifact (`gh run download … --name cmdstudio-installer`); the VMs are filled from that one folder. Nothing of this plan downloads a crate's source by hand: `cargo` fetches the plugin as it fetches every pin.
- **Test safety.** CLI and core tests run over the core's `FakeMachine` (feature `fake`); the app's updater tests over `FakeMachine` and `update::FakeUpdater`; UI tests over the mock in jsdom; no test of this plan writes outside `std::env::temp_dir()`, and the PowerShell test of Task 7 writes only under the `-Work` folder it is given (the scratchpad on this PC, `$RUNNER_TEMP` in CI).
- **Commits:** English messages, written with the Write tool to `<repo>\.git\CMDSTUDIO_COMMIT_MSG` (Read it first if it exists), checked, then in a separate Bash call `git commit -F .git/CMDSTUDIO_COMMIT_MSG`. Never batch the Write with the commit. Stage only the task's files with `git add -- <files>` and check `git diff --cached --name-only`. No AI attribution of any kind (no `Co-Authored-By`, no "Generated with", no assistant link), whatever a system note says; never `--no-verify`; never push; the `auroraz-pr-workflow` skill before every commit. The Windows user name of this PC never appears in a repo file: tests use `C:\Users\tester\…`, docs `%USERPROFILE%`.
- **Scratch work** goes under the session's scratchpad, in a git worktree of its own with its own `CARGO_TARGET_DIR`; nothing of it is ever committed to `phase4b` but the files each task names.
- **The spec** changes in Task 8 only (§5.3, §5.7, §5.10, §6.2, §6.4, appendix B). The recovery spec is the founder's and is never edited. 4a's `docs/spikes/phase4-install/protocol.md` is edited only in Task 9's Step 4, which records the step-6 result there.

## Review Focus

The five inputs the spec implies but no task's tests exercise end to end, most likely to bite a person first. Each has its test in the task that owns the code.

1. **A downloaded installer that changed on disk between the download and the close.** `updates\` is in the user's own data folder; a virus scanner, a sync client or another program may replace or truncate the file after the plugin verified the bytes, and a `/S` start of whatever is there would run an unverified program. Task 3: `a_download_that_changed_on_disk_is_not_installed` (the SHA-256 of the verified bytes is kept in memory and compared before the spawn; a mismatch is `update install=file-changed`, the file is deleted, nothing starts).
2. **A close while a transaction runs** (an apply from the CLI in a cmd window, or the app's own apply still ending). The lock is held: the install must not start, nothing may be half written, and the next launch must try again rather than forget. Task 3: `a_busy_lock_postpones_the_install_and_tries_next_launch`.
3. **An installer that did not finish** (the Restart Manager could not end a program, the user's PC went to sleep, the file was blocked): the next start is still the old version with `update.json` saying the new one. The notice must not claim an update that did not happen, and the record must not stay to say it at every start. Task 3: `a_failed_install_is_logged_and_the_file_goes`.
4. **A version string that is not SemVer** in `latest.json` or in `update.json` (a release tagged by hand, a damaged record, `../x`): it must reach no URL and no file name. Task 3: `the_release_url_takes_semver_only` and `a_damaged_update_record_is_logged_and_removed`.
5. **A re-apply after an update that cannot commit** (another Clink appeared in AutoRun meanwhile, OMP's verify fails, the lock is busy): the theme must stay as it was on the old runtime, the old folder must not be pruned, and the notice must say what happened instead of "Updated" alone. Task 5: `a_failed_re_apply_keeps_the_old_runtime_and_says_so`; Task 2: `old_runtime_folders_go_and_held_ones_stay`.

## Decisions the founder may change

Each is the architect's choice of 2026-11-04/05 where the brief left room; appendix B records them (Task 8). The report to the founder lists them one line each.

- **The install moment: at close, silent, no relaunch** (primary; Task 1's spike confirms or picks a fallback). The download happens in the background after `app_ready`; the installer runs when the window closes, if the lock is free; the next start shows the notice. The rejected candidate "before the window" would delay every start by a download. Fallback order under "The fallbacks".
- **Two install paths, no dead code.** The automatic path is the app's own spawn (`/S /UPDATE`, no relaunch); the user's "Update now" (notify mode's card, the Recovery page) is the plugin's `Update::install` (`/S /R /UPDATE /ARGS`, the app comes back by itself, which the click asked for). The outline's "both are written, one is dead code removed after the spike" is replaced by this split, since the source settles that the plugin's path relaunches.
- **The own spawn passes `/UPDATE` as well as `/S`.** With `$UpdateMode = 1` the installer keeps the user's shortcuts as they are (`CreateOrUpdateStartMenuShortcut` returns early), skips a WebView2 download, and would make an uninstaller started by it skip the cleanup; POSTINSTALL's rename and Recovery shortcut run regardless. U3 runs with it.
- **The endpoint.** GitHub's `releases/latest/download/latest.json` skips releases flagged "pre-release", so an app checking it would never see a v0.x flagged that way. Decision: **the endpoint is that fixed URL, and from the first release the founder publishes with updater assets on, releases are not flagged pre-release; the title and the notes keep saying "pre-release for testers".** Task 6 drops `--prerelease` from `release.yml:78` (the words stay). The alternative, if the founder wants the flag: a second, fixed release tagged `updater` whose one asset `latest.json` the workflow replaces (`gh release upload updater latest.json --clobber`) on every publish, and the endpoint `releases/download/updater/latest.json`; Task 6 names both and builds the one the founder picks.
- **An empty pubkey is a refused check, not a network request.** Until Task 6 the committed config holds `pubkey: ""`; `TauriUpdater::check` answers `UpdateError::Manifest` before the plugin is asked, so a dev build makes no request at every start.
- **`update.json` in the data folder**, written before the installer starts, read at the next start and deleted by `app_ready`; `--cleanup` knows it and `updates\`; `updates\` is emptied at every start before the check, so a postponed download is fetched again rather than trusted across runs.
- **The re-apply after an update runs at the next start, in the background, under `busy = "update_reapply"`** (a name of its own like `recovery_entry`, not an entry of `CHANGES_MACHINE`, which lists IPC commands), only when a theme is applied and `state.json`'s runtime versions differ from the new pins; the UI shows the busy wait it already has (recovery spec §6.3) with its own words; its log line `update reapply=ok|<code>`.
- **Old runtime folders are pruned best effort** after a successful re-apply and at every start: every `runtime\omp-*` and `runtime\clink-*` (and every `clink-*` directly in the user's ProgramData folder) whose name is not the pinned one and that is not one `state.json` names (the applied `omp_version`'s folder and the folder of the Clink exe AutoRun runs); a folder held open (a cmd window still has the DLL) stays until the next start.
- **The upstream-watch PR changes `scripts/fetch-resources.ps1` only**, and its body lists what a human finishes: `OMP_VERSION`/`CLINK_VERSION` in `crates/core/src/plan.rs`, `NERD_FONTS_VERSION` in `crates/core/src/resources.rs`, `app/ui/src/screens/credits.ts`, `docs/credits.md`, `.github/release-notes.md`, the README, and the parity test. CI then fails on the PR until those are done: that is the gate (spec §6.4: "never lower a test").
- **PRs from the workflow need a token that triggers CI:** `GITHUB_TOKEN`'s pushes and PRs do not run `pull_request` workflows; the founder's fine-grained PAT `UPSTREAM_WATCH_TOKEN` (contents: write, pull requests: write, this repo only) does. Without it the workflow still opens the PR (the repository setting "Allow GitHub Actions to create and approve pull requests" must be on) and the founder closes and reopens it to run CI (noted in the workflow).
- **No npm package.** The window never calls the plugin; `@tauri-apps/plugin-updater` is not installed. If the founder wants the JS API later, it is that package at the plugin's version, with `updater:default` in the capability, and a change to `ipc.rs`'s capability test.

## Implementation Gate (spec §0) for this plan

Before the task that uses each, the implementer opens the real thing and reads the shape, not the name; a mismatch stops the task and goes to the founder, and the spec is changed to the real thing.

| # | Confirm | Where | Used by |
|---|---|---|---|
| G-UPD | **Read on 2026-11-05 in 2.10.1** (the one version in this PC's registry); **reread in 2.13.3 by Task 3's implementer after `cargo fetch`**, file:line into the module doc of `update.rs`: (a) `Update::download` fetches the bytes, calls `verify_signature(&buffer, &self.signature, &self.config.pubkey)` and returns them without running anything (`src/updater.rs:652-715`); (b) `Update::install` on Windows → `install_inner`: `extract` (the bytes to a temp file), the NSIS arguments `nsis_args()` + `/UPDATE` + `/ARGS` + the app's own arguments + `installer_args`, `on_before_exit`, `ShellExecuteW(open, file, args)`, `std::process::exit(0)` (`:787-866`); `nsis_args()` for `quiet` is `/S /R` (`src/config.rs:36-47`); (c) `Updater::check` replaces `{{current_version}}`/`{{target}}`/`{{arch}}` in the endpoint, GETs it with `Accept: application/json`, parses `RemoteRelease`, and answers `None` unless `version_comparator` (default `release.version > current`) says so; a manifest that is not JSON or has no `windows-x86_64[-nsis]` target is an `Err` (`:386-566`); (d) `UpdaterBuilder::version_comparator`, `on_before_exit`, `installer_args`; `UpdaterExt::updater_builder()` on `AppHandle` (`src/lib.rs:33-90`); (e) `Config` deserializes `pubkey: String` (required), `endpoints`, `windows.installMode` (`quiet`/`passive`/`basicUi`), `dangerousInsecureTransportProtocol` (`src/config.rs:94-164`); `validate_endpoints` refuses `http:` in a release build unless that flag is set; (f) the plugin's `Error` variants (`src/error.rs`) for `update::classify` | the cargo registry after `cargo fetch` | Task 3 |
| G-NSI-U | **Read on 2026-11-05** from the `installer.nsi` template embedded in `app/node_modules/@tauri-apps/cli-win32-x64-msvc/cli.win32-x64-msvc.node` (line numbers are `sed -n`'s over the binary's text lines): `.onInit` parses `/P`, `/NS`, `/UPDATE` (l.27171-27185); `PageLeaveReinstall` l.26995-27075 (`$UpdateMode = 1 → reinst_done` at l.27004-27006; the `/UPDATE` and `/P` appends l.27042-27043 on the `${Else}` branch of `reinst_uninstall`); `PageReinstall`'s radios and their default l.26955-26990 (the first radio checked unless the user chose the second before); `Section Install` l.27340-27362 (`PREINSTALL`, `CheckIfAppIsRunning`, the files); `.onInstSuccess` l.27442-27452 (`/R` in silent/passive mode → `nsis_tauri_utils::RunAsUser "$INSTDIR\cmdstudio.exe" "<the /ARGS text>"`); `CheckIfAppIsRunning` l.27836-27888 (`IfSilent kill_` → `RmShutdown` force); `CreateOrUpdateStartMenuShortcut` returns early when `$UpdateMode = 1` (l.27638-27643); `Section Uninstall` l.27476-27600 (`PREUNINSTALL` first, the `$UpdateMode <> 1` blocks) | the CLI binary's embedded template | Tasks 1, 3, 9 |
| G-SIG | The Tauri CLI's `signer generate` takes `-w/--write-keys <path>` (writes the private key there and the public key at `<path>.pub`), `-p/--password`, `-f/--force`, `--ci` ("Skip prompting for values": no password); the bundler reads `TAURI_SIGNING_PRIVATE_KEY` (a path or the key's text) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (the strings are in the CLI binary; `write-keys`, "Skip prompting for values", both variable names). With `bundle.createUpdaterArtifacts: true` the bundler signs the NSIS installer and writes `<installer>.sig` beside it (the base64 minisign signature text that `latest.json`'s `signature` carries verbatim) and fails when the key variable is missing. Confirmed by the first spike build's log and listing | the CLI binary; Task 1's CI run | Tasks 1, 6 |
| G-IDX | **Read on 2026-11-05** from the crates.io sparse index (`index.crates.io/ta/ur/tauri-plugin-updater`): 2.13.0–2.13.3 require tauri `^2.12` and tauri-plugin `^2.7`/`^2.7.1`; 2.13.3 is the newest 2.13.x; 2.14.0/2.14.1 exist with the same tauri requirement; 2.10.x–2.12.0 require tauri `^2.10`. Tauri stays at 2.12.4. If `cargo` cannot resolve `=2.13.3` with the pinned `tauri =2.12.4` (a `tauri-utils` conflict through `tauri-plugin ^2.7.1`), pin `=2.12.0` instead and record it (fallback F-PIN) | `cargo fetch` in Task 3 Step 0 | Task 3 |
| G-GH | `releases/latest/download/latest.json` is served from the newest published release that is not a draft and not flagged pre-release (GitHub's "latest" rule); `gh release create` without `--prerelease` makes a release that qualifies once published; `gh release upload <tag> <file> --clobber` replaces an asset. `secrets: inherit` on a `workflow_call` job passes the caller's secrets to the called workflow | GitHub's docs for `gh release` and reusable workflows | Tasks 6, 7, 9 |
| G-VBOX | VirtualBox NAT guests reach the host at `10.0.2.2`; `VBoxManage controlvm <vm> setlinkstate1 off|on` disconnects and reconnects the NIC; the host's firewall must let `python3` listen on 8765 (a founder's one-time allow when Windows asks) | `spikes/vm/README.md`, VirtualBox's manual | Tasks 1, 9 |

## Spike results the later tasks depend on (Task 1)

| # | What the spike shows (VM-A, or VM-B when no VM with Windows Terminal starts; a spike build with an ephemeral key, `latest.json` served from the host at `http://10.0.2.2:8765/`) | Tasks 2–9 assume | Used by | If not |
|---|---|---|---|---|
| U1 | `Update::download()` of tauri-plugin-updater 2.13.3 verifies the minisign signature and returns the bytes without running anything; the app writes them to `updates\cmdstudio-0.1.0-spike.2-setup.exe` and the log has `update downloaded=0.1.0-spike.2` | yes | Task 3 | the updater is dropped from v0.x and the founder decides (no fallback: the plugin's contract) |
| U2 | "Update now" (notify mode's card) through `Update::install` in `installMode: "quiet"`: the window disappears at once (the plugin's `exit(0)`), the installer runs with `/S /R /UPDATE /ARGS` and no window, no uninstaller runs (G-NSI-U), the app comes back by itself (`/R`) within the measured time, AutoRun and the theme are untouched, `cmdstudio.exe` is spike.2, and the returned app shows "Updated to v0.1.0-spike.2" | the relaunch happens; the cleanup never runs | Task 3, Task 4 | if the cleanup ran (AutoRun lost CMD Studio's part): stop, G-NSI-U's reading is wrong (back to 4a); if no relaunch: the "now" path keeps working and the card's words drop "opens again by itself" |
| U3 | The app's own spawn of the verified file with `/S /UPDATE` at `RunEvent::Exit` (automatic mode) installs without a window and without relaunching the app; `cmdstudio.exe`'s file is replaced although the exiting process still runs for a moment (the template's `CheckIfAppIsRunning` ends it in silent mode); `update.json` is there; AutoRun untouched; the time from the close to the new exe is measured | yes: the primary design | Task 3 | F2 |
| U4 | After U3 the user starts the new app: the notice "Updated to v0.1.0-spike.2" shows with its link ("What changed" opens the browser at `…/releases/tag/v0.1.0-spike.2`), and, the pins being equal between the two spike builds, no re-apply runs (the log has `update installed to=0.1.0-spike.2` and no `update reapply=` line); `update.json` is gone after the home page drew | the notice shows; no re-apply when the pins are equal | Tasks 3, 5 | a notice that does not show is a Task 3 bug, not a fallback |
| U5 | `Updater::check()` against a `latest.json` whose `version` equals the current one logs `update check=none`; against one that is not JSON `update check=manifest`; nothing shows on screen either time | yes | Task 3 | `version_comparator` closure set explicitly (it is, Task 3) |
| U6 | A by-hand install of spike.2 over spike.1 (interactive) shows the "already installed" page; choosing "Do not uninstall" installs over the files and keeps AutoRun and the theme; the page's exact words are recorded for the release notes. (The first radio's path, the full cleanup, is 4a's 9b and is not repeated.) | a by-hand upgrade can keep the theme through the second radio | Task 6 (the notes), Task 8 | the notes say "let CMD Studio update itself" only |
| U7 | `UpdateMode::Off`: no request reaches the host's server log and the app's log has `update mode=off`; offline (the NIC off, automatic mode): `update check=offline` and nothing on screen | yes | Tasks 3, 9 | a request in Off mode is a Task 3 bug |

### The fallbacks

- **F2 (U3: the own spawn fails to replace the running exe, or shows a window):** "at launch when idle": the download finishes, and when `app_status.busy` is null, no editor has an unsaved change (the UI answers a third command `update_can_install`) and the lock is free, the app writes `update.json`, `end_run`, and calls `Update::install` (the relaunch of U2 is then wanted); the window disappears and comes back within the installer's time; appendix B records the moment as "ตอนเปิด เมื่อว่าง". In Task 3 the at-exit branch is deleted and `update_install` is called from `update_check_and_download`'s end when `update_can_install` answers yes.
- **F3 (neither the primary nor F2 holds):** "ask to restart": a notify-style card "v<X> is ready: restart CMD Studio to install it" with a button that calls `update_install`; automatic mode behaves as notify for the install step only (the download stays automatic). The founder is told.
- **F-PIN (G-IDX):** `tauri-plugin-updater = "=2.12.0"` and the same reading of its source (its `install_inner` is the same code).
- **F-VER (Task 1):** the bundler refuses `0.1.0-spike.1` as a version (NSIS takes SemVer with a pre-release; if `makensis` or the bundler's version check does not), the spike builds use `0.1.1` and `0.1.2`, the `latest.json` and the protocol say those, and the spike.1 installer is a downgrade for the installed 0.1.0 (allowed: `allowDowngrades` is Tauri's default).
- **F1 is withdrawn** (the source settles it; see "What the sources settled").

## Phase 4a's deferred minors, folded or left

The brief names three 4a minors as candidates. Folded, with a test, where 4b edits the file anyway:

- **M6 (the version grep anchored to `[workspace.package]`): folded.** Task 1 edits `ci.yml`'s listing step and Task 6 edits `release.yml`'s version step; both greps become an `awk` that reads `version = ` only inside the `[workspace.package]` table, and `config_guard::the_workflows_read_the_version_under_workspace_package` pins that text in both files (Task 6).
- **M9 / Task 1's deferred (`start_app_beside` inherits stdio): left.** 4b does not edit `console.rs`. The updater's own spawn (`update::install_quiet`, Task 3) sets its child's stdio to null from the start, so the new code does not repeat the slip.
- **M10 (the ARM64 refusal's empty `$INSTDIR`): left.** `hooks.nsh` is not edited in 4b (Global Constraints).

Every other 4a minor stays where the rulings file lists it.

## File map

- `app/src-tauri/src/update.rs` — the `Updater` trait, `Available`, `UpdateError`, `TauriUpdater` (the plugin), `FakeUpdater` (tests), `install_quiet(file)` (the own spawn), `start_check` (the thread after `app_ready`), `newer` (the version comparator) (Task 3).
- `app/src-tauri/src/backend/update.rs` — `UpdateRecord`, `UpdateState`, `UpdateStatus`, `Updated`, `UPDATE_REAPPLY_BUSY`, `Backend::{update_check_and_download, update_status, update_install, update_at_exit, update_after_start, update_record_consumed, updated}` (Task 3); `Backend::{update_needs_reapply, update_reapply}` (Task 5).
- `crates/core/src/recovery.rs` — `Paths::update_file()`, `Paths::updates_dir()` (Task 2).
- `crates/cli/src/lib.rs` — `cmd_studios_own_file` and `put_there_by_cmd_studio` know `update.json`, `updates\` and the temp name (Task 2).
- `crates/core/src/plan.rs` — `runtime_changed(&State) -> bool` (Task 2); `crates/core/src/cleanup.rs` — `runtime_dirs_named_by`, `prune_old_runtimes` (Task 2).
- `app/src-tauri/src/open.rs` — `ExternalTarget::ReleaseNotes { version }`, `release_url`, `Place::Url(String)` (Task 3).
- `app/src-tauri/src/ipc.rs`, `command_names.rs`, `capabilities/default.json`, `backend/run.rs` (`AppStatus.updated`) — the two commands (Task 4).
- `app/src-tauri/src/lib.rs` — the plugin registration, `TauriUpdater` in Tauri's state, `update_after_start` after the record, the exit hook (Task 3); the re-apply thread (Task 5).
- `app/src-tauri/src/config_guard.rs` — `plugins.updater` pinned (Task 3); `createUpdaterArtifacts` and the pubkey's shape (Task 6); the workflows' version grep (Task 6).
- `app/src-tauri/tauri.conf.json` — `plugins.updater` (`pubkey: ""`, `endpoints`, `windows.installMode: "quiet"`) (Task 3; the pubkey in Task 6); `app/src-tauri/tauri.bundle.conf.json` — `bundle.createUpdaterArtifacts: true` (Task 6).
- `app/src-tauri/Cargo.toml`, `Cargo.lock` — the plugin and `semver` (Task 3).
- `app/ui/src/ipc/types.ts`, `tauri.ts`, `mock.ts`, `work.ts`, `app/ui/src/App.tsx` (the "Updated to" card in `StartNotices`), `app/ui/src/state.tsx`, `app/ui/src/screens/Home.tsx` (the notify card), `Settings.tsx` (the note's words), `Recovery.tsx` (the "Update now" line), `Advanced.tsx` ("Last checked"), `i18n/en.ts`, `th.ts`, `i18n.test.ts`, `App.test.tsx`, `ipc/mock.test.ts`, `ipc/work.test.ts`, the bindings (Task 4).
- `.github/workflows/ci.yml` — the spike input and job steps, the signing env, the `.sig` in the artifact, the version grep (Tasks 1, 6); `.github/workflows/release.yml` — `latest.json`, the `.sig`, no `--prerelease`, `secrets: inherit` (Task 6); `.github/release-notes.md` — how to upgrade (Task 6); `.github/workflows/upstream-watch.yml`, `scripts/upstream-watch.ps1`, `scripts/upstream-watch.test.ps1` (Task 7).
- `docs/spikes/phase4-update/protocol.md` (part A the spike, part B the update check), `docs/spikes/phase4-update/results.md` (the spike's results and the decision), `docs/spikes/phase4-update/helpers.ps1` (host), `spikes/vm/guest/update-check.ps1`, `spikes/vm/guest/guard.test.ps1` (Tasks 1, 9).
- `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` — §5.3, §5.7, §5.10, §6.2, §6.4, appendix B (Task 8).

---
### Task 1: The updater spike (the build in CI, the run on a VM)

**Needs no founder step.** Runs after Task 3 is committed: the spike is the first consumer of Task 3's code, built twice at two versions with an ephemeral key, and run on a VM against a `latest.json` the host serves. Its result (U1–U7) confirms the primary install moment or picks a fallback, and settles the by-hand upgrade's words for Task 6's release notes. The CI half is the implementer's; the VM half is the controller's.

**Files:**
- Modify: `.github/workflows/ci.yml` (the `workflow_dispatch` input `updater_spike`; two steps at the end of the `app` job; the listing step's version grep anchored, M6)
- Create: `docs/spikes/phase4-update/protocol.md` (part A; Task 9 adds part B)
- Create: `docs/spikes/phase4-update/helpers.ps1` (host)
- Create: `docs/spikes/phase4-update/results.md` (the template; the controller fills it)
- Create: `spikes/vm/guest/update-check.ps1`
- Modify: `spikes/vm/guest/guard.test.ps1` (`'update-check.ps1' = @('-Snapshot', 'before')`)

**Interfaces:**
- Consumes: Task 3's `update.rs` and `backend/update.rs` (the log lines `update check=…`, `update available=…`, `update downloaded=…`, `update installing=… how=exit|now`, `update installed to=…`, `update mode=off`; the file `updates\cmdstudio-<version>-setup.exe`; `update.json`), Task 4's cards (the spike runs after Task 4 when the controller's VM time comes later; U2 needs the notify card, so the VM run waits for Task 4 at the latest), `settings.json`'s `updates` field, 4a's `docs/spikes/phase4-install/helpers.ps1` (`Install-Phase4Setup`, `Start-Phase4Install`, `Start-Phase4App`, `Close-Phase4App`, `Close-Phase4Cmd`, `Get-Phase4Installed`, `Invoke-Phase4Control`) and `docs/spikes/phase3-vm-check/helpers.ps1` (`Start-CleanVm`, `Send-Key`, `Get-GuestAutoRun`, `Invoke-GuestButton`), `spikes/vm/vm.ps1` (`Invoke-GuestPs`, `Invoke-GuestDesktop`, `Copy-ToGuest`, `Save-Shot`).
- Produces: the CI artifact `cmdstudio-updater-spike` (`CMD Studio_0.1.0-spike.1_x64-setup.exe`, `CMD Studio_0.1.0-spike.2_x64-setup.exe`, their `.sig` files, `latest.json`, `latest.same.json`, `latest.bad.json`, `spike.key.pub`); `update-check.ps1 -Snapshot <label> | -SetMode auto|notify|off | -WaitForVersion <v> [-WaitSeconds n] | -Log | -StartApp`; the host helpers `Install-Phase4bSetup`, `Start-Phase4bInstall`, `Get-Phase4bSnapshot`, `Set-Phase4bMode`, `Wait-Phase4bVersion`, `Get-Phase4bLog`, `Start-Phase4bServer`, `Stop-Phase4bServer`, `Set-Phase4bLatest`, `Set-Phase4bNic`; `results.md` with the decision (primary, F2 or F3) that Task 3's last step and Task 8 read.

- [ ] **Step 1: The spike input and the two steps in `ci.yml`**

In `.github/workflows/ci.yml`, the `on:` block's `workflow_dispatch:` (line 10) gains inputs:

```yaml
  workflow_dispatch:
    inputs:
      # Phase 4b Task 1: two spike installers signed with an ephemeral key, and a latest.json for
      # the VM. Off by default; a push or a pull request never builds them.
      updater_spike:
        description: 'Also build the updater spike (two versions, an ephemeral key, latest.json)'
        type: boolean
        default: false
```

In the `app` job, after the last step (the `cmdstudio-exe` upload), add:

```yaml
      # Phase 4b Task 1's spike (docs/spikes/phase4-update/protocol.md part A), only on a
      # workflow_dispatch with updater_spike: an ephemeral minisign key made here under
      # $RUNNER_TEMP and never uploaded (its .pub is), two builds of this same code at the
      # versions 0.1.0-spike.1 and 0.1.0-spike.2, each signed, with the updater pointed at the
      # VM's host (http://10.0.2.2:8765 and dangerousInsecureTransportProtocol, which the
      # committed config forbids: config_guard), and a latest.json that offers spike.2, one
      # that offers spike.1 again (U5: equal), and one that is not JSON (U5: malformed). jq
      # merges the bundle file with the override (`*` is jq's recursive merge), so one --config
      # is passed as always. Mixed path separators from $RUNNER_TEMP are fine in Git Bash.
      - name: Updater spike builds (ephemeral key, two versions, latest.json)
        if: github.event_name == 'workflow_dispatch' && inputs.updater_spike
        shell: bash
        working-directory: app
        env:
          TAURI_SIGNING_PRIVATE_KEY: ${{ runner.temp }}/spike.key
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ''
        run: |
          npm run tauri -- signer generate -w "$RUNNER_TEMP/spike.key" --ci
          pub="$(tr -d '\r\n' < "$RUNNER_TEMP/spike.key.pub")"
          test -n "$pub"
          mkdir -p ../spike
          for n in 1 2; do
            v="0.1.0-spike.$n"
            jq -n --arg v "$v" --arg pub "$pub" '{version: $v, plugins: {updater: {pubkey: $pub, endpoints: ["http://10.0.2.2:8765/latest.json"], dangerousInsecureTransportProtocol: true, windows: {installMode: "quiet"}}}, bundle: {createUpdaterArtifacts: true}}' > "$RUNNER_TEMP/spike$n.override.json"
            jq -s '.[0] * .[1]' src-tauri/tauri.bundle.conf.json "$RUNNER_TEMP/spike$n.override.json" > "$RUNNER_TEMP/spike$n.conf.json"
            npm run tauri -- build --config "$RUNNER_TEMP/spike$n.conf.json"
            cp "../target/release/bundle/nsis/CMD Studio_${v}_x64-setup.exe" "../target/release/bundle/nsis/CMD Studio_${v}_x64-setup.exe.sig" ../spike/
          done
          when="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
          sig2="$(tr -d '\r\n' < "../spike/CMD Studio_0.1.0-spike.2_x64-setup.exe.sig")"
          sig1="$(tr -d '\r\n' < "../spike/CMD Studio_0.1.0-spike.1_x64-setup.exe.sig")"
          jq -n --arg sig "$sig2" --arg when "$when" '{version: "0.1.0-spike.2", notes: "the updater spike", pub_date: $when, platforms: {"windows-x86_64": {signature: $sig, url: "http://10.0.2.2:8765/CMD%20Studio_0.1.0-spike.2_x64-setup.exe"}}}' > ../spike/latest.json
          jq -n --arg sig "$sig1" --arg when "$when" '{version: "0.1.0-spike.1", notes: "the updater spike", pub_date: $when, platforms: {"windows-x86_64": {signature: $sig, url: "http://10.0.2.2:8765/CMD%20Studio_0.1.0-spike.1_x64-setup.exe"}}}' > ../spike/latest.same.json
          printf '{ "version": "0.1.0-spike.2", not json' > ../spike/latest.bad.json
          cp "$RUNNER_TEMP/spike.key.pub" ../spike/
          test ! -e ../spike/spike.key
          ls -l ../spike
      - uses: actions/upload-artifact@v4
        if: github.event_name == 'workflow_dispatch' && inputs.updater_spike
        with:
          name: cmdstudio-updater-spike
          path: spike
          if-no-files-found: error
```

`inputs.updater_spike` is empty (false) on `push`, `pull_request` and `workflow_call`, so the steps are skipped there. If the bundler refuses the empty password variable for a key made with `--ci` (the log says so), remove the `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` line; if it refuses the pre-release versions, F-VER. The `.sig` content is one base64 line; `tr -d '\r\n'` keeps `latest.json` to one string.

M6 in the same file: in the step "The installer holds both exes, the resources and the licenses", replace the line

```bash
          version="$(grep -m1 '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')"
```

with

```bash
          # The one version: the line under [workspace.package] and no other (4a's M6).
          version="$(awk '/^\[workspace\.package\]/{t=1;next} /^\[/{t=0} t && /^version = /{gsub(/^version = "|"$/,""); print; exit}' Cargo.toml)"
          test -n "$version"
```

(The same two lines go into `release.yml` in Task 6, which pins both texts in `config_guard`.)

- [ ] **Step 2: Parse `ci.yml`**

Run: `python3 -c "import yaml,sys; yaml.safe_load(open(sys.argv[1]))" .github/workflows/ci.yml`
Expected: no output (parsed). If PyYAML is absent, say "parsed in CI".

- [ ] **Step 3: `update-check.ps1`**

```powershell
# Runs inside a test VM (plan 4b Tasks 1 and 9): what an update of CMD Studio must leave, read and
# printed. -Snapshot <label> writes under -Out and prints: the AutoRun value (type and bytes), the
# installed version (DisplayVersion of the uninstall key, which the installer writes from its own
# version string, and cmdstudio.exe's ProductVersion and FileVersion), the install folder's
# entries, the Start-menu folder's shortcuts, the user Path's entries that name CMD Studio, the
# data folder's update.json (its text or ABSENT) and updates\ (its entries or ABSENT), the `update `
# lines of logs\cmdstudio.log, and the cmdstudio processes. -SetMode auto|notify|off edits
# settings.json's "updates" field (the app must be closed; a missing file is made with that field
# alone, the rest being the app's defaults). -WaitForVersion <v> polls DisplayVersion up to
# -WaitSeconds and prints the seconds it took, or TIMEOUT. -Log prints the log's `update ` lines.
# -StartApp opens the Start-menu shortcut "CMD Studio.lnk" as a click does. Run on the user's
# desktop (Invoke-GuestDesktop): HKCU and the environment are the signed-in user's only there.
param(
  [ValidatePattern('^[a-z0-9]*$')] [string] $Snapshot = '',
  [ValidateSet('', 'auto', 'notify', 'off')] [string] $SetMode = '',
  [string] $WaitForVersion = '', [int] $WaitSeconds = 180,
  [switch] $Log, [switch] $StartApp,
  [string] $Out = 'C:\cmdstudio\out\phase4b'
)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script reads the user's registry and starts programs" }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$local = $env:LOCALAPPDATA
$data = Join-Path $local 'ai.resincore.cmdstudio'
$inst = Join-Path $local 'CMD Studio'
$sm = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\CMD Studio'
$uninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\CMD Studio'

function RegRaw([string] $key, [string] $name) {
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
function RegText([string] $key, [string] $name) {
  $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($key)
  if ($null -eq $k) { return 'KEY ABSENT' }
  try { $v = $k.GetValue($name, $null); if ($null -eq $v) { 'ABSENT' } else { "$v" } } finally { $k.Close() }
}
function Installed {
  $exe = Join-Path $inst 'cmdstudio.exe'
  $display = RegText $uninstallKey 'DisplayVersion'
  if (-not (Test-Path -LiteralPath $exe)) { return "DisplayVersion=$display exe=ABSENT" }
  $vi = (Get-Item -LiteralPath $exe).VersionInfo
  "DisplayVersion=$display ProductVersion=$($vi.ProductVersion) FileVersion=$($vi.FileVersion)"
}
function Entries([string] $dir) {
  if (-not (Test-Path -LiteralPath $dir)) { return 'ABSENT' }
  (Get-ChildItem -LiteralPath $dir -Force | Sort-Object Name | ForEach-Object { if ($_.PSIsContainer) { "$($_.Name)\" } else { "$($_.Name) $($_.Length)" } }) -join "`n"
}
function Shortcuts {
  if (-not (Test-Path -LiteralPath $sm)) { return 'ABSENT' }
  $sh = New-Object -ComObject WScript.Shell
  (Get-ChildItem -LiteralPath $sm -Filter *.lnk | Sort-Object Name | ForEach-Object { $l = $sh.CreateShortcut($_.FullName); "$($_.Name) -> $($l.TargetPath) [$($l.Arguments)]" }) -join "`n"
}
function PathEntries {
  $v = RegText 'Environment' 'Path'
  if ($v -eq 'ABSENT' -or $v -eq 'KEY ABSENT') { return $v }
  $ours = @($v -split ';' | Where-Object { $_ -like '*\CMD Studio' -or $_ -like '*\CMD Studio\' })
  "$($ours.Count) entry(ies): " + ($ours -join ' | ')
}
function UpdateLines {
  $log = Join-Path $data 'logs\cmdstudio.log'
  if (-not (Test-Path -LiteralPath $log)) { return 'LOG ABSENT' }
  $lines = @(Get-Content -LiteralPath $log | Where-Object { $_ -match ' update ' })
  if ($lines.Count -eq 0) { 'no update lines' } else { $lines -join "`n" }
}
function TextOrAbsent([string] $file) {
  if (Test-Path -LiteralPath $file) { [IO.File]::ReadAllText($file) } else { 'ABSENT' }
}
function State {
  [ordered]@{
    autorun    = RegRaw 'Software\Microsoft\Command Processor' 'AutoRun'
    installed  = Installed
    install    = Entries $inst
    shortcuts  = Shortcuts
    path       = PathEntries
    updateJson = TextOrAbsent (Join-Path $data 'update.json')
    updates    = Entries (Join-Path $data 'updates')
    log        = UpdateLines
    processes  = (@(Get-Process cmdstudio -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Id)" }) -join ',')
  }
}
if ($Snapshot) {
  $s = State
  foreach ($k in $s.Keys) { [IO.File]::WriteAllText((Join-Path $Out "$Snapshot-$k.txt"), "$($s[$k])", (New-Object Text.UTF8Encoding $false)) }
  "snapshot $Snapshot written"
  foreach ($k in $s.Keys) { "${k}: $($s[$k] -replace "`n", ' | ')" }
}
if ($SetMode) {
  $file = Join-Path $data 'settings.json'
  if (@(Get-Process cmdstudio -ErrorAction SilentlyContinue).Count -gt 0) { throw 'close the app before -SetMode: it holds its settings in memory' }
  if (Test-Path -LiteralPath $file) {
    $json = [IO.File]::ReadAllText($file) | ConvertFrom-Json
    $json.updates = $SetMode
  } else {
    New-Item -ItemType Directory -Force -Path $data | Out-Null
    $json = [pscustomobject]@{ version = 1; updates = $SetMode }
  }
  [IO.File]::WriteAllText($file, ($json | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))
  "settings.json updates=$SetMode"
}
if ($WaitForVersion) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.Elapsed.TotalSeconds -lt $WaitSeconds) {
    if ((RegText $uninstallKey 'DisplayVersion') -eq $WaitForVersion) { "version $WaitForVersion after $([int]$sw.Elapsed.TotalSeconds) s; $(Installed)"; return }
    Start-Sleep -Seconds 2
  }
  "TIMEOUT after $WaitSeconds s; $(Installed)"
}
if ($Log) { UpdateLines }
if ($StartApp) {
  $lnk = Join-Path $sm 'CMD Studio.lnk'
  if (-not (Test-Path -LiteralPath $lnk)) { throw "no shortcut at $lnk" }
  Start-Process -FilePath $lnk | Out-Null
  Start-Sleep -Seconds 10
  "cmdstudio processes 10 s after the shortcut: $(@(Get-Process cmdstudio -ErrorAction SilentlyContinue).Count)"
}
```

Parse-check on this PC: `[System.Management.Automation.Language.Parser]::ParseFile('<repo>\spikes\vm\guest\update-check.ps1', [ref]$null, [ref]$errors); $errors.Count` must print `0`. Add `'update-check.ps1' = @('-Snapshot', 'before')` to `guard.test.ps1`'s `$changers` (the guard is the script's first statement after `$ErrorActionPreference`, as the test's regex expects). The guard total is computed by the script itself (`2 * $changers.Count + 3`).

- [ ] **Step 4: `helpers.ps1` for the host**

```powershell
# Host-side helpers for the phase 4b spike and update check (protocol.md). Dot-source after
# spikes\vm\vm.ps1, docs\spikes\phase3-vm-check\helpers.ps1 and docs\spikes\phase4-install\helpers.ps1,
# in Windows PowerShell 5.1 on the host, from the repo root. Only the CMD Studio test VMs get
# anything from them; this PC's installer is never run, and nothing here reads this PC's registry,
# fonts or Windows Terminal. Guest paths are built from the VM's user name, never from this host's
# environment (a `$env:` in a double-quoted string would expand on the host PC).
$P4B = @{
  Setup = 'C:\cmdstudio\phase4b\setup'     # the spike's two installers, or the update check's one
  Guest = 'C:\cmdstudio\spikes\vm\guest'   # every script of spikes\vm\guest
  Out   = 'C:\cmdstudio\out\phase4b'       # the snapshots
  Port  = 8765
}
# The scripts and the downloaded CI artifact into the VM (every *-setup.exe in $Artifact), each
# installer's hash compared with the host's. 4a's $P4.Setup is pointed at $P4B.Setup afterwards, so
# that 4a's Start-Phase4Install (-Setup <folder>: one installer there) keeps working for one file.
function Install-Phase4bSetup($Vm, [string] $User, [string] $Artifact) {
  $repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
  Invoke-GuestPs $Vm "New-Item -ItemType Directory -Force -Path '$($P4B.Out)', '$($P4B.Setup)' | Out-Null" $User 60 | Out-Null
  $guest = @(Get-ChildItem -LiteralPath (Join-Path $repo 'spikes\vm\guest') -File | ForEach-Object FullName)
  Copy-ToGuest $Vm $guest ($P4B.Guest + '\') $User | Out-Null
  $exes = @(Get-ChildItem -LiteralPath $Artifact -Filter '*-setup.exe')
  if ($exes.Count -eq 0) { throw "no *-setup.exe in $Artifact" }
  Copy-ToGuest $Vm @($exes | ForEach-Object FullName) ($P4B.Setup + '\') $User | Out-Null
  foreach ($exe in $exes) {
    $hostHash = (Get-FileHash -LiteralPath $exe.FullName -Algorithm SHA256).Hash
    $inVm = "$(Invoke-GuestPs $Vm "(Get-FileHash -LiteralPath '$($P4B.Setup)\$($exe.Name)' -Algorithm SHA256).Hash" $User 180)".Trim()
    Write-Host ('installer: ' + $exe.Name + ' ' + $exe.Length + ' bytes; same file in the VM: ' + ($inVm -eq $hostHash))
  }
  $exes | ForEach-Object Name
}
# One of the spike's installers (-Which 1 or 2), on the desktop; -Silent runs it with /S and waits.
function Start-Phase4bInstall($Vm, [int] $Which, [string] $User = 'tester', [switch] $Silent) {
  $file = "$($P4B.Setup)\CMD Studio_0.1.0-spike.$Which" + '_x64-setup.exe'
  if ($Silent) { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" "-InstallSilent -Setup '$file'" $User 600 }
  else { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" "-Install -Setup '$file'" $User 120 }
}
function Get-Phase4bSnapshot($Vm, [string] $Which, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-check.ps1" "-Snapshot $Which" $User 300 }
function Set-Phase4bMode($Vm, [ValidateSet('auto', 'notify', 'off')] [string] $Mode, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-check.ps1" "-SetMode $Mode" $User 60 }
function Wait-Phase4bVersion($Vm, [string] $Version, [string] $User = 'tester', [int] $Seconds = 180) { Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-check.ps1" "-WaitForVersion $Version -WaitSeconds $Seconds" $User ($Seconds + 60) }
function Get-Phase4bLog($Vm, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-check.ps1" '-Log' $User 60 }
function Start-Phase4bApp($Vm, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-check.ps1" '-StartApp' $User 120 }

# The host's web server over the spike artifact's folder, for the VM's updater (10.0.2.2 is the
# host on VirtualBox NAT). Its request log goes to $Dir\server.log: U7's "no request in Off mode"
# is read there. Windows asks once to let python listen: the founder allows it.
function Start-Phase4bServer([string] $Dir) {
  $log = Join-Path $Dir 'server.log'
  $p = Start-Process -FilePath python -ArgumentList @('-m', 'http.server', "$($P4B.Port)", '--bind', '0.0.0.0', '--directory', $Dir) -PassThru -WindowStyle Hidden -RedirectStandardError $log
  "server pid $($p.Id) on port $($P4B.Port), log $log"
  $p.Id
}
function Stop-Phase4bServer([int] $ServerPid) { Stop-Process -Id $ServerPid -Force; "server $ServerPid stopped" }
# Which manifest the VM gets as latest.json: 'latest' (spike.2), 'same' (spike.1 again, U5) or
# 'bad' (not JSON, U5). The artifact's latest.json is kept as latest.real.json the first time.
function Set-Phase4bLatest([string] $Dir, [ValidateSet('latest', 'same', 'bad')] [string] $Which) {
  $real = Join-Path $Dir 'latest.real.json'
  if (-not (Test-Path -LiteralPath $real)) { Copy-Item -LiteralPath (Join-Path $Dir 'latest.json') -Destination $real }
  $from = switch ($Which) { 'latest' { $real } 'same' { Join-Path $Dir 'latest.same.json' } 'bad' { Join-Path $Dir 'latest.bad.json' } }
  Copy-Item -LiteralPath $from -Destination (Join-Path $Dir 'latest.json') -Force
  "latest.json is now the '$Which' manifest"
}
# The VM's network cable, for U7's offline run: VBoxManage's link state of adapter 1.
function Set-Phase4bNic([string] $VmName, [ValidateSet('on', 'off')] [string] $State) {
  & (Join-Path $env:ProgramFiles 'Oracle\VirtualBox\VBoxManage.exe') controlvm $VmName setlinkstate1 $State
  "nic of $VmName $State"
}
```

Parse-check it as the guest script. `$P4` is 4a's table (dot-sourced before this file); `Start-Phase4bInstall` passes the file path quoted, since 4a's `install-check.ps1 -Setup` takes a file or a folder and the name has spaces.

- [ ] **Step 5: `protocol.md`, part A**

```markdown
# Phase 4b — the updater spike (part A, Task 1) and the update VM check (part B, Task 9)

Both halves are the controller's. Nothing here runs on the dev PC, not even as a dry run; every guest script refuses a machine whose name does not start with `CMDSTUDIO-`. cmd runs without `/d` inside the VMs. Do not repair a VM: record the step and go on. Guest paths are written as `C:\Users\$u\…` on the host, never `$env:…`. The words are `en.ts`'s (`th.ts`'s on VM-C once Thai is chosen). The VMs: `spikes/vm/README.md`; **VM-A and VM-C need a powered-off `clean` snapshot** (their saved-state `clean` fails to start with HGCM `VERR_ACCESS_DENIED`, 4a's finding): the founder takes one before the run; VM-B (Windows 10, no Windows Terminal) is the fallback for every step that does not need Windows Terminal, which is every step of part A and every step of part B but the failed-cleanup step.

## Part A: the spike (Task 1)

- **What it shows:** the install-moment decision of spec §6.2 (U1–U7 in the plan's table) with a build of this branch's own code, signed with an ephemeral key, against a `latest.json` the host serves.
- **The build:** CI's artifact `cmdstudio-updater-spike` of a `workflow_dispatch` run of `ci.yml` on this branch with `updater_spike` on (`gh workflow run ci.yml --ref phase4b -f updater_spike=true`, after the controller's push). **One download, with the founder's explicit yes first:** `gh run download <run id> --name cmdstudio-updater-spike --dir <scratchpad>\p4b-spike`. The folder holds the two installers, their `.sig` files, `latest.json`, `latest.same.json`, `latest.bad.json` and `spike.key.pub`. Record the commit and the run id in `results.md`.
- **Setup (once per host session):** Windows PowerShell 5.1 on the host, from the repo root: `. .\spikes\vm\vm.ps1`, `. .\docs\spikes\phase3-vm-check\helpers.ps1`, `. .\docs\spikes\phase4-install\helpers.ps1`, `. .\docs\spikes\phase4-update\helpers.ps1`; `$spike = '<scratchpad>\p4b-spike'`; `$server = Start-Phase4bServer $spike` (the founder allows python through the firewall when Windows asks); `Set-Phase4bLatest $spike latest`. Per VM: `$vm`, `$u` as 4a's protocol sets them; `Start-CleanVm $vm $u`; `Install-Phase4bSetup $vm $u $spike` (prints `same file in the VM: True` twice); `$P4.Setup = $P4B.Setup`; `Invoke-GuestDesktop $vm "$($P4B.Guest)\guard.test.ps1" '' $u` must end with the computed total of checks passing.
- **How results are judged:** by `update-check.ps1`'s printed lines (`installed:`, `log:`, `updateJson:`, `processes:`), by `Wait-Phase4bVersion`'s seconds, by `server.log`'s request lines on the host, and by screenshots where nothing prints (the cards, no window after a close).

| # | Do (VM-A; VM-B when no VM with Windows Terminal starts) | PASS when | Result |
|---|---|---|---|
| A1 | **spike.1 installed, Terracotta applied.** `Start-Phase4bInstall $vm 1 $u` (interactive; drive the pages as 4a's step 2, "Run CMD Studio" ticked), the wizard to Terracotta applied, "Go to the home page"; `Close-Phase4App $vm $u`; `Get-Phase4bSnapshot $vm a1 $u`. | `installed: DisplayVersion=0.1.0-spike.1`; `autorun:` holds CMD Studio's part; `log:` has `update check=…` (the manifest offered spike.2 at this first start, so `update available=0.1.0-spike.2` and, in automatic mode, `update downloaded=0.1.0-spike.2` and `updates: cmdstudio-0.1.0-spike.2-setup.exe <size>`: that is U1). | |
| A2 | **U5, a manifest that is not JSON, then one that offers the same version.** `Set-Phase4bLatest $spike bad`; `Start-Phase4bApp $vm $u`; wait 20 s; `Save-Shot $vm 'p4b-a2-bad'`; `Close-Phase4App $vm $u`; `Get-Phase4bLog $vm $u`. Then `Set-Phase4bLatest $spike same`; the same again with the shot `p4b-a2-same`. | The log gains `update check=manifest`, then `update check=none`; the shots show the home page with no card and no box; `updates:` is `ABSENT` after each start (the folder is emptied at every start before the check). | |
| A3 | **U1 + U3: the automatic install at close.** `Set-Phase4bLatest $spike latest`; `Start-Phase4bApp $vm $u`; wait 30 s; `Get-Phase4bLog $vm $u` (`update downloaded=0.1.0-spike.2`). `Close-Phase4App $vm $u` (prints `closed with CloseMainWindow: 1`), at once `Wait-Phase4bVersion $vm 0.1.0-spike.2 $u`, then after 3 s `Save-Shot $vm 'p4b-a3-after-close'`; `Get-Phase4bSnapshot $vm a3 $u`; `Get-GuestAutoRun $vm 'a3' $u`. | `Wait-Phase4bVersion` prints `version 0.1.0-spike.2 after <n> s` (record n); the shot shows the desktop with no CMD Studio window and no installer page; `processes:` is empty (no relaunch); `updateJson:` is the record `{"version":1,"from":"0.1.0-spike.1","to":"0.1.0-spike.2","notesUrl":"https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.1.0-spike.2"}`; `log:` ends with `update installing=0.1.0-spike.2 how=exit`; AutoRun unchanged from A1; `install:` lists `cmdstudio.com` and no `cmdstudio-cli.exe`; `shortcuts:` the two. | |
| A4 | **U4: the notice.** `Start-Phase4bApp $vm $u`; wait 15 s; `Save-Shot $vm 'p4b-a4-notice'`; `Invoke-GuestButton $vm 'What changed' $u`; wait 5 s; `Save-Shot $vm 'p4b-a4-browser'`; `Invoke-GuestButton $vm 'Close' $u`; `Get-Phase4bLog $vm $u`; `Get-Phase4bSnapshot $vm a4 $u`. | The home page shows "Updated to v0.1.0-spike.2." with "What changed" and "Close"; the browser opens at `…/releases/tag/v0.1.0-spike.2` (a 404 page is fine); the log has `update installed to=0.1.0-spike.2` and no `update reapply=` line (the pins are equal); `updateJson: ABSENT` (deleted by `app_ready`); `updates: ABSENT`. | |
| A5 | **U2: "Update now" through the plugin.** Fresh `Start-CleanVm`, Setup, A1 again but close the app, then `Set-Phase4bMode $vm notify $u`; `Set-Phase4bLatest $spike latest`; `Start-Phase4bApp $vm $u`; wait 20 s; `Save-Shot $vm 'p4b-a5-card'`; `Invoke-GuestButton $vm 'Update now' $u`; at once `Wait-Phase4bVersion $vm 0.1.0-spike.2 $u`; wait 20 s; `Save-Shot $vm 'p4b-a5-back'`; `Get-Phase4bSnapshot $vm a5 $u`; `Get-GuestAutoRun $vm 'a5' $u`. | The card "CMD Studio v0.1.0-spike.2 is available." with "Update now" and "Later" shows, and no download happened before the click (`updates: ABSENT` in the log before it); after the click the window goes at once, `Wait-Phase4bVersion` prints the seconds, and the app is back by itself (`processes:` one pid; the shot shows the home page with "Updated to v0.1.0-spike.2."); `log:` has `update installing=0.1.0-spike.2 how=now` then, after the restart, `update installed to=0.1.0-spike.2`; AutoRun unchanged; no uninstaller ran (AutoRun kept, `install:` as in A3). | |
| A6 | **U6: a by-hand install of spike.2 over spike.1, "Do not uninstall".** Fresh `Start-CleanVm`, Setup, A1 (closed). `Start-Phase4bInstall $vm 2 $u` (interactive). Welcome: Next. The "Already Installed" page: `Save-Shot $vm 'p4b-a6-page'`, `Invoke-Phase4Control $vm 'Do not uninstall' $u` (`selected: True`; fallback `Send-Key $vm Down`), zoom the shot, Next; the pages on as 4a's step 2 ("Run CMD Studio" ticked). Wait 15 s. `Get-Phase4bSnapshot $vm a6 $u`; `Get-GuestAutoRun $vm 'a6' $u`; `Save-Shot $vm 'p4b-a6-home'`. | The page's exact words for the two radios are recorded (the first is Tauri's "Uninstall before installing", checked by default); after the install `installed: DisplayVersion=0.1.0-spike.2`, AutoRun unchanged (no uninstaller ran), the theme still applied (the home page shows Terracotta in use), no "Updated to" card (no record: a by-hand install writes none). | |
| A7 | **U7: Off, and offline.** From A6's state, close the app; `Set-Phase4bMode $vm off $u`; note the last line of `<spike>\server.log` on the host; `Start-Phase4bApp $vm $u`; wait 20 s; `Get-Phase4bLog $vm $u`; `Close-Phase4App $vm $u`. Then `Set-Phase4bMode $vm auto $u`; `Set-Phase4bNic <vm name> off`; `Start-Phase4bApp $vm $u`; wait 30 s; `Save-Shot $vm 'p4b-a7-offline'`; `Get-Phase4bLog $vm $u`; `Close-Phase4App $vm $u`; `Set-Phase4bNic <vm name> on`. | Off: the log's new line is `update mode=off` and `server.log` gained no request line. Offline: `update check=offline`, the shot shows the home page with no card and no box. | |

### Results (part A)

In `results.md`.
```

- [ ] **Step 6: `results.md`, the template**

```markdown
# Phase 4b — the updater spike's results (Task 1)

- **Run:** <date>, commit <sha>, CI run <id> (the `cmdstudio-updater-spike` artifact), VM <A or B>.
- **U1 (download, verified, written):** <PASS and the log lines, or the finding>.
- **U2 ("Update now" through the plugin: silent, no uninstaller, the app back by itself):** <PASS, the seconds from the click to the new version and to the window, or the finding>.
- **U3 (the own spawn at close: silent, no relaunch, the exe replaced):** <PASS, the seconds from the close to the new version, or the finding>.
- **U4 (the notice, its link, no re-apply with equal pins, the record gone):** <…>.
- **U5 (a manifest that is not JSON, and one with the same version):** <…>.
- **U6 (a by-hand upgrade keeps the theme through "Do not uninstall"; the page's words):** <the two radios' exact texts>.
- **U7 (Off makes no request; offline is silent):** <…>.
- **Decision:** the install moment is **<primary | F2 | F3>**, because <one sentence>. Task 3's last step and Task 8 read this line.
- **Screenshots** (`spikes\vm\tmp\<vm>-p4b-*.png`): <labels>.
```

- [ ] **Step 7: Parse checks, then commit**

Parse-check `update-check.ps1`, `helpers.ps1` and `guard.test.ps1` with `ParseFile`; each must report 0 errors. Parse `ci.yml`.

Write `.git/CMDSTUDIO_COMMIT_MSG`:

```
ci, docs, spikes: the updater spike's build and protocol (phase 4b)

ci.yml: a workflow_dispatch input updater_spike that builds two spike
installers (0.1.0-spike.1 and .2) signed with an ephemeral key made in
the run and never uploaded, with the updater pointed at the VM's host,
and a latest.json for each case (newer, same, malformed), uploaded as
cmdstudio-updater-spike; the installer listing's version grep reads
the line under [workspace.package] (4a's M6). update-check.ps1 (VM
only): snapshots of the installed version, AutoRun, the install
folder, the shortcuts, the Path, update.json, updates\ and the log's
update lines; -SetMode, -WaitForVersion, -Log, -StartApp. helpers.ps1
on the host, with the web server and the NIC switch. protocol.md part
A (U1-U7) and the results template. Parse-checked only; the controller
runs it.
```

Then `git add -- .github/workflows/ci.yml docs/spikes/phase4-update/protocol.md docs/spikes/phase4-update/helpers.ps1 docs/spikes/phase4-update/results.md spikes/vm/guest/update-check.ps1 spikes/vm/guest/guard.test.ps1`, check `git diff --cached --name-only`, then `git commit -F .git/CMDSTUDIO_COMMIT_MSG`.

- [ ] **Step 8: The run (controller)**

After Task 4 is committed (U2 needs the notify card): the controller pushes the branch, dispatches `ci.yml` with `updater_spike` on, reads the spike step's log (the two installers named with their versions, two `.sig` files, `latest.json`: G-SIG settled; a refusal of the pre-release version is F-VER), downloads the artifact once with the founder's yes, runs part A on VM-A (VM-B as the fallback), fills `results.md`, and commits it (`docs: the updater spike's results (phase 4b Task 1)`). A FAIL of U1 stops the phase and goes to the founder; U3's failure picks F2, which Task 3's last step then builds; U2's or U4's failure goes back to Task 3 or Task 4 as a bug.

---
### Task 2: The core and the paths: `update.json`, `updates\`, `runtime_changed`, `prune_old_runtimes`, `--cleanup`

The first task to run. Pure additions to the core and the CLI: where the updater's two files live, whether the applied theme runs on the pinned runtime, which old runtime folders may go, and `--cleanup --data-dir`'s knowledge of the new entries. Nothing here touches the machine; every test runs over the fake.

**Files:**
- Modify: `crates/core/src/recovery.rs` (`Paths::update_file`, `Paths::updates_dir`, in the `impl Paths` block at lines 41-59)
- Modify: `crates/core/src/plan.rs` (`runtime_changed`)
- Modify: `crates/core/src/cleanup.rs` (`runtime_dirs_named_by`, `prune_old_runtimes`)
- Modify: `crates/cli/src/lib.rs` (`put_there_by_cmd_studio`, `cmd_studios_own_file`, their doc comments, two tests)

**Interfaces:**
- Consumes: `Paths { data }`, `Paths::{runtime_root, omp_dir, clink_dir}`, `State`, `Applied { omp_version, clink_version, launch }`, `Launch::to_clink`, `ClinkLaunch::exe`, `OMP_VERSION`, `CLINK_VERSION`, `program_data_dir`, `Fs::{list_dir, remove_dir_all}`, `DirEntry { path, is_dir }`, `cleanup::same_path` (private helper of that file).
- Produces: `Paths::update_file() -> PathBuf` (`<data>\update.json`), `Paths::updates_dir() -> PathBuf` (`<data>\updates`); `plan::runtime_changed(state: &State) -> bool`; `cleanup::runtime_dirs_named_by(state: &State, paths: &Paths) -> Vec<PathBuf>`; `cleanup::prune_old_runtimes(sys: System<'_>, paths: &Paths, keep: &[PathBuf]) -> Vec<PathBuf>`; `--cleanup --data-dir` takes `update.json`, `updates\` and `.update.json.<hex 32>.cmdstudio-tmp` as CMD Studio's own.

- [ ] **Step 0: The baselines**

In the scratch worktree: `cargo test --workspace 2>&1 | grep -E "^test result"` (one line per crate and test binary), `cargo test -p cmdstudio-app --lib 2>&1 | grep "^test result"`, and in `app/`: `npm test 2>&1 | grep -E "Test Files|Tests "`. Note the counts for this task's commit message.

- [ ] **Step 1: The failing tests**

In `crates/core/src/recovery.rs`'s test module (after its last test):

```rust
    /// Spec §5.3, §6.2 (phase 4b): the updater's record and its download folder live directly in
    /// the data folder, under names `--cleanup` knows (the CLI's own-file lists).
    #[test]
    fn update_json_and_updates_dir_are_under_the_data_folder() {
        let p = Paths {
            data: PathBuf::from(r"C:\d"),
        };
        assert_eq!(p.update_file(), PathBuf::from(r"C:\d\update.json"));
        assert_eq!(p.updates_dir(), PathBuf::from(r"C:\d\updates"));
        assert_eq!(p.update_file().parent(), Some(p.data.as_path()));
    }
```

In `crates/core/src/plan.rs`'s test module:

```rust
    /// Spec §6.2 "after an update": the theme is re-applied only when the applied theme runs on
    /// runtime folders of other versions than this build pins; no theme, nothing to re-apply.
    #[test]
    fn runtime_changed_only_when_a_pin_differs() {
        use crate::state::{Applied, Launch, State};
        let applied = |omp: &str, clink: &str| Applied {
            theme_id: uuid::Uuid::nil(),
            theme_name: "Terracotta".into(),
            target: Target::All,
            set_default: true,
            launch: Launch::Direct {
                exe: PathBuf::from(format!(r"C:\d\runtime\clink-{clink}\clink_x64.exe")),
            },
            omp_version: omp.into(),
            clink_version: clink.into(),
            generated: Default::default(),
            wt_font: None,
            clink_log_len: 0,
            snapshot_id: "1791694709000-0123abcd".into(),
            applied_at_ms: 1_791_694_709_000,
        };
        let with = |a: Option<Applied>| State {
            applied: a,
            ..State::default()
        };
        assert!(!runtime_changed(&with(None)));
        assert!(!runtime_changed(&with(Some(applied(OMP_VERSION, CLINK_VERSION)))));
        assert!(runtime_changed(&with(Some(applied("31.4.0", CLINK_VERSION)))));
        assert!(runtime_changed(&with(Some(applied(OMP_VERSION, "1.9.0")))));
    }
```

(`Target` and `PathBuf` are imported in that module already; `uuid` is a dependency of the core. If `Applied` has a field this list does not name, the compiler says so: add it with a plain value.)

In `crates/core/src/cleanup.rs`'s test module:

```rust
    /// Spec §6.2 step 3 (phase 4b): the runtime folders of other versions go, best effort; the
    /// pinned ones, the ones state.json names (AutoRun still runs them until the re-apply), a
    /// folder held open, and anything not named like a runtime stay.
    #[test]
    fn old_runtime_folders_go_and_held_ones_stay() {
        let m = FakeMachine::new();
        let paths = Paths {
            data: PathBuf::from(r"C:\d"),
        };
        let pd = program_data_dir(&m.host.program_data, &m.host.sid);
        for dir in [
            r"C:\d\runtime\omp-31.5.0",
            r"C:\d\runtime\omp-31.4.0",
            r"C:\d\runtime\clink-1.9.34",
            r"C:\d\runtime\clink-1.9.0",
            r"C:\d\runtime\clink-1.8.9",
            r"C:\d\runtime\notes",
        ] {
            m.fs.create_dir_all(Path::new(dir)).unwrap();
        }
        m.fs.put(r"C:\d\runtime\clink-1.9.0\clink_dll_x64.dll", b"x");
        m.fs.put(r"C:\d\runtime\clink-1.8.9\clink_dll_x64.dll", b"x");
        m.fs.put(pd.join("clink-1.9.0").join("clink_x64.exe"), b"x");
        m.fs.put(pd.join("clink-1.9.34").join("clink_x64.exe"), b"x");
        // A cmd window still holds the DLL of 1.8.9: the folder cannot be removed.
        m.fs.fail_removes_of(r"C:\d\runtime\clink-1.8.9");
        // state.json names clink-1.9.0 under the data folder: kept this time.
        let keep = vec![PathBuf::from(r"C:\d\runtime\clink-1.9.0")];
        let removed = prune_old_runtimes(m.system(), &paths, &keep);
        assert_eq!(
            removed,
            [PathBuf::from(r"C:\d\runtime\omp-31.4.0"), pd.join("clink-1.9.0")]
        );
        for kept in [
            r"C:\d\runtime\omp-31.5.0",
            r"C:\d\runtime\clink-1.9.34",
            r"C:\d\runtime\clink-1.9.0",
            r"C:\d\runtime\clink-1.8.9",
            r"C:\d\runtime\notes",
        ] {
            assert!(m.fs.exists(Path::new(kept)), "{kept}");
        }
        assert!(m.fs.exists(&pd.join("clink-1.9.34")), "the pinned ProgramData folder stays");
        // Nothing names 1.9.0 any more: it goes; the held one stays again, without an error.
        let removed = prune_old_runtimes(m.system(), &paths, &[]);
        assert_eq!(removed, [PathBuf::from(r"C:\d\runtime\clink-1.9.0")]);
        assert!(m.fs.exists(Path::new(r"C:\d\runtime\clink-1.8.9")));
        // No runtime folder at all: nothing to do, nothing made.
        let empty = Paths {
            data: PathBuf::from(r"C:\e"),
        };
        assert_eq!(prune_old_runtimes(m.system(), &empty, &[]), Vec::<PathBuf>::new());
        assert!(!m.fs.exists(Path::new(r"C:\e")));
    }

    /// What a prune must keep whatever its version: the applied theme's OMP folder and the folder
    /// of the Clink exe AutoRun runs, under the data folder or under ProgramData; none without a theme.
    #[test]
    fn the_runtime_folders_state_json_names_are_kept() {
        use crate::state::{Applied, Launch};
        let paths = Paths {
            data: PathBuf::from(r"C:\d"),
        };
        let applied = |launch: Launch| Applied {
            theme_id: uuid::Uuid::nil(),
            theme_name: "Terracotta".into(),
            target: Target::All,
            set_default: true,
            launch,
            omp_version: "31.4.0".into(),
            clink_version: "1.9.0".into(),
            generated: Default::default(),
            wt_font: None,
            clink_log_len: 0,
            snapshot_id: "1791694709000-0123abcd".into(),
            applied_at_ms: 1_791_694_709_000,
        };
        let direct = State {
            applied: Some(applied(Launch::Direct {
                exe: PathBuf::from(r"C:\d\runtime\clink-1.9.0\clink_x64.exe"),
            })),
            ..State::default()
        };
        assert_eq!(
            runtime_dirs_named_by(&direct, &paths),
            [
                PathBuf::from(r"C:\d\runtime\omp-31.4.0"),
                PathBuf::from(r"C:\d\runtime\clink-1.9.0")
            ]
        );
        let program_data = State {
            applied: Some(applied(Launch::ProgramData {
                dir: PathBuf::from(r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001\clink-1.9.0"),
            })),
            ..State::default()
        };
        assert_eq!(
            runtime_dirs_named_by(&program_data, &paths),
            [
                PathBuf::from(r"C:\d\runtime\omp-31.4.0"),
                PathBuf::from(r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001\clink-1.9.0")
            ]
        );
        assert_eq!(runtime_dirs_named_by(&State::default(), &paths), Vec::<PathBuf>::new());
    }
```

(`FakeMachine`, `program_data_dir`, `Paths`, `State`, `Target`, `Path`, `PathBuf` are in that test module's imports already, as its `cleanup_removes_a_private_program_data_folder_and_leaves_a_foreign_one` and `own_program_data` show; add what the compiler asks for.)

In `crates/cli/src/lib.rs`'s test module, in `cleanup_with_a_data_dir_takes_the_apps_own_entries_for_cmd_studios`, add to the `for file in [...]` list:

```rust
            "update.json",
            r"updates\cmdstudio-0.2.0-setup.exe",
            ".update.json.0123456789abcdef0123456789abcdef.cmdstudio-tmp",
```

and in `cleanup_with_a_data_dir_refuses_names_that_only_look_like_the_apps_own`, add `"update.json.old"` and `"updates.zip"` to its list.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p cmdstudio-core --features fake --lib recovery::tests::update_json plan::tests::runtime_changed cleanup::tests::old_runtime cleanup::tests::the_runtime_folders`
Expected: compile errors (`update_file`, `updates_dir`, `runtime_changed`, `prune_old_runtimes`, `runtime_dirs_named_by` not found).
Run: `cargo test -p cmdstudio-cli cleanup_with_a_data_dir`
Expected: `cleanup_with_a_data_dir_takes_the_apps_own_entries_for_cmd_studios` fails with exit code 2 ("update.json" named as not CMD Studio's); the refusing test passes already.

- [ ] **Step 3: The paths**

In `crates/core/src/recovery.rs`, in the `impl Paths` block after `settings_reset_file`:

```rust
    /// `<data>\update.json`: the updater's record, written before the new installer starts and
    /// read at the next start to say "Updated to vX" (spec §5.3, §6.2, phase 4b).
    pub fn update_file(&self) -> PathBuf {
        self.data.join("update.json")
    }
    /// `<data>\updates`: the verified installer the updater downloaded, until the next start
    /// empties the folder (phase 4b).
    pub fn updates_dir(&self) -> PathBuf {
        self.data.join("updates")
    }
```

- [ ] **Step 4: `runtime_changed`**

In `crates/core/src/plan.rs`, after the `impl Paths` block (line 59):

```rust
/// Whether the theme applied now runs on runtime folders of other versions than this build pins
/// (spec §6.2 "after an update", phase 4b): then the apply must run again, so that AutoRun and
/// the generated files point at `runtime\omp-<OMP_VERSION>` and `clink-<CLINK_VERSION>`. No theme
/// applied: nothing to re-apply.
pub fn runtime_changed(state: &crate::state::State) -> bool {
    state
        .applied
        .as_ref()
        .is_some_and(|a| a.omp_version != OMP_VERSION || a.clink_version != CLINK_VERSION)
}
```

- [ ] **Step 5: The prune**

In `crates/core/src/cleanup.rs`, after `remove_program_data` (line ~1600; `same_path` and `program_data_dir` are in scope there; add `use crate::plan::CLINK_VERSION;` and `use crate::state::State;` to the file's imports if they are not already):

```rust
/// The runtime folders `state.json` names, which a prune keeps whatever their version (phase
/// 4b): the applied theme's `runtime\omp-<its version>` and the folder of the Clink exe its
/// `<clink-launch>` runs, which AutoRun points at until a re-apply. None without a theme.
pub fn runtime_dirs_named_by(state: &State, paths: &Paths) -> Vec<PathBuf> {
    let Some(applied) = &state.applied else {
        return Vec::new();
    };
    let mut dirs = vec![paths
        .runtime_root()
        .join(format!("omp-{}", applied.omp_version))];
    if let Some(dir) = applied.launch.to_clink().exe().parent() {
        dirs.push(dir.to_path_buf());
    }
    dirs
}

/// Deletes the runtime folders of other versions than this build pins (spec §6.2 step 3, phase
/// 4b), best effort: every folder directly in `runtime\` named `omp-*` or `clink-*`, and every
/// `clink-*` directly in the user's own ProgramData folder, that is neither the pinned one nor in
/// `keep` (what state.json names, [`runtime_dirs_named_by`]). A folder that cannot be removed
/// (a cmd window still holds its DLL) is skipped and met again at the next start; a root that
/// cannot be listed, or is not there, is skipped; nothing is created. Names compare without
/// ASCII case, as Windows does. Returns the folders removed, in the order met.
pub fn prune_old_runtimes(sys: System<'_>, paths: &Paths, keep: &[PathBuf]) -> Vec<PathBuf> {
    let mut pinned = vec![paths.omp_dir(), paths.clink_dir()];
    let mut roots = vec![paths.runtime_root()];
    if let (Ok(pd), Ok(sid)) = (sys.host.program_data(), sys.host.user_sid()) {
        let own = program_data_dir(&pd, &sid);
        pinned.push(own.join(format!("clink-{CLINK_VERSION}")));
        roots.push(own);
    }
    let mut removed = Vec::new();
    for root in roots {
        let Ok(entries) = sys.fs.list_dir(&root) else {
            continue;
        };
        for entry in entries {
            let Some(name) = entry.path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let lower = name.to_ascii_lowercase();
            let runtime = entry.is_dir && (lower.starts_with("omp-") || lower.starts_with("clink-"));
            let kept = pinned
                .iter()
                .chain(keep)
                .any(|k| same_path(k, &entry.path));
            if runtime && !kept && sys.fs.remove_dir_all(&entry.path).is_ok() {
                removed.push(entry.path);
            }
        }
    }
    removed
}
```

If `same_path` in that file is written for the other argument order, or `list_dir` of a missing folder answers `Ok(vec![])` in the fake (then the `C:\e` assertion still holds), the tests say so.

- [ ] **Step 6: Run the core's tests**

Run: `cargo test -p cmdstudio-core --features fake --lib recovery::tests::update_json plan::tests::runtime_changed cleanup::tests::old_runtime cleanup::tests::the_runtime_folders`
Expected: the four tests pass.

- [ ] **Step 7: The CLI's own-file lists**

In `crates/cli/src/lib.rs`:

- `put_there_by_cmd_studio`: the folder list gains `paths.updates_dir(),` after `paths.health(),`; the temp-name list `[paths.state_file(), paths.settings_file(), paths.run_file()]` gains `paths.update_file()`; its doc comment's folder sentence gains "and `updates\` (the updater's verified download, phase 4b)" and the temp-file sentence names `update.json` with the other three ("state.json, settings.json, run.json and update.json are the only files CMD Studio writes directly in the data folder").
- `cmd_studios_own_file`: the `own` array gains `paths.update_file(),`; its doc comment gains "and `update.json`, the updater's record of an update in progress (spec §6.2, phase 4b)".

Run: `cargo test -p cmdstudio-cli cleanup_with_a_data_dir`
Expected: both tests pass (the own-entries test leaves only `themes`; `update.json.old` and `updates.zip` are refused with exit code 2).

- [ ] **Step 8: The checks, then commit**

The four clippy runs, `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo test -p cmdstudio-core` (no fake feature: the library still builds), `cargo test --workspace`.

Write `.git/CMDSTUDIO_COMMIT_MSG`:

```
core, cli: the updater's paths, runtime_changed and the prune (spec §6.2, phase 4b)

Paths::update_file (<data>\update.json) and Paths::updates_dir
(<data>\updates). plan::runtime_changed: the applied theme's OMP or
Clink version differs from this build's pins. cleanup::runtime_dirs_named_by
(the folders state.json names, kept whatever their version) and
cleanup::prune_old_runtimes (every other omp-*/clink-* under runtime\
and clink-* under the user's ProgramData folder, best effort: a held
folder stays). --cleanup --data-dir takes update.json, updates\ and
update.json's temp file as CMD Studio's own.

Baselines at ea91974 on this PC: <the counts measured in Step 0>.
```

Then `git add -- crates/core/src/recovery.rs crates/core/src/plan.rs crates/core/src/cleanup.rs crates/cli/src/lib.rs`, check `git diff --cached --name-only`, then `git commit -F .git/CMDSTUDIO_COMMIT_MSG`.

---
### Task 3: The updater behind a trait, the state machine, `update.json`, the check at launch, the install at close and the install now

Runs after Task 2. The plugin goes behind `Updater`; the backend's state machine is tested over `FakeUpdater` and the fake machine; `lib.rs` wires the plugin, the record's read after the run record, and the exit hook. Both install paths are production code (Decisions): the automatic one at close (the own spawn, `/S /UPDATE`) and "Update now" through the plugin's `install` (`/S /R /UPDATE /ARGS`, the process ends inside, the app comes back). The committed config holds an empty pubkey until Task 6, which `TauriUpdater::check` refuses before any request.

**Files:**
- Modify: `app/src-tauri/Cargo.toml` (`tauri-plugin-updater`, `semver`), `Cargo.lock`
- Create: `app/src-tauri/src/update.rs`
- Create: `app/src-tauri/src/backend/update.rs`
- Modify: `app/src-tauri/src/backend/mod.rs` (`pub mod update;`, the `update` field, `new()`)
- Modify: `app/src-tauri/src/backend/run.rs` (`app_ready` consumes the record)
- Modify: `app/src-tauri/src/open.rs` (`ExternalTarget::ReleaseNotes`, `release_url`, `Place::Url(String)`)
- Modify: `app/src-tauri/src/lib.rs` (`pub mod update;`, the plugin, `TauriUpdater` in Tauri's state, `update_after_start`, the exit hook)
- Modify: `app/src-tauri/src/config_guard.rs` (`plugins.updater` pinned)
- Modify: `app/src-tauri/tauri.conf.json` (`plugins.updater`)

**Interfaces:**
- Consumes: `Paths::{update_file, updates_dir, lock_file}` (Task 2), `Backend::{settings_or_default, note_line, now_ms, end_run, paths, sys}`, `Fs::{try_lock, write_atomic, read, exists, remove_file, remove_dir_all}`, `cmdstudio_core::resources::sha256_hex`, `AppError::{coded, internal}`, `UpdateMode`, the plugin's `UpdaterExt::updater_builder`, `UpdaterBuilder::{version_comparator, build}`, `Updater::check`, `Update::{download, install, version}`, `Config.plugins`.
- Produces: `cmdstudio_app::update::{Updater, Available { version, notes_url }, UpdateError::{Offline, Manifest, Signature, Install}, UpdateError::kind() -> &'static str, TauriUpdater::new(AppHandle), install_quiet(&Path) -> Result<(), String>, start_check(&AppHandle, Arc<Backend<RealMachine>>), is_newer(&Version, &Version) -> bool}`; `cmdstudio_app::backend::update::{UpdateRecord, UpdateState, UpdateStatus { mode, state, checked_at_ms }, Updated { from, to, notes_url }, UPDATE_RECORD_VERSION, UPDATE_REAPPLY_BUSY}`; `Backend::{update_check_and_download(&dyn Updater) -> UpdateState, update_status() -> Result<UpdateStatus, AppError>, update_install(&dyn Updater) -> Result<(), AppError>, update_at_exit(&dyn Fn(&Path) -> Result<(), String>), update_after_start(&str), update_record_consumed(), updated() -> Option<Updated>}`; `open::{ExternalTarget::ReleaseNotes { version: String }, release_url(&str) -> Option<String>, RELEASES_URL}`; the log lines `update mode=off`, `update check=<kind|none>`, `update available=<v>`, `update download=<kind|write>`, `update downloaded=<v>`, `update postponed=busy|lock|record`, `update install=file-changed|spawn|<kind>`, `update installing=<v> how=exit|now`, `update installed to=<v>`, `update failed to=<v> running=<v>`, `update record=unreadable|damaged`, `update updates=kept`, `update check=thread <e>`; the file `updates\cmdstudio-<version>-setup.exe`; `update.json` `{"version":1,"from":…,"to":…,"notesUrl":…}`.

- [ ] **Step 0: The pins and G-UPD**

In `app/src-tauri/Cargo.toml`, under `[dependencies]` after `rfd`:

```toml
# The updater (spec §6.2, phase 4b): GitHub Releases through latest.json, minisign-verified,
# used from Rust only (update.rs): the window gets none of its commands (capabilities/default.json,
# pinned by ipc.rs). G10's table lists 2.13.2; 2.13.3 is the newest 2.13.x on the index
# (tauri ^2.12, so Tauri stays at 2.12.4). Default features: rustls-tls, zip.
tauri-plugin-updater = "=2.13.3"
# open.rs: a version from latest.json or update.json must parse as SemVer before it reaches a URL.
semver = "=1.0.28"
```

Run: `cargo fetch` (in the scratch worktree; `Cargo.lock` gains the plugin's tree). Expected: it resolves; if it does not (G-IDX), F-PIN with `=2.12.0` and the same steps. Then reread, in `~/.cargo/registry/src/*/tauri-plugin-updater-2.13.3/src/`, the facts of G-UPD (a)–(f) and write their file:line into the module doc of `update.rs` (Step 4). `cargo tree -p cmdstudio-app -i tauri-utils` must show one `tauri-utils` version (2.10.2); two would be the G-IDX conflict.

- [ ] **Step 1: The failing tests**

`app/src-tauri/src/update.rs` (the file is created here with its tests; the module body comes in Step 4):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// U5's rule, set explicitly on the plugin's builder: only a strictly higher SemVer is an
    /// update; the same version, a lower one and a pre-release of the current one are not.
    #[test]
    fn only_a_higher_version_is_newer() {
        let v = |s: &str| semver::Version::parse(s).unwrap();
        assert!(is_newer(&v("0.1.0"), &v("0.1.1")));
        assert!(is_newer(&v("0.1.0"), &v("0.2.0-rc.1")));
        assert!(is_newer(&v("0.1.0-spike.1"), &v("0.1.0-spike.2")));
        assert!(!is_newer(&v("0.1.0"), &v("0.1.0")));
        assert!(!is_newer(&v("0.1.0"), &v("0.0.9")));
        assert!(!is_newer(&v("0.1.0"), &v("0.1.0-rc.1")));
    }

    /// The log says the kind of a failure and never its text, which may hold a URL or a path.
    #[test]
    fn each_error_has_a_one_word_kind() {
        assert_eq!(UpdateError::Offline("dns".into()).kind(), "offline");
        assert_eq!(UpdateError::Manifest("x".into()).kind(), "manifest");
        assert_eq!(UpdateError::Signature("x".into()).kind(), "signature");
        assert_eq!(UpdateError::Install("x".into()).kind(), "install");
        assert!(UpdateError::Offline("no route to host".into()).to_string().contains("no route to host"));
    }

    /// The automatic install at close (U3): the own spawn runs the file silently as an update, with
    /// no relaunch and with its stdio closed, and waits for nothing. Read as text: no installer is
    /// ever started on this PC.
    #[cfg(windows)]
    #[test]
    fn the_own_spawn_is_silent_and_does_not_relaunch() {
        let source = include_str!("update.rs");
        let start = source.find("pub fn install_quiet(").unwrap();
        let body = source.get(start..).unwrap().split_once("\n}\n").unwrap().0;
        assert!(body.contains(r#".args(["/S", "/UPDATE"])"#), "{body}");
        assert!(!body.contains("/R"), "no relaunch");
        assert_eq!(body.matches("Stdio::null()").count(), 3, "stdin, stdout and stderr closed");
        assert!(body.contains(".spawn()") && !body.contains(".wait("), "never waited for");
    }

    /// The fake updater records what the backend asked of it, in order.
    #[test]
    fn the_fake_records_its_calls() {
        let fake = FakeUpdater::offering("0.2.0");
        let a = fake.check().unwrap().unwrap();
        assert_eq!(a.version, "0.2.0");
        assert_eq!(a.notes_url, "https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0");
        assert_eq!(fake.download(&a).unwrap(), b"MZ-the-installer".to_vec());
        let ran = std::cell::Cell::new(false);
        fake.install_now(&a, b"MZ-the-installer".to_vec(), &|| {}).unwrap();
        fake.install_now(&a, vec![], &|| ran.set(true)).unwrap();
        assert!(ran.get(), "the hook runs before the installer");
        assert_eq!(fake.calls(), ["check", "download 0.2.0", "install 0.2.0 16", "install 0.2.0 0"]);
        assert_eq!(FakeUpdater::none().check().unwrap(), None);
        assert_eq!(
            FakeUpdater::failing(UpdateError::Offline("x".into())).check().unwrap_err().kind(),
            "offline"
        );
    }
}
```

`app/src-tauri/src/backend/update.rs` (created here with its tests; the body comes in Step 5):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{backend, fake_fs};
    use crate::settings::AppSettings;
    use crate::update::{FakeUpdater, UpdateError};
    use cmdstudio_core::adapters::fake::FakeMachine;
    use std::cell::RefCell;

    fn set_mode<M: Machine>(b: &Backend<M>, updates: UpdateMode) {
        b.settings_set(AppSettings {
            updates,
            ..AppSettings::default()
        })
        .unwrap();
    }

    fn update_lines<M: Machine>(b: &Backend<M>) -> Vec<String> {
        b.log_tail(50)
            .lines()
            .filter_map(|l| l.split_once(" update ").map(|(_, rest)| format!("update {rest}")))
            .collect()
    }

    fn record(b: &Backend<FakeMachine>) -> Option<UpdateRecord> {
        let bytes = b.sys().fs.read(&b.paths().update_file()).unwrap()?;
        Some(serde_json::from_slice(&bytes).unwrap())
    }

    /// Spec §6.2: Off checks nothing and says so once per run.
    #[test]
    fn off_checks_nothing_and_logs_once() {
        let b = backend();
        set_mode(&b, UpdateMode::Off);
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Off);
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Off);
        assert_eq!(fake.calls(), Vec::<String>::new());
        assert_eq!(update_lines(&b), ["update mode=off"]);
        let status = b.update_status().unwrap();
        assert_eq!((status.mode, status.state, status.checked_at_ms), (UpdateMode::Off, UpdateState::Off, None));
    }

    /// Spec §6.2: a check that fails (no network, GitHub down) is one log line and `idle`; nothing
    /// is downloaded, nothing is shown, and the time of the check is noted.
    #[test]
    fn offline_is_silent_and_idle() {
        let b = backend();
        let fake = FakeUpdater::failing(UpdateError::Offline("dns".into()));
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Idle);
        assert_eq!(fake.calls(), ["check"]);
        assert_eq!(update_lines(&b), ["update check=offline"]);
        assert!(!b.sys().fs.exists(&b.paths().updates_dir()));
        assert_eq!(b.updated(), None);
        assert!(b.update_status().unwrap().checked_at_ms.is_some());
    }

    /// U5: a manifest that is not JSON, or offers no Windows target, is a refused check, said as such.
    #[test]
    fn a_malformed_manifest_is_offline() {
        let b = backend();
        let fake = FakeUpdater::failing(UpdateError::Manifest("expected value at line 1".into()));
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Idle);
        assert_eq!(update_lines(&b), ["update check=manifest"]);
        let b = backend();
        assert_eq!(b.update_check_and_download(&FakeUpdater::none()), UpdateState::Idle);
        assert_eq!(update_lines(&b), ["update check=none"]);
    }

    /// Spec §6.2 (automatic, the primary): the installer is downloaded after the check, written to
    /// updates\ in one step, and started at close with the record written and the run ended.
    #[test]
    fn auto_downloads_and_is_ready_at_exit() {
        let b = backend();
        b.start_run(crate::backend::run::GuiStart::Normal);
        b.update_after_start("0.1.0");
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Ready { version: "0.2.0".into() });
        assert_eq!(fake.calls(), ["check", "download 0.2.0"]);
        let file = b.paths().updates_dir().join("cmdstudio-0.2.0-setup.exe");
        assert_eq!(b.sys().fs.read(&file).unwrap().as_deref(), Some(&b"MZ-the-installer"[..]));
        assert_eq!(update_lines(&b), ["update available=0.2.0", "update downloaded=0.2.0"]);
        assert_eq!(b.update_status().unwrap().state, UpdateState::Ready { version: "0.2.0".into() });

        let spawned = RefCell::new(Vec::new());
        b.update_at_exit(&|f| {
            spawned.borrow_mut().push(f.to_path_buf());
            Ok(())
        });
        assert_eq!(spawned.into_inner(), [file]);
        assert_eq!(
            record(&b),
            Some(UpdateRecord {
                version: 1,
                from: "0.1.0".into(),
                to: "0.2.0".into(),
                notes_url: "https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0".into(),
            })
        );
        assert_eq!(update_lines(&b).last().map(String::as_str), Some("update installing=0.2.0 how=exit"));
        assert_eq!(b.update_status().unwrap().state, UpdateState::Installing { version: "0.2.0".into() });
        // The record, on the wire, in camelCase like every file CMD Studio writes.
        let text = String::from_utf8(b.sys().fs.read(&b.paths().update_file()).unwrap().unwrap()).unwrap();
        assert!(text.contains("\"notesUrl\"") && text.contains("\"from\": \"0.1.0\""), "{text}");
    }

    /// Spec §6.2 (notify): nothing is downloaded until the button; "Update now" downloads, writes
    /// the record, ends the run and hands the bytes to the plugin's install, which ends the process.
    #[test]
    fn notify_waits_for_the_button() {
        let b = backend();
        b.start_run(crate::backend::run::GuiStart::Normal);
        b.update_after_start("0.1.0");
        set_mode(&b, UpdateMode::Notify);
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Available { version: "0.2.0".into() });
        assert_eq!(fake.calls(), ["check"]);
        assert!(!b.sys().fs.exists(&b.paths().updates_dir()));
        let spawned = RefCell::new(0);
        b.update_at_exit(&|_| {
            *spawned.borrow_mut() += 1;
            Ok(())
        });
        assert_eq!(spawned.into_inner(), 0, "nothing is ready: nothing starts at close");

        b.update_install(&fake).unwrap();
        assert_eq!(fake.calls(), ["check", "download 0.2.0", "install 0.2.0 16"]);
        assert_eq!(record(&b).map(|r| r.to), Some("0.2.0".into()));
        assert!(!b.sys().fs.exists(&b.paths().run_file()), "the run ended before the installer");
        assert_eq!(update_lines(&b).last().map(String::as_str), Some("update installing=0.2.0 how=now"));
        // Nothing to install: refused with an internal error, nothing written.
        let b = backend();
        let e = b.update_install(&FakeUpdater::none()).unwrap_err();
        assert_eq!(e.code, "E_INTERNAL");
        assert!(!b.sys().fs.exists(&b.paths().update_file()));
    }

    /// Spec §5.5, §6.2 (Review Focus 2): a change that still runs holds the lock; the install at
    /// close is postponed and said, "Update now" answers E_BUSY, and the next launch empties the
    /// download and checks again.
    #[test]
    fn a_busy_lock_postpones_the_install_and_tries_next_launch() {
        let b = backend();
        b.update_after_start("0.1.0");
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Ready { version: "0.2.0".into() });
        let held = b.sys().fs.try_lock(&b.paths().lock_file()).unwrap().unwrap();
        let spawned = RefCell::new(0);
        b.update_at_exit(&|_| {
            *spawned.borrow_mut() += 1;
            Ok(())
        });
        assert_eq!(spawned.into_inner(), 0);
        assert_eq!(update_lines(&b).last().map(String::as_str), Some("update postponed=busy"));
        assert_eq!(
            b.update_status().unwrap().state,
            UpdateState::Postponed { version: "0.2.0".into(), reason: "busy".into() }
        );
        assert_eq!(record(&b), None);
        assert_eq!(b.update_install(&fake).unwrap_err().code, "E_BUSY");
        drop(held);
        // The next launch: the last run's download is not trusted, the folder is emptied, the
        // check runs anew.
        let next = backend();
        fake_fs(&next).put(next.paths().updates_dir().join("cmdstudio-0.2.0-setup.exe"), b"old");
        next.update_after_start("0.1.0");
        assert!(!next.sys().fs.exists(&next.paths().updates_dir()));
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(next.update_check_and_download(&fake), UpdateState::Ready { version: "0.2.0".into() });
        assert_eq!(fake.calls(), ["check", "download 0.2.0"]);
    }

    /// Spec §2.1: the record makes the notice, once; the file goes with app_ready, the notice stays
    /// for the run; a start without a record shows nothing.
    #[test]
    fn update_json_makes_the_notice_once_and_goes_with_app_ready() {
        let b = backend();
        fake_fs(&b).put(
            b.paths().update_file(),
            br#"{"version":1,"from":"0.1.0","to":"0.2.0","notesUrl":"https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0"}"#,
        );
        b.update_after_start("0.2.0");
        assert_eq!(
            b.updated(),
            Some(Updated {
                from: "0.1.0".into(),
                to: "0.2.0".into(),
                notes_url: "https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0".into(),
            })
        );
        assert_eq!(update_lines(&b), ["update installed to=0.2.0"]);
        assert!(b.sys().fs.exists(&b.paths().update_file()), "kept until the UI has shown it");
        b.start_run(crate::backend::run::GuiStart::Normal);
        b.app_ready().unwrap();
        assert!(!b.sys().fs.exists(&b.paths().update_file()));
        assert!(b.updated().is_some(), "the card stays for the run");
        let next = backend();
        next.update_after_start("0.2.0");
        assert_eq!(next.updated(), None);
        assert_eq!(update_lines(&next), Vec::<String>::new());
    }

    /// Review Focus 3: the installer did not finish and the old version starts with the record
    /// there: no notice, one log line, the record gone.
    #[test]
    fn a_failed_install_is_logged_and_the_file_goes() {
        let b = backend();
        fake_fs(&b).put(
            b.paths().update_file(),
            br#"{"version":1,"from":"0.1.0","to":"0.2.0","notesUrl":"https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0"}"#,
        );
        b.update_after_start("0.1.0");
        assert_eq!(b.updated(), None);
        assert_eq!(update_lines(&b), ["update failed to=0.2.0 running=0.1.0"]);
        assert!(!b.sys().fs.exists(&b.paths().update_file()));
    }

    /// Review Focus 4: a record that is not JSON, of another version, or whose `to` is not SemVer
    /// makes no notice and goes; one that cannot be read stays and is said.
    #[test]
    fn a_damaged_update_record_is_logged_and_removed() {
        for text in [
            "{ not json",
            r#"{"version":2,"from":"0.1.0","to":"0.2.0","notesUrl":""}"#,
            r#"{"version":1,"from":"0.1.0","to":"../x","notesUrl":""}"#,
        ] {
            let b = backend();
            fake_fs(&b).put(b.paths().update_file(), text);
            b.update_after_start("0.2.0");
            assert_eq!(b.updated(), None, "{text}");
            assert_eq!(update_lines(&b), ["update record=damaged"], "{text}");
            assert!(!b.sys().fs.exists(&b.paths().update_file()), "{text}");
        }
        let b = backend();
        fake_fs(&b).put(b.paths().update_file(), "{}");
        fake_fs(&b).fail_reads_of(b.paths().update_file());
        b.update_after_start("0.2.0");
        assert_eq!(update_lines(&b), ["update record=unreadable"]);
        assert!(b.sys().fs.exists(&b.paths().update_file()));
    }

    /// Review Focus 1: the file in updates\ is compared with the hash of the verified bytes before
    /// it is started; one that changed is deleted and never started, and "Update now" downloads again.
    #[test]
    fn a_download_that_changed_on_disk_is_not_installed() {
        let b = backend();
        b.start_run(crate::backend::run::GuiStart::Normal);
        b.update_after_start("0.1.0");
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Ready { version: "0.2.0".into() });
        let file = b.paths().updates_dir().join("cmdstudio-0.2.0-setup.exe");
        fake_fs(&b).put(&file, b"MZ-something-else");
        let spawned = RefCell::new(0);
        b.update_at_exit(&|_| {
            *spawned.borrow_mut() += 1;
            Ok(())
        });
        assert_eq!(spawned.into_inner(), 0);
        assert_eq!(update_lines(&b).last().map(String::as_str), Some("update install=file-changed"));
        assert!(!b.sys().fs.exists(&file));
        assert_eq!(record(&b), None);
        b.update_install(&fake).unwrap();
        assert_eq!(fake.calls(), ["check", "download 0.2.0", "download 0.2.0", "install 0.2.0 16"]);
    }

    /// A download that cannot be written is said and leaves the state idle; a spawn that fails
    /// at close takes the record back; an install through the plugin that fails is an error the
    /// window sees, with the record taken back and the state as before.
    #[test]
    fn a_download_or_a_start_that_fails_is_said_and_leaves_no_record() {
        let b = backend();
        b.update_after_start("0.1.0");
        let file = b.paths().updates_dir().join("cmdstudio-0.2.0-setup.exe");
        fake_fs(&b).fail_writes_to(&file);
        assert_eq!(b.update_check_and_download(&FakeUpdater::offering("0.2.0")), UpdateState::Idle);
        assert_eq!(update_lines(&b), ["update available=0.2.0", "update download=write"]);

        let b = backend();
        b.update_after_start("0.1.0");
        let fake = FakeUpdater::offering("0.2.0");
        b.update_check_and_download(&fake);
        b.update_at_exit(&|_| Err("access denied".into()));
        assert_eq!(update_lines(&b).last().map(String::as_str), Some("update install=spawn"));
        assert_eq!(record(&b), None);

        let b = backend();
        b.start_run(crate::backend::run::GuiStart::Normal);
        b.update_after_start("0.1.0");
        let fake = FakeUpdater::offering("0.2.0");
        *fake.install.borrow_mut() = Err(UpdateError::Install("ShellExecuteW answered 5".into()));
        b.update_check_and_download(&fake);
        let e = b.update_install(&fake).unwrap_err();
        assert_eq!(e.code, "E_INTERNAL");
        assert_eq!(update_lines(&b).last().map(String::as_str), Some("update install=install"));
        assert_eq!(record(&b), None);
        assert_eq!(b.update_status().unwrap().state, UpdateState::Ready { version: "0.2.0".into() });
    }

    /// The check runs once per run: a second app_ready (a reload) asks the updater nothing more.
    #[test]
    fn the_check_runs_once_per_run() {
        let b = backend();
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Ready { version: "0.2.0".into() });
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Ready { version: "0.2.0".into() });
        assert_eq!(fake.calls(), ["check", "download 0.2.0"]);
    }
}
```

(`fake_fs(&b).put(path, bytes)` is the fake file system's own writer, which bypasses failure injection; `b.sys().fs` is the `dyn Fs` and has no `put`.) In `open.rs`'s test module, replace `each_external_target_opens_what_the_spec_names`'s `Place::Url("…")` literals with `Place::Url("…".to_string())` (and in the two tests after it: `Place::Url(STORE_URL.to_string())`, `Place::Url(TERMINAL_URL.to_string())`, `Place::Url(ISSUES_URL.to_string())`, and the `match p { Place::Url(url) if *url == STORE_URL => …` arms become `if url == STORE_URL`), and add:

```rust
    /// Review Focus 4, spec §5.10: the release page is opened for a SemVer version and nothing
    /// else; the text of the link is written out here.
    #[test]
    fn the_release_url_takes_semver_only() {
        assert_eq!(
            release_url("0.2.0").as_deref(),
            Some("https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0")
        );
        assert_eq!(
            release_url("0.1.0-spike.2").as_deref(),
            Some("https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.1.0-spike.2")
        );
        for bad in ["", "v0.2.0", "0.2", "../x", "0.2.0/../../issues", "0.2.0 ", "01.2.0", "0.2.0?x=1"] {
            assert_eq!(release_url(bad), None, "{bad:?}");
            assert_eq!(
                external_places(ExternalTarget::ReleaseNotes { version: bad.to_string() }, Path::new("d"), Path::new("l")),
                Vec::<Place>::new(),
                "{bad:?}"
            );
        }
        assert_eq!(
            external_places(ExternalTarget::ReleaseNotes { version: "0.2.0".into() }, Path::new("d"), Path::new("l")),
            [Place::Url("https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0".to_string())]
        );
    }
```

and to `the_ui_can_only_name_a_target`: after the four unit targets, `let notes = ExternalTarget::ReleaseNotes { version: "0.2.0".into() }; assert_eq!(serde_json::to_string(&notes).unwrap(), r#"{"releaseNotes":{"version":"0.2.0"}}"#); assert_eq!(serde_json::from_str::<ExternalTarget>(r#"{"releaseNotes":{"version":"0.2.0"}}"#).unwrap(), notes);` and `r#"{"releaseNotes":"0.2.0"}"#` and `"\"releaseNotes\""` to the refused list.

In `config_guard.rs`, after `the_main_binary_is_the_default_run_of_the_app_crate`:

```rust
/// Spec §6.2, §5.10 (phase 4b): the updater's config, exactly: the one endpoint (GitHub's
/// "latest" download, which serves the newest published release), a quiet install, the public
/// key (empty until Task 6 fills it; TauriUpdater refuses to check with an empty one), and no
/// dangerous switch (the spike sets `dangerousInsecureTransportProtocol` through --config only).
#[test]
fn the_updater_config_is_the_one_endpoint_quiet_and_without_dangerous_switches() {
    let config: Value = serde_json::from_str(CONFIG).unwrap();
    let updater = config.pointer("/plugins/updater").and_then(Value::as_object).unwrap();
    // serde_json's map is ordered by key, not by the file: sorted names, exactly these three.
    let mut keys: Vec<&str> = updater.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["endpoints", "pubkey", "windows"]);
    assert_eq!(
        updater.get("endpoints"),
        Some(&serde_json::json!([
            "https://github.com/luraselenehalo/cmd-studio/releases/latest/download/latest.json"
        ]))
    );
    assert_eq!(updater.get("windows"), Some(&serde_json::json!({ "installMode": "quiet" })));
    let pubkey = updater.get("pubkey").and_then(Value::as_str).unwrap();
    assert!(pubkey.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='), "base64 or empty: {pubkey:?}");
    assert_eq!(config.pointer("/plugins").and_then(Value::as_object).map(|p| p.len()), Some(1), "the updater is the only plugin with a config");
    assert!(!CONFIG.contains("dangerous"), "no dangerous switch in the committed config");
    let merged: tauri::utils::config::Config = serde_json::from_value(config).unwrap();
    assert!(merged.plugins.0.contains_key("updater"));
}
```

In `lib.rs`'s test module, after `the_restarted_start_rolls_back_before_the_window`:

```rust
    /// Spec §6.2 (phase 4b): the record of an update is read right after the run record and
    /// before the window (so the first app_status has the notice), with the version the built
    /// config says; the plugin is registered after single-instance and the updater is Tauri
    /// state; at exit the automatic install runs before the run record ends.
    #[test]
    fn the_update_record_is_read_after_the_run_record_and_the_install_runs_at_exit() {
        let text = production();
        let context = text.find("let context = tauri::generate_context!()").unwrap();
        let version = text.find("context.package_info().version.to_string()").unwrap();
        let record = text.find("backend.start_run(how)").unwrap();
        let read = text.find("backend.update_after_start(&app_version)").unwrap();
        let window = text.find("open_main_window(app, ").unwrap();
        assert!(context < version && version < record, "the version comes from the context");
        assert!(record < read && read < window, "after the run record, before the window");
        let single = text.find("tauri_plugin_single_instance::init(").unwrap();
        let updater = text.find("tauri_plugin_updater::Builder::new().build()").unwrap();
        assert!(single < updater, "single-instance first, as its plugin asks");
        assert!(text.contains("app.manage(std::sync::Arc::new(update::TauriUpdater::new(app.handle().clone())))"));
        let exit = text.find("tauri::RunEvent::Exit").unwrap();
        let install = text.find("for_exit.update_at_exit(&update::install_quiet)").unwrap();
        let ended = text.find("for_exit.end_run()").unwrap();
        assert!(exit < install && install < ended, "the install, then the record's end");
        assert_eq!(text.matches(".build(context)").count(), 1);
    }
```

- [ ] **Step 2: Run them to see them fail**

Add `pub mod update;` to `lib.rs` (after `pub mod settings;`) and `pub mod update;` to `backend/mod.rs` (after `pub mod system;`). Run: `cargo test -p cmdstudio-app --lib update::`
Expected: compile errors (`is_newer`, `UpdateError`, `FakeUpdater`, `UpdateState` … not found).

- [ ] **Step 3: `tauri.conf.json`**

After the `"app"` block and before `"bundle"`:

```json
  "plugins": {
    "updater": {
      "pubkey": "",
      "endpoints": ["https://github.com/luraselenehalo/cmd-studio/releases/latest/download/latest.json"],
      "windows": { "installMode": "quiet" }
    }
  },
```

(`pubkey` is filled in Task 6. The plugin's `Config` requires the key to be present; an empty string deserializes, and `TauriUpdater::check` refuses it before any request.)

- [ ] **Step 4: `update.rs`**

```rust
//! The updater behind a trait (spec §6.2, phase 4b). [`Updater`] is what the backend drives
//! (`backend/update.rs`): a check, a download, and the plugin's own install for "Update now".
//! [`TauriUpdater`] is tauri-plugin-updater behind it; the tests' [`FakeUpdater`] answers what a
//! test scripts. The plugin is used from Rust only: the window gets no `updater:` permission
//! (`capabilities/default.json`, pinned by `ipc.rs`), so its JS commands cannot be called.
//! [`install_quiet`] is the automatic install at close: the app's own spawn of the verified file.
//!
//! Read on <date> in tauri-plugin-updater 2.13.3 (G-UPD): `Update::download` verifies the
//! minisign signature against the config's pubkey and returns the bytes (`src/updater.rs:<l>`);
//! `Update::install` on Windows writes the bytes to a temp file, starts it with
//! `/S /R /UPDATE /ARGS …` in quiet mode after `on_before_exit`, and ends the process with
//! `std::process::exit(0)` (`:<l>`); `check` answers `None` unless the comparator says the offered
//! version is newer (`:<l>`); `check` never reads the pubkey (only `download` does), which is why
//! [`TauriUpdater::check`] refuses an empty one first.
use serde_json::Value;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

/// A version `latest.json` offers that is newer than this build, with the release page it links.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Available {
    pub version: String,
    pub notes_url: String,
}

/// Why a step of the updater did not go through. The variant is what the log says
/// ([`UpdateError::kind`]), never the text, which may hold a URL or a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateError {
    /// No network, DNS, a server that did not answer or answered an error.
    Offline(String),
    /// `latest.json` could not be used: not JSON, no Windows target, a version that is not
    /// SemVer, or no public key to check it with.
    Manifest(String),
    /// The download's signature did not match the public key.
    Signature(String),
    /// The installer could not be started.
    Install(String),
}

impl UpdateError {
    pub fn kind(&self) -> &'static str {
        match self {
            UpdateError::Offline(_) => "offline",
            UpdateError::Manifest(_) => "manifest",
            UpdateError::Signature(_) => "signature",
            UpdateError::Install(_) => "install",
        }
    }
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Offline(why) => write!(f, "offline: {why}"),
            UpdateError::Manifest(why) => write!(f, "the update manifest could not be used: {why}"),
            UpdateError::Signature(why) => write!(f, "the signature did not match: {why}"),
            UpdateError::Install(why) => write!(f, "the installer could not be started: {why}"),
        }
    }
}

/// What the backend asks of the updater. `install_now` runs `before_exit` (the record and the
/// run's end) and then the plugin's install, which ends the process; it returns only when the
/// installer could not be started.
pub trait Updater {
    fn check(&self) -> Result<Option<Available>, UpdateError>;
    fn download(&self, available: &Available) -> Result<Vec<u8>, UpdateError>;
    fn install_now(
        &self,
        available: &Available,
        bytes: Vec<u8>,
        before_exit: &dyn Fn(),
    ) -> Result<(), UpdateError>;
}

/// U5's rule, set explicitly on the plugin's builder: only a strictly higher version is an update.
pub fn is_newer(current: &semver::Version, offered: &semver::Version) -> bool {
    offered > current
}

/// tauri-plugin-updater, read from `plugins.updater` of tauri.conf.json (the pubkey, the endpoint,
/// the quiet install mode). What the last `check` found is kept for `download` and `install_now`:
/// the plugin's `Update` holds the download URL and the signature `latest.json` gave.
pub struct TauriUpdater {
    app: AppHandle,
    found: Mutex<Option<Update>>,
}

impl TauriUpdater {
    pub fn new(app: AppHandle) -> Self {
        TauriUpdater {
            app,
            found: Mutex::new(None),
        }
    }

    /// The public key of `plugins.updater`; empty until the founder's key is in (Task 6).
    fn pubkey(&self) -> String {
        self.app
            .config()
            .plugins
            .0
            .get("updater")
            .and_then(|u| u.get("pubkey"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string()
    }
}

/// The plugin's error by its meaning (G-UPD (f): name the variants as `src/error.rs` of 2.13.3
/// spells them; `_` covers the rest). Network and I/O are `Offline`; the signature's decoding or
/// check is `Signature`; the manifest, the targets, the endpoint and the version are `Manifest`;
/// anything else happened at the install.
fn classify(e: tauri_plugin_updater::Error) -> UpdateError {
    use tauri_plugin_updater::Error;
    let text = e.to_string();
    match e {
        Error::Network(_) | Error::Reqwest(_) | Error::Io(_) => UpdateError::Offline(text),
        Error::Minisign(_) | Error::Base64(_) | Error::SignatureUtf8(_) => UpdateError::Signature(text),
        Error::ReleaseNotFound
        | Error::TargetsNotFound(_)
        | Error::Serialization(_)
        | Error::EmptyEndpoints
        | Error::InsecureTransportProtocol
        | Error::UnsupportedOs
        | Error::Semver(_)
        | Error::UrlParse(_) => UpdateError::Manifest(text),
        _ => UpdateError::Install(text),
    }
}

impl Updater for TauriUpdater {
    fn check(&self) -> Result<Option<Available>, UpdateError> {
        if self.pubkey().is_empty() {
            return Err(UpdateError::Manifest(
                "tauri.conf.json has no updater public key".to_string(),
            ));
        }
        let updater = self
            .app
            .updater_builder()
            .version_comparator(|current, release| is_newer(&current, &release.version))
            .build()
            .map_err(classify)?;
        let Some(update) = tauri::async_runtime::block_on(updater.check()).map_err(classify)?
        else {
            return Ok(None);
        };
        let notes_url = crate::open::release_url(&update.version).ok_or_else(|| {
            UpdateError::Manifest(format!("the offered version is not SemVer: {}", update.version))
        })?;
        let available = Available {
            version: update.version.clone(),
            notes_url,
        };
        *self.found.lock().unwrap_or_else(PoisonError::into_inner) = Some(update);
        Ok(Some(available))
    }

    fn download(&self, available: &Available) -> Result<Vec<u8>, UpdateError> {
        let found = self.found.lock().unwrap_or_else(PoisonError::into_inner);
        let update = found
            .as_ref()
            .filter(|u| u.version == available.version)
            .ok_or_else(|| {
                UpdateError::Manifest(format!("no check offered version {}", available.version))
            })?;
        tauri::async_runtime::block_on(update.download(|_, _| {}, || {})).map_err(classify)
    }

    fn install_now(
        &self,
        available: &Available,
        bytes: Vec<u8>,
        before_exit: &dyn Fn(),
    ) -> Result<(), UpdateError> {
        let found = self.found.lock().unwrap_or_else(PoisonError::into_inner);
        let update = found
            .as_ref()
            .filter(|u| u.version == available.version)
            .ok_or_else(|| {
                UpdateError::Manifest(format!("no check offered version {}", available.version))
            })?;
        before_exit();
        update.install(bytes).map_err(classify)
    }
}

/// The automatic install at close (spec §6.2, the primary; U3): the verified installer in
/// `updates\`, started silently (`/S`) as an update (`/UPDATE`: the installer keeps the user's
/// shortcuts as they are and skips a WebView2 download, G-NSI-U), never waited for, with the
/// child's stdio closed. No `/R`: the app does not come back by itself.
#[cfg(windows)]
pub fn install_quiet(file: &Path) -> Result<(), String> {
    use std::process::Stdio;
    std::process::Command::new(file)
        .args(["/S", "/UPDATE"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{} could not be started: {e}", file.display()))
}

#[cfg(not(windows))]
pub fn install_quiet(_file: &Path) -> Result<(), String> {
    Err("CMD Studio runs on Windows only".to_string())
}

/// The check for a new version, once the window is up (`app_ready`, spec §6.2), on a thread of
/// its own: the window never waits for the network. The backend runs it once per run. A thread
/// that cannot start is one log line; the next start tries again.
#[cfg(windows)]
pub fn start_check(
    app: &AppHandle,
    backend: std::sync::Arc<crate::backend::Backend<crate::machine::RealMachine>>,
) {
    use tauri::Manager;
    let updater = app.state::<std::sync::Arc<TauriUpdater>>().inner().clone();
    let for_thread = backend.clone();
    let spawned = std::thread::Builder::new()
        .name("update-check".to_string())
        .spawn(move || {
            for_thread.update_check_and_download(updater.as_ref());
        });
    if let Err(e) = spawned {
        backend.note_line(&format!("update check=thread {e}"));
    }
}

/// The tests' updater: answers what the test scripted and records what was asked, in order.
#[cfg(test)]
pub(crate) struct FakeUpdater {
    pub check: std::cell::RefCell<Result<Option<Available>, UpdateError>>,
    pub bytes: std::cell::RefCell<Result<Vec<u8>, UpdateError>>,
    pub install: std::cell::RefCell<Result<(), UpdateError>>,
    calls: std::cell::RefCell<Vec<String>>,
}

#[cfg(test)]
impl FakeUpdater {
    /// Offers `version` and the bytes `MZ-the-installer` (16 bytes) for it.
    pub fn offering(version: &str) -> Self {
        let notes_url = crate::open::release_url(version).unwrap_or_default();
        FakeUpdater {
            check: std::cell::RefCell::new(Ok(Some(Available {
                version: version.to_string(),
                notes_url,
            }))),
            bytes: std::cell::RefCell::new(Ok(b"MZ-the-installer".to_vec())),
            install: std::cell::RefCell::new(Ok(())),
            calls: std::cell::RefCell::new(Vec::new()),
        }
    }
    /// Up to date: the check offers nothing.
    pub fn none() -> Self {
        let fake = Self::offering("0.0.0");
        *fake.check.borrow_mut() = Ok(None);
        fake
    }
    /// The check fails with `e`.
    pub fn failing(e: UpdateError) -> Self {
        let fake = Self::offering("0.0.0");
        *fake.check.borrow_mut() = Err(e);
        fake
    }
    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

#[cfg(test)]
impl Updater for FakeUpdater {
    fn check(&self) -> Result<Option<Available>, UpdateError> {
        self.calls.borrow_mut().push("check".to_string());
        self.check.borrow().clone()
    }
    fn download(&self, available: &Available) -> Result<Vec<u8>, UpdateError> {
        self.calls
            .borrow_mut()
            .push(format!("download {}", available.version));
        self.bytes.borrow().clone()
    }
    fn install_now(
        &self,
        available: &Available,
        bytes: Vec<u8>,
        before_exit: &dyn Fn(),
    ) -> Result<(), UpdateError> {
        self.calls
            .borrow_mut()
            .push(format!("install {} {}", available.version, bytes.len()));
        before_exit();
        self.install.borrow().clone()
    }
}
```

`Config.plugins` is `PluginConfig(pub HashMap<String, JsonValue>)` (`tauri-utils-2.10.2/src/config.rs:4150`); `AppHandle::config()` is `Manager::config`, so `use tauri::Manager;` goes at the top when the compiler asks. `tauri::async_runtime::block_on` runs the plugin's futures on our own thread (never the window's: `start_check` and `update_install`'s worker). `UpdaterExt` is implemented for every `Manager`, `AppHandle` included. If `Update`'s `version` field is not public in 2.13.3 (it is `pub version: String` in 2.10.1), keep the version beside the `Update` in the `found` slot as `(String, Update)`.

- [ ] **Step 5: `backend/update.rs`, the backend's field, `app_ready`**

```rust
//! The updater's state in the app (spec §6.2, §2.1, §5.5, phase 4b): the check after the window
//! is up, the download in the background, the install at close (the own spawn, when the lock is
//! free) or now (the plugin, on the user's click), the record `<data>\update.json` written before
//! either, read at the next start for "Updated to vX" and deleted once the UI has shown it. Every
//! step writes one log line of names, versions and codes, never a URL or a path: the updater is
//! silent on screen until an update happened. The machine is never touched here: the installer
//! does that, and only after the lock was taken.
use super::{Backend, Machine};
use crate::error::AppError;
use crate::settings::UpdateMode;
use crate::update::{Available, Updater};
use cmdstudio_core::adapters::FsLock;
use cmdstudio_core::resources::sha256_hex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use ts_rs::TS;

/// The version of `update.json` this build writes and reads.
pub const UPDATE_RECORD_VERSION: u32 = 1;
/// The name `app_status.busy` carries while the re-apply after an update runs (Task 5; recovery
/// spec §6.3). A name of its own, like `recovery_entry`: it is no IPC command.
pub const UPDATE_REAPPLY_BUSY: &str = "update_reapply";

/// `<data>\update.json` (spec §5.3, §6.2): written before the installer starts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecord {
    pub version: u32,
    pub from: String,
    pub to: String,
    pub notes_url: String,
}

/// Where the updater is, for `update_status`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum UpdateState {
    /// `UpdateMode::Off`: nothing was asked.
    Off,
    /// No update, or a check that failed (one log line).
    #[default]
    Idle,
    Checking,
    /// Notify mode: a newer version waits for "Update now".
    Available { version: String },
    Downloading { version: String },
    /// Automatic mode: downloaded and verified; installed at close.
    Ready { version: String },
    /// The installer was started (at close, or now).
    Installing { version: String },
    /// The install at close did not run: `busy` (the lock), `lock` (the lock file), `record`
    /// (update.json could not be written). The next launch tries again.
    Postponed { version: String, reason: String },
}

/// `update_status` (spec §6.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateStatus {
    pub mode: UpdateMode,
    pub state: UpdateState,
    #[ts(type = "number | null")]
    pub checked_at_ms: Option<u64>,
}

/// The notice of the next start (spec §2.1): `app_status.updated`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Updated {
    pub from: String,
    pub to: String,
    pub notes_url: String,
}

/// A verified download: the file and the hash of the bytes the plugin verified, which the file
/// must still match before it is started (Review Focus 1).
struct Ready {
    available: Available,
    file: PathBuf,
    sha256: String,
}

#[derive(Default)]
struct UpdateInner {
    /// The check ran (or was refused by the mode) in this run: once per run.
    checked: bool,
    state: UpdateState,
    checked_at_ms: Option<u64>,
    /// This run's version, as `update_after_start` was told it.
    current: Option<String>,
    available: Option<Available>,
    ready: Option<Ready>,
    updated: Option<Updated>,
}

/// The updater's state of this run, behind one lock (a poisoned lock is used as it is).
pub(crate) struct UpdateSlot {
    inner: Mutex<UpdateInner>,
}

impl UpdateSlot {
    pub(crate) fn new() -> Self {
        UpdateSlot {
            inner: Mutex::new(UpdateInner::default()),
        }
    }
    fn with<T>(&self, f: impl FnOnce(&mut UpdateInner) -> T) -> T {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut inner)
    }
}

impl<M: Machine> Backend<M> {
    fn set_update_state(&self, state: UpdateState) -> UpdateState {
        self.update.with(|u| u.state = state.clone());
        state
    }

    /// `updates\cmdstudio-<version>-setup.exe`: the version is SemVer (checked when it was
    /// offered), so the name holds nothing a path would read.
    fn update_file_for(&self, version: &str) -> PathBuf {
        self.paths()
            .updates_dir()
            .join(format!("cmdstudio-{version}-setup.exe"))
    }

    /// The check and, in automatic mode, the download (spec §6.2), once per run, after the window
    /// is up (`update::start_check`). Off is one log line. A check that fails is one log line and
    /// `Idle`: nothing is shown. Notify stops at `Available`; Auto downloads, verifies (the
    /// updater), writes the file in one step and is `Ready` for the close.
    pub fn update_check_and_download(&self, updater: &dyn Updater) -> UpdateState {
        let first = self.update.with(|u| {
            let first = !u.checked;
            u.checked = true;
            first
        });
        if !first {
            return self.update.with(|u| u.state.clone());
        }
        let mode = self.settings_or_default().updates;
        if mode == UpdateMode::Off {
            self.note_line("update mode=off");
            return self.set_update_state(UpdateState::Off);
        }
        self.set_update_state(UpdateState::Checking);
        let checked = updater.check();
        let now = self.now_ms();
        self.update.with(|u| u.checked_at_ms = Some(now));
        let available = match checked {
            Err(e) => {
                self.note_line(&format!("update check={}", e.kind()));
                return self.set_update_state(UpdateState::Idle);
            }
            Ok(None) => {
                self.note_line("update check=none");
                return self.set_update_state(UpdateState::Idle);
            }
            Ok(Some(available)) => available,
        };
        self.note_line(&format!("update available={}", available.version));
        self.update.with(|u| u.available = Some(available.clone()));
        if mode == UpdateMode::Notify {
            return self.set_update_state(UpdateState::Available {
                version: available.version,
            });
        }
        self.set_update_state(UpdateState::Downloading {
            version: available.version.clone(),
        });
        match self.update_download(updater, &available) {
            Ok(()) => {
                self.note_line(&format!("update downloaded={}", available.version));
                self.set_update_state(UpdateState::Ready {
                    version: available.version,
                })
            }
            Err(kind) => {
                self.note_line(&format!("update download={kind}"));
                self.set_update_state(UpdateState::Idle)
            }
        }
    }

    /// Downloads and verifies through the updater, writes the bytes to `updates\` in one step and
    /// keeps their hash, so that the file is checked again before it is ever started.
    fn update_download(
        &self,
        updater: &dyn Updater,
        available: &Available,
    ) -> Result<(), &'static str> {
        let bytes = updater.download(available).map_err(|e| e.kind())?;
        let file = self.update_file_for(&available.version);
        self.sys()
            .fs
            .write_atomic(&file, &bytes)
            .map_err(|_| "write")?;
        let sha256 = sha256_hex(&bytes);
        self.update.with(|u| {
            u.ready = Some(Ready {
                available: available.clone(),
                file,
                sha256,
            })
        });
        Ok(())
    }

    pub fn update_status(&self) -> Result<UpdateStatus, AppError> {
        let mode = self.settings_or_default().updates;
        Ok(self.update.with(|u| UpdateStatus {
            mode,
            state: u.state.clone(),
            checked_at_ms: u.checked_at_ms,
        }))
    }

    /// The notice of this start, when the last run's update went through (`update_after_start`).
    pub fn updated(&self) -> Option<Updated> {
        self.update.with(|u| u.updated.clone())
    }

    /// The lock of spec §5.5, tried once: `E_BUSY` while another change runs.
    fn update_lock(&self) -> Result<FsLock, AppError> {
        match self.sys().fs.try_lock(&self.paths().lock_file()) {
            Ok(Some(lock)) => Ok(lock),
            Ok(None) => Err(AppError::coded(
                "E_BUSY",
                "cmdstudio.lock is held: a change is running, so the update waits",
            )),
            Err(e) => Err(AppError::coded(
                "E_DATA_WRITE",
                format!("CMD Studio's lock file could not be opened: {e}"),
            )),
        }
    }

    /// The downloaded file's bytes, when it still is the file that was verified (Review Focus 1);
    /// one that changed or went is deleted, said, and forgotten, so the caller downloads again.
    fn update_ready_bytes(&self) -> Option<Vec<u8>> {
        let (file, sha256) = self
            .update
            .with(|u| u.ready.as_ref().map(|r| (r.file.clone(), r.sha256.clone())))?;
        let bytes = self.sys().fs.read(&file).ok().flatten();
        match bytes {
            Some(bytes) if sha256_hex(&bytes) == sha256 => Some(bytes),
            _ => {
                let _ = self.sys().fs.remove_file(&file);
                self.update.with(|u| u.ready = None);
                self.note_line("update install=file-changed");
                None
            }
        }
    }

    fn update_record_for(&self, available: &Available) -> UpdateRecord {
        let from = self
            .update
            .with(|u| u.current.clone())
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
        UpdateRecord {
            version: UPDATE_RECORD_VERSION,
            from,
            to: available.version.clone(),
            notes_url: available.notes_url.clone(),
        }
    }

    /// `update.json`, in one step; whether it was written.
    fn write_update_record(&self, record: &UpdateRecord) -> bool {
        let Ok(mut bytes) = serde_json::to_vec_pretty(record) else {
            return false;
        };
        bytes.push(b'\n');
        self.sys()
            .fs
            .write_atomic(&self.paths().update_file(), &bytes)
            .is_ok()
    }

    /// "Update now" (the notify card, the Recovery page; spec §6.2): the offered version is
    /// downloaded if it is not yet, the lock is taken (E_BUSY while a change runs), the record is
    /// written and the run ended inside the updater's hook, and the plugin's install starts the
    /// installer silently with a relaunch and ends this process. Answers only on a failure: the
    /// record is taken back then, and the state is as before.
    pub fn update_install(&self, updater: &dyn Updater) -> Result<(), AppError> {
        let Some(available) = self.update.with(|u| u.available.clone()) else {
            return Err(AppError::internal("no update is available to install"));
        };
        let _lock = self.update_lock()?;
        let bytes = match self.update_ready_bytes() {
            Some(bytes) => bytes,
            None => updater.download(&available).map_err(|e| {
                self.note_line(&format!("update download={}", e.kind()));
                AppError::internal(format!("the update could not be downloaded: {e}"))
            })?,
        };
        self.note_line(&format!("update installing={} how=now", available.version));
        self.set_update_state(UpdateState::Installing {
            version: available.version.clone(),
        });
        let record = self.update_record_for(&available);
        let started = updater.install_now(&available, bytes, &|| {
            self.write_update_record(&record);
            self.end_run();
        });
        if let Err(e) = started {
            let _ = self.sys().fs.remove_file(&self.paths().update_file());
            self.note_line(&format!("update install={}", e.kind()));
            let before = match self.update.with(|u| u.ready.is_some()) {
                true => UpdateState::Ready {
                    version: available.version.clone(),
                },
                false => UpdateState::Available {
                    version: available.version.clone(),
                },
            };
            self.set_update_state(before);
            return Err(AppError::internal(format!(
                "the update could not be installed: {e}"
            )));
        }
        Ok(())
    }

    /// At close (spec §6.2's primary; `lib.rs` on `RunEvent::Exit`): when a verified download is
    /// ready and the lock is free, the record is written and the file started with `spawn`
    /// (`update::install_quiet`); the lock is held into the start and let go with the process.
    /// Busy, a lock file that cannot be opened, or a record that cannot be written postpone the
    /// install to the next launch, each with one log line; a file that changed is never started.
    pub fn update_at_exit(&self, spawn: &dyn Fn(&Path) -> Result<(), String>) {
        let Some(available) = self.update.with(|u| u.ready.as_ref().map(|r| r.available.clone()))
        else {
            return;
        };
        let postponed = |reason: &str| {
            self.note_line(&format!("update postponed={reason}"));
            self.set_update_state(UpdateState::Postponed {
                version: available.version.clone(),
                reason: reason.to_string(),
            });
        };
        let _lock = match self.sys().fs.try_lock(&self.paths().lock_file()) {
            Ok(Some(lock)) => lock,
            Ok(None) => return postponed("busy"),
            Err(_) => return postponed("lock"),
        };
        if self.update_ready_bytes().is_none() {
            return;
        }
        let file = self.update_file_for(&available.version);
        if !self.write_update_record(&self.update_record_for(&available)) {
            return postponed("record");
        }
        match spawn(&file) {
            Ok(()) => {
                self.note_line(&format!(
                    "update installing={} how=exit",
                    available.version
                ));
                self.set_update_state(UpdateState::Installing {
                    version: available.version,
                });
            }
            Err(_) => {
                let _ = self.sys().fs.remove_file(&self.paths().update_file());
                self.note_line("update install=spawn");
            }
        }
    }

    /// After the run record and before the window (`lib.rs`): the last run's download is thrown
    /// away (never trusted across runs), and `update.json`, when there, makes the notice if its
    /// `to` is this run's version (`update installed to=…`); else the installer did not finish
    /// (`update failed to=…`), and the record goes. A record that cannot be read stays and is
    /// said; one that is not this version's, not JSON, or names a version that is not SemVer is
    /// damaged: said and deleted.
    pub fn update_after_start(&self, current_version: &str) {
        self.update
            .with(|u| u.current = Some(current_version.to_string()));
        let fs = self.sys().fs;
        let updates = self.paths().updates_dir();
        if fs.exists(&updates) && fs.remove_dir_all(&updates).is_err() {
            self.note_line("update updates=kept");
        }
        let file = self.paths().update_file();
        let record = match fs.read(&file) {
            Err(_) => return self.note_line("update record=unreadable"),
            Ok(None) => return,
            Ok(Some(bytes)) => serde_json::from_slice::<UpdateRecord>(&bytes)
                .ok()
                .filter(|r| r.version == UPDATE_RECORD_VERSION)
                .and_then(|r| crate::open::release_url(&r.to).map(|url| (r, url))),
        };
        let Some((record, notes_url)) = record else {
            let _ = fs.remove_file(&file);
            return self.note_line("update record=damaged");
        };
        if record.to != current_version {
            self.note_line(&format!(
                "update failed to={} running={current_version}",
                record.to
            ));
            let _ = fs.remove_file(&file);
            return;
        }
        self.note_line(&format!("update installed to={}", record.to));
        self.update.with(|u| {
            u.updated = Some(Updated {
                from: record.from,
                to: record.to,
                notes_url,
            })
        });
    }

    /// `app_ready`: the UI drew the notice, so the record goes (spec §2.1: once); the notice
    /// stays for the run.
    pub(crate) fn update_record_consumed(&self) {
        if self.updated().is_some() {
            let _ = self.sys().fs.remove_file(&self.paths().update_file());
        }
    }
}
```

In `backend/mod.rs`: `pub mod update;` after `pub mod system;`; the field `/// The updater's state of this run (spec §6.2, phase 4b).` `update: update::UpdateSlot,` after `run`; `update: update::UpdateSlot::new(),` in `new()`. In `backend/run.rs`'s `app_ready`, after the `if let Some(mine) = mine { self.write_record(&mine); }` block: `self.update_record_consumed();`.

- [ ] **Step 6: `open.rs`**

`ExternalTarget` loses `Copy` and gains the variant, after `LogsFolder`:

```rust
    /// The release page of a version (spec §6.2's "what changed"): `releases/tag/v<version>`,
    /// for a `version` that parses as SemVer and nothing else (`release_url`).
    ReleaseNotes { version: String },
```

`Place::Url(&'static str)` becomes `Place::Url(String)`; the three `Place::Url(STORE_URL)`-style arms of `external_places` become `Place::Url(STORE_URL.to_string())` etc. After `ISSUES_URL`:

```rust
pub const RELEASES_URL: &str = "https://github.com/luraselenehalo/cmd-studio/releases";

/// The release page of `version`: only a string `semver` parses (no `v`, no path, no query, no
/// leading zero), so that nothing from a manifest or a record reaches the shell but a version.
pub fn release_url(version: &str) -> Option<String> {
    semver::Version::parse(version)
        .ok()
        .map(|_| format!("{RELEASES_URL}/tag/v{version}"))
}
```

`external_places` gains the arm `ExternalTarget::ReleaseNotes { version } => release_url(&version).map(Place::Url).into_iter().collect(),` (an empty list for a version that is not SemVer). `open_external` says it: before `open_first`, `if places.is_empty() { return Err(format!("{target:?} names nothing that can be opened")); }` (the version's text is in the `Debug` form; it is the record's or the manifest's, never a path). The module doc's "one of four targets" becomes "one of five". `the_ui_can_only_name_a_target`'s comment: "the five names".

- [ ] **Step 7: `lib.rs`**

In `run_gui`, after the `let backend = …` match and before `let decided`:

```rust
    // The built config, read once: the version the updater compares and records is this one
    // (Cargo's, or a --config override's in the spike), never a second constant.
    let context = tauri::generate_context!();
    let app_version = context.package_info().version.to_string();
```

After `let decided = backend.start_run(how);`:

```rust
    // The record of an update (spec §6.2, phase 4b), before the window: the first app_status has
    // the notice. The last run's download goes here too.
    backend.update_after_start(&app_version);
```

In the builder chain, after the single-instance plugin: `.plugin(tauri_plugin_updater::Builder::new().build())` with the comment `// The updater (spec §6.2, phase 4b), used from Rust only (update.rs); the window gets none of its commands.` In `setup`, first line: `app.manage(std::sync::Arc::new(update::TauriUpdater::new(app.handle().clone())));`. `.build(tauri::generate_context!())` becomes `.build(context)`. The run closure:

```rust
        Ok(app) => app.run(move |_, event| {
            if let tauri::RunEvent::Exit = event {
                // The automatic install, when a verified download waits and the lock is free
                // (spec §6.2's primary), then the run's record.
                for_exit.update_at_exit(&update::install_quiet);
                for_exit.end_run();
            }
        }),
```

The `#[cfg(not(windows))] run_gui` is unchanged. `update_after_start` is a method on `Backend<RealMachine>` through `Arc` deref, like `start_run`.

- [ ] **Step 8: Run the tests**

Run: `cargo test -p cmdstudio-app --lib update:: open:: config_guard:: lib`
Expected: the 4 tests of `update.rs`, the 12 of `backend/update.rs`, `open.rs`'s 6 (two changed, one new), `config_guard`'s new test and `lib.rs`'s new text test pass; `ipc.rs`'s tests still pass (no command changed yet; the capability test counts 37 until Task 4).

- [ ] **Step 9: The checks, then commit**

The four clippy runs, `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo build -p cmdstudio-app` (the plugin links; no `resources/` needed). Do not run the app.

Write `.git/CMDSTUDIO_COMMIT_MSG`:

```
app: the updater behind a trait, its state machine and update.json (spec §6.2, phase 4b)

tauri-plugin-updater =2.13.3 (Rust only; the window gets none of its
commands) behind update::Updater, with TauriUpdater (an empty pubkey is
refused before any request; the version comparator set explicitly),
FakeUpdater for the tests, install_quiet (the own spawn: /S /UPDATE,
no relaunch, stdio closed) and start_check (a thread after app_ready).
backend/update.rs: the state machine (Off, Idle, Checking, Available,
Downloading, Ready, Installing, Postponed) over the mode of
settings.json; the download written to updates\ with its hash, which
the file must still match before it is started; update_at_exit under
the lock (busy postpones), update_install (the plugin's install with
the record and the run's end in its hook), update_after_start (the
record read after the run record: "installed to", "failed to",
damaged or unreadable), app_ready consumes the record. open.rs:
ExternalTarget::ReleaseNotes for a SemVer version only. tauri.conf.json:
plugins.updater with the one endpoint, a quiet install and an empty
pubkey until the founder's key, pinned by config_guard. lib.rs: the
plugin, the updater as Tauri state, the record before the window, the
install at exit before the run's end.
```

Then `git add -- app/src-tauri/Cargo.toml Cargo.lock app/src-tauri/src/update.rs app/src-tauri/src/backend/update.rs app/src-tauri/src/backend/mod.rs app/src-tauri/src/backend/run.rs app/src-tauri/src/open.rs app/src-tauri/src/lib.rs app/src-tauri/src/config_guard.rs app/src-tauri/tauri.conf.json`, check `git diff --cached --name-only`, then `git commit -F .git/CMDSTUDIO_COMMIT_MSG`.

- [ ] **Step 10: After the spike (Task 1's `results.md`)**

The primary confirmed: nothing to change; Task 8 records it. F2 chosen: `update_at_exit` and `lib.rs`'s exit call go; `update_check_and_download` ends, after `Ready`, by asking the window `update_can_install` (a third command, Task 4's shape: read-only, answers whether `app_status.busy` is null and no editor holds an unsaved change) and, on yes, calling `update_install` (the plugin's path, with its relaunch); `auto_downloads_and_is_ready_at_exit` becomes `auto_downloads_and_installs_when_idle`; `the_own_spawn_is_silent_and_does_not_relaunch` goes with `install_quiet`. F3 chosen: `update_at_exit` and the exit call go; `Ready` is shown by Task 4's card as "v<X> is ready: restart CMD Studio to install it" with "Update now". Either is one commit `app: the install moment after the spike (phase 4b Task 1: <F2|F3>)`.

---
### Task 4: The two IPC commands, `app_status.updated`, the bindings, the mock, and the UI

Runs after Task 3. `update_status` (read only) and `update_install` (ends the process, like `app_restart`), `app_ready` starting the check, `app_status.updated`, the bindings regenerated by the manual steps, the mock's `update` option, and the four places of the UI: the "Updated to vX" card above every screen, the notify card on the home page, the Recovery page's "Update now" line, the Advanced page's "Last checked", and the Settings note's new words.

**Files:**
- Modify: `app/src-tauri/src/command_names.rs` (`UPDATE_STATUS`, `UPDATE_INSTALL`, `COMMANDS`, the doc's count)
- Modify: `app/src-tauri/src/ipc.rs` (the two commands; `app_ready` starts the check; the tests' counts 39 and 35; a text test)
- Modify: `app/src-tauri/capabilities/default.json` (`allow-update-status`, `allow-update-install`; the description's list)
- Modify: `app/src-tauri/src/backend/run.rs` (`AppStatus.updated`; `app_status` fills it; its tests' literals)
- Modify: `app/ui/src/bindings/AppStatus.ts`, `ExternalTarget.ts`; Create: `UpdateState.ts`, `UpdateStatus.ts`, `Updated.ts` (regenerated, never written by hand)
- Modify: `app/ui/src/ipc/types.ts`, `tauri.ts`, `mock.ts`, `work.ts`, `mock.test.ts`, `work.test.ts`
- Modify: `app/ui/src/App.tsx` (`UpdatedNotice` in `StartNotices`, `updateClosed`, `updateLater`), `app/ui/src/state.tsx` (`updateLater`, `markUpdateLater`)
- Create: `app/ui/src/components/UpdateOffer.tsx` (the notify card and the Recovery line, one component)
- Modify: `app/ui/src/screens/Home.tsx`, `Recovery.tsx`, `Advanced.tsx`, `Settings.tsx` (no code change: the note's key keeps its name)
- Modify: `app/ui/src/i18n/en.ts`, `th.ts`, `i18n.test.ts`, `app/ui/src/App.test.tsx`

**Interfaces:**
- Consumes: `Backend::{update_status, update_install, updated}`, `update::{TauriUpdater, start_check}`, `UpdateStatus`, `UpdateState`, `Updated` (Task 3).
- Produces: the IPC `update_status() -> UpdateStatus`, `update_install() -> ()`; `AppStatus.updated: Option<Updated>`; `Ipc.updateStatus(): Promise<UpdateStatus>`, `Ipc.updateInstall(): Promise<void>`; `MockOptions.update?: "off" | "idle" | "available" | "ready" | "updated"`, `MockOptions.updateVersion?: string` (default `"0.2.0"`), `MockState.update`, `MockState.updated`, `MockState.updateInstalls`; `AppState.updateLater`, `AppState.markUpdateLater`; `UpdateOffer({ status, where })`; the keys `start.updated`, `start.whatChanged`, `home.update.available`, `home.update.now`, `home.update.later`, `home.update.ready`, `home.update.installing`, `recovery.update.available`, `recovery.update.none`, `advanced.update.checked`, `advanced.update.notChecked`, `work.updating`, and the new text of `settings.updates.note`.

- [ ] **Step 1: The Rust side's failing tests**

In `ipc.rs`'s tests: `assert_eq!(COMMANDS.len(), 37)` → `39`; `assert_eq!(logging, 33)` → `35`; the module doc's "33 of the 37" → "35 of the 39". Add:

```rust
    /// Spec §6.2 (phase 4b): the check for a new version starts from app_ready, after the run is
    /// noted ready and off the window's thread, and the install-now command hands the plugin's
    /// updater to the backend.
    #[test]
    fn the_check_starts_after_app_ready_and_the_install_uses_the_plugin() {
        let start = SOURCE.find("pub async fn app_ready(").unwrap();
        let body = SOURCE.get(start..).unwrap().split_once("\n}\n").unwrap().0;
        let logged = body.find(".logged(names::APP_READY").unwrap();
        let check = body.find("crate::update::start_check(&app, ").unwrap();
        assert!(logged < check, "the record first, then the check");
        let start = SOURCE.find("pub async fn update_install(").unwrap();
        let body = SOURCE.get(start..).unwrap().split_once("\n}\n").unwrap().0;
        assert!(body.contains("TauriState<'_, Arc<TauriUpdater>>"));
        assert!(body.contains("b.update_install(u.as_ref())"));
    }
```

In `backend/run.rs`'s tests, every `AppStatus { … }` literal and `toEqual`-style comparison that lists the fields gains `updated: None` (grep `recovery_entry:` in that test module); `the_status_names_the_change_that_runs` adds, at its end:

```rust
        // The notice of an update rides on the same status (spec §2.1).
        fake_fs(&b).put(
            b.paths().update_file(),
            br#"{"version":1,"from":"0.1.0","to":"0.2.0","notesUrl":"https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0"}"#,
        );
        b.update_after_start("0.2.0");
        assert_eq!(b.app_status().unwrap().updated.map(|u| u.to), Some("0.2.0".to_string()));
```

Run: `cargo test -p cmdstudio-app --lib ipc:: run::`
Expected: the counts fail (37 ≠ 39), the new text test fails (`update_install` not found), `updated` is not a field.

- [ ] **Step 2: The commands**

`command_names.rs`: after `APP_RESTART`, `pub const UPDATE_STATUS: &str = "update_status";` and `pub const UPDATE_INSTALL: &str = "update_install";`; both at the end of `COMMANDS`; the doc comment "The 37 IPC command names" → "The 39 IPC command names", and its first paragraph gains "and `update_status`, `update_install` (phase 4b, spec §6.2)".

`ipc.rs`: the imports gain `use crate::backend::update::UpdateStatus;` and `use crate::update::TauriUpdater;`. `app_ready` becomes:

```rust
/// The UI drew its first page (recovery spec §5.1); then the check for a new version starts
/// (spec §6.2, phase 4b), on a thread of its own, once per run.
#[tauri::command]
pub async fn app_ready(app: AppHandle, state: TauriState<'_, Shared>) -> Result<(), AppError> {
    let b = state.inner().clone();
    let ready = blocking(move || b.logged(names::APP_READY, |b| b.app_ready())).await;
    crate::update::start_check(&app, state.inner().clone());
    ready
}
```

After `app_restart`:

```rust
/// Where the updater is and when it last checked (spec §6.2): read only.
#[tauri::command]
pub async fn update_status(state: TauriState<'_, Shared>) -> Result<UpdateStatus, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::UPDATE_STATUS, |b| b.update_status())).await
}

/// "Update now" (the notify card, the Recovery page; spec §6.2): the offered version is
/// downloaded if it is not yet, the record is written, the run ended, and the installer started
/// silently with a relaunch through the plugin, which ends this process; the app comes back by
/// itself. Answers only when it could not: E_BUSY while a change runs, E_INTERNAL otherwise.
#[tauri::command]
pub async fn update_install(
    state: TauriState<'_, Shared>,
    updater: TauriState<'_, Arc<TauriUpdater>>,
) -> Result<(), AppError> {
    let b = state.inner().clone();
    let u = updater.inner().clone();
    blocking(move || b.logged(names::UPDATE_INSTALL, |b| b.update_install(u.as_ref()))).await
}
```

Both at the end of `generate_handler![…]`. `capabilities/default.json`: `"allow-update-status"`, `"allow-update-install"` at the end of `permissions`; the description's list gains "update_status and update_install". `backend/run.rs`: `AppStatus` gains `/// The last run's update went through (spec §2.1): the notice, until closed.` `pub updated: Option<super::update::Updated>,` after `recovery_entry`; `app_status` fills `updated: self.updated(),` (outside the `self.run.with` closure: `let updated = self.updated();` first).

Run: `cargo test -p cmdstudio-app --lib ipc:: run::`
Expected: pass (39 commands, 35 logging, the capability's set equal).

- [ ] **Step 3: The bindings**

Delete `app/ui/src/bindings/AppStatus.ts` and `ExternalTarget.ts`; then, from `app/src-tauri`, with `TS_RS_EXPORT_DIR` set to `<repo>\app\ui\src\bindings` (PowerShell: `$env:TS_RS_EXPORT_DIR = '<repo>\app\ui\src\bindings'`), run `cargo test -p cmdstudio-app --lib export_bindings`. Expected: `AppStatus.ts` (with `updated: Updated | null`), `ExternalTarget.ts` (`"windowsTerminalStore" | "gitHubIssues" | "dataFolder" | "logsFolder" | { "releaseNotes": { version: string, } }`), `UpdateState.ts`, `UpdateStatus.ts` and `Updated.ts` are written; `git status --porcelain app/ui/src/bindings` lists exactly those five. `UpdateMode.ts` is unchanged.

- [ ] **Step 4: The UI's failing tests**

`app/ui/src/i18n/i18n.test.ts`: remove `"home.update.placeholder"` and `"recovery.update.placeholder"` from `SAME_IN_BOTH`; the test "have no empty text except the reserved update notice" becomes "have no empty text" with its `continue` line removed. Add:

```ts
  it("say what each update mode does, and name the version an update brought (phase 4b)", () => {
    expect(en["settings.updates.note"]).toBe(
      "CMD Studio checks for a new version each time it starts. Automatic downloads it in the background and installs it when you close CMD Studio; Tell me first shows it on the home page and waits for you; Off never checks. After an update, CMD Studio says which version it is now.",
    );
    expect(th["settings.updates.note"]).toMatch(/^CMD Studio ตรวจหาเวอร์ชันใหม่ทุกครั้งที่เริ่มทำงาน /);
    expect(en["start.updated"]).toBe("Updated to v{version}.");
    expect(en["home.update.now"]).toBe("Update now");
    expect(en).not.toHaveProperty("home.update.placeholder");
    expect(en).not.toHaveProperty("recovery.update.placeholder");
  });
```

`app/ui/src/ipc/work.test.ts`, in "names every method of Ipc once, with its kind and its task": `expect(KINDS.updateStatus).toBe("read"); expect(KINDS.updateInstall).toBe("exit");` and the `exit` count: `expect(Object.values(KINDS).filter((k) => k === "exit")).toHaveLength(2);`.

`app/ui/src/ipc/mock.test.ts`: in "answers the app's status as fixed data…", the `toEqual({ busy: null, … recoveryEntry: null })` literal gains `updated: null`. Add:

```ts
  it("answers the updater from its option, installs on Update now, and says updated (phase 4b)", async () => {
    const plain = createMockIpc();
    expect(await plain.updateStatus()).toEqual({ mode: "auto", state: { kind: "idle" }, checkedAtMs: 1791694709000 });
    await expect(plain.updateInstall()).rejects.toMatchObject({ code: "E_INTERNAL" });
    const off = createMockIpc({ update: "off", settings: { updates: "off" } });
    expect((await off.updateStatus()).state).toEqual({ kind: "off" });
    const ready = createMockIpc({ update: "ready", updateVersion: "0.3.0" });
    expect((await ready.updateStatus()).state).toEqual({ kind: "ready", version: "0.3.0" });
    const available = createMockIpc({ update: "available" });
    expect((await available.updateStatus()).state).toEqual({ kind: "available", version: "0.2.0" });
    await available.updateInstall();
    expect(available.state.updateInstalls).toBe(1);
    expect((await available.updateStatus()).state).toEqual({ kind: "installing", version: "0.2.0" });
    const updated = createMockIpc({ update: "updated" });
    expect((await updated.appStatus()).updated).toEqual({ from: "0.1.0", to: "0.2.0", notesUrl: "https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0" });
    await updated.openExternal({ releaseNotes: { version: "0.2.0" } });
    expect(updated.state.calls).toContain('openExternal:{"releaseNotes":{"version":"0.2.0"}}');
  });
```

`app/ui/src/App.test.tsx`, a new `describe("the updater (spec §6.2, phase 4b)")` after the recovery-mode block:

```ts
describe("the updater (spec §6.2, phase 4b)", () => {
  it("says Updated to vX above the home page, opens what changed, and stays closed once closed", async () => {
    const { ipc, user } = await home({ update: "updated" });
    expect(await screen.findByText("Updated to v0.2.0.")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "What changed" }));
    await waitFor(() => expect(ipc.state.calls).toContain('openExternal:{"releaseNotes":{"version":"0.2.0"}}'));
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByText("Updated to v0.2.0.")).toBeNull();
    await open(user, "Settings");
    await open(user, "Home");
    expect(screen.queryByText("Updated to v0.2.0.")).toBeNull();
    expect(ipc.state.calls.filter((c) => c === "crashDismiss")).toEqual([]);
  });

  it("offers the available version on the home page with Update now and Later, and Later holds for the run", async () => {
    const { ipc, user } = await home({ update: "available", settings: { updates: "notify" } });
    expect(await screen.findByText("CMD Studio v0.2.0 is available.")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Later" }));
    expect(screen.queryByText("CMD Studio v0.2.0 is available.")).toBeNull();
    await open(user, "Themes");
    await open(user, "Home");
    await screen.findByRole("heading", { name: "Home" });
    expect(screen.queryByText("CMD Studio v0.2.0 is available.")).toBeNull();
    expect(ipc.state.updateInstalls).toBe(0);
  });

  it("Update now installs through the backend and says the app comes back by itself", async () => {
    const { ipc, user } = await home({ update: "available", settings: { updates: "notify" } });
    await user.click(await screen.findByRole("button", { name: "Update now" }));
    await waitFor(() => expect(ipc.state.updateInstalls).toBe(1));
    expect(await screen.findByText(/Downloading and installing v0\.2\.0/)).toBeTruthy();
  });

  it("a downloaded version says it installs at close, and a failed Update now is said", async () => {
    const { ipc, user } = await home({ update: "ready" });
    expect(await screen.findByText(/v0\.2\.0 is downloaded and will be installed when you close CMD Studio/)).toBeTruthy();
    vi.spyOn(ipc, "updateInstall").mockRejectedValueOnce(ipcError("E_BUSY", "cmdstudio.lock is held"));
    await user.click(screen.getByRole("button", { name: "Update now" }));
    expect(await screen.findByText("(E_BUSY)")).toBeTruthy();
  });

  it("the Recovery page offers Update now when a version waits (recovery spec §5.2)", async () => {
    await mount({ safeMode: { kind: "recovery" }, update: "available", settings: { wizardDone: true } });
    await screen.findByRole("heading", { name: "Recovery mode" });
    expect(await screen.findByText(/CMD Studio v0\.2\.0 is available; a new version may fix/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Update now" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Later" })).toBeNull();
  });

  it("the Recovery page says that no update waits when none does", async () => {
    await mount({ safeMode: { kind: "recovery" }, settings: { wizardDone: true } });
    await screen.findByRole("heading", { name: "Recovery mode" });
    expect(await screen.findByText("No update is waiting.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Update now" })).toBeNull();
  });

  it("the Advanced page says when updates were last checked", async () => {
    const { user } = await home({ update: "idle" });
    await open(user, "Advanced");
    expect(await screen.findByText(/^Updates last checked at /)).toBeTruthy();
  });

  it("every new button has a name in Thai too", async () => {
    await home({ update: "available", settings: { language: "th", updates: "notify" } });
    expect(await screen.findByRole("button", { name: th["home.update.now"] })).toBeTruthy();
    expect(screen.getByRole("button", { name: th["home.update.later"] })).toBeTruthy();
  });
});
```

Run (in `app/`): `npm test -- i18n work mock App`
Expected: the i18n test fails on the missing keys and the old note; `work.test` fails (`KINDS.updateStatus` undefined); `mock.test` fails (`updateStatus` is not a function); the eight App tests fail.

- [ ] **Step 5: `types.ts`, `tauri.ts`, `work.ts`, `mock.ts`**

`types.ts`: imports `UpdateStatus`; `Ipc` gains, after `appRestart`:

```ts
  /** Where the updater is and when it last checked (spec §6.2): read only. */
  updateStatus(): Promise<UpdateStatus>;
  /** "Update now": downloads the offered version if it is not yet and installs it through the
   * plugin, which ends this process; the app comes back by itself. Like `appRestart`, an Ok
   * answer also means it is on its way; only a rejection (E_BUSY while a change runs, E_INTERNAL)
   * is an error. */
  updateInstall(): Promise<void>;
```

`tauri.ts`: `updateStatus: () => call("update_status"), updateInstall: () => call("update_install"),`.

`work.ts`: `KINDS` gains `updateStatus: "read", updateInstall: "exit",`; `TASK` gains `updateStatus: "work.reading", updateInstall: "work.updating",`; the module doc's "the restart itself is neither timed nor shown" becomes "the restart and Update now are neither timed nor shown".

`mock.ts`: imports `UpdateStatus`, `Updated`, `UpdateState`. `MockOptions` gains:

```ts
  /** Where the updater is when the mock starts (spec §6.2), as `update_status` says it; `updated`
   * is the notice of a start after an update (`app_status.updated`) with the state idle. Not given:
   * idle, as after a check that found nothing. `off` with `settings: { updates: "off" }`. */
  update?: "off" | "idle" | "available" | "ready" | "updated";
  /** The version the updater offers or brought; 0.2.0 when not given. */
  updateVersion?: string;
```

`MockState` gains `update: UpdateStatus; updated: Updated | null; updateInstalls: number;`. In `createMockIpc`, after `restarted: false,`:

```ts
    update: {
      mode: options.settings?.updates ?? "auto",
      state: updateState(options.update ?? "idle", options.updateVersion ?? "0.2.0"),
      checkedAtMs: options.update === "off" ? null : 1791694709000,
    },
    updated:
      options.update === "updated"
        ? { from: "0.1.0", to: options.updateVersion ?? "0.2.0", notesUrl: `https://github.com/luraselenehalo/cmd-studio/releases/tag/v${options.updateVersion ?? "0.2.0"}` }
        : null,
    updateInstalls: 0,
```

with, above `createMockIpc`:

```ts
/** The updater's state for an option: fixed data, as the backend's state machine would leave it. */
function updateState(option: NonNullable<MockOptions["update"]>, version: string): UpdateState {
  switch (option) {
    case "off":
      return { kind: "off" };
    case "available":
      return { kind: "available", version };
    case "ready":
      return { kind: "ready", version };
    default:
      return { kind: "idle" };
  }
}
```

`openExternal` records an object target as JSON: `state.calls.push(\`openExternal:${typeof target === "string" ? target : JSON.stringify(target)}\`);`. `appStatus()` gains `updated: state.updated,`. After `appRestart`:

```ts
    async updateStatus(): Promise<UpdateStatus> {
      state.calls.push("updateStatus");
      return structuredClone(state.update);
    },
    async updateInstall(): Promise<void> {
      state.calls.push("updateInstall");
      const s = state.update.state;
      if (s.kind !== "available" && s.kind !== "ready") {
        throw new IpcError({ code: "E_INTERNAL", messageTh: "(E_INTERNAL)", messageEn: "(E_INTERNAL)", details: "no update is available to install" });
      }
      state.updateInstalls += 1;
      state.update = { ...state.update, state: { kind: "installing", version: s.version } };
    },
```

(`IpcError` is imported in `mock.ts` already if other rejections use it; else import it from `./types`.)

- [ ] **Step 6: The words**

`en.ts` (replace the two placeholder lines; add the rest beside their sections):

```ts
  "start.updated": "Updated to v{version}.",
  "start.whatChanged": "What changed",
  "home.update.available": "CMD Studio v{version} is available.",
  "home.update.now": "Update now",
  "home.update.later": "Later",
  "home.update.ready": "CMD Studio v{version} is downloaded and will be installed when you close CMD Studio.",
  "home.update.installing": "Downloading and installing v{version}… CMD Studio closes and opens again by itself.",
  "recovery.update.available": "CMD Studio v{version} is available; a new version may fix what went wrong.",
  "recovery.update.none": "No update is waiting.",
  "advanced.update.checked": "Updates last checked at {time}",
  "advanced.update.notChecked": "Updates were not checked yet in this run",
  "work.updating": "downloading and installing the update",
  "settings.updates.note": "CMD Studio checks for a new version each time it starts. Automatic downloads it in the background and installs it when you close CMD Studio; Tell me first shows it on the home page and waits for you; Off never checks. After an update, CMD Studio says which version it is now.",
```

`th.ts`:

```ts
  "start.updated": "อัปเดตเป็น v{version} แล้ว",
  "start.whatChanged": "ดูว่ามีอะไรเปลี่ยน",
  "home.update.available": "มี CMD Studio v{version} ให้อัปเดต",
  "home.update.now": "อัปเดตตอนนี้",
  "home.update.later": "ไว้ทีหลัง",
  "home.update.ready": "ดาวน์โหลด CMD Studio v{version} แล้ว จะติดตั้งเมื่อคุณปิด CMD Studio",
  "home.update.installing": "กำลังดาวน์โหลดและติดตั้ง v{version}… CMD Studio จะปิดแล้วเปิดกลับมาเอง",
  "recovery.update.available": "มี CMD Studio v{version} ให้อัปเดต เวอร์ชันใหม่อาจแก้สาเหตุของปัญหาได้",
  "recovery.update.none": "ยังไม่มีอัปเดตรออยู่",
  "advanced.update.checked": "ตรวจอัปเดตล่าสุดเมื่อ {time}",
  "advanced.update.notChecked": "ยังไม่ได้ตรวจอัปเดตในการเปิดครั้งนี้",
  "work.updating": "กำลังดาวน์โหลดและติดตั้งอัปเดต",
  "settings.updates.note": "CMD Studio ตรวจหาเวอร์ชันใหม่ทุกครั้งที่เริ่มทำงาน แบบอัตโนมัติจะดาวน์โหลดเบื้องหลังแล้วติดตั้งเมื่อคุณปิด CMD Studio แบบแจ้งก่อนจะแสดงที่หน้าแรกแล้วรอคุณ ส่วนปิดจะไม่ตรวจเลย หลังอัปเดต CMD Studio จะบอกเสมอว่าเป็นเวอร์ชันอะไรแล้ว",
```

- [ ] **Step 7: The components**

`app/ui/src/state.tsx`: `AppState` gains `/** "Later" on the notify card: the offer is hidden for the rest of this run (spec §6.2). */ updateLater: boolean; markUpdateLater: () => void;`. In `App.tsx`: `const [updateLater, setUpdateLater] = useState(false); const [updateClosed, setUpdateClosed] = useState(false); const markUpdateLater = useCallback(() => setUpdateLater(true), []);` and the `appState` memo gains `updateLater, markUpdateLater` and strips the notice once closed: `appStatus: status ? { ...status, crashNotice: crashClosed ? null : status.crashNotice, updated: updateClosed ? null : status.updated } : null` (replacing the `crashClosed && status ? … : status` form), with `updateLater, updateClosed` in the dependency list. `StartProblems` gains `onUpdateClosed: () => void;`, passed as `onUpdateClosed={() => setUpdateClosed(true)}` beside `onCrashClosed`. `StartNotices` renders, after the crash notice:

```tsx
      {appStatus?.updated && <UpdatedNotice updated={appStatus.updated} onClosed={onUpdateClosed} />}
```

and, after `CrashClosed`:

```tsx
/** The last run's update went through (spec §2.1, §6.2): which version this is now, and one click
 * to the release page, which the backend opens for a SemVer version only. Close hides it for the
 * run; the backend forgot the record when the first screen drew (app_ready). */
function UpdatedNotice({ updated, onClosed }: { updated: Updated; onClosed: () => void }) {
  const { t } = useI18n();
  const ipc = useIpc();
  const [openError, setOpenError] = useState<unknown>(null);
  return (
    <Notice kind="ok">
      <p>{t("start.updated", { version: updated.to })}</p>
      <div className="row-actions">
        <button
          type="button"
          className="primary"
          onClick={() => {
            setOpenError(null);
            ipc.openExternal({ releaseNotes: { version: updated.to } }).catch(setOpenError);
          }}
        >
          {t("start.whatChanged")}
        </button>
        <button type="button" onClick={onClosed}>
          {t("common.close")}
        </button>
      </div>
      {openError !== null && <ErrorNotice error={openError} />}
    </Notice>
  );
}
```

(`import type { Updated } from "./bindings/Updated";` at the top. Task 5 adds the re-apply's text to this card.)

`app/ui/src/components/UpdateOffer.tsx`:

```tsx
// The updater's offer (spec §6.2): on the home page, a card when a newer version waits for "Update
// now" (notify mode) or is downloaded and installs at close (automatic mode); on the Recovery page,
// one line that says Update now when a version waits, else that none waits (recovery spec §5.2).
// "Update now" hands the install to the backend, which ends the process and brings the app back by
// itself; the card says so while that happens. "Later" (home only) hides the offer for the run.
import { useState } from "react";
import type { UpdateStatus } from "../bindings/UpdateStatus";
import { useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import { useAppState } from "../state";
import { ErrorNotice } from "./Notices";

export function UpdateOffer({ status, where }: { status: UpdateStatus | null; where: "home" | "recovery" }) {
  const { t } = useI18n();
  const ipc = useIpc();
  const { updateLater, markUpdateLater } = useAppState();
  const [installing, setInstalling] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const state = status?.state ?? null;
  const version = state && "version" in state ? state.version : null;
  const updateNow = (v: string) => {
    setError(null);
    setInstalling(v);
    ipc.updateInstall().catch((e: unknown) => {
      setInstalling(null);
      setError(e);
    });
  };
  if (installing !== null || state?.kind === "installing") {
    const v = installing ?? version ?? "";
    return (
      <div className="notice" role="status">
        {t("home.update.installing", { version: v })}
      </div>
    );
  }
  const offered = state?.kind === "available" || state?.kind === "ready";
  if (!offered || version === null || (where === "home" && updateLater)) {
    return where === "recovery" ? <p className="update-line">{t("recovery.update.none")}</p> : null;
  }
  const text =
    where === "recovery"
      ? t("recovery.update.available", { version })
      : state.kind === "ready"
        ? t("home.update.ready", { version })
        : t("home.update.available", { version });
  return (
    <div className={where === "home" ? "notice" : "update-line"} role="status">
      <p>{text}</p>
      <div className="row-actions">
        <button type="button" className="primary" onClick={() => updateNow(version)}>
          {t("home.update.now")}
        </button>
        {where === "home" && (
          <button type="button" onClick={markUpdateLater}>
            {t("home.update.later")}
          </button>
        )}
      </div>
      {error !== null && <ErrorNotice error={error} />}
    </div>
  );
}
```

`Home.tsx`: `const readUpdate = (ipc: Ipc) => ipc.updateStatus();` beside the other module-level reads; `const update = useRead(readUpdate, version);` in the component; `import { UpdateOffer } from "../components/UpdateOffer";`; the slot line `<div data-slot="update-notice">{t("home.update.placeholder")}</div>` and its comment become `<UpdateOffer status={update.value} where="home" />` (a read that failed is silent here: the updater is silent until it has something, and `update.error` is not shown). The file's header comment: "A line is kept for Phase 4's …" becomes "The updater's offer sits under the status (`UpdateOffer`)".

`Recovery.tsx`: `const readUpdate = (ipc: Ipc) => ipc.updateStatus();`, `const update = useRead(readUpdate, version);`, the placeholder line becomes `<UpdateOffer status={update.value} where="recovery" />`; the header's last sentence names it.

`Advanced.tsx`: `const [update, setUpdate] = useState<UpdateStatus | null>(null);` read in the same effect as `advancedRead` (`ipc.updateStatus().then((u) => { if (alive) setUpdate(u); }, () => {})`); after the versions table, inside the `info &&` block:

```tsx
          <p>
            {update?.checkedAtMs != null
              ? t("advanced.update.checked", { time: formatTime(update.checkedAtMs, language) })
              : t("advanced.update.notChecked")}
          </p>
```

with `formatTime` and `language` from `useI18n()` (as `Home.tsx` uses them) and `import type { UpdateStatus } from "../bindings/UpdateStatus";`.

- [ ] **Step 8: Run the UI's checks**

In `app/`: `npm run typecheck`, `npm test`, `TZ=UTC npm test`, `npm run build`, `bash scripts/check-dist.sh` (from the repo root).
Expected: all pass; the i18n rules hold (every new key in both dictionaries, Thai on the Thai side, the same placeholders); `App.test`'s "every button on every page … has a name" tests still pass (the new buttons are named).

- [ ] **Step 9: The checks, then commit**

The four clippy runs, `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo test -p cmdstudio-app`.

Write `.git/CMDSTUDIO_COMMIT_MSG`:

```
app, ui: update_status, update_install, app_status.updated and the updater's cards (spec §6.2, phase 4b)

The 39th and 38th commands: update_status (read only) and
update_install ("Update now": the plugin's install through the
backend, which ends the process); app_ready starts the check once the
window is up. app_status.updated carries the notice of a start after
an update. Bindings regenerated (AppStatus, ExternalTarget, UpdateState,
UpdateStatus, Updated). UI: "Updated to vX" with What changed (the
release page, for a SemVer version only) and Close above every screen;
the notify card on the home page with Update now and Later (for the
run), the downloaded-installs-at-close line in automatic mode; the
Recovery page's Update now or "No update is waiting"; the Advanced
page's "last checked"; the Settings note's words; the mock's update
option. The two reserved placeholder keys go.
```

Then `git add -- app/src-tauri/src/command_names.rs app/src-tauri/src/ipc.rs app/src-tauri/capabilities/default.json app/src-tauri/src/backend/run.rs app/ui/src/bindings/AppStatus.ts app/ui/src/bindings/ExternalTarget.ts app/ui/src/bindings/UpdateState.ts app/ui/src/bindings/UpdateStatus.ts app/ui/src/bindings/Updated.ts app/ui/src/ipc/types.ts app/ui/src/ipc/tauri.ts app/ui/src/ipc/mock.ts app/ui/src/ipc/work.ts app/ui/src/ipc/mock.test.ts app/ui/src/ipc/work.test.ts app/ui/src/App.tsx app/ui/src/state.tsx app/ui/src/components/UpdateOffer.tsx app/ui/src/screens/Home.tsx app/ui/src/screens/Recovery.tsx app/ui/src/screens/Advanced.tsx app/ui/src/i18n/en.ts app/ui/src/i18n/th.ts app/ui/src/i18n/i18n.test.ts app/ui/src/App.test.tsx`, check `git diff --cached --name-only`, then `git commit -F .git/CMDSTUDIO_COMMIT_MSG`.

---
### Task 5: After an update: the re-apply through a transaction and the pruning

Runs after Task 4. Spec §6.2 "after an update" steps 2 and 3: when the theme applied runs on runtime folders of other versions than this build pins (an update went through, or a by-hand upgrade), the next normal start applies it again through `plan_apply` and `apply` (a transaction under the lock, a restore point), on a thread under `busy = "update_reapply"` so the UI waits as it waits for any change; then the old runtime folders go, best effort, and at every start. A re-apply that does not commit leaves the theme on the old runtime, which `state.json` still names, so nothing of it is pruned, and the notice says so.

**Files:**
- Modify: `app/src-tauri/src/backend/update.rs` (`Updated.reapply_failed`, `Backend::{update_needs_reapply, update_reapply, prune_runtimes}`, the prune at every start, tests)
- Modify: `app/src-tauri/src/backend/mod.rs` (`testing::fake_process`)
- Modify: `app/src-tauri/src/lib.rs` (`spawn_update_reapply`, called in `setup` outside safe mode; a text test)
- Modify: `app/ui/src/bindings/Updated.ts` (regenerated)
- Modify: `app/ui/src/App.tsx` (`StartWait`'s words for `update_reapply`; the card's failure text), `app/ui/src/ipc/mock.ts` (`updateReapplyFailed`), `i18n/en.ts`, `th.ts`, `App.test.tsx`

**Interfaces:**
- Consumes: `plan::runtime_changed`, `cleanup::{runtime_dirs_named_by, prune_old_runtimes}` (Task 2), `Backend::{plan_apply, apply}`, `PlanOutcome::{Ready, OtherClink}`, `ApplyRequest`, `ApplyStatus::Committed`, `State::load`, `Backend::{busy_mark, busy_until_dropped, busy_clear}`, `UPDATE_REAPPLY_BUSY` (Task 3).
- Produces: `Updated.reapply_failed: Option<String>` (the code, when the re-apply did not commit); `Backend::update_needs_reapply() -> bool`, `Backend::update_reapply()`, `Backend::prune_runtimes() -> usize`; the log lines `update reapply=ok|<code>|thread <e>`, `update pruned=<n>`; `testing::fake_process(&Backend<FakeMachine>) -> &FakeProcess`; the keys `start.updateReapply`, `start.updatedReapplyFailed`; `MockOptions.updateReapplyFailed?: string`.

- [ ] **Step 1: The failing tests**

In `backend/mod.rs`'s `testing` module, after `fake_events`:

```rust
    /// The fake programs under the backend (see [`fake_fs`]): a test makes Oh My Posh fail.
    pub(crate) fn fake_process(b: &Backend<FakeMachine>) -> &cmdstudio_core::adapters::fake::FakeProcess {
        &b.machine.process
    }
```

In `backend/update.rs`'s test module, the imports gain `use crate::backend::testing::{apply_terracotta, fake_process};`, `use cmdstudio_core::plan::OMP_VERSION;` and `use cmdstudio_core::state::State;`, and the tests:

```rust
    /// state.json as the last run left it, but with the applied theme's OMP version an older one,
    /// as an update that moved the pin leaves it; the old runtime folder is there too.
    fn applied_on_an_old_omp(b: &Backend<FakeMachine>) {
        apply_terracotta(b);
        let file = b.paths().state_file();
        let mut state = State::load(b.sys().fs, &file).unwrap();
        state.applied.as_mut().unwrap().omp_version = "31.4.0".into();
        b.sys().fs.write_atomic(&file, &state.to_bytes().unwrap()).unwrap();
        fake_fs(b).put(b.paths().runtime_root().join("omp-31.4.0").join("oh-my-posh.exe"), b"old");
    }

    fn omp_version_applied(b: &Backend<FakeMachine>) -> String {
        State::load(b.sys().fs, &b.paths().state_file()).unwrap().applied.unwrap().omp_version
    }

    /// Spec §6.2 steps 2-3: the theme on an old runtime is applied again, once, through a
    /// transaction; state.json then names this build's pins, the old folder goes, and the notice
    /// carries no failure.
    #[test]
    fn a_changed_runtime_pin_re_applies_the_theme_once() {
        let b = backend();
        applied_on_an_old_omp(&b);
        fake_fs(&b).put(
            b.paths().update_file(),
            br#"{"version":1,"from":"0.1.0","to":"0.2.0","notesUrl":"https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0"}"#,
        );
        b.update_after_start("0.2.0");
        assert!(b.update_needs_reapply());
        b.update_reapply();
        assert_eq!(omp_version_applied(&b), OMP_VERSION);
        assert!(!b.sys().fs.exists(&b.paths().runtime_root().join("omp-31.4.0")));
        assert!(b.sys().fs.exists(&b.paths().omp_dir()));
        let lines = update_lines(&b);
        assert!(lines.contains(&"update reapply=ok".to_string()), "{lines:?}");
        assert!(lines.contains(&"update pruned=1".to_string()), "{lines:?}");
        assert_eq!(b.updated().unwrap().reapply_failed, None);
        assert!(!b.update_needs_reapply(), "once");
        // A restore point was taken, as for every apply (spec §5.5).
        assert!(b.snapshots_list().unwrap().points.len() >= 2);
    }

    /// Equal pins, or no theme, or state.json that cannot be read: nothing to re-apply; a by-hand
    /// upgrade (no record) re-applies all the same, since state.json says the runtime changed.
    #[test]
    fn equal_pins_re_apply_nothing() {
        let b = backend();
        assert!(!b.update_needs_reapply(), "no theme");
        apply_terracotta(&b);
        assert!(!b.update_needs_reapply(), "the pins are this build's");
        let b = backend();
        applied_on_an_old_omp(&b);
        assert!(b.update_needs_reapply(), "a by-hand upgrade: no record, the runtime changed");
        fake_fs(&b).fail_reads_of(b.paths().state_file());
        assert!(!b.update_needs_reapply(), "state.json unreadable: nothing is decided from it");
    }

    /// Review Focus 5: a re-apply that rolls back (Oh My Posh cannot render the new config)
    /// leaves the theme on the old runtime, keeps that folder, says the code in the log and in
    /// the notice.
    #[test]
    fn a_failed_re_apply_keeps_the_old_runtime_and_says_so() {
        let b = backend();
        applied_on_an_old_omp(&b);
        fake_fs(&b).put(
            b.paths().update_file(),
            br#"{"version":1,"from":"0.1.0","to":"0.2.0","notesUrl":"https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0"}"#,
        );
        b.update_after_start("0.2.0");
        fake_process(&b).answer("oh-my-posh.exe", "print", 1, "bad config");
        b.update_reapply();
        assert_eq!(omp_version_applied(&b), "31.4.0", "rolled back: state.json as before");
        assert!(b.sys().fs.exists(&b.paths().runtime_root().join("omp-31.4.0")), "named by state.json: kept");
        let lines = update_lines(&b);
        assert!(lines.contains(&"update reapply=E_VERIFY_OMP".to_string()), "{lines:?}");
        assert!(lines.contains(&"update pruned=0".to_string()), "{lines:?}");
        assert_eq!(b.updated().unwrap().reapply_failed.as_deref(), Some("E_VERIFY_OMP"));
        assert!(b.update_needs_reapply(), "the next start tries again");
    }

    /// Spec §6.2 step 3, at every start: a runtime folder nobody names goes; one state.json names
    /// stays; a start with no theme prunes nothing it should not.
    #[test]
    fn old_runtime_folders_are_pruned_at_every_start() {
        let b = backend();
        apply_terracotta(&b);
        fake_fs(&b).put(b.paths().runtime_root().join("clink-1.9.0").join("clink_dll_x64.dll"), b"old");
        b.update_after_start("0.1.0");
        assert!(!b.sys().fs.exists(&b.paths().runtime_root().join("clink-1.9.0")));
        assert!(b.sys().fs.exists(&b.paths().clink_dir()));
        assert_eq!(update_lines(&b), ["update pruned=1"]);
        let b = backend();
        applied_on_an_old_omp(&b);
        b.update_after_start("0.1.0");
        assert!(b.sys().fs.exists(&b.paths().runtime_root().join("omp-31.4.0")), "named: kept");
        assert_eq!(update_lines(&b), Vec::<String>::new(), "nothing pruned, nothing said");
    }
```

`update_json_makes_the_notice_once_and_goes_with_app_ready`'s `Updated { … }` literal gains `reapply_failed: None`.

In `lib.rs`'s test module:

```rust
    /// Spec §6.2 steps 2-3 (phase 4b): the re-apply after an update runs on its own thread once
    /// the window is up, marked busy before the thread exists (the first app_status waits for it)
    /// and cleared by a guard, outside safe mode only; the prune follows inside it.
    #[test]
    fn the_re_apply_is_marked_before_its_thread_and_runs_outside_safe_mode_only() {
        let body = body_of("fn spawn_update_reapply(");
        let marked = body.find(".busy_mark(UPDATE_REAPPLY_BUSY)").unwrap();
        let spawned = body.find(".spawn(").unwrap();
        let guard = body.find(".busy_until_dropped(UPDATE_REAPPLY_BUSY)").unwrap();
        let work = body.find(".update_reapply()").unwrap();
        let failed = body.find("if let Err(e) = spawned").unwrap();
        let cleared = body.find(".busy_clear(UPDATE_REAPPLY_BUSY)").unwrap();
        assert!(marked < spawned && spawned < guard && guard < work, "marked, then the guarded work");
        assert!(failed < cleared, "cleared only when the thread could not start");
        assert!(body.contains("update reapply=thread"));
        let text = production();
        let window = text.find("open_main_window(app, ").unwrap();
        let call = text.find("spawn_update_reapply(&for_window)").unwrap();
        assert!(window < call, "after the window");
        let before = text.get(..call).unwrap_or("");
        assert!(before.trim_end().ends_with("if decided.safe_mode.is_none() && for_window.update_needs_reapply() {"), "outside safe mode, and only when needed");
    }
```

In `App.test.tsx`'s updater block:

```ts
  it("waits for the re-apply after an update with its own words, then shows the notice", async () => {
    const { ipc } = await home({ update: "updated", busy: "update_reapply" });
    expect(await screen.findByText("CMD Studio is still updating the theme for the new version…")).toBeTruthy();
    ipc.state.busy = null;
    expect(await screen.findByText("Updated to v0.2.0.", {}, { timeout: 5000 })).toBeTruthy();
  });

  it("says when the theme could not be applied again for the new version", async () => {
    await home({ update: "updated", updateReapplyFailed: "E_VERIFY_OMP" });
    expect(await screen.findByText(/^Updated to v0\.2\.0\. The theme could not be applied again for the new version \(E_VERIFY_OMP\)/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "What changed" })).toBeTruthy();
  });
```

(`home` with `busy` set: `App` polls `app_status` every 2 s (`APP_STATUS_POLL_MS`); the first test clears the mock's `busy` and waits up to 5 s, as "waits while a change still runs in this process" does.)

Run: `cargo test -p cmdstudio-app --lib backend::update:: lib` and, in `app/`, `npm test -- App`.
Expected: compile errors (`update_needs_reapply`, `update_reapply`, `reapply_failed`, `fake_process` not found); the two App tests fail.

- [ ] **Step 2: The backend**

In `backend/update.rs`, `Updated` gains, after `notes_url`:

```rust
    /// The code of a re-apply that did not commit (spec §6.2 step 2): the theme stays as it was
    /// on the old runtime, and the notice says so.
    pub reapply_failed: Option<String>,
```

(`update_after_start` fills `reapply_failed: None`.) The imports gain `use crate::backend::apply::{ApplyRequest, ApplyStatus, PlanOutcome};`, `use cmdstudio_core::cleanup::{prune_old_runtimes, runtime_dirs_named_by};`, `use cmdstudio_core::plan::runtime_changed;`, `use cmdstudio_core::state::State;`. At the end of `update_after_start` (after the `updated` is set, and on every return path before it: put it first, right after the `current` is set and `updates\` is emptied):

```rust
        let pruned = self.prune_runtimes();
        if pruned > 0 {
            self.note_line(&format!("update pruned={pruned}"));
        }
```

And the methods, after `update_record_consumed`:

```rust
    /// Whether this start must apply the theme again (spec §6.2 step 2): a theme is applied and
    /// its runtime versions are not this build's pins, as an update that moved a pin leaves
    /// state.json, a by-hand upgrade included (no record then). state.json that cannot be read
    /// decides nothing: doctor says what is wrong with it.
    pub fn update_needs_reapply(&self) -> bool {
        State::load(self.sys().fs, &self.paths().state_file())
            .map(|state| runtime_changed(&state))
            .unwrap_or(false)
    }

    /// The re-apply (spec §6.2 steps 2-3), on its own thread under `UPDATE_REAPPLY_BUSY`
    /// (`lib.rs`): the applied theme as state.json names it, through `plan_apply` and `apply`
    /// like every apply (the lock, a restore point, the verify), then the old runtime folders.
    /// The outcome goes to the log (`update reapply=ok|<code>`) and, when the notice is there, to
    /// it. A re-apply that does not commit rolls back to the old runtime, which state.json still
    /// names, so the prune keeps that folder and the next start tries again.
    pub fn update_reapply(&self) {
        let failed = self.reapply_once();
        self.note_line(&format!(
            "update reapply={}",
            failed.as_deref().unwrap_or("ok")
        ));
        let pruned = self.prune_runtimes();
        self.note_line(&format!("update pruned={pruned}"));
        if let Some(code) = failed {
            self.update.with(|u| {
                if let Some(updated) = &mut u.updated {
                    updated.reapply_failed = Some(code);
                }
            });
        }
    }

    /// One apply of the theme state.json names; `None` when it committed, else the code.
    fn reapply_once(&self) -> Option<String> {
        let state = match State::load(self.sys().fs, &self.paths().state_file()) {
            Ok(state) => state,
            Err(e) => return Some(e.code().to_string()),
        };
        let applied = state.applied?;
        let req = ApplyRequest {
            theme_id: applied.theme_id,
            target: applied.target,
            set_wt_default_profile: applied.set_default,
            replace_other_clink: false,
        };
        let plan = match self.plan_apply(req.clone()) {
            Ok(PlanOutcome::Ready { plan }) => plan,
            // Another Clink appeared in AutoRun: the user decides on the home page, not a thread.
            Ok(PlanOutcome::OtherClink { .. }) => return Some("E_AUTORUN_FOREIGN_CLINK".to_string()),
            Err(e) => return Some(e.code),
        };
        match self.apply(req, &plan.digest, &mut |_| {}) {
            Ok(outcome) if outcome.status == ApplyStatus::Committed => None,
            Ok(outcome) => Some(
                outcome
                    .error
                    .map(|e| e.code)
                    .unwrap_or_else(|| crate::error::INTERNAL.to_string()),
            ),
            Err(e) => Some(e.code),
        }
    }

    /// Spec §6.2 step 3: the runtime folders of other versions than this build's, except the
    /// ones state.json names; how many went. state.json that cannot be read prunes nothing: what
    /// AutoRun runs is not known then.
    pub(crate) fn prune_runtimes(&self) -> usize {
        let Ok(state) = State::load(self.sys().fs, &self.paths().state_file()) else {
            return 0;
        };
        let keep = runtime_dirs_named_by(&state, self.paths());
        prune_old_runtimes(self.sys(), self.paths(), &keep).len()
    }
```

`StateError::code()` exists (`state.rs:374`). `ApplyPlan.digest` is the field `apply` compares (`apply.rs:546`).

- [ ] **Step 3: `lib.rs`**

After `start_crash_check`'s `if … else if` in `setup`:

```rust
            // Spec §6.2 steps 2-3 (phase 4b): after an update the theme applied on the old
            // runtime is applied again, on a thread of its own, under its own busy name, so the
            // first app_status waits for it; then the old runtime folders go. Not in safe mode,
            // which changes nothing on the machine by itself.
            if decided.safe_mode.is_none() && for_window.update_needs_reapply() {
                spawn_update_reapply(&for_window);
            }
```

After `start_recovery_entry`:

```rust
/// The re-apply after an update (spec §6.2) on a thread of its own, marked busy before the
/// thread exists (a UI that asks in between does not find it idle) and cleared by the guard on
/// every exit of the work, a panic included; a thread that cannot start clears the mark and is
/// one log line, and the next start tries again.
#[cfg(windows)]
fn spawn_update_reapply(backend: &std::sync::Arc<backend::Backend<machine::RealMachine>>) {
    use backend::update::UPDATE_REAPPLY_BUSY;
    let b = backend.clone();
    backend.busy_mark(UPDATE_REAPPLY_BUSY);
    let spawned = std::thread::Builder::new()
        .name("update-reapply".to_string())
        .spawn(move || {
            let _busy = b.busy_until_dropped(UPDATE_REAPPLY_BUSY);
            b.update_reapply();
        });
    if let Err(e) = spawned {
        backend.busy_clear(UPDATE_REAPPLY_BUSY);
        backend.note_line(&format!("update reapply=thread {e}"));
    }
}
```

- [ ] **Step 4: The bindings and the UI**

Delete `app/ui/src/bindings/Updated.ts` and regenerate as in Task 4 Step 3 (`reapplyFailed: string | null`). `App.tsx`: `StartWait` becomes

```tsx
      {busy === "recovery_entry" ? t("start.recovering") : busy === "update_reapply" ? t("start.updateReapply") : t("start.busyWith", { task: t(taskOf(busy)) })}
```

and `UpdatedNotice`'s `<p>` becomes

```tsx
      <p>
        {updated.reapplyFailed
          ? t("start.updatedReapplyFailed", { version: updated.to, code: updated.reapplyFailed })
          : t("start.updated", { version: updated.to })}
      </p>
```

`mock.ts`: `MockOptions` gains `/** The re-apply after the update did not commit with this code (spec §6.2); `updated` only. */ updateReapplyFailed?: string;` and the `updated` literal gains `reapplyFailed: options.updateReapplyFailed ?? null`. `en.ts`:

```ts
  "start.updateReapply": "CMD Studio is still updating the theme for the new version…",
  "start.updatedReapplyFailed": "Updated to v{version}. The theme could not be applied again for the new version ({code}): run the health check on the Help page, or apply the theme again from the Themes page.",
```

`th.ts`:

```ts
  "start.updateReapply": "CMD Studio กำลังใช้ธีมซ้ำสำหรับเวอร์ชันใหม่อยู่…",
  "start.updatedReapplyFailed": "อัปเดตเป็น v{version} แล้ว แต่ใช้ธีมซ้ำสำหรับเวอร์ชันใหม่ไม่สำเร็จ ({code}) ให้ตรวจสุขภาพที่หน้าช่วยเหลือ หรือใช้ธีมอีกครั้งจากหน้าธีม",
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p cmdstudio-app --lib`, and in `app/`: `npm run typecheck`, `npm test`, `TZ=UTC npm test`.
Expected: the four backend tests, the text test and the two App tests pass; `git status --porcelain app/ui/src/bindings` lists `Updated.ts` only.

- [ ] **Step 6: The checks, then commit**

The four clippy runs, `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo test --workspace`.

Write `.git/CMDSTUDIO_COMMIT_MSG`:

```
app, ui: the re-apply after an update and the prune of old runtimes (spec §6.2, phase 4b)

update_needs_reapply (a theme applied on runtime versions that are not
this build's pins, a by-hand upgrade included; unreadable state.json
decides nothing) and update_reapply (the applied theme through
plan_apply and apply like every apply, then prune_old_runtimes; the
code of a re-apply that did not commit goes to the log and to the
notice, and the old folder state.json still names stays). The prune
runs at every start. lib.rs runs the re-apply on its own thread under
busy = update_reapply, outside safe mode, marked before the thread
exists. The UI waits with its own words and the notice says when the
theme could not be applied again.
```

Then `git add -- app/src-tauri/src/backend/update.rs app/src-tauri/src/backend/mod.rs app/src-tauri/src/lib.rs app/ui/src/bindings/Updated.ts app/ui/src/App.tsx app/ui/src/ipc/mock.ts app/ui/src/i18n/en.ts app/ui/src/i18n/th.ts app/ui/src/App.test.tsx`, check `git diff --cached --name-only`, then `git commit -F .git/CMDSTUDIO_COMMIT_MSG`.

---

### Task 6: The key-dependent wiring — **waits for the founder's key**

Runs after Task 7 and Task 8's Steps 1–3; nothing before it needs the key. The founder's public key goes into `tauri.conf.json`, the bundle makes updater artifacts (so every CI installer is signed with the founder's key from the repository's secrets), and `release.yml` uploads the `.sig` and `latest.json` under the asset name of 4a's ruling, without `--prerelease`. The release notes say how to upgrade, in the words U6 recorded.

**The founder's exact steps, which the task's first step prints and waits on:**

1. In a shell outside the repo: `npm --prefix app run tauri -- signer generate -w %USERPROFILE%\.tauri\cmdstudio.key` (it asks for a password; keep the file and the password outside the repo and never paste either into a chat).
2. GitHub → the repo → Settings → Secrets and variables → Actions → New repository secret: `TAURI_SIGNING_PRIVATE_KEY` = the content of `cmdstudio.key`; `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = the password.
3. Give the agent the content of `cmdstudio.key.pub` (public; it goes into `tauri.conf.json`'s `plugins.updater.pubkey`).
4. Decide the endpoint (Decisions, "The endpoint"): the fixed `releases/latest/download/latest.json` with releases not flagged pre-release (the default below), or the second release tagged `updater` (the alternative, two lines of `release.yml` below).

Until step 3 the task does not start; Tasks 1–5, 7 and 8 do not depend on it.

**Files:**
- Modify: `app/src-tauri/tauri.conf.json` (`plugins.updater.pubkey`)
- Modify: `app/src-tauri/tauri.bundle.conf.json` (`bundle.createUpdaterArtifacts: true`)
- Modify: `app/src-tauri/src/config_guard.rs` (the pubkey's shape, `createUpdaterArtifacts`, the workflows' text)
- Modify: `.github/workflows/ci.yml` (the signing env on the bundle step; the `.sig` in the listing and the artifact)
- Modify: `.github/workflows/release.yml` (`secrets: inherit`; the `.sig` and `latest.json`; no `--prerelease`; the version grep, M6)
- Modify: `.github/release-notes.md` (how to upgrade)

**Interfaces:**
- Consumes: the public key (the founder), `CMD Studio_<v>_x64-setup.exe.sig` (the bundler, G-SIG), `ci.yml`'s artifact `cmdstudio-installer` (Task 1's listing step), U6's words (Task 1's `results.md`).
- Produces: the draft release `v<version>` with the assets `CMD-Studio_<version>_x64-setup.exe`, `CMD-Studio_<version>_x64-setup.exe.sig`, `latest.json`, `SHA256SUMS.txt`; `latest.json` = `{"version":"<v>","notes":"<the first line of the notes>","pub_date":"<ISO 8601>","platforms":{"windows-x86_64":{"signature":"<the .sig's text>","url":"https://github.com/luraselenehalo/cmd-studio/releases/download/v<v>/CMD-Studio_<v>_x64-setup.exe"}}}`; the artifact `cmdstudio-installer` gains the `.sig`.

- [ ] **Step 1: Print the founder's steps and wait**

The implementer's first message is the four steps above, and the task stops until the controller hands over the public key's text (one base64 line; the controller never pastes the private key or the password, which it never sees).

- [ ] **Step 2: The failing guards**

In `config_guard.rs`, `the_bundle_file_is_a_per_user_nsis_installer_and_the_committed_config_stays_buildable`: the assertion `bundle.get("createUpdaterArtifacts").is_none()` becomes `assert_eq!(bundle.get("createUpdaterArtifacts"), Some(&Value::Bool(true)), "the bundler signs the installer and writes its .sig (phase 4b)");` and its doc's "no updater artifacts until 4b" becomes "updater artifacts (the installer's .sig)". In `the_updater_config_is_the_one_endpoint_quiet_and_without_dangerous_switches`, after the `pubkey` character check:

```rust
    assert!(pubkey.len() >= 64 && !pubkey.contains(char::is_whitespace), "the founder's minisign public key, one base64 line (Task 6): {pubkey:?}");
```

Add:

```rust
/// Spec §6.4 (phase 4b), 4a's rulings: the release workflow signs through ci.yml (the secrets
/// are handed to the called workflow), uploads the installer under the hyphenated asset name
/// with its .sig and latest.json (the updater's manifest, whose url is that asset under the
/// release's tag), and makes a draft that is not flagged pre-release (GitHub's "latest"
/// download skips flagged ones; the title and the notes keep the words). Both workflows read the
/// version under [workspace.package] and nowhere else (4a's M6).
#[test]
fn the_release_workflow_signs_uploads_latest_json_and_is_not_a_prerelease() {
    let release = include_str!("../../../.github/workflows/release.yml");
    let ci = include_str!("../../../.github/workflows/ci.yml");
    assert!(release.contains("    secrets: inherit\n"), "the signing key reaches ci.yml");
    assert!(!release.contains("--prerelease"), "not flagged: the endpoint is releases/latest");
    assert!(release.contains("--draft"), "still a draft the founder publishes");
    for needle in [
        "CMD-Studio_${version}_x64-setup.exe.sig",
        "release/latest.json",
        "releases/download/v${version}/CMD-Studio_${version}_x64-setup.exe",
        "\"windows-x86_64\"",
    ] {
        assert!(release.contains(needle), "release.yml lacks {needle}");
    }
    assert!(ci.contains("TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}"));
    assert!(ci.contains("TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}"));
    assert!(ci.contains("cp \"$installer.sig\" stage/installer/"), "the .sig travels with the installer");
    let awk = r#"awk '/^\[workspace\.package\]/{t=1;next} /^\[/{t=0} t && /^version = /{gsub(/^version = "|"$/,""); print; exit}' Cargo.toml"#;
    for (name, text) in [("ci.yml", ci), ("release.yml", release)] {
        assert!(text.contains(awk), "{name} reads the version under [workspace.package]");
        assert!(!text.contains("grep -m1 '^version = '"), "{name} still greps the first version line");
    }
}
```

Run: `cargo test -p cmdstudio-app --lib config_guard`
Expected: the three tests fail (`createUpdaterArtifacts` absent, the pubkey empty, the workflows' text).

- [ ] **Step 3: The configs**

`tauri.conf.json`: `"pubkey": "<the founder's key, one line>"`. `tauri.bundle.conf.json`: after `"publisher"`, `"createUpdaterArtifacts": true,` (the v2 form: the bundler signs the NSIS installer itself and writes `<installer>.sig`; no `.nsis.zip`).

- [ ] **Step 4: `ci.yml`**

The step "Tauri build (with the NSIS bundle)" gains, between `working-directory: app` and `run:`:

```yaml
        # The updater's signature (spec §6.2, §6.4, phase 4b): the founder's key from the
        # repository's secrets, which GitHub hands to pushes and pull requests of this repository
        # (never to a fork's: such a build fails here, which is right). createUpdaterArtifacts in
        # tauri.bundle.conf.json makes the bundler refuse to build without the key, so every
        # installer CI makes is signed, and its .sig goes with it. Never printed.
        env:
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
```

In the listing step, after `test -f "$installer"`: `test -f "$installer.sig"` and `ls -l "$installer.sig"`; after `cp "$installer" stage/installer/`: `cp "$installer.sig" stage/installer/`; the step's comment gains "and the updater's .sig (phase 4b)". (Task 1's spike step keeps its own `env`, which overrides these for that step.)

- [ ] **Step 5: `release.yml`**

The version step's grep becomes the `awk` of Task 1 Step 1 (M6):

```bash
          version="$(awk '/^\[workspace\.package\]/{t=1;next} /^\[/{t=0} t && /^version = /{gsub(/^version = "|"$/,""); print; exit}' Cargo.toml)"
          test -n "$version"
```

The `ci:` job gains `secrets: inherit` after `uses:`. The draft job's two steps become:

```yaml
      - name: The notes, the hash and the updater's manifest, with the version filled in
        shell: bash
        run: |
          version="${{ needs.version.outputs.version }}"
          # The asset name has a hyphen, no space: GitHub may rename an uploaded file whose name has
          # a space, and SHA256SUMS.txt and latest.json must name the file a user and the updater
          # download. ci.yml's artifact (with the space) is left as it is for the VM check.
          mkdir release
          cp installer/*.exe "release/CMD-Studio_${version}_x64-setup.exe"
          cp installer/*.exe.sig "release/CMD-Studio_${version}_x64-setup.exe.sig"
          (cd release && sha256sum -- "CMD-Studio_${version}_x64-setup.exe" > SHA256SUMS.txt && cat SHA256SUMS.txt)
          sha="$(cut -d' ' -f1 release/SHA256SUMS.txt | head -1)"
          sed -e "s/@VERSION@/v$version/g" -e "s/@SHA256@/$sha/g" .github/release-notes.md > notes.md
          cat notes.md
          # The updater's manifest (spec §6.4, phase 4b): the version, the first line of the notes,
          # the time, and for windows-x86_64 the signature (the .sig's one line) and the asset's
          # URL under this release's tag. The app checks releases/latest/download/latest.json,
          # which GitHub serves from the newest published release that is not flagged pre-release.
          sig="$(tr -d '\r\n' < "release/CMD-Studio_${version}_x64-setup.exe.sig")"
          test -n "$sig"
          jq -n --arg v "$version" --arg notes "$(head -1 notes.md)" --arg when "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg sig "$sig" \
            --arg url "https://github.com/${GITHUB_REPOSITORY}/releases/download/v${version}/CMD-Studio_${version}_x64-setup.exe" \
            '{version: $v, notes: $notes, pub_date: $when, platforms: {"windows-x86_64": {signature: $sig, url: $url}}}' > release/latest.json
          cat release/latest.json
      # Not flagged pre-release (Decisions, "The endpoint"): the app's fixed endpoint
      # releases/latest/download/latest.json skips flagged ones. The title and the notes say
      # "pre-release for testers" instead.
      - name: Create the draft release
        env:
          GH_TOKEN: ${{ github.token }}
        shell: bash
        run: |
          version="${{ needs.version.outputs.version }}"
          gh release create "v$version" --draft --title "CMD Studio v$version" --verify-tag --notes-file notes.md "release/CMD-Studio_${version}_x64-setup.exe" "release/CMD-Studio_${version}_x64-setup.exe.sig" release/latest.json release/SHA256SUMS.txt
```

The header comment's "a DRAFT pre-release" becomes "a DRAFT release (named a pre-release for testers in its title and notes, not flagged: the updater's endpoint skips flagged ones)". **The alternative, if the founder chose it in step 4:** keep `--prerelease`, and after `gh release create` add `gh release upload updater release/latest.json --clobber` (the founder makes the fixed release tagged `updater` once, by hand, with that one asset), the endpoint in `tauri.conf.json` and `config_guard` becomes `https://github.com/luraselenehalo/cmd-studio/releases/download/updater/latest.json`, and the guard's `--prerelease` assertion flips.

- [ ] **Step 6: The notes**

In `.github/release-notes.md`, after the "**Uninstalling**" paragraph:

```markdown
**Updating:** CMD Studio checks for a new version each time it starts (Settings > Updates: Automatic, Tell me first, or Off), installs it when you close the app, and tells you which version it is now. To upgrade by hand instead, run the new installer and, on its "already installed" page, choose **Do not uninstall**: your theme and your settings stay. "Uninstall before installing" puts everything back first, and the setup wizard runs again.
```

The two quoted choices are U6's recorded words (`docs/spikes/phase4-update/results.md`); copy them from there if they differ. The first paragraph's "a pre-release for testers" stays.

- [ ] **Step 7: Parse, run the guards, then commit**

Parse both YAML files; `cargo test -p cmdstudio-app --lib config_guard` (Expected: all pass, the three above included); `cargo test -p cmdstudio-app --lib` (the config with a real pubkey still builds the merged `Config`).

Write `.git/CMDSTUDIO_COMMIT_MSG`:

```
app, ci: the updater's public key, signed installers and latest.json (spec §6.2, §6.4, phase 4b)

tauri.conf.json carries the founder's minisign public key;
tauri.bundle.conf.json turns createUpdaterArtifacts on, so the bundler
signs every installer with the key the repository's secrets hold
(ci.yml's bundle step; the .sig travels in the artifact). release.yml
hands the secrets to ci.yml, uploads the installer and its .sig under
the hyphenated asset name with latest.json (the updater's manifest,
its url under the release's tag) and SHA256SUMS.txt, and makes a draft
that is not flagged pre-release: the app's endpoint,
releases/latest/download/latest.json, skips flagged ones. Both
workflows read the version under [workspace.package] (4a's M6), pinned
by config_guard. The release notes say how to upgrade by hand.
```

Then `git add -- app/src-tauri/tauri.conf.json app/src-tauri/tauri.bundle.conf.json app/src-tauri/src/config_guard.rs .github/workflows/ci.yml .github/workflows/release.yml .github/release-notes.md`, check `git diff --cached --name-only`, then `git commit -F .git/CMDSTUDIO_COMMIT_MSG`.

- [ ] **Step 8: CI is the gate (after the controller's push)**

The implementer reads the `app` job's log of the next run: the bundle step signs (no "no private key" error), the listing prints `CMD Studio_<v>_x64-setup.exe.sig`, the artifact holds three files. A `workflow_dispatch` of `release.yml` (the controller's dry run) proves the `version` and `ci` jobs with the secrets passed; the `draft` job runs only on the founder's tag.

---
### Task 7: The upstream-watch workflow — **PR creation prefers a founder token**

Runs after Task 5; needs no founder step (the token is optional). Spec §6.2 and §6.4: a daily workflow asks GitHub for the newest release of Oh My Posh, Clink and Nerd Fonts; when one is newer than `scripts/fetch-resources.ps1` pins, a script downloads what that file pins, computes every SHA-256, rewrites the pins in place, and the workflow opens one pull request that CI fails until a person finishes the rest. The script runs on this PC only in `-WhatIf` and `-Fixture` mode, through its own test, which touches no network and writes only under the folder it is given.

**Files:**
- Create: `scripts/upstream-watch.ps1`
- Create: `scripts/upstream-watch.test.ps1`
- Create: `.github/workflows/upstream-watch.yml`
- Modify: `.github/workflows/ci.yml` (`windows` job: the test in fixture mode)

**Interfaces:**
- Consumes: `scripts/fetch-resources.ps1`'s three tables (`$Downloads` with `Name`/`Url`/`Sha256`, `$Pinned`, the `Manifest` function's `& $bundle '<version>' '<prefix>'` lines), its `$ClinkSource` line and its `.SYNOPSIS` versions; GitHub's `GET /repos/<owner>/<repo>/releases/latest` (`tag_name`, `assets[].name`, `assets[].browser_download_url`); `gh pr list`, `gh pr create`.
- Produces: `upstream-watch.ps1 [-Script <file>] [-Fixture <dir>] [-WhatIf]` (prints `nothing newer upstream`, or `would change:`/`changed:` with one line per component `Oh My Posh 31.5.0 -> 31.6.0`); `upstream-watch.test.ps1 -Work <dir>` (prints `upstream-watch tests: N/N pass`, exit 1 on a failure); the workflow's PR titled `upstream: <versions>` on the branch `upstream/<yyyy-mm-dd>` with the body's checklist.

- [ ] **Step 1: The test, first**

`scripts/upstream-watch.test.ps1`:

```powershell
# Tests scripts/upstream-watch.ps1 without the network and without touching the repo: a fixture
# folder stands in for GitHub (three release documents and the assets fetch-resources.ps1 pins,
# with made-up bytes), and the script rewrites a copy of fetch-resources.ps1 under -Work. Runs on
# the dev PC (-Work: the session's scratchpad) and in CI (-Work: $RUNNER_TEMP). Prints one line per
# check and a total; exits 1 when one failed.
param([Parameter(Mandatory = $true)] [string] $Work)
$ErrorActionPreference = 'Stop'
$fail = 0; $total = 0
function Expect($what, $ok) { $script:total++; if ($ok) { "pass: $what" } else { "FAIL: $what"; $script:fail++ } }
function Hash([string] $Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
$watch = Join-Path $PSScriptRoot 'upstream-watch.ps1'
$original = Join-Path $PSScriptRoot 'fetch-resources.ps1'
$Work = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Work)
New-Item -ItemType Directory -Force -Path $Work | Out-Null
$fixture = Join-Path $Work 'fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
$copy = Join-Path $Work 'fetch-resources.ps1'
$originalText = [IO.File]::ReadAllText($original)
$utf8 = New-Object Text.UTF8Encoding $false

# A release document as GitHub's API shapes it, under <owner>-<repo>.json.
function Release([string] $Repo, [string] $Tag, [string[]] $Assets) {
  $doc = @{ tag_name = $Tag; assets = @($Assets | ForEach-Object { @{ name = $_; browser_download_url = "fixture://$_" } }) }
  [IO.File]::WriteAllText((Join-Path $fixture (($Repo -replace '/', '-') + '.json')), ($doc | ConvertTo-Json -Depth 5), $utf8)
}
# An archive with these files at its root, as Clink's and Nerd Fonts' are.
function Zip([string] $Name, [hashtable] $Files) {
  $src = Join-Path $Work ('src-' + $Name)
  if (Test-Path -LiteralPath $src) { Remove-Item -LiteralPath $src -Recurse -Force }
  New-Item -ItemType Directory -Path $src | Out-Null
  foreach ($k in $Files.Keys) { [IO.File]::WriteAllText((Join-Path $src $k), $Files[$k], $utf8) }
  $zip = Join-Path $fixture $Name
  if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip }
  Compress-Archive -Path (Join-Path $src '*') -DestinationPath $zip
}
[IO.File]::WriteAllText((Join-Path $fixture 'posh-windows-amd64.exe'), 'MZ posh 31.6.0', $utf8)
[IO.File]::WriteAllText((Join-Path $fixture 'COPYING'), 'MIT 31.6.0', $utf8)
Zip 'clink.1.9.35.abc123.zip' @{ 'clink_x64.exe' = 'MZ clink 1.9.35'; 'clink_dll_x64.dll' = 'MZ dll 1.9.35'; 'LICENSE' = 'GPL' }
Zip 'CascadiaCode.zip' @{ 'CaskaydiaCoveNerdFont-Regular.ttf' = 'ttf 3.6.0'; 'LICENSE' = 'OFL' }
Release 'JanDeDobbeleer/oh-my-posh' 'v31.6.0' @('posh-windows-amd64.exe')
Release 'chrisant996/clink' 'v1.9.35' @('clink.1.9.35.abc123.zip')
Release 'ryanoasis/nerd-fonts' 'v3.6.0' @('CascadiaCode.zip')

# 1. -WhatIf names the change and writes nothing.
Copy-Item -LiteralPath $original -Destination $copy -Force
$out = & $watch -Script $copy -Fixture $fixture -WhatIf | Out-String
Expect '-WhatIf names the three versions' ($out -match 'Oh My Posh 31\.5\.0 -> 31\.6\.0' -and $out -match 'Clink 1\.9\.34 -> 1\.9\.35' -and $out -match 'Nerd Fonts 3\.5\.1 -> 3\.6\.0')
Expect '-WhatIf shows the new download lines' ($out -match '\+ .*releases/download/v31\.6\.0/posh-windows-amd64\.exe')
Expect '-WhatIf writes nothing' ([IO.File]::ReadAllText($copy) -eq $originalText)

# 2. The rewrite: every pin of the three components, the manifest's versions, the Clink source
# line, the synopsis; the rewritten script still parses; the repo's own script is untouched.
$out = & $watch -Script $copy -Fixture $fixture | Out-String
$new = [IO.File]::ReadAllText($copy)
$exe = Hash (Join-Path $fixture 'posh-windows-amd64.exe')
$cop = Hash (Join-Path $fixture 'COPYING')
$zip = Hash (Join-Path $fixture 'clink.1.9.35.abc123.zip')
$font = Hash (Join-Path $fixture 'CascadiaCode.zip')
Expect 'the script says changed' ($out -match 'changed:' -and $out -match 'Clink 1\.9\.34 -> 1\.9\.35')
Expect 'omp url and hash' ($new.Contains("releases/download/v31.6.0/posh-windows-amd64.exe'; Sha256 = '$exe'"))
Expect 'omp COPYING url and hash' ($new.Contains("oh-my-posh/v31.6.0/COPYING'; Sha256 = '$cop'"))
Expect 'omp pinned' (($new -match "'omp\\oh-my-posh\.exe'\s*= '$exe'") -and ($new -match "'LICENSES\\oh-my-posh-MIT\.txt'\s*= '$cop'"))
Expect 'clink download line' ($new.Contains("Name = 'clink.1.9.35.abc123.zip'; Url = 'https://github.com/chrisant996/clink/releases/download/v1.9.35/clink.1.9.35.abc123.zip'; Sha256 = '$zip'"))
Expect 'clink expand key' ($new.Contains("`$got['clink.1.9.35.abc123.zip']"))
Expect 'clink source' ($new.Contains('Clink 1.9.35 is GPL-3.0') -and $new.Contains('/clink/tree/v1.9.35'))
Expect 'font url and hash' ($new.Contains("nerd-fonts/releases/download/v3.6.0/CascadiaCode.zip'; Sha256 = '$font'"))
Expect 'manifest versions' ($new.Contains("'31.6.0' 'omp'") -and $new.Contains("'1.9.35' 'clink'") -and $new.Contains("'3.6.0' 'fonts'"))
Expect 'synopsis' ($new.Contains('Oh My Posh 31.6.0, Clink 1.9.35 (x64') -and $new.Contains('Nerd Fonts 3.6.0'))
$x = Join-Path $Work 'x'
if (Test-Path -LiteralPath $x) { Remove-Item -LiteralPath $x -Recurse -Force }
Expand-Archive -LiteralPath (Join-Path $fixture 'clink.1.9.35.abc123.zip') -DestinationPath $x
Expect 'clink files pinned' (($new -match "'clink\\clink_x64\.exe'\s*= '$(Hash (Join-Path $x 'clink_x64.exe'))'") -and ($new -match "'LICENSES\\clink-GPL-3\.0\.txt'\s*= '$(Hash (Join-Path $x 'LICENSE'))'"))
$errors = $null
[System.Management.Automation.Language.Parser]::ParseFile($copy, [ref]$null, [ref]$errors) | Out-Null
Expect 'the rewritten script parses' ($errors.Count -eq 0)
Expect 'the repo script is untouched' ([IO.File]::ReadAllText($original) -eq $originalText)

# 3. Nothing newer: said, and nothing changes.
Copy-Item -LiteralPath $original -Destination $copy -Force
Release 'JanDeDobbeleer/oh-my-posh' 'v31.5.0' @('posh-windows-amd64.exe')
Release 'chrisant996/clink' 'v1.9.34' @('clink.1.9.34.4e8ae4.zip')
Release 'ryanoasis/nerd-fonts' 'v3.5.1' @('CascadiaCode.zip')
$out = & $watch -Script $copy -Fixture $fixture | Out-String
Expect 'nothing newer is said' ($out -match 'nothing newer upstream')
Expect 'nothing newer changes nothing' ([IO.File]::ReadAllText($copy) -eq $originalText)

"upstream-watch tests: $($total - $fail)/$total pass"
if ($fail) { exit 1 }
```

- [ ] **Step 2: Run it to see it fail**

Run (PowerShell 5.1, from the repo root): `.\scripts\upstream-watch.test.ps1 -Work '<scratchpad>\upstream-test'`
Expected: it stops at the first `& $watch` with "The term '…upstream-watch.ps1' is not recognized" (the script does not exist). Nothing under the repo changed (`git status --porcelain` is empty).

- [ ] **Step 3: The script**

`scripts/upstream-watch.ps1`:

```powershell
<#
.SYNOPSIS
  Watches upstream for newer releases of the add-ons CMD Studio bundles (spec §6.2, §6.4, phase
  4b): Oh My Posh, Clink and Nerd Fonts. For each newer release it downloads what
  fetch-resources.ps1 pins, computes every SHA-256 (the downloads and the files taken out of the
  archives), and rewrites that script's pins in place. Nothing else changes: the versions in the
  core, credits.ts, docs/credits.md, the release notes, the README and the parity test are a
  person's, which the pull request the workflow opens lists. Works in Windows PowerShell 5.1 and
  PowerShell 7.
.PARAMETER Script
  The fetch-resources.ps1 to read and rewrite. Default: the one beside this script.
.PARAMETER Fixture
  A folder that stands in for GitHub: <owner>-<repo>.json (the release document, as the API
  shapes it) and the assets under their names. No network is used then (upstream-watch.test.ps1).
.PARAMETER WhatIf
  Print the lines that would change; write nothing.
#>
param(
  [string] $Script = (Join-Path $PSScriptRoot 'fetch-resources.ps1'),
  [string] $Fixture = '',
  [switch] $WhatIf
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$Script = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Script)
$text = [IO.File]::ReadAllText($Script)

function Hash([string] $Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function NewDir([string] $Parent) {
  $d = Join-Path $Parent ('cmdstudio-upstream-' + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Path $d | Out-Null
  $d
}
# The version pinned for an asset: the `$Downloads` line that names it, .../download/v<version>/...
function PinnedVersion([string] $Text, [string] $AssetName) {
  $m = [regex]::Match($Text, "Name = '" + [regex]::Escape($AssetName) + "'[^\r\n]*?/releases/download/v(?<v>[^/']+)/")
  if (-not $m.Success) { throw "fetch-resources.ps1 has no download line for $AssetName" }
  $m.Groups['v'].Value
}
# Clink's zip is named with the version and a commit hash: both come from the line.
function PinnedClink([string] $Text) {
  $m = [regex]::Match($Text, "Name = 'clink\.(?<v>[0-9.]+)\.(?<h>[0-9a-f]+)\.zip'")
  if (-not $m.Success) { throw 'fetch-resources.ps1 has no Clink download line' }
  @{ Version = $m.Groups['v'].Value; Name = 'clink.' + $m.Groups['v'].Value + '.' + $m.Groups['h'].Value + '.zip' }
}
function LatestRelease([string] $Repo) {
  if ($Fixture) { return (Get-Content -LiteralPath (Join-Path $Fixture (($Repo -replace '/', '-') + '.json')) -Raw | ConvertFrom-Json) }
  Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ 'User-Agent' = 'cmd-studio-upstream-watch'; Accept = 'application/vnd.github+json' } -UseBasicParsing
}
function Fetch([string] $Url, [string] $Name, [string] $Dir) {
  $file = Join-Path $Dir $Name
  if ($Fixture) { Copy-Item -LiteralPath (Join-Path $Fixture $Name) -Destination $file }
  else { Invoke-WebRequest -Uri $Url -OutFile $file -UseBasicParsing }
  $file
}
function AssetUrl($Release, [string] $Name) {
  $a = @($Release.assets | Where-Object { $_.name -eq $Name })
  if ($a.Count -ne 1) { throw "release $($Release.tag_name) has $($a.Count) assets named $Name" }
  $a[0].browser_download_url
}
function Newer([string] $Old, [string] $New) { [version] $New -gt [version] $Old }
function Tag($Release) { "$($Release.tag_name)".TrimStart('v') }
# One replacement that must match exactly once, so a reshaped fetch-resources.ps1 stops the
# script instead of being half rewritten. In the replacement, `${1}` is the first group and `$$`
# a literal dollar.
function ReplaceOnce([string] $Text, [string] $Pattern, [string] $Replacement, [string] $What) {
  $count = [regex]::Matches($Text, $Pattern).Count
  if ($count -ne 1) { throw "${What}: expected one match of '$Pattern', found $count" }
  [regex]::Replace($Text, $Pattern, $Replacement)
}

$work = NewDir ([IO.Path]::GetTempPath())
$changes = @()
try {
  $new = $text

  # Oh My Posh: the exe, and COPYING at the tag (raw.githubusercontent, not a release asset).
  $ompOld = PinnedVersion $text 'posh-windows-amd64.exe'
  $omp = LatestRelease 'JanDeDobbeleer/oh-my-posh'
  $ompNew = Tag $omp
  if (Newer $ompOld $ompNew) {
    $dir = NewDir $work
    $exe = Fetch (AssetUrl $omp 'posh-windows-amd64.exe') 'posh-windows-amd64.exe' $dir
    $copying = Fetch "https://raw.githubusercontent.com/JanDeDobbeleer/oh-my-posh/v$ompNew/COPYING" 'COPYING' $dir
    $exeHash = Hash $exe
    $copyingHash = Hash $copying
    $o = [regex]::Escape($ompOld)
    $new = ReplaceOnce $new ("(Name = 'posh-windows-amd64\.exe'; Url = '[^']*/v)" + $o + "(/posh-windows-amd64\.exe'; Sha256 = ')[0-9a-f]{64}") ('${1}' + $ompNew + '${2}' + $exeHash) 'omp download'
    $new = ReplaceOnce $new ("(Name = 'oh-my-posh-COPYING'; Url = '[^']*/v)" + $o + "(/COPYING'; Sha256 = ')[0-9a-f]{64}") ('${1}' + $ompNew + '${2}' + $copyingHash) 'omp COPYING'
    $new = ReplaceOnce $new "('omp\\oh-my-posh\.exe'\s*= ')[0-9a-f]{64}" ('${1}' + $exeHash) 'omp pinned'
    $new = ReplaceOnce $new "('LICENSES\\oh-my-posh-MIT\.txt'\s*= ')[0-9a-f]{64}" ('${1}' + $copyingHash) 'omp license pinned'
    $new = ReplaceOnce $new ("(& \`$bundle ')" + $o + "(' 'omp')") ('${1}' + $ompNew + '${2}') 'omp manifest'
    $new = ReplaceOnce $new ('Oh My Posh ' + $o) ('Oh My Posh ' + $ompNew) 'omp synopsis'
    $changes += "Oh My Posh $ompOld -> $ompNew"
  }

  # Clink: the one zip asset named clink.<version>.<hash>.zip; three files taken out of it.
  $clinkOld = PinnedClink $text
  $clink = LatestRelease 'chrisant996/clink'
  $clinkNew = Tag $clink
  if (Newer $clinkOld.Version $clinkNew) {
    $asset = @($clink.assets | Where-Object { $_.name -match '^clink\.[0-9.]+\.[0-9a-f]+\.zip$' })
    if ($asset.Count -ne 1) { throw "release $($clink.tag_name) has $($asset.Count) assets named like clink.<version>.<hash>.zip" }
    $zipName = $asset[0].name
    $dir = NewDir $work
    $zip = Fetch $asset[0].browser_download_url $zipName $dir
    $x = NewDir $dir
    Expand-Archive -LiteralPath $zip -DestinationPath $x
    $zipHash = Hash $zip
    $exeHash = Hash (Join-Path $x 'clink_x64.exe')
    $dllHash = Hash (Join-Path $x 'clink_dll_x64.dll')
    $licenseHash = Hash (Join-Path $x 'LICENSE')
    $oldName = [regex]::Escape($clinkOld.Name)
    $o = [regex]::Escape($clinkOld.Version)
    $new = ReplaceOnce $new ("Name = '" + $oldName + "'; Url = '([^']*/v)" + $o + '/' + $oldName + "'; Sha256 = '[0-9a-f]{64}") ("Name = '" + $zipName + "'; Url = '" + '${1}' + $clinkNew + '/' + $zipName + "'; Sha256 = '" + $zipHash) 'clink download'
    $new = ReplaceOnce $new ('\$got\[' + "'" + $oldName + "'" + '\]') ('$$got[' + "'" + $zipName + "'" + ']') 'clink expand'
    $new = ReplaceOnce $new "('clink\\clink_x64\.exe'\s*= ')[0-9a-f]{64}" ('${1}' + $exeHash) 'clink exe pinned'
    $new = ReplaceOnce $new "('clink\\clink_dll_x64\.dll'\s*= ')[0-9a-f]{64}" ('${1}' + $dllHash) 'clink dll pinned'
    $new = ReplaceOnce $new "('LICENSES\\clink-GPL-3\.0\.txt'\s*= ')[0-9a-f]{64}" ('${1}' + $licenseHash) 'clink license pinned'
    $new = ReplaceOnce $new ('"Clink ' + $o + ' is GPL') ('"Clink ' + $clinkNew + ' is GPL') 'clink source line'
    $new = ReplaceOnce $new ('/clink/tree/v' + $o) ('/clink/tree/v' + $clinkNew) 'clink source url'
    $new = ReplaceOnce $new ("(& \`$bundle ')" + $o + "(' 'clink')") ('${1}' + $clinkNew + '${2}') 'clink manifest'
    $new = ReplaceOnce $new ('Clink ' + $o + ' \(x64') ('Clink ' + $clinkNew + ' (x64') 'clink synopsis'
    $changes += "Clink $($clinkOld.Version) -> $clinkNew"
  }

  # Nerd Fonts: the CascadiaCode archive; the one font file and its license taken out of it.
  $fontOld = PinnedVersion $text 'CascadiaCode.zip'
  $fonts = LatestRelease 'ryanoasis/nerd-fonts'
  $fontNew = Tag $fonts
  if (Newer $fontOld $fontNew) {
    $dir = NewDir $work
    $zip = Fetch (AssetUrl $fonts 'CascadiaCode.zip') 'CascadiaCode.zip' $dir
    $x = NewDir $dir
    Expand-Archive -LiteralPath $zip -DestinationPath $x
    $zipHash = Hash $zip
    $ttfHash = Hash (Join-Path $x 'CaskaydiaCoveNerdFont-Regular.ttf')
    $licenseHash = Hash (Join-Path $x 'LICENSE')
    $o = [regex]::Escape($fontOld)
    $new = ReplaceOnce $new ("(Name = 'CascadiaCode\.zip'; Url = '[^']*/v)" + $o + "(/CascadiaCode\.zip'; Sha256 = ')[0-9a-f]{64}") ('${1}' + $fontNew + '${2}' + $zipHash) 'font download'
    $new = ReplaceOnce $new "('fonts\\CaskaydiaCoveNerdFont-Regular\.ttf'\s*= ')[0-9a-f]{64}" ('${1}' + $ttfHash) 'font pinned'
    $new = ReplaceOnce $new "('LICENSES\\CaskaydiaCove-NF-OFL-1\.1\.txt'\s*= ')[0-9a-f]{64}" ('${1}' + $licenseHash) 'font license pinned'
    $new = ReplaceOnce $new ("(& \`$bundle ')" + $o + "(' 'fonts')") ('${1}' + $fontNew + '${2}') 'font manifest'
    $new = ReplaceOnce $new ('Nerd Fonts ' + $o) ('Nerd Fonts ' + $fontNew) 'font synopsis'
    $changes += "Nerd Fonts $fontOld -> $fontNew"
  }

  if ($changes.Count -eq 0) { 'nothing newer upstream'; return }
  if ($WhatIf) {
    'would change:'
    $changes
    $oldLines = $text -split "`r?`n"
    $newLines = $new -split "`r?`n"
    for ($i = 0; $i -lt $newLines.Count; $i++) {
      if ($oldLines[$i] -ne $newLines[$i]) { '- ' + $oldLines[$i]; '+ ' + $newLines[$i] }
    }
    return
  }
  [IO.File]::WriteAllText($Script, $new, (New-Object Text.UTF8Encoding $false))
  'changed:'
  $changes
} finally {
  Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
```

Each `ReplaceOnce` names its pattern and the text it edits; a `fetch-resources.ps1` reshaped so that a pattern matches twice or not at all stops the script with that name, which is what a reviewer of the PR wants to see rather than a half-rewritten file. The `.SYNOPSIS` of `fetch-resources.ps1` reads "Oh My Posh 31.5.0, Clink 1.9.34 (x64 files only), the CaskaydiaCove NF font from Nerd Fonts 3.5.1": the three synopsis patterns match those words once each (the comment "the v31.5.0 tag" under `# Downloads.` says `v31.5.0`, not `Oh My Posh 31.5.0`, and the `$ClinkSource` line says `Clink 1.9.34 is`, not `Clink 1.9.34 (x64`).

- [ ] **Step 4: Run the test**

Run: `.\scripts\upstream-watch.test.ps1 -Work '<scratchpad>\upstream-test'`
Expected: `upstream-watch tests: 17/17 pass`, exit code 0; `git status --porcelain` is empty (the repo's `fetch-resources.ps1` is untouched). Parse-check both scripts with `ParseFile` (0 errors each).

- [ ] **Step 5: The workflow, and CI's step**

`.github/workflows/upstream-watch.yml`:

```yaml
# The upstream watch (spec §6.2, §6.4, phase 4b): every day, the newest releases of Oh My Posh,
# Clink and Nerd Fonts; when one is newer than scripts/fetch-resources.ps1 pins, the script
# rewrites the pins and this workflow opens one pull request with what a person finishes. It
# never merges, never tags, never pushes to main. A pull request's checks run only when the push
# came from a person's token: GITHUB_TOKEN's pushes start no workflow, so without the founder's
# fine-grained UPSTREAM_WATCH_TOKEN (contents: write, pull requests: write, this repository) the
# PR is opened all the same and the founder closes and reopens it to run CI (with GITHUB_TOKEN
# the repository setting "Allow GitHub Actions to create and approve pull requests" must be on).
# The token is referenced as a secret only and never printed.
name: upstream-watch
on:
  schedule:
    - cron: '17 3 * * *'
  workflow_dispatch:
permissions:
  contents: write
  pull-requests: write
concurrency:
  group: upstream-watch
  cancel-in-progress: false
jobs:
  watch:
    runs-on: ubuntu-latest
    timeout-minutes: 20
    steps:
      - uses: actions/checkout@v4
        with:
          token: ${{ secrets.UPSTREAM_WATCH_TOKEN || github.token }}
      # The script's own test first (fixture mode: no network, nothing of the repo written), so a
      # script that would half-rewrite the pins never runs against the real releases.
      - name: The script's own test (fixture mode, no network)
        shell: pwsh
        run: ./scripts/upstream-watch.test.ps1 -Work "$env:RUNNER_TEMP/upstream-test"
      - name: Rewrite the pins when upstream is newer
        shell: pwsh
        run: ./scripts/upstream-watch.ps1
      - name: One pull request, unless one for these versions is open
        env:
          GH_TOKEN: ${{ secrets.UPSTREAM_WATCH_TOKEN || github.token }}
        shell: bash
        run: |
          if git diff --quiet -- scripts/fetch-resources.ps1; then
            echo "nothing newer upstream"
            exit 0
          fi
          # The three versions, from the release download URLs (the exe, the Clink zip, the font).
          versions="$(grep -o -E "releases/download/v[^/']+" scripts/fetch-resources.ps1 | sed 's#releases/download/v##' | tr '\n' ' ' | sed 's/ $//')"
          title="upstream: $versions"
          if [ -n "$(gh pr list --state open --search "\"$title\" in:title" --json number --jq '.[].number')" ]; then
            echo "a pull request for these versions is open already: $title"
            exit 0
          fi
          branch="upstream/$(date -u +%Y-%m-%d)"
          git config user.name "github-actions[bot]"
          git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
          git switch -c "$branch"
          git add -- scripts/fetch-resources.ps1
          git commit -m "scripts: upstream pins ($versions)"
          git push --set-upstream origin "$branch"
          cat > body.md <<'EOF'
          The daily upstream watch found a newer release of a bundled add-on and rewrote the pins (the URLs and every SHA-256) in `scripts/fetch-resources.ps1`. This pull request is not mergeable as it is: CI fails until a person finishes what the script cannot:

          - [ ] `OMP_VERSION` and `CLINK_VERSION` in `crates/core/src/plan.rs`, `NERD_FONTS_VERSION` in `crates/core/src/resources.rs`
          - [ ] the versions in `app/ui/src/screens/credits.ts`, `docs/credits.md`, `.github/release-notes.md` and `README.md`
          - [ ] the parity test (`crates/core/tests/parity.rs`) against the new Oh My Posh: fix the preview's simulator, never lower the test (spec §6.4)
          - [ ] the doctor's reading of `oh-my-posh version` (D2) if its output changed

          Nothing is merged or tagged by this workflow; the founder reviews, merges, and releases through `release.yml`.
          EOF
          gh pr create --title "$title" --body-file body.md --base main --head "$branch"
```

In `.github/workflows/ci.yml`, `windows` job, after the "Format" step:

```yaml
      # The upstream watch's script (phase 4b) in fixture mode: no network, nothing of the repo
      # written, under Windows PowerShell 5.1 as on the dev PC (upstream-watch.yml runs it under
      # PowerShell 7 on Linux too).
      - name: Upstream watch script (fixture mode)
        shell: powershell
        run: ./scripts/upstream-watch.test.ps1 -Work "$env:RUNNER_TEMP\upstream-test"
```

Parse both workflow files.

- [ ] **Step 6: Commit**

Write `.git/CMDSTUDIO_COMMIT_MSG`:

```
scripts, ci: the upstream watch (spec §6.2, §6.4, phase 4b)

upstream-watch.ps1 reads the pins of fetch-resources.ps1, asks GitHub
for the newest release of Oh My Posh, Clink and Nerd Fonts, and for
each newer one downloads what that file pins, computes every SHA-256
(the downloads and the files taken out of the archives) and rewrites
the download lines, the pinned hashes, the manifest's versions, the
Clink source line and the synopsis in place, each replacement exactly
once or the script stops; -WhatIf prints, -Fixture stands in for
GitHub. upstream-watch.test.ps1 runs it over a fixture under -Work,
on this PC and in CI (windows, PowerShell 5.1). upstream-watch.yml
runs daily and on dispatch, opens one pull request per set of
versions on upstream/<date> with the human steps listed, never
merges, never tags; UPSTREAM_WATCH_TOKEN when the founder sets it,
else GITHUB_TOKEN and a close-and-reopen for CI.
```

Then `git add -- scripts/upstream-watch.ps1 scripts/upstream-watch.test.ps1 .github/workflows/upstream-watch.yml .github/workflows/ci.yml`, check `git diff --cached --name-only`, then `git commit -F .git/CMDSTUDIO_COMMIT_MSG`.

**Founder gate (optional):** the fine-grained PAT `UPSTREAM_WATCH_TOKEN` (contents: write, pull requests: write, this repository only), or the repository setting that lets Actions open pull requests. Without either, the workflow's `gh pr create` fails with a permission error and the founder decides; nothing else breaks.

---

### Task 8: The main spec records phase 4b

Runs after Task 7 for Steps 1–3 (the design as built, with the spike's decision from Task 1's `results.md`), and its Step 4 after Task 9 (the VM facts). Thai, in the spec's register; code names in English. `cargo test -p cmdstudio-core --test error_codes` and `cargo test -p cmdstudio-app --lib error::` keep passing (§5.9 is unchanged).

**Files:**
- Modify: `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§5.3, §5.7, §5.10, §6.2, §6.4, appendix B)

- [ ] **Step 1: §5.3, §5.7, §5.10**

§5.3's tree: after the line `├─ settings.json.reset-<ms>  …` add two lines:

```
├─ update.json          บันทึกของตัวอัปเดต (phase 4b, §6.2): `{"version":1,"from":"<เวอร์ชันเดิม>","to":"<เวอร์ชันใหม่>","notesUrl":"<หน้า release>"}` เขียนในขั้นเดียวก่อนรันตัวติดตั้งรุ่นใหม่ · การเปิดครั้งถัดไปอ่านหลัง `run.json` ก่อนสร้างหน้าต่าง ถ้า `to` ตรงกับเวอร์ชันที่รันอยู่ แสดง "อัปเดตเป็น vX แล้ว" (บรรทัด `update installed to=`) ไม่ตรงคือตัวติดตั้งทำไม่จบ (`update failed to=`) · ไฟล์ที่อ่านไม่ได้คงไว้และบอก ไฟล์ที่เสีย รุ่นอื่น หรือ `to` ไม่ใช่ SemVer ลบและบอก (`update record=`) · `app_ready` ลบเมื่อหน้าจอแสดงประกาศแล้ว ประกาศคงอยู่จนปิดการ์ดหรือปิดแอป
├─ updates\             ตัวติดตั้งที่ตัวอัปเดตดาวน์โหลดและ plugin ตรวจลายเซ็นแล้ว (`cmdstudio-<เวอร์ชัน>-setup.exe` เขียนในขั้นเดียว แอปเก็บ SHA-256 ของไบต์ที่ตรวจแล้วไว้ในหน่วยความจำและเทียบก่อนรัน) · การเปิดครั้งถัดไปลบทั้งโฟลเดอร์ก่อนเช็กใหม่ ไม่เชื่อไฟล์ข้ามการเปิด (phase 4b)
```

In the `logs\` line (the one that lists the phase 3b-2 groups), before its final sentence about `logs\crash-<unix ms>.txt`, add: `· กลุ่มของ phase 4b ตัวอัปเดต (§6.2): \`update mode=off\` · \`update check=offline|manifest|signature|none\` · \`update available=<เวอร์ชัน>\` · \`update download=offline|manifest|signature|install|write\` · \`update downloaded=<เวอร์ชัน>\` · \`update postponed=busy|lock|record\` · \`update install=file-changed|spawn|<ชนิด>\` · \`update installing=<เวอร์ชัน> how=exit|now\` · \`update installed to=<เวอร์ชัน>\` · \`update failed to=<เวอร์ชัน> running=<เวอร์ชัน>\` · \`update record=unreadable|damaged\` · \`update updates=kept\` · \`update check=thread <เหตุผล>\` · \`update reapply=ok|<รหัส>|thread <เหตุผล>\` · \`update pruned=<n>\` มีแต่ชื่อ เวอร์ชันและรหัส ไม่มี URL path หรือข้อมูลของผู้ใช้`.

§5.7: after the `app_ready, app_status, …` bullet, a new bullet:

```
- `update_status`, `update_install` (phase 4b, §6.2): `update_status` อ่านอย่างเดียว คืน `{ mode, state, checkedAtMs }` โดย `state` เป็น `off` · `idle` · `checking` · `available{version}` · `downloading{version}` · `ready{version}` · `installing{version}` · `postponed{version, reason}` · `update_install` คือ "อัปเดตตอนนี้" (การ์ดของโหมดแจ้งก่อนที่หน้าแรก และหน้าโหมดกู้คืน recovery spec §5.2): ดาวน์โหลดถ้ายังไม่ได้ ถือล็อก §5.5 (`E_BUSY` ถ้ามีคำสั่งที่เปลี่ยนเครื่องอยู่ ไม่มีอะไรรอคืน `E_INTERNAL`) เขียน `update.json` เรียก `end_run` แล้วให้ `Update::install` ของ plugin รันตัวติดตั้งแบบเงียบพร้อมเปิดแอปกลับ (`/S /R /UPDATE /ARGS`) ซึ่งจบโปรเซสนี้เอง คืนคำตอบเฉพาะเมื่อทำไม่ได้ (เหมือน `app_restart`) · `app_status` เพิ่ม `updated: { from, to, notesUrl, reapplyFailed } | null` เมื่อการเปิดครั้งนี้ตามหลังอัปเดตที่สำเร็จ · `app_ready` เริ่มการเช็กเวอร์ชันใหม่ใน thread ของตัวเอง (ครั้งเดียวต่อการเปิด หน้าต่างไม่รอเน็ต) และลบ `update.json` · `open_external` รับ `{ releaseNotes: { version } }` เปิด `https://github.com/luraselenehalo/cmd-studio/releases/tag/v<version>` เฉพาะเมื่อ `version` เป็น SemVer
```

In the paragraph after the list, "ไฟล์ capability อนุญาตเฉพาะ 37 คำสั่งนี้" → "39 คำสั่งนี้", and append "· plugin updater ลงทะเบียนคำสั่ง JS ของมันเอง แต่ capability ไม่ให้สิทธิ์ `updater:` ใดๆ หน้าต่างจึงเรียกไม่ได้ (phase 4b)". In the `--cleanup --data-dir` sentence: the folder list gains `` `updates\` `` after `` `health\` ``; the file list gains `` `update.json` `` after `` `run.json` ``; the temp names gain `` `.update.json.<hex 32 ตัว>.cmdstudio-tmp` ``; the "at least one own file" list gains `` `update.json` ``; the phase note gains "· `update.json`, `updates\` และไฟล์ชั่วคราวของ `update.json` เพิ่มใน phase 4b".

§5.10: "เปิด capability ของ Tauri เฉพาะ 30 คำสั่ง IPC ใน §5.7" → "เฉพาะ 39 คำสั่ง IPC ใน §5.7"; the last bullet gains: "· ข้อยกเว้นเดียว (phase 4b, §6.2): ตัวติดตั้งรุ่นใหม่ที่ plugin updater ตรวจลายเซ็น minisign กับ `pubkey` ใน `tauri.conf.json` แล้ว รันจาก `<data>\updates\` (แอปเทียบ SHA-256 ของไฟล์กับของไบต์ที่ตรวจแล้วก่อนรัน) ด้วย `/S /UPDATE` หรือจากไฟล์ชั่วคราวของ plugin ด้วย `/S /R /UPDATE` เท่านั้น ไม่มีสิ่งใดจาก `latest.json` ไปถึง command line นอกจากเวอร์ชัน (ตรวจเป็น SemVer) · `pubkey` ว่างถูกปฏิเสธก่อนส่งคำขอใดๆ".

- [ ] **Step 2: §6.2 and §6.4**

§6.2: replace the bullet that begins `  - ติดตั้งตอนไหน (ตอนเปิดก่อนขึ้นหน้าจอ, ตอนปิดโปรแกรม หรือขอให้เปิดใหม่) ให้ phase 4 เลือก` (the whole bullet, through `ภาคผนวก B 2026-11-04)`) with:

```
  - ติดตั้งตอนไหน: ✅ phase 4b ตัดสินจาก spike บน VM (`docs/spikes/phase4-update/results.md`, <date>): **<the decision line of results.md: ตอนปิด เงียบ ไม่เปิดกลับ | ตอนเปิด เมื่อว่าง | ขอให้เปิดใหม่>** · โหมดอัตโนมัติ: ดาวน์โหลดเบื้องหลังหลัง `app_ready` แล้วตอนปิดโปรแกรม ถ้าล็อก §5.5 ว่าง (`try_lock` ไม่ว่างคือเลื่อนไปครั้งหน้า `update postponed=busy`) แอปรันตัวติดตั้งที่ตรวจแล้วเองด้วย `/S /UPDATE` โดยไม่เปิดแอปกลับ และคืนล็อกเมื่อโปรเซสจบ · "อัปเดตตอนนี้" (โหมดแจ้งก่อน และหน้าโหมดกู้คืน) ใช้ `Update::install` ของ plugin ซึ่งรัน `/S /R /UPDATE /ARGS` แล้วจบโปรเซส แอปเปิดกลับเอง · ทั้งสองทางไม่รันตัวถอนรุ่นเก่าเลย (template ของ Tauri: `/S` ข้ามหน้า reinstall และ `$UpdateMode = 1` ข้ามตัวถอน อ่านจาก template เมื่อ 2026-11-05) hook `$UpdateMode` ของ 4a จึงเป็นกันไว้เฉยๆ · ติดตั้งทับด้วยมือ: หน้า "already installed" ของตัวติดตั้งรุ่นใหม่ให้เลือก "<U6's second radio>" (ติดตั้งทับ เก็บธีมและการตั้งค่า) หรือ "<U6's first radio>" (ค่าเริ่มต้น: รันตัวถอนรุ่นเก่าโดยไม่มี `/UPDATE` ล้างเต็ม วิซาร์ดใหม่) release notes บอกไว้ · ทุกแบบเขียน `<data>\update.json` (`version`, `from`, `to`, `notesUrl`) ก่อนรันตัวติดตั้ง แล้วการเปิดครั้งถัดไปอ่านไฟล์นั้นเพื่อแสดง "อัปเดตเป็น vX แล้ว" พร้อมลิงก์ ใช้ธีมซ้ำถ้าเวอร์ชันของส่วนเสริมที่ pin เปลี่ยน แล้ว `app_ready` ลบไฟล์ · โหมด "ปิด" ไม่เช็กเลย (`update mode=off`) และทุกโหมดเมื่อเน็ตไม่มี manifest เสีย หรือลายเซ็นไม่ตรง เขียน log หนึ่งบรรทัดแล้วเงียบ (§5.3) · ตรวจสอบจาก `settings.json` ทุกครั้งที่เปิด ไฟล์เสียคือค่าเริ่มต้น (อัตโนมัติ) · เวลาที่วัดบน VM: ปิดถึงรุ่นใหม่ <n> วินาที คลิก "อัปเดตตอนนี้" ถึงหน้าต่างกลับ <n> วินาที (architect เลือก founder เปลี่ยนได้ ภาคผนวก B 2026-11-05)
```

In the same §6.2, after the "หลังอัปเดต" list's item 3 (`ลบโฟลเดอร์ runtime เก่า…`), add:

```
  · phase 4b: ข้อ 2 ทำที่การเปิดครั้งถัดไปแบบปกติ (ไม่ทำในโหมดกู้คืน) ใน thread ใต้ `busy = "update_reapply"` (หน้าจอรอด้วยข้อความของมันเอง recovery spec §6.3) เมื่อ `state.json` อ่านได้และบอกว่าธีมที่ใช้อยู่ใช้ OMP หรือ Clink คนละเวอร์ชันกับที่ build นี้ pin (`plan::runtime_changed` ติดตั้งทับด้วยมือก็เข้าข้อนี้) ผ่าน `plan_apply`/`apply` ตามปกติ (ล็อก จุดย้อนกลับ ขั้นตรวจผล) · ล้มเหลว (ย้อนกลับ Clink ตัวอื่นโผล่ `E_AUTORUN_FOREIGN_CLINK` หรือ `E_BUSY`) คงธีมบน runtime เดิมที่ `state.json` ยังระบุ ประกาศบอกรหัส (`updated.reapplyFailed`) และเปิดครั้งหน้าลองใหม่ · ข้อ 3 ทำทุกครั้งที่เปิดและหลังข้อ 2 (`cleanup::prune_old_runtimes`): `runtime\omp-*`, `runtime\clink-*` และ `clink-*` ใน ProgramData ของผู้ใช้ ที่ไม่ใช่เวอร์ชันที่ pin และไม่ใช่โฟลเดอร์ที่ `state.json` ระบุ โฟลเดอร์ที่ลบไม่ได้ (cmd ยังถือ DLL) รอครั้งหน้า `state.json` อ่านไม่ได้ไม่ลบอะไร (`update pruned=<n>`)
```

§6.4: after the phase 4a bullet, a new bullet:

```
- **phase 4b (architect 2026-11-05 founder เปลี่ยนได้):** `release.yml` เซ็นตัวติดตั้งด้วยคีย์ minisign ของ founder (`TAURI_SIGNING_PRIVATE_KEY` และ `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` ใน secrets ส่งให้ `ci.yml` ด้วย `secrets: inherit`; `createUpdaterArtifacts` ใน `tauri.bundle.conf.json` ทำให้ตัวติดตั้งทุกตัวที่ CI สร้างถูกเซ็นและมี `.sig` ใน artifact) → อัปโหลด `CMD-Studio_<เวอร์ชัน>_x64-setup.exe`, `.sig` ของมัน, `latest.json` (`{version, notes, pub_date, platforms.windows-x86_64.{signature, url}}` โดย `url` ชี้ asset นั้นใต้ tag ของ release) และ `SHA256SUMS.txt` ลง draft ที่**ไม่ติดธง pre-release** เพราะ endpoint ของแอปคือ `releases/latest/download/latest.json` ซึ่ง GitHub ให้จาก release ล่าสุดที่เผยแพร่แล้วและไม่ติดธง ชื่อและข้อความยังบอกว่าเป็น pre-release สำหรับผู้ทดสอบ (ทางเลือกของ founder: release คงที่ tag `updater` ที่ workflow แทนที่ `latest.json` ด้วย `gh release upload --clobber` ทุกครั้ง และ endpoint `releases/download/updater/latest.json`) · plugin `tauri-plugin-updater` =2.13.3 ใช้จาก Rust เท่านั้น ไม่มีแพ็กเกจ npm · workflow เฝ้ารุ่นใหม่ (`upstream-watch.yml` ทุกวัน 03:17 UTC และ `workflow_dispatch`): `scripts/upstream-watch.ps1` ถาม `releases/latest` ของ OMP, Clink และ Nerd Fonts เจอรุ่นใหม่กว่าจะดาวน์โหลด คำนวณ SHA-256 ทุกไฟล์ แล้วเขียน pin ใน `scripts/fetch-resources.ps1` เท่านั้น (แต่ละจุดแทนที่ต้องตรงหนึ่งครั้ง ไม่เช่นนั้นหยุด) เปิด PR หนึ่งอันต่อชุดเวอร์ชันบน branch `upstream/<วันที่>` (ไม่ซ้ำถ้ามี PR ชื่อเดียวกันเปิดอยู่) ซึ่ง CI ต้องล้มจนกว่าคนจะแก้เวอร์ชันใน core, `credits.ts`, `docs/credits.md`, release notes, README และ parity test ตามรายการใน PR · ใช้ `UPSTREAM_WATCH_TOKEN` ของ founder เพื่อให้ CI ของ PR รัน ไม่มีก็เปิด PR ได้แต่ founder ต้องปิด-เปิด PR · มี `upstream-watch.test.ps1` โหมด fixture ไม่ใช้เน็ต รันใน CI · เลขเวอร์ชันใน workflow อ่านจากบรรทัดใต้ `[workspace.package]` เท่านั้น (4a M6)
```

- [ ] **Step 3: Appendix B, then commit**

A new last row:

```
| 2026-11-05 | Phase 4b ตัวอัปเดตและ upstream watch (architect เลือก founder เปลี่ยนได้ แผนอยู่ใน `docs/superpowers/plans/2026-11-04-phase4b-updater-upstream.md` ผลอ่านซอร์สใน "What the sources settled" ของแผน):<br>• ช่วงเวลาติดตั้งตามผล spike (§6.2): อัตโนมัติ = ตอนปิด เงียบ ไม่เปิดกลับ ด้วยการรันตัวติดตั้งเองด้วย `/S /UPDATE`; "อัปเดตตอนนี้" = `Update::install` ของ plugin (`/S /R /UPDATE /ARGS` แอปเปิดกลับเอง) ทั้งสองเป็นโค้ดจริง ไม่มี dead code<br>• endpoint คงที่ `releases/latest/download/latest.json` และ release ไม่ติดธง pre-release ตั้งแต่ release แรกที่มี updater assets (ชื่อยังบอกว่า pre-release สำหรับผู้ทดสอบ)<br>• `pubkey` ว่างถูก `TauriUpdater::check` ปฏิเสธก่อนส่งคำขอ (plugin เองไม่อ่าน pubkey ตอน check)<br>• `update.json` ในโฟลเดอร์ข้อมูล เขียนก่อนรันตัวติดตั้ง `app_ready` ลบ; `updates\` ถูกลบทุกครั้งที่เปิดก่อนเช็ก และไฟล์ที่ดาวน์โหลดต้องตรง SHA-256 ของไบต์ที่ตรวจแล้วก่อนรัน<br>• การใช้ธีมซ้ำหลังอัปเดตทำใต้ชื่อ busy ของตัวเอง `update_reapply` (ไม่อยู่ใน `CHANGES_MACHINE` ซึ่งเป็นรายการคำสั่ง IPC) และตัดสินจาก `state.json` อย่างเดียว ติดตั้งทับด้วยมือจึงได้ด้วย<br>• ลบ runtime เก่าแบบ best effort ทุกครั้งที่เปิด ยกเว้นที่ `state.json` ระบุ<br>• PR ของ upstream watch แก้ `scripts/fetch-resources.ps1` อย่างเดียว ที่เหลือเป็นของคนตามรายการใน PR; token ของ founder เป็นทางเลือก<br>• ไม่มีแพ็กเกจ npm ของ plugin (หน้าต่างไม่เรียก plugin)<br>• minor ของ 4a ที่พับเข้า: M6 (grep เวอร์ชันใต้ `[workspace.package]`) ส่วน M9 และ M10 คงไว้ (ไฟล์ไม่ถูกแก้ใน 4b)<br>• `tauri-plugin-updater` =2.13.3 (ใหม่สุดของ 2.13.x ใน index 2026-11-05, tauri ^2.12) |
```

Run `cargo test -p cmdstudio-core --test error_codes` and `cargo test -p cmdstudio-app --lib error::` (Expected: pass; §5.9 is untouched). `git diff docs/superpowers/specs/2026-10-10-cmd-studio-design.md` touches §5.3, §5.7, §5.10, §6.2, §6.4 and appendix B only. `git add -- docs/superpowers/specs/2026-10-10-cmd-studio-design.md`.

```
spec: phase 4b as designed and spiked in §5.3, §5.7, §5.10, §6.2, §6.4 and appendix B
```

- [ ] **Step 4: The VM facts (after Task 9)**

Append to the 2026-11-05 row, before its closing ` |`: `<br>• VM check ของ phase 4b (<date>, from-build <sha> CI run <id> 0.1.0, to-release v0.1.1): <verdict per VM, one clause each: B1–B5 บน VM-B, B6 (cleanup ที่ทำไม่จบ ย้ายมาจาก 4a ขั้น 6) บน <VM-A|VM-C>, B7 บน VM-C>; รายละเอียดใน \`docs/spikes/phase4-update/protocol.md\` § Results` and fill the two `<n> วินาที` of §6.2 from `results.md` if Step 2 left them as the spike measured them. Commit: `spec: the phase 4b VM check in appendix B`.

---

### Task 9: The update VM check — **waits for a published release with updater assets**

Spec §8's phase 4 criterion, the half 4a left: update passes on every tier. The real pubkey and the real endpoint are in the build from Task 6 on, and a draft's assets are not reachable through `releases/latest`, so the check runs against **the first published release with updater assets**. The "from" build is CI's `cmdstudio-installer` of this branch at version 0.1.0 (any run after Task 6: signed with the founder's key, the real endpoint, the updater code), the "to" release is `v0.1.1`, which the controller's version bump (Step 0) and the founder's tag and publish make. 4a's VM step 6 (a cleanup that cannot finish, its box, the kept data folder) runs here, on a VM with Windows Terminal. The protocol and the guest script are Task 1's; this task adds part B.

**PC safety, for this whole task:** every script here is for the VMs and is parse-checked only on this PC. The controller runs the protocol after the final review, with one download of the from-build with the founder's explicit yes.

**Files:**
- Modify: `docs/spikes/phase4-update/protocol.md` (part B)
- Modify: `docs/spikes/phase4-install/protocol.md` (Step 4 only: the result of its step 6 and 6b, once run)
- The version bump (Step 0, the controller's commit): `Cargo.toml`, `Cargo.lock` (the three workspace packages' `version`), `app/package.json`, `app/package-lock.json` (two places)

**Interfaces:**
- Consumes: Task 1's `update-check.ps1`, `helpers.ps1`, Task 6's release assets, 4a's `docs/spikes/phase4-install/helpers.ps1` and `install-check.ps1` (`-HoldFont`, `-Uninstall`, `-CleanupAll`), `config_guard::the_version_has_one_source`.
- Produces: part B's steps with PASS lines; the Results section; the 4a protocol's step-6 result.

- [ ] **Step 0: The version bump (controller, after the final review)**

In the scratch worktree: `Cargo.toml` `version = "0.1.0"` → `"0.1.1"` under `[workspace.package]`; `cargo update -w` (the three `cmdstudio-*` entries of `Cargo.lock` follow; nothing else changes: check `git diff Cargo.lock` shows only those three `version =` lines); `app/package.json` `"version": "0.1.1"`; `app/package-lock.json`: the root `"version"` and `packages[""].version` (two places; `npm install --package-lock-only --ignore-scripts` in `app/` does it and must change nothing else). `cargo test -p cmdstudio-app --lib config_guard::the_version_has_one_source` passes. Commit (`git add -- Cargo.toml Cargo.lock app/package.json app/package-lock.json`):

```
release: version 0.1.1 (phase 4b)

The first release with updater assets; the branch's 0.1.0 builds are
the update check's "from".
```

- [ ] **Step 1: Part B of `protocol.md`**

Append to `docs/spikes/phase4-update/protocol.md`:

```markdown
## Part B: the update check (Task 9)

- **What it checks:** spec §8's phase 4 criterion, the update half: on every tier the app installed at 0.1.0 finds v0.1.1 on GitHub, installs it at close (automatic) or on "Update now" (notify), keeps AutoRun, the theme, the Path entry and the shortcuts, says "Updated to v0.1.1" with a working link, and is silent in Off mode and offline; and 4a's open step 6: an uninstall whose cleanup cannot finish keeps the data folder and says so.
- **The builds:** the from-build is CI's `cmdstudio-installer` artifact of a run of this branch at version 0.1.0 after Task 6 (`gh run list --workflow ci.yml --branch phase4b --limit 10 --json databaseId,headSha,status,conclusion`; the run's `app` job log shows the `.sig`). **One download, with the founder's explicit yes:** `gh run download <run id> --name cmdstudio-installer --dir <scratchpad>\p4b-from`. The to-release is **v0.1.1 published** by the founder (founder step 3 below): `gh release view v0.1.1 --json assets --jq '.assets[].name'` lists `CMD-Studio_0.1.1_x64-setup.exe`, its `.sig`, `latest.json` and `SHA256SUMS.txt`, and `curl -sL https://github.com/luraselenehalo/cmd-studio/releases/latest/download/latest.json | jq .version` prints `"0.1.1"`. Record the run id and the release in Results.
- **VMs:** B1–B5 and B7 need no Windows Terminal: VM-B runs them when VM-A does not start; B6 needs Windows Terminal (the font): VM-A or VM-C from a powered-off `clean` (the founder's VM step). B7 is VM-C's.
- **Setup:** as part A, with `$from = '<scratchpad>\p4b-from'`, `Install-Phase4bSetup $vm $u $from` (one installer), `$P4.Setup = $P4B.Setup`; no host server. The VM's NAT must reach GitHub (`Invoke-GuestPs $vm "(Invoke-WebRequest -UseBasicParsing https://github.com/luraselenehalo/cmd-studio/releases/latest/download/latest.json).StatusCode" $u 60` prints 200).

| # | Do | PASS when | Result |
|---|---|---|---|
| B1 | **The from-build installed, Terracotta applied.** `Start-Phase4Install $vm $u` (interactive, as 4a's step 2: SmartScreen or the Open File warning, the pages, "Run CMD Studio" ticked), the wizard to Terracotta applied, "Go to the home page"; `Close-Phase4App $vm $u`; `Get-Phase4bSnapshot $vm b1 $u`; `Get-GuestAutoRun $vm 'b1' $u`. | `installed: DisplayVersion=0.1.0`; AutoRun holds CMD Studio's part; `log:` has `update available=0.1.1` and `update downloaded=0.1.1` (automatic mode, the real endpoint); `updates:` lists `cmdstudio-0.1.1-setup.exe`. | |
| B2 | **The automatic install at close.** `Start-Phase4bApp $vm $u`; wait 60 s (the download from GitHub); `Get-Phase4bLog $vm $u`; `Close-Phase4App $vm $u`; at once `Wait-Phase4bVersion $vm 0.1.1 $u`; after 3 s `Save-Shot $vm 'p4b-b2-after-close'`; `Get-Phase4bSnapshot $vm b2 $u`; `Get-GuestAutoRun $vm 'b2' $u`. | `version 0.1.1 after <n> s`; the shot shows no window; `processes:` empty; `updateJson:` the record with `"to":"0.1.1"`; `log:` ends with `update installing=0.1.1 how=exit`; AutoRun unchanged; `path:` `1 entry(ies)`; `shortcuts:` the two; `install:` has `cmdstudio.com`, no `cmdstudio-cli.exe`. | |
| B3 | **The notice, the link, the CLI still on the Path.** `Start-Phase4bApp $vm $u`; wait 15 s; `Save-Shot $vm 'p4b-b3-notice'`; `Invoke-GuestButton $vm 'What changed' $u`; wait 8 s; `Save-Shot $vm 'p4b-b3-release-page'`; `Invoke-GuestButton $vm 'Close' $u`; `Get-Phase4bSnapshot $vm b3 $u`; `Close-Phase4App $vm $u`; `Get-Phase4Cli $vm $u`. | "Updated to v0.1.1." with its two buttons; the browser shows the v0.1.1 release page; `log:` has `update installed to=0.1.1` and no `update reapply=` (the pins did not change between 0.1.0 and 0.1.1); `updateJson: ABSENT`; `updates: ABSENT`; the `-Cli` output has the doctor's lines before `EXIT=` and `PROMPT-RETURNED` (the Path entry survived the update). | |
| B4 | **Notify and "Update now".** Fresh `Start-CleanVm`, Setup, B1 (closed); `Set-Phase4bMode $vm notify $u`; `Start-Phase4bApp $vm $u`; wait 30 s; `Save-Shot $vm 'p4b-b4-card'`; `Invoke-GuestButton $vm 'Update now' $u`; at once `Wait-Phase4bVersion $vm 0.1.1 $u`; wait 20 s; `Save-Shot $vm 'p4b-b4-back'`; `Get-Phase4bSnapshot $vm b4 $u`; `Get-GuestAutoRun $vm 'b4' $u`. | The card "CMD Studio v0.1.1 is available." with "Update now" and "Later", and `updates: ABSENT` before the click (notify downloads nothing); after the click the window goes, the version arrives in `<n> s`, the app is back by itself with "Updated to v0.1.1."; `log:` has `update installing=0.1.1 how=now` then `update installed to=0.1.1`; AutoRun unchanged. | |
| B5 | **Off, and offline.** From B4's state, close the app; `Set-Phase4bMode $vm off $u`; `Start-Phase4bApp $vm $u`; wait 20 s; `Get-Phase4bLog $vm $u`; `Close-Phase4App $vm $u`. Then `Set-Phase4bMode $vm auto $u`; `Set-Phase4bNic <vm name> off`; `Start-Phase4bApp $vm $u`; wait 30 s; `Save-Shot $vm 'p4b-b5-offline'`; `Get-Phase4bLog $vm $u`; `Close-Phase4App $vm $u`; `Set-Phase4bNic <vm name> on`. | `update mode=off`, then `update check=offline`; the shot shows the home page with no card and no box. | |
| B6 | **A cleanup that cannot finish (VM-A or VM-C; 4a's step 6 and 6b).** From a powered-off `clean`: 4a's Setup, 1, 2 (interactive) and 5 with **Tokyo Night** (the theme that installs the font), then 4a's steps 6 and 6b exactly as `docs/spikes/phase4-install/protocol.md` writes them (`Start-Phase4FontHold`, the ticked uninstall, the box, the kept data folder, the font hold ended, the silent reinstall, `Invoke-Phase4CleanupAll`, the second uninstall, `Compare-Phase4 … ThemesGone`). | 4a's PASS lines for 6 and 6b: the box "CMD Studio could not put everything back (cleanup result: 3)…" naming the font, `install folder: ABSENT`, the data folder kept with `state.json` and `snapshots`; then 6b's `EXIT=0`, no box, `VERDICT: PASS` with `DATA FOLDER GONE`. | |
| B7 | **Thai (VM-C).** B1 with the wizard in Thai, then B4. | The card and the notice in `th.ts`'s words (`มี CMD Studio v0.1.1 ให้อัปเดต`, `อัปเดตตอนนี้`, `อัปเดตเป็น v0.1.1 แล้ว`); the cmd window's welcome box English under the Thai user folder, as before. | |

### Results (part B)

Written by the controller: the date, the from-build's commit and run id, the release, each step's Result (PASS with the printed lines, or the finding), the screenshots `spikes\vm\tmp\<vm>-p4b-b*.png` by label, and the time from the close to the new version. Findings go to the task that owns them before the PR; B6's result is written into `docs/spikes/phase4-install/protocol.md`'s step 6 and 6b rows as well.
```

- [ ] **Step 2: Commit the protocol**

`git add -- docs/spikes/phase4-update/protocol.md`:

```
docs: the update VM check's protocol, part B (phase 4b Task 9)

The from-build (this branch at 0.1.0, signed) and the to-release
(v0.1.1 published): the automatic install at close, the notice and its
link, notify's Update now, Off and offline, 4a's open step 6 (a
cleanup that cannot finish) on a VM with Windows Terminal, and Thai.
```

- [ ] **Step 3: The run (controller)**

After the final review and the merge, the version bump (Step 0) and the founder's steps 2 and 3: the download with the founder's yes, the VMs, the Results sections (part B here, step 6 in 4a's protocol), committed by the controller (`docs: the update VM check's results (phase 4b Task 9)`), then Task 8 Step 4. A FAIL goes back to its task; a release that a FAIL makes wrong is the founder's to unpublish or supersede.

- [ ] **Step 4: 4a's protocol**

In `docs/spikes/phase4-install/protocol.md`, the Result cells of rows 6 and 6b get B6's outcome (the VM, the date, PASS or the finding), and the Results section's "6 and 6b open" sentence gains "(run in 4b's Task 9 on <VM>, <date>: <verdict>)". Committed with Step 3's results.

**Founder gates:** the key (Task 6) before any of this; the version bump merged; tagging and pushing `v0.1.1`; publishing the draft (a public post); a powered-off `clean` for VM-A or VM-C.

---

## Founder steps, in order, with what waits

1. **The updater key pair** (Task 6's steps 1–3) — Task 6 waits; Tasks 1–5, 7, 8 do not.
2. **The endpoint choice** (Task 6's step 4; the default needs no act) — Task 6's `release.yml` shape.
3. **Tag and push `v0.1.0`** after 4a merged, if wanted — nothing in 4b waits on it; a published v0.1.0 (no updater code) is upgraded by hand, as its notes will not say but v0.1.1's say.
4. **Merge 4b, then tag and push `v0.1.1`** (the controller's bump is in the PR) — the draft with updater assets.
5. **Publish the draft** (the first public release with `latest.json`; not flagged pre-release, titled "pre-release for testers") — Task 9 waits.
6. **A powered-off `clean` for VM-A or VM-C** — Task 9's B6 (and part A, if VM-A is wanted for the spike) waits; VM-B carries the rest.
7. **The firewall allow for `python3` on 8765** when Windows asks during Task 1's part A — the spike waits.
8. **`UPSTREAM_WATCH_TOKEN`** (optional) — Task 7's PRs run CI by themselves.
9. **SignPath** (apply after the first release; the commented step in `release.yml`) — not in 4b's tasks.
10. **winget** (after a signed release) — not in 4b's tasks.

## Self-review (done while writing; the executor need not repeat it)

- **Spec coverage.** §2.1 (no silent change: the notice once, with a link): Task 3's `update_json_makes_the_notice_once_and_goes_with_app_ready`, Task 4's card. §5.3 (`update.json`, `updates\`, the log lines): Tasks 2, 3, 8. §5.5 (the lock): Task 3's `a_busy_lock_postpones_the_install_and_tries_next_launch`, `update_lock`. §5.7 (the two commands, `--cleanup`'s list): Tasks 2, 4, 8. §5.9: no new row (E_BUSY, E_INTERNAL). §5.10 (the one outside program; the capability): Task 3's `install_quiet` and the hash check, Task 4's capability, Task 8. §6.2: every bullet: the check at every launch in the mode of `settings.json` (Task 3), the three modes (Task 3), no interruption (Task 3), no focus (Task 3's own spawn, Task 4's cards), the notice (Tasks 3, 4), the moment ⬜ → the spike (Task 1) and Task 8, offline silent (Task 3), the key as a secret (Task 6), add-ons with the app only (no download path anywhere; Task 7's PR), the upstream workflow (Task 7), "after an update" 1–3 (Task 5 with Task 2's `runtime_changed`, `prune_old_runtimes`), old theme files converted (phase 2's, unchanged). §6.4 (the manifest, the signing, the upstream watch, the version source): Tasks 6, 7, 8. §8 (update on every tier): Task 9. Recovery spec §5.2 ("Update now" line): Task 4's `UpdateOffer` on the Recovery page; §6.3 (the busy wait after a reload; the journal rule): Task 5's `update_reapply` under `UPDATE_REAPPLY_BUSY`, the "now" path through `end_run` like `app_restart`. 4a's carries: the asset name (Task 6's `latest.json` url and `.sig`), the by-hand upgrade (U6, the notes in Task 6), step 6 (Task 9's B6), M6 (folded), M9/M10 (left, said).
- **Placeholders.** `<date>`, `<sha>`, `<id>`, `<n>`, `<l>`, `<the founder's key, one line>`, `<U6's … radio>`, `<the decision line of results.md>` stand for values the implementer or the controller measures or the founder gives, and are named as such; no "TBD", no "similar to Task N", no step without its code.
- **Types.** `Updater::{check, download, install_now}` (Task 3) are what `Backend::{update_check_and_download, update_install}` call; `FakeUpdater::{offering, none, failing, calls}` are what the tests use; `Backend::updated()` feeds `AppStatus.updated` (Task 4) and `Updated.reapply_failed` (Task 5); `update_at_exit(&dyn Fn(&Path) -> Result<(), String>)` takes `update::install_quiet` (Task 3's `lib.rs`); `Paths::{update_file, updates_dir}` (Task 2) are what Task 3 reads and the CLI knows; `runtime_changed`, `runtime_dirs_named_by`, `prune_old_runtimes` (Task 2) are what Task 5 calls; `ExternalTarget::ReleaseNotes { version }` is `{ releaseNotes: { version } }` on the wire (Task 4's UI and mock); `UpdateState`'s `kind` tags match the mock's `updateState`; `UPDATE_REAPPLY_BUSY = "update_reapply"` is the string `StartWait` compares; `release_url` is used by `TauriUpdater::check`, `update_after_start` and `external_places`.
- **Review Focus.** 1 in Task 3's `a_download_that_changed_on_disk_is_not_installed`; 2 in Task 3's `a_busy_lock_postpones_the_install_and_tries_next_launch`; 3 in Task 3's `a_failed_install_is_logged_and_the_file_goes`; 4 in Task 3's `the_release_url_takes_semver_only` and `a_damaged_update_record_is_logged_and_removed`; 5 in Task 5's `a_failed_re_apply_keeps_the_old_runtime_and_says_so` and Task 2's `old_runtime_folders_go_and_held_ones_stay`.
- **The sources against the outline.** F1 withdrawn (the plugin's quiet install relaunches: `/S /R`); U2 reworded (no uninstaller runs in update mode or under `/S`: the template); the "both branches, one dead" of the outline replaced by two production paths; an empty pubkey refused by the app, not by the plugin; `update_reapply` a busy name of its own, not a `CHANGES_MACHINE` entry; no npm package; the plugin pinned at 2.13.3 (the index), not 2.13.2 (G10's table); `latest.json`'s url and `.sig` on the hyphenated asset name (4a's carry); `--prerelease` named at `release.yml:78`; the `from` version of the record from the built config, not `CARGO_PKG_VERSION` (the spike's `--config` version).
