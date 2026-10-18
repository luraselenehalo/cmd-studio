# Phase 3b-1 recovery mode, the cmd side: rulings, deferred minors and carries

Every decision the controller made on the founder's behalf while executing `2026-10-17-phase3b-1-recovery-cmd.md` (subagent-driven, 2026-10-17 to 2026-10-18), in the order made, each with its reason and what it costs if wrong; the founder's own decisions during the run; every minor finding a reviewer raised that this plan did not fix; every item handed on. Then the controller's rulings files in full, which override the plan's text where they differ. Source: the execution ledger and the rulings files; the git history has the commits and their evidence.

## The founder's decisions during the run (2)

- Founder (2026-10-18): "โอเค 10 นาทีได้ ทำต่อเลย" — FORGET_AFTER_SECS 600 confirmed.
- Founder (2026-10-18): "อนุญาต" — download the artifact cmdstudio-exe of CI run 39137518009 (8f08c72), about 33 MB, for VM-A and VM-C only.

## Rulings (57)

- Ruling: branch phase3b-1 is cut from phase3-ui 04e36f6, not from main — phase 3 (PR #9) waits for its VM check; the 3b-1 PR opens after #9 merges, with main merged in first — cost if wrong: a merge of later phase 3 fixes into phase3b-1.
- Ruling: tasks with no spike dependency (the plan's spike table: Task 2 and others it lists none for) may run before Task 1; Tasks 3 and 4 wait for Task 1's results.md — the host was locked when execution began and the VMs serve phase 3's check first — cost if wrong: rework of a task whose interface a spike result changes.
- Pre-flight scan (opus): preflight.md (a) 10, (b) 26, (c) 12, (d) 8, (e) 12 rows; nothing blocks Task 2. Rulings recorded in rulings-by-task.md: branch/prerequisites (a5); parity test stays allowed under phase 2a R3 (d1); UI totals 212→216→224 (a1-a3); Task 7 filter 23 (a4); spike crates' Cargo.lock staged (d2); F5/F9 extended if they trigger (d7, b4). Order: Tasks 2 and 6 first; the rest after Task 1's results.
- Ruling: re-graded minor 2 (clear_pause order) to Important — the user's "Turn back on" could fail half-way and pause again at once — fix round 1 takes it with minor 1 and write_pause stamping PAUSED_VERSION — cost if wrong: none.
- Ruling: re-graded minor 1 (the failure line's advice) to Important — following it can pause the next window at once — fix round 1 takes 1-4 — cost if wrong: none.
- Ruling (carry to Task 11): spec §5.7 gains one sentence: an apply that committed but could not lift the pause exits 0 and prints one E_DATA_WRITE line on stderr saying what to do — cost if wrong: the spec is silent on a shipped behaviour.
- Ruling: Task 4 split into 4a (now; synthetic events, no recorded fixtures) and 4b (after Task 1: fixtures, the fixture test, F8/F9 if needed) — rulings-by-task.md 'Task 4' — cost if wrong: 4b reworks eventlog.rs only. Task 4a: dispatched (sonnet, BASE 268c931).
- Ruling: fix round 1 takes Important 1-2 and minors 3, 5, 6 (single-flight), 7, 8 — they decide whether the read keeps spec's 5 s and whether CI exercises the unsafe render — cost if wrong: none.
- Ruling (carry to 4b): MOST_EVENTS counting every program's crashes — add `and EventData[Data='cmd.exe']` to the query only if S10 shows the service accepts it; otherwise raise the cap — cost if wrong: a crash-looping other program can hide cmd's crashes.
- Ruling (carry to Tasks 5 and 7): D12 and the start's crash check never run through library.rs's DiskMachine (it holds a real WinEventLog with no gate); their tests use FakeMachine — cost if wrong: a test reads this PC's log.
- Ruling (carry to 4b): the query's lower bound drifts by the time between now_ms and the query (timediff counts from the service's clock) — use an absolute @SystemTime >= bound or widen by the drift, together with S10/F9; F9's text in the plan names the old test, use the_query_asks_for_application_error_1000_since_the_time — cost if wrong: events in the first moments after the apply missed.
- Ruling: fix round 1 takes Important 1-2 and minors (report filter, timing order, config comment + a dumpbin check note) — they decide whether a VM cycle gives answers or silence — cost if wrong: none.
- PC-safety deviation (re-reviewer): ran one read-only Get-WinEvent query against this PC's Application log to confirm PowerShell 5.1's error shapes (result: no matching events; nothing saved or sent). Ruling: pc-safety.md now says no agent reads this PC's event log, registry or windows for a check — confirm such behaviour on a VM or from documentation — cost if wrong: none. Reported to the founder.
- Ruling (S6): F5 (clear the tag when the first edit line ends) is rejected — a window opened and closed without typing would count as a failed start, so three such windows pause the theme falsely. New design to confirm on a VM (S6b): onbeginedit registers clink.addcoroutine; Clink resumes coroutines only while the edit loop idles, i.e. after every prompt filter (OMP's included) returned and the prompt is drawn; the coroutine deletes the tag on its first run; a hanging filter keeps the tag — cost if wrong: Task 3 waits for another probe.
- Ruling (S11): F10 — the fake cmd.exe loads the stand-in itself to produce event 1000 (en and th) for Task 4b's fixtures.
- Ruling: spike round 2 dispatched to the VM runner before the probe's re-review returned — the host was unlocked (VM-C is reachable only then); if the re-review finds an Important issue, the affected runs repeat on VM-A's cold-boot path — cost if wrong: one VM-A rerun.
- Ruling: the scratch mlua compile check of the spike Lua (compile only, the chunk never called) is accepted as the Lua counterpart of ParseFile — mlua is already the project's dev-dependency for running generated Lua in tests — cost if wrong: none.
- Ruling: if both routes run correctly on the VMs, Task 3 uses clink.promptcoroutine (documented) and not clink.addcoroutine (undocumented) — cost if wrong: none.
- Rulings after the spike recorded in rulings-by-task.md ('After the spike' — the coroutine first-prompt design for Task 3; Task 4b's fixtures, the cmd.exe query variant and the lower-bound margin). Task 4b dispatched (sonnet, BASE f00c42f).
- Ruling: drop the Data filter (back to the base query S10 verified), raise MOST_EVENTS from 200 to 1000, and keep the case-insensitive match in is_clink_crash — a silent miss of real crashes is worse than reading more events inside the 5 s deadline (a timeout is "could not be read", the safe direction) — cost if wrong: a program crash-looping over 1000 times in 7 days hides cmd's crashes until the Recovery shortcut (3b-2).
- Ruling (Task 3 concern 1, type-ahead): set the "first line done" flag inside the coroutine after it ran, not at the call — a line that ends before Clink idles (keys waiting, a paste with Enter) drops the coroutine, and the next line must try again — cost if wrong: none (every pinned test stays).
- Ruling (concern 2): with no clink.promptcoroutine a window writes no tag and counts nothing; a raise from promptcoroutine or from registering the filter logs one line and removes only the window's own tag — confirmed: a tag nothing can clear would pause falsely.
- Ruling (concern 3): priority math.min(0, zl) accepted.
- Ruling: Task 3 fix round 1 takes the type-ahead flag, the §13 pin tests, pcall around the paused callback, Thai health paths in the Lua tests, the doc path — and a liveness check before pausing: when PAUSE_AFTER or more stale tags are found, one io.popen("tasklist /FO CSV /NH 2>nul") lists processes once; a stale tag whose pid is still a running cmd.exe is not counted (a cmd /k <long command> window or a window still open); if tasklist gives nothing, count as before — false pauses from long-running cmd /k windows are a real developer case; the check runs only in the rare pause decision — cost if wrong: a hung window left open is counted only once closed.
- Ruling (carry to Task 7): every committed apply from the app calls recovery::clear_pause (which makes health\), as the CLI's does, so the counter can write its tags on a fresh data folder.
- Ruling (carry to Task 12): on a VM, check (a) Oh My Posh's filter raising at run time still lets the prompt coroutine run (no tag left); (b) three cmd /k "ping -t localhost" windows plus a fourth window after 60 s do not pause; (c) the tasklist call's cost at the pause decision.
- Ruling: re-graded the bare tasklist name to Important (security: a tasklist.bat in the folder a user opens cmd in would run at the pause decision) — fix round 2: an absolute, quoted %SystemRoot%\System32\tasklist.exe — cost if wrong: none.
- Ruling: Task 7's apply note keeps Task 6's corrected wording — the brief's text was the CLI's first line, found untrue by Task 6's review — cost if wrong: one string.
- Ruling: fix the Important now — the user would see "paused because cmd crashed" while cmd keeps crashing — paused_now only when the revert ended ok and the record was saved; the refusal or rollback carried to the UI; a by:app record counts as this computer's pause only while state.applied is None, so the next start retries — cost if wrong: one more Revert everything attempt per start while it keeps failing.
- Ruling: pause_machine writes paused.json first, then reverts (crash-safe; core revert_all never touches it) — cost if wrong: none.
- Ruling: undo and restore lift a pause in cmd when no theme is left (as revert_all does); they never lift a machine pause — cost if wrong: none.
- Ruling (carry to Task 10): a pause record with count 0 (crash reports unreadable) never shows "0 times" in the banner.
- Ruling: fix round 2 — the status and D12 say a pause of this computer is in effect only while the theme is out, "not yet … tries again at the next start" otherwise; a "Pause the theme" press that did not go through says so in its result — a status that claims a pause that did not happen takes control from the user (COE) — cost if wrong: none.
- Ruling: accept the start retry of any app record (it makes "tries again at the next start" true and completes a refused "Pause the theme" press) and the "not known" Thai text — cost if wrong: one Revert everything attempt per start while it keeps failing.
- Ruling: fix round 3 (last) — D12's main messages for computerNotYet and computerUnknown, and the unreadable-record text, say what is true — cost if wrong: none.
- Ruling: fix round 4 with a fresh implementer on a more capable model (fable), per the fix-loop rule (rounds ≥4): an app record is "not yet" (retried) only when its at_ms ≥ the current apply's applied_at_ms, else it is a pause in cmd; the not-yet words say paused in cmd only, retried the next time the CMD Studio app starts; D12's count words (no 0, "once"); the PauseLevel::Cmd doc — brief task-7-fix-4.md — cost if wrong: none.
- Ruling (carry to Task 10): a retried user or recovery pause comes back as CrashCheck::Read count 0; the UI words the banner from status.paused.reason, never "paused for cmd's crashes".
- Ruling: accept theme_resume by level (a Cmd-level pause, including an older app record, is lifted at once — the theme is already applied) — cost if wrong: one match arm.
- Ruling (carry to Task 10): for an older app record the status is by:app, level:cmd with resume still filled — the banner goes by level and offers the lift, not the noted theme.
- Ruling (carry to Task 8, a separate docs commit): fix the three inverted comments and regenerate PauseInfo.ts — the Task 10 implementer reads that binding — cost if wrong: none.
- Ruling (carry to Tasks 9 and 10): the UI never polls recovery_status — each call can hold a worker up to 30 s; call it once on Home's load (and again only after a pause/resume or an apply), never on a timer.
- Ruling (carry to Task 11): update capabilities/default.json's description (the three new commands) and config_guard.rs:3's "27" to 30, with the spec's §5.7/§5.10 counts — cost if wrong: none.
- Ruling (Task 10): task-10-rulings.md R1-R8 override the brief — fixtures get level; the banner's reason line from reason (clinkCrash by count: 2+, once, no count) and reach line from level/by (cmd-clink, cmd-older app, computer, notYet, unknown; unreadable record: only removes the note); pause headline by pausedNow (done, notOk, notSaved, failed, notYet; pause.refused dropped); Home shows a start pause that did not go through (startTried + PauseResult); the reread keys on the start check's committed revert; +9 tests; phase 3 carries (display: contents, button overflow-wrap) as a second commit — the brief predates Task 7's bindings and the not-yet words — cost if wrong: words and one condition to change.
- Ruling (Task 10): the not-yet banner keeps the title 'The theme is paused' — it IS paused in cmd (init.lua loads no theme while the record is there), and the reach line says 'in cmd only for now, not yet on this computer'; the carry's 'never paused' meant never claiming the computer-level pause — cost if wrong: one title key.
- Ruling (Task 10): R3 corrected — pausedNow → done; notSaved → notSaved if committed else failed; committed (ok false) → notOk; else notYet. The mock gains failPause "partial", and R4's card also shows a partial start pause at level computer — cost if wrong: one condition.
- Ruling (Task 10): the start card hides for the run once the user pauses or resumes (an AppState flag, not a module variable) — cost if wrong: a card hidden one action early.
- Ruling (Task 11): task-11-rulings.md R1-R6 override the brief — neither the brief's text nor F5/F8 shipped; the spec says what shipped (promptcoroutine before OMP, the tasklist liveness check, PauseLevel and the not-yet retry, the XPath without a Data filter and the refilter, the 1000/5 s/single-flight read, theme_pause's record-first order and pausedNow, resume by level, the undo/restore lift, the CLI's failed-lift line); Task 8's carries (capabilities description, config_guard 30) ride in the same commit — cost if wrong: a docs edit.
- Ruling (Task 12): task-12-rulings.md R0-R7 override the brief — Steps 1-7 now, Step 8 (VM run) after the final review and its fix pass on the head CI builds; the crash by F10 (S11 failed); the cost by the onbeginedit marker (S8); new steps: a filter raising at priority 1, three live ping windows (tasklist) + the closed-before-prompt observation, cmd /c never counts, tasklist's cost, a 1000-event read, the refilter's cost; PASS words copied from the shipped code; the guard test's total recounted — cost if wrong: VM time.
- Ruling (Task 12, sent to its implementer mid-task): R4(b)'s PASS — after the fourth window's prompt, health\ lists no tag (the ping tags, older than 60 s, are deleted by first_prompt); the fifth window after closing the pings does not pause — cost if wrong: one PASS line.
- Ruling (Task 3 code stands): a window that reaches its prompt deleting older live tags is the "in a row" reset; nothing counts a live window either way — cost if wrong: none found.
- Ruling (Task 12): the open nit and the two minors go to the final fix pass (comments and protocol text only), not a second round — cost if wrong: none (no PASS line changes).
- Final: Ruling: I1 — init.lua counts a failed start only within 10 minutes (FORGET_AFTER_SECS 600): older tags are deleted and never counted, before tasklist — a hang is retried within minutes, a dev-server window lives longer; missing a late failure is the safe direction (the user can press Pause) — cost if wrong: one constant (the founder's number).
- Final: Ruling: m2 — a second read waits for the reader within its own 5 s instead of failing at once — cost if wrong: a D12 run up to 5 s slower while the start's check reads.
- Final: Ruling: the liveness check's narrowing of criterion 1 stands (counting live hung windows would pause on every long cmd /k); told to the founder, with VM step 3's observation — cost if wrong: a founder decision to revisit.
- Ruling (Task 12 Step 8): screenshots stay in spikes/vm/tmp (git-ignored) and are cited by name; only the text outputs go beside vm-check.md — as phase 3 did; repo size and no host pixels in git — cost if wrong: the founder asks for images to be added.
- Ruling (F2): the Rust doc comments (lua.rs, recovery.rs) and appendix B say that a cmd /k window writes no tag and what the liveness check really covers; the Lua comment inside HEALTH_BODY (lua.rs ~:148) is left as is so the merged init.lua stays byte-equal to the VM-checked one (goldens unchanged) — cost if wrong: one stale comment in init.lua until 3b-2.
- Ruling (F2, criterion 1): 3b-1 merges with the liveness check as verified; whether to drop it (so the fourth window pauses even while hung windows stay open, and the 0.6 s tasklist goes) is the founder's call for 3b-2, recommended: drop it — cost if wrong: criterion 1 holds only once hung windows are closed until 3b-2.

## Deferred minors (34)

- Final: minor (deferred): the round-trip test of paused.json does not assert "resume": null or a bare-number "at".
- Final: minor (deferred → Task 7): an unreadable paused.json (newer version, unknown enum) reads as a pause in cmd with no resume, so "Turn back on" drops a stored theme; Task 7's UI should say "unreadable" distinctly.
- Final: minor (deferred): utc::parse accepts 2026-02-31, an empty fraction and unpadded fields; harmless for Windows event text.
- Final: minor (deferred): PauseRecord.version and its struct doc do not say write_pause stamps PAUSED_VERSION over the caller's value.
- Final: minor (deferred): the CLI lifts the pause after run_apply let go of the lock — a race with the app's own pause needs a click within milliseconds of a CLI apply.
- Final: minor (deferred): clear_pause failing only at create_dir_all makes the CLI say the start records "could not be cleared" when they were.
- Final: minor (deferred): no test pins "every EVT handle closed once, also on error" (drain's tests use plain u32); EvtNext returning FALSE with events would leak them, unconfirmed by the docs.
- Final: minor (deferred): the single-flight ordering tests are probabilistic (5000 reads; one panic sample); a finish(slot, answer, give) refactor would make them deterministic.
- Final: minor (deferred): with prompt.async off in CMD Studio's Clink profile, the coroutine runs at once and hangs go uncounted (no false pause); writing prompt.async = true into clink_settings would pin it (changes goldens).
- Final: minor (deferred): eventlog.rs:23's doc states the AppName case rule as fact; the evidence is os.gethost's host= field, not an event's AppName.
- Final: minor (deferred → Task 12 measures): the tasklist read has no timeout; a hung tasklist would leave that start uncounted.
- Final: minor (deferred): D12 details pin only the cmd-level pause; the app-level, unknown-reason and missing-since lines and a 1-crash warn are untested.
- Final: minor (deferred): D12's details list one line per crash (up to MOST_EVENTS 1000); a crash loop gives a long block in Help and the diagnostics — show the newest N plus "and K more".
- Final: minor (deferred): the single-flight guard's text is hand-typed in doctor's test and the paused_check fix sentence repeats in five arms (shared consts would be tidier).
- Final: minor (deferred): paused.json and the apply's lift are written outside the lock (a concurrent CLI apply in the gap), as accepted for the CLI; a core entry point writing under revert_all's lock would cure it (3b-2's --recovery would reuse it).
- Final: minor (deferred): any find_theme error counts as the theme being gone (PickTheme), the brief's choice.
- Final: minor (deferred): a revert that commits with a failed step (WT settings unreadable, AutoRun not text, a font left) reads as a pause in effect and D12 says "CMD Studio took its changes out"; the press's revert.steps says what is left.
- Final: minor (deferred): undo and restore delete an unreadable paused.json together with any theme it noted (as revert_all does; Task 2's carry).
- Final: minor (deferred): each start whose retried revert rolls back adds a revertAll restore point until a later commit prunes them.
- Final: minor (deferred): apply A, a refused pause at T2, apply B whose lift fails, undo B → A (older than T2) makes the record "not yet" again and the next start reverts A; very rare.
- Final: minor (deferred): the live crash count's warn in D12 still says "1 time(s)".
- Final: minor (deferred): Undo everything lifts only by:clink records; after an unlifted older app record it leaves a Computer-level record naming a theme the user did not ask to restore (Task 10's banner goes by level).
- Final: minor (deferred): the start wiring's mark-before-spawn order is pinned only by source text; a #[cfg(test)] CrashSlot accessor would make it behavioural.
- Final: minor (deferred): the mock's D12 rows have no since: line and no crash list in details.
- Final: minor (deferred): no committed test pins the mock's D12 words to doctor.rs (only some rows have exact asserts).
- Final: minor (deferred): mock.ts grew to ~1000 lines; its D12 word functions could move to ipc/mockPause.ts.
- Final: minor (deferred): the mock's D12 counts cmdCrashes while crashCheck carries its own count (two sources for one number).
- Final: minor (deferred): in the stacked editor the preview error's <summary> (and a browser-focusable scroller) is lifted above the form but reached by Tab after it; the comment is corrected to say so.
- Final: minor (deferred): Home rereads once on every mount when the start check committed a revert and a theme was applied later in the run (R5's once per visit).
- Final: minor (deferred): the start card hides once the user resumes even if the resume then fails (the banner still says the true state).
- Final: minor (deferred): the no-Data-filter rationale is written in both D12's row and appendix B (plan-mandated), so the two can drift.
- Final: minor (deferred): theme_resume at computerUnknown ends in the planner's error (state.json unreadable) with no way out named.
- Final: minor (deferred): at app start, Help's D12 resume can run before recovery_status answers, so lifting an unreadable record there prints "The theme is back on" (the null fallback).
- Final: minor (deferred): the reader's last-sleep overshoot gives "did not answer within 5s" rather than STILL_BEING_READ (same meaning to every caller).

## Carries (6)

- Carry (plan 3b-2): when cleanup::transaction runs no transaction (status null) and a step failed with the record saved, the headline says notYet while the level is computer — unreachable from the UI (a theme is applied, so a transaction runs), but 3b-2's --recovery pause path must decide it (treat "applied: None saved" as out, not status === committed).
- Final: carry to the final fix pass: D12's E_CMD_CRASHED message says "cmd crashed in its add-on {n} time(s)" (doctor.rs:1167) — use the crashed_times words ("once", "{n} times") as the pause rows do; then update vm-check.md step 6's quote.
- Task 11: re-review (sonnet) of fix round 1 — all addressed, no new breakage; scope 3 files +6/-5; recovery spec and §5.9 untouched. Out of scope: lua.rs:49-51's doc comment says live tags are "neither counted nor deleted" without the first_prompt deletion (code comment, carry to the final fix pass).
- Final: carry to the final fix pass: lua.rs:49-51's comment — live tags are kept only at the pause decision; a window's first prompt deletes every tag older than 60 s.
- Final: carry to the final fix pass: recovery-ping.ps1 — pass '"ping -t localhost"' quoted or fix the comment; recovery-timing.ps1's header names median/min/max; vm-check.md says to read the newest stamp of $appLog's lines.
- Carry (plan 3b-2): the liveness check's fate (F2); the HEALTH_BODY comment at lua.rs ~:148; a same-boot A/B timing with and without HEALTH_BODY (F4); F3's second refilter (which one, and whether it matters); m4 (theme_resume at computerUnknown).

## The rulings files

### By task (pre-flight, the spike and every task)

### Controller rulings for plan 3b-1, by task

Binding; they override the plan text. The pre-flight scan is `preflight.md` here, sections (a)-(e), 68 rows. Ground truth: the shipped code at 04e36f6 (phase3-ui). The scan applied the plan's UI code verbatim at 04e36f6 (typecheck clean, 224 tests) and built its Rust (core lib 750, health_lua 17, banner_lua 31, golden 21, english_in_cmd 2, error_codes 3, CLI lib 50, app lib 210; fmt and four clippy clean).

#### All tasks

- **Branch and prerequisites (a5).** The work happens on branch `phase3b-1`, cut from `phase3-ui` 04e36f6; the plan's "phase 3 merged" prerequisite is superseded by the ledger ruling, and the 3b-1 PR opens only after PR #9 merges. "Task 1 before Task 2" is superseded: Tasks 2 and 6 have no spike dependency (preflight (e)) and run first; Tasks 3, 4 and everything behind Task 4 (5, 7, 8, 9, 10) wait for Task 1's `docs/spikes/phase3b-recovery/results.md`; Task 11 also depends on S1 and S4.
- **parity (d1).** `cargo test -p cmdstudio-core` and `cargo test --workspace` run `crates/core/tests/parity.rs`, which starts the pinned `oh-my-posh.exe` from `resources\`. Phase 2a's ruling R3 allowed that test and every phase since has run it; it stays allowed. "Never run Oh My Posh yourself" means by hand. Never set `CMDSTUDIO_ALLOW_PARITY_SKIP`.
- **UI test totals (a1-a3).** The base at 04e36f6 is 212 UI tests, not 206: Task 9 ends at 216 (the plan says 210) and Task 10 at 224 (the plan says 218; also the plan's line 13). Use the measured numbers.

#### Task 1 (controller, VMs)

- **d2.** Stage the `Cargo.lock` files that the crash-dll, hang and fake-cmd spike crates create (precedent: `spikes/jsonc-cst/Cargo.lock` is tracked).
- **d7, b4.** If fallback F9 triggers, it also covers `recovery-crash.ps1:741` and step 4 of `vm-check.md`; if F5 triggers, steps 2, 3b and 5 of `vm-check.md` need one Enter per window. The controller extends them when the spike says so.

#### Task 7

- **a4.** Step 7's filter `backend::recovery::` reports 23 passed, not 17: six ts-rs `export_bindings_*` tests match it too.

#### Task 11

- **Carry from Task 6:** spec §5.7 says that an apply that committed but could not lift the pause still exits 0, prints the "Applied" line, and prints one English `E_DATA_WRITE` line on stderr saying what to do (delete the start records in `health\` or turn the theme back on in CMD Studio).

#### Task 4 (split: 4a now, 4b after Task 1)

- **4a (now):** Steps 1 and 3-9 without the recorded fixtures. Step 2 (copy Task 1's `event1000-en.xml`/`event1000-th.xml`) and the test `reads_the_events_the_spike_recorded_on_english_and_thai_windows` are left out; every other test of eventlog.rs (the synthetic events of the plan, named and positional fields) and of the adapter stays. `parse_crash` already reads named fields and falls back to positions (F8 built in).
- **4b (after Task 1's results.md):** copy the two recorded XML files byte for byte (Step 2), add the fixture test, and apply F8 (field names) or F9 (the query's `timediff`) only if S9 or S10 say so. 4b gets its own review.
- **Why:** the VMs serve phase 3's VM check first; Tasks 5, 7, 8, 9 and 10 consume Task 4's interfaces (`EventLog`, `AppCrash`, `eventlog::clink_crashes`, `System.event_log`), not its field names — cost if wrong: 4b changes eventlog.rs's parser or query and their tests; downstream tasks unchanged.
- The real-log test `this_computers_application_log_answers_within_its_time` stays gated by `CMDSTUDIO_SYSTEM_TESTS=1`, which is never set on this PC; it prints SKIPPED here.

#### Task 4b additions (from 4a's review)

- If S10 shows the service accepts it, add `and EventData[Data='cmd.exe']` to the query so another program's crash loop cannot push cmd's Clink crashes out of `MOST_EVENTS`; otherwise raise `MOST_EVENTS`. Re-check `the_query_asks_for_application_error_1000_since_the_time` against what the spike ran.

#### Tasks 5 and 7

- D12 and the start's crash check never run through `app/src-tauri/src/library.rs`'s `DiskMachine`, which holds a real `WinEventLog` with no gate; their tests use `FakeMachine`.
- **Also (from 4a fix round 1):** an Err "the Application log is still being read" (the single-flight guard) is "could not be read" for D12 (skipped) and for the start check (no pause), never "no crash".

#### Task 10 (carries from phase 3's VM fix pass)

- In the stacked editor, keep the Tab order and the visual order together: `display: contents` on `.preview-column` inside the container rule, with `order: -1` on its heading, the preview and its error notice only, so the sample fields come after the form in both orders.
- `button { overflow-wrap: normal }` (and `.button`), so the inherited `overflow-wrap: anywhere` of cards, notices and lists never breaks a squeezed button label mid-word; check button rows at 900x620 with the root at 150%.

#### After the spike (2026-10-18; docs/spikes/phase3b-recovery/results.md, commit f00c42f) — binding

- **S1-S5, S7 passed; F1-F4 are not used.** `os.getpid`, `io.open`/`os.remove` on the Thai path, `os.globfiles` (bare names), `os.gethost`, `clink set` loading all work as the plan's primary path assumes.
- **S6 → neither the plan's priority-2 filter nor F5.** No filter after Oh My Posh's runs (its `rightfilter` returns `false`), and F5 (clear at the first line's end) would count a window opened and closed without typing as a failed start. **The first prompt is seen by a coroutine:**
  - `HEALTH_BODY` registers CMD Studio's own prompt filter at a priority **before** Oh My Posh's (Oh My Posh registers at `_ZL_CLINK_PROMPT_PRIORITY + 1`, which was 1 on both VMs; use priority 0). Its `filter` returns nothing (the chain goes on) and, on the first edit line of the window only (a flag), calls `clink.promptcoroutine(function() … end)`. The coroutine deletes this window's tag and the stale tags (as the plan's first-prompt code does), then returns.
  - Clink resumes prompt coroutines only while the edit loop waits for input — after every filter, Oh My Posh's included, has returned and the prompt is drawn (measured 0.06-0.17 s after Oh My Posh's end, no key pressed). A hanging filter or a window closed during the hang never runs it (measured), so the tag stays and is counted.
  - Use `clink.promptcoroutine` (documented), not `clink.addcoroutine` (works, undocumented). Every call goes through the existing never-raise wrapping; if `clink.promptcoroutine` is not a function, write nothing new (the tag stays; log one line).
  - When the coroutine completes, Clink refilters the prompt once (first line only). Accepted; Task 12 measures it.
  - mlua tests (Task 3): a fake `clink` with a filter registry in priority order, `promptcoroutine` that queues the function, and an explicit "idle" step that runs the queue. Pin: the tag is gone after idle; it stays when idle never comes (a hang, a window closed); a refilter on the same line clears nothing twice; the filter returns no values; a raise in the coroutine is logged, never thrown; `clink set` (not cmd.exe) writes no tag (S4).
- **S8 → F7 (recorded, no change).** The counter costs 2.6 ms (VM-A) and 6.1 ms (VM-C) with 10 leftover tags.
- **S9 passed → F8 not needed;** named fields `AppName`/`ModuleName`, the same in Thai.
- **S10 passed → F9 not needed;** the query is accepted and the `EventData[Data='cmd.exe']` variant works.
- **S11 → F10 used** for the fixtures only (the fake cmd.exe loads the stand-in); Task 12's VM check uses the same F10 program.

#### Task 4b (now)

- Copy `docs/spikes/phase3b-recovery/event1000-en.xml` and `event1000-th.xml` byte for byte to `crates/core/tests/fixtures/eventlog/` (plan Task 4 Step 2) and add the test `reads_the_events_the_spike_recorded_on_english_and_thai_windows` from the plan's Step 3 text, fitted to the recorded values (3 events? one file each holds one event — read the files).
- Add `and EventData[Data='cmd.exe']` to `crash_query` (the 4b carry: another program's crash loop must not push cmd's out of `MOST_EVENTS`); update `the_query_asks_for_application_error_1000_since_the_time` to the new text, which the spike ran on both VMs (S10).
- The query's lower bound: `timediff` counts back from the service's clock when it runs, later than `now_ms`. Widen the span by the time between `now_ms` and the call (or by a fixed 60 s margin) so events right after the apply are not missed; `clink_crashes` still keeps only `>= since`. Test it with the pure helper.

#### Task 3 fix round 1 (from the review)

- The "first line done" flag is set inside the coroutine after it ran (type-ahead).
- Tests pin §13: a window whose Oh My Posh exe is missing (`__popen_result = nil`) and one whose Oh My Posh script raises at load still reach a prompt and leave no tag.
- The paused `oninject` callback and its registration run inside `pcall`; a failure goes to `note`, never to the window.
- Lua tests with a Thai `HEALTH` path, with and without `__glob_paths`.
- Before pausing (PAUSE_AFTER or more stale tags), one `io.popen("tasklist /FO CSV /NH 2>nul")`: a stale tag whose pid appears as a running `cmd.exe` row (`"cmd.exe","<pid>",` case-insensitive on the name) is not counted and not deleted; if the popen fails or gives nothing, count as before. Never more than one tasklist per window start; never on the normal path.
- One line for the doc path.

#### Task 7 (carry from Task 3's review)

- Every committed apply from the app calls `recovery::clear_pause`, which makes `health\`, as the CLI's apply does.

#### Task 12 (carries from Task 3's review)

- On a VM: Oh My Posh's filter raising at run time still lets the prompt coroutine run (no tag left); three `cmd /k "ping -t localhost"` windows and a fourth window after 60 s do not pause; the tasklist call's cost at the pause decision.
- **Task 10 (carry from Task 7):** a pause record with `count` 0 (the crash reports could not be read) never says "0 times"; the banner then says cmd crashed in Clink without a number.

#### Tasks 9 and 10 — collected carries (2026-10-18)

- **The pause's truth (Task 7 rounds 1-2):** the status says whether a pause of this computer is in effect (theme out) or "not yet" (CMD Studio could not take its changes out and tries again at the next start). Home's banner and Help's D12 row say "not yet" plainly, never "paused", in en and th. `CrashCheck.pause` (PauseOutcome) and a "Pause the theme" result that did not go through are shown at once, with the reason the §5.5 way (refused: E_BUSY / E_SNAPSHOT_READ, or rolled back).
- **An unreadable `paused.json`** (`PauseInfo.reason` null): the banner says the pause record could not be read, and "Turn the theme back on" then only lifts it (no theme to resume) — say that before the click.
- **A pause record with `count` 0** (crash reports unreadable): the banner never says "0 times"; it says cmd crashed in Clink, without a number.
- **D12's "Pause the theme" fix** asks for a yes first (a dialog that says Revert everything runs and the theme can be turned back on), with no Close while busy (R7).
- **The stacked editor (phase 3 carry):** keep the Tab order and the visual order together — `display: contents` on `.preview-column` inside the container rule, `order: -1` on its heading, the preview and its error notice only.
- **`button { overflow-wrap: normal }`** (and `.button`) so the inherited `overflow-wrap: anywhere` never breaks a squeezed label mid-word; check button rows at 900x620 with the root at 150%.
- **The mock stays logic-free:** fixed data for every new status shape (in effect, not yet, unreadable, count 0), chosen by `MockOptions`.
- **Task 10 (carry from Task 7 round 3):** the plan's banner text `home.paused.levelClink` ("nothing on this computer was changed", plan ~5304) is never shown for an unreadable record (`reason: null`); the banner has its own words per `PauseInfo.level` — `cmd`, `computer`, `computerNotYet` ("not paused yet … tries again at the next start"), `computerUnknown` ("may be paused … state.json could not be read") — in en and th, matching D12's texts.
- **Task 10 (carry from Task 7 round 4):** the banner and any start-check notice word the pause from `status.paused.reason`, never "paused for cmd's crashes" when the check returns count 0 for a retried user or recovery pause.
- **Task 10 (carry from Task 7 round 4b):** an older app record reads `by: app`, `level: cmd`, `resume` filled — the banner goes by `level` and offers the lift, never the noted theme's name.

#### Task 8 (carries)

- From Task 7's first review (the wiring it could not see): `lib.rs` calls `crash_check_started` before it spawns the check's thread; the check runs once per start, off the window's thread; `crash_check_not_run` is called when the spawn fails; the `recovery_status` IPC wrapper uses `blocking(...)` like the other commands, so its 30 s wait never holds the main thread. Test what can be tested.
- From Task 7's round-4 re-review, as a separate docs commit: the three inverted comments — `app/src-tauri/src/backend/recovery.rs:59` (`PauseInfo.level`), its binding `app/ui/src/bindings/PauseInfo.ts:13` (regenerate by the manual steps, never by the script), and `crates/core/src/doctor.rs:1192` — say "applied no later than the record" (an app record is "not yet" while the theme in use was applied at or before it).
- **Tasks 9 and 10 (carry from Task 8):** never poll `recovery_status` (each call can hold a worker up to 30 s while the start check runs): call it once on Home's load, and again only after a pause, a resume or an apply — never on a timer.
- **Task 11 (carry from Task 8):** `app/src-tauri/capabilities/default.json`'s description string names the three new commands (recovery_status, theme_pause, theme_resume) beside preview_render, open_external and advanced_read; `app/src-tauri/src/config_guard.rs:3` says 30 commands.
- **Task 9 (carry from Task 8):** `themePause(reason)` takes only the generated `PauseReason` type, never a free string.

### Task 10 (Home banner, Pause, D12 fixes)

### Task 10: the controller's rulings (they override task-10-brief.md)

The brief was written before Task 7's rounds changed the bindings. The shipped bindings are the authority: `app/ui/src/bindings/{PauseInfo,PauseLevel,PauseOutcome,CrashCheck,RecoveryStatus,ResumeOutcome}.ts`, and the mock at `app/ui/src/ipc/mock.ts` (Task 9: `MockOptions.paused` takes a whole `PauseInfo`, plus `cmdCrashes`, `failRecoveryWith`, `crashCheck` and `failPause: "refused" | "rolledBack" | "notSaved"`).

The facts the brief does not know:
- `PauseInfo` has `level: "cmd" | "computer" | "computerNotYet" | "computerUnknown"`.
- A record that can't be read comes back as `by: "clink"`, `level: "cmd"`, `reason: null`, `atMs: null`, `count: 0`, `resume: null`.
- An app record older than the theme applied now comes back as `by: "app"`, `level: "cmd"`, `resume` filled. `themeResume` lifts it at once (`resumed`).
- `PauseOutcome` has `pausedNow`. A record that was saved, with a revert that was refused or rolled back, leaves level `computerNotYet`: cmd opens without the theme, and the app tries again at its next start.
- `theme_pause("user")` records count 0. `theme_pause("clinkCrash")` records the crashes counted now, which may be 0 (reports unreadable), 1 or 2.

#### R1. Fixtures

Give every `PauseInfo` in the brief's tests its `level`: `PAUSED_IN_CMD` gets `level: "cmd"`, and `gone` gets `level: "computer"`.

#### R2. The banner's words (`PausedBanner`)

It has two lines: the reason, from `paused.reason`, then the reach, from `paused.level` and `paused.by`. Use these words exactly, en then th. Keys not listed keep the brief's words.

**Reason:**
- `cmdStarts`: the brief's `home.paused.reason.cmdStarts`, unchanged.
- `clinkCrash` replaces the brief's single key with three, chosen by `count`:
  - `home.paused.reason.clinkCrash` (count 2 or more):
    - en: "cmd crashed in its add-on {n} times, so CMD Studio paused the theme."
    - th: "cmd แครชในส่วนเสริม {n} ครั้ง CMD Studio จึงหยุดธีมไว้"
  - `home.paused.reason.clinkCrashOnce` (count 1):
    - en: "cmd crashed in its add-on once, so CMD Studio paused the theme."
    - th: "cmd แครชในส่วนเสริม 1 ครั้ง CMD Studio จึงหยุดธีมไว้"
  - `home.paused.reason.clinkCrashNoCount` (count 0):
    - en: "cmd crashed in its add-on, so CMD Studio paused the theme."
    - th: "cmd แครชในส่วนเสริม CMD Studio จึงหยุดธีมไว้"
- `recovery` and `user`: the brief's words.
- `null`: the brief's `home.paused.reason.unknown`, then, in place of a reach line:
  - `home.paused.unknownResume`:
    - en: "Turning it back on only removes the note: it applies no theme."
    - th: "เปิดธีมกลับจะลบบันทึกนี้เท่านั้น ไม่ได้ใช้ธีมใด"

**Reach** (only when `reason` is not null):
- `level: "cmd"` and `by: "clink"`: the brief's `home.paused.levelClink`.
- `level: "cmd"` and `by: "app"` (an older app record). Never name the noted theme.
  - `home.paused.levelCmdOlder`:
    - en: "A theme was applied after this pause, but the pause could not be lifted, so cmd windows still open without it. Turning it back on lifts the pause."
    - th: "มีการใช้ธีมหลังการหยุดครั้งนี้ แต่ยกเลิกการหยุดไม่สำเร็จ หน้าต่าง cmd จึงยังเปิดโดยไม่มีธีม เปิดธีมกลับจะยกเลิกการหยุดนี้"
- `level: "computer"`: the brief's `home.paused.levelApp` when `resume?.themeName`, else `home.paused.levelAppGone`.
- `level: "computerNotYet"`:
  - `home.paused.levelNotYet`:
    - en: "It is paused in cmd only for now, not yet on this computer: CMD Studio could not take its changes out, and tries again the next time the CMD Studio app starts."
    - th: "ตอนนี้หยุดธีมไว้ใน cmd เท่านั้น ยังหยุดทั้งเครื่องไม่สำเร็จ CMD Studio เอาการเปลี่ยนแปลงออกไม่ได้ และจะลองใหม่ตอนเปิดแอป CMD Studio ครั้งหน้า"
- `level: "computerUnknown"`:
  - `home.paused.levelUnknown`:
    - en: "A pause of this computer may be in place, but CMD Studio could not read whether its changes are out (state.json)."
    - th: "อาจมีการหยุดธีมทั้งเครื่องอยู่ แต่ CMD Studio อ่านไม่ได้ว่าเอาการเปลี่ยนแปลงออกแล้วหรือยัง (state.json)"

**The "Undo everything" button** stays as the brief has it: shown for `by === "clink"`, which includes a record that can't be read. Undo everything lifts only init.lua's records.

#### R3. What the pause did (`pauseHeadline`, `PauseResult`)

Go by `pausedNow` first:
- `pausedNow` and `revert?.ok`: `pause.done`, tone `ok`.
- `pausedNow` and not `revert.ok`: `pause.notOk`.
- not `pausedNow` and `notSaved`:
  - when `revert?.status === "committed"`: `pause.notSaved`, the brief's words;
  - otherwise the new key `pause.failed`:
    - en: "The theme could not be paused: CMD Studio could not note the pause or take its changes out, so cmd windows still open with it."
    - th: "หยุดธีมไม่สำเร็จ CMD Studio บันทึกการหยุดธีมไม่ได้และเอาการเปลี่ยนแปลงออกไม่ได้ หน้าต่าง cmd จึงยังเปิดพร้อมธีม"
- not `pausedNow` otherwise (refused, rolled back, or a rollback not complete). The new key `pause.notYet` replaces the brief's `pause.refused`, which is dropped:
  - en: "The theme is paused in cmd only for now: CMD Studio could not take its changes out of this computer, and tries again the next time the CMD Studio app starts."
  - th: "ตอนนี้หยุดธีมไว้ใน cmd เท่านั้น CMD Studio เอาการเปลี่ยนแปลงออกจากเครื่องนี้ไม่ได้ และจะลองใหม่ตอนเปิดแอป CMD Studio ครั้งหน้า"

Every headline except `pause.done` uses tone `warn`. The error notices below the headline are the brief's: unrestored, the failed steps, `revert.error`, `refused` and `notSaved`. Export `PauseResult`, because Home uses it (R4).

`setLastApply(null)` stays as the brief has it: on `revert?.status === "committed"`.

#### R4. Home: the start's pause that did not go through

Let `startPause` be `crashes.pause` when `crashes` is not null and `crashes.kind !== "noTheme"`, else null.

Show `startPause` when all of these hold:
- it is not null;
- not `startPause.pausedNow`;
- one of these:
  - `paused?.level === "computerNotYet"`;
  - `paused === null && startPause.notSaved !== null`, and either the revert did not commit or no theme is applied (`lib?.applied` is null).

Show it in a card right under the banner:
- a lead line, `home.paused.startTried`:
  - en: "When CMD Studio started, it tried to pause the theme on this computer:"
  - th: "ตอนเปิด CMD Studio ได้ลองหยุดธีมของเครื่องนี้:"
- then `<PauseResult outcome={startPause} />`.

This is the rulings' "`CrashCheck.pause` shown at once, with the reason the §5.5 way".

#### R5. Home's read again

The brief's effect is keyed on `crashes.kind === "read" && crashes.pausedNow` and `paused?.by === "app"`. Key it on the start check taking the theme out: `startPause?.revert?.status === "committed"` while `lib?.applied` is not null. Read once per visit, as the brief has it. This also covers an `unreadable` check that retried a pause, and a pause that was not saved.

#### R6. Tests beyond the brief's

Add these tests to `describe("the theme's pause", …)`. Measure the total yourself; the base at 037e90a is 227.
- **Not yet** (`paused` at level `computerNotYet`, reason `user`, Terracotta applied):
  - the banner says `home.paused.reason.user` and `levelNotYet`;
  - there is no "Pause the theme" button.
- **A record that can't be read:**
  - the banner shows `reason.unknown` and `unknownResume`, with no "Since" line;
  - "Undo everything" is shown;
  - "Turn the theme back on" lifts the record at once (`resume.done`), with no plan.
- **The count's words** for clinkCrash at 0, 1 and 3: never "0 times", never "1 times". This can be one test over three mounts.
- **An older app record** (`by: "app"`, `level: "cmd"`, `resume` naming Terracotta, Terracotta applied):
  - the banner shows `levelCmdOlder` and nowhere names Terracotta;
  - "Turn the theme back on" lifts it at once, with no plan dialog.
- **`computerUnknown`'s words.**
- **`failPause: "refused"`** from Home's "Pause the theme":
  - the dialog says `pause.notYet`, with the E_BUSY notice;
  - after Close, the banner says `levelNotYet`.
- **`failPause: "notSaved"`:** the dialog says `pause.notSaved`.
- **A start pause that did not go through.** `crashCheck` is `{ kind: "read", count: 3, pausedNow: false, pause: { pausedNow: false, revert: null, refused: <an E_BUSY AppError>, notSaved: null } }`, and `paused` is at level `computerNotYet`. Expect:
  - Home shows `home.paused.startTried` and `pause.notYet` with the E_BUSY notice;
  - Home does not read again (`themesList` is called once).
- **One Thai test:** the not-yet banner's words in th.

Fit the brief's own tests to R2:
- the clinkCrash sentences lose "on this computer";
- "Pause the theme"'s path stays `pause.done`.

#### R7. Phase 3 carries (a second commit in this task)

These are in `rulings-by-task.md` under "Task 10 (carries from phase 3's VM fix pass)". Make them in `app/ui/src/styles.css`, with tests in `app/ui/src/styles.test.tsx`; read how the existing F2 tests there check the stylesheet.
- **The stacked editor.** Keep the Tab order and the visual order together.
  - Inside `@container editor (width < 49.5rem)`, give `.editor .preview-column` `display: contents`, in place of `position: static; order: -1`.
  - Give `order: -1` to the preview column's heading, the preview, and its error notice only. Read `app/ui/src/screens/Editor.tsx` for their elements and classes; add a class if one is needed.
  - The sample fields then come after the form in both orders.
  - Fix the comment above `.editor-box`, which says `order` leaves the Tab order as it is.
- **Buttons.** Add `button, .button { overflow-wrap: normal }`, so the `overflow-wrap: anywhere` that cards, notices and lists pass down never breaks a squeezed button label mid-word.

Commit message: `ui: the stacked editor keeps its Tab order with its visual order, and button labels never break mid-word (phase 3 VM carries)`.

#### R8. Never poll

Home reads `recoveryStatus` through `useRead(readRecovery, version)`, as the brief has it: once on load, and again after a `bump`. Never on a timer, and never in a loop that waits for `crashes` to stop being null.

Since Task 9's fix round, `MockOptions.crashCheck` may also be `null`: `recoveryStatus` then always answers `crashes: null`, as a check that has not finished. Add one test: with `crashCheck: null` and Terracotta applied, Home renders. Then wait well past any short interval, using vitest's fake timers or a few `settle()` rounds, whichever the file already uses. `recoveryStatus` must be in `ipc.state.calls` exactly once.

Also since that round, an undo or a restore that leaves no theme lifts a `by: clink` pause (as the backend does), and Home reads again after it through `bump`.

### Task 11 (the main spec)

### Task 11: the controller's rulings (they override task-11-brief.md)

The brief was written before the spike and Tasks 3-10. It has two fallbacks, F5 and F8, and neither came true. The spec text must say what shipped. Check every sentence you write against the shipped code and these sources:
- `docs/superpowers/specs/2026-10-17-recovery-mode-design.md` §13. The recovery spec is the founder's: read it, never edit it.
- `docs/spikes/phase3b-recovery/results.md`.
- The code:
  - `crates/core/src/generate/lua.rs` (`HEALTH_BODY`);
  - `crates/core/src/recovery.rs` (`PauseLevel`, `Paused::level`, `clear_pause`);
  - `crates/core/src/eventlog.rs` and `crates/core/src/adapters/windows/eventlog.rs`;
  - `crates/core/src/doctor.rs` (`d12`);
  - `crates/cli/src/lib.rs` (apply and `--cleanup`);
  - `app/src-tauri/src/backend/recovery.rs` and `apply.rs`.

Write the spec in Thai like the rest of the main spec. Keep code names, codes and file names in English.

#### R1. §5.3 (the brief's Step 1)

`health\`'s line stays as the brief has it, with one change: say that the start tag is written only when Clink runs in cmd.exe (`os.gethost`).

The log sentence must name the lines the app actually writes. They are in `app/src-tauri/src/backend/recovery.rs`:
- `crash_check …`, whose forms are `not_run`, `theme=none`, `state=unreadable`, `unreadable`, and `crashes=<n> paused=<bool>`;
- `pause reason=… count=… revert=… saved=…`;
- `resume by=… level=cmd`. The brief's `resume by=clink` is out of date.

If `apply.rs` writes a line when an apply, undo or restore lifts the pause, name it too. Grep for `note_line` there.

#### R2. §5.7 and §5.10 (the brief's Step 2)

Keep the brief's text, with these corrections:
- `theme_pause`:
  - it writes `paused.json` first, then runs the core's Revert everything;
  - it is "in effect" (`pausedNow`) only when that revert ended ok and the record was saved;
  - when not, cmd still opens without the theme while the record is there, and the app tries again at its next start.
- `theme_resume`:
  - lifts a pause at level `cmd` at once. That includes init.lua's pause, a record that can't be read, and an app record older than the theme applied now;
  - for the other levels, it returns the request and the plan, or `pickTheme` when the theme is gone.
- The CLI's apply row: keep the brief's sentence, and add the Task 6 carry from `rulings-by-task.md` "## Task 11". When the lift fails, the apply still exits 0 and prints its "Applied" line, plus one English stderr line that begins `E_DATA_WRITE: the theme was applied, but CMD Studio's start records in <health> could not be cleared`. Take the exact wording from `crates/cli/src/lib.rs`.
- Undo and restore in the app lift a pause in cmd when no theme is left (`apply.rs`). Say it beside Undo everything wherever §5.7 describes those commands. If there is no such place, put it in appendix B only (R4).
- The two counts become 30, as the brief says.

#### R3. §5.8 D12 (the brief's Step 3)

Use the brief's row, with the facts as shipped. Check each in `eventlog.rs` and `doctor.rs`:
- The query, in plain words: events 1000 of `Application Error` in the Application log, from the last seven days. Read at most 1000 events, newest first, within a 5-second deadline, through the Event Log API.
- The XPath selects by provider and event ID only, with no Data filter, because XPath is case-sensitive and the AppName follows the launch string. The app keeps the events whose `AppName` is `cmd.exe` and whose `ModuleName` is `clink_dll_x64.dll` with no regard to case. Use the exact rule in the code.
- A second read while one is still running is refused (`STILL_BEING_READ`) and counts as "could not be read".
- The pause comes first, with `E_THEME_PAUSED`. Say what it reports per level: in cmd only, this computer, not yet on this computer (CMD Studio tries again at the next app start), and not known (state.json could not be read). Also cover a record that cannot be read.
- A count of 0 never says "0 times".
- Keep the brief's sentence about D10 and D12.
- Check whether `paused.json` that cannot be read gives `E_MACHINE_READ` or an unreadable record (reason null), and write what the code does. The brief says `E_MACHINE_READ`.

#### R4. Appendix B (the brief's Step 4)

Use the brief's row, with these changes:
- **The third bullet**, the first prompt, is replaced. Neither the brief's text nor its F5 alternative shipped. Say what shipped, following §13:
  - CMD Studio's own prompt filter runs before Oh My Posh's, at priority `math.min(0, …)`. On the first line it calls `clink.promptcoroutine` once.
  - Clink wakes that coroutine only while it waits for input, so after every filter has returned and the prompt is up. The spike measured 0.06-0.17 s after Oh My Posh, and never while a filter hangs. The coroutine deletes the tag.
  - Oh My Posh's `rightfilter` returns false, so no filter after its own ever runs (spike S6).
  - F5 (onendedit) was not taken, because a window closed at its first prompt would count as failed.
- **Add a bullet:** before pausing, init.lua runs `tasklist.exe` once, from `%SystemRoot%\System32` by full path. A stale tag whose pid is a running cmd.exe is not counted. If tasklist gives nothing, the tags count as before.
- **Add a bullet on the pause's levels** (`PauseLevel`: `cmd`, `computer`, `computerNotYet`, `computerUnknown`):
  - an app record is "not yet" only when it is no older than the theme applied now;
  - an older one is a pause in cmd only. That happens when an apply committed but could not lift it, and it is never a reason to revert;
  - the start retries a "not yet" record.
- **Add a bullet on the crash check's read:** the XPath has no Data filter, and the reasons are those of R3.
- **The "every apply lifts" bullet:** add that undo and restore lift a pause in cmd when no theme is left. Also add the CLI's failed-lift line (R2).
- Keep the other bullets, checked against the code.

#### R5. Carries from Task 8: two code files, in the same commit

- `app/src-tauri/capabilities/default.json`: the description string names the three new commands too. It currently reads "(with preview_render, open_external and advanced_read)"; add recovery_status, theme_pause and theme_resume in the same style.
- `app/src-tauri/src/config_guard.rs:3`: "all 27" becomes "all 30".
- Run `cargo test -p cmdstudio-app --lib` and the four clippy commands from `constraints.md`, and check `cargo fmt --all -- --check`.

#### R6. Checks and commit

- Run the brief's Step 5 tests, plus R5's.
- Read the spec's diff: it must change only the places of R1-R4.
- Make one commit. The message: `spec: phase 3b-1 in §5.3, §5.7, §5.8, §5.10 and appendix B (health\, the pause's IPC, D12); the capability description and config_guard name 30 commands`.

### Task 12 (the VM check)

### Task 12: the controller's rulings (they override task-12-brief.md)

The brief was written before the spike's two rounds and before Tasks 3-10 shipped. Fit the protocol (`docs/spikes/phase3b-recovery/vm-check.md`) and the scripts to what shipped and to what the spike found. Read these first:
- `docs/spikes/phase3b-recovery/results.md`: both rounds, especially S6, S6b, S8, S11 and F10;
- the recovery spec's §13 (`docs/superpowers/specs/2026-10-17-recovery-mode-design.md`; read only);
- the shipped code that every PASS line quotes:
  - `crates/core/src/generate/lua.rs` `HEALTH_BODY`;
  - `crates/core/src/doctor.rs` `d12`;
  - `app/ui/src/i18n/en.ts` and `th.ts` (`home.paused.*`, `pause.*`, `resume.*`, `help.guide.cmdWontOpen`);
  - `app/src-tauri/src/backend/recovery.rs` (the log lines `crash_check …`, `pause reason=… count=… revert=… saved=…`, `resume by=… level=cmd`).

#### R0. What you may run on this PC

The rules are `pc-safety.md`, which is binding:
- **Allowed:**
  - build the hang stand-in into the scratchpad with cargo (build and clippy only; never start it);
  - parse-check every `.ps1` you write or change with the PowerShell parser, as Step 5 does.
- **Never, not even as a dry run:**
  - run any script written for a VM;
  - start any binary built for a VM;
  - read this PC's event log, registry or windows;
  - run VBoxManage.

The VM run (Step 8) is the controller's, and it does not happen in this task.

#### R1. Order

Do Steps 1-7 now. Step 8 runs later, on the head CI builds after the final whole-branch review and its fix pass, so the VM checks the code that goes into the PR. The protocol says "this branch's head" and stays as it is.

#### R2. The crash (steps 4 and 6): F10, not the DLL swap

S11 failed: Clink refuses the stand-in DLL (`loader_main_thunk`), so `recovery-crash.ps1 -Swap` never crashes a real cmd window. Use F10, as round 2 did:
- **The program:** `fake-cmd.exe` copied as `C:\cmdstudio\fake\cmd.exe`, with the stand-in `clink_dll_x64.dll` beside it. Read how `f10-vm-a.txt` and `f10-vm-c.txt` ran it, and any guest script that did.
- **What it produces:** each run writes one event 1000 with `AppName` `cmd.exe` and `ModuleName` `clink_dll_x64.dll`.
- **Run it after the theme is applied,** because the app counts events since `applied_at_ms`.
- **The binaries** are in the controller's scratchpad as `crash-dll\clink_dll_x64.dll` and `vmrun\fake\cmd.exe`. Name those paths in the Setup's `Copy-ToGuest` lines as `<scratchpad>\…`, the way the brief does.
- **Steps 4 and 6's "each window crashes" and "the window closes"** become the runs of F10's program, as F10's output shows them.
- **What stays:** step 4's check that a new real cmd window afterwards is plain, prints no line and loads no Clink.

#### R3. The cost (step 7): the onbeginedit marker

S8 found that a marker filter after Oh My Posh's never runs (S6), so `recovery-timing.ps1 -InstallMarker` with a filter marker measures nothing.
- **The marker:** measure the shipped app's start the way S8's second half did, from cmd's start to the first `clink.onbeginedit`, with the VMs' `timing.lua` copy. Compare it with `timing-vm-a.txt`, measured the same way without the counter.
- **The fix:** fix `recovery-timing.ps1 -InstallMarker` (or add a switch) so it installs that onbeginedit marker into CMD Studio's `generated\`.
- **The verdict:** more than 5 ms is recorded for the founder (F7), not a FAIL.

#### R4. New steps (the carries in `rulings-by-task.md` and `progress.md`)

Add these steps to the protocol, with PASS lines. Each script they need must be guarded like the others (`$env:COMPUTERNAME -notlike 'CMDSTUDIO-*'` throws in its first statements) and listed in `guard.test.ps1`'s `$changers` when it changes the machine.

- **(a) A filter that raises at run time.** Write `zz-raise.lua` into CMD Studio's `generated\`: one prompt filter at priority 1, the same as Oh My Posh's, whose `filter` calls `error("raise")`.
  - Then open a cmd window and wait 10 s.
  - PASS: the window reaches a prompt and `health\` lists no tag. CMD Studio's coroutine still ran.
  - Remove the file afterwards.
- **(b) Live windows that never reach their prompt are not counted** (the tasklist check). Open three `cmd /k "ping -t localhost"` windows, then wait 65 s and open a fourth plain window.
  - PASS: the fourth window is themed, and no `paused.json` exists. `health\` still lists the three ping windows' tags.
  - Then close the three ping windows and open a fifth window 65 s later. Record what happens, and do not mark it PASS or FAIL. By design, the three tags of windows closed before their prompt now count, so a pause is expected. That is an observation for the founder.
- **(c) `cmd /c` never counts.** Run `cmd /c echo hi` three times, from a script in the guest. Wait 65 s, then open a plain window.
  - PASS: no pause.
  - Record whether `cmd /c` left a tag at all. Programs that run `cmd /c` load AutoRun too. If Clink skips non-interactive cmd, there is no tag; if there is one, the design would pause wrongly, and that is a FAIL to report.
- **(d) The tasklist call's cost.** In the guest, time the exact command `HEALTH_BODY` runs, five times: `"%SystemRoot%\System32\tasklist.exe" /FO CSV /NH`, through `cmd /d /c` with its `2>nul`. Record the average.
- **(e) A read of 1000 events.**
  - Write 1100 `Application Error` events with id 1000 into the guest's Application log, with any message (`Write-EventLog -LogName Application -Source 'Application Error' -EventId 1000 …`). Do it in a guest script guarded like the others.
  - Start the app with a theme applied, and read the log.
  - PASS: the `crash_check` line reports `crashes=<n>`, not `unreadable`. Record the time from the app's start to that line, using whatever start line the app's log has (check `app/src-tauri/src/lib.rs` and its log calls).
- **(f) The refilter.** §13 says that when the coroutine ends, Clink refilters the prompt once, on the window's first line only.
  - Measure its cost with a marker Lua in `generated\` at a priority before CMD Studio's filter (for example -1000). It appends the time and the call's number to a file on each filter call of the first line. Read `probe-s6b.lua` for how round 2 logged.
  - Record the time between the first call and the refilter's, and whether exactly one refilter happened.
  - Remove the marker afterwards.

Place these steps where they need the least setup: after step 2, with the theme applied and the app closed. Steps (a)-(d) and (f) run on VM-A. Step (e) runs on VM-A only.

#### R5. The words in the PASS lines

Every quoted string must be the shipped one, copied from the code:
- **The banner:**
  - "cmd crashed in its add-on 3 times, so CMD Studio paused the theme." (no "on this computer");
  - "…once, so…" for one crash;
  - `levelApp`, `levelClink`, and the rest from en.ts.
- **D12's message and fix** come from `doctor.rs`. Step 6's single crash says "once", never "1 time(s)".
- **The pause dialog's headline** comes from `pause.done`.
- **The log lines** come from recovery.rs. Step 4 expects `crash_check crashes=3 paused=true` and the `pause …` line in its real format.
- **init.lua's two lines in cmd** come from `HEALTH_BODY`.

Where Thai is checked (step 9 and VM-C), use th.ts.

#### R6. The guard test's total

Recount `guard.test.ps1`'s expected last line after your additions, and write the right number into Step 8's PASS line.

#### R7. Commit and report

- **One commit for Steps 1-7,** with the brief's message. It includes the new or changed guest scripts and the Lua markers. The protocol leaves the Result column empty.
- **The report** goes to `task-12-report.md`. It holds:
  - what changed from the brief and why;
  - the parse results;
  - the hang stand-in's build and clippy output;
  - what Step 8 will need from the controller: the binaries, the artifact and the order of steps.

### The final fix pass

### Plan 3b-1: the final fix pass (one pass)

The final whole-branch review is `final-review-report.md` in this folder; read it first. Its assessment is "With fixes": no Critical, one Important (I1), five Minors. Fix the items below in this one pass.

**Rules for every item:**
- **TDD.** Write the test that reproduces the finding first, watch it fail, then fix.
- **Rust production code** has no unwrap, expect, panic or indexing.
- **Commits:**
  - English messages.
  - Read `.git/CMDSTUDIO_COMMIT_MSG`, then Write it, then `git commit -F` as a separate step.
  - `git add --` with the intended files only, then check `git diff --cached --name-only`.
  - No AI attribution. Never push.
  - Never write this PC's Windows user name.
- **PC safety:** `pc-safety.md` and `constraints.md`, binding:
  - Never run the app, a VM script, a Lua probe or a spike binary.
  - Never read this PC's event log, registry or windows.
  - Never set CMDSTUDIO_* variables. Never pass `-ExecutionPolicy`.
  - Run cmd.exe only with `/d`.
  - If Windows Application Control blocks a command, rerun it once, never work around it, and report it.
  - You do not dispatch subagents.
- **Commit grouping:** several commits are fine, one per area. Suggested: core + Lua; core eventlog; doctor words; UI; docs and protocol.

#### 1. I1 (Important): a long-lived window closed without a prompt must not count as a failed start

**Ruling (the controller's; the founder can change the number):** init.lua counts a start that failed only within the last 10 minutes. These are the same three windows "in a row" as before, just not hours apart. A tag older than that is forgotten: deleted, and never counted. It is a window that lived long, such as a `cmd /k <dev server>` profile, or a failure from long ago.

- **The constant.** In `crates/core/src/recovery.rs`, add `pub const FORGET_AFTER_SECS: u64 = 600;` beside `STALE_AFTER_SECS` and `PAUSE_AFTER`, with a doc comment saying why. Extend the existing pin test (`assert_eq!((STALE_AFTER_SECS, PAUSE_AFTER), (60, 3))`) to cover the new constant.
- **init.lua** (`crates/core/src/generate/lua.rs`):
  - Write `local FORGET_AFTER = 600` into init.lua beside `STALE_AFTER` and `PAUSE_AFTER`, the same way (around :580). Update the test around :766 that pins those lines.
  - In `count_start`, before `#old >= PAUSE_AFTER` is decided (so tasklist never runs for them), delete every tag whose age is over `FORGET_AFTER` and leave it out of `old`.
  - Keep a tag being written (empty, `tag_time` nil) out of this, as today.
  - `first_prompt`'s deletion of tags older than 60 s stays as it is.
  - Every file step stays under `pcall`, like the others.
- **Tests** (`crates/core/tests/health_lua.rs`), in the style of the existing ones:
  - three dead tags 3 hours old do not pause, and all three are deleted;
  - three dead tags 3 minutes old do pause;
  - the boundary: 600 s still counts, 601 s does not. Pick the comparison and pin it;
  - tasklist is not run when only forgotten tags are over the threshold.
- **Goldens.** Update init.lua's golden files, the way the repo regenerates them (find how; never by hand-editing generated output if a regenerate path exists).
- **Docs:**
  - the doc comment at `lua.rs:48-56`. Also fix its "neither counted nor deleted": live tags are kept only at the pause decision, and a window's first prompt deletes every tag older than 60 s;
  - the module doc of `recovery.rs`;
  - the main spec's appendix B tasklist/counter bullet (`docs/superpowers/specs/2026-10-10-cmd-studio-design.md`, the last row): one Thai sentence saying a start that failed counts only within 10 minutes, and why. Never edit the recovery spec.

#### 2. m2: the start's crash check must not lose to Home's doctor read

`crates/core/src/adapters/windows/eventlog.rs` `within_time` (about :149) fails at once with `STILL_BEING_READ` when another read holds `READING`. If Home's doctor read came first, the start's check would be "could not be read" and pause nothing at that start.

**Fix:** when the slot is busy, wait for it within the caller's own deadline. Poll every 50 ms, say, and run with the time that is left. Return `STILL_BEING_READ` only when the deadline runs out first. Keep:
- one reader at a time;
- the slot freed before the answer is sent, also on panic;
- the overall 5 s bound per caller.

**Tests:** pin both cases with the existing test seams of `within_time` (read how they fake the work):
- a second caller that waits while the first finishes in time gets its own answer;
- one whose deadline runs out first gets `STILL_BEING_READ`.

Update the doc comments and the main spec's D12 sentence about the single reader: a second read waits for the first, within its own 5 s.

#### 3. D12's crash count words (carry)

`crates/core/src/doctor.rs:1167` says "cmd crashed in its add-on {n} time(s) since the theme was applied…", and the test at :2743 pins "2 time(s)". Use the same count words as the pause rows (`crashed_times`): "once" for 1 and "{n} times" for 2 or more, in en and th, as the pause rows do. Update the test, and the mock's copy of that row in `app/ui/src/ipc/mock.ts` with its tests.

#### 4. m1: after lifting a record that can't be read, the status line must not say the theme is back on

`app/ui/src/components/Pause.tsx` (about :273, :301) prints `resume.done` ("The theme is back on…") after lifting a `reason: null` record. The banner said that turning it back on only removes the note.

- Add the key `resume.noteRemoved`:
  - en: "The note was removed: cmd windows open without a pause again."
  - th: "ลบบันทึกแล้ว หน้าต่าง cmd จะเปิดได้ตามปกติอีกครั้ง"
- Choose it when the pause that was lifted had `reason: null`. `useResume` can take the current `PauseInfo` from its callers, Home and Help. Add a test in `App.test.tsx`.

#### 5. The VM protocol (`docs/spikes/phase3b-recovery/vm-check.md` and the guest scripts)

- **m3.** In step 3, add an observation before the three hung windows are closed. Open a fourth window while they are still open, `Save-Shot`, and record that it hangs too: their pids are live cmd.exe, so they are not counted. Then close all and go on as written. Mark it "observation", not PASS/FAIL.
- **I1's VM line.** In step 2b, say why the fifth window does not pause: the fourth window's first prompt deleted the ping tags. Add one optional observation for the 10-minute rule only if it costs no more than one 11-minute wait. Otherwise state that `health_lua` pins it.
- **Carries:**
  - `spikes/vm/guest/recovery-ping.ps1:18-20`: pass the argument quoted (`'/k', '"ping -t localhost"'`) and fix the comment. Start-Process does not quote by itself.
  - `recovery-timing.ps1`'s header names the median, min and max it prints.
  - `vm-check.md`'s Setup says that `$appLog` prints every matching line, so read the newest stamp.
  - Step 6's quote follows item 3's new words.
- Parse-check every changed `.ps1`: 0 errors, with the guard in its first statements. The guard test's total stays 41/41 unless a script was added.

#### Not in this pass (ledgered as deferred)

- **m4:** `theme_resume` at `computerUnknown` ends in the planner's error.
- The other known minors.

#### Checks before the report

- `cargo fmt --all -- --check`, the four clippy commands from constraints.md, and `cargo test --workspace`. parity runs as allowed; never set CMDSTUDIO_ALLOW_PARITY_SKIP.
- In `app\`: `npm run typecheck`, `npm test` (also with `TZ=UTC`), `npm run build`, and `bash scripts/check-dist.sh`.
- If a binding changes, regenerate it by the manual steps: delete the generated .ts files, then `cargo test export_bindings` with `TS_RS_EXPORT_DIR` set. Never run `export-bindings.ps1`.

#### Report

Write the full report to `final-fix-report.md` in this folder. For each item give:
- the test that failed first, with its RED output;
- the GREEN output;
- the commit.

Then give the suite totals and the concerns.

Your final message, under 15 lines, gives:
- the status;
- the commits;
- a one-line test summary;
- the concerns.
