# Phase 4a installer, console exe, Path and release: rulings, deferred minors and carries

Every decision the controller made on the founder's behalf while executing `2026-11-04-phase4a-installer-cli-release.md` (subagent-driven, 2026-11-04 to 2026-11-05), in the order made, each with its reason and what it costs if wrong; the founder's own decisions during the run; every minor finding a reviewer raised that this plan did not fix; every item handed on; the VM check's findings. Then the pre-flight conflicts and the controller's rulings files in full, which override the plan's text where they differ. Source: the execution ledger and the rulings files; the git history has the commits and their evidence.

## The founder's decisions during the run (3)

- Founder (2026-11-04): "ทำ exe แยกในเฟส 4 แล้วต่อเลย" — the console exe is phase 4's.
- Founder (2026-11-04): "ใช้ตามแผน Subagent-driven ลุยต่อเลย" — plans 4a/4b as written (cmdstudio.com, user Path, hooks, draft release), execution subagent-driven.
- Founder 2026-11-05: permission to download the CI artifact (once). Downloaded cmdstudio-installer from run 41887810651 to the scratchpad dl-41887810651/; hash matches the CI log.

## Rulings (13)

- Ruling (pre-flight): the architect revises the plan in place for P1-P22 per preflight-rulings.md — P1 bundle block in a separate --config file used only when bundling; P2/P15 settled from tauri source; P8 string compares; P11 Task 8 waits for a pushed CI run; P12 release.yml reuses ci.yml via workflow_call; P18 uninstall continues and the box never names a removed program; P19 deletes inside the not-update guard per §6.3 — the scan's evidence holds against shipped code — cost if wrong: plan text only.
- Ruling (Task 3): CheckIfAppIsRunning first inside the guard; the two Deletes move to POSTUNINSTALL under the guard (replaces the brief's empty-POSTUNINSTALL pin) — cost if wrong: one hook reorder.
- Ruling (Task 3): 4a keeps the manual over-install behaviour (full cleanup, theme off, data except themes\ removed, wizard again: safe, not smooth); comments corrected; an appendix B sentence says so; keeping the theme across versions is 4b's updater path (/UPDATE) and 4b's spike looks at the manual case — cost if wrong: users who upgrade by hand re-run the wizard once.
- Ruling: no further push until CI run on b610e6b completes — each push cancels the previous PR run (cancel-in-progress), so pushing per task never yields a finished run — cost if wrong: CI feedback arrives one batch later.
- Ruling (Task 6): draft runs only on event push of a v* tag; --verify-tag added; the draft job uploads CMD-Studio_<version>_x64-setup.exe (no space) with a fresh SHA256SUMS.txt for that name; ci.yml's artifact unchanged — cost if wrong: the release asset name differs from the installer's build name.
- Ruling: Task 8 (spec records what shipped) runs after Task 9 and after a finished CI run gives the installer size and run id (P11) — cost if wrong: none.
- Ruling (Task 9): the done-criterion's "registry byte-equal" covers what CMD Studio changes (AutoRun, the user Path, fonts' keys, WT settings); Tauri's own install-state key under HKCU\Software\<publisher>\CMD Studio, kept by its uninstaller when the box is unticked, is outside it — step 7 prints it as information and Task 8 records it in appendix B — cost if wrong: one small leftover key per uninstalled PC.
- Ruling (Task 8): §6.4's "first dry run of release.yml: run <id>" cannot be filled before merge — workflow_dispatch needs the workflow on the default branch, and release.yml reaches main only with this PR — so Task 8 writes that the first dry run (workflow_dispatch) follows the merge, before the founder's tag — cost if wrong: one later spec line with the run id.
- Ruling (Task 8): the .com size comes from the installed file on VM-A (the CI 7z listing goes to a file, not the log) — cost if wrong: none, same bytes.
- Ruling (Task 9): the one-line -Cli redirect fix is made by the controller (f553859), parse-checked, no re-review — one mechanical line whose effect VM-A's step 4 shows next — cost if wrong: VM-A's step 4 file read fails again and shows it.
- Ruling (Task 9): VM-A's remaining steps move to VM-B, which starts reliably: step 6 (font held, with Tokyo Night applied in step 5) and 6b, then 9b; VM-A's 7 and 9a are covered by VM-B's and VM-C's runs of the same installer; protocol.md's step 6 names Tokyo Night — the same installer and hooks, and the font hold does not depend on Windows 11 or Terminal — cost if wrong: a Windows 11-only difference in the failed-cleanup or by-hand over-install path goes unseen until 4b's VM run.
- Ruling (Task 9): step 6 and its 6b (the failed-cleanup box, the kept data folder) stay open in 4a and move to 4b's VM run, which first needs a working VM with Windows Terminal (a powered-off clean for VM-A or VM-C — the founder's VM step); the founder is told before the v0.1.0 tag — the hook's failure branch is NSIS text reviewed twice and pinned by config_guard, and every other path passed on real VMs — cost if wrong: a cleanup that fails on a user's PC shows a wrong box or loses the data folder; what --cleanup could not put back stays until the hand fix the box names (install again, Undo everything on the Help page).
- Ruling (Final): the Drop guard deletes only its own <ms> key and the parent only when empty (narrower than the review's text) — it avoids one run deleting another's key — cost if wrong: an empty parent key left on a runner.

## Final review findings re-graded into the fix pass (1)

- Final: re-graded by effect into the fix pass: M2 (a running cmdstudio.com keeps the install folder after uninstall: a leftover on the user's PC), M3 (the failure box can lose the FAILED line to nsExec's 1024-char cap: the one case the box exists for), M7 and M8 (README states the over-install's uninstall as unconditional and names only SmartScreen; the VMs met the Open File warning).

## Deferred minors (16)

- Task 1: minor (deferred): start_app_beside should null the child's stdio (cmdstudio.com --recovery | more never sees EOF).
- Task 2: minor (deferred): raw bytes kept at text level only (extra NULs / no terminator normalised to one NUL); remove's write/delete failures untested; REG_SZ with bytes after NUL untested; broadcast SAFETY comment overstates HWND_BROADCAST's 5 s; a missing blank line in user_path.rs.
- Task 3: minor (deferred): Section WebView2 runs before PREINSTALL's ARM64 refusal (.onInit would refuse earlier); the gate reading missed the reinstall-page lines.
- Task 4: minor (deferred): Help.test's constant-reading asserts are low value; no test ties the credits sentence to tauri.bundle.conf.json's mapping.
- Task 5: minor (deferred): the root Cargo.toml version check is a substring (any section; CRLF would break it); `version.workspace = true` substring would match a comment.
- Task 3: minor (deferred): inside_if counts only `${If} ` openers; a failed rename is followed by the Path failure box.
- Task 6: minor (deferred): a dispatch dry run on main waits in CI's main group.
- Task 7: minor (deferred): the credits-page copyright lines and the "security" issue-title policy rest on the spec/brief (no SECURITY.md).
- Task 7: minor (deferred): "deletes the app's data. Your themes stay" reads oddly (themes live in that folder); README:21 omits the ProgramData folder that --cleanup removes when <clink-launch> used it.
- Task 9: minor (deferred): FontValues compares data, not kind; the silent uninstall's exit code belongs to the launcher copy (the key wait is the signal).
- Final: minor (deferred): M1 the Start-menu ">" (no shortcuts) is not mirrored by the hook; the outcome is right by accident (invalid name).
- Final: minor (deferred): M4 no fallback to cmdstudio-cli.exe when cmdstudio.com is missing at uninstall (only after a hand deletion).
- Final: minor (deferred): M5 another spelling of the install folder in the Path is the user's entry, not ours — a module-doc sentence.
- Final: minor (deferred): M6 the version grep in release.yml/ci.yml takes the first "version =" line of Cargo.toml, not anchored to [workspace.package].
- Final: minor (deferred): M9 start_app_beside inherits stdio; M10 the ARM64 refusal leaves an empty $INSTDIR; M11 config_guard's inside_if counts only "${If} "; M12 the broadcast's SAFETY comment's 5 s is per window.
- Final: minor (deferred): §5.9's E_PATH_WRITE row and error.rs name "not plain text / could not be written", not the new ';'-in-folder cause (the message itself names it).

## Carries (11)

- Carry (final fix pass): app/src-tauri/Cargo.toml gets `default-run = "cmdstudio"` if two [[bin]]s make `cargo run`/`tauri dev` ask for --bin (check in the final review).
- Carry (final fix pass): refuse an exe_dir containing ';' with E_PATH_WRITE; the gated Path test gets a drop guard for its throwaway subtree and its doc says it calls user_path directly.
- Carry (4b): the spike checks whether a manual over-install can keep the theme (a signal from the newer installer to the old uninstaller), and the release notes say how to upgrade.
- Carry (Task 7 / final): the README names the release asset CMD-Studio_<version>_x64-setup.exe.
- Task 7: implementer DONE 934ee42 (README only; versions, CLI table, exit codes, installer, SmartScreen, uninstaller, Path checked against sources; user-name grep empty; P17: exports carry no $schema, the reader drops one — "accepted and ignored" kept). Departures: "usually has it"; exit 1 adds "or refused before anything was changed"; Undo = the button after an apply; Path listed; manual over-install bullet.
- Task 7 review dispatched (sonnet); Task 6 fix round 1 dispatched (resume implementer, base 934ee42, plus the README's asset name per the carry)
- Carry (Task 8): the spec names the release asset CMD-Studio_<version>_x64-setup.exe where it names a release download (CI's artifact keeps the spaced build name).
- Carry (4b): the 4b plan's latest.json URL (line ~100, CMD%20Studio_...) must use the release asset name CMD-Studio_<version>_x64-setup.exe.
- CI run 41884180720 (b610e6b): windows ✅ (incl. system_user_path_add_and_remove_round_trip ok in the system-test step, no SKIPPED); app ❌ at "Tauri build (with the NSIS bundle)": "failed to find main binary, make sure you have a `package > default-run`" — two [[bin]]s (the carry from Task 1). Installer never built yet.
- VM-A round 1 (fork): steps 1-5 PASS, step 4's file now holds EXIT=0 (f553859 verified); 6b adapted (ticked uninstall with Terracotta applied) VERDICT: PASS. Finding (Task 9): step 6 cannot be set up with Terracotta — it installs no font (state.json "fonts": []), so -HoldFont has nothing to hold; tokyo-night and one-dark carry CaskaydiaCove NF. Minor (Task 9): Invoke-Phase4Control sees the uninstaller's checkbox and the warning's Run only as Panes; keys work. VM-A then failed to start from clean again (HGCM VERR_ACCESS_DENIED; also "VBoxIsoMaker does not support async I/O"), twice in the fork, once by the controller; left aborted-saved, not repaired.
- Task 8: implementer DONE_WITH_CONCERNS 95b7ca1 (error_codes 4, app error:: 7; diff §6.1 l.676/683, §6.4 l.728, appendix B l.906). Carry 1: no existing x64-setup mention, so the release and CI names were added to §6.4 l.728. Carry 3: the file list existed; "no cmdstudio-cli.exe" added. PC-safety slip: wrote a scratch script to /tmp, deleted it.

## The VM check (2026-11-05, build efb7435, CI run 41887810651)

- Verdict: no finding in the hooks, the console exe or the Path. VM-A 1-5 and a ticked-box uninstall PASS; VM-B 1-5, 7, 9a, 9b PASS; VM-C 1-5, 7, 8, 9a PASS. Details: `docs/spikes/phase4-install/protocol.md` § Results.
- Open: step 6 (a cleanup that cannot finish, its box, the kept data folder) and its 6b. It needs a theme that installs the Nerd font, which the plan does only with Windows Terminal, and VM-A and VM-C (the two with Terminal) failed to start from their saved `clean` state (HGCM VERR_ACCESS_DENIED). It moves to 4b's VM run, which first needs a powered-off `clean` for VM-A or VM-C; the founder is told before the v0.1.0 tag.
- F1 (Task 9, fixed in f553859): the `-Cli` batch file lost the exit code (`EXIT=0>>` is a handle-0 redirect).
- Observed: SmartScreen did not show for the unsigned build; Windows' "Open File - Security Warning" (Unknown Publisher) did. `cmdstudio.com` is 2,704,384 bytes. Tauri keeps `HKCU\Software\CMD Studio contributors\CMD Studio` after an unticked uninstall. Doctor exits 0 with a warning (D6 on VM-B).

## The rulings files

### Pre-flight conflicts P1-P22 (the plan was revised for them in 443341e)

##### E. Conflicts found

| # | Plan line | What is wrong | Suggested ruling |
|---|---|---|---|
| P1 | T3 Step 2 (`bundle.resources`, l.1145-1149), G-RES (l.72), F-A (l.1280) | `tauri-build` copies every `bundle.resources` entry at **every** cargo build and returns `ResourcePathNotFound` for a missing source (`tauri-build-2.7.3/src/lib.rs:744-761`, `tauri-utils resources.rs:206`). G-RES's assumption is false. `target-stage/cmdstudio.com` is gitignored and created only by `beforeBundleCommand`, after the cargo build, so after T3 every `cargo build/clippy/test` of the app crate fails, `config_guard` in T3 Step 5 included. `../../resources` is also gitignored and `ci.yml`'s `windows` job runs clippy before "Fetch the pinned runtimes" (l.43-69), `release.yml` likewise, and `build.rs` promises fresh-clone builds. F-A as written drops only the `.com` entry, not `resources`/`LICENSE`. | Treat G-RES as failed now. Keep `bundle.resources` out of the committed `tauri.conf.json`: put the bundle block in a separate `app/src-tauri/tauri.bundle.conf.json` passed with `--config` by CI, `release.yml` and the README, and make `config_guard` read both files. Or install resources from `hooks.nsh` with `File /r`. Update the spec §6.1 sentence on the map. |
| P2 | T1 Step 5 (second `[[bin]]`), T3 Step 2/4 | Unverified but likely: the Tauri bundler installs every `[[bin]]` of the package next to the main exe, so the installer would also hold `cmdstudio-cli.exe` (a second ~app-sized exe linking the whole tauri tree) besides the staged `cmdstudio.com`. G10 never checked it; neither gate lists it; `Delete`/listing/spec size do not account for it. | Add to G-NSI: "does the bundle contain `cmdstudio-cli.exe`". If yes: drop the stage script and the resource entry (also dissolves P1 for the `.com`), `Rename "$INSTDIR\cmdstudio-cli.exe" "$INSTDIR\cmdstudio.com"` in POSTINSTALL (or `CopyFiles`), delete both names in PREUNINSTALL, and list both in the 7z check. Report the extra size in T8. |
| P3 | T3 hooks test (l.1110) and hooks text (l.1202, 1241, 1242) | `assert_eq!(hooks.matches("$UpdateMode").count(), 2, …)` fails: the specified file contains `$UpdateMode` three times. | Count code lines only (strip lines starting `;`) and assert 1, or assert 3; do not count comments. |
| P4 | Review Focus 3, 4, 5 (l.48-50), T1 Step 6 (l.311) | Names tests that do not exist: `a_failed_cleanup_keeps_the_data_folder`, `the_path_and_the_cleanup_run_only_outside_update_mode`, `the_recovery_shortcut_is_made_after_every_install` (T3 has three differently named tests) and `usage_names_the_two_programs` (the usage test is `the_usage_text_is_english_and_names_every_command`, so "keep the existing name" applies and the Focus entry is dead). | Split T3's long test into the three named ones (this also fixes D8: put the `StrCpy` inside an `${If} $0 != 0` pin) and change Focus 4 to the real usage test name. |
| P5 | T2 Step 3 (l.605) | "`use std::cell::{Cell, RefCell}` is in the file already": `fake.rs:8` imports only `RefCell`; `Cell` is used nowhere. `FakeUserPath` does not compile. The file also imports from `super::{…}`, not `crate::adapters::{…}`. | Say "change line 8 to `use std::cell::{Cell, RefCell};` and add `UserPath` to the `use super::{…}` list". |
| P6 | T2 Step 6 (l.813) | `library.rs`'s `System` literal is `DiskMachine`'s, built from real Windows adapters (`library.rs:1342-1380`); there is no fake `user_path` to hand over. | Add a `user_path: WinUserPath` field to `DiskMachine`, `WinUserPath::at(unused())` in `under()` and `user_path: &self.user_path` in `system()`. |
| P7 | T2 Step 2/6, `crates/cli/Cargo.toml` | Files header omits `crates/cli/Cargo.toml` and `Cargo.lock`, which the `git add` list and Step 7 change. `Cargo.lock` does change (the `cmdstudio-cli` package gains a `winreg` edge), contrary to the "no new dependency in Cargo.lock" constraint's plain reading. | Add both files to the header; reword the constraint to "no new crate". |
| P8 | T3 hooks (l.1231-1264) | `${If} $0 != 0` after `nsExec::ExecToStack`: on failure to start, `$0` is the text `error`, which LogicLib's numeric `!=` treats as 0, so a missing or blocked `cmdstudio.com` is taken as success (cleanup "succeeded", data folder deleted, Path left). | Use string compares: `${If} $0 == "error"` `${OrIf} $0 S!= "0"`; pin the form in the hooks test. |
| P9 | T9 Step 1 vs Step 3 | `-Snapshot` has `ValidateSet('before','after','')` but the protocol calls `Get-Phase4Snapshot … mid`, `… applied`, `… reinstall`: parameter binding fails on steps 3, 5, 9. | Take any label (`[ValidatePattern('^[a-z0-9]+$')]`) and make `-Compare` use only `before` and `after`. |
| P10 | T9 Step 3 protocol steps 2, 5, 6, 6b, 7, 9 | Step 2's VM-B silent install uses `Invoke-GuestDesktop … setup.exe '/S'`, but that function runs a `.ps1`; the plan flags this for steps 6b and 7 only and "step 9 uses step 2's VM-B form". Steps 5 and 6 use `& $closeApp`/`& $closeCmd`, defined by 3b-1's `vm-check.md`, not "phase 3's". `-ReleaseFont` is promised and absent. Step 4 parse-checks two of three changed `.ps1`. Step 6's `Send-Run "$env:LOCALAPPDATA\…"` expands on the host (D13). | Rewrite step 2's VM-B cell, 6, 6b, 7 to `Invoke-GuestPs … Start-Process -Wait … '/S'` in the committed text; name 3b-1's setup as the source of `$closeApp/$closeCmd` (or define them in `helpers.ps1`); drop `-ReleaseFont`; parse-check all three scripts; use `C:\Users\$u\…` paths. |
| P11 | T8 Steps 1-2, T3 Step 8, T9 Step 5 | T8 needs the installer size and a CI run id; they exist only after a push, which agents never do. The "(เขียนตอนวางแผน)" string to replace has 0 matches in the spec. T9 Step 5 edits the spec outside T9's `git add` list. | Make T8 conditional on the controller's pushed run (or move it after the first CI run), delete the absent-string instruction, add the spec to T9's step 5 commit or leave that line to T8's follow-up. |
| P12 | T6 `release.yml` | Claims to repeat CI's gates but leaves out the "System tests" step with `CMDSTUDIO_SYSTEM_TESTS=1` and its SKIPPED grep (which also runs the new real-registry Path test), "Clippy of the app crate", "Tests of the app crate" and the console exe's `--help`/exit 2 checks. Spec §6.4: "ทำทุกขั้นตรวจเหมือน ci.yml". | Add the system-tests step and the exit-2 check, or reuse CI through `workflow_call`/a composite action instead of a pasted copy (also cures A6). |
| P13 | T2 `run` (`Defaults.exe_dir`), `path_change` | `exe_dir` is `unwrap_or_default()` (empty) if `current_exe()` fails; `--add-to-path` then adds an empty entry. | Refuse an empty or relative `exe_dir` with `E_PATH_WRITE` (and a test) before calling `user_path::add/remove`. |
| P14 | T2 Step 7 gated test | `let _ = PATH_VALUE;` hack; wrong test name in the SKIPPED line from the shared `enabled()`. | Remove the import and the line; give `enabled()` a name argument. |
| P15 | T1 Step 8, T3 `stage-console-exe.mjs` | Assumes `tauri build` (with and without `--no-bundle`) also builds `cmdstudio-cli.exe`; not in any gate. If the CLI passes `--bin cmdstudio` to cargo, T1's CI step and the stage script fail with "is not there". | Add to G-RES/G-NSI; or have `beforeBundleCommand` run `cargo build --release -p cmdstudio-app --bin cmdstudio-cli` first. |
| P16 | T1 Step 5 `main.rs` doc | "Replace the module doc's third paragraph": the `cmd does not wait…` text is the end of the second paragraph (`main.rs:6-10`); the third is the crash-hook paragraph. | Name the exact lines (`//! cmd does not wait…` to `…both wait.`) to replace. |
| P17 | T7 README, "Theme files" | "the file's `$schema` line points at it" is not shown by any shipped preset or exporter (no `$schema` in `presets/terracotta.cmdtheme`; `model.rs:86` only accepts one). | Check the exporter, or drop the clause. |
| P18 | T3 hooks failure message (l.1257, l.1278) | After the uninstall the program, Path entry and install folder are gone, so "run `cmdstudio --cleanup` again" cannot be followed; the data folder (kept on purpose) is the only copy of the original settings. | Say: "install CMD Studio again, then press Undo everything in Help", or `Abort` the uninstall on a failed cleanup. Put the same text in README and protocol step 6. |
| P19 | T3 Step 4 (`Delete` lines after the `${If}`) vs spec §6.3 | The spec deletes the Recovery shortcut and the `.com` in the "not an update" bullet; the plan deletes them also in update mode (then recreates). Plan and spec differ on the update case; Review Focus 5 treats it as intended. | Record the difference in T8's appendix B row, or move the two `Delete`s inside the `$UpdateMode <> 1` guard. |
| P20 | T4 Step 1 | "the three assertions fail": two tests fail (M7 test, credits test). | Reword. |
| P21 | T2 Step 6 parse arm | Second read of `args` after `take_flag` (D4). | Bind both `take_flag` results. |
| P22 | T3 Step 1 test `the_bundle_is_a_per_user_nsis_installer…` | Pins `resources` as `{"../../resources": "resources", …, "target-stage/cmdstudio.com": …}` in the committed config: invalidated by P1 and P2. | Re-pin after the P1/P2 rulings. |

### The controller's rulings on P1-P22

#### Controller rulings on the pre-flight conflicts P1-P22 (plan 4a)

Take the scan's suggested ruling (preflight.md section E) for every P row, with these choices. Where a ruling depends on Tauri's real behaviour, read the source (cargo registry under %USERPROFILE%\.cargo\registry, app\node_modules) and decide from it; reading is fine, building the bundle is not (no NSIS download).

- P1: the bundle block (`bundle.resources`, the NSIS options, `beforeBundleCommand`) moves out of the committed `tauri.conf.json` into `app/src-tauri/tauri.bundle.conf.json`, passed with `--config` only by the commands that bundle (CI's bundle step, release.yml). Every cargo build, clippy and test keeps working without `resources/` or `target-stage/`. `config_guard.rs` and Task 3's config test pin both files. If the source shows a better way (for example a resource source path that tauri-build tolerates), state it and use it.
- P2 and P15: settle them from the tauri-bundler/tauri-cli source now, while revising. Find out whether `tauri build` builds and bundles every `[[bin]]`. Pick the smallest design that yields exactly one `cmdstudio.exe` and one `cmdstudio.com` in `$INSTDIR`, and record what you read (file:line) in the plan's Implementation Gate.
- P3, P4, P20, P22: fix the counts and the names. Split Task 3's hooks-text test into the named tests that Review Focus cites (`a_failed_cleanup_keeps_the_data_folder`, `the_path_and_the_cleanup_run_only_outside_update_mode`, `the_recovery_shortcut_is_made_after_every_install`). Review Focus 4 cites the real usage test name, or Task 1 adds `usage_names_the_two_programs`.
- P5, P6, P7, P13, P14, P16, P21: as suggested. P13: refuse an empty or relative `exe_dir` with `E_PATH_WRITE`.
- P8: string compares for nsExec's result (`"error"`, `"timeout"`, then `S!= "0"`), pinned in the hooks-text test.
- P9, P10: as suggested. Parse-check all three changed `.ps1` files. Keep the protocol's VM commands defined in this protocol.
- P11: Task 8 runs after the controller pushes the branch and CI has built the installer. The plan says so: Task 8 waits for a run id the controller gives it. Fix the spec string to one that exists. Task 9's spec edit goes in its `git add` list, or moves to Task 8.
- P12: release.yml reuses ci.yml's gates through `workflow_call`, not a copy, so the two cannot drift.
- P17: check the exporter. If exported themes carry `$schema`, say "exported theme files"; if they do not, drop the clause.
- P18: the uninstall continues. The failure box shows the code and what `--cleanup` printed, and says to install CMD Studio again and use "Undo everything" (or run `cmdstudio --cleanup` from the reinstalled app). It never names a program that is already gone.
- P19: follow spec §6.3. The shortcut and `.com` deletes go inside the not-update guard, unless the source shows that an update leaves a stale file. In that case, keep them outside and add an appendix B line saying why.

Revise the plan in place. Keep every task's shape and order unless a ruling forces a change, and keep the Global Constraints. Commit it alone.

### Task 3: fix round 1

#### Task 3, fix round 1

The review of 2d6ec16..92a5461 found two Important issues and some Minors. The full review is below the rulings. Fix everything listed here in one commit. The rules are the same as before:
- TDD;
- the checks;
- the commit through `.git/CMDSTUDIO_COMMIT_MSG`;
- `git add --` with only the intended files;
- no attribution, never push;
- pc-safety.md.

##### Controller rulings (they override the brief where they differ)

1. **Important 2: PREUNINSTALL runs before Tauri's CheckIfAppIsRunning.**
   - **Ruling:** the guarded block (`$UpdateMode <> 1`) begins with `!insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"`. Use the macro's real signature as the template's utils.nsh defines it; read it and cite file:line in your report. If it aborts, nothing has run yet.
   - The two `Delete`s (`cmdstudio.com` and the Recovery shortcut) move to `NSIS_HOOK_POSTUNINSTALL`, still under `$UpdateMode <> 1`. POSTUNINSTALL also needs `$AppStartMenuFolder`, so check whether it is already set by then (template ~:27527) or needs the GETFOLDER line.
   - This replaces the brief's "POSTUNINSTALL is empty" pin (config_guard.rs ~:406-411).
   - Pin the new order in the tests:
     - the app-running check comes before the first nsExec, inside the guard;
     - the Deletes are in POSTUNINSTALL, inside a guard.
2. **Important 1: a manual over-install runs the old uninstaller with `$UpdateMode = 0`.**
   - **Ruling for 4a:** keep the behaviour. A manual install of a newer version over an old one runs the full cleanup. The theme comes off and the app data goes, except `themes\`. Then the new version installs, and the wizard runs again. This is safe (the machine returns to its state before CMD Studio), but it is not smooth.
   - Tauri's updater path, which passes `/UPDATE` so that `$UpdateMode = 1` skips the uninstaller, is 4b's to keep the theme across updates. 4b's spike also looks at whether a manual over-install can keep it.
   - Correct the two false comments in hooks.nsh (~:41-45, ~:104-108). Say exactly what the template does and cite :27004-27006 and :27041-27044.
   - Add one sentence to the main spec's appendix B row of 2026-11-04 (Thai, in the spec's register): "ติดตั้งรุ่นใหม่ทับรุ่นเก่าด้วยมือ ตัวติดตั้งของ Tauri เรียกตัวถอนรุ่นเก่าโดยไม่มี /UPDATE จึงย้อนธีมและล้างข้อมูลแอป (ยกเว้น themes\\) แล้ววิซาร์ดเริ่มใหม่ ปลอดภัยแต่ไม่ราบรื่น การเก็บธีมข้ามรุ่นเป็นงานของ updater ใน 4b". The spec file joins this commit's `git add`.
3. **Minor 1, taken:** POSTINSTALL's Delete and Rename are error-checked.
   - A failed rename shows its own English box: the console program could not be put in place, so `cmdstudio` in cmd may not work, and the app's `cmdstudio.exe` still runs.
   - Then the Path step goes on.
   - Pin it.
4. **Minor 3, taken:** under `/NS`, where Tauri skips its shortcut, the Recovery shortcut is skipped too. Check how the template tests `/NS` (~:27640-27643), do the same, and pin it.
5. **Minor 4, taken:** comment the `rfind("${EndIf}")` assumption in config_guard.rs. Better still, make the "inside the guard" check find the matching `${EndIf}`.
6. **Minors 2 and 5:** deferred in the ledger. Nothing to do.

Append a "Fix round 1" section to task-3-report.md. It contains:
- each ruling's change;
- the covering tests, with RED and GREEN;
- the command and its output;
- the template lines you read.

Your final message gives only:
- the status;
- the commit;
- a one-line test summary;
- the concerns.

##### The review

See the controller's copy: the reviewer's report is reproduced in the ledger summary. The two Important items and the Minors are exactly as listed above.

### Task 6: fix round 1

#### Task 6, fix round 1

The review of 098b221..b610e6b came back "Needs fixes", with one plan-mandated Important and three Minors.

Fix the items below in one commit, under the same rules as before:
- read `.git/CMDSTUDIO_COMMIT_MSG`, write it, then run `git commit -F` as a separate step;
- `git add --` with only the intended files;
- no attribution, never push;
- follow `pc-safety.md`.

##### Controller rulings (they override the brief)

1. **Important (plan-mandated): a hand-started run on a tag can create a draft.**
   - In `release.yml` ~:49, the `draft` job's `if` checks only `startsWith(github.ref, 'refs/tags/v')`. A `workflow_dispatch` started on a tag ref passes that check, so it runs `gh release create`. That breaks the binding rule that a dispatch run "uploads an artifact and creates nothing".
   - **Ruling:** use `if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v')`.
2. **Minor 3, taken:** add `--verify-tag` to `gh release create`, so that a re-run after the tag was deleted fails instead of making a tag from the default branch.
3. **Minor 1, taken:** GitHub may rename an uploaded asset whose name has a space, and the shipped `SHA256SUMS.txt` would then not match the file a user downloads.
   - **Ruling:** in the `draft` job, before upload, copy the installer to `CMD-Studio_<version>_x64-setup.exe` (a hyphen, no space).
   - Write `SHA256SUMS.txt` fresh for that name, in the `sha256sum` format, so that `sha256sum -c` works on the downloaded pair.
   - Upload those two files, and use that name in the release notes and the step summary.
   - Do not change ci.yml's artifact, which the VM check uses as it is.
4. **Minor 2:** recorded only, no change.

Append a "Fix round 1" section to `task-6-report.md`. Your final message gives only:
- the status;
- the commit;
- a one-line check summary;
- the concerns.

### Task 7: fix round 1

#### Task 7, fix round 1

The review of b610e6b..934ee42 came back "Needs fixes". Fix the items below in one commit, README.md only, under the same rules as before:

- read `.git/CMDSTUDIO_COMMIT_MSG`, write it, then run `git commit -F` as a separate step;
- `git add -- README.md`;
- no attribution, never push;
- follow pc-safety.md.

##### Items

1. **Important.** README.md:37 says you can uninstall from "Settings > Apps, or the Start menu". Tauri's template makes no Start-menu uninstall shortcut: its only `CreateShortcut` lines are the app shortcut (template :27648/:27651) and the desktop one (:27675). The controller checked this. Say "Settings > Apps (or `uninstall.exe` in the install folder)", per spec §6.1.
2. **Minor, taken.** README.md:61's "Uninstalling ... does the same" follows the Undo-everything line, but the uninstaller does more. Say so in a few words: it also deletes the app's data, except `themes\` unless you tick the box, and takes the install folder off your Path.
3. **Check.** The download name is already `CMD-Studio_<version>_x64-setup.exe` (Task 6's fix d53c8ad). Make every other mention of the installer's file name in the README use that name too, including any at :12 and :62.

Append a "Fix round 1" section to task-7-report.md. Your final message gives only:

- the status;
- the commit;
- a one-line check summary;
- the concerns.

### Task 9: fix round 1

#### Task 9, fix round 1

The review of d53c8ad..528d040 came back "Needs fixes". Fix the items below in one commit, under the same rules as before:

- VM scripts are parse-checked only, never run;
- commit through `.git/CMDSTUDIO_COMMIT_MSG`;
- `git add --` with only the task's files;
- no attribution, never push;
- follow pc-safety.md.

##### Items

1. **Important: `-ThemesKept` can print "THEMES KEPT, the rest gone" while the rest is still there.** See `install-check.ps1:384`. Today the check fails only on `state.json` or `snapshots`. Leftover `webview`, `logs`, `generated`, `runtime`, `clink-profile`, `drafts`, `settings.json` or `cmdstudio.lock` all pass. `crates/core/src/cleanup.rs:1735-1741` keeps only `themes`.
   - Require the data folder's top level to be exactly `themes`.
   - Print any other entry as DIFFERENT, with its name.
2. **Important: on VM-B nothing starts the app before step 5.** See `protocol.md:197`/`:200`: a silent install does not start the app.
   - Add a VM-B line to step 5 that starts the app first, from the Start-menu `CMD Studio.lnk` or phase 3's `start-app.vbs`.
   - Take a screenshot of the wizard's page 1.
3. **Minor, taken: step 5's `data folder:` list is incomplete.** See `protocol.md:200`. Say "at least these", or list the full set that `--cleanup` knows.
4. **Minor, taken: the kept themes are not compared.** Compare the kept themes with the `after` themes, byte for byte. This proves the applied "My Terracotta" came through, not just that a `themes` folder exists.
5. **Minor, taken: "No box" in a silent run proves nothing.** See `protocol.md:197`/`:203`: `/SD IDOK` skips the boxes. Say so in the PASS line, and name the checks that still catch a failure.
6. **Minors 3 and 5 of the review:** deferred in the ledger. Nothing to do.

After the change, parse-check all three `.ps1` files again. The guard total stays as it is unless a script is added.

Append "Fix round 1" to task-9-report.md. Your final message gives only:

- the status;
- the commit;
- a one-line check summary;
- the concerns.

