# Phase 3b-2 recovery mode, the app side: rulings, deferred minors and carries

Every decision the controller made on the founder's behalf while executing `2026-11-03-phase3b-2-recovery-app.md` (subagent-driven, 2026-11-03 to 2026-11-04), in the order made, each with its reason and what it costs if wrong; the founder's own decisions during the run; every minor finding a reviewer raised that this plan did not fix; every item handed on; the VM check's findings. Then the pre-flight conflicts and the controller's rulings files in full, which override the plan's text where they differ. Source: the execution ledger and the rulings files; the git history has the commits and their evidence.

## The founder's decisions during the run (2)

- Founder (2026-11-03): "ใช้ตามที่แผนเขียนทั้ง 3 ข้อ Subagent-driven ลุยต่อเลย" — no test-only crash hook (VM uses run.json fixtures); "Start normally" = app_restart; --recovery on a running app reloads the page; execution subagent-driven.
- Founder (2026-11-04): "อนุญาต" — download cmdstudio-exe of CI run 41691534355 (244a898), about 33 MB, for VM-A and VM-C only. CI run green (windows, app); downloaded to the session's new scratchpad artifact-3b2-244a898.

## Rulings (11)

- Ruling (pre-flight): the architect revises the plan in place for K1-K20, taking the scan's suggested rulings, with these choices: K3 — error_codes.rs names UI-only codes explicitly (E_TIMEOUT), pinned by a test that the TypeScript says it; K8 — no reload after settings_reset, the app adopts the defaults (language included) from state; K12 — the --recovery entry marks busy for its duration with its own start.busy text, so the UI waits and asks app_status again; K13 — app_restart passes a hidden `--after-pid <old>` that the new process's main waits on (up to 10 s) before the single-instance plugin, proved on a VM as spike S4 in Task 1 (no cmd.exe helper: AutoRun); K14 — the renderer give-up calls end_run before exit(1); window_failed leaves the record on purpose (a failed start); K18 — extract the three helpers; K19, K20 as suggested — the scan's evidence holds against the shipped code — cost if wrong: plan text.
- Ruling (carry to Task 11): appendix B's 3b-1 row gets back, in Thai, that a cmd /k long-command window writes no tag (F2) and that a failed start counts only within 10 minutes (FORGET_AFTER_SECS 600, the founder's number) — cost if wrong: none.
- Ruling (carry to Task 5, same file run.rs): fix Minors 1-4 there — end_run leaves an Err read alone; the Busy guard restores the previous value on drop; busy is set before spawn and cleared on a spawn failure, with a recovery_entry note the page can show; a log line for a pid that is there but cannot be opened — cost if wrong: small.
- Ruling: Task 1's VM half (S2 renderer reload, S4 --after-pid) runs with Task 12's VM check on one CI artifact of the final head, not after Task 6 — one download and one VM session; F2/F4 are small fallbacks applied then — cost if wrong: a late fix to webview_watch.rs or main.rs.
- Ruling (Task 8): doctorFix, themeResume and settingsReset keep the read-only deadline — doctor_fix only proposes, theme_resume at cmd writes only health\, settings_reset renames one file — cost if wrong: a retry while one is still finishing.
- Ruling (carry to Task 10, binding): route on safe mode to the Recovery page, and send app_ready only after the Recovery page is drawn; the ErrorBoundary logs a caught render error once through log_ui_error (pass ipc or share a logOnce); add the start.recovering text test — cost if wrong: none.
- Ruling (Task 10): on the Recovery page the banner's Revert/Report buttons are hidden (the page has its own), the notice's Report focuses the inline Report, pickTheme says to choose a theme after leaving recovery mode — cost if wrong: one more key.
- Ruling: the VM runner uses the rebuilt hashes above; the protocol's header gets them in the results commit — cost if wrong: none (same sources).
- Ruling: Task 12's Important and Minors 1-3, the header hashes, and a scratch mlua compile check of refilter.lua go to the final fix pass (protocol text and one script) — cost if wrong: none.
- Final: Ruling: I1 — the --after-pid start runs recover_at_start before the window (a refusal logs run start rollback=<code>) — cost if wrong: a bounded wait at that start.
- Final: Ruling: M7 — the guide's Start-menu shortcut sentence waits for phase 4 (the installer makes the shortcut) — cost if wrong: one sentence.

## Deferred minors (9)

- Final: minor (deferred): recovery-renderer.ps1 throws with a stack trace when the app has gone after the third kill (expected; noisy log).
- Final: minor (deferred): a second launch whose first instance's record is missing counts one failed start (harmless at one).
- Final: minor (deferred): after crash_dismiss the diagnostics copy stops attaching the previous run's crash note while the Report tab still shows it.
- Final: minor (deferred): Backtrace::force_capture in the hook takes std's backtrace lock (the same exposure as Rust's default hook).
- Final: minor (deferred): --after-pid waits on a reused pid for the full 10 s (no creation-time check).
- Final: minor (deferred): logged() writes a cmd=log_ui_error line on every call, past UI_ERRORS_PER_RUN; mock.ts's defaults literal twice; Pause.tsx's nested ternary.
- Final: minor (deferred): a go() backstop for non-recovery routes in safe mode, as protection against a future component.
- Final: minor (deferred): M5 no restart offer during the post-reload busy wait; M6 app_status logs every 2 s poll; M8 the file-dialog bar text; the go() backstop.
- Final: minor (deferred): the entry card says a change was put back when the entry thread could not start (a third key, recovery.entry.notRun); M4 takes a common-word user name out of messages too; end_run runs twice on a restart (idempotent).

## Carries (9)

- Final: carry to the final fix pass: lua.rs's module doc says the cmd /k (F2) point twice (:53-55 and :59-60); drop the repeat.
- Carry (Task 6): app_restart must end the old process through Tauri's exit (or call end_run itself), never a bare process::exit, or the next start shows a false crash notice.
- Task 5: review (fable) — Needs fixes. Important: (1, plan-mandated) the hook's catch_unwind protects nothing (a panic in the hook aborts; probe proved it): the doc's claim is false, safety rests on callees never panicking (checked: none panics); (2) carry 2's Busy restore can leave busy stuck at "recovery_entry" in one interleaving (the reloaded UI waits forever); (3) when the 8.3 alias read fails, redaction() drops every name, so the user's name reaches crash notes and the log. Minors: recover_in_place's spawn failure drops the reload; crash_report_read unbounded; diagnostics stop attaching the note after crash_dismiss; the hook runs for --cleanup too; Backtrace::force_capture's lock.
- Final: carry to Task 7: the UI does not switch on ResumeOutcome kind "notKnown" yet — Task 7 runs npm run typecheck first and handles it (Pause.tsx, mock.ts).
- Final: carry to the final fix pass: spawn_recovery_entry clears its busy_mark on every exit of the thread, a panic included (a drop guard around the entry).
- Final: carry to the final fix pass: app_restart calls b.end_run() before app.exit(0) (end_run is idempotent), so Tauri's process::exit fallback leaves no record.
- Final: carry to the final fix pass: styles.test.tsx:216 regex → /\.working-bar\.empty\s*\{[^}]*display:\s*none/ (as a not-match).
- Final: carry to the final fix pass: spec §5.3 line ~438 "ฝั่งใดฝั่งหนึ่ง"; reword the app_restart Exit sentence once end_run() is called before exit(0).
- Task 12: review (sonnet) — Approved in substance (guards 43/43, S2/S4 true to the code, the run.json fixtures give two failed starts, 24+ words match, refilter.lua sound). Important: step 12's 'Reset the app's settings' breaks the PowerShell quoting. Minors: step 6 miscounts the kills (the box comes on the second); Esc does nothing on MB_YESNO; step 4's wording garbled; recovery-timing's -WithoutCounter copy and write are outside the try. Carry: the header's old stand-in hashes.

## The VM check (2026-11-04, build 244a898, CI run 41691534355)

- Verdict: no FAIL. VM-A 1, 2, 2b, 3, 3b, 4, 5, 6, 9, 12 and VM-C 1, 2, 2b, 3, 3b, 5, 12 (in Thai) PASS; 7, 10, 11 recorded; 2c and 8 pinned by tests (also seen on VM-A). S2 and S4 hold with no fallback. Details: `docs/spikes/phase3b-recovery/vm-check-3b2.md` and `results-3b2.md`.
- F1 (protocol): a fresh data folder's first start logs `crash_check theme=none`, not `crashes=0 paused=false` (the check runs before the wizard applies).
- F2 (protocol and stand-in): the hang stand-in never reaches the apply's verify step, because the runtime step re-checks the bundle's pinned hashes (`E_RUNTIME_MISSING` in 105 ms); the still-working bar and the 60 s deadline were seen through Home's `doctor_run` instead. The 3-minute restart offer was not reachable.
- Observed: a restart writes `run end` twice (already a deferred minor); both refilters come from Clink's own prompt.lua; the health counter's cost is inside the timing noise.

## The rulings files

### Pre-flight conflicts K1-K20 (the plan was revised for them in 5b4e285)

###### E. Conflicts found

| K | Plan line | What is wrong | Suggested ruling |
|---|---|---|---|
| K1 | T5 L1915-1917 (D01) | `std::panic::catch_unwind(\|\| hook_body(&fs, ...))` captures `&FakeFs`; `FakeFs` holds a `RefCell` (`fake.rs:49-54`), so the closure is not `UnwindSafe`: compile error. The test also asserts only "did not panic". | Wrap in `AssertUnwindSafe`; add an assertion that the other behaviour is intact (for example that a later `write_note` on the same `FakeFs` after `stop_failing` succeeds). |
| K2 | T5 L2227, L1800 (P12, N06) | `Backend::redaction()` in `run.rs` calls `self.user_names()`, a private `fn` of `diagnostics.rs:186`; E0624. | Make `user_names` `pub(crate)` in `diagnostics.rs` (it is already a modified file in T5) and list it in Interfaces. Better, build `machine::redaction()` from the same function so there is one rule (also fixes D03). |
| K3 | T3 L773-789, L793 (P10, N35) | `crates/core/tests/error_codes.rs` (`the_codes_of_spec_5_9_and_the_codes_the_code_says_are_the_same`) needs every §5.9 row to be said by Rust production code outside `const MEANINGS`. `E_TIMEOUT` is said only in `MEANINGS` and in TypeScript, so that test fails from Task 3 on, and Step 7's "error_codes ok. 3 passed" is false. | Either make Rust say it (a `pub const TIMEOUT: &str = "E_TIMEOUT"` in `error.rs` outside the table, used by the app where it matters), or change `error_codes.rs` to name UI-only codes explicitly; decide in Task 3 and say which. |
| K4 | T2 L458-485, L557 (P07) | A fifth test relies on tasklist: `health_lua.rs:1235-1255`, `assert_eq!(w.tasklists.len(), 1)` at `:1251`. After Step 3 it fails; the "33 - 4 + 1 = 30" count assumes it does not. Also `lua.rs:50` and `health_lua.rs:57-58` keep tasklist wording. | Add to Step 1: edit that test's second half to `assert!(w.tasklists.is_empty())` (the pause still counts three; the forgotten two are gone) and update the two doc lines; count stays 30. |
| K5 | T6 L2443, L2449 | The m4 test calls `level(&b)`, "the helper the test at :849 uses". At `recovery.rs:849` `level` is a closure local to another test; there is no helper. | Define the closure in the new test (`let level = \|b: &Backend<FakeMachine>\| b.recovery_status().unwrap().paused.unwrap().level;`) and drop the parenthesis. |
| K6 | T7 L2977 (P21, N44) | The mock change `if (paused.level === "computerUnknown") return { kind: "notKnown" }` contradicts the existing `mock.test.ts:444` (`computerUnknown` with a missing theme -> `pickTheme`). The plan neither mentions nor updates it. | Update `mock.test.ts:444` to `notKnown` in T7 Step 4 (or limit the new branch to the case the plan's own test builds); keep the count at 265. |
| K7 | T9 L3892-3898, L3700, L3713 (P27, N50) | `<WorkingBar />` is an always-present `role="status"` region inside `App`. `App.test.tsx:685` (`getAllByRole("status").find(el => el.textContent === "")`) then picks the bar, not the Settings "Saved." region; T9's own test L3700 `findByRole("status")` finds two (bar and `StartWait`). | In T9 Step 1 change test 2 to `findByText("A theme is still being applied...")`, and change `App.test.tsx:685` to pick the region by being inside `main`, or by excluding `aria-label="Still working"`; name both edits in the Files list. |
| K8 | T10 L4150-4151, L3996-4005, L4225 (P42) | After `settingsReset` the page sets `resetTo` and calls `window.location.reload()`: the notice is erased in production, and in jsdom (no reload) the language remains Thai, so the test that waits for the English "The settings were reset..." cannot pass. | Drop the reload; call the app's `adopt(done)` (or a context setter) so the defaults, language included, stand at once; render the notice from that state; test expects Thai text, or sets no language. Or keep the reload and drop `resetTo` and the test line. |
| K9 | T4 L861 (D07) | Unused `use cmdstudio_core::adapters::Fs;` in the `run.rs` test module (all calls are on `&dyn Fs` or inherent `FakeFs` methods): `unused_imports`, fatal under `-D warnings --all-targets`. | Remove the import. |
| K10 | T5 L1959 (D06) | `assert_eq!(d.crash_notice.unwrap().has_report, false, "...")` trips `clippy::bool_assert_comparison` (warn by default), fatal under `-D warnings --all-targets`. | `assert!(!d.crash_notice.unwrap().has_report, "...")`. |
| K11 | T4 L1340-1368, L1659-1667 (P16) | `start_run` runs before the single-instance plugin, which exits a second process only afterwards. A second launch reads the first's record, logs `still-running`, then overwrites `run.json` with its own record (dead pid, `uiReadyAt: null`) and exits. The first process then never deletes it; the next start counts a failed start (two such runs open safe mode). Spec §5.1 says "skip". | In the `alive && old.pid != me.pid` branch return a `StartDecision` that writes nothing and leaves `mine = None` (so `end_run`, `app_ready` and `enter_safe_mode` are no-ops for that process); add a test that the first record is unchanged after the second `start_run`. |
| K12 | T4 L1706-1719, L1561; T9 L3858-3861; T10 L4135 (P17) | On the closed-app `--recovery` path the entry thread finishes after the page's single `app_status`, and nothing marks it busy or tells the page. The page shows no entry and no banner. | Run `recovery_entry` before the window is built (its cost is the rollback, bounded by the lock), or mark `busy` for its duration (a name that is not in `CHANGES_MACHINE`, with its own `start.busy` text) so the UI waits and asks again. State which. |
| K13 | T6 L2555-2561; T7 L2780-2792; L49; T12 L4534 (P15) | `app_restart` spawns the new exe and then `app.exit(0)`. The new process, started before the old mutex is released, forwards its args to the dying old one and exits (`single-instance windows.rs:62-130`); if the old window is already gone it carries on instead. Result is a race; "under a second" is not measured. Also the "Start normally" path of safe mode depends on it. | Prove or fix in a spike before Task 7: let the old process exit first and start the new one from a short-lived helper (for example `cmd.exe /d /c ping -n 2 127.0.0.1 >nul & start "" "<exe>"`), or wait for the old pid inside `main` on a hidden flag; add VM step 2b as a gate. |
| K14 | T6 L2644; T12 L4539 (P23) | The renderer give-up thread ends with `std::process::exit(1)`, which does not raise Tauri's Exit event, so `end_run` does not run and `run.json` stays (with `uiReadyAt` set: the next start shows the crash notice). VM step 6 expects "`run.json` is gone (a normal exit)". | Call `b.end_run()` before `exit(1)` in the give-up path (and decide the same for the `window_failed` path: there leaving the record counts a failed start, which may be intended), or change the PASS text. |
| K15 | L54, L3299, L4381 (P36, D13) | "fourteen" `work.*` task groups; `TASK` has fifteen. | Say fifteen in all three places (or merge two keys). |
| K16 | L25 vs T7 L2922-2934 (P26) | Global constraint lists `MockOptions` `failedStarts`; Task 7 defines none (`safeMode: {kind:"failedStarts", count}` instead). | Edit L25 to the real option names. |
| K17 | T7 L2697 vs L2937; T8, T9, T10 Files lists (P25, P31-P33) | Files lists miss files the steps and `git add` use: `components.test.tsx` (T8), `state.tsx` (T9), `useRead.ts` and `Home.tsx` (T10); T7's Interfaces omit `MockState.ready`. | Add them to the Files lists. |
| K18 | T6 L2463-2480; T5 L2172-2186; T4 L1706-1740 (D02-D04) | Three near-verbatim duplications (message box helper, redaction rule, thread-spawn block). | Extract `console::message_box(text, flags) -> i32` used by `fatal` and `ask_yes_no`; one redaction constructor; one `spawn_recovery_entry(backend, after: impl FnOnce()+Send)` helper. |
| K19 | T9 L3769, L3783 (D11) | A window `error` event with no `Error` object (resource errors, ResizeObserver notices) blanks the app. | Ignore events whose `error` is null and whose target is not `window`, and the ResizeObserver messages; still log them. |
| K20 | various (minor) | (a) T1 L141-142 `windows-core` direct dependency looks unused. (b) T9 L3738 imports `WorkingBar` unused in `start.tsx`. (c) T10 L4054 uses `s` out of scope in `readSettings`. (d) T5 L2271 "before `Ok(out)`": `diagnostics_copy` ends with a redaction fold, so the note must be appended before `let out = redact_sids(...)`. (e) T5 L2285 b2 test is prose and, as described, vacuous. (f) T4 busy flag not exception safe; panic hook also writes notes for caught panics (Report tab shows "Crash note" after a non-fatal panic). (g) T6 `settings_reset` fails when `settings.json` cannot be read, and drops the damaged-file notice. (h) T2 stale tasklist wording at `lua.rs:50`. | Fix in place by the implementer; none needs a new decision except (f) and (g), which the controller should rule on. |

### Task 5: the carries (they override task-5-brief.md)

#### Task 5: carries from Task 4's review (the controller's ruling; they are part of Task 5)

The review of Task 4 (commit 9116f92) approved it, with four Minors in `app/src-tauri/src/backend/run.rs` and `lib.rs`. Task 5 changes the same file anyway, so fix them here. Each fix needs a test that fails first.

1. **`end_run` (run.rs ~:347-364)** deletes when the read comes back `Err`. A record another process is renaming into place, under a sharing violation, would be deleted as if it were ours.
   - Delete only in three cases: no file (`Ok(None)`), a file that does not parse (nobody's record), or a record that is this process's (pid and createdAt).
   - Leave an `Err` read alone, and log one line.
   - Test: a FakeFs read failure on run.json leaves the file.
2. **The `Busy` guard (run.rs ~:413-430)** sets `None` on drop. A `logged()` command that runs while the `--recovery` entry runs in place therefore clears the entry's busy early.
   - On drop, restore the value the guard replaced.
   - Test: an outer guard for "recovery entry", an inner guard for "apply", drop the inner one, and `app_status().busy` still names the entry.
3. **`spawn_recovery_entry` (lib.rs ~:114-128)** marks busy inside the spawned thread.
   - Mark busy before `spawn`, and clear it if the spawn fails.
   - On a spawn failure, record a note the Recovery page can show. Use the `recovery_entry` outcome with the reason: "CMD Studio could not start its recovery step: <error>". Use the existing outcome type if it has a place for a failure; otherwise add the smallest field the plan's later UI tasks can read, and name it in your report.
   - Test what can be tested: a source-text pin of the order mark, spawn, clear-on-failure, as Task 4's tests pin lib.rs.
4. **`start_run` (run.rs ~:166-170)** folds a `process_created_at` `Err` (a pid that is there but cannot be opened) into "gone" without saying so. Log a line of its own for that case. The count stays as it is.

### Task 5: fix round 1

#### Task 5, fix round 1

The review of 92132f0..5918a2e found three Important issues and some Minors. Fix the items below, with TDD where they change behaviour.

Rules:
- Same rules as before.
- The branch head now includes Task 6. Do not change Task 6's code beyond what an item needs.
- One commit, written through `.git/CMDSTUDIO_COMMIT_MSG` (Read, then Write, then `git commit -F` as its own step).
- `git add --` only the intended files.
- No attribution. Never push.

##### 1. Important: the panic hook's doc claims a guarantee it does not have (crash.rs ~:114-150)

Rust aborts at once when the panic hook itself panics, so the `catch_unwind` inside the hook protects nothing. The reviewer proved this with a probe. The hook is safe today only because nothing it calls can panic: `redact`/`redact_sids`, `iso_utc`, `now_ms`, `WinFs`, and the `downcast_ref` chains.

**Fix:**
- Remove the false claim from the doc.
- Say that the hook's safety rests on its callees never panicking, so any change to them must keep that. Name them.
- The `catch_unwind` may stay, as harmless, or go. Say which you chose.

##### 2. Important: the `Busy` restore can leave the entry's busy stuck (run.rs ~:505-544)

`busy_clear` clears unconditionally, and `Busy::drop` restores unconditionally. On the in-place `--recovery` path, this order leaves `busy = Some("recovery_entry")` for the rest of the run, with nothing to clear it:
1. The entry is marked busy.
2. A machine-changing command's guard replaces it with "apply".
3. The entry ends: `busy_clear` sets it to None.
4. The command ends: the guard restores "recovery_entry".

The reloaded UI then waits forever.

**Fix:**
- `Busy::drop` restores only when the current value is its own name.
- `busy_clear(name)` clears only when the current value is `Some(name)`.
- Or use an equivalent that is correct in both orders.

**Tests:** both interleavings. Each must end with `busy` back to `None` once both finish:
- outer mark, inner guard, clear, drop inner;
- inner guard, outer mark, drop inner, clear.

##### 3. Important: when the 8.3 alias cannot be read, the user's name reaches the crash note and the log (run.rs ~:289-296, diagnostics.rs ~:205-237)

`user_names` returns `Err` when `short_path` fails, after it already holds `windows_user_name()` and the profile folder's name. `redaction()` then turns that into no names. So `hook_body` and `redact_for_log` redact only the SID.

**Fix:**
- `redaction()` uses every name that is known even when the alias is not, for example a `user_names_known()` that returns the names it found.
- Keep `diagnostics_copy` refusing on `Err`, as it is.

**Test:** a FakeMachine whose short path fails. A crash note and a `log_ui_error` line holding the Windows user name and the profile folder's name come out redacted.

##### 4. Minor, take it: a failed spawn in `recover_in_place` drops the page reload (lib.rs ~:106-139)

Clone the `after` callback into the thread, and also call it in the spawn-`Err` branch. Then the reloaded Recovery page shows the `skipped` reason. Pin it with a source-text test, as the file's other pins do.

##### 5. Minor, take it: `crash_report_read` has no size bound (run.rs ~:272-289)

Cap what it returns at 256 KiB, and end the text with a "(truncated)" line when it cuts.

**Test:** a 300 KiB note comes back capped and marked.

##### Not in this round (deferred)

- The diagnostics stop attaching the previous run's note after `crash_dismiss`.
- The hook is installed for `--cleanup` too. Add one line about it in `main.rs`'s module doc; that line is in this round.
- `Backtrace::force_capture`'s lock.

##### Checks and report

- Run `cargo fmt --all`, the four clippy commands and `cargo test --workspace`.
- Append a "Fix round 1" section to `task-5-report.md`.
- Your final message gives:
  - the status;
  - the commit;
  - a one-line test summary;
  - the concerns.

### Task 6: fix round 1

#### Task 6, fix round 1

The review of 5918a2e..9b2d5df found one Important issue and some Minors. Fix the following in one commit, under the same rules as before:
- TDD where testable, otherwise a source-text pin as `lib.rs`'s tests do.
- Write the commit message with the Write tool to `.git/CMDSTUDIO_COMMIT_MSG` (Read it first), then run `git commit -F` as a separate step.
- `git add --` the intended files only. No attribution. Never push.

Task 5's fix round may already be committed on the branch. Do not change its code.

1. **Important (plan-mandated).** `webview_watch.rs` (~:831-834 and ~:859) swallows the two real registration failures:
   - `controller.CoreWebView2()` returns `Err` and the code does a bare `return`;
   - the HRESULT of `add_ProcessFailed` is dropped with `let _ =`.

   `watch_renderer` then returns `Ok(())`, and `renderer watch=not_registered` never fires. Fix: give `watch_renderer` a note callback, or the backend clone, and log both failures inside the closure. Use the same `renderer watch=not_registered <why>` line.
2. **Give up once.** A further `ProcessFailed` after the give-up starts a second give-up thread and a second box. Add a once-guard (`AtomicBool::swap(true)`) so there is one box and one `revert_all`. Test the guard, which `Failures` can hold.
3. **The give-up thread spawn (`lib.rs:430`, `let _ =`).** On a spawn failure, `note_line` the error.
4. **The token in `webview_watch.rs` (~:856-859).** Say in the `// SAFETY:` comment that the registration lives for the window's life, and that WebView2 releases its handlers at the controller's `Close()`. The dropped token is a choice.
5. **The pending name in `settings_reset` (`backend/mod.rs` ~:100-123).** When a file is set aside, the earlier damaged file's name is dropped. Write a `note_line` of that name, so it is not lost silently.

Deferred, not in this round: the `--after-pid` creation-time check. The 10 s cap bounds it.

**Checks:** `cargo fmt --all`, the four clippy commands and `cargo test --workspace`. Append a "Fix round 1" section to `task-6-report.md`.

**Final message:** the status, the commit, a one-line test summary and the concerns.

### Task 8: fix round 1

#### Task 8, fix round 1

The review of e75db58..5055da7 found two Important issues and two Minors to take now. Make all of them in one commit, under the same rules as before:
- TDD: watch each new test fail first.
- UI checks, then the commit through `.git/CMDSTUDIO_COMMIT_MSG` (Read it, then Write it, then run `git commit -F` as a separate step).
- `git add --` only the intended files. No attribution. Never push.
- Task 9 has committed the start code; read `App.tsx` and `start.tsx` fresh, and do not change Task 9's behaviour.

1. **Important: the restart has no "restarting" state** (`WorkingBar.tsx` ~:22-30).
   - Set `restarting` before the call. Disable the button and show `work.restarting` in the dialog.
   - Keep it set on an Ok answer and while there is no answer. Clear it only on a rejection, and show the error then.
   - Tests:
     - a second click while it is pending does not call `appRestart` again;
     - a rejection shows the error and enables the button again;
     - an Ok answer keeps "restarting" shown.
2. **Important: the empty status region is `display:none`** (`styles.css` `.working-bar.empty`). A `display:none` live region is out of the accessibility tree, so the change is not announced.
   - Hide it visually instead, with the existing `.sr-only` pattern or zero size, padding and border. Keep it in the tree.
   - Pin it with a test that reads the stylesheet, the way `styles.test.tsx` checks rules: `.working-bar.empty` sets no `display: none`.
3. **Minor: the late-answer test cannot fail** (`work.test.ts`). The first promise has already rejected, so a late resolve changes nothing.
   - Pin the drop with a side effect: `work.get()` shows no pending entry after the late answer, and the late answer raises no unhandled rejection. Or use a counter that the drop path increments.
   - Show that the test fails if the drop logic is removed.
4. **Minor: a synchronous throw leaks a pending entry** (`work.ts` `track`: `begin(p)` runs before `run()`).
   - Call `end(p)` on a synchronous throw.
   - Test: an inner method that throws synchronously leaves nothing pending, and the ticker stops.
   - Also fix the header comment: the file-dialog commands are shown, not hidden. Only `exit` is hidden.

Rulings that stand (no change):
- `doctorFix`, `themeResume` and `settingsReset` keep the read-only deadline. doctor_fix only proposes; theme_resume at the cmd level writes only `health\`; settings_reset renames one file.
- K7, the App test's status lookup, is Task 9's.

Append a "Fix round 1" section to `task-8-report.md`. Your final message gives only:
- the status;
- the commit;
- a one-line test summary;
- the concerns.

### Task 10: the carries (they override task-10-brief.md)

#### Task 10: carries (binding, part of Task 10)

From Task 9's review:
1. **Safe mode routes to the Recovery page.**
   - `App.tsx` reads `safeMode` (`readSettings`'s `_safe`, about :202). When it is true, the first route is the Recovery page.
   - `app_ready` is sent only after the Recovery page is drawn, never after a Home that safe mode should not show.
   - Test: with `MockOptions.safeMode` set, the Recovery page is shown, and `appReady` comes after it.
2. **The ErrorBoundary logs a caught render error once through `log_ui_error`.**
   - The boundary sits outside `IpcProvider`, so either pass `ipc` in as a prop or share a `logOnce` with `watchWindowErrors` (`start.tsx`). Keep the 20-per-load cap.
   - Test: a child that throws on render gives exactly one `logUiError` call.
3. **One test** for the `start.recovering` text while `busy === "recovery_entry"`.

From the plan's own scope, already in the brief: m4's words. `resume.notKnown` exists since Task 7, so reuse it and do not add a second key.

### Task 10: fix round 1

#### Task 10, fix round 1

The review of e26b57a..5382dfd found one Important issue and some Minors. The controller's visual check passed on what it saw.

Fix all items below in one commit, under the same rules as before:
- TDD;
- the UI checks, then the commit through `.git/CMDSTUDIO_COMMIT_MSG` (Read, Write, then `git commit -F` as a separate step);
- `git add --` with only the intended files;
- no attribution, never push;
- **never write a file outside the repo and the session scratchpad, not even from a script.**

Task 11 may have committed spec text on the branch. Do not touch `docs/`.

1. **Important: safe mode must stay on the Recovery page.** Today two buttons on it open the full Shell, where Home, Themes and Settings load the library:
   - `PausedBanner`'s "Report a problem" and "Undo everything" (`Pause.tsx` ~:83-87, rendered at `Recovery.tsx` ~:1125);
   - `CrashClosed`'s "Report a problem" (`App.tsx` ~:458).

   `Pause.tsx` ~:293 (`pickTheme` → `go({name:"themes"})`) is a third way out. The fix:
   - On the Recovery page, hide the banner's Revert and Report buttons. The page has its own inline Undo everything and Report.
   - Give `CrashClosed` an `onReport` that, on the recovery route, moves focus to the inline Report section (scroll it into view) instead of navigating.
   - In recovery mode, `pickTheme` says so in words instead of going to Themes: "Choose a theme after you leave recovery mode." / th "เลือกธีมหลังออกจากโหมดกู้คืน", under a new key.
   - Optionally, make `go` a no-op for non-recovery routes while in safe mode, as a backstop. Say whether you did.
   - Tests: in safe mode,
     - clicking the notice's "Report a problem" leaves the Recovery heading on screen, with no sidebar;
     - the banner on the Recovery page has no Revert or Report button;
     - a `pickTheme` outcome shows the new words and does not navigate.
2. **Minor: a dismissed crash notice comes back.** `CrashClosed` keeps `closed` in local state, so it reappears when `StartNotices` remounts, for example from the wizard to Home. Hold "dismissed" in `AppState`, or set `appStatus.crashNotice` to null on Close. Test: Close, then a route change, and the notice is not shown again.
3. **Minor: duplicate and contradictory reset notices.** After "Reset the app's settings", `StartNotices` also shows `settings.reset` ("could not be read and were reset"), which is wrong when the user chose the reset. Keep only the Recovery page's notice. Test it.
4. **Minor: the Thai house word.** `recovery.reset.text` and `recovery.reset.confirm` use ค่าตั้งต้น. The existing `settings.reset` uses ค่าเริ่มต้น. Use ค่าเริ่มต้น and update the test that quotes it.
5. **Minor: the safe-mode ready test.** Make it fail if `appReady` fires before the Recovery page is drawn. Record the order of the calls and the render, or assert that `appReady` is absent at the moment the first Recovery render commits.
6. **Minor: the reset result's status.** Render an always-present `<div role="status">` and put the "settings were reset" text inside it.

Append a "Fix round 1" section to `task-10-report.md`. Your final message gives only:
- the status;
- the commit;
- a one-line test summary;
- the concerns.

### The final fix pass

#### Plan 3b-2: the final fix pass (one pass)

Read `final-review-report.md` in this folder first. It is the final whole-branch review: "With fixes", Critical 0, Important 2, Minor 8.

Fix the items below in this pass.

**Rules for every item:**
- TDD: write the test first and watch it fail.
- Rust production code has no unwrap, expect, panic or indexing.
- Commits:
  - English messages. Read `.git/CMDSTUDIO_COMMIT_MSG`, Write it, then run `git commit -F` as a separate step.
  - `git add --` with the intended files only.
  - No AI attribution. Never push.
- PC safety: binding (`pc-safety.md`):
  - never run the app, a VM script, a Lua probe or a binary;
  - never read this PC's event log, registry or windows;
  - **never write any file outside the repo and the session scratchpad, not even from a script or a redirect**;
  - never pass `-ExecutionPolicy`.
- Several commits are fine, one per area.

##### Important

1. **I1: restart rollback.** On the `--after-pid` start only, run `recover_at_start` before the window.
   - It takes the lock itself. Run it on the crash-check thread, or before the builder if that is simpler and bounded.
   - A refusal is one log line: `run start rollback=<code>`.
   - Then the restart dialog's promise holds. Keep `work.restartConfirm`'s words.
   - Test the path through the backend seam that `--recovery`'s entry uses.
2. **I2: "Try again" for `E_TIMEOUT`.**
   - `ErrorNotice` gets an optional `onRetry`, which renders a `common.retry` button.
   - Home, Help's tabs and Recovery pass `bump`.
   - Test on Home: a `themesList` that times out shows Try again, and pressing it reads again and shows the second answer.
3. **The busy mark (re-graded to Important).** `spawn_recovery_entry` clears its `busy_mark` on every exit of the thread, a panic included. Use a drop guard around the entry. Test it with an entry that panics.
4. **`end_run` before exit (re-graded to Important).** `app_restart` calls `b.end_run()` before `app.exit(0)`. `end_run` is idempotent. Pin it with a source-text test.

##### Minor, taken now

5. **M1.** `start.busy` names the change that runs. Use one key, `start.busyWith`, with `{task}` from `work.ts`'s `TASK` mapping (backend snake_case to the method). Words in en and th.
6. **M2.** In `host.rs`, a pid whose process has exited but is still held open is not "running". Use the exit time from `GetProcessTimes`, or `GetExitCodeProcess` with `STILL_ACTIVE`. Test it through the fake, if the adapter seam allows.
7. **M3.** `recovery.entry.did` gets two sentences: one for a pause, one for `skipped`. Words in en and th.
8. **M4: privacy.** The redaction takes the user's name out wherever it stands, not only after `\` or `/`.
   - Keep the rule safe for short or common names. A name shorter than 3 characters gets path context only. Say so in the doc.
   - Test: `user <name>`, `name@host`, and `USERNAME=<name>` come out redacted.
9. **M7.** Hold the Start-menu-shortcut sentence of `help.guide.cmdWontOpen` until phase 4. Remove that sentence now, in en and th, and record it in appendix B's 3b-2 row as phase 4's.

##### Carries recorded in progress.md

10. **lua.rs's module doc** says the `cmd /k` (F2) point twice (:53-55 and :59-60). Drop the repeat. It is a Rust doc comment, so the goldens do not change. Check that.
11. **The `styles.test.tsx` regex** at ~:216 becomes `/\.working-bar\.empty\s*\{[^}]*display:\s*none/`, asserted as a non-match.
12. **The spec.**
    - §5.3 ~:438 reads "ฝั่งใดฝั่งหนึ่ง".
    - Reword the `app_restart` Exit sentence, since `end_run()` now runs before `exit(0)`.
    - Add I1's rollback to the §5.7 or appendix B text.
13. **The VM protocol** (`docs/spikes/phase3b-recovery/vm-check-3b2.md`):
    - step 12's `'Reset the app''s settings'` quoting;
    - step 6: the box comes on the second kill, and the script ends with an error;
    - press No with Alt+N, or Tab then Enter, not Esc;
    - step 4's garbled wording;
    - the header's stand-in hashes become `crash-dll\clink_dll_x64.dll` 8F65FDBA…979B (103,424 B), `vmrun\fake\cmd.exe` 047173B5…7B5E (227,328 B) and `hang\oh-my-posh.exe` F88FC050…2719 (225,792 B). Take the full hashes from `Get-FileHash` on the scratchpad files; reading them is fine.
    - Add a step for I1: after a restart that cut a change off, the next start's log has the rollback line.
14. **`recovery-timing.ps1`:** move `-WithoutCounter`'s backup copy and write inside the `try`.
15. **`refilter.lua`:** compile-check it in scratch with mlua, as the plan's Task 12 brief Step 5 describes. Do this in a scratch worktree with its own `CARGO_TARGET_DIR`. Report the result.

##### Deferred (ledger only, not in this pass)

- M5: no restart offer during the post-reload busy wait.
- M6: `app_status` logs every poll.
- M8: the file-dialog bar text.
- The `go()` backstop.

##### Checks before the report

- `cargo fmt --all`, the four clippy commands, `cargo test --workspace` (parity runs).
- In `app\`: `npm run typecheck`, `npm test` and `TZ=UTC npm test`, `npm run build`.
- From the repo root: `bash scripts/check-dist.sh`.
- Parse-check every changed `.ps1`. The guard total stays 43/43 unless a script was added.
- Regenerate bindings by the manual steps if any change.

##### Report

Write `final-fix-report.md`. For each item give the RED and GREEN output and the commit, then the suite totals and the concerns.

Your final message, under 15 lines, gives:
- the status;
- the commits;
- a one-line test summary;
- the concerns.

