# Phase 3b-2 Implementation Plan — Recovery mode, the app side: `run.json` and safe mode, `--recovery`, crash notes, hangs, and the Recovery shortcut

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Task 1's VM half and Task 12's run are the controller's (VirtualBox VMs); an implementer never runs them.

**Goal:** CMD Studio does not die quietly: an app that fails to start twice opens its third time in a recovery (safe) mode from which everything can be undone; `cmdstudio.exe --recovery` (the "CMD Studio Recovery" Start-menu shortcut) reverts a cut-off change, pauses the theme on this computer and opens that mode; a panic leaves a redacted crash note that the next start points to, two clicks from the problem report; a window that cannot open offers to undo everything in a native Yes/No box; a command that takes long says so, a read-only one gives up after a minute with a retry, and a change that runs over three minutes offers a restart that the journal makes safe (recovery spec §1 criteria 3, 4, 5; §12's 3b-2).

**Architecture:** The core's `Host` adapter learns this process's id and creation time and those of any pid (`GetProcessTimes`), and `Paths` the new files. The app gets `backend/run.rs` (`run.json`, the safe-mode state, `app_ready`, `app_status`, the `--recovery` entry) and `crash.rs` (the panic hook that writes `logs\crash-<ms>.txt` through `redact`/`redact_sids`, keeps ten, never panics); `main.rs` sends `--recovery` to the window, not the CLI; `lib.rs` writes the run record before the window, ends it on a normal exit, turns `window_failed` into a Yes/No box over `Backend::revert_all`, and, if Task 1 passes, reloads the page when WebView2's renderer dies. Seven IPC commands join the thirty. The UI wraps its `Ipc` in `withWork` (a "still working" bar after 10 s, a 60-second deadline with `E_TIMEOUT` for read-only commands, late answers dropped, "Restart CMD Studio" after 3 minutes of a change), asks `app_status` first on every load, catches `window.onerror`/`unhandledrejection` into the existing `CrashNotice`, shows the crash notice in `StartNotices`, and has one Recovery page built from Help's existing parts.

**Tech Stack:** Rust 1.99.0, windows-sys 0.61.2 (`Win32_System_Threading`, `Win32_Foundation`, already on), Tauri 2.12.4 (`WebviewWindow::with_webview`, `tauri-plugin-single-instance` 2.5.4), webview2-com 0.39.1 and windows-core 0.62.2 (already in `Cargo.lock` through wry 0.57.0; the app names them as direct dependencies only if Task 1 passes), ts-rs 12.0.1, React 19.3.1, TypeScript 5.9.3, Vitest 5.0.5 (fake timers), mlua 0.10 `lua52` in the core's tests.

**Spec:** `docs/superpowers/specs/2026-10-17-recovery-mode-design.md` (the recovery spec: §0's WebView2 item, §1 criteria 3–6, §4.4, §5, §6, §7, §8, §9, §10, §12, §13) and the main spec `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§5.3, §5.7, §5.9, §6.1, appendix B). The carries from 3b-1 are in `docs/superpowers/plans/2026-10-17-phase3b-1-recovery-cmd-rulings.md` ("Carries", "Deferred minors", the founder's decision of 2026-10-19). Where the recovery spec and the shipped code (main `60061c0`) disagree, this plan follows the code and says so ("Where this plan departs from the recovery spec").

**Prerequisite:** branch `phase3b-2`, cut from main `60061c0` (PR #10 merged). Baselines measured on this PC at `60061c0`: the app's `lib` 227 tests, the core's `lib` 774, `health_lua` 33, the CLI's `lib` 55, the UI 261 (12 files). Every count below is a delta on these; measure the real total when a task ends and write it into the commit message.

## Global Constraints

Every task's requirements include these. Values are copied from the recovery spec, the main spec, the founder's decisions and the 3b-1 rulings where they bind here.

- **No Thai in cmd (recovery spec §1.6, §9).** Nothing CMD Studio writes into a cmd window, and nothing the CLI prints, is Thai. The Windows message boxes of `console.rs` are English (the UI is not up). `crates/core/tests/english_in_cmd.rs` keeps passing.
- **The UI's languages (founder 2026-10-16; recovery spec §9).** The app opens in English; Thai is chosen in Settings (in safe mode, read from `settings.json` when it can be read, else English). Every UI text is a key of both `app/ui/src/i18n/en.ts` and `th.ts`, in `th.ts`'s words: the prompt is "พร้อมท์", the add-on "ส่วนเสริม", a version "เวอร์ชัน", the app "CMD Studio" (never "เรา"), a backup "จุดย้อนกลับ" (never "สำรอง"). `i18n.test.ts` fails on a missing key, an empty text, Thai in English or English-only Thai, a placeholder that differs, and a text of 30 or more characters under two keys.
- **Rust production code** (outside `#[cfg(test)]` and `tests/`): no `unwrap`, `expect`, `panic!`, unchecked indexing or slicing, string slicing (the workspace denies clippy `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `string_slice`, `undocumented_unsafe_blocks`); typed errors; every `unsafe` block carries a `// SAFETY:` comment. Never `panic = "abort"` in any profile (`Cargo.toml`'s comment: the journal needs unwinding).
- **The numbers, each in one place:** `FAILED_STARTS_FOR_SAFE_MODE = 2` and `CRASH_NOTES_KEPT = 10` in the app (`backend/run.rs`, `crash.rs`); `UI_ERRORS_PER_RUN = 20` (`backend/run.rs`); in the UI (`app/ui/src/ipc/work.ts`) `STILL_WORKING_MS = 10_000`, `READ_DEADLINE_MS = 60_000`, `RESTART_AFTER_MS = 180_000`; the renderer gives up after `RENDERER_FAILURES = 3` within `RENDERER_WINDOW_MS = 60_000` (`webview_watch.rs`, Task 6, if Task 1 passes). `STALE_AFTER_SECS = 60`, `FORGET_AFTER_SECS = 600`, `PAUSE_AFTER = 3` stay (the founder's 10-minute rule stays as it is).
- **The files (recovery spec §5.1, §5.3, §5.2, §7):** `<data>\run.json` is `{"version":1,"pid":<n>,"createdAt":<ms>|null,"startedAt":<ms>,"uiReadyAt":null|<ms>,"failedStarts":<n>,"safeMode":<bool>}`, written with `write_atomic`; `<data>\logs\crash-<unix ms>.txt` (UTF-8, redacted, ten kept); `<data>\settings.json.reset-<ms>`. `--cleanup` knows every one of them (Task 3). `revert_all` (core) touches none of them.
- **IPC (main spec §5.7, §5.9, §5.10).** From Task 7 the commands are the 30 of 3b-1 plus `app_ready`, `app_status`, `crash_dismiss`, `crash_report_read`, `settings_reset`, `log_ui_error` and `app_restart`: 37 names in `app/src-tauri/src/command_names.rs`, thin wrappers over `Backend`, each `async`, each through `blocking`, each logging once through `Backend::logged` (`ipc.rs`'s tests pin it; `logging` becomes 33). `capabilities/default.json` allows exactly these and `core:event:allow-listen`/`allow-unlisten`; no new event (`apply-progress` stays the only one); the CSP is unchanged; `config_guard.rs:3` says 37. Every failure crosses the IPC as `{code, messageTh, messageEn, details}` with the codes of main spec §5.9 and no other: `error.rs`'s `MEANINGS` equals the §5.9 rows (a test reads the spec), so `E_TIMEOUT` goes into both in one commit (Task 3).
- **The mock stays logic-free (main spec §5.6).** `app/ui/src/ipc/mock.ts` answers the new commands with fixed data per status shape, chosen by `MockOptions` (`safeMode`, `crashNotice`, `busy`, `failedStarts`); it works nothing out.
- **Bindings.** Regenerate them by the manual steps only: delete the generated `.ts` files that change, then `cargo test -p cmdstudio-app --lib export_bindings` with `TS_RS_EXPORT_DIR` set to `<repo>\app\ui\src\bindings`. Never run `scripts/export-bindings.ps1`; never pass `-ExecutionPolicy`.
- **The checks.** Each Rust task ends with `cargo fmt --all`, then `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features ts -- -D warnings`. Each UI task ends with `npm run typecheck` and `npm test`, then `TZ=UTC npm test` (in Git Bash: `TZ=UTC npm test`; in PowerShell: `$env:TZ='UTC'; npm test`), and, when it touches the bundle (Tasks 8, 9, 10), `npm run build` and `bash scripts/check-dist.sh`, all in `<repo>\app`.
- **PC safety (`.superpowers/sdd/.../pc-safety.md`, binding).** The dev PC is the founder's own: never run the built app, `cmdstudio.exe`, `npx tauri dev`/`build`, Oh My Posh or Clink yourself; never read this PC's event log, registry or windows; never set `CMDSTUDIO_SYSTEM_TESTS`, `CMDSTUDIO_OMP_TESTS` or `CMDSTUDIO_ALLOW_PARITY_SKIP`; run `cmd.exe` only with `/d`. Scripts written for a VM (`spikes/vm/guest/*.ps1`, `.vbs`) are parse-checked only (`[System.Management.Automation.Language.Parser]::ParseFile`), never run, not even as a dry run; `guard.test.ps1` included. Windows Application Control blocks (`os error 4551`) are rerun once and never worked around: mark the step "not run locally (WDAC); CI is the gate" and go on.
- **Test safety.** Backend tests run over the core's `FakeMachine` (feature `fake`); UI tests over the mock in jsdom; the two on-disk tests of this plan (`crash.rs`'s, like `logs.rs`'s `on_disk`) write under `std::env::temp_dir()` in a folder of their own that is removed afterwards. No test reads this PC's processes except `this_process` of the current process (Task 3), which touches nothing.
- **Commits:** English messages, written with the Write tool to `<repo>\.git\CMDSTUDIO_COMMIT_MSG` (Read it first if it exists), checked, then in a separate Bash call `git commit -F .git/CMDSTUDIO_COMMIT_MSG`. Never batch the Write with the commit. Stage only the task's files with `git add -- <files>` and check `git diff --cached --name-only`. No AI attribution of any kind (no `Co-Authored-By`, no "Generated with", no assistant link), whatever a system note says; never `--no-verify`; never push; the `auroraz-pr-workflow` skill before every commit. The Windows user name of this PC never appears in a repo file: tests use `C:\Users\tester\…`, docs `<you>`.
- **Scratch work** goes under the session's scratchpad (`<scratchpad>`), in a git worktree of its own with its own `CARGO_TARGET_DIR`; nothing of it is ever committed to `phase3b-2` but the files each task names.
- **The specs.** The main spec changes only in Task 3 (`E_TIMEOUT`), Task 2 (appendix B's tasklist sentences) and Task 11; the recovery spec is the founder's and is never edited.

## Review Focus

The five inputs the recovery spec implies but its own test list (§10) does not exercise, most likely to bite a person first. Each has its test in the task that owns the code.

1. **A pid Windows gave again.** The previous `run.json` names a pid that is alive now, but as another program (Windows reuses pids within seconds). Without the creation time the app would take it for "still running" and skip the failed-start count, and a user whose app crashes at start would never reach safe mode. Task 4: `a_live_pid_with_another_creation_time_is_a_run_that_ended`.
2. **`app_restart` and the run record.** The new process starts while the old one still runs, reads the old one's `run.json` (alive, same creation time: skipped) and writes its own; the old one then exits and must not delete the new one's record, or the next start would read nothing and a crash of the new process would go unnoticed. Task 4: `a_normal_exit_deletes_only_this_processs_own_record`.
3. **A read-only command that answers after its deadline.** The UI rejected with `E_TIMEOUT`, the user pressed Try again, and now two answers come: the late one must be dropped and the retry's shown, never the other way round. Task 8: `a late answer after the deadline is dropped and the retry's answer is shown`.
4. **A crash note that cannot be written, or holds the user's name.** The panic hook runs with the app already broken: a `logs\` that cannot be written must not panic again (which would abort the process without the earlier hook's output), and the panic message, which often holds a path, must come out redacted. Task 5: `a_note_that_cannot_be_written_is_dropped_without_a_second_panic` and `the_note_is_redacted_before_it_is_written`.
5. **`--recovery` on a computer already paused, or whose state.json cannot be read.** A second Revert everything on a computer that is already out would make a second `revertAll` restore point and a second record for nothing; and with state.json unreadable the entry must not revert blind, but say so on the Recovery page. Task 4: `recovery_entry_pauses_nothing_twice_and_says_when_it_cannot_tell`.

## Where this plan departs from the recovery spec

These follow from the shipped code at `60061c0` (file:line) or from a decision this plan makes where the spec is silent; the report to the founder lists them.

- **`--recovery` while the app is running (recovery spec §4.4) reloads the page instead of sending an event.** The single-instance callback (`app/src-tauri/src/lib.rs:47-52`) runs steps 1–2 on a thread of its own, then `window.eval("window.location.reload()")`; the reloaded UI asks `app_status` first (recovery spec §6.3's rule for every reload) and sees `safeMode: true`. Main spec §5.7 keeps `apply-progress` as the only event (§13 of the recovery spec made that choice for `recovery_status`); the capability file does not change.
- **"Start normally" (recovery spec §5.2) is `app_restart`.** In safe mode `app_ready` has already set `failedStarts` to 0, so a restart without arguments is a normal start; the spec's "reload the app page" would need one more IPC to leave safe mode in the backend. One command less; the restart takes under a second.
- **`app_status` (recovery spec §8) carries two fields more:** `safeModeReason` is a tagged enum (`failedStarts {count}` or `recovery`), and `recoveryEntry` says what the `--recovery` entry did (the start's notes, the pause's `PauseOutcome`), so the Recovery page can say it the §5.5 way (3b-1 Task 10's rulings for a pause that did not go through).
- **The crash notice's time (recovery spec §5.3).** The app cannot know when it died; the notice says the newest crash note's time when one is newer than the run that ended, else the time that run started ("CMD Studio closed unexpectedly after starting at …"). `hasReport` says which.
- **"Close" on the crash notice (recovery spec §5.3) is remembered for this run only** (`crash_dismiss`): the notice comes from the previous run's `run.json`, which this run has replaced, so the next start cannot show it again anyway. No file.
- **The panic hook in CLI mode writes to the default data folder** (`%LOCALAPPDATA%\ai.resincore.cmdstudio\logs`), installed in `main.rs` before the arguments are parsed: a `--data-dir` run's crash note goes there. `crates/cli/src/lib.rs:70-117` parses inside `run_with`; moving the hook behind it would need `redact`/`redact_sids` (`app/src-tauri/src/backend/diagnostics.rs:372, 483`) in the CLI crate.
- **The 60-second deadline (recovery spec §6.2) exempts the two commands that wait for the user** (`theme_import_preview`, `theme_export`: `app/src-tauri/src/ipc.rs:85-110`, a native dialog the user may leave open) and `app_restart` (it never answers). They are a third and fourth kind in the one list of Task 8, which a test checks for completeness.
- **The "still working" bar's task names are fourteen groups, not one per command** (recovery spec §6.1: "from the dictionary"): `ipc/work.ts` maps every method to one `work.*` key; the test of the list covers the map.
- **Commands that change the machine, for `busy` (recovery spec §6.3, §8):** `apply`, `undo_last`, `snapshot_restore`, `revert_all` and `theme_pause` (they run the core's journaled transactions: `app/src-tauri/src/backend/apply.rs:535, 666, 779, 821`, `recovery.rs:412`). `plan_apply` takes the lock and changes nothing; `theme_resume` at the cmd level writes only `health\`; `doctor_fix` only proposes.
- **No test-only "crash at start" variable in the build** (recovery spec §10): CI's artifact is a release build with no test feature. Task 12's VM check makes the two failed starts with `run.json` fixtures (a dead pid, `uiReadyAt: null`, `failedStarts: 1`) and the crash notice with `Stop-Process` while the window is up; the panic hook is proven by Task 5's tests, one of them on the real disk. Whether the founder wants a cargo feature `test-hooks` instead is an open question in the report.
- **WebView2 `ProcessFailed` (recovery spec §6.4) is in Task 6 only if Task 1 passes**; its fallback F1 drops §6.4 from this phase and records why.
- **m4 (3b-1 deferred):** `theme_resume` at `computerUnknown` returns a new `ResumeOutcome::NotKnown` ("state.json could not be read; fix it first") instead of the planner's `E_STATE_INVALID` (`app/src-tauri/src/backend/recovery.rs:496-512`). Task 6.
- **The liveness check (the founder, 2026-10-19):** `HEALTH_BODY` no longer runs tasklist; the fourth window pauses even while three hung windows are still open (recovery spec §1.1 as written). Task 2.

## Deferred minors of 3b-1 (rulings file, "Deferred minors")

Folded in, because 3b-2's code touches the same lines: **m4** (Task 6). **Still deferred**, named here so nobody looks for them: the round-trip test of `paused.json` (resume null, bare `at`); `utc::parse`'s leniency; `PauseRecord.version`'s doc; the CLI's lift after the lock; `clear_pause` failing only at `create_dir_all`; the EVT handle-closing pin; the probabilistic single-flight tests; `prompt.async` not pinned in `clink_settings`; `eventlog.rs:23`'s doc; D12's untested lines and its 1000-line details; the hand-typed guard text; `paused.json` written outside the lock (`--recovery` reuses `pause_machine` as it is); `find_theme` errors as "gone"; a revert that commits with a failed step read as a pause in effect; undo/restore deleting an unreadable `paused.json`; a `revertAll` point per retried start; the A/B/undo-B corner; D12's "1 time(s)" in the live count; Undo everything after an unlifted older app record; the mark-before-spawn pin by source text; the mock's D12 rows and its size (~1000 lines, grows here; a split into `mockRecovery.ts` is not made); the stacked editor's `<summary>` order; Home's reread once per mount; the start card hidden after a failed resume; the no-Data-filter rationale in two places; Help's D12 resume before `recovery_status`; the reader's last-sleep overshoot.

## Spike results the later tasks depend on

Task 1 records each result in `docs/spikes/phase3b-recovery/results-3b2.md`. The later tasks are written for the "assumes" column. When a result differs, the task named applies the fallback given below and its commit message names the spike result.

| # | What the spike shows | Tasks 2–12 assume | Used by | If not |
|---|---|---|---|---|
| S1 | `tauri::WebviewWindow::with_webview` gives `PlatformWebview::controller()` (an `ICoreWebView2Controller`), its `CoreWebView2()` an `ICoreWebView2`, and `add_ProcessFailed` with a `webview2_com::ProcessFailedEventHandler` compiles with the app's pins (webview2-com 0.39.1, windows-core 0.62.2 as direct dependencies) | it compiles; the handler's closure may hold a clone of the `ICoreWebView2` and call `Reload()` from it | Task 6 | F1 |
| S2 | on VM-A, with the handler registered, `Stop-Process` of the window's renderer (`msedgewebview2.exe --type=renderer` whose parent chain ends at `cmdstudio.exe`) fires the handler with `ProcessFailedKind` `RENDER_PROCESS_EXITED`, and `Reload()` brings the page back | yes: the UI comes back on its own within 5 s, `app_status` is asked, Home is drawn | Task 6, Task 12 | F2 |
| S3 | `GetProcessTimes` of this process and of a pid that does not exist, through `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` | this process: a creation time within 60 s of now; a pid that is not there: `OpenProcess` fails with `ERROR_INVALID_PARAMETER` (87) and the function answers `Ok(None)` | Task 3 | F3 |

### The fallbacks

- **F1 (S1 does not compile, or needs a dependency the lock file does not hold).** Recovery spec §6.4 is dropped from this phase, as §0 and §6.4 say: Task 6 skips its Step 7 (`webview_watch.rs` is not created, `Cargo.toml` gains no dependency, `lib.rs` registers nothing), Task 12's step 6 is marked "not run: §6.4 dropped (F1)", and Task 11's appendix B row says: "§6.4 (WebView2 ProcessFailed) ไม่ได้ทำใน phase นี้: Tauri 2.12.4 … <the compiler's reason, one line>". The report to the founder names it.
- **F2 (the handler fires but `Reload()` from inside it does nothing, or fires for every kind).** The handler only counts and hands the reload to the window's thread: replace `unsafe { reloader.Reload() }` in `webview_watch.rs` with `let _ = window.eval("window.location.reload()"); Ok(())` where `window` is a clone of the `tauri::WebviewWindow` captured before `with_webview` (it is `Send`), and keep the kind filter. If neither reloads the page, F1.
- **F3 (`OpenProcess` of a pid that is gone fails with another code, e.g. `ERROR_ACCESS_DENIED`, or `GetProcessTimes` is refused for this process).** `process_created_at` answers `Ok(None)` for *every* `OpenProcess` failure (the record's pid is then "not alive"; a process the user may not open is not this app's own earlier instance, which runs as this user), and the test `a_pid_that_is_not_there_is_none` loses its error-code assertion. `this_process` keeps `created_at_ms: None` when `GetProcessTimes` fails, and a run record with `createdAt: null` is compared on the pid alone (Task 4's `same_run`).

## File map

New files, one responsibility each:

- `crates/core/src/adapters/mod.rs` — `Host::this_process` and `Host::process_created_at`, `ProcessStamp` (Task 3).
- `crates/core/src/adapters/windows/host.rs` — their Windows implementation (Task 3).
- `crates/core/src/adapters/fake.rs` — `FakeHost.pid`, `.created_at_ms`, `.processes` (Task 3).
- `crates/core/src/recovery.rs` — `Paths::run_file()`, `Paths::settings_reset_file(ms)` (Task 3).
- `crates/cli/src/lib.rs` — `cmd_studios_own_file` accepts `run.json` and `settings.json.reset-<ms>` (Task 3).
- `app/src-tauri/src/error.rs` — the `E_TIMEOUT` row (Task 3).
- `app/src-tauri/src/backend/run.rs` — `RunRecord`, `RunState`, `Backend::{start_run, app_ready, end_run, app_status, recovery_entry, log_ui_error}`, `AppStatus`, `SafeModeReason`, `RecoveryEntry`, `CrashNotice` (Tasks 4, 5).
- `app/src-tauri/src/crash.rs` — the panic hook, `report_text`, `write_note`, `latest_note`, `CRASH_NOTES_KEPT` (Task 5).
- `app/src-tauri/src/console.rs` — `ask_yes_no` (Task 6).
- `app/src-tauri/src/webview_watch.rs` — the `ProcessFailed` handler (Task 6, if S1 and S2 pass).
- `app/src-tauri/src/lib.rs` — `run_gui(GuiStart)`, the run record around the window, the Yes/No path, the single-instance `--recovery`, the exit hook (Tasks 4, 5, 6).
- `app/src-tauri/src/main.rs` — `--recovery` to the window; the panic hook for both modes (Tasks 4, 5).
- `app/src-tauri/src/ipc.rs`, `command_names.rs`, `capabilities/default.json`, `config_guard.rs` — the seven commands (Task 7).
- `app/ui/src/ipc/types.ts`, `tauri.ts`, `mock.ts` — the seven methods (Task 7).
- `app/ui/src/ipc/work.ts` — `withWork`, `WorkStore`, the one list of command kinds and task names (Task 8).
- `app/ui/src/components/WorkingBar.tsx` — the bar (Task 8).
- `app/ui/src/start.tsx` — `withWork` around the IPC, the catch-all (Task 9).
- `app/ui/src/App.tsx` — `app_status` first, the busy wait, the crash notice, the Recovery route (Tasks 9, 10).
- `app/ui/src/screens/Recovery.tsx` — the Recovery page (Task 10).
- `app/ui/src/screens/Help.tsx` — `Doctor`, `RestorePointsTab`, `Revert`, `Report` exported; the crash note on the Report tab; the guide's shortcut sentence (Task 10).
- `docs/spikes/phase3b-recovery/results-3b2.md`, `vm-check-3b2.md`, `spikes/vm/guest/recovery-run.ps1`, `recovery-renderer.ps1`, `recovery-timing.ps1` (A/B), `recovery-filters.ps1` (the refilter's caller) (Tasks 1, 12).

---

### Task 1: The spike: WebView2 `ProcessFailed` through Tauri, and the process stamp (compile here, run on a VM)

Recovery spec §0 asks whether `ProcessFailed` is reachable under the pinned Tauri before §6.4 is built, and 3b-1's spike did not ask. The compile half runs on this PC in the scratch worktree; the VM half is the controller's. Also S3, which Task 3's code relies on: it is a unit test here, on this process only.

**PC safety, for this task:** the scratch worktree never starts the app; `cargo check`/`cargo test` of the spike's test module runs only `GetProcessTimes` on the test's own process and `OpenProcess` of a pid that is not there. The VM script `recovery-renderer.ps1` is parse-checked here and run by the controller on VM-A against CI's artifact of the branch at the end of Task 6.

**Files:**
- Create: `docs/spikes/phase3b-recovery/results-3b2.md`
- Create: `spikes/vm/guest/recovery-renderer.ps1`
- Modify: `spikes/vm/guest/guard.test.ps1` (`$changers`)
- Scratch only (never committed): `<scratchpad>\wt\app\src-tauri\src\webview_watch.rs`, `<scratchpad>\wt\app\src-tauri\Cargo.toml`, `<scratchpad>\wt\crates\core\src\adapters\windows\host.rs`

**Interfaces:**
- Consumes: `tauri::WebviewWindow::with_webview` (tauri 2.12.4 `src/webview/webview_window.rs:2498`), `PlatformWebview::controller()` (`src/webview/mod.rs:184`, returns a cloned `ICoreWebView2Controller`), `webview2_com::ProcessFailedEventHandler::create(Box<dyn FnMut(Option<ICoreWebView2>, Option<ICoreWebView2ProcessFailedEventArgs>) -> windows_core::Result<()>>)`, `ICoreWebView2::add_ProcessFailed(&self, handler, token: *mut i64)`, `ICoreWebView2::Reload()`, `ICoreWebView2ProcessFailedEventArgs::ProcessFailedKind(*mut COREWEBVIEW2_PROCESS_FAILED_KIND)` (webview2-com-sys 0.39.1 `src/bindings.rs:1299, 1396, 30853`).
- Produces: `results-3b2.md` with S1–S3 and the fallback taken; the exact `webview_watch.rs` text Task 6 Step 6 copies. **Verified while this plan was written** (a scratch worktree at `60061c0`, its own target dir, the app never started): with `webview2-com = "=0.39.1"` and `windows-core = "=0.62.2"` added, Step 2's file compiles, its test passes, and `cargo clippy -p cmdstudio-app --all-targets -- -D warnings` and `cargo fmt --check` are clean; Step 3's `process_stamp_tests` passes (this process's creation time within 60 s; pid 4294967294 is `None`), clippy clean for the core too. S2 is the only open question, for VM-A.

- [ ] **Step 1: The scratch worktree**

```bash
cd <repo>
git worktree add --detach <scratchpad>/wt HEAD
```

- [ ] **Step 2: S1, the handler (scratch)**

In `<scratchpad>\wt\app\src-tauri\Cargo.toml`, under `[target.'cfg(windows)'.dependencies]`, add:

```toml
# WebView2's ProcessFailed (recovery spec §6.4): the renderer's death reloads the page. Both
# versions are the ones wry 0.57.0 already pins in Cargo.lock; nothing new is downloaded.
webview2-com = "=0.39.1"
windows-core = "=0.62.2"
```

In `<scratchpad>\wt\app\src-tauri\src\lib.rs`, after `pub mod settings;`, add `#[cfg(windows)]\npub mod webview_watch;`. Create `<scratchpad>\wt\app\src-tauri\src\webview_watch.rs`:

```rust
//! WebView2's `ProcessFailed` (recovery spec §6.4): when the window's renderer exits or stops
//! answering, the page is loaded again by itself; from the third failure within a minute the
//! app gives up and asks the Yes/No question of §5.4 instead (`give_up`). Registered once the
//! window is built (`lib.rs`). Only the renderer's two kinds count: the GPU or a utility
//! process going is WebView2's own business, and the browser process going ends the window.
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Failures within this window of time count together (recovery spec §6.4: three in a minute).
pub const RENDERER_WINDOW_MS: u64 = 60_000;
/// From this many failures within [`RENDERER_WINDOW_MS`], the app stops reloading.
pub const RENDERER_FAILURES: u32 = 3;

/// The failures seen lately; `note` says whether this one is still to be reloaded (`true`) or
/// is the one to give up on (`false`). Pure, so it is tested without WebView2.
pub struct Failures {
    seen: Mutex<Vec<Instant>>,
}

impl Failures {
    pub fn new() -> Self {
        Failures {
            seen: Mutex::new(Vec::new()),
        }
    }

    pub fn note(&self, now: Instant) -> bool {
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        seen.retain(|t| now.duration_since(*t) < Duration::from_millis(RENDERER_WINDOW_MS));
        seen.push(now);
        u32::try_from(seen.len()).unwrap_or(u32::MAX) < RENDERER_FAILURES
    }
}

impl Default for Failures {
    fn default() -> Self {
        Self::new()
    }
}

/// Registers the handler on `window`'s `ICoreWebView2`. `give_up` runs on WebView2's thread (the
/// window's): it must only hand the work on (a thread that shows the box, then exits).
#[cfg(windows)]
pub fn watch_renderer(
    window: &tauri::WebviewWindow,
    give_up: impl Fn() + Send + Sync + 'static,
) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2, COREWEBVIEW2_PROCESS_FAILED_KIND,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
    };
    use webview2_com::ProcessFailedEventHandler;
    let failures = Failures::new();
    window
        .with_webview(move |webview| {
            let controller = webview.controller();
            // SAFETY: `controller` is the live controller of this window, cloned by Tauri on
            // the window's thread; the call reads one COM property into a new reference.
            let core: ICoreWebView2 = match unsafe { controller.CoreWebView2() } {
                Ok(c) => c,
                Err(_) => return,
            };
            let reloader = core.clone();
            let handler = ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                // SAFETY: `args` is the live event argument object WebView2 handed over, and
                // `kind` a stack value that outlives the call.
                unsafe { args.ProcessFailedKind(&mut kind) }?;
                if kind != COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                    && kind != COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE
                {
                    return Ok(());
                }
                if !failures.note(Instant::now()) {
                    give_up();
                    return Ok(());
                }
                // SAFETY: `reloader` is the live `ICoreWebView2` of this window.
                unsafe { reloader.Reload() }
            }));
            let mut token: i64 = 0;
            // SAFETY: `handler` is a COM object WebView2 keeps a reference to for as long as the
            // registration lives; `token` is a stack value that outlives the call.
            let _ = unsafe { core.add_ProcessFailed(&handler, &mut token) };
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two failures reload, the third within a minute gives up, and one a minute later starts
    /// the count again.
    #[test]
    fn the_third_failure_within_a_minute_gives_up() {
        let f = Failures::new();
        let t0 = Instant::now();
        assert!(f.note(t0));
        assert!(f.note(t0 + Duration::from_secs(10)));
        assert!(!f.note(t0 + Duration::from_secs(20)));
        // The two oldest are out of the window now: this one is the second again.
        assert!(f.note(t0 + Duration::from_secs(71)));
    }
}
```

Run, in the worktree, with its own target dir:

```bash
cd <scratchpad>/wt
CARGO_TARGET_DIR=<scratchpad>/target cargo check -p cmdstudio-app --lib 2>&1 | tail -20
CARGO_TARGET_DIR=<scratchpad>/target cargo test -p cmdstudio-app --lib webview_watch 2>&1 | tail -5
```

Expected: `Finished` with no error, and `test result: ok. 1 passed`. Record the exact outcome (or the first error) under S1 in `results-3b2.md`. A type error on `controller.CoreWebView2()` or `add_ProcessFailed` is F1 unless it is a one-line fix in the handler's types (record the fix: Task 6 copies the compiled text).

- [ ] **Step 3: S3, the process stamp (scratch)**

Append to `<scratchpad>\wt\crates\core\src\adapters\windows\host.rs` (the final code lives in Task 3; this is the same text):

```rust
/// When the process `pid` was created, in milliseconds since 1970 (`GetProcessTimes`), or `None`
/// when there is no such process: `OpenProcess` fails with `ERROR_INVALID_PARAMETER` for a pid
/// that is not in use. Any other failure is an error.
pub fn process_created_at(pid: u32) -> Result<Option<u64>, AdapterError> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, FILETIME,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: `OpenProcess` takes three plain values and returns a handle or null; it touches
    // no memory of ours.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        // SAFETY: reads the calling thread's last error code; no arguments.
        let code = unsafe { GetLastError() };
        if code == ERROR_INVALID_PARAMETER {
            return Ok(None);
        }
        return Err(last_error("OpenProcess"));
    }
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: `handle` is open (above), and the four out-pointers point at stack `FILETIME`s
    // that outlive the call.
    let ok = unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    // SAFETY: `handle` came from the successful `OpenProcess` above and is closed once, here.
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return Err(last_error("GetProcessTimes"));
    }
    Ok(Some(filetime_unix_ms(created)))
}

/// A `FILETIME` (100-nanosecond ticks since 1601) as milliseconds since 1970; 0 for a time
/// before 1970.
pub fn filetime_unix_ms(t: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    const TICKS_1601_TO_1970: u64 = 116_444_736_000_000_000;
    let ticks = (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    ticks.saturating_sub(TICKS_1601_TO_1970) / 10_000
}

/// This process's id.
pub fn current_pid() -> u32 {
    // SAFETY: no arguments, and it touches no memory of ours.
    unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() }
}

#[cfg(test)]
mod process_stamp_tests {
    use super::*;

    /// This process was created within the last minute; a pid that nothing uses is `None`
    /// (spike S3). Pid 4_294_967_294 is `u32::MAX - 1`: Windows pids are multiples of 4 below
    /// that, so nothing holds it.
    #[test]
    fn this_process_has_a_creation_time_and_a_missing_pid_is_none() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        let mine = process_created_at(current_pid()).unwrap().unwrap();
        assert!(now.saturating_sub(mine) < 60_000, "{mine} vs {now}");
        assert_eq!(process_created_at(4_294_967_294).unwrap(), None);
        let t = windows_sys::Win32::Foundation::FILETIME {
            dwLowDateTime: 0xD53E_8000,
            dwHighDateTime: 0x019D_B1DE,
        };
        assert_eq!(filetime_unix_ms(t), 0);
    }
}
```

Run: `CARGO_TARGET_DIR=<scratchpad>/target cargo test -p cmdstudio-core --lib process_stamp 2>&1 | tail -5`
Expected: `test result: ok. 1 passed`. If the missing-pid assertion fails, print the error code (`last_error`'s text) and record it: F3.

- [ ] **Step 4: The VM script for S2 (parse-checked only)**

`spikes/vm/guest/recovery-renderer.ps1`:

```powershell
# Runs inside a test VM on the signed-in user's desktop (plan 3b-2 Task 1 S2 and Task 12 step 6):
# kills the renderer process of CMD Studio's window, so that WebView2 raises ProcessFailed
# (RENDER_PROCESS_EXITED), and reports whether the window's page came back by itself. -Times
# kills it that many times, -Gap seconds apart (3 within a minute make the app give up and show
# the Yes/No box; answer it by hand or with Send-Key). The renderer is the msedgewebview2.exe
# with --type=renderer whose parent chain reaches cmdstudio.exe; there is one per window.
param([int] $Times = 1, [int] $Gap = 8)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script kills processes" }
function Get-Renderer {
  $app = @(Get-Process cmdstudio -ErrorAction SilentlyContinue)
  if (-not $app) { throw 'cmdstudio.exe is not running' }
  $byId = @{}
  Get-CimInstance Win32_Process | ForEach-Object { $byId[$_.ProcessId] = $_ }
  foreach ($p in Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'") {
    if ($p.CommandLine -notmatch '--type=renderer') { continue }
    $q = $p
    for ($i = 0; $i -lt 6 -and $q; $i++) {
      if ($q.Name -eq 'cmdstudio.exe') { return $p }
      $q = $byId[$q.ParentProcessId]
    }
  }
  throw 'no renderer of cmdstudio.exe found'
}
for ($n = 1; $n -le $Times; $n++) {
  $r = Get-Renderer
  "kill $n`: renderer pid $($r.ProcessId)"
  Stop-Process -Id $r.ProcessId -Force
  Start-Sleep -Seconds 5
  try { $again = Get-Renderer; "after $n`: renderer pid $($again.ProcessId) (page came back: $($again.ProcessId -ne $r.ProcessId))" }
  catch { "after $n`: no renderer ($($_.Exception.Message))" }
  if ($n -lt $Times) { Start-Sleep -Seconds $Gap }
}
```

Add to `guard.test.ps1`'s `$changers`, after `'recovery-events.ps1' = @()`:

```powershell
  'recovery-renderer.ps1' = @()
```

Parse-check both (this PC, never run):

```powershell
foreach ($s in 'recovery-renderer.ps1', 'guard.test.ps1') {
  $t = $e = $null
  $ast = [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path "spikes\vm\guest\$s"), [ref]$t, [ref]$e)
  $first = ($ast.EndBlock.Statements | Select-Object -First 3 | ForEach-Object { $_.Extent.Text }) -join "`n"
  "$s errors=$($e.Count) guarded=$($first -match 'COMPUTERNAME -notlike ''CMDSTUDIO-\*''\) \{ throw')"
}
```

Expected: `recovery-renderer.ps1 errors=0 guarded=True`, `guard.test.ps1 errors=0 guarded=False`.

- [ ] **Step 5: Write `docs/spikes/phase3b-recovery/results-3b2.md`**

```markdown
# Phase 3b-2 spike — WebView2 ProcessFailed through Tauri 2.12.4, and the process stamp (recovery spec §0, §6.4)

- **Date / commit:** <date> / <sha>.
- **S1 and S3 ran on the dev PC in a scratch worktree** (`cargo check` and two unit tests; the app was never started). **S2 runs on VM-A** against CI's artifact of this branch once Task 6 is in (the controller; `spikes/vm/guest/recovery-renderer.ps1`).

| # | What | Plan 3b-2 assumes | Result | Holds? | Fallback taken |
|---|---|---|---|---|---|
| S1 | `with_webview` → `controller().CoreWebView2()` → `add_ProcessFailed(ProcessFailedEventHandler)`, `Reload()` from the handler, with webview2-com 0.39.1 and windows-core 0.62.2 as direct dependencies | compiles | <`cargo check` output's last line; the test's result> | <✅/❌> | <— / F1> |
| S2 | `Stop-Process` of the renderer fires the handler and `Reload()` brings the page back | the page is back within 5 s; `app_status` asked; Home drawn | <recovery-renderer.ps1's lines; the app's log lines> | <✅/❌> | <— / F2 / F1> |
| S3 | `GetProcessTimes` of this process; `OpenProcess` of a pid that is not there | a time within 60 s; `ERROR_INVALID_PARAMETER` → `Ok(None)` | <the test's result> | <✅/❌> | <— / F3> |
```

Fill S1 and S3 from Steps 2–3; leave S2's cells as `pending (VM-A, after Task 6)` for the controller.

- [ ] **Step 6: Clean the scratch, commit**

`git -C <scratchpad>/wt checkout -- . && git -C <scratchpad>/wt clean -fd` (the worktree stays for the later tasks' scratch checks). In `<repo>`:

```bash
git add -- docs/spikes/phase3b-recovery/results-3b2.md spikes/vm/guest/recovery-renderer.ps1 spikes/vm/guest/guard.test.ps1
git diff --cached --name-only
```

Commit message (Write to `.git/CMDSTUDIO_COMMIT_MSG`, then `git commit -F` in a separate call): `spikes: phase 3b-2 spike of WebView2 ProcessFailed under Tauri 2.12.4 and the process stamp (S1, S3 here; S2 on VM-A)`.

---

### Task 2: Carry 1: `init.lua` drops the tasklist liveness check (the founder, 2026-10-19)

The fourth window pauses even while the three hung windows are still open, as recovery spec §1.1 says; the 0.6-second `tasklist` at the pause decision goes with it. `FORGET_AFTER` 600 and `STALE_AFTER` 60 stay.

**Files:**
- Modify: `crates/core/src/generate/lua.rs` (the module doc at :56-67, `HEALTH_BODY` at :149-210, the test at :810)
- Modify: `crates/core/tests/health_lua.rs` (the harness's `tasklists` and `__tasklist`, the tests at :937-1050)
- Modify: `crates/core/tests/golden/init.lua` (regenerated)
- Modify: `crates/core/src/recovery.rs` (module doc: no tasklist sentence; it has none at :1-13, check)
- Modify: `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (appendix B's tasklist bullet)

**Interfaces:**
- Consumes: `init_lua(omp_exe, omp_config, health)` (`lua.rs:596`), the test harness of `health_lua.rs` (`open`, `open_with`, `window`, `tags_of_age`, `Disk`, `PAUSED_NOW`).
- Produces: a `HEALTH_BODY` with no `running_cmds`, no `io.popen` of its own; `health_lua.rs` with 33 − 4 + 1 = 30 tests.

- [ ] **Step 1: Write the failing test**

In `crates/core/tests/health_lua.rs`, replace the four tests `a_stale_tag_of_a_cmd_window_still_running_is_neither_counted_nor_deleted`, `a_tasklist_that_cannot_be_read_counts_every_stale_tag_as_before`, `tasklist_runs_once_and_only_when_the_theme_would_pause` and `tasklist_is_run_by_its_full_system32_path_and_never_by_its_bare_name` (`:937-1050`) with this one:

```rust
#[test]
fn a_stale_tag_of_a_live_window_counts_and_nothing_runs_tasklist() -> mlua::Result<()> {
    // The founder's decision of 2026-10-19 (3b-1's rulings file): a window that hangs is a start
    // that failed, open or closed. Three stale tags pause the fourth window while the three
    // hung windows are still there, and init.lua never starts tasklist (0.6 s on the VM).
    let mut disk = tags_of_age(&[1, 2, 3], 600);
    let w = open_with(&mut disk, window(4, T), |lua| {
        lua.globals().set(
            "__tasklist",
            tasklist(&[("cmd.exe", 1), ("cmd.exe", 2), ("cmd.exe", 3), ("cmd.exe", 4)]),
        )
    })?;
    assert!(w.paused);
    assert_eq!(w.lines(), [PAUSED_NOW.replace("%d", "3")]);
    assert!(w.tasklists.is_empty(), "{:?}", w.tasklists);
    assert_eq!(disk, Disk::from([(paused_file(), pause_record(T, 3))]));
    // Two stale tags and the fourth window's own: no pause, no tasklist, as before.
    let mut disk = tags_of_age(&[1, 2], 600);
    let w = open(&mut disk, window(5, T).prompted())?;
    assert!(!w.paused && w.tasklists.is_empty());
    Ok(())
}
```

Keep the harness's `__tasklist` stub, `usual_tasklist` and the `tasklists` field: they now prove that nothing runs tasklist. Delete `tasklist_raises` handling if it is referenced only by the deleted tests (`__tasklist_raises` in the STUBS Lua at `:142`: leave the stub line; it is harmless).

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p cmdstudio-core --test health_lua a_stale_tag_of_a_live_window -- --nocapture`
Expected: FAIL, `assertion failed: w.paused` or the `tasklists` assertion (`["\"C:\\Windows\\System32\\tasklist.exe\" /FO CSV /NH 2>nul"]`).

- [ ] **Step 3: Remove the check from `HEALTH_BODY`**

In `crates/core/src/generate/lua.rs`, inside `HEALTH_BODY`:

1. Delete the whole `running_cmds` function and its comment (from the line `-- The pids of the cmd windows still running, from one tasklist, asked only when the theme would` through the `end` that closes `local function running_cmds()`), and delete `local function pid_of(path)` … `end` with it (nothing else uses it).
2. In `count_start`, replace

```lua
  -- The forgotten tags go before the pause is decided, so tasklist never runs for them.
  local old, forgotten = stale_split(now)
  for _, path in ipairs(forgotten) do os.remove(path) end
  local live = {}
  if #old >= PAUSE_AFTER then
    local asked, running = pcall(running_cmds)
    if asked and running then
      live = running
      local dead = {}
      for _, path in ipairs(old) do
        if not live[pid_of(path)] then dead[#dead + 1] = path end
      end
      old = dead
    end
  end
  if #old >= PAUSE_AFTER then
```

with

```lua
  -- The forgotten tags go first, so they are never counted.
  local old, forgotten = stale_split(now)
  for _, path in ipairs(forgotten) do os.remove(path) end
  if #old >= PAUSE_AFTER then
```

and, inside that block, replace

```lua
      for _, path in ipairs(tags()) do
        if not live[pid_of(path)] then os.remove(path) end
      end
```

with

```lua
      for _, path in ipairs(tags()) do os.remove(path) end
```

3. In `HEALTH_BODY`'s opening comment (`-- The start counter (recovery spec §3): …`), delete any sentence that mentions tasklist or a window "still running" (read the comment; there is one about the pause decision).
4. In the module doc (`lua.rs:56-67`), replace the bullet that starts `/// - Before it pauses the theme, and only then, the counter lists the running programs once` (through `/// before PATH, so a bare \`tasklist\` would run a \`tasklist.bat\` that sits there.`) with:

```rust
/// - A hung window counts, open or closed (the founder, 2026-10-19): three stale tags pause the
///   next window while the three hung ones are still there. Nothing lists the running
///   programs; the tasklist liveness check of 3b-1 is gone, and so is its 0.6 s at the pause
///   decision. A `cmd /k <a long command>` window writes no tag at all: Clink loads its scripts
///   only when cmd first reads a line (the VM check of 2026-10-18, finding F2).
```

5. In the test at `lua.rs:810` (`// Oh My Posh's popen, not the counter's own tasklist one, which is defined above it.`): read the assertion it guards and change it to assert that `HEALTH_BODY` holds no `io.popen` at all: `assert!(!HEALTH_BODY.contains("io.popen"), "the counter runs no program");` with the comment `// The counter runs no program: Oh My Posh's popen is INIT_BODY's.`

- [ ] **Step 4: Run the Lua tests, regenerate the golden**

Run: `cargo test -p cmdstudio-core --test health_lua`
Expected: `test result: ok. 30 passed`.

Run: `UPDATE_GOLDEN=1 cargo test -p cmdstudio-core --test golden` (PowerShell: `$env:UPDATE_GOLDEN='1'; cargo test -p cmdstudio-core --test golden; Remove-Item Env:UPDATE_GOLDEN`), then `cargo test -p cmdstudio-core --test golden` again without the variable.
Expected: both `test result: ok. 21 passed`; `git diff --stat crates/core/tests/golden/` shows only `init.lua` changed, and `git diff crates/core/tests/golden/init.lua` removes the `running_cmds` block and the `live` lines and adds nothing else.

Run: `cargo test -p cmdstudio-core --test english_in_cmd` → `ok. 2 passed`; `cargo test -p cmdstudio-core --lib generate::lua` → all pass.

- [ ] **Step 5: The main spec's appendix B**

In `docs/superpowers/specs/2026-10-10-cmd-studio-design.md`, the last row of appendix B (phase 3b-1's), find the bullet that begins with the tasklist (search `tasklist`; 3b-1's Task 11 R4 added "ก่อนหยุดธีม init.lua รัน tasklist.exe หนึ่งครั้ง …") and replace that bullet with one Thai sentence:

```markdown
- ตัวนับไม่ตรวจว่าหน้าต่างยังเปิดอยู่หรือไม่ (founder 2026-10-19, phase 3b-2): ป้ายค้าง 3 ใบหยุดธีมให้หน้าต่างที่ 4 แม้หน้าต่างที่ค้างทั้งสามยังเปิดอยู่ ตามเกณฑ์ข้อ 1 ของ recovery spec · การเรียก `tasklist.exe` ที่จุดตัดสินใจหยุดธีมของ 3b-1 (0.6 วินาทีบน VM) ถูกเอาออก · กฎ 10 นาที (`FORGET_AFTER = 600`) และ `STALE_AFTER = 60` คงเดิม
```

Also search the same row for the sentence from the final fix pass about the 10-minute rule and tasklist ("… before tasklist …" in Thai, "ก่อน tasklist"): drop the words that mention tasklist, keep the 10-minute sentence.

- [ ] **Step 6: Checks and commit**

`cargo fmt --all`; the four clippy commands; `cargo test -p cmdstudio-core` (parity runs as allowed).

```bash
git add -- crates/core/src/generate/lua.rs crates/core/tests/health_lua.rs crates/core/tests/golden/init.lua docs/superpowers/specs/2026-10-10-cmd-studio-design.md
```

Message: `core: init.lua counts a hung window open or closed; the tasklist liveness check is gone (founder 2026-10-19)`.

---

### Task 3: The core and the paths: the process stamp, the new files, `--cleanup`, and `E_TIMEOUT`

**Files:**
- Modify: `crates/core/src/adapters/mod.rs` (the `Host` trait at :298-315)
- Modify: `crates/core/src/adapters/windows/host.rs` (`impl Host for WinHost` at :549; Task 1 Step 3's functions)
- Modify: `crates/core/src/adapters/fake.rs` (`FakeHost` at :638-712)
- Modify: `crates/core/src/exec.rs` (the test-only `NoKnownFolders` at :3669)
- Modify: `crates/core/src/recovery.rs` (`impl Paths` at :40-49)
- Modify: `crates/cli/src/lib.rs` (`cmd_studios_own_file` at :1018; the test at :2467)
- Modify: `app/src-tauri/src/error.rs` (`MEANINGS` at :34-176)
- Modify: `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§5.9, one row)

**Interfaces:**
- Consumes: `AdapterError::new(context, message)`, `last_error` (`adapters/windows/mod.rs:33`), `Paths` (`plan.rs:32`), `same_name` and `cmd_studios_own_file` (`crates/cli/src/lib.rs:1018`), `crate::error::meaning`.
- Produces:
  ```rust
  // crates/core/src/adapters/mod.rs
  pub struct ProcessStamp { pub pid: u32, pub created_at_ms: Option<u64> }
  pub trait Host { …
      fn this_process(&self) -> ProcessStamp;
      fn process_created_at(&self, pid: u32) -> Result<Option<u64>, AdapterError>;
  }
  // FakeHost: pub pid: u32 (default 4242), pub created_at_ms: Option<u64> (default Some(1_791_694_000_000)),
  //           pub processes: RefCell<BTreeMap<u32, u64>> (pid → created_at_ms of a process that is alive)
  // crates/core/src/recovery.rs
  impl Paths { pub fn run_file(&self) -> PathBuf /* <data>\run.json */;
               pub fn settings_reset_file(&self, ms: u64) -> PathBuf /* <data>\settings.json.reset-<ms> */ }
  // app/src-tauri/src/error.rs: the row ("E_TIMEOUT", th, en) in MEANINGS
  ```

- [ ] **Step 1: The failing tests**

In `crates/core/src/adapters/fake.rs`, at the end of the file's `#[cfg(test)] mod tests` (read the file's end; add the module if there is none), add:

```rust
#[test]
fn the_fake_host_knows_this_process_and_the_processes_it_is_told_of() {
    let host = FakeHost::default();
    assert_eq!(host.this_process().pid, 4242);
    assert_eq!(host.this_process().created_at_ms, Some(1_791_694_000_000));
    assert_eq!(host.process_created_at(4242).unwrap(), Some(1_791_694_000_000));
    assert_eq!(host.process_created_at(99).unwrap(), None);
    host.processes.borrow_mut().insert(99, 7);
    assert_eq!(host.process_created_at(99).unwrap(), Some(7));
}
```

In `crates/core/src/recovery.rs`'s tests (the module at the end; it has `fn paths()`), add:

```rust
#[test]
fn the_apps_run_record_and_a_settings_file_set_aside_have_their_paths() {
    let p = paths();
    assert_eq!(
        p.run_file(),
        PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\run.json")
    );
    assert_eq!(
        p.settings_reset_file(1_791_694_709_000),
        PathBuf::from(
            r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\settings.json.reset-1791694709000"
        )
    );
}
```

In `crates/cli/src/lib.rs`, in the test `cleanup_with_a_data_dir_takes_the_apps_own_entries_for_cmd_studios` (`:2467`), add to the `for file in [...]` list, after `r"health\paused.json",`:

```rust
            "run.json",
            "settings.json.reset-1791694709000",
            r"logs\crash-1791694709000.txt",
```

In `app/src-tauri/src/error.rs`'s tests, add:

```rust
    #[test]
    fn a_timed_out_command_has_its_row() {
        let (th, en) = meaning("E_TIMEOUT").unwrap();
        assert!(th.contains("60 วินาที"), "{th}");
        assert!(en.starts_with("CMD Studio did not answer within 60 seconds"), "{en}");
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p cmdstudio-core --lib the_fake_host_knows_this_process` → compile error: no method `this_process`.
Run: `cargo test -p cmdstudio-core --lib the_apps_run_record` → compile error: no method `run_file`.
Run: `cargo test -p cmdstudio-cli --lib cleanup_with_a_data_dir_takes` → FAIL: `--cleanup` refuses the folder (`run.json` is not CMD Studio's), exit 2.
Run: `cargo test -p cmdstudio-app --lib a_timed_out_command_has_its_row` → FAIL: `None`.

- [ ] **Step 3: The `Host` trait**

In `crates/core/src/adapters/mod.rs`, before `pub trait Host`:

```rust
/// A process as the run record names it (recovery spec §5.1): its id and when Windows created it,
/// in milliseconds since 1970. Windows gives a pid again once its process is gone, so a pid alone
/// never says "the same process": the creation time does. `None` when it could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessStamp {
    pub pid: u32,
    pub created_at_ms: Option<u64>,
}
```

and inside `pub trait Host`, after `fn system_info(&self) -> SystemInfo;`:

```rust
    /// This process (recovery spec §5.1).
    fn this_process(&self) -> ProcessStamp;
    /// When the process `pid` was created, or `None` when there is no process with that id now.
    /// An id that is there but cannot be opened is an error, never "gone".
    fn process_created_at(&self, pid: u32) -> Result<Option<u64>, AdapterError>;
```

`crates/core/src/exec.rs:3669`'s test-only `NoKnownFolders` delegates every method to a `FakeHost`: add the two there, `self.0.this_process()` and `self.0.process_created_at(pid)` (and `ProcessStamp` to its `use`).

- [ ] **Step 4: The Windows host**

Append Task 1 Step 3's items (`process_created_at`, `filetime_unix_ms`, `current_pid`, `mod process_stamp_tests`) to `crates/core/src/adapters/windows/host.rs` as written there (they compiled and passed in the scratch worktree while this plan was written), and inside `impl Host for WinHost` add:

```rust
    fn this_process(&self) -> ProcessStamp {
        let pid = current_pid();
        ProcessStamp {
            pid,
            created_at_ms: process_created_at(pid).ok().flatten(),
        }
    }
    fn process_created_at(&self, pid: u32) -> Result<Option<u64>, AdapterError> {
        process_created_at(pid)
    }
```

(`ProcessStamp` joins the file's `use crate::adapters::{…}` line.) If Task 1 took F3, make the free function answer `Ok(None)` for every `OpenProcess` failure and drop the `ERROR_INVALID_PARAMETER` branch.

- [ ] **Step 5: The fake host**

In `crates/core/src/adapters/fake.rs`, add to `pub struct FakeHost` (after `pub info: SystemInfo,`):

```rust
    /// This process, as the run record names it (recovery spec §5.1).
    pub pid: u32,
    pub created_at_ms: Option<u64>,
    /// The processes alive now, pid → creation time; this process is in it by its own fields.
    pub processes: RefCell<BTreeMap<u32, u64>>,
```

to `Default::default()`: `pid: 4242, created_at_ms: Some(1_791_694_000_000), processes: RefCell::new(BTreeMap::new()),`; and to `impl Host for FakeHost`:

```rust
    fn this_process(&self) -> ProcessStamp {
        ProcessStamp {
            pid: self.pid,
            created_at_ms: self.created_at_ms,
        }
    }
    fn process_created_at(&self, pid: u32) -> Result<Option<u64>, AdapterError> {
        if pid == self.pid {
            return Ok(self.created_at_ms);
        }
        Ok(self.processes.borrow().get(&pid).copied())
    }
```

- [ ] **Step 6: The paths, `--cleanup`, the row**

`crates/core/src/recovery.rs`, in `impl Paths` after `paused_file`:

```rust
    /// `<data>\run.json`: the app's record of its own run (recovery spec §5.1).
    pub fn run_file(&self) -> PathBuf {
        self.data.join("run.json")
    }
    /// `<data>\settings.json.reset-<ms>`: the settings the Recovery page's "Reset the app's
    /// settings" set aside (recovery spec §5.2).
    pub fn settings_reset_file(&self, ms: u64) -> PathBuf {
        self.data.join(format!("settings.json.reset-{ms}"))
    }
```

Update the module doc's first line: `//! The theme's pause, the start counter's files, and the app's own records (recovery spec §3.1, §4.1, §5.1, §5.2, §7).`

`crates/cli/src/lib.rs`, `cmd_studios_own_file`: change `let own = [paths.state_file(), paths.settings_file(), paths.lock_file()];` to `let own = [paths.state_file(), paths.settings_file(), paths.lock_file(), paths.run_file()];`, and where the function accepts `<name>.damaged-<ms>` (read its body after `:1023`: a `strip_prefix` of the file name plus `.damaged-` and a digits check), accept `settings.json.reset-<digits>` the same way (one more prefix, same digits check). Update its doc comment: `… or a damaged copy of the last two set aside as \`<name>.damaged-<ms>\`, or the settings the Recovery page set aside as \`settings.json.reset-<ms>\` (recovery spec §5.2); and \`run.json\`, the app's record of its run (recovery spec §5.1).` `logs\crash-*.txt` needs nothing: `logs\` is deleted whole.

`app/src-tauri/src/error.rs`, in `MEANINGS` after the `E_CMD_CRASHED` row:

```rust
    (
        "E_TIMEOUT",
        r#"CMD Studio ไม่ตอบภายใน 60 วินาที สำหรับคำสั่งที่ไม่เปลี่ยนเครื่อง ให้ลองใหม่ ถ้ายังเป็นอยู่ให้เริ่ม CMD Studio ใหม่ (recovery spec §6.2)"#,
        r#"CMD Studio did not answer within 60 seconds for a command that changes nothing; try again, and if it stays, restart CMD Studio (recovery spec §6.2)"#,
    ),
```

`docs/superpowers/specs/2026-10-10-cmd-studio-design.md` §5.9, after the `E_CMD_CRASHED` row, the same texts as a table row:

```markdown
| `E_TIMEOUT` | CMD Studio ไม่ตอบภายใน 60 วินาที สำหรับคำสั่งที่ไม่เปลี่ยนเครื่อง ให้ลองใหม่ ถ้ายังเป็นอยู่ให้เริ่ม CMD Studio ใหม่ (recovery spec §6.2) | CMD Studio did not answer within 60 seconds for a command that changes nothing; try again, and if it stays, restart CMD Studio (recovery spec §6.2) |
```

The two must be byte-equal: `crates/core/tests/error_codes.rs` and `error.rs`'s test compare them.

- [ ] **Step 7: Run everything**

Run: `cargo test -p cmdstudio-core --lib` → `ok. 777 passed` (774 + the fake's, the paths', the stamp's). `cargo test -p cmdstudio-core --test error_codes` → `ok. 3 passed`. `cargo test -p cmdstudio-cli --lib` → `ok. 55 passed`. `cargo test -p cmdstudio-app --lib` → `ok. 228 passed`.

- [ ] **Step 8: Checks and commit**

`cargo fmt --all`; the four clippy commands.

```bash
git add -- crates/core/src/adapters/mod.rs crates/core/src/adapters/windows/host.rs crates/core/src/adapters/fake.rs crates/core/src/exec.rs crates/core/src/recovery.rs crates/cli/src/lib.rs app/src-tauri/src/error.rs docs/superpowers/specs/2026-10-10-cmd-studio-design.md
```

Message: `core, cli, app: the Host knows a process's creation time, Paths the run record and the settings set aside, --cleanup both, and E_TIMEOUT has its §5.9 row`.

---

### Task 4: `run.json`, safe mode, and the `--recovery` entry

The app's own record of its run (recovery spec §5.1), the safe mode it opens in after two failed starts or with `--recovery` (§5.2), what `--recovery` does first (§4.4 steps 1–2), and `app_status`'s `busy` (§6.3). The IPC wrappers come in Task 7; here it is the backend and `lib.rs`/`main.rs`.

**Files:**
- Create: `app/src-tauri/src/backend/run.rs`
- Modify: `app/src-tauri/src/backend/mod.rs` (`pub mod run;`, the `run: run::RunState` field, `logged`, `testing::fake_host`)
- Modify: `app/src-tauri/src/command_names.rs` (`CHANGES_MACHINE`)
- Modify: `app/src-tauri/src/lib.rs` (`run_gui(GuiStart)`, the record around the window, the exit, single-instance)
- Modify: `app/src-tauri/src/main.rs` (`--recovery`)

**Interfaces:**
- Consumes: `Backend::{paths, sys, now_ms, ctx, note_line, log_tail}` (`backend/mod.rs`), `Backend::pause_machine(reason, count)` (`recovery.rs:412`), `read_pause`, `Paused::level`, `State::load`, `cmdstudio_core::actions::recover_at_start` (`actions.rs:1079`), `start_notes`/`start_unrestored` (`mod.rs:296, 346`), `Host::{this_process, process_created_at}` (Task 3), `Paths::run_file()`.
- Produces:
  ```rust
  // backend/run.rs
  pub const RUN_VERSION: u32 = 1;
  pub const FAILED_STARTS_FOR_SAFE_MODE: u32 = 2;
  pub const UI_ERRORS_PER_RUN: u32 = 20;
  pub struct RunRecord { version, pid, created_at: Option<u64>, started_at, ui_ready_at: Option<u64>, failed_starts: u32, safe_mode: bool }   // serde camelCase
  pub enum GuiStart { Normal, Recovery }
  pub enum SafeModeReason { FailedStarts { count: u32 }, Recovery }   // tagged "kind", TS
  pub struct CrashNotice { at_ms: u64, has_report: bool }            // TS
  pub struct RecoveryEntry { notes: Vec<Note>, unrestored: Vec<Unrestored>, pause: Option<PauseOutcome>, skipped: Option<String> }   // TS
  pub struct AppStatus { busy: Option<String>, safe_mode: bool, safe_mode_reason: Option<SafeModeReason>, crash_notice: Option<CrashNotice>, recovery_entry: Option<RecoveryEntry> }   // TS
  pub struct StartDecision { pub safe_mode: Option<SafeModeReason>, pub crash_notice: Option<CrashNotice> }
  impl<M: Machine> Backend<M> {
      pub fn start_run(&self, how: GuiStart) -> StartDecision;
      pub fn enter_safe_mode(&self, reason: SafeModeReason);
      pub fn recovery_entry(&self) -> RecoveryEntry;
      pub fn app_ready(&self) -> Result<(), AppError>;
      pub fn end_run(&self);
      pub fn app_status(&self) -> Result<AppStatus, AppError>;
      pub fn crash_dismiss(&self);
      pub fn log_ui_error(&self, message: &str, place: &str) -> Result<(), AppError>;
      pub(crate) fn busy_begin(&self, name: &'static str); pub(crate) fn busy_end(&self);
  }
  // command_names.rs
  pub const CHANGES_MACHINE: &[&str] = &[APPLY, UNDO_LAST, SNAPSHOT_RESTORE, REVERT_ALL, THEME_PAUSE];
  // lib.rs
  pub const RECOVERY_ARG: &str = "--recovery";
  pub fn run_gui(how: backend::run::GuiStart);
  ```

- [ ] **Step 1: The failing tests**

Create `app/src-tauri/src/backend/run.rs` with this tests module (the code of Step 3 goes above it):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{apply_terracotta, backend, fake_fs, fake_host};
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::adapters::Fs;
    use cmdstudio_core::recovery::{read_pause, PauseLevel, PauseReason, PausedBy};
    use cmdstudio_core::state::State;

    fn record(b: &Backend<FakeMachine>) -> Option<RunRecord> {
        let bytes = b.sys().fs.read(&b.paths().run_file()).unwrap()?;
        Some(serde_json::from_slice(&bytes).unwrap())
    }

    fn old(ready: Option<u64>, failed: u32) -> RunRecord {
        RunRecord {
            version: 1,
            pid: 9999,
            created_at: Some(5),
            started_at: 1_791_694_000_000,
            ui_ready_at: ready,
            failed_starts: failed,
            safe_mode: false,
        }
    }

    fn put(b: &Backend<FakeMachine>, r: &RunRecord) {
        b.sys()
            .fs
            .write_atomic(&b.paths().run_file(), &serde_json::to_vec(r).unwrap())
            .unwrap();
    }

    /// The first start writes its record before the window, as spec §5.1 has it, and a
    /// normal run marks itself ready and ends with the file gone.
    #[test]
    fn a_start_writes_its_record_and_a_normal_run_ends_without_one() {
        let b = backend();
        let decided = b.start_run(GuiStart::Normal);
        assert_eq!((decided.safe_mode, decided.crash_notice), (None, None));
        let r = record(&b).unwrap();
        assert_eq!((r.version, r.pid, r.created_at), (1, 4242, Some(1_791_694_000_000)));
        assert_eq!((r.ui_ready_at, r.failed_starts, r.safe_mode), (None, 0, false));
        assert!(r.started_at >= 1_791_694_709_000);
        b.app_ready().unwrap();
        assert!(record(&b).unwrap().ui_ready_at.is_some());
        assert!(!b.app_status().unwrap().safe_mode);
        b.end_run();
        assert_eq!(record(&b), None);
        let log = b.log_tail(5);
        assert!(log.contains("run start safe=false failed_starts=0"), "{log}");
        assert!(log.contains("run end"), "{log}");
    }

    /// Spec §5.1: a record whose process is gone and that never reached its UI is a start that
    /// failed; two of them open the third start in safe mode. One that reached its UI is a run
    /// that ended abnormally: the next start says so, and counts no failed start.
    #[test]
    fn two_failed_starts_open_safe_mode_and_an_ended_run_is_a_crash_notice() {
        let b = backend();
        put(&b, &old(None, 0));
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, None);
        assert_eq!(record(&b).unwrap().failed_starts, 1);
        put(&b, &old(None, 1));
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, Some(SafeModeReason::FailedStarts { count: 2 }));
        let r = record(&b).unwrap();
        assert_eq!((r.failed_starts, r.safe_mode), (2, true));
        let s = b.app_status().unwrap();
        assert!(s.safe_mode);
        assert_eq!(s.safe_mode_reason, Some(SafeModeReason::FailedStarts { count: 2 }));
        // Ready in safe mode: the count goes back to 0, so a restart is a normal start.
        b.app_ready().unwrap();
        assert_eq!(record(&b).unwrap().failed_starts, 0);
        // A run that reached its UI and then died.
        put(&b, &old(Some(1_791_694_100_000), 0));
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, None);
        assert_eq!(
            d.crash_notice,
            Some(CrashNotice {
                at_ms: 1_791_694_000_000,
                has_report: false
            })
        );
        assert_eq!(record(&b).unwrap().failed_starts, 0);
        assert_eq!(b.app_status().unwrap().crash_notice, d.crash_notice);
        b.crash_dismiss();
        assert_eq!(b.app_status().unwrap().crash_notice, None);
    }

    /// Review Focus 1: a pid Windows gave again is a run that ended, not one still running.
    /// Only the same pid with the same creation time is "still running" (spec §5.1: then
    /// single-instance handled it, and the record is skipped, not counted).
    #[test]
    fn a_live_pid_with_another_creation_time_is_a_run_that_ended() {
        let b = backend();
        let alive = RunRecord {
            pid: 777,
            created_at: Some(50),
            failed_starts: 1,
            ..old(None, 1)
        };
        // Alive with the same creation time: skipped (no count), this run's record replaces it.
        fake_host(&b).processes.borrow_mut().insert(777, 50);
        put(&b, &alive);
        let d = b.start_run(GuiStart::Normal);
        assert_eq!((d.safe_mode, d.crash_notice), (None, None));
        assert_eq!(record(&b).unwrap().failed_starts, 0);
        assert!(b.log_tail(5).contains("run record=still-running"), "{}", b.log_tail(5));
        // The same pid, another creation time: that process is another program; the run ended.
        fake_host(&b).processes.borrow_mut().insert(777, 51);
        put(&b, &alive);
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, Some(SafeModeReason::FailedStarts { count: 2 }));
        // A record with no creation time is compared on the pid alone (F3).
        let no_time = RunRecord {
            created_at: None,
            ..alive.clone()
        };
        put(&b, &no_time);
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, None, "pid 777 is alive: skipped");
        assert_eq!(record(&b).unwrap().failed_starts, 0);
    }

    /// Review Focus 2: after `app_restart` the old process exits while the new one's record is
    /// there; the exit deletes only a record of this process.
    #[test]
    fn a_normal_exit_deletes_only_this_processs_own_record() {
        let b = backend();
        b.start_run(GuiStart::Normal);
        let theirs = RunRecord {
            pid: 4243,
            created_at: Some(1_791_694_000_500),
            started_at: 1_791_694_709_500,
            ..old(None, 0)
        };
        put(&b, &theirs);
        b.end_run();
        assert_eq!(record(&b), Some(theirs));
    }

    /// Spec §5.1's last line: a record that cannot be read or written never stops the app, and
    /// says so in one log line.
    #[test]
    fn a_record_that_cannot_be_read_or_written_never_stops_the_start() {
        let b = backend();
        let file = b.paths().run_file();
        fake_fs(&b).put(&file, b"{ not json");
        let d = b.start_run(GuiStart::Normal);
        assert_eq!((d.safe_mode, d.crash_notice), (None, None));
        assert_eq!(record(&b).unwrap().failed_starts, 0, "a damaged record counts nothing");
        assert!(b.log_tail(5).contains("run record=damaged"), "{}", b.log_tail(5));
        fake_fs(&b).fail_writes_to(&file);
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, None);
        assert!(b.app_ready().is_ok(), "ready never fails the UI for the record");
        assert!(b.log_tail(10).contains("run record=unwritable"), "{}", b.log_tail(10));
        fake_fs(&b).stop_failing(&file);
        fake_fs(&b).fail_reads_of(&file);
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, None);
        assert!(b.log_tail(10).contains("run record=unreadable"), "{}", b.log_tail(10));
    }

    /// `--recovery` opens safe mode whatever the record says, and a running app told to recover
    /// is in safe mode from then on.
    #[test]
    fn the_recovery_start_is_safe_mode_for_its_own_reason() {
        let b = backend();
        let d = b.start_run(GuiStart::Recovery);
        assert_eq!(d.safe_mode, Some(SafeModeReason::Recovery));
        assert!(record(&b).unwrap().safe_mode);
        assert_eq!(b.app_status().unwrap().safe_mode_reason, Some(SafeModeReason::Recovery));
        let b = backend();
        b.start_run(GuiStart::Normal);
        b.enter_safe_mode(SafeModeReason::Recovery);
        assert_eq!(b.app_status().unwrap().safe_mode_reason, Some(SafeModeReason::Recovery));
        assert!(record(&b).unwrap().safe_mode);
    }

    /// Spec §4.4 steps 1–2: with a theme applied and no pause of this computer, `--recovery`
    /// pauses it (`reason: recovery`) through the usual pause; the status carries the outcome.
    #[test]
    fn recovery_entry_pauses_an_applied_theme_on_this_computer() {
        let b = backend();
        apply_terracotta(&b);
        b.start_run(GuiStart::Recovery);
        let entry = b.recovery_entry();
        assert_eq!(entry.skipped, None);
        let pause = entry.pause.unwrap();
        assert!(pause.paused_now, "{pause:?}");
        let p = read_pause(b.sys().fs, b.paths()).unwrap().unwrap();
        assert_eq!((p.by, p.reason), (PausedBy::App, Some(PauseReason::Recovery)));
        assert_eq!(
            p.level(&State::load(b.sys().fs, &b.paths().state_file())),
            PauseLevel::Computer
        );
        let status = b.app_status().unwrap();
        assert!(status.recovery_entry.unwrap().pause.unwrap().paused_now);
        let log = b.log_tail(10);
        assert!(log.contains("pause reason=recovery count=0 revert=ok saved=true"), "{log}");
    }

    /// Review Focus 5: a computer already paused is not reverted again, and a state.json that
    /// cannot be read is not reverted blind; both are said, and no theme is nothing to pause.
    #[test]
    fn recovery_entry_pauses_nothing_twice_and_says_when_it_cannot_tell() {
        let b = backend();
        b.start_run(GuiStart::Recovery);
        let entry = b.recovery_entry();
        assert_eq!(entry.pause, None);
        assert_eq!(entry.skipped.as_deref(), Some("no theme is applied"));
        // Paused on this computer already (by the user): nothing to do again.
        apply_terracotta(&b);
        b.theme_pause(PauseReason::User).unwrap();
        let points_before = b.snapshots_list().unwrap().points.len();
        let entry = b.recovery_entry();
        assert_eq!(entry.pause, None);
        assert_eq!(
            entry.skipped.as_deref(),
            Some("the theme is already paused on this computer")
        );
        assert_eq!(b.snapshots_list().unwrap().points.len(), points_before, "no second revert");
        // state.json cannot be read: whether a theme is applied is not known; nothing is reverted.
        fake_fs(&b).fail_reads_of(b.paths().state_file());
        let entry = b.recovery_entry();
        assert_eq!(entry.pause, None);
        assert!(
            entry.skipped.as_deref().unwrap().starts_with("state.json could not be read"),
            "{entry:?}"
        );
    }

    /// Spec §4.4 step 1: the entry runs the start every change runs first (`recover_at_start`:
    /// a cut-off change rolled back, restore points that cannot be read set aside), and its
    /// notes say so the §5.5 way. A restore point whose manifest is damaged is the case the fake
    /// can make: the start sets it aside and the note carries `E_SNAPSHOT_READ`.
    #[test]
    fn recovery_entry_runs_the_start_first_and_carries_its_notes() {
        let b = backend();
        apply_terracotta(&b);
        let id = b.snapshots_list().unwrap().points.into_iter().next().unwrap().id;
        let manifest = b.paths().snapshots().join(&id).join("manifest.json");
        assert!(b.sys().fs.exists(&manifest), "{}", manifest.display());
        fake_fs(&b).put(&manifest, b"{ not json");
        let entry = b.recovery_entry();
        assert!(
            entry.notes.iter().any(|n| n.code == "E_SNAPSHOT_READ" && n.details.contains(&id)),
            "{:?}",
            entry.notes
        );
        // The pause came after the start, and went through.
        assert!(entry.pause.unwrap().paused_now);
    }

    /// Spec §6.3: while a command that changes the machine runs, `app_status` names it; after
    /// it, nothing. The names are the one list `CHANGES_MACHINE`.
    #[test]
    fn the_status_names_the_change_that_runs() {
        use crate::command_names::{APPLY, CHANGES_MACHINE, PLAN_APPLY, THEMES_LIST};
        let b = backend();
        assert_eq!(b.app_status().unwrap().busy, None);
        let seen = std::cell::Cell::new(None);
        let _ = b.logged(APPLY, |b| {
            seen.set(b.app_status().unwrap().busy);
            Ok::<(), AppError>(())
        });
        assert_eq!(seen.into_inner().as_deref(), Some("apply"));
        assert_eq!(b.app_status().unwrap().busy, None);
        let _ = b.logged(THEMES_LIST, |b| {
            assert_eq!(b.app_status().unwrap().busy, None, "a read is not busy");
            Ok::<(), AppError>(())
        });
        assert!(!CHANGES_MACHINE.contains(&PLAN_APPLY), "planning changes nothing");
        assert_eq!(CHANGES_MACHINE.len(), 5);
    }

    /// Spec §6.5: the UI's errors are logged, at most twenty per run (Task 5 pins the
    /// redaction; here the cap and the place).
    #[test]
    fn ui_errors_are_logged_twenty_times_per_run() {
        let b = backend();
        for n in 0..25 {
            b.log_ui_error(&format!("TypeError: x is undefined ({n})"), "Home").unwrap();
        }
        let log = b.log_tail(40);
        assert_eq!(log.matches("ui_error place=Home ").count(), 20, "{log}");
        assert!(!log.contains("(24)"), "{log}");
    }
}
```

`crate::backend::testing::fake_host` does not exist yet: add it to `backend/mod.rs`'s `testing` module beside `fake_fs`:

```rust
    /// The fake host under the backend (see [`fake_fs`]).
    pub(crate) fn fake_host(b: &Backend<FakeMachine>) -> &cmdstudio_core::adapters::fake::FakeHost {
        &b.machine.host
    }
```

(`RestorePoints.points[..].id` and `Paths::snapshots()`: read `apply.rs:209-236` and `crates/core/src/state.rs:20` for the exact field and method names.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p cmdstudio-app --lib backend::run::` → compile errors (`RunRecord`, `start_run`, … not found).

- [ ] **Step 3: `backend/run.rs`**

Above the tests module:

```rust
//! The app's own record of its run (recovery spec §5.1), the safe mode it opens in (§5.2), what
//! `--recovery` does before the window (§4.4 steps 1–2), and `app_status` (§6.3, §8). `run.json`
//! is written before the window is made and deleted on a normal exit; a record left behind says
//! how the earlier run ended: never ready (a start that failed; two in a row open safe mode) or
//! ready (a run that ended abnormally: the next start shows the crash notice). Windows gives a
//! pid again, so a record is "still running" only with the same pid and the same creation time.
//! A record that cannot be read or written never stops the app: one log line says so.
use super::recovery::PauseOutcome;
use super::{note, start_notes, start_unrestored, Backend, Machine, Note};
use crate::error::AppError;
use cmdstudio_core::actions::recover_at_start;
use cmdstudio_core::recovery::{read_pause, PauseLevel, PauseReason};
use cmdstudio_core::snapshot::Unrestored;
use cmdstudio_core::state::State;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, PoisonError};
use ts_rs::TS;

pub const RUN_VERSION: u32 = 1;
/// Starts in a row that never reached the UI, from which the next start opens in safe mode.
pub const FAILED_STARTS_FOR_SAFE_MODE: u32 = 2;
/// How many of the UI's own errors (`log_ui_error`) are logged per run (recovery spec §6.5).
pub const UI_ERRORS_PER_RUN: u32 = 20;

/// `<data>\run.json` (recovery spec §5.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub version: u32,
    pub pid: u32,
    /// When Windows created the process (`GetProcessTimes`); `None` when it could not be read.
    pub created_at: Option<u64>,
    pub started_at: u64,
    pub ui_ready_at: Option<u64>,
    pub failed_starts: u32,
    pub safe_mode: bool,
}

/// How the app was started (`main.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuiStart {
    Normal,
    /// `cmdstudio.exe --recovery` (recovery spec §4.4).
    Recovery,
}

/// Why the app is in safe mode (recovery spec §5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum SafeModeReason {
    /// `count` starts in a row never reached the UI.
    FailedStarts { count: u32 },
    Recovery,
}

/// The earlier run ended abnormally (recovery spec §5.3): when, and whether a crash note is
/// there. With a note, `at_ms` is the note's time; without, the time that run started.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CrashNotice {
    #[ts(type = "number")]
    pub at_ms: u64,
    pub has_report: bool,
}

/// What the `--recovery` entry did (recovery spec §4.4 steps 1–2): the start's notes (a change
/// rolled back, a restore point that cannot be read), what a rollback left to fix by hand, the
/// pause it made, or why it made none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecoveryEntry {
    pub notes: Vec<Note>,
    pub unrestored: Vec<Unrestored>,
    pub pause: Option<PauseOutcome>,
    /// Why no pause was made, in the core's English: no theme, already paused on this computer,
    /// state.json unreadable.
    pub skipped: Option<String>,
}

/// `app_status` (recovery spec §8, §6.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppStatus {
    /// The command that changes the machine and runs now (`command_names::CHANGES_MACHINE`).
    pub busy: Option<String>,
    pub safe_mode: bool,
    pub safe_mode_reason: Option<SafeModeReason>,
    pub crash_notice: Option<CrashNotice>,
    pub recovery_entry: Option<RecoveryEntry>,
}

/// What `start_run` decided, for `lib.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartDecision {
    pub safe_mode: Option<SafeModeReason>,
    pub crash_notice: Option<CrashNotice>,
}

/// This run's state, behind one lock (replaced whole; a poisoned lock is used as it is).
pub(crate) struct RunState {
    inner: Mutex<RunInner>,
}

#[derive(Default)]
struct RunInner {
    /// This process's record as last written; `None` before `start_run`.
    mine: Option<RunRecord>,
    safe_mode: Option<SafeModeReason>,
    crash_notice: Option<CrashNotice>,
    recovery_entry: Option<RecoveryEntry>,
    busy: Option<&'static str>,
    ui_errors: u32,
}

impl RunState {
    pub(crate) fn new() -> Self {
        RunState {
            inner: Mutex::new(RunInner::default()),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut RunInner) -> T) -> T {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut inner)
    }
}

/// Whether the earlier record's process is the process alive now under its pid: the same
/// creation time; with no creation time on either side, the pid alone (fallback F3).
fn same_run(old: &RunRecord, alive_created_at: Option<u64>) -> bool {
    match (old.created_at, alive_created_at) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

impl<M: Machine> Backend<M> {
    /// Reads the record the earlier run left, decides safe mode and the crash notice, and
    /// writes this run's record (recovery spec §5.1, §5.2). Before the window; never fails.
    pub fn start_run(&self, how: GuiStart) -> StartDecision {
        let fs = self.sys().fs;
        let file = self.paths().run_file();
        let me = self.sys().host.this_process();
        let now = self.now_ms();
        let mut failed_starts = 0;
        let mut crash_notice = None;
        match fs.read(&file) {
            Err(_) => self.note_line("run record=unreadable"),
            Ok(None) => {}
            Ok(Some(bytes)) => match serde_json::from_slice::<RunRecord>(&bytes) {
                Err(_) => self.note_line("run record=damaged"),
                Ok(old) if old.version != RUN_VERSION => {
                    self.note_line("run record=other-version");
                }
                Ok(old) => {
                    let alive = match self.sys().host.process_created_at(old.pid) {
                        Ok(Some(created)) => same_run(&old, Some(created)),
                        // Gone, or a process this user may not open (not this app's own earlier
                        // run, which runs as this user).
                        Ok(None) | Err(_) => false,
                    };
                    if alive && old.pid != me.pid {
                        self.note_line("run record=still-running");
                    } else if old.ui_ready_at.is_none() {
                        failed_starts = old.failed_starts.saturating_add(1);
                    } else {
                        crash_notice = Some(self.crash_notice_for(&old));
                    }
                }
            },
        }
        let safe_mode = match how {
            GuiStart::Recovery => Some(SafeModeReason::Recovery),
            GuiStart::Normal if failed_starts >= FAILED_STARTS_FOR_SAFE_MODE => {
                Some(SafeModeReason::FailedStarts {
                    count: failed_starts,
                })
            }
            GuiStart::Normal => None,
        };
        let mine = RunRecord {
            version: RUN_VERSION,
            pid: me.pid,
            created_at: me.created_at_ms,
            started_at: now,
            ui_ready_at: None,
            failed_starts,
            safe_mode: safe_mode.is_some(),
        };
        self.write_record(&mine);
        self.note_line(&format!(
            "run start safe={} failed_starts={failed_starts}",
            safe_mode.is_some()
        ));
        self.run.with(|r| {
            r.mine = Some(mine);
            r.safe_mode = safe_mode;
            r.crash_notice = crash_notice;
        });
        StartDecision {
            safe_mode,
            crash_notice,
        }
    }

    /// Safe mode from now on, for a `--recovery` that reached a running app (recovery spec §4.4).
    pub fn enter_safe_mode(&self, reason: SafeModeReason) {
        let mine = self.run.with(|r| {
            r.safe_mode = Some(reason);
            let m = r.mine.as_mut()?;
            m.safe_mode = true;
            Some(m.clone())
        });
        if let Some(mine) = mine {
            self.write_record(&mine);
        }
    }

    /// The notice of a run that reached its UI and ended abnormally: the newest crash note's
    /// time when there is one newer than that run's start, else the time it started.
    fn crash_notice_for(&self, old: &RunRecord) -> CrashNotice {
        match self.latest_crash_note_since(old.started_at) {
            Some(at_ms) => CrashNotice {
                at_ms,
                has_report: true,
            },
            None => CrashNotice {
                at_ms: old.started_at,
                has_report: false,
            },
        }
    }

    /// The time of the newest crash note written at or after `since_ms`. Task 5 fills it in
    /// over `crash::latest_note`; until then there is none.
    fn latest_crash_note_since(&self, since_ms: u64) -> Option<u64> {
        let _ = since_ms;
        None
    }

    fn write_record(&self, record: &RunRecord) {
        let bytes = match serde_json::to_vec_pretty(record) {
            Ok(mut b) => {
                b.push(b'\n');
                b
            }
            Err(_) => return self.note_line("run record=unwritable"),
        };
        if self
            .sys()
            .fs
            .write_atomic(&self.paths().run_file(), &bytes)
            .is_err()
        {
            self.note_line("run record=unwritable");
        }
    }

    /// `--recovery`'s steps 1–2 (recovery spec §4.4): the cut-off change rolled back under the
    /// lock, as every command that changes the machine does first, then the theme paused on
    /// this computer (`reason: recovery`) when one is applied and no pause of this computer is
    /// in place. Run off the window's thread (`lib.rs`); the outcome goes to `app_status`.
    pub fn recovery_entry(&self) -> RecoveryEntry {
        let sys = self.sys();
        let ctx = self.ctx();
        let (mut notes, unrestored) = match recover_at_start(sys, &ctx) {
            Ok(report) => (start_notes(&report), start_unrestored(&report)),
            Err(e) => (vec![note(e.code(), e.to_string())], Vec::new()),
        };
        let state = State::load(sys.fs, &ctx.paths.state_file());
        let level = match read_pause(sys.fs, &ctx.paths) {
            Ok(Some(p)) => Some(p.level(&state)),
            Ok(None) => None,
            Err(e) => {
                notes.push(note(
                    "E_MACHINE_READ",
                    format!("paused.json could not be read: {e}"),
                ));
                None
            }
        };
        let (pause, skipped) = match (&state, level) {
            (Err(e), _) => (None, Some(format!("state.json could not be read: {e}"))),
            (_, Some(PauseLevel::Computer)) => (
                None,
                Some("the theme is already paused on this computer".to_string()),
            ),
            (Ok(State { applied: None, .. }), _) => {
                (None, Some("no theme is applied".to_string()))
            }
            (Ok(_), _) => (Some(self.pause_machine(PauseReason::Recovery, 0)), None),
        };
        let entry = RecoveryEntry {
            notes,
            unrestored,
            pause,
            skipped,
        };
        self.run.with(|r| r.recovery_entry = Some(entry.clone()));
        entry
    }

    /// The UI drew its first page (recovery spec §5.1): `uiReadyAt` is noted and the count of
    /// failed starts goes back to 0. A record that cannot be written is logged, never an error
    /// to the UI.
    pub fn app_ready(&self) -> Result<(), AppError> {
        let now = self.now_ms();
        let mine = self.run.with(|r| {
            let m = r.mine.as_mut()?;
            m.ui_ready_at = Some(now);
            m.failed_starts = 0;
            Some(m.clone())
        });
        if let Some(mine) = mine {
            self.write_record(&mine);
        }
        Ok(())
    }

    /// A normal exit (the window closed, Tauri's exit event): this process's record goes. One
    /// another process wrote since (`app_restart`: the new process started before this one
    /// ended) stays as it is.
    pub fn end_run(&self) {
        let fs = self.sys().fs;
        let file = self.paths().run_file();
        let Some(mine) = self.run.with(|r| r.mine.clone()) else {
            return;
        };
        let theirs = match fs.read(&file) {
            Ok(Some(bytes)) => serde_json::from_slice::<RunRecord>(&bytes).ok(),
            _ => None,
        };
        let is_mine = theirs
            .as_ref()
            .is_none_or(|t| t.pid == mine.pid && t.created_at == mine.created_at);
        if is_mine {
            let _ = fs.remove_file(&file);
        }
        self.note_line("run end");
    }

    pub fn app_status(&self) -> Result<AppStatus, AppError> {
        Ok(self.run.with(|r| AppStatus {
            busy: r.busy.map(str::to_string),
            safe_mode: r.safe_mode.is_some(),
            safe_mode_reason: r.safe_mode,
            crash_notice: r.crash_notice,
            recovery_entry: r.recovery_entry.clone(),
        }))
    }

    /// "Close" on the crash notice (recovery spec §5.3): noted for this run.
    pub fn crash_dismiss(&self) {
        self.run.with(|r| r.crash_notice = None);
    }

    /// The UI's own error (recovery spec §6.5): one log line, at most [`UI_ERRORS_PER_RUN`] per
    /// run; `place` is letters and digits only, 40 at most. The text goes through the
    /// diagnostics' redaction (Task 5); until then as given, with line breaks folded.
    pub fn log_ui_error(&self, message: &str, place: &str) -> Result<(), AppError> {
        let allowed = self.run.with(|r| {
            if r.ui_errors >= UI_ERRORS_PER_RUN {
                return false;
            }
            r.ui_errors += 1;
            true
        });
        if !allowed {
            return Ok(());
        }
        let text = self.redact_for_log(message);
        let place: String = place.chars().filter(|c| c.is_alphanumeric()).take(40).collect();
        self.note_line(&format!("ui_error place={place} {text}"));
        Ok(())
    }

    /// Task 5 replaces this with `diagnostics::redact` and `redact_sids` over the user's name
    /// and SID; until then the text as it is.
    fn redact_for_log(&self, text: &str) -> String {
        text.replace(['\r', '\n'], " ")
    }

    pub(crate) fn busy_begin(&self, name: &'static str) {
        self.run.with(|r| r.busy = Some(name));
    }

    pub(crate) fn busy_end(&self) {
        self.run.with(|r| r.busy = None);
    }
}
```

`Option::is_none_or` is stable since Rust 1.82: fine on 1.99.0.

In `backend/mod.rs`: `pub mod run;`; the field `/// This run: the record, safe mode, the crash notice, what runs now (recovery spec §5).\n    run: run::RunState,` with `run: run::RunState::new(),` in `new`; and in `logged`, around `let result = f(self);`:

```rust
        let changes = crate::command_names::CHANGES_MACHINE.contains(&name);
        if changes {
            self.busy_begin(name);
        }
        let result = f(self);
        if changes {
            self.busy_end();
        }
```

In `command_names.rs`, after `COMMANDS`:

```rust
/// The commands that change the machine (recovery spec §6.3): each runs one of the core's
/// journaled transactions, and `app_status` names the one that runs. `plan_apply` takes the
/// lock and changes nothing; `theme_resume` at the cmd level writes only `health\`;
/// `doctor_fix` proposes.
pub const CHANGES_MACHINE: &[&str] = &[APPLY, UNDO_LAST, SNAPSHOT_RESTORE, REVERT_ALL, THEME_PAUSE];
```

(`build.rs` `include!`s this file: read `app/src-tauri/build.rs` and check it iterates `COMMANDS` only; a second constant is harmless.)

- [ ] **Step 4: Run the backend tests**

Run: `cargo test -p cmdstudio-app --lib backend::run::`
Expected: `ok. 11 passed`. `cargo test -p cmdstudio-app --lib` → `ok. 239 passed` (228 + 11).

- [ ] **Step 5: `main.rs` and `lib.rs`**

`app/src-tauri/src/main.rs`, replace the `#[cfg(windows)] fn main()`:

```rust
#[cfg(windows)]
fn main() {
    use cmdstudio_app::backend::run::GuiStart;
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    // `--recovery` alone opens the window in recovery mode (recovery spec §4.4, §8): the
    // Start-menu shortcut "CMD Studio Recovery". Every other argument is the CLI's.
    match args.as_slice() {
        [] => return cmdstudio_app::run_gui(GuiStart::Normal),
        [one] if one == cmdstudio_app::RECOVERY_ARG => {
            return cmdstudio_app::run_gui(GuiStart::Recovery)
        }
        _ => {}
    }
    cmdstudio_app::console::attach_parent();
    let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
    let code = cmdstudio_cli::run(
        &args,
        &mut cmdstudio_cli::Io {
            out: &mut out,
            err: &mut err,
        },
    );
    std::process::exit(code);
}
```

and its module doc's first sentence becomes: `Started with no arguments it opens the window; with \`--recovery\` alone, the window in recovery mode (recovery spec §4.4); with any other, it runs \`cmdstudio_cli::run\` and exits with its code, …`. The non-Windows `main` stays.

`app/src-tauri/src/lib.rs`: after the `mod` lines add

```rust
/// The one argument that opens the window instead of the CLI (recovery spec §4.4, §8).
pub const RECOVERY_ARG: &str = "--recovery";
```

and replace `run_gui` and add the two helpers:

```rust
#[cfg(windows)]
pub fn run_gui(how: backend::run::GuiStart) {
    use backend::run::GuiStart;
    use tauri::Manager;
    let backend = match machine::real_backend() {
        Ok(backend) => std::sync::Arc::new(backend),
        Err(why) => {
            console::fatal(&start_failed(&why));
            std::process::exit(1);
        }
    };
    // The run's record, before the window (recovery spec §5.1); `--recovery`'s own steps run on
    // a thread once the window is up, so a slow rollback never delays the window.
    let decided = backend.start_run(how);
    let for_window = backend.clone();
    let for_instance = backend.clone();
    let for_exit = backend.clone();
    let built = tauri::Builder::default()
        // A second start focuses this window instead of opening another (spec §5.7); one with
        // `--recovery` makes this instance do the shortcut's work and reload into recovery mode
        // (recovery spec §4.4). Registered first, as the plugin asks.
        .plugin(tauri_plugin_single_instance::init(move |app, args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
                if args.iter().any(|a| a == RECOVERY_ARG) {
                    recover_in_place(&for_instance, window);
                }
            }
        }))
        .manage(backend)
        .invoke_handler(ipc::handler())
        .setup(move |app| {
            // Said and ended here, not returned: Tauri would turn the error into a panic.
            if let Err(why) = open_main_window(app, for_window.paths().webview()) {
                console::fatal(&window_failed(&why));
                std::process::exit(1);
            }
            if how == GuiStart::Recovery {
                start_recovery_entry(&for_window);
            } else if decided.safe_mode.is_none() {
                // Recovery mode starts no crash check (recovery spec §4.3).
                start_crash_check(&for_window);
            }
            Ok(())
        })
        .build(tauri::generate_context!());
    match built {
        Ok(app) => app.run(move |_, event| {
            if let tauri::RunEvent::Exit = event {
                for_exit.end_run();
            }
        }),
        Err(e) => {
            console::fatal(&start_failed(&e.to_string()));
            std::process::exit(1);
        }
    }
}

/// `--recovery`'s steps 1–2 on a thread of their own, once the window is up (recovery spec
/// §4.4): the window shows the Recovery page meanwhile, and `app_status` carries the outcome.
#[cfg(windows)]
fn start_recovery_entry(backend: &std::sync::Arc<backend::Backend<machine::RealMachine>>) {
    let b = backend.clone();
    let spawned = std::thread::Builder::new()
        .name("recovery-entry".to_string())
        .spawn(move || {
            let _ = b.recovery_entry();
        });
    if spawned.is_err() {
        backend.note_line("recovery_entry not_run");
    }
}

/// A second start with `--recovery` while this one runs (recovery spec §4.4): steps 1–2 on a
/// thread, then the page is loaded again; the reloaded UI asks `app_status` first and finds
/// safe mode on (§6.3). Nothing waits on the window's thread.
#[cfg(windows)]
fn recover_in_place(
    backend: &std::sync::Arc<backend::Backend<machine::RealMachine>>,
    window: tauri::WebviewWindow,
) {
    backend.enter_safe_mode(backend::run::SafeModeReason::Recovery);
    let b = backend.clone();
    let spawned = std::thread::Builder::new()
        .name("recovery-entry".to_string())
        .spawn(move || {
            let _ = b.recovery_entry();
            let _ = window.eval("window.location.reload()");
        });
    if spawned.is_err() {
        backend.note_line("recovery_entry not_run");
    }
}
```

`note_line` is `pub(crate)`: fine inside the crate. The non-Windows `run_gui` takes the same parameter: `pub fn run_gui(_how: backend::run::GuiStart)`.

Update `lib.rs`'s source-text test `the_check_is_marked_before_its_thread_starts_and_started_once_after_the_window`: add

```rust
        let record = text.find("backend.start_run(how)").unwrap();
        assert!(record < window, "the run's record comes before the window");
        assert!(text.contains("tauri::RunEvent::Exit"), "a normal exit ends the run");
        assert!(production().contains("for_exit.end_run()"));
```

and this test for `main.rs`'s rule by text (the binary is never run here):

```rust
    /// `--recovery` alone opens the window in recovery mode; every other argument is the CLI's
    /// (recovery spec §4.4, §8). `main.rs` is read as text: the binary is never started here.
    #[test]
    fn only_the_recovery_argument_opens_the_window() {
        let main = include_str!("main.rs");
        assert_eq!(RECOVERY_ARG, "--recovery");
        assert!(main.contains("[one] if one == cmdstudio_app::RECOVERY_ARG"), "{main}");
        assert!(main.contains("GuiStart::Recovery"));
        let gui = main.find("run_gui(GuiStart::Normal)").unwrap();
        let cli = main.find("cmdstudio_cli::run(").unwrap();
        assert!(gui < cli, "the window's cases come before the CLI");
    }
```

- [ ] **Step 6: Build and test**

Run: `cargo test -p cmdstudio-app --lib` → `ok. 240 passed` (239 + 1). `cargo build -p cmdstudio-app` must finish (the exe is built, never started; a WDAC block on the build is rerun once and then marked in the message).

- [ ] **Step 7: Checks and commit**

`cargo fmt --all`; the four clippy commands.

```bash
git add -- app/src-tauri/src/backend/run.rs app/src-tauri/src/backend/mod.rs app/src-tauri/src/command_names.rs app/src-tauri/src/lib.rs app/src-tauri/src/main.rs
```

Message: `app: run.json records each run, two failed starts open safe mode, --recovery opens the window after rolling back and pausing (recovery spec §4.4, §5.1, §5.2)`.

---

### Task 5: The panic hook and the crash notes, the next start's notice, and the UI's errors in the log

Recovery spec §5.3: a panic in the app or the CLI leaves `<data>\logs\crash-<unix ms>.txt` (version, time, thread, message, location, backtrace; every line through `redact` and `redact_sids`; ten kept; the hook never panics and never fails on a write that fails; the earlier hook runs after it). The next start's notice (`CrashNotice.has_report`), `crash_report_read` for the Report tab, and `log_ui_error`'s redaction.

**Files:**
- Create: `app/src-tauri/src/crash.rs`
- Modify: `app/src-tauri/src/lib.rs` (`pub mod crash;`)
- Modify: `app/src-tauri/src/main.rs` (the hook, both modes)
- Modify: `app/src-tauri/src/machine.rs` (`default_data_dir`, `redaction`)
- Modify: `app/src-tauri/src/backend/run.rs` (`latest_crash_note_since`, `redact_for_log`, `crash_report_read`)
- Modify: `app/src-tauri/src/backend/diagnostics.rs` (the crash note at the end of `diagnostics_copy`)

**Interfaces:**
- Consumes: `diagnostics::{redact, redact_sids, user_name_of}` (`diagnostics.rs:358-500`), `Backend::user_names() -> Result<Vec<String>, AppError>` (`diagnostics.rs:186`, the names `diagnostics_copy` redacts), `Logger`'s `Fs` use (`logs.rs`), `WinFs`, `WinHost::{local_app_data, user_sid}`, `cmdstudio_cli::{DATA_DIR_NAME, now_ms}`, `versions()` (`backend/mod.rs:365`).
- Produces:
  ```rust
  // crash.rs
  pub const CRASH_NOTES_KEPT: usize = 10;
  pub const NOTE_PREFIX: &str = "crash-";
  pub struct Redaction { pub names: Vec<String>, pub sid: Option<String> }   // the names `diagnostics::user_names` redacts, and the SID
  pub struct Report<'a> { pub version: &'a str, pub now_ms: u64, pub thread: &'a str, pub message: &'a str, pub location: Option<&'a str>, pub backtrace: &'a str }
  pub fn redact_all(text: &str, r: &Redaction) -> String;   // `redact` for each name, then `redact_sids`
  pub fn report_text(r: &Report<'_>, redaction: &Redaction) -> String;
  pub fn write_note(fs: &dyn Fs, logs: &Path, now_ms: u64, text: &str) -> Result<PathBuf, AdapterError>;   // and prunes
  pub fn note_time(path: &Path) -> Option<u64>;
  pub fn latest_note(fs: &dyn Fs, logs: &Path) -> Option<(u64, PathBuf)>;
  pub fn install_hook(logs: PathBuf, redaction: Redaction, now: fn() -> u64);
  // backend/run.rs
  pub struct CrashReport { pub file: String, pub at_ms: u64, pub text: String }   // TS
  impl Backend { pub fn crash_report_read(&self) -> Result<Option<CrashReport>, AppError>; }
  // machine.rs
  pub fn default_data_dir() -> Option<PathBuf>;
  pub fn redaction() -> crash::Redaction;
  ```

- [ ] **Step 1: The failing tests**

Create `app/src-tauri/src/crash.rs` with this tests module (the code of Step 3 goes above it):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cmdstudio_core::adapters::fake::FakeFs;
    use cmdstudio_core::adapters::Fs;
    use std::path::PathBuf;

    fn logs() -> PathBuf {
        PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\logs")
    }

    fn redaction() -> Redaction {
        Redaction {
            names: vec!["tester".to_string()],
            sid: Some("S-1-5-21-1-2-3-1001".to_string()),
        }
    }

    fn report<'a>(message: &'a str, backtrace: &'a str) -> Report<'a> {
        Report {
            version: "0.1.0",
            now_ms: 1_791_694_709_000,
            thread: "main",
            message,
            location: Some(r"app\src-tauri\src\backend\apply.rs:120:9"),
            backtrace,
        }
    }

    /// Review Focus 4: the user's name and SID never reach the note, wherever they are: the
    /// message, the location, the backtrace.
    #[test]
    fn the_note_is_redacted_before_it_is_written() {
        let text = report_text(
            &report(
                r"called `Option::unwrap()` on a `None` value at C:\Users\tester\x.json",
                "   0: cmdstudio_app::f\n             at C:\\Users\\tester\\cmd-studio\\app\\src-tauri\\src\\lib.rs:40\n   1: S-1-5-21-1-2-3-1001 owned\n",
            ),
            &redaction(),
        );
        assert!(text.starts_with("CMD Studio 0.1.0 crashed at 2026-10-11T04:58:29Z\n"), "{text}");
        assert!(text.contains("thread: main\n"), "{text}");
        assert!(text.contains("message: called `Option::unwrap()` on a `None` value at C:\\Users\\%USERNAME%\\x.json\n"), "{text}");
        assert!(text.contains("location: app\\src-tauri\\src\\backend\\apply.rs:120:9\n"), "{text}");
        assert!(text.contains("C:\\Users\\%USERNAME%\\cmd-studio\\app"), "{text}");
        assert!(text.contains("<SID> owned"), "{text}");
        assert!(!text.contains("tester") && !text.contains("S-1-5-21"), "{text}");
        assert!(text.ends_with('\n'));
    }

    /// Ten notes are kept, the newest; the file is named by its time, which `note_time` reads
    /// back, and `latest_note` finds the newest by that name.
    #[test]
    fn ten_notes_are_kept_and_the_newest_is_found_by_its_name() {
        let fs = FakeFs::new();
        for n in 0..12_u64 {
            let ms = 1_791_694_709_000 + n * 1000;
            let path = write_note(&fs, &logs(), ms, "x\n").unwrap();
            assert_eq!(path, logs().join(format!("crash-{ms}.txt")));
            assert_eq!(note_time(&path), Some(ms));
        }
        let left: Vec<String> = fs
            .list_dir(&logs())
            .unwrap()
            .into_iter()
            .map(|e| e.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left.len(), CRASH_NOTES_KEPT, "{left:?}");
        assert!(!left.contains(&"crash-1791694709000.txt".to_string()), "{left:?}");
        assert!(!left.contains(&"crash-1791694710000.txt".to_string()), "{left:?}");
        assert_eq!(
            latest_note(&fs, &logs()),
            Some((1_791_694_720_000, logs().join("crash-1791694720000.txt")))
        );
        // Another file of the log folder is neither counted nor touched.
        fs.put(logs().join("cmdstudio.log"), b"log");
        assert_eq!(note_time(&logs().join("cmdstudio.log")), None);
        assert_eq!(note_time(&logs().join("crash-x.txt")), None);
        write_note(&fs, &logs(), 1_791_694_730_000, "y\n").unwrap();
        assert!(fs.exists(&logs().join("cmdstudio.log")));
    }

    /// Review Focus 4: a note that cannot be written is dropped, and the hook's body, run as
    /// the hook runs it, never panics for it (a second panic inside a hook aborts the process).
    #[test]
    fn a_note_that_cannot_be_written_is_dropped_without_a_second_panic() {
        let fs = FakeFs::new();
        fs.fail_writes_to(logs().join("crash-1791694709000.txt"));
        let outcome = std::panic::catch_unwind(|| {
            hook_body(&fs, &logs(), 1_791_694_709_000, &redaction(), &report("boom", ""))
        });
        assert!(outcome.is_ok(), "the hook's body panicked");
        assert_eq!(latest_note(&fs, &logs()), None);
    }

    /// On the real disk, as the hook writes it: a temp folder of its own, removed afterwards
    /// (as `logs.rs`'s on-disk tests do).
    #[cfg(windows)]
    #[test]
    fn on_disk_a_note_is_written_in_utf8_and_read_back() {
        use cmdstudio_core::adapters::windows::fs::WinFs;
        let dir = std::env::temp_dir().join(format!(
            "cmdstudio-crash-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let scratch = Scratch(dir.clone());
        let text = report_text(&report("ไทย and English", "   0: f\n"), &redaction());
        let path = write_note(&WinFs, &scratch.0.join("logs"), 1, &text).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert_eq!(latest_note(&WinFs, &scratch.0.join("logs")).map(|(ms, _)| ms), Some(1));
    }
}
```

In `app/src-tauri/src/backend/run.rs`'s tests, add:

```rust
    /// Spec §5.3: a run that ended abnormally with a crash note newer than its start has a
    /// report, which `crash_report_read` gives redacted, and the diagnostics attach.
    #[test]
    fn a_crash_note_newer_than_the_run_makes_the_notice_a_report() {
        let b = backend();
        let logs = b.paths().logs();
        crate::crash::write_note(b.sys().fs, &logs, 1_791_693_000_000, "older\n").unwrap();
        put(&b, &old(Some(1_791_694_100_000), 0));
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.crash_notice.unwrap().has_report, false, "an older note is not this run's");
        crate::crash::write_note(b.sys().fs, &logs, 1_791_694_200_000, "message: at C:\\Users\\tester\\x\n").unwrap();
        put(&b, &old(Some(1_791_694_100_000), 0));
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(
            d.crash_notice,
            Some(CrashNotice {
                at_ms: 1_791_694_200_000,
                has_report: true
            })
        );
        let report = b.crash_report_read().unwrap().unwrap();
        assert_eq!(report.file, "crash-1791694200000.txt");
        assert_eq!(report.at_ms, 1_791_694_200_000);
        assert!(report.text.contains("%USERNAME%") && !report.text.contains("tester"), "{}", report.text);
        let diagnostics = b.diagnostics_copy().unwrap();
        assert!(diagnostics.contains("== crash report crash-1791694200000.txt =="), "{diagnostics}");
        assert!(diagnostics.contains("%USERNAME%\\x"), "{diagnostics}");
        // No note at all: none to read.
        let b = backend();
        assert_eq!(b.crash_report_read().unwrap(), None);
    }

    /// Spec §6.5: the UI's error text is redacted like the diagnostics.
    #[test]
    fn a_ui_error_is_logged_redacted() {
        let b = backend();
        b.log_ui_error("Error: file:///C:/Users/tester/x.js S-1-5-21-1-2-3-1001", "Themes").unwrap();
        let log = b.log_tail(3);
        assert!(log.contains("ui_error place=Themes Error: file:///C:/Users/%USERNAME%/x.js <SID>"), "{log}");
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p cmdstudio-app --lib crash::` → compile errors. `cargo test -p cmdstudio-app --lib a_crash_note_newer` → compile error (`crash_report_read`).

- [ ] **Step 3: `crash.rs`**

```rust
//! The crash notes (recovery spec §5.3): a panic hook, installed at the start of the app and of
//! the CLI, writes `<data>\logs\crash-<unix ms>.txt` with the version, the time, the thread, the
//! panic's message and location and a backtrace, every line through the diagnostics' redaction
//! (`redact`, `redact_sids`), keeps the ten newest, never panics itself and never fails for a
//! write that fails, and then runs the hook that was there before (Rust's own, which prints to
//! stderr). A native crash (WebView2) runs no hook: only `run.json` tells of it.
use crate::backend::diagnostics::{redact, redact_sids};
use cmdstudio_core::adapters::{AdapterError, Fs};
use cmdstudio_core::utc::iso as iso_utc;
use std::backtrace::Backtrace;
use std::path::{Path, PathBuf};

/// How many crash notes stay in `logs\` (recovery spec §5.3).
pub const CRASH_NOTES_KEPT: usize = 10;
/// A note's name: this, the time in milliseconds since 1970, `.txt`.
pub const NOTE_PREFIX: &str = "crash-";

/// What the diagnostics take out of a text (`diagnostics.rs`): the names `user_names` redacts
/// (the Windows user name, the profile folder's names) and the user's SID.
#[derive(Clone, Debug, Default)]
pub struct Redaction {
    pub names: Vec<String>,
    pub sid: Option<String>,
}

/// `redact` for each name, then `redact_sids`.
pub fn redact_all(text: &str, r: &Redaction) -> String {
    let named = r.names.iter().fold(text.to_string(), |t, name| redact(&t, name));
    redact_sids(&named, r.sid.as_deref())
}

/// One panic, as the hook sees it.
pub struct Report<'a> {
    pub version: &'a str,
    pub now_ms: u64,
    pub thread: &'a str,
    pub message: &'a str,
    pub location: Option<&'a str>,
    pub backtrace: &'a str,
}

/// The note's text, every line redacted.
pub fn report_text(r: &Report<'_>, redaction: &Redaction) -> String {
    let raw = format!(
        "CMD Studio {} crashed at {}\nthread: {}\nmessage: {}\nlocation: {}\nbacktrace:\n{}",
        r.version,
        iso_utc(r.now_ms),
        r.thread,
        r.message,
        r.location.unwrap_or("unknown"),
        r.backtrace
    );
    let mut out = String::with_capacity(raw.len());
    for line in raw.lines() {
        out.push_str(&redact_all(line, redaction));
        out.push('\n');
    }
    out
}

/// The time a note's name carries; `None` for any other file.
pub fn note_time(path: &Path) -> Option<u64> {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix(NOTE_PREFIX))
        .and_then(|n| n.strip_suffix(".txt"))
        .and_then(|ms| ms.parse::<u64>().ok())
}

/// The notes in `logs`, newest first.
fn notes(fs: &dyn Fs, logs: &Path) -> Vec<(u64, PathBuf)> {
    let mut found: Vec<(u64, PathBuf)> = fs
        .list_dir(logs)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| !e.is_dir)
        .filter_map(|e| note_time(&e.path).map(|ms| (ms, e.path)))
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found
}

/// The newest note, by its name.
pub fn latest_note(fs: &dyn Fs, logs: &Path) -> Option<(u64, PathBuf)> {
    notes(fs, logs).into_iter().next()
}

/// Writes `crash-<now_ms>.txt` in `logs` (made when missing) and deletes the notes beyond the
/// ten newest. A note that cannot be deleted is left.
pub fn write_note(
    fs: &dyn Fs,
    logs: &Path,
    now_ms: u64,
    text: &str,
) -> Result<PathBuf, AdapterError> {
    let path = logs.join(format!("{NOTE_PREFIX}{now_ms}.txt"));
    fs.write_atomic(&path, text.as_bytes())?;
    for (_, old) in notes(fs, logs).into_iter().skip(CRASH_NOTES_KEPT) {
        let _ = fs.remove_file(&old);
    }
    Ok(path)
}

/// What the hook does, with the pieces it has: writes the note and drops any failure. Split out
/// so a test runs it under `catch_unwind` with a disk that refuses the write.
pub fn hook_body(fs: &dyn Fs, logs: &Path, now_ms: u64, redaction: &Redaction, r: &Report<'_>) {
    let text = report_text(r, redaction);
    let _ = write_note(fs, logs, now_ms, &text);
}

/// Installs the hook (recovery spec §5.3). The earlier hook runs after the note is written.
/// Everything the hook does is inside `catch_unwind`: a panic in the hook itself would abort
/// the process at once, before the earlier hook could say anything.
#[cfg(windows)]
pub fn install_hook(logs: PathBuf, redaction: Redaction, now: fn() -> u64) {
    use cmdstudio_core::adapters::windows::fs::WinFs;
    let earlier = std::panic::take_hook();
    let version = crate::backend::versions().app;
    std::panic::set_hook(Box::new(move |info| {
        // UNWIND SAFETY: the closure reads `info` and its own captures and writes one file;
        // nothing of the program's state is left half changed when it unwinds.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let message = match info.payload().downcast_ref::<&str>() {
                Some(s) => (*s).to_string(),
                None => match info.payload().downcast_ref::<String>() {
                    Some(s) => s.clone(),
                    None => "(no message)".to_string(),
                },
            };
            let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
            let thread = std::thread::current();
            let backtrace = Backtrace::force_capture().to_string();
            hook_body(
                &WinFs,
                &logs,
                now(),
                &redaction,
                &Report {
                    version: &version,
                    now_ms: now(),
                    thread: thread.name().unwrap_or("unnamed"),
                    message: &message,
                    location: location.as_deref(),
                    backtrace: &backtrace,
                },
            );
        }));
        earlier(info);
    }));
}
```

(`std::panic::set_hook`'s closure is `Fn`: everything captured is read, `logs`, `redaction`, `version` and `earlier` are owned by it. `info.payload()` is `&dyn Any`; `PanicHookInfo::location()` is stable.)

In `lib.rs`: `pub mod crash;`.

- [ ] **Step 4: `machine.rs`, `main.rs`**

`app/src-tauri/src/machine.rs`, after `real_backend`:

```rust
/// `%LOCALAPPDATA%\ai.resincore.cmdstudio`, as `real_backend` works it out; `None` when
/// Windows cannot say where the local app data is.
pub fn default_data_dir() -> Option<PathBuf> {
    WinHost
        .local_app_data()
        .ok()
        .map(|local| local.join(cmdstudio_cli::DATA_DIR_NAME))
}

/// What the crash notes take out (recovery spec §5.3): this user's name, from the local app
/// data path and from `USERNAME`, and the SID. Nothing when neither can be read: the notes then
/// redact nothing, and a name in a path stays (the user reads the note before sending it).
pub fn redaction() -> crate::crash::Redaction {
    let host = WinHost;
    let mut names: Vec<String> = host
        .local_app_data()
        .map(|p| vec![crate::backend::diagnostics::user_name_of(&p)])
        .unwrap_or_default();
    if let Ok(user) = std::env::var("USERNAME") {
        names.push(user);
    }
    names.retain(|n| !n.is_empty());
    crate::crash::Redaction {
        names,
        sid: host.user_sid().ok(),
    }
}
```

`app/src-tauri/src/main.rs`, as the first lines of the Windows `main`, before `let args`:

```rust
    // The crash notes, for the app and the CLI alike (recovery spec §5.3): the default data
    // folder's logs\ (a `--data-dir` run's note goes there too).
    if let Some(data) = cmdstudio_app::machine::default_data_dir() {
        cmdstudio_app::crash::install_hook(
            data.join("logs"),
            cmdstudio_app::machine::redaction(),
            cmdstudio_cli::now_ms,
        );
    }
```

`machine` is `#[cfg(windows)] pub mod machine;`: fine, `main` is `#[cfg(windows)]` too. Add to `lib.rs`'s text test `only_the_recovery_argument_opens_the_window`: `assert!(main.find("crash::install_hook").unwrap() < main.find("let args").unwrap(), "the hook comes first");`.

- [ ] **Step 5: The backend's use of the notes**

In `backend/run.rs`, replace the two stand-ins:

```rust
    /// The time of the newest crash note written at or after `since_ms` (`crash::latest_note`).
    fn latest_crash_note_since(&self, since_ms: u64) -> Option<u64> {
        crate::crash::latest_note(self.sys().fs, &self.paths().logs())
            .map(|(ms, _)| ms)
            .filter(|ms| *ms >= since_ms)
    }

    /// The text as the diagnostics would show it: the user's names and SID taken out.
    fn redact_for_log(&self, text: &str) -> String {
        crate::crash::redact_all(&text.replace(['\r', '\n'], " "), &self.redaction())
    }

    /// The names `diagnostics_copy` redacts (`user_names`, `diagnostics.rs:186`: the Windows
    /// user name and the profile folder's names) and this user's SID; empty when they cannot
    /// be read, so nothing is taken out.
    pub(crate) fn redaction(&self) -> crate::crash::Redaction {
        crate::crash::Redaction {
            names: self.user_names().unwrap_or_default(),
            sid: self.sys().host.user_sid().ok(),
        }
    }
```

(If `user_names` returns a tuple or other field names, fit the two lines to it; its doc says what it holds.) Add:

```rust
/// The newest crash note, for the Report tab (recovery spec §5.3): its file name, its time, and
/// its text (written redacted; redacted once more here, in case the hook had no name to take out).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CrashReport {
    pub file: String,
    #[ts(type = "number")]
    pub at_ms: u64,
    pub text: String,
}

impl<M: Machine> Backend<M> {
    pub fn crash_report_read(&self) -> Result<Option<CrashReport>, AppError> {
        let fs = self.sys().fs;
        let Some((at_ms, path)) = crate::crash::latest_note(fs, &self.paths().logs()) else {
            return Ok(None);
        };
        let bytes = fs
            .read(&path)
            .map_err(|e| AppError::coded("E_MACHINE_READ", e.to_string()))?
            .unwrap_or_default();
        let text = crate::crash::redact_all(&String::from_utf8_lossy(&bytes), &self.redaction());
        Ok(Some(CrashReport {
            file: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            at_ms,
            text,
        }))
    }
}
```

In `backend/diagnostics.rs`, at the end of `diagnostics_copy` (before `Ok(out)`; read the function's tail), append the newest note when the start found an abnormal exit with a report, or a note was written during this run (newer than this run's start):

```rust
        // The newest crash note, when it belongs to this run or the one that ended abnormally
        // (recovery spec §5.3): the user sees it here before copying.
        if let Ok(Some(report)) = self.crash_report_read() {
            let attach = self.run_started_at().is_some_and(|started| report.at_ms >= started)
                || self.app_status().ok().and_then(|s| s.crash_notice).is_some_and(|n| n.has_report);
            if attach {
                out.push_str(&format!("\n== crash report {} ==\n{}", report.file, report.text));
            }
        }
```

with, in `run.rs`'s `impl`: `pub(crate) fn run_started_at(&self) -> Option<u64> { self.run.with(|r| r.mine.as_ref().map(|m| m.started_at)) }`. In the test above, the notice has a report, so the note is attached; add to the same test, after the diagnostics assertion, a backend with a note older than its run and no notice: `assert!(!b2.diagnostics_copy().unwrap().contains("== crash report"))`.

- [ ] **Step 6: Run everything**

Run: `cargo test -p cmdstudio-app --lib` → `ok. 246 passed` (240 + 4 crash + 2 run). The on-disk test writes under the temp folder only.

- [ ] **Step 7: Checks and commit**

`cargo fmt --all`; the four clippy commands; `cargo build -p cmdstudio-app` (never started).

```bash
git add -- app/src-tauri/src/crash.rs app/src-tauri/src/lib.rs app/src-tauri/src/main.rs app/src-tauri/src/machine.rs app/src-tauri/src/backend/run.rs app/src-tauri/src/backend/diagnostics.rs
```

Message: `app: a panic writes a redacted crash note, the next start says the run ended abnormally, and the UI's errors are logged redacted (recovery spec §5.3, §6.5)`.

---

### Task 6: The native Yes/No box, `app_restart`, `settings_reset`, m4, and the renderer watch

Recovery spec §5.4 (a window that cannot open offers Undo everything), §6.3 (`app_restart`), §5.2 ("Reset the app's settings"), 3b-1's m4, and §6.4 if Task 1's S1 passed. Backend methods and `lib.rs`; the IPC wrappers are Task 7's.

**Files:**
- Modify: `app/src-tauri/src/console.rs` (`ask_yes_no`)
- Modify: `app/src-tauri/src/lib.rs` (the Yes/No path, `restart_process`, the renderer watch)
- Modify: `app/src-tauri/src/backend/mod.rs` (`settings_reset`)
- Modify: `app/src-tauri/src/backend/recovery.rs` (`ResumeOutcome::NotKnown`)
- Create (only if S1 passed): `app/src-tauri/src/webview_watch.rs`; Modify: `app/src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: `console::fatal` (`console.rs:84`), `Backend::revert_all` (`apply.rs:821`) and `RevertOutcome { ok, status, error, … }` (`apply.rs:295`), `settings::load`/`SettingsCache` (`mod.rs:48, 175`), `Paths::settings_reset_file` (Task 3), `Paused::level` (`recovery.rs:157`), Task 1's `webview_watch.rs`.
- Produces:
  ```rust
  // console.rs
  pub fn ask_yes_no(message: &str) -> bool;
  // lib.rs
  pub fn window_question(why: &str, logs: &Path) -> String;
  pub fn revert_said(result: &Result<RevertOutcome, AppError>, logs: &Path) -> String;
  pub fn restart_process() -> Result<(), String>;      // spawns cmdstudio.exe with no arguments
  // backend/mod.rs
  impl Backend { pub fn settings_reset(&self) -> Result<SettingsLoad, AppError>; }
  // backend/recovery.rs
  pub enum ResumeOutcome { …, NotKnown }   // state.json could not be read
  ```

- [ ] **Step 1: The failing tests**

In `app/src-tauri/src/lib.rs`'s tests:

```rust
    /// Recovery spec §5.4: the window that cannot open asks, in English, whether to undo
    /// everything, and says what the undo did.
    #[test]
    fn a_window_that_cannot_open_asks_to_undo_and_says_what_the_undo_did() {
        use crate::backend::apply::RevertOutcome;
        let logs = std::path::Path::new(r"C:\Users\x\AppData\Local\ai.resincore.cmdstudio\logs");
        let q = window_question("no WebView2", logs);
        assert_eq!(
            q,
            "CMD Studio could not open its window: no WebView2. Your themes are safe. Undo \
             everything CMD Studio changed on this computer (cmd, Windows Terminal, fonts)? The \
             log is in C:\\Users\\x\\AppData\\Local\\ai.resincore.cmdstudio\\logs."
        );
        let ok = RevertOutcome {
            ok: true,
            steps: vec![],
            notes: vec![],
            status: Some(crate::backend::apply::TxnStatus::Committed),
            error: None,
            unrestored: vec![],
        };
        assert_eq!(
            revert_said(&Ok(ok.clone()), logs),
            "Everything CMD Studio changed on this computer is back as it was."
        );
        let not_ok = RevertOutcome { ok: false, ..ok };
        assert_eq!(
            revert_said(&Ok(not_ok), logs),
            "Not everything could be undone. The log in C:\\Users\\x\\AppData\\Local\\ai.resincore.cmdstudio\\logs says what to finish by hand."
        );
        let busy = Err(crate::error::AppError::coded("E_BUSY", "cmdstudio.lock is held"));
        assert_eq!(
            revert_said(&busy, logs),
            "Nothing was undone: another CMD Studio, or another program, is changing this computer right now (E_BUSY). Try again when it is done."
        );
        let other = Err(crate::error::AppError::coded("E_SNAPSHOT_READ", "manifest.json is not JSON"));
        assert_eq!(
            revert_said(&other, logs),
            "Nothing was undone (E_SNAPSHOT_READ): manifest.json is not JSON"
        );
        for text in [q, revert_said(&busy, logs)] {
            assert!(text.is_ascii(), "{text}");
        }
        // The wiring, by text: Yes runs the backend's Undo everything, No ends as before, and
        // `start_failed` keeps its plain box (no data folder to undo from).
        let text = production();
        assert!(text.contains("console::ask_yes_no(&window_question("), "the window's failure asks");
        assert!(text.contains("for_window.revert_all()"), "Yes undoes everything");
        assert!(text.contains("console::fatal(&revert_said("), "the outcome is said");
        assert_eq!(text.matches("console::fatal(&start_failed(").count(), 2, "start_failed stays a plain box");
    }

    /// Recovery spec §6.3: the restart starts this exe again with no argument; the process is
    /// never started here (the text is checked: the exe is never run on this PC).
    #[test]
    fn the_restart_starts_this_exe_with_no_argument() {
        let body = body_of("pub fn restart_process(");
        assert!(body.contains("std::env::current_exe()"));
        assert!(body.contains("std::process::Command::new(exe)"));
        assert!(!body.contains(".arg("), "no argument: a normal start");
        assert!(body.contains(".spawn()"));
    }
```

(`RevertOutcome`'s fields: read `apply.rs:295` and fit the struct literal; `status` may be `Option<TxnStatus>` or `TxnStatus`.)

In `app/src-tauri/src/console.rs`'s tests nothing: `ask_yes_no` shows a box (never in a test); its flags are pinned by text in `lib.rs`'s test above through `console::ask_yes_no(`; add there `assert!(include_str!("console.rs").contains("MB_YESNO | MB_ICONQUESTION"));` and `assert!(include_str!("console.rs").contains("== IDYES"));`.

In `app/src-tauri/src/backend/mod.rs`'s tests:

```rust
    /// Recovery spec §5.2: "Reset the app's settings" sets settings.json aside as
    /// `settings.json.reset-<ms>`, the defaults stand from then on, and the page is told which
    /// file was set aside. With no file, nothing is set aside.
    #[test]
    fn resetting_the_settings_sets_the_file_aside_and_the_defaults_stand() {
        let b = backend();
        let mine = AppSettings {
            language: crate::settings::Language::Th,
            wizard_done: true,
            ..AppSettings::default()
        };
        b.settings_set(mine).unwrap();
        let reset = b.settings_reset().unwrap();
        assert_eq!(reset.settings, AppSettings::default());
        let aside = reset.reset_from.unwrap();
        assert!(aside.starts_with("settings.json.reset-"), "{aside}");
        assert!(!b.sys().fs.exists(&b.paths().settings_file()));
        assert!(b.sys().fs.exists(&b.paths().settings_file().with_file_name(&aside)));
        assert_eq!(b.settings_get().unwrap().settings, AppSettings::default());
        assert_eq!(b.settings_or_default(), AppSettings::default());
        let b = backend();
        let reset = b.settings_reset().unwrap();
        assert_eq!((reset.settings, reset.reset_from), (AppSettings::default(), None));
    }
```

In `app/src-tauri/src/backend/recovery.rs`'s tests (m4):

```rust
    /// 3b-1's m4: "Turn the theme back on" with a pause of this computer whose state.json
    /// cannot be read says so, instead of ending in the planner's error; nothing is changed.
    #[test]
    fn turning_back_on_when_state_json_cannot_be_read_says_so() {
        let b = backend();
        apply_terracotta(&b);
        b.theme_pause(PauseReason::User).unwrap();
        fake_fs(&b).fail_reads_of(b.paths().state_file());
        assert_eq!(level(&b), PauseLevel::ComputerUnknown);
        assert_eq!(b.theme_resume().unwrap(), ResumeOutcome::NotKnown);
        assert!(read_pause(b.sys().fs, b.paths()).unwrap().is_some(), "the record stays");
    }
```

(`level(&b)` is the helper the test at `:849` uses.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p cmdstudio-app --lib a_window_that_cannot_open` → compile error (`window_question`). `… resetting_the_settings` → compile error. `… turning_back_on_when_state_json` → compile error (`NotKnown`).

- [ ] **Step 3: `console.rs`**

After `fatal`:

```rust
/// Shows `message` with Yes and No, titled "CMD Studio", and answers whether Yes was pressed. For
/// a window that cannot be opened (recovery spec §5.4); English, like `fatal`.
#[cfg(windows)]
pub fn ask_yes_no(message: &str) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO,
    };
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let (text, title) = (wide(message), wide("CMD Studio"));
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call, the owner
    // window is null (no parent), and the flags are documented constants.
    let pressed = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        )
    };
    pressed == IDYES
}

#[cfg(not(windows))]
pub fn ask_yes_no(message: &str) -> bool {
    eprintln!("CMD Studio: {message}");
    false
}
```

- [ ] **Step 4: `lib.rs`**

Replace, inside `setup`,

```rust
            if let Err(why) = open_main_window(app, for_window.paths().webview()) {
                console::fatal(&window_failed(&why));
                std::process::exit(1);
            }
```

with

```rust
            if let Err(why) = open_main_window(app, for_window.paths().webview()) {
                // Recovery spec §5.4: Yes undoes everything through the backend, under the lock
                // as every change is; No ends as before. Either way exit code 1.
                let logs = for_window.paths().logs();
                if console::ask_yes_no(&window_question(&why, &logs)) {
                    console::fatal(&revert_said(&for_window.revert_all(), &logs));
                }
                std::process::exit(1);
            }
```

and add the helpers beside `window_failed` (keep `window_failed` for its test, or fold it in: `window_question` uses it):

```rust
/// The Yes/No question of a window that could not be built (recovery spec §5.4).
#[cfg(windows)]
pub fn window_question(why: &str, logs: &std::path::Path) -> String {
    format!(
        "{}. Your themes are safe. Undo everything CMD Studio changed on this computer (cmd, \
         Windows Terminal, fonts)? The log is in {}.",
        window_failed(why),
        logs.display()
    )
}

/// What the undo did, for the second box (recovery spec §5.4): done, not everything (the log
/// says what to finish by hand), refused because busy, or refused for another reason.
#[cfg(windows)]
pub fn revert_said(
    result: &Result<backend::apply::RevertOutcome, error::AppError>,
    logs: &std::path::Path,
) -> String {
    match result {
        Ok(out) if out.ok => {
            "Everything CMD Studio changed on this computer is back as it was.".to_string()
        }
        Ok(_) => format!(
            "Not everything could be undone. The log in {} says what to finish by hand.",
            logs.display()
        ),
        Err(e) if e.code == "E_BUSY" => "Nothing was undone: another CMD Studio, or another \
                                         program, is changing this computer right now (E_BUSY). \
                                         Try again when it is done."
            .to_string(),
        Err(e) => format!("Nothing was undone ({}): {}", e.code, e.details),
    }
}

/// Starts this exe again with no argument, a normal start (recovery spec §6.3): the caller
/// exits once it has spawned. The new process reads this one's run record as still running and
/// writes its own, which this one's exit leaves alone (`Backend::end_run`).
#[cfg(windows)]
pub fn restart_process() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 5: `settings_reset` and m4**

`backend/mod.rs`, after `settings_set`:

```rust
    /// "Reset the app's settings" (recovery spec §5.2): settings.json is set aside as
    /// `settings.json.reset-<ms>` and the defaults stand, in memory and on the next read; the
    /// name of the file set aside is told, once, like a damaged file's. Nothing to set aside
    /// when there is no file.
    pub fn settings_reset(&self) -> Result<SettingsLoad, AppError> {
        self.with_settings(|cache| {
            let fs = self.sys().fs;
            let file = self.paths.settings_file();
            let reset_from = if fs.exists(&file) {
                let mut ms = self.now_ms();
                let aside = loop {
                    let candidate = self.paths.settings_reset_file(ms);
                    if !fs.exists(&candidate) {
                        break candidate;
                    }
                    ms = ms.saturating_add(1);
                };
                fs.rename(&file, &aside).map_err(|e| {
                    AppError::coded(
                        "E_DATA_WRITE",
                        format!(
                            "{} could not be set aside as {}: {e}",
                            file.display(),
                            aside.display()
                        ),
                    )
                })?;
                aside.file_name().map(|n| n.to_string_lossy().into_owned())
            } else {
                None
            };
            cache.settings = AppSettings::default();
            cache.reset_from = None;
            Ok(SettingsLoad {
                settings: AppSettings::default(),
                reset_from,
            })
        })
    }
```

`backend/recovery.rs`: add the variant to `ResumeOutcome` with its doc:

```rust
    /// A pause of this computer whose state can't be told (state.json could not be read): fix
    /// state.json first (doctor says what is wrong with it); nothing is changed.
    NotKnown,
```

and in `theme_resume`, before the `(_, Some(r)) if …` arm:

```rust
            (PauseLevel::ComputerUnknown, _) => Ok(ResumeOutcome::NotKnown),
```

- [ ] **Step 6: The renderer watch (only if Task 1's S1 passed; else skip to Step 8 and record F1)**

Create `app/src-tauri/src/webview_watch.rs` with Task 1 Step 2's text as it compiled (its `Failures` test included). `Cargo.toml`: Task 1's two lines under `[target.'cfg(windows)'.dependencies]`. `lib.rs`: `#[cfg(windows)] pub mod webview_watch;`, and in `setup`, right after the `open_main_window` block:

```rust
            // Recovery spec §6.4: the renderer's death reloads the page; the third in a minute
            // asks the Yes/No question instead, on a thread of its own (WebView2's thread must
            // not block), then ends the app.
            if let Some(window) = app.get_webview_window("main") {
                let b = for_window.clone();
                let watched = webview_watch::watch_renderer(&window, move || {
                    let b = b.clone();
                    let _ = std::thread::Builder::new()
                        .name("renderer-gave-up".to_string())
                        .spawn(move || {
                            let logs = b.paths().logs();
                            let why = "its page stopped three times in a minute";
                            if console::ask_yes_no(&window_question(why, &logs)) {
                                console::fatal(&revert_said(&b.revert_all(), &logs));
                            }
                            std::process::exit(1);
                        });
                });
                if let Err(e) = watched {
                    for_window.note_line(&format!("renderer watch=not_registered {e}"));
                }
            }
```

(`e` is a `String` from `with_webview`'s error: it holds no user data.) Add to `lib.rs`'s text test: `assert!(production().contains("webview_watch::watch_renderer(&window,"))`.

- [ ] **Step 7: Run everything**

Run: `cargo test -p cmdstudio-app --lib` → `ok. 250 passed` (246 + 2 lib + 1 settings + 1 m4), `251` with `webview_watch`'s test. `cargo build -p cmdstudio-app` (never started).

- [ ] **Step 8: Checks and commit**

`cargo fmt --all`; the four clippy commands.

```bash
git add -- app/src-tauri/src/console.rs app/src-tauri/src/lib.rs app/src-tauri/src/backend/mod.rs app/src-tauri/src/backend/recovery.rs
# and, if Step 6 ran:
git add -- app/src-tauri/src/webview_watch.rs app/src-tauri/Cargo.toml Cargo.lock
```

(`Cargo.lock` changes only by the two new direct edges; `git diff Cargo.lock` must show no new package.) Message: `app: a window that cannot open offers to undo everything, app_restart, settings_reset, turning back on says when state.json cannot be read, and the renderer's death reloads the page (recovery spec §5.2, §5.4, §6.3, §6.4)` (without the last clause under F1).

---

### Task 7: The seven IPC commands, the capability, the bindings, and the UI's `Ipc` over Tauri and in the mock

Recovery spec §8's new commands as thin wrappers (`app_ready`, `app_status`, `crash_dismiss`, `crash_report_read`, `settings_reset`, `log_ui_error`, `app_restart`), the allow list, the 37 count, the ts-rs bindings, and the UI's two `Ipc` implementations.

**Files:**
- Modify: `app/src-tauri/src/command_names.rs`, `app/src-tauri/src/ipc.rs`, `app/src-tauri/capabilities/default.json`, `app/src-tauri/src/config_guard.rs:3`
- Create (generated): `app/ui/src/bindings/{AppStatus,SafeModeReason,CrashNotice,CrashReport,RecoveryEntry}.ts`; Modify (generated): `app/ui/src/bindings/ResumeOutcome.ts`
- Modify: `app/ui/src/ipc/types.ts`, `app/ui/src/ipc/tauri.ts`, `app/ui/src/ipc/tauri.test.ts`, `app/ui/src/ipc/mock.ts`, `app/ui/src/ipc/mock.test.ts`

**Interfaces:**
- Consumes: Tasks 4–6's backend methods; `blocking`, `Backend::logged` (`ipc.rs:53, mod.rs:138`); `crate::restart_process` (Task 6); `AppHandle::exit`.
- Produces:
  ```ts
  // ipc/types.ts
  interface Ipc { …
    appReady(): Promise<void>;
    appStatus(): Promise<AppStatus>;
    crashDismiss(): Promise<void>;
    crashReportRead(): Promise<CrashReport | null>;
    settingsReset(): Promise<SettingsLoad>;
    logUiError(message: string, place: string): Promise<void>;
    appRestart(): Promise<void>;
  }
  // ipc/mock.ts: MockOptions gains safeMode?: SafeModeReason; crashNotice?: CrashNotice; crashReport?: CrashReport;
  //   busy?: string; recoveryEntry?: RecoveryEntry. MockState gains safeMode, crashNotice, uiErrors: string[], restarted: boolean.
  ```

- [ ] **Step 1: The failing tests (Rust)**

`ipc.rs`'s tests: change `assert_eq!(COMMANDS.len(), 30);` to `37` and `assert_eq!(logging, 26);` to `33`. Run `cargo test -p cmdstudio-app --lib ipc::` → FAIL on the counts.

- [ ] **Step 2: The names, the commands, the capability**

`command_names.rs`: after `pub const THEME_RESUME`, add

```rust
pub const APP_READY: &str = "app_ready";
pub const APP_STATUS: &str = "app_status";
pub const CRASH_DISMISS: &str = "crash_dismiss";
pub const CRASH_REPORT_READ: &str = "crash_report_read";
pub const SETTINGS_RESET: &str = "settings_reset";
pub const LOG_UI_ERROR: &str = "log_ui_error";
pub const APP_RESTART: &str = "app_restart";
```

and the seven to the end of `COMMANDS` in that order; the doc `/// The 37 IPC command names, in the order of spec §5.7.`; the header comment names them: `and \`app_ready\`, \`app_status\`, \`crash_dismiss\`, \`crash_report_read\`, \`settings_reset\`, \`log_ui_error\` and \`app_restart\` (phase 3b-2, recovery spec §8)`.

`ipc.rs`, after `theme_resume` and before `handler`:

```rust
/// The UI drew its first page (recovery spec §5.1).
#[tauri::command]
pub async fn app_ready(state: TauriState<'_, Shared>) -> Result<(), AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::APP_READY, |b| b.app_ready())).await
}

/// What runs now, safe mode and why, the crash notice, what `--recovery` did (recovery spec
/// §6.3, §8). The first thing the UI asks on every load.
#[tauri::command]
pub async fn app_status(state: TauriState<'_, Shared>) -> Result<AppStatus, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::APP_STATUS, |b| b.app_status())).await
}

#[tauri::command]
pub async fn crash_dismiss(state: TauriState<'_, Shared>) -> Result<(), AppError> {
    let b = state.inner().clone();
    blocking(move || {
        b.logged(names::CRASH_DISMISS, |b| {
            b.crash_dismiss();
            Ok(())
        })
    })
    .await
}

#[tauri::command]
pub async fn crash_report_read(
    state: TauriState<'_, Shared>,
) -> Result<Option<CrashReport>, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::CRASH_REPORT_READ, |b| b.crash_report_read())).await
}

#[tauri::command]
pub async fn settings_reset(state: TauriState<'_, Shared>) -> Result<SettingsLoad, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::SETTINGS_RESET, |b| b.settings_reset())).await
}

/// The UI's own error (recovery spec §6.5), logged redacted, twenty per run.
#[tauri::command]
pub async fn log_ui_error(
    state: TauriState<'_, Shared>,
    message: String,
    place: String,
) -> Result<(), AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::LOG_UI_ERROR, |b| b.log_ui_error(&message, &place))).await
}

/// Starts CMD Studio again and ends this process (recovery spec §6.3): a transaction that
/// runs is cut off here and rolled back by the journal at the next command that changes the
/// machine, as a crash's would be. The answer never reaches the window; the exit comes after
/// the spawn, so a spawn that fails is an error the window still sees.
#[tauri::command]
pub async fn app_restart(app: AppHandle, state: TauriState<'_, Shared>) -> Result<(), AppError> {
    let b = state.inner().clone();
    let spawned = blocking(move || {
        b.logged(names::APP_RESTART, |_| {
            crate::restart_process().map_err(AppError::internal)
        })
    })
    .await;
    if spawned.is_ok() {
        app.exit(0);
    }
    spawned
}
```

(`use crate::backend::run::AppStatus; use crate::backend::run::CrashReport;` join the imports.) `handler()`'s list gains the seven names. `ipc.rs`'s test `each_command_logs_once…` counts `.logged(` per command: `app_restart` has one; its `names::` count is one. Its module doc's first paragraph: `Every command that reads or changes the machine, the data folder, the settings or the run (33 of the 37) …`.

`capabilities/default.json`: the description names the seven after `theme_resume`; the permissions gain, after `allow-theme-resume`: `allow-app-ready`, `allow-app-status`, `allow-crash-dismiss`, `allow-crash-report-read`, `allow-settings-reset`, `allow-log-ui-error`, `allow-app-restart`. `config_guard.rs:3`: `all 30` → `all 37`.

Run: `cargo test -p cmdstudio-app --lib ipc::` → `ok. 4 passed`. `cargo build -p cmdstudio-app` (the ACL of the seven is generated by `build.rs`).

- [ ] **Step 3: The bindings**

Delete `app/ui/src/bindings/ResumeOutcome.ts`, then, in Git Bash:

```bash
cd <repo>
TS_RS_EXPORT_DIR="$(pwd -W)/app/ui/src/bindings" cargo test -p cmdstudio-app --lib export_bindings
git status --short app/ui/src/bindings
```

Expected: `AppStatus.ts`, `SafeModeReason.ts`, `CrashNotice.ts`, `CrashReport.ts`, `RecoveryEntry.ts` new; `ResumeOutcome.ts` changed (`| { "kind": "notKnown" }`); nothing else changed. `AppStatus.ts` must read `busy: string | null, safeMode: boolean, safeModeReason: SafeModeReason | null, crashNotice: CrashNotice | null, recoveryEntry: RecoveryEntry | null`.

- [ ] **Step 4: The UI's `Ipc` (failing tests first)**

`app/ui/src/ipc/tauri.test.ts`, inside `describe("Tauri's IPC")`, add:

```ts
  it("calls the seven commands of phase 3b-2 by their names and argument names", async () => {
    invoke.mockReset();
    invoke.mockResolvedValue(undefined);
    await tauriIpc.appReady();
    await tauriIpc.appStatus();
    await tauriIpc.crashDismiss();
    await tauriIpc.crashReportRead();
    await tauriIpc.settingsReset();
    await tauriIpc.logUiError("TypeError: x", "Home");
    await tauriIpc.appRestart();
    expect(invoke.mock.calls).toEqual([
      ["app_ready", undefined],
      ["app_status", undefined],
      ["crash_dismiss", undefined],
      ["crash_report_read", undefined],
      ["settings_reset", undefined],
      ["log_ui_error", { message: "TypeError: x", place: "Home" }],
      ["app_restart", undefined],
    ]);
  });
```

`app/ui/src/ipc/mock.test.ts`, inside `describe("the mock IPC")`:

```ts
  it("answers the app's status as fixed data from its options, notes ready and dismiss, and counts the UI's errors", async () => {
    const plain = createMockIpc();
    expect(await plain.appStatus()).toEqual({ busy: null, safeMode: false, safeModeReason: null, crashNotice: null, recoveryEntry: null });
    await plain.appReady();
    expect(plain.state.ready).toBe(true);
    const ipc = createMockIpc({
      safeMode: { kind: "failedStarts", count: 2 },
      crashNotice: { atMs: 1791694709000, hasReport: true },
      crashReport: { file: "crash-1791694709000.txt", atMs: 1791694709000, text: "CMD Studio 0.1.0 crashed at 2026-10-11T04:58:29Z\nmessage: boom\n" },
      busy: "apply",
    });
    const status = await ipc.appStatus();
    expect(status.safeMode).toBe(true);
    expect(status.safeModeReason).toEqual({ kind: "failedStarts", count: 2 });
    expect(status.crashNotice).toEqual({ atMs: 1791694709000, hasReport: true });
    expect(status.busy).toBe("apply");
    expect((await ipc.crashReportRead())?.file).toBe("crash-1791694709000.txt");
    await ipc.crashDismiss();
    expect((await ipc.appStatus()).crashNotice).toBeNull();
    await ipc.logUiError("TypeError: x", "Home");
    expect(ipc.state.uiErrors).toEqual(["Home: TypeError: x"]);
    await ipc.appRestart();
    expect(ipc.state.restarted).toBe(true);
    expect(await createMockIpc().crashReportRead()).toBeNull();
    expect(ipc.state.calls).toEqual(expect.arrayContaining(["appStatus", "crashReportRead", "crashDismiss", "logUiError", "appRestart"]));
  });

  it("resets the settings to the defaults and names the file set aside, as the backend does", async () => {
    const ipc = createMockIpc({ settings: { language: "th", wizardDone: true } });
    const reset = await ipc.settingsReset();
    expect(reset.settings).toEqual({ language: "en", target: "all", setWtDefaultProfile: true, updates: "auto", wizardDone: false });
    expect(reset.resetFrom).toMatch(/^settings\.json\.reset-\d+$/);
    expect((await ipc.settingsGet()).settings.language).toBe("en");
  });

  it("gives what --recovery did as fixed data, and turning back on says notKnown when told", async () => {
    const entry = { notes: [], unrestored: [], pause: null, skipped: "no theme is applied" };
    const ipc = createMockIpc({ safeMode: { kind: "recovery" }, recoveryEntry: entry });
    expect((await ipc.appStatus()).recoveryEntry).toEqual(entry);
    const unknown = createMockIpc({ paused: { by: "app", level: "computerUnknown", reason: "user", atMs: 1, count: 0, resume: null } });
    expect(await unknown.themeResume()).toEqual({ kind: "notKnown" });
  });
```

Run: `npm run typecheck` → errors (the methods are missing); that is the failure.

- [ ] **Step 5: `types.ts`, `tauri.ts`, `mock.ts`**

`types.ts`: import `AppStatus`, `CrashReport` from the bindings; add to `interface Ipc` after `themeResume`:

```ts
  /** The UI drew its first page (recovery spec §5.1): the run is a good start from then on. */
  appReady(): Promise<void>;
  /** What runs now, safe mode and why, the crash notice, what `--recovery` did (recovery spec
   * §6.3, §8). The first call on every load; may be asked again every 2 s only while `busy`. */
  appStatus(): Promise<AppStatus>;
  crashDismiss(): Promise<void>;
  /** The newest crash note, redacted; `null` when there is none. */
  crashReportRead(): Promise<CrashReport | null>;
  /** settings.json set aside as `settings.json.reset-<ms>`; the defaults from then on. */
  settingsReset(): Promise<SettingsLoad>;
  /** One of the UI's own errors (recovery spec §6.5); the backend logs twenty per run. */
  logUiError(message: string, place: string): Promise<void>;
  /** Starts CMD Studio again and ends this one (recovery spec §6.3); never answers. */
  appRestart(): Promise<void>;
```

`tauri.ts`, after `themeResume`:

```ts
  appReady: () => call("app_ready"),
  appStatus: () => call("app_status"),
  crashDismiss: () => call("crash_dismiss"),
  crashReportRead: () => call("crash_report_read"),
  settingsReset: () => call("settings_reset"),
  logUiError: (message, place) => call("log_ui_error", { message, place }),
  appRestart: () => call("app_restart"),
```

`mock.ts`: `MockOptions` gains

```ts
  /** The app is in safe mode for this reason (recovery spec §5.2), as `app_status` says it. */
  safeMode?: SafeModeReason;
  /** The earlier run ended abnormally (recovery spec §5.3), as given. */
  crashNotice?: CrashNotice;
  /** The newest crash note, as given; none when not given. */
  crashReport?: CrashReport;
  /** `app_status.busy`, as given, every time (recovery spec §6.3). */
  busy?: string;
  /** What `--recovery` did, as given (recovery spec §4.4). */
  recoveryEntry?: RecoveryEntry;
```

`MockState` gains `safeMode: SafeModeReason | null; crashNotice: CrashNotice | null; ready: boolean; uiErrors: string[]; restarted: boolean;` (initialized from the options, `ready: false`, `uiErrors: []`, `restarted: false`). The methods, after `themeResume`:

```ts
    async appReady(): Promise<void> {
      state.calls.push("appReady");
      state.ready = true;
    },
    async appStatus(): Promise<AppStatus> {
      state.calls.push("appStatus");
      return {
        busy: options.busy ?? null,
        safeMode: state.safeMode !== null,
        safeModeReason: state.safeMode,
        crashNotice: state.crashNotice,
        recoveryEntry: options.recoveryEntry ?? null,
      };
    },
    async crashDismiss(): Promise<void> {
      state.calls.push("crashDismiss");
      state.crashNotice = null;
    },
    async crashReportRead(): Promise<CrashReport | null> {
      state.calls.push("crashReportRead");
      return options.crashReport ? structuredClone(options.crashReport) : null;
    },
    async settingsReset(): Promise<SettingsLoad> {
      state.calls.push("settingsReset");
      state.settings = { language: "en", target: "all", setWtDefaultProfile: true, updates: "auto", wizardDone: false };
      return { settings: structuredClone(state.settings), resetFrom: `settings.json.reset-${now()}` };
    },
    async logUiError(message, place): Promise<void> {
      state.calls.push("logUiError");
      state.uiErrors.push(`${place}: ${message}`);
    },
    async appRestart(): Promise<void> {
      state.calls.push("appRestart");
      state.restarted = true;
    },
```

In `themeResume`, before the `paused.resume` line: `if (paused.level === "computerUnknown") return { kind: "notKnown" };` (the backend's m4). Read how `settingsGet` builds its answer and keep `settingsReset`'s defaults equal to `DEFAULT_SETTINGS` of `state.tsx` (import it rather than repeating the literal if the mock already imports from `../state`; the mock may not import React modules: then the literal, as above).

- [ ] **Step 6: Run the UI checks**

`npm run typecheck`; `npm test` → `Tests 265 passed` (261 + 1 tauri + 3 mock); `TZ=UTC npm test` the same.

- [ ] **Step 7: Checks and commit**

`cargo fmt --all`; the four clippy commands; `cargo test -p cmdstudio-app --lib` → `ok. 251 passed` (or 250 under F1).

```bash
git add -- app/src-tauri/src/command_names.rs app/src-tauri/src/ipc.rs app/src-tauri/capabilities/default.json app/src-tauri/src/config_guard.rs app/ui/src/bindings app/ui/src/ipc/types.ts app/ui/src/ipc/tauri.ts app/ui/src/ipc/tauri.test.ts app/ui/src/ipc/mock.ts app/ui/src/ipc/mock.test.ts
```

Message: `app, ui: seven IPC commands for the run, the crash notice, the settings reset and the restart; the UI's Ipc over Tauri and in the mock (recovery spec §8)`.

---

### Task 8: The UI's `Ipc` wrapper: the "still working" bar, the read-only deadline with `E_TIMEOUT`, late answers dropped, and "Restart CMD Studio" after three minutes

Recovery spec §6.1, §6.2, §6.3. One wrapper over any `Ipc`, one list of what each command is, one store the bar reads.

**Files:**
- Create: `app/ui/src/ipc/work.ts`, `app/ui/src/ipc/work.test.ts`
- Create: `app/ui/src/components/WorkingBar.tsx`
- Modify: `app/ui/src/i18n/en.ts`, `th.ts` (the `work.*` keys, `error.timeout` none: the texts come from the §5.9 row)
- Modify: `app/ui/src/styles.css` (`.working-bar`)

**Interfaces:**
- Consumes: `Ipc`, `IpcError` (`ipc/types.ts`), `Dialog` (`components/Dialog.tsx`), `useI18n`, `Key`.
- Produces:
  ```ts
  // ipc/work.ts
  export const STILL_WORKING_MS = 10_000, READ_DEADLINE_MS = 60_000, RESTART_AFTER_MS = 180_000;
  export type Kind = "read" | "changes" | "user" | "exit";
  export const KINDS: Record<keyof Ipc, Kind>;
  export const TASK: Record<keyof Ipc, Key>;
  export interface WorkState { task: Key | null; offerRestart: boolean }
  export interface WorkStore { get(): WorkState; subscribe(listener: () => void): () => void }
  export function withWork(ipc: Ipc, now?: () => number): { ipc: Ipc; work: WorkStore };
  export const TIMEOUT_ROW: { th: string; en: string };
  // components/WorkingBar.tsx
  export const WorkContext: Context<WorkStore | null>;
  export function WorkingBar(): ReactNode;    // reads the context; role="status"; the restart button and its dialog
  ```

- [ ] **Step 1: The failing tests**

`app/ui/src/ipc/work.test.ts`:

```ts
// The wrapper of recovery spec §6: it counts what waits, says so after ten seconds, gives a
// read-only command up after a minute with E_TIMEOUT, drops the answer that comes after that,
// and offers a restart once a change runs for three minutes. Fake timers throughout.
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { en } from "../i18n/en";
import { createMockIpc } from "./mock";
import { IpcError, type Ipc } from "./types";
import { KINDS, READ_DEADLINE_MS, RESTART_AFTER_MS, STILL_WORKING_MS, TASK, TIMEOUT_ROW, withWork } from "./work";

/** The E_TIMEOUT row of `MEANINGS` in error.rs (spec §5.9), as the Rust source has it. */
function timeoutRow(): { th: string; en: string } {
  const here = dirname(fileURLToPath(import.meta.url));
  const source = readFileSync(resolve(here, "../../../src-tauri/src/error.rs"), "utf8");
  const row = /\(\s*"E_TIMEOUT",\s*r#"([^]*?)"#,\s*r#"([^]*?)"#,\s*\)/.exec(source);
  if (!row?.[1] || !row[2]) throw new Error("error.rs has no E_TIMEOUT row");
  return { th: row[1], en: row[2] };
}

/** A promise the test settles by hand. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

describe("the one list of commands", () => {
  it("names every method of Ipc once, with its kind and its task", () => {
    const methods = Object.keys(createMockIpc()).filter((k) => k !== "state").sort();
    expect(Object.keys(KINDS).sort()).toEqual(methods);
    expect(Object.keys(TASK).sort()).toEqual(methods);
    expect(KINDS.apply).toBe("changes");
    expect(KINDS.undoLast).toBe("changes");
    expect(KINDS.snapshotRestore).toBe("changes");
    expect(KINDS.revertAll).toBe("changes");
    expect(KINDS.themePause).toBe("changes");
    expect(KINDS.themeImportPreview).toBe("user");
    expect(KINDS.themeExport).toBe("user");
    expect(KINDS.appRestart).toBe("exit");
    expect(Object.values(KINDS).filter((k) => k === "changes")).toHaveLength(5);
    for (const key of Object.values(TASK)) expect(key in en, key).toBe(true);
  });

  it("carries the E_TIMEOUT row of spec §5.9 as error.rs has it", () => {
    expect(TIMEOUT_ROW).toEqual(timeoutRow());
  });
});

describe("the wrapper", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("says what still works after ten seconds and clears when it answers", async () => {
    const inner = createMockIpc();
    const held = deferred<Awaited<ReturnType<Ipc["themesList"]>>>();
    vi.spyOn(inner, "themesList").mockReturnValue(held.promise);
    const { ipc, work } = withWork(inner, () => Date.now());
    const seen: string[] = [];
    work.subscribe(() => seen.push(String(work.get().task)));
    const call = ipc.themesList();
    expect(work.get().task).toBeNull();
    await vi.advanceTimersByTimeAsync(STILL_WORKING_MS - 1000);
    expect(work.get().task).toBeNull();
    await vi.advanceTimersByTimeAsync(1000);
    expect(work.get()).toEqual({ task: "work.readingThemes", offerRestart: false });
    held.resolve(await createMockIpc().themesList());
    await call;
    expect(work.get().task).toBeNull();
    expect(seen).toContain("work.readingThemes");
  });

  it("gives a read-only command up after a minute with E_TIMEOUT, both texts from the row", async () => {
    const inner = createMockIpc();
    vi.spyOn(inner, "doctorRun").mockReturnValue(new Promise(() => {}));
    const { ipc, work } = withWork(inner, () => Date.now());
    const call = ipc.doctorRun();
    const settled = call.then(
      () => "resolved",
      (e: unknown) => e,
    );
    await vi.advanceTimersByTimeAsync(READ_DEADLINE_MS);
    const e = await settled;
    expect(e).toBeInstanceOf(IpcError);
    expect((e as IpcError).code).toBe("E_TIMEOUT");
    expect((e as IpcError).messageEn).toBe(TIMEOUT_ROW.en);
    expect((e as IpcError).messageTh).toBe(TIMEOUT_ROW.th);
    expect((e as IpcError).details).toBe("doctor_run did not answer within 60 s");
    expect(work.get().task).toBeNull();
  });

  it("a late answer after the deadline is dropped and the retry's answer is shown", async () => {
    // Review Focus 3.
    const inner = createMockIpc();
    const first = deferred<string>();
    const second = deferred<string>();
    vi.spyOn(inner, "diagnosticsCopy").mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { ipc, work } = withWork(inner, () => Date.now());
    const shown: string[] = [];
    const show = (p: Promise<string>) =>
      p.then(
        (text) => shown.push(text),
        () => shown.push("E_TIMEOUT"),
      );
    const one = show(ipc.diagnosticsCopy());
    await vi.advanceTimersByTimeAsync(READ_DEADLINE_MS);
    await one;
    expect(shown).toEqual(["E_TIMEOUT"]);
    const two = show(ipc.diagnosticsCopy());
    first.resolve("late");
    await vi.advanceTimersByTimeAsync(0);
    second.resolve("fresh");
    await two;
    expect(shown).toEqual(["E_TIMEOUT", "fresh"]);
    expect(work.get().task).toBeNull();
  });

  it("never gives up a change, a dialog or the restart, and offers the restart after three minutes of a change", async () => {
    const inner = createMockIpc();
    vi.spyOn(inner, "apply").mockReturnValue(new Promise(() => {}));
    vi.spyOn(inner, "themeExport").mockReturnValue(new Promise(() => {}));
    const { ipc, work } = withWork(inner, () => Date.now());
    const change = ipc.apply({ themeId: "x", target: "all", setWtDefaultProfile: true, replaceOtherClink: false }, "d", () => {});
    const dialog = ipc.themeExport("x");
    let settled = false;
    void change.finally(() => (settled = true));
    void dialog.finally(() => (settled = true));
    await vi.advanceTimersByTimeAsync(READ_DEADLINE_MS + 1000);
    expect(settled).toBe(false);
    expect(work.get()).toEqual({ task: "work.applying", offerRestart: false });
    await vi.advanceTimersByTimeAsync(RESTART_AFTER_MS - READ_DEADLINE_MS);
    expect(work.get()).toEqual({ task: "work.applying", offerRestart: true });
  });

  it("the restart itself is never counted as work", async () => {
    const inner = createMockIpc();
    vi.spyOn(inner, "appRestart").mockReturnValue(new Promise(() => {}));
    const { ipc, work } = withWork(inner, () => Date.now());
    void ipc.appRestart();
    await vi.advanceTimersByTimeAsync(STILL_WORKING_MS * 2);
    expect(work.get().task).toBeNull();
  });
});
```

`app/ui/src/components/components.test.tsx`, a new `describe("the working bar")`:

```tsx
describe("the working bar", () => {
  const store = (state: WorkState) => {
    const listeners = new Set<() => void>();
    return {
      get: () => state,
      subscribe: (l: () => void) => {
        listeners.add(l);
        return () => listeners.delete(l);
      },
      set: (next: WorkState) => {
        state = next;
        listeners.forEach((l) => l());
      },
    };
  };
  const mount = (work: WorkStore, ipc = createMockIpc()) =>
    render(
      <IpcProvider ipc={ipc}>
        <I18nProvider language="en" setLanguage={() => {}}>
          <WorkContext.Provider value={work}>
            <WorkingBar />
          </WorkContext.Provider>
        </I18nProvider>
      </IpcProvider>,
    );

  it("is an empty status region until something works, then names the task", () => {
    const s = store({ task: null, offerRestart: false });
    mount(s);
    const region = screen.getByRole("status", { name: "Still working" });
    expect(region).toHaveTextContent("");
    act(() => s.set({ task: "work.applying", offerRestart: false }));
    expect(region).toHaveTextContent("CMD Studio is still working: applying the theme…");
    expect(screen.queryByRole("button", { name: "Restart CMD Studio" })).toBeNull();
  });

  it("offers the restart after three minutes, asks first, and restarts on yes", async () => {
    const s = store({ task: "work.applying", offerRestart: true });
    const ipc = createMockIpc();
    const user = userEvent.setup();
    mount(s, ipc);
    await user.click(screen.getByRole("button", { name: "Restart CMD Studio" }));
    const dialog = screen.getByRole("dialog", { name: "Restart CMD Studio" });
    expect(dialog).toHaveTextContent("A change that has not finished is put back when CMD Studio starts again.");
    await user.click(within(dialog).getByRole("button", { name: "Restart CMD Studio" }));
    expect(ipc.state.restarted).toBe(true);
  });
});
```

(Read the file's imports: `render`, `screen`, `act`, `within` from `@testing-library/react`, `userEvent`, `IpcProvider`, `I18nProvider`, `createMockIpc` are there or get added; add `WorkContext`, `WorkingBar`, `WorkStore`, `WorkState`.)

- [ ] **Step 2: Run them to see them fail**

`npm test -- work` → the module does not exist. `npm run typecheck` → errors.

- [ ] **Step 3: `work.ts`**

```ts
// The wrapper every screen's IPC goes through (recovery spec §6.1–§6.3). It counts what waits;
// after ten seconds the bar (`WorkingBar`) names the oldest task; a command that changes
// nothing is given up after a minute with E_TIMEOUT and its late answer dropped; one that
// changes the machine is never given up (its transaction must end), but after three minutes
// the bar offers "Restart CMD Studio". The two commands that wait for the user's file dialog
// and the restart itself are neither timed nor shown. One list says what each command is.
import type { Key } from "../i18n/types";
import { IpcError, type Ipc } from "./types";

export const STILL_WORKING_MS = 10_000;
export const READ_DEADLINE_MS = 60_000;
export const RESTART_AFTER_MS = 180_000;

/** What a command is: it only reads or computes (a deadline), changes the machine (a restart
 * after three minutes), waits for the user in a native dialog, or ends the process. */
export type Kind = "read" | "changes" | "user" | "exit";

export const KINDS: Record<keyof Ipc, Kind> = {
  systemDetect: "read",
  themesList: "read",
  themeGet: "read",
  themeSave: "read",
  themeDuplicate: "read",
  themeDelete: "read",
  themeImportPreview: "user",
  themeImportConfirm: "read",
  themeExport: "user",
  paletteGenerate: "read",
  contrastCheck: "read",
  contrastFix: "read",
  previewRender: "read",
  planApply: "read",
  apply: "changes",
  undoLast: "changes",
  snapshotsList: "read",
  snapshotRestore: "changes",
  revertAll: "changes",
  doctorRun: "read",
  doctorFix: "read",
  settingsGet: "read",
  settingsSet: "read",
  openCmd: "read",
  openExternal: "read",
  diagnosticsCopy: "read",
  advancedRead: "read",
  recoveryStatus: "read",
  themePause: "changes",
  themeResume: "read",
  appReady: "read",
  appStatus: "read",
  crashDismiss: "read",
  crashReportRead: "read",
  settingsReset: "read",
  logUiError: "read",
  appRestart: "exit",
};

/** The bar's words for each command (recovery spec §6.1): fourteen tasks. */
export const TASK: Record<keyof Ipc, Key> = {
  systemDetect: "work.reading",
  themesList: "work.readingThemes",
  themeGet: "work.readingThemes",
  themeSave: "work.savingTheme",
  themeDuplicate: "work.savingTheme",
  themeDelete: "work.savingTheme",
  themeImportPreview: "work.choosingFile",
  themeImportConfirm: "work.savingTheme",
  themeExport: "work.choosingFile",
  paletteGenerate: "work.drawingPreview",
  contrastCheck: "work.drawingPreview",
  contrastFix: "work.drawingPreview",
  previewRender: "work.drawingPreview",
  planApply: "work.planning",
  apply: "work.applying",
  undoLast: "work.undoing",
  snapshotsList: "work.reading",
  snapshotRestore: "work.undoing",
  revertAll: "work.undoing",
  doctorRun: "work.checking",
  doctorFix: "work.checking",
  settingsGet: "work.settings",
  settingsSet: "work.settings",
  openCmd: "work.opening",
  openExternal: "work.opening",
  diagnosticsCopy: "work.reading",
  advancedRead: "work.reading",
  recoveryStatus: "work.reading",
  themePause: "work.pausing",
  themeResume: "work.resuming",
  appReady: "work.noting",
  appStatus: "work.reading",
  crashDismiss: "work.noting",
  crashReportRead: "work.reading",
  settingsReset: "work.settings",
  logUiError: "work.noting",
  appRestart: "work.restarting",
};

/** The E_TIMEOUT row of spec §5.9, as `MEANINGS` in app/src-tauri/src/error.rs has it (a test
 * reads that row and compares). */
export const TIMEOUT_ROW = {
  th: "CMD Studio ไม่ตอบภายใน 60 วินาที สำหรับคำสั่งที่ไม่เปลี่ยนเครื่อง ให้ลองใหม่ ถ้ายังเป็นอยู่ให้เริ่ม CMD Studio ใหม่ (recovery spec §6.2)",
  en: "CMD Studio did not answer within 60 seconds for a command that changes nothing; try again, and if it stays, restart CMD Studio (recovery spec §6.2)",
};

export interface WorkState {
  /** The oldest task that has waited ten seconds or more; `null` when none has. */
  task: Key | null;
  /** A change has run for three minutes or more. */
  offerRestart: boolean;
}

export interface WorkStore {
  get(): WorkState;
  subscribe(listener: () => void): () => void;
}

/** The command names as the Rust side logs them, for E_TIMEOUT's details. */
function snake(method: string): string {
  return method.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
}

interface Pending {
  seq: number;
  method: keyof Ipc;
  startedAt: number;
}

export function withWork(ipc: Ipc, now: () => number = Date.now): { ipc: Ipc; work: WorkStore } {
  const pending: Pending[] = [];
  const listeners = new Set<() => void>();
  let state: WorkState = { task: null, offerRestart: false };
  let seq = 0;
  let ticker: ReturnType<typeof setInterval> | null = null;

  const compute = (): WorkState => {
    const t = now();
    const shown = pending.filter((p) => KINDS[p.method] !== "exit" && t - p.startedAt >= STILL_WORKING_MS);
    const oldest = shown.reduce<Pending | null>((a, p) => (a === null || p.startedAt < a.startedAt ? p : a), null);
    const offerRestart = pending.some((p) => KINDS[p.method] === "changes" && t - p.startedAt >= RESTART_AFTER_MS);
    return { task: oldest ? TASK[oldest.method] : null, offerRestart };
  };
  const publish = () => {
    const next = compute();
    if (next.task === state.task && next.offerRestart === state.offerRestart) return;
    state = next;
    listeners.forEach((l) => l());
  };
  const tick = () => {
    publish();
    if (pending.length === 0 && ticker !== null) {
      clearInterval(ticker);
      ticker = null;
    }
  };
  const begin = (method: keyof Ipc): Pending => {
    const p = { seq: ++seq, method, startedAt: now() };
    pending.push(p);
    if (ticker === null) ticker = setInterval(tick, 1000);
    return p;
  };
  const end = (p: Pending) => {
    const at = pending.indexOf(p);
    if (at >= 0) pending.splice(at, 1);
    tick();
  };

  const track = <T,>(method: keyof Ipc, run: () => Promise<T>): Promise<T> => {
    if (KINDS[method] === "exit") return run();
    const p = begin(method);
    const settled = run().finally(() => end(p));
    if (KINDS[method] !== "read") return settled;
    return new Promise<T>((resolve, reject) => {
      let done = false;
      const timer = setTimeout(() => {
        if (done) return;
        done = true;
        end(p);
        reject(
          new IpcError({
            code: "E_TIMEOUT",
            messageTh: TIMEOUT_ROW.th,
            messageEn: TIMEOUT_ROW.en,
            details: `${snake(method)} did not answer within ${READ_DEADLINE_MS / 1000} s`,
          }),
        );
      }, READ_DEADLINE_MS);
      settled.then(
        (value) => {
          // A late answer, after the deadline, is dropped (recovery spec §6.2).
          if (done) return;
          done = true;
          clearTimeout(timer);
          resolve(value);
        },
        (error: unknown) => {
          if (done) return;
          done = true;
          clearTimeout(timer);
          reject(error);
        },
      );
    });
  };

  const wrapped = {} as Record<keyof Ipc, (...args: unknown[]) => Promise<unknown>>;
  const inner = ipc as unknown as Record<keyof Ipc, (...args: unknown[]) => Promise<unknown>>;
  for (const method of Object.keys(KINDS) as (keyof Ipc)[]) {
    wrapped[method] = (...args: unknown[]) => track(method, () => inner[method](...args));
  }
  const work: WorkStore = {
    get: () => state,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
  return { ipc: wrapped as unknown as Ipc, work };
}
```

(`run().finally` on a promise that never settles never runs `end`: the deadline's `end(p)` does, for a read; a change or a dialog stays pending, which is the spec's point.) `Object.keys(KINDS)` is the mock's key set minus `state`: the wrapper never carries the mock's `state`; tests that need it keep the inner mock.

- [ ] **Step 4: The words and the bar**

`en.ts`, a new block after the `resume.*` keys:

```ts
  "work.label": "Still working",
  "work.still": "CMD Studio is still working: {task}…",
  "work.reading": "reading this computer",
  "work.readingThemes": "reading your themes",
  "work.savingTheme": "saving the theme",
  "work.choosingFile": "waiting for the file dialog",
  "work.drawingPreview": "drawing the preview",
  "work.planning": "planning the change",
  "work.applying": "applying the theme",
  "work.undoing": "putting things back",
  "work.checking": "checking this computer",
  "work.settings": "reading or saving the settings",
  "work.opening": "opening a window",
  "work.pausing": "pausing the theme",
  "work.resuming": "turning the theme back on",
  "work.noting": "noting",
  "work.restarting": "restarting",
  "work.restart": "Restart CMD Studio",
  "work.restartConfirm": "A change that has not finished is put back when CMD Studio starts again.",
```

`th.ts`, the same keys:

```ts
  "work.label": "ยังทำงานอยู่",
  "work.still": "CMD Studio ยังทำงานอยู่: {task}…",
  "work.reading": "กำลังอ่านข้อมูลของเครื่องนี้",
  "work.readingThemes": "กำลังอ่านธีมของคุณ",
  "work.savingTheme": "กำลังบันทึกธีม",
  "work.choosingFile": "กำลังรอกล่องเลือกไฟล์",
  "work.drawingPreview": "กำลังวาดพรีวิว",
  "work.planning": "กำลังวางแผนการเปลี่ยนแปลง",
  "work.applying": "กำลังใช้ธีม",
  "work.undoing": "กำลังคืนค่าเดิม",
  "work.checking": "กำลังตรวจเครื่องนี้",
  "work.settings": "กำลังอ่านหรือบันทึกการตั้งค่า",
  "work.opening": "กำลังเปิดหน้าต่าง",
  "work.pausing": "กำลังหยุดธีม",
  "work.resuming": "กำลังเปิดธีมกลับ",
  "work.noting": "กำลังจดบันทึก",
  "work.restarting": "กำลังเริ่มใหม่",
  "work.restart": "เริ่ม CMD Studio ใหม่",
  "work.restartConfirm": "การเปลี่ยนแปลงที่ยังไม่เสร็จจะถูกคืนค่าเดิมตอน CMD Studio เริ่มใหม่",
```

`app/ui/src/components/WorkingBar.tsx`:

```tsx
// The "still working" bar of recovery spec §6.1 and the restart of §6.3: a status region at the
// top of the page, empty until a command has waited ten seconds, then the task's name; after
// three minutes of a change, "Restart CMD Studio", which asks first (the change that has not
// finished is put back by the journal when the app starts again) and then calls `appRestart`.
import { createContext, useContext, useState, useSyncExternalStore } from "react";
import { useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import type { WorkStore } from "../ipc/work";
import { Dialog } from "./Dialog";
import { ErrorNotice } from "./Notices";

export const WorkContext = createContext<WorkStore | null>(null);

const NONE: WorkStore = { get: () => ({ task: null, offerRestart: false }), subscribe: () => () => {} };

export function WorkingBar() {
  const store = useContext(WorkContext) ?? NONE;
  const ipc = useIpc();
  const { t } = useI18n();
  const state = useSyncExternalStore(store.subscribe, store.get, store.get);
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const restart = async () => {
    setError(null);
    try {
      await ipc.appRestart();
    } catch (e) {
      setError(e);
      setAsking(false);
    }
  };
  return (
    <>
      {/* Always there, its text toggled, so a screen reader hears the change. */}
      <div role="status" aria-label={t("work.label")} className={state.task ? "working-bar" : "working-bar empty"}>
        {state.task && <span>{t("work.still", { task: t(state.task) })}</span>}
        {state.offerRestart && (
          <button type="button" onClick={() => setAsking(true)}>
            {t("work.restart")}
          </button>
        )}
      </div>
      {asking && (
        <Dialog title={t("work.restart")} onClose={() => setAsking(false)}>
          <p>{t("work.restartConfirm")}</p>
          {error !== null && <ErrorNotice error={error} />}
          <div className="row-actions">
            <button type="button" className="primary" onClick={() => void restart()}>
              {t("work.restart")}
            </button>
            <button type="button" onClick={() => setAsking(false)}>
              {t("common.cancel")}
            </button>
          </div>
        </Dialog>
      )}
    </>
  );
}
```

`styles.css`, beside `.notice`:

```css
/* The "still working" bar (recovery spec §6.1): at the top, over the page; empty until needed. */
.working-bar {
  position: sticky;
  top: 0;
  z-index: 5;
  display: flex;
  gap: 1rem;
  align-items: center;
  padding: 0.5rem 1rem;
  background: var(--warn-bg, #fff4d6);
  border-bottom: 1px solid var(--warn-border, #e0b94f);
}
.working-bar.empty {
  display: none;
}
```

(Read `styles.css` for the variable names the `.notice.warn` rule uses and use those.)

- [ ] **Step 5: Run the UI checks**

`npm run typecheck`; `npm test` → `Tests 274 passed` (265 + 7 work + 2 bar); `TZ=UTC npm test`.

- [ ] **Step 6: Commit**

```bash
git add -- app/ui/src/ipc/work.ts app/ui/src/ipc/work.test.ts app/ui/src/components/WorkingBar.tsx app/ui/src/components/components.test.tsx app/ui/src/i18n/en.ts app/ui/src/i18n/th.ts app/ui/src/styles.css
```

Message: `ui: the Ipc wrapper says what still works after ten seconds, gives a read up after a minute with E_TIMEOUT, drops late answers, and offers a restart after three minutes of a change (recovery spec §6.1–§6.3)`.

---

### Task 9: The UI's start: the wrapper and the bar in place, the catch-all, `app_status` first, the busy wait, and `app_ready`

Recovery spec §6.3's rule for every load (ask `app_status` first; while a change runs, say so and wait), §6.5 (the catch-all into the existing `CrashNotice`, `log_ui_error`), §5.1 (`app_ready` after the first page). The Recovery route itself is Task 10's.

**Files:**
- Modify: `app/ui/src/start.tsx`, `app/ui/src/start.test.tsx`
- Modify: `app/ui/src/App.tsx`, `app/ui/src/App.test.tsx`
- Modify: `app/ui/src/ipc/mock.ts` (`state.busy`)
- Modify: `app/ui/src/i18n/en.ts`, `th.ts` (`start.busy`)

**Interfaces:**
- Consumes: `withWork`, `WorkContext`, `WorkingBar` (Task 8), `CrashNotice`, `ErrorBoundary` (`components/ErrorBoundary.tsx`), `Ipc.appStatus/appReady/logUiError` (Task 7).
- Produces:
  ```ts
  // start.tsx
  export function watchWindowErrors(ipc: Ipc, onError: (error: unknown) => void): () => void;   // registers `error` and `unhandledrejection`; returns the unregister
  export const UI_ERRORS_SENT_PER_LOAD = 20;
  // App.tsx: `appStatus` first; `busy` → the wait (polled every APP_STATUS_POLL_MS = 2000); `appReady` once the first screen is up; `status` kept in AppState as `appStatus: AppStatus | null`
  // mock.ts: MockState.busy: string | null (from options.busy), returned by appStatus
  ```

- [ ] **Step 1: The failing tests**

`app/ui/src/start.test.tsx` (read its helpers: it mounts `start(container, pickIpc, reload)` with a mock), add:

```tsx
describe("the catch-all (recovery spec §6.5)", () => {
  it("shows the E_INTERNAL notice for an error the window reports, and logs it through the IPC", async () => {
    const ipc = createMockIpc({ settings: { wizardDone: true } });
    const container = document.createElement("div");
    document.body.append(container);
    await act(async () => {
      await start(container, async () => ipc, () => {});
    });
    await screen.findByRole("heading", { name: "Home" });
    await act(async () => {
      window.dispatchEvent(new ErrorEvent("error", { error: new TypeError("x is undefined"), message: "x is undefined" }));
    });
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Something went wrong");
    expect(alert).toHaveTextContent("Code E_INTERNAL");
    expect(within(alert).getByText("x is undefined", { exact: false })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Reload/ })).toBeTruthy();
    await waitFor(() => expect(ipc.state.uiErrors).toEqual(["window: TypeError: x is undefined"]));
    container.remove();
  });

  it("catches a promise nobody handled the same way, and sends at most twenty per load", async () => {
    const ipc = createMockIpc({ settings: { wizardDone: true } });
    const seen: unknown[] = [];
    const stop = watchWindowErrors(ipc, (e) => seen.push(e));
    for (let n = 0; n < 25; n++) {
      const event = new Event("unhandledrejection") as Event & { reason?: unknown };
      event.reason = new Error(`late ${n}`);
      window.dispatchEvent(event);
    }
    await waitFor(() => expect(ipc.state.uiErrors).toHaveLength(20));
    expect(seen).toHaveLength(25);
    expect(ipc.state.uiErrors[0]).toBe("window: Error: late 0");
    stop();
    window.dispatchEvent(new ErrorEvent("error", { error: new Error("after") }));
    expect(seen).toHaveLength(25);
  });
});
```

(`PromiseRejectionEvent` is not in jsdom: a plain `Event` with `reason` set is what the listener reads.)

`app/ui/src/App.test.tsx`, a new `describe("the start (recovery spec §5.1, §6.3)")` before `describe("the first run")`:

```tsx
describe("the start (recovery spec §5.1, §6.3)", () => {
  it("asks app_status before anything, and says it is ready once the first screen is drawn", async () => {
    const { ipc } = await home();
    expect(ipc.state.calls[0]).toBe("appStatus");
    expect(ipc.state.calls.indexOf("settingsGet")).toBeGreaterThan(0);
    await waitFor(() => expect(ipc.state.ready).toBe(true));
    expect(ipc.state.calls.filter((c) => c === "appReady")).toHaveLength(1);
    expect(ipc.state.calls.indexOf("appReady")).toBeGreaterThan(ipc.state.calls.indexOf("settingsGet"));
  });

  it("waits while a change still runs in this process, then reads as a start would", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const { ipc } = await mount({ busy: "apply", settings: { wizardDone: true } });
      expect(await screen.findByRole("status")).toHaveTextContent("A theme is still being applied. CMD Studio waits for it to finish…");
      expect(ipc.state.calls).not.toContain("settingsGet");
      ipc.state.busy = null;
      await vi.advanceTimersByTimeAsync(2100);
      await screen.findByRole("heading", { name: "Home" });
      expect(ipc.state.calls.filter((c) => c === "appStatus").length).toBeGreaterThanOrEqual(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("the working bar sits above the shell and the wizard, empty", async () => {
    await home();
    expect(screen.getByRole("status", { name: "Still working" })).toHaveTextContent("");
  });
});
```

- [ ] **Step 2: Run them to see them fail**

`npm test -- start` and `npm test -- App` → fail (`watchWindowErrors` missing; `appStatus` not called first).

- [ ] **Step 3: `start.tsx`**

Replace the file's body (keep its header comment, extended):

```tsx
// Mounts the app once the IPC is picked (main.tsx calls this; it is a file of its own so the
// tests can hand it an IPC that fails). The IPC goes through `withWork` (recovery spec §6: the
// "still working" bar, the read-only deadline, the restart offer), and the window's own errors
// (`error`, `unhandledrejection`, §6.5) show the same E_INTERNAL notice the boundary shows and go
// to the log through `log_ui_error`, twenty per load. If the IPC cannot be picked, there is no
// app to mount and no boundary to catch anything: the window shows the notice with Reload, never
// a blank window.
import { StrictMode, useEffect, useState, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { App } from "./App";
import { CrashNotice, ErrorBoundary } from "./components/ErrorBoundary";
import { WorkContext, WorkingBar } from "./components/WorkingBar";
import { IpcProvider } from "./ipc/context";
import type { Ipc } from "./ipc/types";
import { withWork } from "./ipc/work";

/** How many of the window's errors are sent to the log per load (the backend keeps its own
 * count per run, recovery spec §6.5). */
export const UI_ERRORS_SENT_PER_LOAD = 20;

/** What was thrown, as one line for the log: its name and message, or its text. */
function describe(error: unknown): string {
  if (error instanceof Error) return `${error.name}: ${error.message}`;
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error) ?? String(error);
  } catch {
    return String(error);
  }
}

/** Listens for the window's errors and promises nobody handled; `onError` gets each, and the
 * first twenty go to the log. Returns the function that stops listening. */
export function watchWindowErrors(ipc: Ipc, onError: (error: unknown) => void): () => void {
  let sent = 0;
  const report = (error: unknown) => {
    onError(error);
    if (sent < UI_ERRORS_SENT_PER_LOAD) {
      sent += 1;
      ipc.logUiError(describe(error), "window").catch(() => {});
    }
  };
  const onErrorEvent = (event: ErrorEvent) => report(event.error ?? event.message);
  const onRejection = (event: Event) => report((event as Event & { reason?: unknown }).reason);
  window.addEventListener("error", onErrorEvent);
  window.addEventListener("unhandledrejection", onRejection);
  return () => {
    window.removeEventListener("error", onErrorEvent);
    window.removeEventListener("unhandledrejection", onRejection);
  };
}

/** The catch-all around the app: the first error the window reports replaces the app with the
 * notice (a render that throws is the boundary's, below this). */
function Catch({ ipc, reload, children }: { ipc: Ipc; reload?: () => void; children: ReactNode }) {
  const [crashed, setCrashed] = useState<{ error: unknown } | null>(null);
  useEffect(() => watchWindowErrors(ipc, (error) => setCrashed((c) => c ?? { error })), [ipc]);
  if (crashed) return <CrashNotice error={crashed.error} reload={reload} />;
  return children;
}

export async function start(container: HTMLElement, pickIpc: () => Promise<Ipc>, reload?: () => void): Promise<Root> {
  const root = createRoot(container);
  let picked: Ipc;
  try {
    picked = await pickIpc();
  } catch (error) {
    console.error("CMD Studio: the app could not start", error);
    root.render(
      <StrictMode>
        <CrashNotice error={error} reload={reload} />
      </StrictMode>,
    );
    return root;
  }
  const { ipc, work } = withWork(picked);
  root.render(
    <StrictMode>
      <ErrorBoundary reload={reload}>
        <IpcProvider ipc={ipc}>
          <WorkContext.Provider value={work}>
            <Catch ipc={ipc} reload={reload}>
              <App />
            </Catch>
          </WorkContext.Provider>
        </IpcProvider>
      </ErrorBoundary>
    </StrictMode>,
  );
  return root;
}
```

(The bar itself is rendered by `App`, inside its `I18nProvider`, so it speaks the UI's language: Step 4. `Catch` is inside `IpcProvider` so the notice's Reload stays the boundary's.)

- [ ] **Step 4: `App.tsx`**

In `App`:

```tsx
  /** `app_status` as read first on this load (recovery spec §6.3); `null` until then. */
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [busyWith, setBusyWith] = useState<string | null>(null);
```

Replace the first `useEffect` (the one that calls `ipc.settingsGet()`) with:

```tsx
  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const readSettings = () =>
      ipc.settingsGet().then(
        (loaded) => {
          if (!alive) return;
          adopt(loaded);
          setRoute(loaded.settings.wizardDone ? { name: "home" } : { name: "wizard" });
        },
        (e: unknown) => {
          if (!alive) return;
          setStartError(e);
          setRoute({ name: "home" });
        },
      );
    // Recovery spec §6.3: on every load, `app_status` first. While this process still runs a
    // change, say so and ask again every two seconds; a status that cannot be read is no reason
    // to hold the start.
    const readStatus = () =>
      ipc.appStatus().then(
        (s) => {
          if (!alive) return;
          if (s.busy !== null) {
            setBusyWith(s.busy);
            timer = setTimeout(() => void readStatus(), APP_STATUS_POLL_MS);
            return;
          }
          setBusyWith(null);
          setStatus(s);
          void readSettings();
        },
        () => {
          if (!alive) return;
          setBusyWith(null);
          void readSettings();
        },
      );
    void readStatus();
    return () => {
      alive = false;
      if (timer !== null) clearTimeout(timer);
    };
  }, [ipc, adopt]);

  // The first screen is drawn: the run is a good start from then on (recovery spec §5.1).
  const ready = useRef(false);
  useEffect(() => {
    if (route === null || ready.current) return;
    ready.current = true;
    ipc.appReady().catch(() => {});
  }, [ipc, route]);
```

with `export const APP_STATUS_POLL_MS = 2000;` above `App`, `import type { AppStatus } from "./bindings/AppStatus";`, and `status` added to `AppState` (`state.tsx`: `appStatus: AppStatus | null;` with a doc `/** app_status as read when this load started (recovery spec §6.3); the Recovery page and the crash notice read it. */`) and to the `useMemo`. In the JSX, between `<div lang={language}>` and the route switch:

```tsx
            <WorkingBar />
            {busyWith !== null && (
              <p role="status" style={{ padding: "2rem" }}>
                {t("start.busy")}
              </p>
            )}
            {busyWith === null && route === null ? ( … the existing "…" status … ) : …}
```

`App` itself has no `t` (it renders the `I18nProvider`): the busy line is this component, rendered inside the provider in place of the `<p role="status">` above:

```tsx
/** Recovery spec §6.3: a change of this process still runs; the start waits for it. */
function StartWait() {
  const { t } = useI18n();
  return (
    <p role="status" style={{ padding: "2rem" }}>
      {t("start.busy")}
    </p>
  );
}
```

so the JSX reads `<WorkingBar />{busyWith !== null ? <StartWait /> : route === null ? (…the existing "…" status…) : (<Screens … />)}`. Import `WorkingBar` from `./components/WorkingBar`.

`en.ts`: `"start.busy": "A theme is still being applied. CMD Studio waits for it to finish…"`; `th.ts`: `"start.busy": "กำลังใช้ธีมอยู่ CMD Studio รอให้เสร็จก่อน…"`.

`mock.ts`: `MockState` gains `busy: string | null` (from `options.busy ?? null`), and `appStatus` answers `busy: state.busy`.

- [ ] **Step 5: Run the UI checks**

`npm run typecheck`; `npm test` → `Tests 279 passed` (274 + 2 start + 3 App); `TZ=UTC npm test`; `npm run build`; `bash scripts/check-dist.sh`.

- [ ] **Step 6: Commit**

```bash
git add -- app/ui/src/start.tsx app/ui/src/start.test.tsx app/ui/src/App.tsx app/ui/src/App.test.tsx app/ui/src/state.tsx app/ui/src/ipc/mock.ts app/ui/src/i18n/en.ts app/ui/src/i18n/th.ts
```

Message: `ui: the start asks app_status first and waits for a running change, the window's own errors show the E_INTERNAL notice and go to the log, and the first screen says ready (recovery spec §5.1, §6.3, §6.5)`.

---

### Task 10: The Recovery page, the crash notice, the Report tab's crash note, the guide's shortcut sentence, and m4's words

Recovery spec §5.2 (the one page of safe mode), §5.3 (the notice in `StartNotices`, two clicks to the report), §4.4's sentence in the guide, §9 (both languages; the heading takes focus; the button sweep), and 3b-1's m4 in the UI.

**Files:**
- Create: `app/ui/src/screens/Recovery.tsx`
- Modify: `app/ui/src/router.tsx` (`{ name: "recovery" }`), `app/ui/src/useRouteFocus.ts` (if `screenOf` lists names)
- Modify: `app/ui/src/App.tsx` (the route from `appStatus.safeMode`; `StartNotices`' crash notice; `Screens` for `recovery`)
- Modify: `app/ui/src/screens/Help.tsx` (`Doctor`, `RestorePointsTab`, `Revert`, `Report` exported; the crash note on `Report`)
- Modify: `app/ui/src/components/Pause.tsx` (`notKnown`)
- Modify: `app/ui/src/i18n/en.ts`, `th.ts`, `i18n.test.ts` (the placeholder exemption)
- Modify: `app/ui/src/App.test.tsx`, `app/ui/src/screens/Help.test.tsx`

**Interfaces:**
- Consumes: `AppState.appStatus` (Task 9), `Ipc.{settingsReset, appRestart, openExternal, crashDismiss, crashReportRead, recoveryStatus}`, `PausedBanner`, `PauseResult`, `useResume` (`components/Pause.tsx`), `NotesList`, `UnrestoredList`, `Notice`, `ErrorNotice`, `Dialog`, `useRead` (Home's; move it to `app/ui/src/useRead.ts` and export it, with Home importing from there).
- Produces: `Recovery` (`screens/Recovery.tsx`), the exported Help parts, the keys below.

- [ ] **Step 1: The failing tests**

`app/ui/src/App.test.tsx`, a new `describe("recovery mode (recovery spec §5.2)")`:

```tsx
describe("recovery mode (recovery spec §5.2)", () => {
  const entryPaused = {
    notes: [],
    unrestored: [],
    pause: { pausedNow: true, revert: { ok: true, steps: [], notes: [], status: "committed", error: null, unrestored: [] }, refused: null, notSaved: null },
    skipped: null,
  };

  it("opens on the Recovery page after two failed starts, says why, and offers every way out", async () => {
    const { ipc, user } = await mount({ safeMode: { kind: "failedStarts", count: 2 }, settings: { wizardDone: true } });
    const heading = await screen.findByRole("heading", { name: "Recovery mode" });
    expect(heading).toHaveAttribute("tabindex", "-1");
    expect(screen.getByText("CMD Studio failed to start 2 times in a row, so it opened in recovery mode. Nothing of your themes was loaded.")).toBeTruthy();
    expect(screen.queryByRole("navigation")).toBeNull();
    for (const name of ["Undo everything", "Check now", "Open the log folder", "Copy details", "Reset the app's settings", "Start normally"]) {
      expect(screen.getAllByRole("button", { name }).length, name).toBeGreaterThan(0);
    }
    expect(screen.getByRole("heading", { name: "Restore points" })).toBeTruthy();
    expect(ipc.state.calls).not.toContain("themesList");
    await user.click(screen.getByRole("button", { name: "Open the log folder" }));
    await waitFor(() => expect(ipc.state.calls).toContain("openExternal"));
    await user.click(screen.getByRole("button", { name: "Start normally" }));
    await waitFor(() => expect(ipc.state.restarted).toBe(true));
  });

  it("from the Recovery shortcut, says what the entry did and lets the theme be turned back on", async () => {
    const { user } = await mount(
      { safeMode: { kind: "recovery" }, recoveryEntry: entryPaused, paused: { by: "app", level: "computer", reason: "recovery", atMs: 1791694709000, count: 0, resume: { themeId: presetId("Terracotta"), target: "all", themeName: "Terracotta" } } },
    );
    await screen.findByRole("heading", { name: "Recovery mode" });
    expect(screen.getByText("CMD Studio was opened from the CMD Studio Recovery shortcut.")).toBeTruthy();
    expect(screen.getByText("When it opened, CMD Studio put back any change that was cut off and paused the theme on this computer:")).toBeTruthy();
    expect(screen.getByText("The theme is paused: cmd windows open without it.")).toBeTruthy();
    expect(screen.getByText("The theme was paused from the CMD Studio Recovery shortcut.")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Turn the theme back on" }));
    expect(await screen.findByRole("dialog", { name: "Turn the theme back on" })).toBeTruthy();
  });

  it("says why no pause was made, and resets the settings after asking", async () => {
    const { ipc, user } = await mount({ safeMode: { kind: "recovery" }, recoveryEntry: { notes: [], unrestored: [], pause: null, skipped: "no theme is applied" }, settings: { language: "th" } });
    await screen.findByRole("heading", { name: "โหมดกู้คืน" });
    expect(screen.getByText("ไม่ได้หยุดธีม:", { exact: false })).toHaveTextContent("no theme is applied");
    await user.click(screen.getByRole("button", { name: "รีเซ็ตการตั้งค่าของแอป" }));
    const dialog = await screen.findByRole("dialog", { name: "รีเซ็ตการตั้งค่าของแอป" });
    expect(dialog).toHaveTextContent("ภาษา ที่ที่ธีมไป และตัวเลือกอื่นของแอปกลับเป็นค่าตั้งต้น ไฟล์เดิมเก็บไว้ให้ดู ธีมของคุณไม่หาย");
    await user.click(within(dialog).getByRole("button", { name: "รีเซ็ตการตั้งค่าของแอป" }));
    await waitFor(() => expect(ipc.state.calls).toContain("settingsReset"));
    // The defaults stand: English from now on.
    expect(await screen.findByText(/^The settings were reset\. The old file is kept as settings\.json\.reset-\d+\.$/)).toBeTruthy();
  });

  it("shows the crash notice on the home page and reaches the report with the crash note in two clicks (recovery spec §1.4)", async () => {
    const { ipc, user } = await home({
      crashNotice: { atMs: Date.UTC(2026, 9, 11, 4, 58), hasReport: true },
      crashReport: { file: "crash-1791694709000.txt", atMs: 1791694709000, text: "CMD Studio 0.1.0 crashed at 2026-10-11T04:58:29Z\nmessage: boom\n" },
    });
    const notice = screen.getByRole("alert");
    expect(notice).toHaveTextContent(/CMD Studio closed unexpectedly at .*2026/);
    const clicks = counting(user);
    await clicks.click(within(notice).getByRole("button", { name: "Report a problem" }));
    expect(await screen.findByRole("heading", { name: "Report a problem" })).toBeTruthy();
    expect(await screen.findByText("Crash note crash-1791694709000.txt")).toBeTruthy();
    expect(screen.getByText(/message: boom/)).toBeTruthy();
    expect(clicks.count()).toBe(1);
    await user.click(screen.getByRole("button", { name: "Home" }));
    await user.click(within(screen.getByRole("alert")).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(ipc.state.calls).toContain("crashDismiss"));
    expect(screen.queryByText(/closed unexpectedly/)).toBeNull();
  });

  it("a notice without a report names the start it had", async () => {
    await home({ crashNotice: { atMs: Date.UTC(2026, 9, 11, 4, 58), hasReport: false } });
    expect(screen.getByRole("alert")).toHaveTextContent(/CMD Studio closed unexpectedly after starting at .*2026/);
  });

  it("turning back on when state.json cannot be read says so (3b-1 m4)", async () => {
    const { user } = await home({}, pausedWith({ by: "app", level: "computerUnknown", reason: "user", atMs: 1791694709000, count: 0, resume: null }));
    await user.click(await screen.findByRole("button", { name: "Turn the theme back on" }));
    expect(await screen.findByText("Whether the theme is on this computer could not be read (state.json). Fix state.json first: the health check says what is wrong with it.")).toBeTruthy();
  });
});
```

(`pausedWith` and `counting` are helpers of the existing describes; move `pausedWith` to the file's top so both describes use it.) In `describe("accessibility")`, where the button sweep mounts each page, add the Recovery page (`mount({ safeMode: { kind: "recovery" }, recoveryEntry: entryPaused, paused: … })`) to the pages swept in both languages, as Home and Help are.

`app/ui/src/screens/Help.test.tsx`: in its guide test (search `cmdWontOpen` or "If cmd won't open"), the expected text becomes the new one (Step 4).

- [ ] **Step 2: Run them to see them fail**

`npm test -- App` → fails (no "Recovery mode" heading, no crash notice).

- [ ] **Step 3: The route, the page, the notice**

`router.tsx`: `| { name: "recovery" }` in `Route`. `useRouteFocus.ts`: read `screenOf`; if it maps names, add `recovery`.

Move `useRead` out of `Home.tsx` into `app/ui/src/useRead.ts` (the function as it is, exported; `Home` imports it; `readRecovery` stays module-level in each screen that uses it).

`App.tsx`: when `status?.safeMode` is true, the route is `{ name: "recovery" }` whatever `wizardDone` says (`readSettings`' success branch: `setRoute(s.safeMode ? { name: "recovery" } : loaded.settings.wizardDone ? … )`; the failure branch the same with `safeMode`). In `Screens`, before the wizard case:

```tsx
  if (route.name === "recovery") {
    return (
      <main id="content" className="recovery" ref={main}>
        <StartNotices {...problems} />
        <Recovery />
      </main>
    );
  }
```

`StartNotices` gains the crash notice (recovery spec §5.3), from `useAppState().appStatus?.crashNotice`, hidden once closed:

```tsx
function CrashClosed({ notice }: { notice: CrashNotice }) {
  const { t, language } = useI18n();
  const ipc = useIpc();
  const { go } = useNav();
  const [closed, setClosed] = useState(false);
  if (closed) return null;
  const time = formatTime(notice.atMs, language);
  return (
    <Notice kind="warn">
      <p>{notice.hasReport ? t("start.crashed", { time }) : t("start.crashedNoReport", { time })}</p>
      <div className="row-actions">
        <button type="button" className="primary" onClick={() => go({ name: "help", tab: "report" })}>
          {t("home.paused.report")}
        </button>
        <button
          type="button"
          onClick={() => {
            setClosed(true);
            ipc.crashDismiss().catch(() => {});
          }}
        >
          {t("common.close")}
        </button>
      </div>
    </Notice>
  );
}
```

rendered first in `StartNotices` when `appStatus?.crashNotice` is set (`Notice kind="warn"` has `role="alert"`, as the test expects).

`app/ui/src/screens/Recovery.tsx`:

```tsx
// Recovery mode's one page (recovery spec §5.2): why the app is here, what the --recovery entry
// did (§4.4), the pause's state with "Turn the theme back on", and the ways out, each a part Help
// already has (Undo everything, Restore points, Health check, Report a problem), plus the log
// folder, "Reset the app's settings" (settings.json set aside) and "Start normally" (a restart:
// the first screen of this run said ready, so the next start is a normal one). No library, no
// drafts, no preview, no quick switch are read here. A line is kept for phase 4's "Update now".
import { useRef, useState } from "react";
import { Dialog } from "../components/Dialog";
import { ErrorNotice, Notice, NotesList, UnrestoredList } from "../components/Notices";
import { PausedBanner, PauseResult, useResume } from "../components/Pause";
import { useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import type { Ipc } from "../ipc/types";
import { useAppState } from "../state";
import { useRead } from "../useRead";
import { Doctor, Report, RestorePointsTab, Revert } from "./Help";

const readRecovery = (ipc: Ipc) => ipc.recoveryStatus();

export function Recovery() {
  const { t } = useI18n();
  const ipc = useIpc();
  const { appStatus, version, bump } = useAppState();
  const heading = useRef<HTMLHeadingElement>(null);
  const recovery = useRead(readRecovery, version);
  const paused = recovery.value?.paused ?? null;
  const resume = useResume(heading, paused);
  const [asking, setAsking] = useState<"reset" | null>(null);
  const [resetTo, setResetTo] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const reason = appStatus?.safeModeReason ?? null;
  const entry = appStatus?.recoveryEntry ?? null;

  const act = async (run: () => Promise<void>) => {
    setError(null);
    try {
      await run();
    } catch (e) {
      setError(e);
    }
  };
  const reset = () =>
    act(async () => {
      const done = await ipc.settingsReset();
      setAsking(null);
      setResetTo(done.resetFrom);
      // The defaults stand: the language too.
      window.location.reload();
    });

  return (
    <section>
      <h1 tabIndex={-1} ref={heading}>
        {t("recovery.title")}
      </h1>
      <p>
        {reason?.kind === "failedStarts" && t("recovery.why.failedStarts", { n: reason.count })}
        {reason?.kind === "recovery" && t("recovery.why.shortcut")}
      </p>
      {/* Phase 4's "Update now" goes here (recovery spec §5.2). */}
      <p className="update-line">{t("recovery.update.placeholder")}</p>
      {entry && (
        <div className="card">
          <p>{t("recovery.entry.did")}</p>
          <NotesList notes={entry.notes} />
          <UnrestoredList steps={entry.unrestored} live={false} />
          {entry.pause && <PauseResult outcome={entry.pause} />}
          {entry.skipped && (
            <p>
              {t("recovery.entry.skipped")} <span lang="en">{entry.skipped}</span>
            </p>
          )}
        </div>
      )}
      {recovery.error !== null && <ErrorNotice error={recovery.error} title={t("home.paused.unreadable")} />}
      {paused && <PausedBanner paused={paused} onResume={() => void resume.resume()} />}
      {resume.view}
      {error !== null && <ErrorNotice error={error} />}
      <Revert />
      <RestorePointsTab />
      <Doctor />
      <h2>{t("recovery.logs.title")}</h2>
      <div className="row-actions">
        <button type="button" onClick={() => void act(() => ipc.openExternal("logsFolder"))}>
          {t("advanced.openLogs")}
        </button>
      </div>
      <Report />
      <h2>{t("recovery.reset.title")}</h2>
      <p>{t("recovery.reset.text")}</p>
      {resetTo && <Notice kind="ok">{t("recovery.reset.done", { file: resetTo })}</Notice>}
      <div className="row-actions">
        <button type="button" onClick={() => setAsking("reset")}>
          {t("recovery.reset.button")}
        </button>
      </div>
      <h2>{t("recovery.normal.title")}</h2>
      <p>{t("recovery.normal.text")}</p>
      <div className="row-actions">
        <button type="button" className="primary" onClick={() => void act(() => ipc.appRestart())}>
          {t("recovery.normal.button")}
        </button>
      </div>
      {asking === "reset" && (
        <Dialog title={t("recovery.reset.button")} onClose={() => setAsking(null)}>
          <p>{t("recovery.reset.confirm")}</p>
          <div className="row-actions">
            <button type="button" className="primary" onClick={() => void reset()}>
              {t("recovery.reset.button")}
            </button>
            <button type="button" onClick={() => setAsking(null)}>
              {t("common.cancel")}
            </button>
          </div>
        </Dialog>
      )}
    </section>
  );
}
```

(`window.location.reload()` after the reset: in jsdom it is a no-op that logs "not implemented"; the test asserts the `recovery.reset.done` text before the reload would land. `ExternalTarget`'s value for the log folder: read `app/ui/src/bindings/ExternalTarget.ts` and use the exact string, `"logsFolder"`.) `Help.tsx`: `export function Doctor()`, `export function RestorePointsTab()`, `export function Revert()`, `export function Report()`.

`Report` (in `Help.tsx`) gains the crash note under the diagnostics text:

```tsx
  const [note, setNote] = useState<CrashReport | null>(null);
  useEffect(() => {
    let alive = true;
    ipc.crashReportRead().then(
      (n) => {
        if (alive) setNote(n);
      },
      () => {},
    );
    return () => {
      alive = false;
    };
  }, [ipc]);
  …
      {note && (
        <>
          <h3>{t("help.report.crashNote", { file: note.file })}</h3>
          <pre className="log" lang="en">
            {note.text}
          </pre>
        </>
      )}
```

(The diagnostics text the Copy copies already ends with the note when it belongs to this run or the crashed one, Task 5; the page shows the note on its own so the user sees it either way.)

`Pause.tsx`, `useResume`: `resumed` gains `"notKnown"`; in `resume()`: `else if (outcome.kind === "notKnown") { setResumed("notKnown"); }` before the `else { bump(); }`; the status line: `t(resumed === "noteRemoved" ? "resume.noteRemoved" : resumed === "notKnown" ? "resume.notKnown" : "resume.done")`.

- [ ] **Step 4: The words**

`en.ts`:

```ts
  "start.crashed": "CMD Studio closed unexpectedly at {time}. A crash note was kept for the problem report.",
  "start.crashedNoReport": "CMD Studio closed unexpectedly after starting at {time}.",
  "recovery.title": "Recovery mode",
  "recovery.why.failedStarts": "CMD Studio failed to start {n} times in a row, so it opened in recovery mode. Nothing of your themes was loaded.",
  "recovery.why.shortcut": "CMD Studio was opened from the CMD Studio Recovery shortcut.",
  "recovery.update.placeholder": "",
  "recovery.entry.did": "When it opened, CMD Studio put back any change that was cut off and paused the theme on this computer:",
  "recovery.entry.skipped": "The theme was not paused:",
  "recovery.logs.title": "The log",
  "recovery.reset.title": "The app's settings",
  "recovery.reset.text": "If CMD Studio fails because of its own settings, reset them: the language, where themes go and the other choices go back to their defaults. The old file is kept.",
  "recovery.reset.button": "Reset the app's settings",
  "recovery.reset.confirm": "The language, where themes go and the app's other choices go back to their defaults. The old file is kept for a look. Your themes stay.",
  "recovery.reset.done": "The settings were reset. The old file is kept as {file}.",
  "recovery.normal.title": "Leave recovery mode",
  "recovery.normal.text": "Starts CMD Studio again the normal way, with your themes.",
  "recovery.normal.button": "Start normally",
  "resume.notKnown": "Whether the theme is on this computer could not be read (state.json). Fix state.json first: the health check says what is wrong with it.",
  "help.report.crashNote": "Crash note {file}",
```

`help.guide.cmdWontOpen` becomes: `"If cmd won't open: open CMD Studio, or choose CMD Studio Recovery in the Start menu. When cmd keeps failing to start because of the theme, CMD Studio pauses the theme by itself, and you can also press Pause the theme on the home page."`

`th.ts`:

```ts
  "start.crashed": "CMD Studio ปิดไปแบบไม่ปกติเมื่อ {time} มีบันทึกแครชเก็บไว้สำหรับการแจ้งปัญหา",
  "start.crashedNoReport": "CMD Studio ปิดไปแบบไม่ปกติ หลังจากเปิดเมื่อ {time}",
  "recovery.title": "โหมดกู้คืน",
  "recovery.why.failedStarts": "CMD Studio เปิดไม่สำเร็จ {n} ครั้งติดกัน จึงเปิดในโหมดกู้คืน ยังไม่ได้โหลดธีมของคุณ",
  "recovery.why.shortcut": "CMD Studio ถูกเปิดจากทางลัด CMD Studio Recovery",
  "recovery.update.placeholder": "",
  "recovery.entry.did": "ตอนเปิด CMD Studio ได้ย้อนการเปลี่ยนแปลงที่ค้างอยู่ และหยุดธีมของเครื่องนี้:",
  "recovery.entry.skipped": "ไม่ได้หยุดธีม:",
  "recovery.logs.title": "บันทึกการทำงาน",
  "recovery.reset.title": "การตั้งค่าของแอป",
  "recovery.reset.text": "ถ้า CMD Studio มีปัญหาเพราะการตั้งค่าของตัวเอง ให้รีเซ็ต: ภาษา ที่ที่ธีมไป และตัวเลือกอื่นกลับเป็นค่าตั้งต้น ไฟล์เดิมเก็บไว้",
  "recovery.reset.button": "รีเซ็ตการตั้งค่าของแอป",
  "recovery.reset.confirm": "ภาษา ที่ที่ธีมไป และตัวเลือกอื่นของแอปกลับเป็นค่าตั้งต้น ไฟล์เดิมเก็บไว้ให้ดู ธีมของคุณไม่หาย",
  "recovery.reset.done": "รีเซ็ตการตั้งค่าแล้ว ไฟล์เดิมเก็บไว้เป็น {file}",
  "recovery.normal.title": "ออกจากโหมดกู้คืน",
  "recovery.normal.text": "เริ่ม CMD Studio ใหม่แบบปกติ พร้อมธีมของคุณ",
  "recovery.normal.button": "เริ่มแบบปกติ",
  "resume.notKnown": "อ่านไม่ได้ว่าธีมอยู่บนเครื่องนี้หรือไม่ (state.json) ให้แก้ state.json ก่อน การตรวจสุขภาพบอกว่าไฟล์นี้ผิดตรงไหน",
  "help.report.crashNote": "บันทึกแครช {file}",
```

and `help.guide.cmdWontOpen`: `'ถ้า cmd เปิดไม่ขึ้น: เปิด CMD Studio หรือเลือก CMD Studio Recovery ในเมนู Start เมื่อ cmd เปิดไม่ขึ้นซ้ำๆ เพราะธีม CMD Studio จะหยุดธีมให้เอง และกด "หยุดธีม" ที่หน้าแรกเองก็ได้'`.

`i18n.test.ts`: the empty-text test exempts `home.update.placeholder`; add `recovery.update.placeholder` to that exemption and to `SAME_IN_BOTH`.

- [ ] **Step 5: Run the UI checks**

`npm run typecheck`; `npm test` → `Tests 285 passed` (279 + 6); `TZ=UTC npm test`; `npm run build`; `bash scripts/check-dist.sh`.

- [ ] **Step 6: Commit**

```bash
git add -- app/ui/src/screens/Recovery.tsx app/ui/src/useRead.ts app/ui/src/screens/Home.tsx app/ui/src/screens/Help.tsx app/ui/src/screens/Help.test.tsx app/ui/src/components/Pause.tsx app/ui/src/router.tsx app/ui/src/useRouteFocus.ts app/ui/src/App.tsx app/ui/src/App.test.tsx app/ui/src/i18n/en.ts app/ui/src/i18n/th.ts app/ui/src/i18n/i18n.test.ts
```

Message: `ui: the Recovery page, the crash notice two clicks from the report, the crash note on the Report tab, the guide names the Recovery shortcut, and turning back on says when state.json cannot be read (recovery spec §4.4, §5.2, §5.3; 3b-1 m4)`.

---

### Task 11: The main spec records phase 3b-2

The main spec says what shipped: §5.3's new files and log lines, §5.7's seven commands and `--recovery`, §6.1's shortcut (phase 4's job), and appendix B's row. §5.9's `E_TIMEOUT` was Task 3's; appendix B's tasklist sentence Task 2's. The recovery spec is the founder's and is not edited. Write in Thai, as the rest of the spec; code names, codes and file names in English. Check every sentence against the shipped code of Tasks 2–10.

**Files:**
- Modify: `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§5.3, §5.7, §6.1, appendix B)

**Interfaces:**
- Consumes: the shipped code: `backend/run.rs` (the log lines `run start safe=… failed_starts=…`, `run record=unreadable|damaged|other-version|still-running|unwritable`, `run end`, `ui_error place=… …`), `lib.rs` (`recovery_entry not_run`, `renderer watch=not_registered …`), `command_names.rs`, `main.rs`, `crash.rs`.
- Produces: the spec text below.

- [ ] **Step 1: §5.3**

In the tree of `<data>\`, after the `├─ health\` line add:

```
├─ run.json            บันทึกการเปิดแอปครั้งนี้ (recovery spec §5.1): pid, เวลาสร้างโปรเซส (GetProcessTimes), เวลาเริ่ม, เวลาที่หน้าแรกวาดเสร็จ, จำนวนครั้งที่เปิดไม่สำเร็จติดกัน, อยู่ในโหมดกู้คืนหรือไม่ · เขียนก่อนสร้างหน้าต่าง ลบเมื่อปิดปกติ (เฉพาะบันทึกของโปรเซสตัวเอง) · ถ้าพบของครั้งก่อนที่โปรเซสตายแล้วและไม่เคยถึงหน้าแรก นับเป็นเปิดไม่สำเร็จ ครบ 2 ครั้งเปิดเป็นโหมดกู้คืน ถ้าเคยถึงหน้าแรก แสดงประกาศว่าปิดไปแบบไม่ปกติ · pid ซ้ำได้ จึงเทียบเวลาสร้างโปรเซสด้วย (phase 3b-2)
├─ settings.json.reset-<ms>  settings.json ที่ "รีเซ็ตการตั้งค่าของแอป" ในโหมดกู้คืนย้ายไปเก็บ (phase 3b-2)
```

and in the `logs\` line, after the three kinds of the app's own lines, add: `· บรรทัดของ phase 3b-2: การเปิดแอป (\`run start safe=<bool> failed_starts=<n>\`, \`run record=unreadable|damaged|other-version|still-running|unwritable\`, \`run end\`), ข้อผิดพลาดฝั่ง UI (\`ui_error place=<ชื่อหน้า> <ข้อความที่ลบชื่อผู้ใช้แล้ว>\` ไม่เกิน 20 บรรทัดต่อการเปิดหนึ่งครั้ง) และของ --recovery กับ WebView2 (\`recovery_entry not_run\`, \`renderer watch=not_registered <เหตุผล>\`) · \`logs\\crash-<unix ms>.txt\` คือบันทึกแครชจาก panic hook (recovery spec §5.3): เวอร์ชัน เวลา ชื่อ thread ข้อความ ตำแหน่ง และ backtrace ทุกบรรทัดผ่าน \`redact\` และ \`redact_sids\` เก็บ 10 ไฟล์ล่าสุด hook ไม่ panic ซ้ำและไม่ล้มเพราะเขียนไม่ได้ ตั้งทั้งแอปและ CLI ที่โฟลเดอร์ข้อมูลตั้งต้น (phase 3b-2)`.

- [ ] **Step 2: §5.7**

After the `recovery_status, theme_pause, theme_resume` bullet, add:

```markdown
- `app_ready`, `app_status`, `crash_dismiss`, `crash_report_read`, `settings_reset`, `log_ui_error`, `app_restart` (recovery spec §8, phase 3b-2): `app_ready` บันทึก `uiReadyAt` ใน `run.json` และตั้ง `failedStarts` เป็น 0 · `app_status` คืน `{ busy, safeMode, safeModeReason, crashNotice, recoveryEntry }` โดย `busy` คือชื่อคำสั่งที่เปลี่ยนเครื่องซึ่งกำลังทำอยู่ (`apply`, `undo_last`, `snapshot_restore`, `revert_all`, `theme_pause`) `safeModeReason` คือ `failedStarts {count}` หรือ `recovery` `crashNotice` คือ `{ atMs, hasReport }` (เวลาของบันทึกแครชถ้ามี ไม่มีก็เวลาที่ครั้งก่อนเริ่ม) และ `recoveryEntry` คือผลของขั้น 1–2 ของ `--recovery` (`notes`, `unrestored`, `pause`, `skipped`) UI ถาม `app_status` ก่อนทุกครั้งที่โหลดหน้า และถามซ้ำทุก 2 วินาทีเฉพาะระหว่างที่ `busy` ไม่เป็น null · `crash_dismiss` จดว่าปิดประกาศแล้วสำหรับการเปิดครั้งนี้ (ไม่มีไฟล์ เพราะประกาศมาจาก `run.json` ของครั้งก่อนซึ่งถูกเขียนทับแล้ว) · `crash_report_read` คืนบันทึกแครชล่าสุดที่ลบชื่อแล้ว หรือ null · `settings_reset` ย้าย `settings.json` ไปเป็น `settings.json.reset-<ms>` แล้วใช้ค่าตั้งต้น · `log_ui_error(message, place)` เขียน log หนึ่งบรรทัดผ่าน `redact` ไม่เกิน 20 ครั้งต่อการเปิดหนึ่งครั้ง · `app_restart` เริ่ม `cmdstudio.exe` ใหม่โดยไม่มี argument แล้วจบโปรเซสนี้ ทรานแซกชันที่ค้างถูกย้อนด้วย journal ที่คำสั่งถัดไป (§5.5) · `theme_resume` เพิ่มผล `notKnown` เมื่อ state.json อ่านไม่ได้ (ไม่ทำอะไร ให้แก้ state.json ก่อน)
```

In the paragraph after the list, `ไฟล์ capability อนุญาตเฉพาะ 30 คำสั่งนี้` becomes `37 คำสั่งนี้`, and the sentence about events stays (`apply-progress` is still the only event: `--recovery` while the app runs reloads the page instead).

In the CLI table, add after the `--cleanup` row:

```markdown
| `cmdstudio --recovery` | เปิดแอปในโหมดกู้คืน ไม่ใช่ CLI (recovery spec §4.4): ย้อนทรานแซกชันที่ค้างใต้ล็อก แล้วถ้ามีธีมใช้อยู่และยังไม่ได้หยุดระดับเครื่อง หยุดธีมระดับเครื่อง (`reason: recovery`) แล้วแสดงหน้าโหมดกู้คืน ถ้าแอปเปิดอยู่แล้ว ตัวที่เปิดอยู่ทำสองขั้นนั้นแล้วโหลดหน้าใหม่เป็นโหมดกู้คืน · ทางลัด "CMD Studio Recovery" ของตัวติดตั้ง (§6.1, phase 4) เรียกคำสั่งนี้ · argument อื่นทุกตัวยังเป็น CLI (phase 3b-2) |
```

Also in that paragraph's `main.rs` sentence (if there is one about "any argument is the CLI"), say `--recovery` alone is the window.

- [ ] **Step 3: §6.1 (the installer, phase 4's job)**

Read §6.1 and add one bullet at its end:

```markdown
- **ทางลัด "CMD Studio Recovery" (phase 4 ทำ ตาม recovery spec §4.4):** ตัวติดตั้งสร้างในเมนู Start คู่กับทางลัดของแอป เรียก `cmdstudio.exe --recovery` (ไม่มี argument อื่น) ตัวถอนการติดตั้งลบทั้งคู่ · แอปรองรับ `--recovery` แล้วตั้งแต่ phase 3b-2 (§5.7)
```

- [ ] **Step 4: Appendix B**

Add a row after phase 3b-1's:

```markdown
| 2026-11-03 | Phase 3b-2 โหมดกู้คืนฝั่งแอป (architect เลือก founder เปลี่ยนได้ แผนอยู่ใน `docs/superpowers/plans/2026-11-03-phase3b-2-recovery-app.md`):<br>• `run.json` เขียนก่อนสร้างหน้าต่าง เทียบ pid กับเวลาสร้างโปรเซสเสมอ (pid ซ้ำได้) เปิดไม่สำเร็จ 2 ครั้งติดเข้าโหมดกู้คืน ปิดปกติลบเฉพาะบันทึกของโปรเซสตัวเอง (หลัง `app_restart` โปรเซสใหม่เขียนบันทึกของตัวเองก่อนโปรเซสเก่าจบ)<br>• `--recovery` ขณะแอปเปิดอยู่: callback ของ single-instance ทำขั้น 1–2 บน thread แล้ว `window.location.reload()` ไม่มี event ใหม่ (§5.7 ยังมีแค่ `apply-progress`) UI ที่โหลดใหม่ถาม `app_status` ก่อนตามกฎของ recovery spec §6.3<br>• "เริ่มแบบปกติ" คือ `app_restart` เพราะ `app_ready` ในโหมดกู้คืนตั้ง `failedStarts` เป็น 0 แล้ว การเริ่มใหม่โดยไม่มี argument จึงเป็นการเริ่มปกติ ไม่ต้องมี IPC ออกจากโหมดกู้คืน<br>• ประกาศแครช: เวลาคือเวลาของบันทึกแครชถ้ามีไฟล์ใหม่กว่าการเปิดครั้งที่ตาย ไม่มีก็เวลาที่ครั้งนั้นเริ่ม "ปิด" จำไว้แค่การเปิดครั้งนี้<br>• panic hook ตั้งใน `main.rs` ก่อน parse argument ทั้งสองโหมด ที่โฟลเดอร์ข้อมูลตั้งต้น (`--data-dir` ไม่มีผลกับบันทึกแครช)<br>• เส้นตาย 60 วินาทีของ UI ยกเว้น `theme_import_preview`, `theme_export` (รอผู้ใช้ในกล่องเลือกไฟล์) และ `app_restart` (ไม่ตอบ) รายการชนิดของคำสั่งอยู่ที่เดียวใน `app/ui/src/ipc/work.ts` มีเทสต์ตรวจว่าครบทุกเมธอดของ `Ipc` · แถบ "ยังทำงานอยู่" ใช้ชื่องาน 14 กลุ่มจากพจนานุกรม<br>• คำสั่งที่นับว่าเปลี่ยนเครื่องสำหรับ `busy` และปุ่ม "เริ่ม CMD Studio ใหม่": `apply`, `undo_last`, `snapshot_restore`, `revert_all`, `theme_pause` (`command_names::CHANGES_MACHINE`)<br>• กล่อง Yes/No ของ §5.4: Yes เรียก `Backend::revert_all` ใต้ล็อก แล้วกล่องที่สองบอกผล (สำเร็จ / ไม่ครบ ดู log / `E_BUSY` / รหัสอื่น) `start_failed` ยังเป็นกล่องเดิม<br>• WebView2 `ProcessFailed` (§6.4): <ทำแล้วผ่าน `with_webview` → `ICoreWebView2::add_ProcessFailed` นับเฉพาะ renderer ตายหรือไม่ตอบ โหลดหน้าใหม่ด้วย `Reload()` ครั้งที่ 3 ใน 1 นาทีใช้กล่อง Yes/No แทน | หรือ: ไม่ได้ทำใน phase นี้ (F1): <เหตุผลจาก spike S1/S2>> ผล spike อยู่ใน `docs/spikes/phase3b-recovery/results-3b2.md`<br>• ตัวนับใน init.lua เลิกตรวจ tasklist (founder 2026-10-19) หน้าต่างที่ 4 หยุดธีมแม้สามหน้าต่างที่ค้างยังเปิดอยู่<br>• m4 ของ 3b-1: `theme_resume` ที่ `computerUnknown` คืน `notKnown` แทน error ของ planner<br>• ไม่มีตัวแปรทดสอบ "แครชตอนเปิด" ใน build: VM check ใช้ไฟล์ `run.json` จำลอง (pid ที่ตายแล้ว, `uiReadyAt: null`) และ `Stop-Process` ขณะหน้าต่างเปิดอยู่ ส่วน panic hook พิสูจน์ด้วยเทสต์ (หนึ่งเทสต์เขียนดิสก์จริงในโฟลเดอร์ชั่วคราว) |
```

Fill the `<…>` of the `ProcessFailed` bullet from Task 6's outcome (one of the two texts), and nothing else is left in angle brackets.

- [ ] **Step 5: Read the diff, run the spec tests, commit**

`git diff docs/superpowers/specs/2026-10-10-cmd-studio-design.md` must touch only §5.3, §5.7, §6.1 and appendix B. `cargo test -p cmdstudio-core --test error_codes` and `cargo test -p cmdstudio-app --lib error::` still pass (the §5.9 table is unchanged here).

```bash
git add -- docs/superpowers/specs/2026-10-10-cmd-studio-design.md
```

Message: `spec: phase 3b-2 in §5.3, §5.7, §6.1 and appendix B (run.json, the crash notes, the seven commands, --recovery, the Recovery shortcut)`.

---

### Task 12: The VM check of phase 3b-2 (the protocol and the scripts here; the run is the controller's)

Recovery spec §10's VM list for 3b-2: the app crashes at start twice and the third start is recovery mode; the Recovery shortcut with the app closed and with it open; a crash while in use and the notice; the "still working" bar and the restart; the renderer's death (if §6.4 is in); the Thai VM; and the carries: the same-boot A/B timing with and without `HEALTH_BODY` (F4), the second refilter (F3), the tasklist step gone, the guard total. The hang stand-in and the fake cmd of 3b-1 are reused.

**PC safety, for this whole task:** every script here is for the VMs and is parse-checked only on this PC (Global Constraints). The controller runs the protocol on VM-A and VM-C against CI's artifact of this branch's head after the final review.

**Files:**
- Create: `docs/spikes/phase3b-recovery/vm-check-3b2.md`
- Create: `spikes/vm/guest/recovery-run.ps1`
- Modify: `spikes/vm/guest/recovery-timing.ps1` (`-WithoutCounter`)
- Modify: `spikes/vm/guest/recovery-filters.ps1` (`-Probe refilter` logs the refilter's caller)
- Delete: `spikes/vm/guest/recovery-tasklist.ps1`; Modify: `spikes/vm/guest/guard.test.ps1` (its entry goes; `recovery-run.ps1` comes)
- Modify: `docs/spikes/phase3b-recovery/vm-check.md` (step 2d marked "removed in 3b-2"; the guard total)

**Interfaces:**
- Consumes: `spikes/vm/vm.ps1` and `docs/spikes/phase3-vm-check/helpers.ps1` as 3b-1's `vm-check.md` "Setup" uses them (`$startApp`, `$closeApp`, `$closeCmd`, `$health`, `$appLog`, `$started`, `Invoke-GuestPs`, `Invoke-GuestDesktop`, `Invoke-GuestButton`, `Send-Run`, `Send-Key`, `Save-Shot`, `Get-GuestCmdWindows`); Task 1's `recovery-renderer.ps1`; 3b-1's `recovery-hang.ps1`, `recovery-fakecrash.ps1`, `recovery-timing.ps1`, `recovery-filters.ps1`; the words of `en.ts`/`th.ts` (Tasks 8–10), `backend/run.rs`'s log lines (Task 4), `console.rs`'s box texts (Task 6).
- Produces: `recovery-run.ps1 -FailedStarts <n> [-Data <dir>]` (writes a `run.json` of a dead pid), `recovery-run.ps1 -Kill` (ends `cmdstudio.exe` without CloseMainWindow), `recovery-run.ps1 -Show [-Data <dir>]` (prints `run.json`, the crash notes and the log's `run ` lines); `recovery-timing.ps1 -WithoutCounter` (the same timing over an `init.lua` with `HEALTH_BODY` cut out, restored afterwards); the protocol with PASS lines.

- [ ] **Step 1: `recovery-run.ps1`**

```powershell
# Runs inside a test VM (plan 3b-2 Task 12): the app's run record (recovery spec §5.1) without
# a test build. -FailedStarts n writes <data>\run.json as a run of a pid that is gone would have
# left it (uiReadyAt null, failedStarts n): the next start counts one more, and from 2 opens in
# recovery mode. -Kill ends cmdstudio.exe the way a crash would (no CloseMainWindow): the next
# start then shows the crash notice. -Show prints run.json, the crash notes in logs\ and the
# log's `run ` lines. The pid written is 4000000 - 4: Windows pids are multiples of 4 and far
# below it, so it is never a live process.
param([int] $FailedStarts = -1, [switch] $Kill, [switch] $Show, [string] $Data = (Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio'))
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script writes the app's data folder and kills processes" }
$run = Join-Path $Data 'run.json'
if ($FailedStarts -ge 0) {
  $ms = [int64]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
  $json = '{"version":1,"pid":3999996,"createdAt":' + ($ms - 600000) + ',"startedAt":' + ($ms - 500000) + ',"uiReadyAt":null,"failedStarts":' + $FailedStarts + ',"safeMode":false}'
  [IO.File]::WriteAllText($run, $json + "`n", (New-Object Text.UTF8Encoding $false))
  "wrote run.json: failedStarts=$FailedStarts pid=3999996 (gone)"
}
if ($Kill) {
  $p = @(Get-Process cmdstudio -ErrorAction SilentlyContinue)
  if (-not $p) { throw 'cmdstudio.exe is not running' }
  $p | ForEach-Object { "killing cmdstudio pid $($_.Id)"; Stop-Process -Id $_.Id -Force }
  Start-Sleep -Seconds 2
  "run.json after the kill: " + $(if (Test-Path -LiteralPath $run) { 'still there' } else { 'gone (not a crash)' })
}
if ($Show) {
  if (Test-Path -LiteralPath $run) { 'run.json = ' + (Get-Content -Raw -LiteralPath $run).Trim() } else { '(no run.json)' }
  $logs = Join-Path $Data 'logs'
  $notes = @(Get-ChildItem -LiteralPath $logs -Filter 'crash-*.txt' -ErrorAction SilentlyContinue | Sort-Object Name)
  "crash notes: $($notes.Count)"
  foreach ($n in $notes) { '--- ' + $n.Name; Get-Content -LiteralPath $n.FullName -TotalCount 6 -Encoding UTF8 }
  $log = Join-Path $logs 'cmdstudio.log'
  if (Test-Path -LiteralPath $log) { Select-String -LiteralPath $log -Pattern ' run |ui_error|recovery_entry|renderer watch|pause reason|cmd=app_' | ForEach-Object Line | Select-Object -Last 20 }
}
```

- [ ] **Step 2: `recovery-timing.ps1 -WithoutCounter` (carry 2, F4)**

Add `[switch] $WithoutCounter` to the `param(...)` line, and after the guard line:

```powershell
# Carry F4 of plan 3b-1: the same boot, the same data folder, the same marker, once with the
# shipped init.lua and once with the start counter (HEALTH_BODY) cut out of it. The cut is from
# the line that starts the counter to the line before Oh My Posh's part (the first line of
# INIT_BODY in crates\core\src\generate\lua.rs); the original is put back afterwards, whatever
# happens. The counter's cost per start is the difference of the two averages.
$initLua = Join-Path $Data 'generated\init.lua'
$backup = "$initLua.with-counter"
if ($WithoutCounter) {
  $lines = [IO.File]::ReadAllLines($initLua)
  $from = [Array]::FindIndex($lines, [Predicate[string]]{ param($l) $l.StartsWith('-- The start counter (recovery spec') })
  $to = [Array]::FindIndex($lines, [Predicate[string]]{ param($l) $l.StartsWith('-- cmd /c drops the outermost quotes') })
  if ($from -lt 0 -or $to -le $from) { throw "init.lua has no counter block to cut (from=$from to=$to)" }
  Copy-Item -LiteralPath $initLua -Destination $backup -Force
  $cut = $lines[0..($from - 1)] + $lines[$to..($lines.Length - 1)]
  [IO.File]::WriteAllLines($initLua, $cut, (New-Object Text.UTF8Encoding $false))
  "init.lua without the counter: $($lines.Length - $cut.Length) lines cut (from line $($from + 1) to $to)"
}
try {
```

and wrap the rest of the script's body (from `$tagsFound = 0` to its last line) in that `try { … } finally { if ($WithoutCounter -and (Test-Path -LiteralPath $backup)) { Move-Item -LiteralPath $backup -Destination $initLua -Force; 'init.lua restored' } }`. Update the header comment: the two runs are compared in the same boot (`-InstallMarker` once, then the script twice: with, then `-WithoutCounter`).

- [ ] **Step 3: `recovery-filters.ps1`, the refilter's caller (carry 3, F3)**

Read how `-Probe refilter -Install` writes `generated\aa-refilter.lua` (3b-1 Task 12). In the Lua it writes, where each `own n=<k>` line is logged, also log who asked for the refilter: wrap `clink.refilterprompt` once at load, so that each call logs `refilterprompt from <debug.traceback's second frame>` into the same file, and log the result of `clink.promptcoroutine`'s completion if the API exposes it (it does not; the traceback is the evidence). Add to the Lua, before the filter is registered:

```lua
local real_refilter = clink.refilterprompt
clink.refilterprompt = function(...)
  local who = debug.traceback("", 2):match("\n%s*([^\n]+)") or "?"
  log("refilterprompt from " .. who)
  return real_refilter(...)
end
```

(`log` is the file writer the probe already has; read its name.) Oh My Posh's own `clink.refilterprompt` call (its async/transient prompt) and Clink's internal refilter after a coroutine ends are then told apart: the internal one logs no `refilterprompt from` line before its `own n=` line.

- [ ] **Step 4: The tasklist script goes; the guard**

`git rm spikes/vm/guest/recovery-tasklist.ps1`; in `guard.test.ps1`'s `$changers` replace `'recovery-tasklist.ps1' = @();` with `'recovery-run.ps1' = @('-Show');` (keep `'recovery-renderer.ps1' = @()` from Task 1). The keys are then 3b-1's 19, minus `recovery-tasklist.ps1`, plus `recovery-renderer.ps1` and `recovery-run.ps1`: 20, so the script's last line is `guest guards: 43/43 checks pass` (`2 * 20 + 3`). Count the keys in the file and write that number into the protocol's PASS line.

In 3b-1's `docs/spikes/phase3b-recovery/vm-check.md`, step 2d's "Do" cell gets a first sentence: `**Removed in 3b-2** (the liveness check is gone; \`recovery-tasklist.ps1\` with it).`, and its guard sentence (`41/41`, the list of scripts) a note: `(3b-2: \`recovery-tasklist.ps1\` gone, \`recovery-renderer.ps1\` and \`recovery-run.ps1\` added: 43/43)`.

- [ ] **Step 5: Parse-check (this PC, never run)**

```powershell
foreach ($s in 'recovery-run.ps1', 'recovery-timing.ps1', 'recovery-filters.ps1', 'recovery-renderer.ps1', 'guard.test.ps1') {
  $t = $e = $null
  $ast = [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path "spikes\vm\guest\$s"), [ref]$t, [ref]$e)
  $first = ($ast.EndBlock.Statements | Select-Object -First 3 | ForEach-Object { $_.Extent.Text }) -join "`n"
  "$s errors=$($e.Count) guarded=$($first -match 'COMPUTERNAME -notlike ''CMDSTUDIO-\*''\) \{ throw')"
}
```

Expected: `errors=0` for all five; `guarded=True` for the four guest scripts, `False` for `guard.test.ps1`. The Lua of Step 3 is compile-checked with the project's mlua, as 3b-1's spike Lua was: a scratch test in `<scratchpad>/wt/crates/core/tests/` that `lua.load(text).into_function()` (never called) and is not committed.

- [ ] **Step 6: Write `docs/spikes/phase3b-recovery/vm-check-3b2.md`**

````markdown
# Phase 3b-2 — VM check of recovery mode on the app side (controller)

- **The build:** CI's artifact `cmdstudio-exe` of this branch's head, checked as `docs/spikes/phase3-vm-check/protocol.md` "Setup" says. Record the commit and the run id.
- **VMs:** VM-A (`cmdstudio-win11`, `tester`) and VM-C (`cmdstudio-th`), each from `clean`. The Setup, the helpers, `$startApp`, `$closeApp`, `$closeCmd`, `$health`, `$appLog`, `$started` and the driving rules are 3b-1's `vm-check.md` "Setup", with `$g`, `$dataArg` and the three binaries copied the same way, plus `$run = { Invoke-GuestDesktop $vm "$g\recovery-run.ps1" "-Show $dataArg" $u }`.
- **The words** in the PASS lines are the shipped ones: `app/ui/src/i18n/en.ts` (`th.ts` on VM-C), `app/src-tauri/src/backend/run.rs` and `lib.rs` (the log lines), `app/src-tauri/src/lib.rs` (`window_question`, `revert_said`).
- **Rules:** as 3b-1's. Nothing here runs on the dev PC. `recovery-run.ps1 -Kill` is the one place the app is not closed with `CloseMainWindow`: that is the crash under test.
- **Date / commit / run:** <date> / <sha> / <run id>

**The guard test first, on VM-A:** `Invoke-GuestDesktop $vm "$g\guard.test.ps1" '' $u`. PASS when its last line is `guest guards: 43/43 checks pass` (write the number Step 4 counted).

## Steps (VM-A all, in this order; VM-C 1, 3, 5, 9)

| # | Do | PASS when | Result |
|---|---|---|---|
| 1 | `& $startApp`, the wizard to Terracotta applied, Home (3b-1 step 1). `& $closeApp`. `& $run`. | The log has `run start safe=false failed_starts=0` and, after the close, `run end`; `& $run` prints `(no run.json)` and `crash notes: 0`. | |
| 2 | **Two failed starts.** App closed. `Invoke-GuestDesktop $vm "$g\recovery-run.ps1" "-FailedStarts 1 $dataArg" $u`. `& $startApp`, wait 15 s, `Save-Shot $vm 'p3b2-recovery-failed'`; `& $run`; `& $appLog`. | The page is "Recovery mode" with "CMD Studio failed to start 2 times in a row, so it opened in recovery mode. Nothing of your themes was loaded.", the buttons "Undo everything", "Check now", "Open the log folder", "Copy details", "Reset the app's settings", "Start normally", and no sidebar. The log has `run record=…` nothing (the pid is gone: no `still-running`), `run start safe=true failed_starts=2`, and no `crash_check` line (recovery mode starts none). `run.json` holds `"failedStarts": 0` once the page is up (`app_ready`). `themes_list` is not in the log since the start. | |
| 2b | `Invoke-GuestButton $vm 'Start normally' $u`, wait 10 s, `Save-Shot $vm 'p3b2-after-normal'`; `& $run`; `& $appLog`. | Home, with Terracotta in use. The log has `cmd=app_restart ms=<n> ok`, then the old process's `run end`, then the new one's `run start safe=false failed_starts=0`; `run.json` names the new pid (the old one's exit left it alone). | |
| 3 | **The Recovery shortcut, app closed.** `& $closeApp; & $closeCmd`. `Send-Run $vm 'C:\cmdstudio\phase3\check\cmdstudio.exe --recovery'` (the artifact's exe, as `start-app.vbs` names it; read that file for the path), wait 15 s, `Save-Shot $vm 'p3b2-shortcut-closed'`; `& $appLog`; `& $health`; `Get-GuestAutoRun $vm 'after-recovery' $u`. `Send-Run $vm 'cmd'`, wait 10 s, `Get-GuestCmdWindows $vm $u`, `& $closeCmd`. | "Recovery mode", "CMD Studio was opened from the CMD Studio Recovery shortcut.", "When it opened, CMD Studio put back any change that was cut off and paused the theme on this computer:", "The theme is paused: cmd windows open without it.", the banner "The theme was paused from the CMD Studio Recovery shortcut." with "Turn the theme back on". The log has `run start safe=true failed_starts=0` and `pause reason=recovery count=0 revert=ok saved=true`. `paused.json` has `"reason": "recovery"`, `"by": "app"`. AutoRun `(absent)`. The new cmd window is plain, `Clink loaded: no`. | |
| 3b | `Invoke-GuestButton $vm 'Turn the theme back on' $u` → Apply → Close (3b-1 step 4b). `Invoke-GuestButton $vm 'Start normally' $u`, wait 10 s. | The plan applies Terracotta; after the restart Home shows Terracotta, no banner. | |
| 4 | **The shortcut, app open.** `& $startApp` (Home). `Send-Run $vm 'C:\cmdstudio\phase3\check\cmdstudio.exe --recovery'`, wait 15 s, `Save-Shot $vm 'p3b2-shortcut-open'`; `& $appLog`; `Invoke-GuestPs $vm 'Get-Process cmdstudio | Measure-Object | ForEach-Object Count' $u`. | One `cmdstudio` process. The window reloaded into "Recovery mode" with the shortcut's reason and the paused banner; the log has `pause reason=recovery …` and then `cmd=app_status ms=<n> ok` from the reload. Then "Turn the theme back on" → Apply → Close, "Start normally". | |
| 5 | **A crash while in use.** Home with Terracotta. `Invoke-GuestDesktop $vm "$g\recovery-run.ps1" '-Kill' $u`. `& $startApp`, wait 15 s, `Save-Shot $vm 'p3b2-crash-notice'`. `Invoke-GuestButton $vm 'Report a problem' $u`, `Save-Shot $vm 'p3b2-report'`. Home, `Invoke-GuestButton $vm 'Close' $u`; `& $appLog`. | `-Kill` prints `run.json after the kill: still there`. Home opens normally (no recovery mode: the run had reached its UI) with the notice "CMD Studio closed unexpectedly after starting at <time>." (no crash note: a kill writes none) and "Report a problem", "Close". "Report a problem" is one click to the Report tab (criterion 4: two clicks at most). After Close the notice is gone and the log has `cmd=crash_dismiss ms=<n> ok`. | |
| 6 | **The renderer** (only if §6.4 is in; else "not run: §6.4 dropped (F1)"). Home. `Invoke-GuestDesktop $vm "$g\recovery-renderer.ps1" '' $u`, wait 10 s, `Save-Shot $vm 'p3b2-renderer-1'`; `& $appLog`. Then `Invoke-GuestDesktop $vm "$g\recovery-renderer.ps1" '-Times 3 -Gap 8' $u`, wait 10 s, `Save-Shot $vm 'p3b2-renderer-3'`. | The first kill: `page came back: True`, Home drawn again, the log has `cmd=app_status` after the kill (the reload asked first). Three kills within a minute: the Yes/No box "CMD Studio could not open its window: its page stopped three times in a minute. Your themes are safe. Undo everything CMD Studio changed on this computer (cmd, Windows Terminal, fonts)? The log is in <logs>." Press No (`Send-Key $vm Escape` or Tab to No, Enter): the app ends; `run.json` is gone (a normal exit). Record S2 in `results-3b2.md` from the first kill. | |
| 7 | **The bar and the restart.** `& $closeApp; & $closeCmd`; `Invoke-GuestDesktop $vm "$g\recovery-hang.ps1" '-Swap' $u` (Oh My Posh never answers: an apply's verify step waits on it until the process timeout, 60 s). `& $startApp`; Themes → Tokyo Night → Apply; wait 12 s, `Save-Shot $vm 'p3b2-working'`; wait until the apply ends (`settings.json`/`state.json` change or 90 s), `Save-Shot $vm 'p3b2-working-end'`. `Invoke-GuestDesktop $vm "$g\recovery-hang.ps1" '-Restore' $u`. | At 12 s the top of the page shows "CMD Studio is still working: applying the theme…" (`role=status` "Still working"); when the apply ends the bar is empty again. The 3-minute restart button is not reached here (the process timeout is 60 s): recorded, not judged; the unit tests pin it. | |
| 8 | **The deadline (`E_TIMEOUT`).** Not reproducible on a VM without a stand-in that holds a read for a minute: `work.test.ts` pins it. Record "pinned by work.test.ts". | — | |
| 9 | **Thai (VM-C):** steps 1, 3 and 5 in Thai after the Thai wizard walk (3b-1's "On VM-C"). | The words of `th.ts`: "โหมดกู้คืน", "CMD Studio ถูกเปิดจากทางลัด CMD Studio Recovery", "ธีมถูกหยุดไว้จากทางลัด CMD Studio Recovery", "เปิดธีมกลับ", "เริ่มแบบปกติ"; the crash notice "CMD Studio ปิดไปแบบไม่ปกติ หลังจากเปิดเมื่อ <time>"; the box of step 6 stays English. | |
| 10 | **Timing A/B (carry F4), VM-A.** Theme applied, app closed, `& $health` empty. `Invoke-GuestDesktop $vm "$g\recovery-timing.ps1" "-InstallMarker $dataArg" $u` (with the counter; save as `vm-check-timing-ab-with-vm-a.txt`), then at once `Invoke-GuestDesktop $vm "$g\recovery-timing.ps1" "-WithoutCounter $dataArg" $u` (save as `…-without-vm-a.txt`), then the first again (the order's effect). Remove `zz-timing.lua` and the markers as 3b-1 step 7 does. | Three `average of 10` lines in one boot; the counter's cost is with − without, recorded for the founder against the 5 ms target (F7), not judged. `init.lua restored` printed; `& $health` empty. | |
| 11 | **The second refilter (carry F3), VM-A.** As 3b-1 step 2f with the new Lua: `-Probe refilter -Install`, a window, 15 s, `-Report`, `-Remove`. | The report has `own n=1`, `own n=2`, `own n=3` and, before `own n=3`, a `refilterprompt from <caller>` line, or none. Record which: a line naming Oh My Posh's script is its transient or async prompt (nothing of ours); none is Clink's own second refilter. Whether it matters: the time between `own n=2` and `own n=3`'s end (3b-1 measured 1 ms). No code change: recorded for the founder. | |
| 12 | **Cleanup.** `& $closeApp`; `Get-ExitCode $vm 'cleanup' '--cleanup --data-dir @LOCALAPPDATA@\ai.resincore.cmdstudio' $u`; list the folder. | `exit=0`; nothing but `themes\`: `run.json`, `logs\crash-*.txt` (if any) and a `settings.json.reset-<ms>` (make one in step 2's page first with "Reset the app's settings", VM-A) are taken for CMD Studio's own. | |

## VM-C's words

The Thai of each PASS line above comes from `th.ts` (Task 10's keys); what the box says, `run.json` and the log lines are the same as on VM-A.
````

- [ ] **Step 7: Commit**

```bash
git add -- docs/spikes/phase3b-recovery/vm-check-3b2.md docs/spikes/phase3b-recovery/vm-check.md spikes/vm/guest/recovery-run.ps1 spikes/vm/guest/recovery-timing.ps1 spikes/vm/guest/recovery-filters.ps1 spikes/vm/guest/guard.test.ps1
git rm -- spikes/vm/guest/recovery-tasklist.ps1
```

Message: `spikes: the VM protocol of phase 3b-2 (recovery mode, the shortcut, the crash notice, the bar, the renderer, the timing A/B and the refilter's caller); the tasklist script goes`.

---

## Self-review (done while writing; the executor need not repeat it)

- **Spec coverage.** §4.4 (Tasks 4, 10, 11); §5.1 (4); §5.2 (4, 6, 10); §5.3 (5, 10); §5.4 (6); §6.1–§6.3 (8, 9); §6.4 (1, 6, 12); §6.5 (9); §7 (3); §8 (7); §9 (8–10); §10 (every task's tests; 12); §12's records (2, 3, 11); the carries (2, 12, 6 for m4). §4.4's installer shortcut is phase 4's: Task 11 writes its spec text only.
- **Placeholders.** None: the angle brackets left are the controller's VM results (`<date>`, `<sha>`, `<time>`, the appendix B bullet that waits for Task 6's outcome, each named as such).
- **Types.** `GuiStart`, `SafeModeReason`, `CrashNotice`, `RecoveryEntry`, `AppStatus`, `CrashReport` (Tasks 4–5) are what Task 7 exports and Tasks 8–10 import; `KINDS`/`TASK` (Task 8) key on `keyof Ipc` as Task 7 leaves it; `ResumeOutcome::NotKnown` (Task 6) is `{ kind: "notKnown" }` in Tasks 7 and 10; `Paths::run_file`/`settings_reset_file` (Task 3) are what Tasks 4 and 6 call; `fake_host` (Task 4) over `FakeHost.processes` (Task 3).
- **Review Focus.** Each of the five has its test named in its task (4, 4, 8, 5, 4).
