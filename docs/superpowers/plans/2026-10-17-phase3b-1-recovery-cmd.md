# Phase 3b-1 Implementation Plan — Recovery mode, the cmd side: the start counter, Windows' crash reports, D12, pause and turn back on

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Task 1 and Task 12 are the controller's (VirtualBox VMs); an implementer never runs them.

**Goal:** When the theme makes cmd fail to open (Oh My Posh hangs, crashes or errors before the first prompt, three windows in a row), the fourth cmd window opens plain by itself and says so in one English line; when cmd crashes inside Clink's DLL before any script runs, the next start of CMD Studio pauses the theme on this computer by itself; the user can pause the theme from Home and turn it back on in one click (spec 3b-1 of recovery spec §12).

**Architecture:** `init.lua` gains a start counter in front of the load of Oh My Posh: each cmd window leaves a tag `<data>\health\start-<pid>` until a prompt filter of CMD Studio's, called after Oh My Posh's, takes it away at the first prompt; three stale tags make the next window write `health\paused.json` and load no theme. The core gets a `recovery` module (the paths, `paused.json`, the numbers), an `EventLog` adapter (Windows' "Application Error" events 1000 through `EvtQuery`/`EvtRender`, and a fake), and doctor D12. The app gets `recovery_status`, `theme_pause` (Revert everything, then `paused.json` with the theme to apply again) and `theme_resume` (a pause in cmd lifted at once; a pause of this computer returned as a request for the usual ApplyFlow), plus a crash check on a thread of its own at every start. Home shows a banner while paused and a "Pause the theme" button; Help's D12 offers both fixes, and the guide says what to do when cmd won't open.

**Tech Stack:** Rust 1.99.0, Lua 5.2 (Clink 1.9.34; mlua 0.10.5 `lua52` in tests), windows-sys 0.61.2 (`Win32_System_EventLog` added), Tauri 2.12.4, ts-rs 12.0.1, React 19.3.1, TypeScript 5.9.3, Vitest 5.0.5. No new dependency.

**Spec:** `docs/superpowers/specs/2026-10-17-recovery-mode-design.md` (the recovery spec: §0, §1 criteria 1, 2 and 6, §3, §4.1–§4.3, §4.5, §7, §8, §9, §10, §12) and the main spec `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§5.3, §5.7, §5.8, §5.9, appendix B). Where the recovery spec and the shipped code disagree, this plan follows the code and says so (see "Where this plan departs from the recovery spec").

**Prerequisite:** phase 3 (`docs/superpowers/plans/2026-10-16-phase3-wizard-home-help.md`) merged into the branch this plan starts from, CI green. Task 1's results recorded (`docs/spikes/phase3b-recovery/results.md`) before Task 2 starts. The code of Tasks 2–10 was type-checked, clippy-checked and run in a scratch copy of the repository at `224e910`, whose code `e896940` still has (the core's `lib` 750 tests, `health_lua` 17, `banner_lua` 31, `golden` 21, `english_in_cmd` 2; the CLI's `lib` 50; the app's `lib` 210; the UI 218; clippy and `tsc` clean): only Task 4's two event fixtures were stand-ins there, because the real ones come from Task 1.

## Global Constraints

Every task's requirements include these. Values are copied from the recovery spec, the founder's decisions, and the phase 3 plan's Global Constraints (`.superpowers/sdd/2026-10-16-phase3-wizard-home-help/constraints.md`, `pc-safety.md`) where they bind here.

- Repository `C:\Users\<you>\cmd-studio` (written `<repo>` below), on the branch the controller cuts for phase 3b-1. Rust `1.99.0` (`rust-toolchain.toml`), edition 2021, license MIT. Pinned runtimes: Oh My Posh `31.5.0`, Clink `1.9.34`, Nerd Fonts `3.5.1`.
- **The numbers (recovery spec §3.1, §4.3), each in one place:** `STALE_AFTER_SECS = 60`, `PAUSE_AFTER = 3` (windows in a row, and cmd's crashes in Clink), `CRASH_WINDOW_MS = 604_800_000` (seven days), all in `crates/core/src/recovery.rs`; the event log read gives up after `READ_TIMEOUT = 5 s` (`adapters/windows/eventlog.rs`); `recovery_status` waits at most `CRASH_CHECK_WAIT = 30 s` for the start's crash check (`app/src-tauri/src/backend/recovery.rs`). The generator writes the first two into `init.lua`; nothing else hard-codes them.
- **The files (recovery spec §3.1, §7):** `<data>\health\` is CMD Studio's alone. A tag is `health\start-<pid>` holding the ASCII line `v1 <unix seconds>`; its age is read from its text, never from the file's time. `health\paused.json` is `{"version":1,"reason":"cmdStarts"|"clinkCrash"|"recovery"|"user","at":<unix ms>,"count":<n>,"by":"clink"|"app","resume":null|{"themeId":"…","target":"all"|"wt"}}`; init.lua writes the first five fields with `"by":"clink"` and no `resume`. `revert_all` (core) never deletes `paused.json`; `--cleanup` deletes the whole folder.
- **What cmd shows (founder 2026-10-13; recovery spec §1.6, §3.3, §9):** nothing CMD Studio writes into a cmd window is Thai. The paused window prints exactly one line, in the window's own colors: `CMD Studio paused its theme because cmd failed to start <n> times. Open CMD Studio to turn it back on.`, and every window after it `CMD Studio's theme is paused. Open CMD Studio to turn it back on.` `crates/core/tests/english_in_cmd.rs` must keep passing. init.lua never raises and prints nothing else; a file it cannot read or write skips the counter and never pauses the theme.
- **Language of the UI (founder 2026-10-16):** the app opens in English; every UI string lives in both `app/ui/src/i18n/en.ts` and `th.ts`, following `th.ts`'s header: the prompt is "พร้อมท์", a version "เวอร์ชัน" (never "รุ่น"), the app calls itself "CMD Studio" (never "เรา"), a backup is "จุดย้อนกลับ" (never "สำรอง"); the add-on is "ส่วนเสริม". `i18n.test.ts` fails on a missing key, an empty text, Thai in English or English-only Thai, a text of 30 or more characters under two keys. The core's Thai texts pass `assert_thai_words` (`BANNED_THAI_WORDS = ["prompt", "รุ่น", "เรา", "สำรอง"]`).
- **One exe (main spec §5.7).** `cmdstudio.exe` is the app; started with any argument it runs `cmdstudio_cli::run`. This plan adds no argument (`--recovery` is 3b-2's).
- **IPC (main spec §5.7, §5.9, §5.10).** From Task 8 on the commands are the 27 of phase 3 plus `recovery_status`, `theme_pause` and `theme_resume`: 30 names in `app/src-tauri/src/command_names.rs`, thin wrappers over `Backend`, each logging once through `Backend::logged`. `capabilities/default.json` allows exactly these and `core:event:allow-listen`/`allow-unlisten`; no new event (`apply-progress` stays the only one). The CSP is unchanged. Every failure crosses the IPC as `{code, messageTh, messageEn, details}`, with the codes of main spec §5.9 and no other; `error.rs`'s `MEANINGS` must equal the §5.9 rows (a test reads the spec), so a new code goes into both in one commit.
- **Doctor (main spec §5.8):** the new check is **D12** (recovery spec §4.5 calls it D10, but D10 is the packaged-app check since phase 1b). Doctor reads only; every `warn` or `fail` carries a code (`doctor.rs`'s `run()` test helper); D12's are `E_THEME_PAUSED` and `E_CMD_CRASHED`, new rows of §5.9 (Task 5).
- **No palette, contrast or prompt logic in TypeScript (main spec §5.6).** The mock answers with fixed data; D12's `warn`/`fail` split in the mock is the core's rows copied as fixed data.
- **`--cleanup` stays true.** Every entry the app or init.lua writes under the data folder is known to `cmdstudio_cli`'s `put_there_by_cmd_studio`; from Task 6 that includes `health\`.
- **Production code** (outside `#[cfg(test)]` and `tests/`): no `unwrap`, `expect`, `panic!`, unchecked indexing or slicing, string slicing; the workspace denies clippy `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `string_slice`, `undocumented_unsafe_blocks`. Errors are typed. Every `unsafe` block carries a `// SAFETY:` comment.
- **Test safety.** No test touches this computer's AutoRun, fonts, Windows Terminal settings, `%LOCALAPPDATA%\ai.resincore.cmdstudio` or `%ProgramData%`: backend tests run over the core's `FakeMachine` (feature `fake`), UI tests over the mock in jsdom, Lua tests in mlua with in-memory files. The one test that reads this computer's Application event log returns at once unless `CMDSTUDIO_SYSTEM_TESTS=1` (CI's system-test step). Never set `CMDSTUDIO_SYSTEM_TESTS`, `CMDSTUDIO_OMP_TESTS` or `CMDSTUDIO_ALLOW_PARITY_SKIP` on the dev PC; `cmd.exe` only with `/d`.
- **PC safety (`pc-safety.md`):** never run the cmdstudio binary, the built app, `npx tauri dev`/`build`, Oh My Posh or Clink yourself on this PC; never touch `HKCU\Software\Microsoft\Command Processor`, installed fonts, Windows Terminal settings, `%LOCALAPPDATA%\ai.resincore.cmdstudio` or `%ProgramData%`. No `.env` files, secrets or gh tokens.
- **Scripts written for a VM never run on this PC (coordinator, 2026-10-17)**, not even as a dry run: this covers `.vbs` files, `cscript` and `wscript`, PowerShell that starts a window or a process, UI Automation, and anything that reads this PC's registry or windows. An implementer checks such a script by parsing it only: `$t = $e = $null; [void][System.Management.Automation.Language.Parser]::ParseFile('<path>.ps1', [ref]$t, [ref]$e); $e.Count` must print `0`. That includes `spikes/vm/guest/guard.test.ps1`, which starts the guest scripts in a child PowerShell: it is not run here; its guard rule is checked by parsing (Task 1 Step 9). The controller runs the scripts on the VMs. Building a test-only binary for a VM on this PC (Task 1's crash DLL, Task 12's hang program) is building, not running: it is allowed, in the scratchpad, and the binary is never started here.
- **Windows Application Control.** This PC blocks a freshly built unsigned binary now and then with `os error 4551`. Rerun the identical command once. If it is blocked again, do not work around it (no signing, no policy change, no other toolchain): mark the step "not run locally (WDAC); CI is the gate" in the commit message and go on. For the app crate, the check crate of `docs/superpowers/plans/2026-10-16-phase2b-shell.md` appendix B builds the non-Tauri modules (and exports the bindings) without Tauri.
- **Pins.** Every direct dependency is exact (`= "x.y.z"`), `Cargo.lock` and `app/package-lock.json` are committed. This plan adds no dependency: only the windows-sys feature `"Win32_System_EventLog"` to `crates/core/Cargo.toml` (the lock file does not change). In the repository only `npm ci`, never `npm install`.
- **JSON** CMD Studio writes or sends is camelCase (`#[serde(rename_all = "camelCase")]` on every DTO); ts-rs follows serde. The bindings under `app/ui/src/bindings` are generated by `scripts/export-bindings.ps1` and committed; CI fails when they are stale.
- **Each Rust task ends clean:** `cargo fmt --all`, then `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features ts -- -D warnings`. **Each UI task ends clean:** `npm run typecheck` and `npm test` in `<repo>\app`.
- **The specs.** The main spec changes only in the tasks that name the change (Task 5: two §5.9 rows; Task 11: §5.3, §5.7, §5.8, appendix B); edit the file as it is in the working copy, never from an older commit. The recovery spec is the founder's: this plan does not edit it.
- **Commits:** write the English message with the Write tool to `<repo>\.git\CMDSTUDIO_COMMIT_MSG` (Read it first if it exists), check that the Write succeeded, then in one Bash call run `head -1 .git/CMDSTUDIO_COMMIT_MSG` and `git commit -F .git/CMDSTUDIO_COMMIT_MSG`. Never batch the Write with the commit. Stage only the task's files with `git add -- <files>` and check `git diff --cached --name-only`. No `Co-Authored-By` line, no "Generated with" line, no link to the assistant's website, nothing that names the AI assistant or its maker; never `--no-verify`; never push; follow the `auroraz-pr-workflow` skill.
- Scratch work goes under the session's scratchpad (written `<scratchpad>` below; the session gives its path), with its own `CARGO_TARGET_DIR`, never the repository's `target\`; scratch npm runs set `npm_config_cache` under the scratchpad. No user name of this PC in a committed file: tests use `C:\Users\tester\…`, docs `<you>` or `%USERPROFILE%`. VM test accounts (`tester`, `ผู้ทดสอบ ไทย`) are not this PC's and may appear in spike records.

## Review Focus

The five inputs the recovery spec implies but its own test list does not exercise, most likely to bite first. Each has its test in the task that owns the code.

1. **CMD Studio's own `clink set` loads init.lua.** Every apply's check (`verify.rs`, `clink --profile <ours> set clink.logo`) and doctor D3 (run on every visit to Home) start Clink's own program, which loads every script on `clink.path` and never shows a prompt. A counter that wrote a tag there would pause the theme after three Home visits. Only a cmd window counts (`os.gethost()`). Task 3: `cmd_studios_own_clink_set_never_counts`.
2. **A tag another window is still writing.** Windows Terminal opens several tabs at once; `io.open` makes the file before its text is in it, so another window can find it empty. It must be neither counted nor taken away. Task 3: `a_tag_another_window_is_writing_is_left_alone`.
3. **A `paused.json` that is damaged, half written or from a newer version.** init.lua pauses on the file being there, whatever it holds, so the app must still say "paused" (at the cmd level) and "Turn the theme back on" must delete it. Task 2: `a_pause_file_that_cannot_be_read_as_one_is_still_a_pause_in_cmd`; Task 3: `a_damaged_pause_file_still_pauses`; Task 7: `a_pause_record_that_cannot_be_read_is_a_pause_in_cmd_and_turning_back_on_lifts_it`.
4. **Windows' crash reports that are off, denied or slow.** Never a pause, never "no crash", never a slower start: the read gives up after five seconds on its own thread, D12 says `skipped` with the reason, the start check logs one line. Task 4: `an_answer_in_time_is_the_answer_and_a_late_one_an_error`; Task 5: `d12_never_takes_crash_reports_it_cannot_read_for_none`; Task 7: `two_crashes_or_reports_that_cannot_be_read_pause_nothing`.
5. **An apply by any path while the theme is paused.** The wizard, Themes, the quick switch, a doctor fix and the CLI's `apply` all commit an apply; a pause left behind would keep every cmd window plain under the new theme. Every committed apply lifts the pause and its tags. Task 6: `an_apply_lifts_the_pause_and_every_tag`; Task 7: `an_apply_through_the_plan_lifts_any_pause`.

## Where this plan departs from the recovery spec

These follow from the shipped code; the report to the founder lists them (file:line at `224e910`).

- **D12, not D10** (recovery spec §4.5): D10 is the packaged-app check (`crates/core/src/doctor.rs:972-996`, main spec §5.8 row D10).
- **Two new §5.9 codes**, `E_THEME_PAUSED` and `E_CMD_CRASHED` (recovery spec §12.5 names only `E_TIMEOUT`): doctor's rule that every `warn`/`fail` has a code (`doctor.rs:1588-1591`) and the UI's code badge need them.
- **The counter counts cmd windows only** (recovery spec §3.2 step 3 writes a tag unconditionally): see Review Focus 1 (`verify.rs:313-330`, `doctor.rs:300-311`, `generate/lua.rs:4-5`).
- **"The first prompt" is a prompt filter of CMD Studio's after Oh My Posh's** (the spec's primary is `clink.onbeginedit`): Clink sends `onbeginedit` before the prompt filters (`spikes/vm/guest/proto-setup.ps1:61`), and Oh My Posh 31.5.0 registers `clink.promptfilter(_ZL_CLINK_PROMPT_PRIORITY + 1)` whose `filter` returns one value (it never ends the chain). Spike S6 confirms; fallback F5 if not.
- **The start's crash check runs on a thread of `lib.rs`, and `recovery_status` waits for it** (≤ 30 s), so `recovery_status` changes nothing (recovery spec §8) and no new event is needed (main spec §5.7: `apply-progress` is the only event).
- **Help's "if cmd won't open" paragraph** points to opening CMD Studio and "Pause the theme": the Start-menu shortcut CMD Studio Recovery of recovery spec §4.4 comes with `--recovery` (3b-2; `app/src-tauri/src/main.rs:13-17` takes any argument as the CLI) and phase 4's installer. 3b-2 adds the shortcut's sentence.
- **The Lua tests use in-memory files**, not a temp folder (recovery spec §10): mlua's `io.open` is Lua's own `fopen`, which takes the ANSI code page on Windows, unlike Clink's; Thai paths are the spike's (S2).
- **The app's "Undo everything" (`Backend::revert_all`) lifts a pause in cmd** after a revert that went fully well (the banner's "Undo everything", recovery spec §4.2); the core's `revert_all` still never touches `paused.json` (§7). A pause of this computer stays: it holds the theme to turn back on.
- **The time "since the last apply"** (recovery spec §4.3, §0) is `Applied.applied_at_ms` (`crates/core/src/state.rs:88`): the clock when the apply was prepared, a moment before its commit.

## Spike results the later tasks depend on

Task 1 records each result in `docs/spikes/phase3b-recovery/results.md`. The later tasks are written for the "assumes" column. When a result differs, the task named applies the fallback given below, word for word, and its commit message names the spike result.

| # | What the spike shows | Tasks 2–12 assume | Used by | If not |
|---|---|---|---|---|
| S1 | `os.getpid()` in a cmd window | a number: cmd.exe's process id | Task 3 | F1 |
| S2 | `io.open` (write, read) and `os.remove` on `<data>\health\` under a Thai user name (VM-C) | they work on the UTF-8 path | Tasks 3, 4 | F2 |
| S3 | `os.globfiles(<health>\start-*)` | a table of the matching files, names alone or whole paths (both are handled) | Task 3 | F3 |
| S4 | `os.gethost()` in a cmd window and in `clink set` | ends in `\cmd.exe` in a cmd window, and not in `clink set` (Clink's own exe) | Task 3 | F4 |
| S5 | `clink set` loads the scripts on `clink.path` | yes (main spec §2.2 #4): Review Focus 1 is real | Task 3 | none: the host check is harmless either way |
| S6 | the order: `oninject`, `onbeginedit`, a prompt filter at priority 0, Oh My Posh's (1), one at 2 | `onbeginedit` before the filters; the filter at 2 runs after Oh My Posh's and sees its prompt (an escape sequence in it) | Task 3 | F5 |
| S7 | `cmd /c` with AutoRun on | Clink does not inject: no probe line from it | Task 3 | F6 |
| S8 | the counter's cost per cmd start | ≤ 5 ms more from cmd's start to its first prompt (recovery spec §0); the counter's own `os.clock` time well under it | Task 3 | F7 |
| S9 | event 1000 of cmd.exe crashing in a module named `clink_dll_x64.dll`, its EventData, on English and Thai Windows | named fields `AppName` = `cmd.exe` and `ModuleName` = `clink_dll_x64.dll`, the same names on both | Task 4 | F8 |
| S10 | the XPath query of `eventlog::crash_query` through `Get-WinEvent -FilterXPath` | accepted, and it finds the crashes of S9 | Task 4 | F9 |
| S11 | Clink injects the crash stand-in (its version check passes) and cmd crashes in it | yes | Tasks 1, 4, 12 | F10 |

### The fallbacks

- **F1 (no `os.getpid`).** In `HEALTH_BODY` (Task 3 Step 3), replace the line `  local tag = health .. "\\start-" .. os.getpid()` with these three lines, and in `tests/health_lua.rs` let the `__pid` stand-in stay unused (the test `a_first_window_leaves_its_tag_and_loads_the_prompt_and_the_box` then matches the tag by `start-\d+` instead of `start-100`):
  ```lua
  math.randomseed(math.floor(os.clock() * 1000000) + os.time())
  local id = os.getpid and os.getpid() or string.format("%.0f%06d", os.time(), math.random(0, 999999))
  local tag = health .. "\\start-" .. id
  ```
- **F2 (Thai paths fail in Clink's file functions, ASCII ones work).** init.lua is given the health folder through the data folder's 8.3 path, and no folder (no counter) when there is none. `init_lua`'s third parameter becomes `health: Option<&Path>` (`local health = nil` for `None`), `HEALTH_BODY` is wrapped in `if health then` … `end` (its `return` stays the last statement of its block), and the planner (Task 3 Step 6) passes `lua_health_dir(paths, machine).as_deref()` with this function in `crates/core/src/plan.rs` beside `is_ascii`:
  ```rust
  /// The health folder as init.lua is given it (spike S2: Clink's file functions take the ANSI
  /// code page): `<data>\health` when it is ASCII, else the same folder through the data folder's
  /// 8.3 path, else none, and the counter is off.
  fn lua_health_dir(paths: &Paths, machine: &Machine) -> Option<PathBuf> {
      let long = paths.health();
      if is_ascii(&long) {
          return Some(long);
      }
      machine
          .data_short
          .as_ref()
          .map(|short| Paths { data: short.clone() }.health())
          .filter(|p| is_ascii(p))
  }
  ```
  `golden.rs` and `english_in_cmd.rs` pass `Some(&under_data("health"))`; `tests/health_lua.rs` passes `Some(Path::new(HEALTH))` and adds a test that `None` writes no tag. The app and the CLI keep `Paths::health()` for their own file work: the 8.3 path names the same folder.
- **F3 (no `os.globfiles`).** In `HEALTH_BODY`'s `tags()`, replace `os.globfiles(health .. "\\start-*")` with `(os.globfiles or clink.find_files)(health .. "\\start-*")`. If neither exists, stop and report: the counter cannot list its tags.
- **F4 (`os.gethost` missing, or not telling `clink set` from cmd).** Every program CMD Studio runs gets a variable that says so, and init.lua counts only without it. In `HEALTH_BODY` replace the body of `in_cmd` with `return os.getenv("CMDSTUDIO_CHECKING") == nil`. In `crates/core/src/adapters/windows/process.rs`, `WinProcess::run`, add `.env("CMDSTUDIO_CHECKING", "1")` after `.args(args)` on the `Command`, with the comment `// init.lua counts a cmd window's start; Clink's own program run from here is none (spike S4).`; and in `tests/health_lua.rs` the `clink set` case sets `__env.CMDSTUDIO_CHECKING = "1"` instead of the host. A person's own `clink set` with CMD Studio's profile would then count; nothing of CMD Studio's runs one.
- **F5 (the filter at priority 2 does not run after Oh My Posh's, or does not see its prompt).** "The first prompt" becomes the first edit line that ends: in `HEALTH_BODY` replace the block `if counted and is_cmd and clink.promptfilter then … end` with
  ```lua
  if counted and is_cmd and clink.onendedit then
    -- The first line typed (or an empty Enter): the prompt was there (spike S6, fallback F5).
    local reached = false
    clink.onendedit(function()
      if reached then return end
      reached = true
      pcall(function()
        if own_tag then os.remove(own_tag) end
        for _, path in ipairs(stale(os.time())) do os.remove(path) end
      end)
    end)
  end
  ```
  In `tests/health_lua.rs` add `clink.onendedit = function(f) __endedit = f end` to `STUBS` and make the "prompt" step call `__endedit()` after `__prompt()`. Its cost: a window closed at its first prompt without Enter counts as one that failed; the report to the founder says so.
- **F6 (`cmd /c` injects).** Stop before Task 3 and report: recovery spec §11's premise (only interactive windows reach Clink) does not hold, and every `io.popen` would leave a tag.
- **F7 (more than 5 ms).** Record the numbers and go on; the founder decides whether to keep the counter (the spec's 5 ms is a target). No code changes in this plan.
- **F8 (other field names, or none).** `parse_crash` reads fields by position when none has a name (program first, module fourth). When the fields are named otherwise, put the recorded names into `APP_FIELD` and `MODULE_FIELD` in `crates/core/src/eventlog.rs` (Task 4 Step 3) and say so in their comment.
- **F9 (the `timediff` query is refused).** In `crash_query`, drop `and TimeCreated[timediff(@SystemTime) <= {within_ms}]` (keep the parameter, named `_within_ms`, for the call sites); `clink_crashes` already keeps only the crashes at or after `since`, and the read is newest first, at most `MOST_EVENTS`. Change the test `the_query_is_the_one_the_spike_ran` to the new text.
- **F10 (Clink refuses the stand-in, or cmd does not crash in it).** Get the event from a program named `cmd.exe` that loads the stand-in itself. Build `spikes/recovery/fake-cmd/` (below) as Task 1 Step 2 builds the DLL, copy `target\release\fake-cmd.exe` to the VM as `C:\cmdstudio\fake\cmd.exe`, and run `C:\cmdstudio\fake\cmd.exe C:\cmdstudio\crash-dll\clink_dll_x64.dll` three times (`Invoke-GuestDesktop` with a one-line script, or Win+R). The event's AppPath differs (`C:\cmdstudio\fake\cmd.exe`): `is_clink_crash` reads file names only, so the fixture serves the same. Task 12's crash steps then use this program instead of a cmd window, and its report says so.

  `spikes/recovery/fake-cmd/Cargo.toml`:
  ```toml
  [package]
  name = "fake-cmd"
  version = "0.0.0"
  edition = "2021"
  publish = false

  [workspace]
  ```
  `spikes/recovery/fake-cmd/src/main.rs`:
  ```rust
  //! A program named cmd.exe on the test VM that loads the crash stand-in named
  //! clink_dll_x64.dll, for spike S11's fallback: Windows then records an event 1000 with
  //! AppName cmd.exe and ModuleName clink_dll_x64.dll. Test VMs only.
  use std::os::windows::ffi::OsStrExt;

  #[link(name = "kernel32")]
  extern "system" {
      fn LoadLibraryW(name: *const u16) -> *mut core::ffi::c_void;
  }

  fn main() {
      let path: Vec<u16> = std::env::args_os()
          .nth(1)
          .map(|a| a.encode_wide().chain(Some(0)).collect())
          .unwrap_or_else(|| vec![0]);
      // SAFETY: a NUL-terminated UTF-16 path; the module it names crashes this process on
      // purpose when it loads.
      unsafe { LoadLibraryW(path.as_ptr()) };
  }
  ```

## File map

```
spikes/recovery/probe.lua, counter.lua, omp-init.lua, timing.lua            the spike's Lua (Task 1)
spikes/recovery/crash-dll/{Cargo.toml, src/lib.rs, version.rc}               crash stand-in, VMs only (Task 1)
spikes/vm/guest/recovery-{setup,report,cmdc,timing,crash}.ps1                guest scripts (Task 1)
spikes/vm/guest/guard.test.ps1                                               the new scripts listed (Task 1)
docs/spikes/phase3b-recovery/results.md + raw outputs + event1000-{en,th}.xml  the controller's record (Task 1)
crates/core/src/recovery.rs, utc.rs, lib.rs, app/ui/src/bindings/{PauseReason,PausedBy}.ts  paths, paused.json, numbers, UTC text (Task 2)
crates/core/src/generate/lua.rs, plan.rs                                     the counter in init.lua, the box waits (Task 3)
crates/core/tests/health_lua.rs, banner_lua.rs, golden.rs, english_in_cmd.rs, golden/{init.lua, terracotta.banner.en.lua}  (Task 3)
crates/core/Cargo.toml, src/eventlog.rs, src/adapters/{mod.rs, fake.rs, windows/mod.rs, windows/eventlog.rs}  EventLog (Task 4)
crates/core/tests/fixtures/eventlog/event1000-{en,th}.xml                     from Task 1 (Task 4)
crates/cli/src/lib.rs, crates/cli/tests/system.rs, app/src-tauri/src/{machine.rs, library.rs}  System gets the event log (Task 4)
crates/core/src/doctor.rs, app/src-tauri/src/error.rs, main spec §5.9        D12 and its two codes (Task 5)
crates/cli/src/lib.rs (tests), app/src-tauri/src/backend/doctor.rs (test)    twelve checks (Task 5)
crates/cli/src/lib.rs                                                        --cleanup knows health\; apply lifts the pause (Task 6)
app/src-tauri/src/backend/{recovery.rs, mod.rs, apply.rs, doctor.rs}, logs.rs, app/ui/src/bindings/*  the app's pause (Task 7)
app/src-tauri/src/{command_names.rs, ipc.rs, lib.rs}, capabilities/default.json  IPC (Task 8)
app/ui/src/ipc/{types.ts, tauri.ts, tauri.test.ts, mock.ts, mock.test.ts}    the UI's IPC (Task 9)
app/ui/src/components/Pause.tsx, screens/{Home.tsx, Help.tsx, Help.test.tsx}, i18n/{en.ts, th.ts}, App.test.tsx  (Task 10)
docs/superpowers/specs/2026-10-10-cmd-studio-design.md                       §5.3, §5.7, §5.8, appendix B (Task 11)
spikes/recovery/hang/{Cargo.toml, src/main.rs}, spikes/vm/guest/{recovery-hang.ps1, guard.test.ps1}, docs/spikes/phase3b-recovery/vm-check.md  the VM check (Task 12)
```

Commands run in `<repo>` unless said otherwise; UI commands in `<repo>\app`.

---


### Task 1: The VM spike of recovery spec §0 (the controller runs it)

Recovery spec §0 lists what must be seen on Windows before the counter is written: Clink 1.9.34's `os.getpid`, file functions on a Thai path, the listing of `health\`, when `onbeginedit` and the prompt filters run against Oh My Posh's, the cost per start, and a real event 1000 of cmd.exe crashing in a DLL named `clink_dll_x64.dll`, on English and Thai Windows. This task writes the scripts (an implementer may write them, and checks them by parsing only), builds the crash stand-in on this PC (building only), and the controller runs everything on VM-A (`cmdstudio-win11`, user `tester`) and VM-C (`cmdstudio-th`, user `ผู้ทดสอบ ไทย`) from their `clean` snapshots, with `spikes/vm/vm.ps1` (its README covers the helpers), and records S1–S11 of "Spike results the later tasks depend on".

Oh My Posh 31.5.0's own `omp.lua` (tag `v31.5.0`, `src/shell/scripts/omp.lua`), read from GitHub while this plan was written: it registers one prompt filter, `local p = clink.promptfilter(zl_prompt_priority + 1)` with `local zl_prompt_priority = get_priority_number('_ZL_CLINK_PROMPT_PRIORITY', 0)`, so priority 1 unless `_ZL_CLINK_PROMPT_PRIORITY` is set; `p:filter` returns one value on every path (`return cached_prompt.left`), and only `p:rightfilter` and `p:transientrightfilter` return `false`; it registers `clink.onbeginedit` and `clink.onendedit` and no `clink.oninject`; it runs no `io.popen` at load (only inside its callbacks). Nobody runs Oh My Posh or Clink on this PC.

**Files:**
- Create: `spikes/recovery/probe.lua`, `spikes/recovery/counter.lua`, `spikes/recovery/omp-init.lua`, `spikes/recovery/timing.lua`
- Create: `spikes/recovery/crash-dll/Cargo.toml`, `spikes/recovery/crash-dll/src/lib.rs`, `spikes/recovery/crash-dll/version.rc`
- Create: `spikes/vm/guest/recovery-setup.ps1`, `recovery-report.ps1`, `recovery-cmdc.ps1`, `recovery-timing.ps1`, `recovery-crash.ps1`
- Modify: `spikes/vm/guest/guard.test.ps1` (`$changers`)
- Create (the controller, from the VM runs): `docs/spikes/phase3b-recovery/results.md`, `probe-vm-a.txt`, `probe-vm-c.txt`, `timing-vm-a.txt`, `timing-vm-c.txt`, `crash-vm-a.txt`, `crash-vm-c.txt`, `event1000-en.xml`, `event1000-th.xml`

**Interfaces:**
- Consumes: `spikes/vm/vm.ps1` (`Invoke-GuestPs`, `Invoke-GuestDesktop`, `Copy-ToGuest`, `Send-Run`, `Save-Shot`, `Wait-GuestReady`), the repo's `resources\clink\*` and `resources\omp\oh-my-posh.exe`, `crates/core/tests/golden/terracotta.box.omp.json` and `terracotta.banner.en.lua`.
- Produces: `docs/spikes/phase3b-recovery/results.md` with S1–S11 filled in; `event1000-en.xml` and `event1000-th.xml`, which Task 4 copies to `crates/core/tests/fixtures/eventlog/`. `spikes/recovery/counter.lua` is the text of Task 3's `HEALTH_BODY`, and `omp-init.lua` the text of `INIT_BODY` (`crates/core/src/generate/lua.rs`): if a fallback changes the one, it changes the other.

- [ ] **Step 1: Write the Lua the VMs load**

`spikes/recovery/probe.lua` (loaded first, as `aa-probe.lua`; `recovery-setup.ps1` writes `local p3b_data = [==[<data folder>]==]` in front of it):

```lua
-- Phase 3b-1 spike probes (recovery spec §0; plan 3b-1 Task 1). recovery-setup.ps1 loads this
-- first on clink.path as aa-probe.lua, with `local p3b_data = [==[<data folder>]==]` in front.
-- Each line is "p3b <pid> ...": in clink.log, in <data>\probe.txt (when io.open works there), and
-- printed when the host is not cmd (clink set), so recovery-setup.ps1's own `clink set` shows it.
local pid = os.getpid and os.getpid() or "nopid"
local host = os.gethost and os.gethost() or "nohost"
local in_cmd = tostring(host):lower():find("\\cmd%.exe$") ~= nil
local function say(what)
  local line = "p3b " .. tostring(pid) .. " " .. what
  log.info(line)
  local f = io.open(p3b_data .. "\\probe.txt", "ab")
  if f then
    f:write(line .. "\n")
    f:close()
  end
  if not in_cmd then print(line) end
end
local seq = 0
local function step(name)
  seq = seq + 1
  say("order " .. seq .. " " .. name)
end
-- S1, S4, S5: the process id, the host program, the clock, Oh My Posh's priority variable.
say("load host=" .. tostring(host) .. " time=" .. tostring(os.time()) ..
  " zl=" .. tostring(os.getenv("_ZL_CLINK_PROMPT_PRIORITY")))
say("api getpid=" .. type(os.getpid) .. " gethost=" .. type(os.gethost) .. " globfiles=" ..
  type(os.globfiles) .. " find_files=" .. type(clink.find_files) .. " promptfilter=" ..
  type(clink.promptfilter) .. " onbeginedit=" .. type(clink.onbeginedit) .. " onendedit=" ..
  type(clink.onendedit) .. " oninject=" .. type(clink.oninject) .. " clock=" ..
  string.format("%.6f", os.clock()))
-- S2, S3: write, read, list and delete a tag in health\, whose path is Thai on VM-C.
local health = p3b_data .. "\\health"
local ok, err = pcall(function()
  local tag = health .. "\\start-" .. tostring(pid)
  local f, why = io.open(tag, "wb")
  say("io.open write ok=" .. tostring(f ~= nil) .. " why=" .. tostring(why))
  if f then
    f:write("v1 " .. os.time() .. "\n")
    f:close()
  end
  local r, why2 = io.open(tag, "rb")
  local text = r and r:read("*a")
  if r then r:close() end
  say("io.open read text=" .. tostring(text and text:gsub("\n", "\\n")) .. " why=" .. tostring(why2))
  local list = os.globfiles(health .. "\\start-*")
  say("globfiles count=" .. tostring(list and #list) .. " first=" .. tostring(list and list[1]))
  local removed, why3 = os.remove(tag)
  say("os.remove ok=" .. tostring(removed) .. " why=" .. tostring(why3))
  local again = io.open(tag, "rb")
  say("gone after remove=" .. tostring(again == nil))
  if again then again:close() end
end)
say("files pcall ok=" .. tostring(ok) .. " err=" .. tostring(err))
-- S6: when each hook runs, against Oh My Posh's prompt filter (priority 1, loaded by init.lua).
if clink.oninject then clink.oninject(function() step("oninject") end) end
if clink.onbeginedit then
  local first = true
  clink.onbeginedit(function()
    if first then
      first = false
      step("onbeginedit")
    end
  end)
end
if clink.promptfilter then
  local before, after = clink.promptfilter(0), clink.promptfilter(2)
  local seen0, seen2 = false, false
  function before:filter(prompt)
    if seen0 then return end
    seen0 = true
    step("filter-0 bytes=" .. #prompt .. " esc=" .. tostring(prompt:find("\27") ~= nil))
  end
  function after:filter(prompt)
    if seen2 then return end
    seen2 = true
    step("filter-2 bytes=" .. #prompt .. " esc=" .. tostring(prompt:find("\27") ~= nil))
  end
end
```

`spikes/recovery/counter.lua` (the same Lua as `HEALTH_BODY` in Task 3 Step 3, byte for byte, with a newline at the end; `recovery-setup.ps1 -Counter` puts the five `local` lines `init_lua` writes in front of it):

```lua
-- The start counter (recovery spec §3): each cmd window leaves a tag health\start-<pid> that
-- holds "v1 <unix seconds>" until its first prompt. PAUSE_AFTER tags older than STALE_AFTER
-- seconds are that many windows in a row that never got there, and the theme is paused in cmd:
-- health\paused.json. Only a cmd window counts: CMD Studio's own `clink set` (the apply's check,
-- doctor D3) loads this file too and never shows a prompt. A file that cannot be read or written
-- skips the counter: the theme is never paused for that.
local paused_file = health .. "\\paused.json"
local own_tag = nil
local paused_count = 0
local function in_cmd()
  local host = os.gethost and os.gethost() or ""
  return host:lower():find("\\cmd%.exe$") ~= nil
end
local host_ok, is_cmd = pcall(in_cmd)
is_cmd = host_ok and is_cmd
local function tag_time(path)
  local f = io.open(path, "rb")
  if not f then return nil end
  local text = f:read("*a") or ""
  f:close()
  local t = text:match("^v1 (%d+)%s*$")
  return t and tonumber(t)
end
local function tags()
  local found = {}
  for _, name in ipairs(os.globfiles(health .. "\\start-*") or {}) do
    local file = name:match("([^\\/]+)$")
    if file and file:match("^start%-%d+$") then found[#found + 1] = health .. "\\" .. file end
  end
  return found
end
local function stale(now)
  local old = {}
  for _, path in ipairs(tags()) do
    local t = tag_time(path)
    if t and now - t > STALE_AFTER then old[#old + 1] = path end
  end
  return old
end
local function count_start()
  local f = io.open(paused_file, "rb")
  if f then
    f:close()
    return true
  end
  if not is_cmd then return false end
  local now = os.time()
  local old = stale(now)
  if #old >= PAUSE_AFTER then
    local p = io.open(paused_file, "wb")
    if p then
      p:write(string.format('{"version":1,"reason":"cmdStarts","at":%.0f,"count":%d,"by":"clink"}\n', now * 1000, #old))
      p:close()
      for _, path in ipairs(tags()) do os.remove(path) end
      paused_count = #old
      return true
    end
  end
  local tag = health .. "\\start-" .. os.getpid()
  local t = io.open(tag, "wb")
  if t then
    t:write("v1 " .. now .. "\n")
    t:close()
    own_tag = tag
  end
  return false
end
local counted, paused = pcall(count_start)
if counted and paused then
  -- No prompt and no welcome box (banner.lua reads CMDSTUDIO_PAUSED), and one English line in a
  -- cmd window, once for it and the cmd windows started from it.
  CMDSTUDIO_PAUSED = true
  if is_cmd and clink.oninject then
    clink.oninject(function()
      if os.getenv("CMDSTUDIO_PAUSE_SAID") then return end
      os.setenv("CMDSTUDIO_PAUSE_SAID", "1")
      if paused_count > 0 then
        clink.print(string.format(PAUSED_NOW, paused_count))
      else
        clink.print(PAUSED_STILL)
      end
    end)
  end
  return
end
if counted and is_cmd and clink.promptfilter then
  -- Oh My Posh registers its prompt filter at _ZL_CLINK_PROMPT_PRIORITY + 1 (1 when that is
  -- unset), and Clink calls the lower number first: this one runs once Oh My Posh's has returned.
  local first = clink.promptfilter((tonumber(os.getenv("_ZL_CLINK_PROMPT_PRIORITY") or "") or 0) + 2)
  local reached = false
  function first:filter()
    if reached then return end
    reached = true
    pcall(function()
      if own_tag then os.remove(own_tag) end
      for _, path in ipairs(stale(os.time())) do os.remove(path) end
    end)
  end
end
```

`spikes/recovery/omp-init.lua` (the Lua of `INIT_BODY` in `crates/core/src/generate/lua.rs` as it is at the start of this plan, byte for byte, with a newline at the end):

```lua
-- cmd /c drops the outermost quotes, so the command is wrapped once more (spec gate G17).
-- 2>nul sits inside them: io.popen pipes stdout only, so without it the error text of cmd
-- (a missing exe) would be printed in every window.
local handle = io.popen('"' .. '"' .. omp .. '" init cmd --config "' .. config .. '" 2>nul' .. '"')
local script = handle and handle:read("*a") or ""
if handle then handle:close() end
if script == "" then
  log.info("cmdstudio: oh-my-posh init returned nothing")
  return
end
local chunk, err = load(script)
if not chunk then
  log.info("cmdstudio: cannot load oh-my-posh init: " .. tostring(err))
  return
end
-- A script that parses but raises must not print an error in every window either.
local ok, failure = pcall(chunk)
if not ok then
  log.info("cmdstudio: oh-my-posh init failed: " .. tostring(failure))
  return
end
log.info("cmdstudio: prompt loaded")
```

`spikes/recovery/timing.lua` (loaded last, as `zz-timing.lua`, with `p3b_data` in front):

```lua
-- Marks the first prompt of each cmd window for recovery-timing.ps1: a prompt filter after every
-- other one (priority 100) writes <data>\first-prompt-<pid>.txt once.
local marked = false
local mark = clink.promptfilter(100)
function mark:filter()
  if marked then return end
  marked = true
  local f = io.open(p3b_data .. "\\first-prompt-" .. tostring(os.getpid()) .. ".txt", "w")
  if f then
    f:write("ok")
    f:close()
  end
end
```

- [ ] **Step 2: Write the crash stand-in and build it on this PC (building only, never started)**

`spikes/recovery/crash-dll/Cargo.toml`:

```toml
[package]
name = "crash-dll"
version = "0.0.0"
edition = "2021"
publish = false

# Not a member of CMD Studio's workspace: a test-only DLL for the VMs.
[workspace]

[lib]
crate-type = ["cdylib"]

[profile.release]
panic = "abort"
```

`spikes/recovery/crash-dll/src/lib.rs`:

```rust
//! A stand-in for Clink's `clink_dll_x64.dll` that crashes the process loading it, at once: an
//! illegal instruction (exception c000001d) inside this module, so that Windows records an
//! "Application Error" event 1000 for cmd.exe with this module as the faulting one (recovery
//! spec §0, §10). Test VMs only: it never goes into the installer, nor onto the dev PC's own
//! runtime.

/// `DLL_PROCESS_ATTACH`: the process is loading this module.
const DLL_PROCESS_ATTACH: u32 = 1;

/// The module's entry point, which the C runtime's own entry point calls.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(
    _module: *mut core::ffi::c_void,
    reason: u32,
    _reserved: *mut core::ffi::c_void,
) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        // SAFETY: ud2 raises the illegal instruction exception on purpose; it never returns.
        unsafe { core::arch::asm!("ud2", options(noreturn)) }
    }
    1
}
```

`spikes/recovery/crash-dll/version.rc` (Clink's loader checks the DLL's version numbers before it injects it, `check_dll_version`; the strings say what this is, not Clink):

```
// The version resource Clink's loader checks before it injects its DLL (check_dll_version): the
// numbers of Clink 1.9.34, which this stand-in is not. Names say what it is.
1 VERSIONINFO
FILEVERSION 1,9,34,0
PRODUCTVERSION 1,9,34,0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x2L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "CMD Studio test VMs only"
            VALUE "FileDescription", "CMD Studio crash stand-in (not Clink)"
            VALUE "FileVersion", "1.9.34.4e8ae4"
            VALUE "InternalName", "crash-dll"
            VALUE "ProductName", "CMD Studio crash stand-in"
            VALUE "ProductVersion", "1.9.34.4e8ae4"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
```

Build it into the scratchpad (PowerShell, in `<repo>\spikes\recovery\crash-dll`, so `rust-toolchain.toml`'s 1.99.0 applies); `rc.exe` is the Windows SDK's, and nothing built here is started:

```powershell
$out = "<scratchpad>\crash-dll"   # the session's scratchpad folder
New-Item -ItemType Directory -Force $out | Out-Null
$rc = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin\*\x64\rc.exe' | Sort-Object FullName | Select-Object -Last 1
& $rc.FullName /nologo /fo "$out\version.res" version.rc
$env:CARGO_TARGET_DIR = "$out\target"
cargo rustc --release --lib -- -C "link-arg=$out\version.res"
Remove-Item Env:CARGO_TARGET_DIR
Copy-Item "$out\target\release\crash_dll.dll" "$out\clink_dll_x64.dll"
$v = (Get-Item "$out\clink_dll_x64.dll").VersionInfo
"$($v.FileVersion) $($v.FileMajorPart).$($v.FileMinorPart).$($v.FileBuildPart).$($v.FilePrivatePart)"
```

Expected: `1.9.34.4e8ae4 1.9.34.0` (checked while this plan was written, with SDK 10.0.26100.0).

- [ ] **Step 3: Write `spikes/vm/guest/recovery-setup.ps1`**

```powershell
# Runs inside a test VM as the test user (phase 3b-1 spike, recovery spec §0). Lays out Clink
# 1.9.34 and Oh My Posh 31.5.0 from the repo's resources\ under the user's own LOCALAPPDATA (a
# Thai path with a space on VM-C), as CMD Studio does (spec §5.4), with these scripts on
# clink.path:
#   aa-probe.lua   the Clink API probes (spikes\recovery\probe.lua), unless -NoProbe
#   banner.lua     the Terracotta welcome box as the generator writes it (golden file)
#   init.lua       Oh My Posh loaded as the generator loads it, with the start counter
#                  (spikes\recovery\counter.lua) in front when -Counter is given
#   zz-timing.lua  marks each window's first prompt for recovery-timing.ps1
# and CMD Studio's guarded AutoRun entry. Clink starts through the 8.3 path of the data folder
# when the long one is not ASCII (G19), as the app does. -Remove puts AutoRun back.
param([switch] $Counter, [switch] $NoProbe, [switch] $Remove)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script rewrites the user's cmd AutoRun" }
$key = 'HKCU:\Software\Microsoft\Command Processor'
# Never New-Item -Force on this key: it wipes every value and subkey.
if (-not (Test-Path $key)) { New-Item -Path $key | Out-Null }
$data = Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike3b'
# The user's AutoRun as it was before the first setup: text unexpanded, and its type.
$before = Join-Path $data 'autorun-before.json'
if ($Remove) {
  if (-not (Test-Path $before)) { 'no saved AutoRun, nothing restored'; return }
  $saved = Get-Content -LiteralPath $before -Raw | ConvertFrom-Json
  if ($saved.Absent) { Remove-ItemProperty $key AutoRun -ErrorAction SilentlyContinue }
  else { New-ItemProperty -Path $key -Name AutoRun -Value $saved.Data -PropertyType $saved.Kind -Force | Out-Null }
  Remove-Item -LiteralPath $before
  "AutoRun restored ($(if ($saved.Absent) { 'there was none' } else { $saved.Kind }))"
  return
}
$utf8 = New-Object Text.UTF8Encoding $false
$gen = Join-Path $data 'generated'
$profileDir = Join-Path $data 'clink-profile'
$health = Join-Path $data 'health'
$clinkDir = Join-Path $data 'runtime\clink-1.9.34'
$ompDir = Join-Path $data 'runtime\omp-31.5.0'
New-Item -ItemType Directory -Force $gen, $profileDir, $health, $clinkDir, $ompDir | Out-Null
if (-not (Test-Path $before)) {
  $k = Get-Item $key
  $saved = if ($k.GetValueNames() -contains 'AutoRun') { @{ Data = $k.GetValue('AutoRun', $null, 'DoNotExpandEnvironmentNames'); Kind = "$($k.GetValueKind('AutoRun'))" } } else { @{ Absent = $true } }
  $saved | ConvertTo-Json | Set-Content -LiteralPath $before -Encoding UTF8
}
# The real DLL every time: a setup after recovery-crash.ps1 -Swap puts it back.
Copy-Item 'C:\cmdstudio\resources\clink\*' $clinkDir -Force
Copy-Item 'C:\cmdstudio\resources\omp\oh-my-posh.exe' $ompDir -Force
$omp = Join-Path $ompDir 'oh-my-posh.exe'
$config = Join-Path $gen 'theme.omp.json'
Copy-Item 'C:\cmdstudio\golden\terracotta.box.omp.json' $config -Force
Copy-Item 'C:\cmdstudio\golden\terracotta.banner.en.lua' (Join-Path $gen 'banner.lua') -Force
function Lua-Long([string] $s) { "[==[$s]==]" }
$read = { param($name) [IO.File]::ReadAllText("C:\cmdstudio\spikes\recovery\$name") }
$probe = Join-Path $gen 'aa-probe.lua'
Remove-Item -LiteralPath $probe -ErrorAction SilentlyContinue
if (-not $NoProbe) { [IO.File]::WriteAllText($probe, "local p3b_data = $(Lua-Long $data)`n" + (& $read 'probe.lua'), $utf8) }
# The five locals init_lua writes in front of its counter (generate::lua::init_lua).
$head = "local omp = $(Lua-Long $omp)`nlocal config = $(Lua-Long $config)`nlocal health = $(Lua-Long $health)`n" +
  "local STALE_AFTER = 60`nlocal PAUSE_AFTER = 3`n" +
  "local PAUSED_NOW = `"CMD Studio paused its theme because cmd failed to start %d times. Open CMD Studio to turn it back on.`"`n" +
  "local PAUSED_STILL = `"CMD Studio's theme is paused. Open CMD Studio to turn it back on.`"`n"
$counterText = ''
if ($Counter) { $counterText = "local p3b_t0 = os.clock()`n" + (& $read 'counter.lua') + "log.info(string.format('p3b counter ms=%.3f', (os.clock() - p3b_t0) * 1000))`n" }
[IO.File]::WriteAllText((Join-Path $gen 'init.lua'), $head + $counterText + (& $read 'omp-init.lua'), $utf8)
[IO.File]::WriteAllText((Join-Path $gen 'zz-timing.lua'), "local p3b_data = $(Lua-Long $data)`n" + (& $read 'timing.lua'), $utf8)
Remove-Item (Join-Path $data 'probe.txt'), (Join-Path $profileDir 'clink.log') -ErrorAction SilentlyContinue
Get-ChildItem -LiteralPath $data -Filter 'first-prompt-*.txt' | Remove-Item
Get-ChildItem -LiteralPath $health -Force | Remove-Item -Force
$exe = Join-Path $clinkDir 'clink_x64.exe'
$launch = $exe
if ($exe -notmatch '^[\x00-\x7F]*$') {
  $short = (New-Object -ComObject Scripting.FileSystemObject).GetFolder($data).ShortPath
  $launch = Join-Path $short 'runtime\clink-1.9.34\clink_x64.exe'
}
& $launch --profile $profileDir set clink.path $gen | Out-Null
& $launch --profile $profileDir set clink.logo none | Out-Null
& $launch --profile $profileDir set clink.autoupdate off | Out-Null
& $launch --profile $profileDir set prompt.transient off | Out-Null
# S4, S5: Clink's own program loads the scripts too; what the probe printed there.
"clink set said:"
& $launch --profile $profileDir set clink.logo 2>&1 | Select-String '^p3b '
Set-ItemProperty $key AutoRun "(if exist `"$launch`" `"$launch`" inject --autorun --quiet --profile `"$profileDir`")"
"AutoRun: $((Get-ItemProperty $key).AutoRun)"
"data: $data"
"health: $health"
```

- [ ] **Step 4: Write `spikes/vm/guest/recovery-report.ps1`**

```powershell
# Runs inside a test VM after the cmd windows of a step were opened on the recovery-setup layout.
# Prints every probe line, from clink.log and from <data>\probe.txt (one of the two may be
# missing: probe.txt needs io.open on the data folder's path, which is spike S2), then what is
# left in health\. Changes nothing.
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$data = Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike3b'
$log = Join-Path $data 'clink-profile\clink.log'
'--- clink.log'
if (Test-Path -LiteralPath $log) {
  Get-Content -LiteralPath $log -Encoding UTF8 | Select-String '\sp3b ' | ForEach-Object { $_.Line -replace '^.*?\s(?=p3b )', '' }
}
'--- probe.txt'
$probe = Join-Path $data 'probe.txt'
if (Test-Path -LiteralPath $probe) { Get-Content -LiteralPath $probe -Encoding UTF8 } else { '(none)' }
'--- health\'
Get-ChildItem -LiteralPath (Join-Path $data 'health') -Force | ForEach-Object { "$($_.Name): $([IO.File]::ReadAllText($_.FullName).Trim())" }
'--- cmd.exe processes now'
Get-CimInstance Win32_Process -Filter "Name='cmd.exe'" | ForEach-Object { "pid=$($_.ProcessId) parent=$($_.ParentProcessId) $($_.CommandLine)" }
```

- [ ] **Step 5: Write `spikes/vm/guest/recovery-cmdc.ps1`**

```powershell
# Runs inside a test VM (phase 3b-1 spike S7): three `cmd /c` runs with AutoRun on, as io.popen
# and other programs start cmd. Prints their process ids: recovery-report.ps1 must show no probe
# line from any of them (Clink's AutoRun mode does not inject into cmd /c).
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script starts cmd with the user's AutoRun" }
foreach ($i in 1..3) {
  $p = Start-Process -FilePath "$env:SystemRoot\System32\cmd.exe" -ArgumentList '/c', 'echo', "p3b-cmd-c-$i" -PassThru -Wait -WindowStyle Hidden
  "cmd /c pid=$($p.Id) exit=$($p.ExitCode)"
}
```

- [ ] **Step 6: Write `spikes/vm/guest/recovery-timing.ps1`**

```powershell
# Runs inside a test VM on the signed-in user's desktop (Invoke-GuestDesktop), spike S8 and plan
# 3b-1 Task 12: starts cmd.exe $Runs times, each time waits for zz-timing.lua's marker of its
# first prompt, and prints the time from the process's start to the marker, then the average.
# Each window is closed after its prompt is up, so it leaves no tag. -Data points at another data
# folder (Task 12: CMD Studio's own); -InstallMarker first writes zz-timing.lua into its
# generated\ folder.
param([int] $Runs = 10, [string] $Data = (Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike3b'), [switch] $InstallMarker)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script starts and closes cmd windows" }
if ($InstallMarker) {
  $text = "local p3b_data = [==[$Data]==]`n" + [IO.File]::ReadAllText('C:\cmdstudio\spikes\recovery\timing.lua')
  [IO.File]::WriteAllText((Join-Path $Data 'generated\zz-timing.lua'), $text, (New-Object Text.UTF8Encoding $false))
}
$times = foreach ($i in 1..$Runs) {
  $p = Start-Process -FilePath "$env:SystemRoot\System32\cmd.exe" -PassThru
  $marker = Join-Path $Data "first-prompt-$($p.Id).txt"
  $deadline = (Get-Date).AddSeconds(20)
  while (-not (Test-Path -LiteralPath $marker) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 50 }
  if (Test-Path -LiteralPath $marker) { [int]((Get-Item -LiteralPath $marker).LastWriteTime - $p.StartTime).TotalMilliseconds }
  else { "no marker for pid $($p.Id)" }
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 2
}
$times
$ms = @($times | Where-Object { $_ -is [int] })
if ($ms.Count) { "average of $($ms.Count): $([int](($ms | Measure-Object -Average).Average)) ms" }
```

- [ ] **Step 7: Write `spikes/vm/guest/recovery-crash.ps1`**

```powershell
# Runs inside a test VM (spike S9-S11, plan 3b-1 Task 12). -Swap puts the crash stand-in in the
# place of Clink's DLL in the runtime copy of $Data (never the dev PC's, never the installer's)
# and empties Clink's DLL cache, so the next cmd window crashes inside a module named
# clink_dll_x64.dll; -Restore puts the real DLL back. -Events saves the newest $Last
# "Application Error" events 1000 as XML into C:\cmdstudio\out, prints their EventData fields,
# and runs the app's XPath query (crates/core/src/eventlog.rs, crash_query, seven days). Close
# every cmd window before -Swap and -Restore.
param([switch] $Swap, [switch] $Restore, [switch] $Events, [int] $Last = 3,
  [string] $Data = (Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike3b'))
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script replaces Clink's DLL with one that crashes cmd" }
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$dll = Join-Path $Data 'runtime\clink-1.9.34\clink_dll_x64.dll'
$cache = Join-Path $env:TEMP 'clink\dll_cache'
if ($Swap) {
  Copy-Item 'C:\cmdstudio\crash-dll\clink_dll_x64.dll' $dll -Force
  Remove-Item -LiteralPath $cache -Recurse -Force -ErrorAction SilentlyContinue
  "swapped: $((Get-Item $dll).VersionInfo.FileVersion) $((Get-FileHash $dll).Hash)"
}
if ($Restore) {
  Copy-Item 'C:\cmdstudio\resources\clink\clink_dll_x64.dll' $dll -Force
  Remove-Item -LiteralPath $cache -Recurse -Force -ErrorAction SilentlyContinue
  "restored: $((Get-FileHash $dll).Hash)"
}
if ($Events) {
  $out = 'C:\cmdstudio\out'
  New-Item -ItemType Directory -Force $out | Out-Null
  $found = @(Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'Application Error'; Id = 1000 } -MaxEvents $Last -ErrorAction SilentlyContinue)
  "events 1000: $($found.Count)"
  $i = 0
  foreach ($e in $found) {
    $i++
    $xml = $e.ToXml()
    [IO.File]::WriteAllText((Join-Path $out "event1000-$i.xml"), $xml, (New-Object Text.UTF8Encoding $false))
    "--- event $i at $($e.TimeCreated.ToUniversalTime().ToString('o'))"
    ([xml]$xml).Event.EventData.Data | ForEach-Object { "  [$($_.Name)] = [$($_.'#text')]" }
  }
  $query = "*[System[Provider[@Name='Application Error'] and (EventID=1000) and TimeCreated[timediff(@SystemTime) <= 604800000]]]"
  $hits = @(Get-WinEvent -LogName Application -FilterXPath $query -ErrorAction SilentlyContinue)
  "the app's query finds: $($hits.Count)"
  '--- clink.log, last lines'
  $log = Join-Path $Data 'clink-profile\clink.log'
  if (Test-Path -LiteralPath $log) { Get-Content -LiteralPath $log -Encoding UTF8 -Tail 15 }
}
```

- [ ] **Step 8: List the new scripts in `spikes/vm/guest/guard.test.ps1`**

In `$changers`, after `'add-thai-user.ps1' = @(); 'phase1b-check.ps1' = @('-Step', 'Snapshot')`, add the four scripts that change settings or start cmd, so the guard test (run on a VM only, see Step 9) covers them:

```powershell
  'recovery-setup.ps1' = @(); 'recovery-crash.ps1' = @('-Swap'); 'recovery-timing.ps1' = @(); 'recovery-cmdc.ps1' = @()
```

(`recovery-report.ps1` only reads, like `proto-report.ps1`, and is not listed.)

- [ ] **Step 9: Check every script by parsing it only (this PC)**

No script of this task runs on this PC, `guard.test.ps1` included (it starts the guest scripts in a child PowerShell). Parse each, and check that each guarded one starts with the guard, as `guard.test.ps1` would:

```powershell
$guest = 'spikes\vm\guest'
foreach ($s in 'recovery-setup.ps1', 'recovery-report.ps1', 'recovery-cmdc.ps1', 'recovery-timing.ps1', 'recovery-crash.ps1', 'guard.test.ps1') {
  $t = $e = $null
  $ast = [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path "$guest\$s"), [ref]$t, [ref]$e)
  $first = ($ast.EndBlock.Statements | Select-Object -First 3 | ForEach-Object { $_.Extent.Text }) -join "`n"
  $guarded = $first -match 'COMPUTERNAME -notlike ''CMDSTUDIO-\*''\) \{ throw'
  "$s errors=$($e.Count) guarded=$guarded"
}
```

Expected: `errors=0` for all six; `guarded=True` for `recovery-setup`, `recovery-cmdc`, `recovery-timing`, `recovery-crash`; `False` for `recovery-report` and `guard.test` (neither changes anything). The Lua files are checked on the VMs by Clink itself (Step 11's first lines).

- [ ] **Step 10: Commit the scripts**

Stage `spikes/recovery/probe.lua spikes/recovery/counter.lua spikes/recovery/omp-init.lua spikes/recovery/timing.lua spikes/recovery/crash-dll/Cargo.toml spikes/recovery/crash-dll/src/lib.rs spikes/recovery/crash-dll/version.rc spikes/vm/guest/recovery-setup.ps1 spikes/vm/guest/recovery-report.ps1 spikes/vm/guest/recovery-cmdc.ps1 spikes/vm/guest/recovery-timing.ps1 spikes/vm/guest/recovery-crash.ps1 spikes/vm/guest/guard.test.ps1`. Message: `spikes: the phase 3b-1 probes of Clink, the start counter and Windows' crash reports (recovery spec §0)`.

- [ ] **Step 11: Run the probes on VM-A, then VM-C (the controller)**

On the host, dot-source the helpers and copy what the VM needs. VM-A: `$vm = 'cmdstudio-win11'; $user = 'tester'`. VM-C: `$vm = 'cmdstudio-th'; $user = -join ([char[]](0x0E1C,0x0E39,0x0E49,0x0E17,0x0E14,0x0E2A,0x0E2D,0x0E1A,0x20,0x0E44,0x0E17,0x0E22))` (the Thai user name as code points, as `docs/spikes/phase3-vm-check/protocol.md` writes it, so that no shell has to carry Thai letters), and before every `Send-Run` press Win+Space once (`& $Vbm controlvm $vm keyboardputscancode e0 5b 39 b9 e0 db`) so that the keyboard is ENG.

```powershell
. "$env:USERPROFILE\cmd-studio\spikes\vm\vm.ps1"
$repo = "$env:USERPROFILE\cmd-studio"
& $Vbm snapshot $vm restore clean; & $Vbm startvm $vm --type headless; Wait-GuestReady $vm $user
Copy-ToGuest $vm @("$repo\spikes\vm", "$repo\spikes\recovery") 'C:\cmdstudio\spikes\' $user
Copy-ToGuest $vm @("$repo\resources\clink", "$repo\resources\omp") 'C:\cmdstudio\resources\' $user
Copy-ToGuest $vm @("$repo\crates\core\tests\golden\terracotta.box.omp.json", "$repo\crates\core\tests\golden\terracotta.banner.en.lua") 'C:\cmdstudio\golden\' $user
Copy-ToGuest $vm @("<scratchpad>\crash-dll\clink_dll_x64.dll") 'C:\cmdstudio\crash-dll\' $user
$g = 'C:\cmdstudio\spikes\vm\guest'
```

1. **Layout, `clink set` (S4, S5):** `Invoke-GuestDesktop $vm "$g\recovery-setup.ps1" '' $user`. The output's `clink set said:` part holds `p3b <pid> load host=...\clink_x64.exe ...` and the `api` line. A Lua error there (`…lua:<n>: …`) is a script to fix before going on.
2. **Two cmd windows (S1, S2, S3, S6):** `Send-Run $vm 'cmd'`, wait 10 s, `Save-Shot $vm 'p3b-1'`, type `exit` and Enter (`& $Vbm controlvm $vm keyboardputstring "exit"` then `keyboardputscancode 1c 9c`); once more for a second window.
3. **`cmd /c` (S7):** `Invoke-GuestDesktop $vm "$g\recovery-cmdc.ps1" '' $user`.
4. **Report:** `Invoke-GuestDesktop $vm "$g\recovery-report.ps1" '' $user | Set-Content -Encoding utf8 "$repo\docs\spikes\phase3b-recovery\probe-vm-a.txt"` (`probe-vm-c.txt` on VM-C).
5. **Cost (S8):** `Invoke-GuestDesktop $vm "$g\recovery-setup.ps1" '-NoProbe' $user`, then `Invoke-GuestDesktop $vm "$g\recovery-timing.ps1" '' $user` (without the counter); `Invoke-GuestDesktop $vm "$g\recovery-setup.ps1" '-NoProbe -Counter' $user`, then the timing again (with it); then `Invoke-GuestDesktop $vm "$g\recovery-report.ps1" '' $user` for the `p3b counter ms=` lines. Save the two timing outputs and the counter lines in `timing-vm-a.txt` (`timing-vm-c.txt`).
6. **Crash (S9, S10, S11):** close every cmd window (`Invoke-GuestPs $vm 'Get-Process cmd -ErrorAction SilentlyContinue | Stop-Process -Force' $user`), `Invoke-GuestDesktop $vm "$g\recovery-crash.ps1" '-Swap' $user`; three times `Send-Run $vm 'cmd'` with 15 s between (`Save-Shot $vm 'p3b-crash'` after the first: the window or tab closes, Windows Terminal says the process exited with `0xc000001d`); `Invoke-GuestDesktop $vm "$g\recovery-crash.ps1" '-Events' $user | Set-Content -Encoding utf8 "$repo\docs\spikes\phase3b-recovery\crash-vm-a.txt"` (`crash-vm-c.txt`); copy the newest event's XML out: `(Invoke-GuestPs $vm "Get-Content -Raw -Encoding UTF8 C:\cmdstudio\out\event1000-1.xml" $user) -join "`n" | Set-Content -Encoding utf8 -NoNewline "$repo\docs\spikes\phase3b-recovery\event1000-en.xml"` (`event1000-th.xml` on VM-C); then `-Restore`.
7. **Clean up:** `Invoke-GuestDesktop $vm "$g\recovery-setup.ps1" '-Remove' $user`, power off, restore `clean`.

In the two XML files, replace the `UserID='S-1-5-21-…'` SID with `S-1-5-21-1-2-3-1001` and nothing else; a VM's account name (`tester`, `3607~1`, `ผู้ทดสอบ ไทย`) may stay. Each file is one `<Event …>…</Event>` element, as `EvtRender` writes it.

- [ ] **Step 12: Write `docs/spikes/phase3b-recovery/results.md`**

```markdown
# Phase 3b-1 spike — Clink, the start counter and Windows' crash reports (recovery spec §0)

- **Date:** <date>
- **Where:** VM-A (`cmdstudio-win11`, Windows 11 26H2 en-US, user `tester`) and VM-C (`cmdstudio-th`, Thai, user `ผู้ทดสอบ ไทย`, a Thai data folder path), each from its `clean` snapshot. Clink 1.9.34 and Oh My Posh 31.5.0 from the repo's `resources\`; scripts `spikes/vm/guest/recovery-*.ps1` and `spikes/recovery/`.
- **Raw output** beside this file: `probe-vm-a.txt`, `probe-vm-c.txt`, `timing-vm-a.txt`, `timing-vm-c.txt`, `crash-vm-a.txt`, `crash-vm-c.txt`, `event1000-en.xml`, `event1000-th.xml` (the SID replaced).

| # | What | Plan 3b-1 assumes | VM-A | VM-C | Holds? | Fallback taken |
|---|---|---|---|---|---|---|
| S1 | `os.getpid()` in a cmd window | a number, cmd.exe's pid | | | | |
| S2 | `io.open` write/read and `os.remove` in `health\` | work on the (Thai) path | | | | |
| S3 | `os.globfiles(<health>\start-*)` | the matching files; names alone or whole paths | | | | |
| S4 | `os.gethost()` | `…\cmd.exe` in cmd, Clink's own exe in `clink set` | | | | |
| S5 | `clink set` loads the scripts | yes | | | | |
| S6 | order of oninject, onbeginedit, filter 0, Oh My Posh's (1), filter 2 | onbeginedit before the filters; filter 2 sees Oh My Posh's prompt (`esc=true`) | | | | |
| S7 | `cmd /c` with AutoRun | no probe line from those pids | | | | |
| S8 | cost per start: without / with the counter, and `p3b counter ms=` | ≤ 5 ms more | | | | |
| S9 | event 1000: EventData names and values | `AppName` = `cmd.exe`, `ModuleName` = `clink_dll_x64.dll`, same names in Thai | | | | |
| S10 | `crash_query`'s XPath through `Get-WinEvent -FilterXPath` | accepted, finds the three crashes | | | | |
| S11 | Clink injects the stand-in and cmd crashes in it | yes | | | | |

## Notes

<one paragraph per row that does not hold, quoting the raw line, and what the fallback changes>
```

Fill every cell from the raw files, quoting the line that shows it; "Holds?" is ✅ or ❌, and a ❌ names its fallback (F1–F10). Commit: stage `docs/spikes/phase3b-recovery/`. Message: `spikes: phase 3b-1 results on VM-A and VM-C`. Task 2 starts once this commit is in.

---


### Task 2: The core's `recovery` module and UTC text

Recovery spec §3.1, §4.1, §7: the folder `<data>\health\`, `paused.json` read and written, the pause lifted, the numbers in one place; and the UTC text that D12 writes and the event reader parses (`TimeCreated SystemTime`). The app's `logs::iso_utc` becomes this one in Task 7.

**Files:**
- Create: `crates/core/src/recovery.rs`, `crates/core/src/utc.rs`
- Modify: `crates/core/src/lib.rs`
- Generate: `app/ui/src/bindings/PauseReason.ts`, `PausedBy.ts`
- Test: the two modules' own tests (8 + 3)

**Interfaces:**
- Consumes: `adapters::{AdapterError, Fs}`, `model::Target`, `plan::Paths`.
- Produces:
  - `recovery::{STALE_AFTER_SECS: u64 = 60, PAUSE_AFTER: u32 = 3, CRASH_WINDOW_MS: u64 = 604_800_000, PAUSED_VERSION: u32 = 1, TAG_PREFIX: &str = "start-"}`;
  - `Paths::health() -> PathBuf` (`<data>\health`), `Paths::paused_file() -> PathBuf`;
  - `enum PauseReason { CmdStarts, ClinkCrash, Recovery, User }` (serde camelCase, ts export) with `fn as_str(self) -> &'static str`; `enum PausedBy { Clink, App }` (camelCase, ts export);
  - `struct Resume { theme_id: uuid::Uuid, target: Target }`, `struct PauseRecord { version: u32, reason: PauseReason, at: u64, count: u32, by: PausedBy, resume: Option<Resume> }` (camelCase, `resume` defaults to `None`);
  - `struct Paused { by: PausedBy, reason: Option<PauseReason>, at_ms: Option<u64>, count: u32, resume: Option<Resume> }`, `impl From<PauseRecord> for Paused`;
  - `fn read_pause(fs: &dyn Fs, paths: &Paths) -> Result<Option<Paused>, AdapterError>` (a file that is not a version-1 record is `Paused { by: Clink, reason: None, at_ms: None, count: 0, resume: None }`), `fn write_pause(fs, paths, &PauseRecord) -> Result<(), AdapterError>`, `fn clear_pause(fs, paths) -> Result<(), AdapterError>` (deletes `paused.json` and every tag, makes `health\`), `fn is_tag(&Path) -> bool`;
  - `utc::{iso(ms: u64) -> String, parse(text: &str) -> Option<u64>}`.

- [ ] **Step 1: Write `crates/core/src/utc.rs`** (its tests are in it)

```rust
//! UTC times as text (recovery spec §4.3, §4.5): what Windows' event log writes in an event's
//! `TimeCreated SystemTime`, read into milliseconds since 1970, and the same written for people.
//! Howard Hinnant's civil-from-days and days-from-civil algorithms; nothing reads the clock.

/// `YYYY-MM-DDTHH:MM:SSZ` from milliseconds since 1970.
pub fn iso(ms: u64) -> String {
    let secs = ms / 1000;
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Milliseconds since 1970 from `YYYY-MM-DDTHH:MM:SS[.fraction]Z`, the form of an event's
/// `SystemTime` (`2026-10-17T03:15:42.1234567Z`). Digits of the fraction past the third are
/// dropped. `None` for any other text, or a time before 1970.
pub fn parse(text: &str) -> Option<u64> {
    let text = text.trim().strip_suffix('Z')?;
    let (date, time) = text.split_once('T')?;
    let mut date = date.splitn(3, '-');
    let year: u64 = number(date.next()?)?;
    let month: u64 = number(date.next()?)?;
    let day: u64 = number(date.next()?)?;
    let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
    let mut clock = clock.splitn(3, ':');
    let hour: u64 = number(clock.next()?)?;
    let minute: u64 = number(clock.next()?)?;
    let second: u64 = number(clock.next()?)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    if second > 60 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let millis: String = fraction
        .chars()
        .chain(std::iter::repeat('0'))
        .take(3)
        .collect();
    let millis: u64 = millis.parse().ok()?;
    let days = days_from_civil(year, month, day)?;
    Some((days * 86_400 + hour * 3600 + minute * 60 + second) * 1000 + millis)
}

/// Digits only: `str::parse` alone would take a leading `+`.
fn number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Days since 1970-01-01 of a date in the proleptic Gregorian calendar; `None` before 1970.
fn days_from_civil(year: u64, month: u64, day: u64) -> Option<u64> {
    let y = if month <= 2 {
        year.checked_sub(1)?
    } else {
        year
    };
    let era = y / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe).checked_sub(719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_the_same_instant() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(1_791_694_709_000), "2026-10-11T04:58:29Z");
        assert_eq!(iso(951_782_400_000), "2000-02-29T00:00:00Z");
        for ms in [0, 951_782_400_000, 1_791_694_709_000, 4_102_444_799_000] {
            assert_eq!(parse(&iso(ms)), Some(ms), "{ms}");
        }
    }

    #[test]
    fn reads_the_event_logs_form_with_its_fraction() {
        assert_eq!(
            parse("2026-10-11T04:58:29.1234567Z"),
            Some(1_791_694_709_123)
        );
        assert_eq!(parse("2026-10-11T04:58:29.5Z"), Some(1_791_694_709_500));
        assert_eq!(
            parse("2026-10-11T04:58:29.000000000Z"),
            Some(1_791_694_709_000)
        );
        assert_eq!(parse(" 2026-10-11T04:58:29Z "), Some(1_791_694_709_000));
    }

    #[test]
    fn anything_else_is_none() {
        for text in [
            "",
            "2026-10-11T04:58:29",
            "2026-10-11 04:58:29Z",
            "2026-13-11T04:58:29Z",
            "2026-10-32T04:58:29Z",
            "2026-10-11T24:00:00Z",
            "2026-10-11T04:60:00Z",
            "2026-10-11T04:58:29.12aZ",
            "+2026-10-11T04:58:29Z",
            "1969-12-31T23:59:59Z",
            "2026-10-11T04:58Z",
        ] {
            assert_eq!(parse(text), None, "{text:?}");
        }
    }
}
```

- [ ] **Step 2: Write `crates/core/src/recovery.rs`** (its tests are in it; `a_pause_file_that_cannot_be_read_as_one_is_still_a_pause_in_cmd` is Review Focus 3)

```rust
//! The theme's pause and the start counter's files (recovery spec §3.1, §4.1, §7). The folder
//! `<data>\health\` is CMD Studio's alone:
//! - `start-<pid>`: init.lua's tag for one cmd window, the text `v1 <unix seconds>`, from the
//!   window's start to its first prompt;
//! - `paused.json`: the pause. init.lua writes it when [`PAUSE_AFTER`] windows in a row never
//!   reached their prompt, and loads no theme while the file is there, whatever it holds; the app
//!   writes it when it pauses the theme on this computer (Revert everything underneath), with the
//!   theme to apply again.
//!
//! The numbers live here once: the generator writes them into init.lua
//! (`generate::lua::init_lua`).
use crate::adapters::{AdapterError, Fs};
use crate::model::Target;
use crate::plan::Paths;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A tag older than this many seconds is a window that never reached its first prompt.
pub const STALE_AFTER_SECS: u64 = 60;
/// How many windows in a row that never reached their prompt pause the theme in cmd; also how
/// many crashes of cmd in Clink's DLL pause it on this computer (recovery spec §4.3) and make
/// doctor D12 fail.
pub const PAUSE_AFTER: u32 = 3;
/// How far back the app looks for cmd's crashes in Clink: seven days (recovery spec §4.3).
pub const CRASH_WINDOW_MS: u64 = 7 * 24 * 60 * 60 * 1000;
/// The version of `paused.json` this build writes and reads.
pub const PAUSED_VERSION: u32 = 1;
/// A start tag's name: this, then the window's process id.
pub const TAG_PREFIX: &str = "start-";

impl Paths {
    /// `<data>\health`: the start counter's tags and the pause (recovery spec §3.1).
    pub fn health(&self) -> PathBuf {
        self.data.join("health")
    }
    /// `<data>\health\paused.json`.
    pub fn paused_file(&self) -> PathBuf {
        self.health().join("paused.json")
    }
}

/// Why the theme is paused (recovery spec §3.1), as `paused.json` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PauseReason {
    /// init.lua: windows in a row that never reached their prompt.
    CmdStarts,
    /// The app: cmd crashed in Clink's DLL, as Windows' crash reports say.
    ClinkCrash,
    /// The app, started from the CMD Studio Recovery shortcut (phase 3b-2).
    Recovery,
    /// The user pressed "Pause the theme".
    User,
}

impl PauseReason {
    /// The name `paused.json` and the log use.
    pub fn as_str(self) -> &'static str {
        match self {
            PauseReason::CmdStarts => "cmdStarts",
            PauseReason::ClinkCrash => "clinkCrash",
            PauseReason::Recovery => "recovery",
            PauseReason::User => "user",
        }
    }
}

/// Who paused the theme, which is how far (recovery spec §4.1): init.lua in cmd, nothing on the
/// computer changed; or the app, on this computer, through Revert everything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PausedBy {
    Clink,
    App,
}

/// The theme a pause of this computer applies again when the theme is turned back on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resume {
    pub theme_id: uuid::Uuid,
    pub target: Target,
}

/// `paused.json` (recovery spec §3.1): `{"version": 1, "reason": …, "at": <unix ms>,
/// "count": …, "by": "clink" | "app", "resume": null | {"themeId": …, "target": …}}`. init.lua
/// writes the first five fields and no `resume`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PauseRecord {
    pub version: u32,
    pub reason: PauseReason,
    /// When, in milliseconds since 1970.
    pub at: u64,
    /// The windows in a row, or the crashes, that paused it; 0 for the others.
    pub count: u32,
    pub by: PausedBy,
    #[serde(default)]
    pub resume: Option<Resume>,
}

/// The pause as CMD Studio reads `paused.json`. A file that can't be read as a [`PauseRecord`]
/// of this version (damaged, half written, from a newer CMD Studio) is a pause all the same,
/// at the cmd level: init.lua loads no theme while the file is there, and turning the theme back
/// on deletes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paused {
    pub by: PausedBy,
    /// `None` for a file that can't be read as a record.
    pub reason: Option<PauseReason>,
    pub at_ms: Option<u64>,
    pub count: u32,
    pub resume: Option<Resume>,
}

impl From<PauseRecord> for Paused {
    fn from(r: PauseRecord) -> Self {
        Paused {
            by: r.by,
            reason: Some(r.reason),
            at_ms: Some(r.at),
            count: r.count,
            resume: r.resume,
        }
    }
}

/// The pause, `None` when `paused.json` is not there. A read that fails is an error, never "not
/// paused".
pub fn read_pause(fs: &dyn Fs, paths: &Paths) -> Result<Option<Paused>, AdapterError> {
    let Some(bytes) = fs.read(&paths.paused_file())? else {
        return Ok(None);
    };
    let record = serde_json::from_slice::<PauseRecord>(&bytes)
        .ok()
        .filter(|r| r.version == PAUSED_VERSION);
    Ok(Some(record.map_or(
        Paused {
            by: PausedBy::Clink,
            reason: None,
            at_ms: None,
            count: 0,
            resume: None,
        },
        Paused::from,
    )))
}

/// Writes `paused.json` in one step (`Fs::write_atomic`, which makes `health\` when it is
/// missing).
pub fn write_pause(fs: &dyn Fs, paths: &Paths, record: &PauseRecord) -> Result<(), AdapterError> {
    let file = paths.paused_file();
    let mut bytes = serde_json::to_vec_pretty(record)
        .map_err(|e| AdapterError::new(format!("write {}", file.display()), e.to_string()))?;
    bytes.push(b'\n');
    fs.write_atomic(&file, &bytes)
}

/// Lifts the pause: deletes `paused.json`, then every start tag (a window that comes after counts
/// from nothing), and makes sure `health\` is there for init.lua to write its tags in. Files that
/// are gone already are fine; nothing else in the folder is touched.
pub fn clear_pause(fs: &dyn Fs, paths: &Paths) -> Result<(), AdapterError> {
    fs.remove_file(&paths.paused_file())?;
    for entry in fs.list_dir(&paths.health())? {
        if !entry.is_dir && is_tag(&entry.path) {
            fs.remove_file(&entry.path)?;
        }
    }
    fs.create_dir_all(&paths.health())
}

/// Whether `path` is named like a start tag: `start-` and the digits of a process id.
pub fn is_tag(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix(TAG_PREFIX))
        .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeFs;

    fn paths() -> Paths {
        Paths {
            data: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio"),
        }
    }

    /// What init.lua writes when it pauses the theme (`generate::lua`, HEALTH_BODY).
    const FROM_LUA: &[u8] =
        b"{\"version\":1,\"reason\":\"cmdStarts\",\"at\":1791694709000,\"count\":3,\"by\":\"clink\"}\n";

    #[test]
    fn the_folder_and_its_files_are_under_the_data_folder() {
        let p = paths();
        assert_eq!(p.health(), p.data.join("health"));
        assert_eq!(p.paused_file(), p.data.join(r"health\paused.json"));
        assert_eq!((STALE_AFTER_SECS, PAUSE_AFTER), (60, 3));
        assert_eq!(CRASH_WINDOW_MS, 604_800_000);
    }

    #[test]
    fn a_pause_init_lua_wrote_reads_as_one_in_cmd() {
        let (fs, p) = (FakeFs::new(), paths());
        fs.put(p.paused_file(), FROM_LUA);
        assert_eq!(
            read_pause(&fs, &p).unwrap(),
            Some(Paused {
                by: PausedBy::Clink,
                reason: Some(PauseReason::CmdStarts),
                at_ms: Some(1_791_694_709_000),
                count: 3,
                resume: None,
            })
        );
    }

    #[test]
    fn a_pause_the_app_wrote_round_trips_with_its_theme() {
        let (fs, p) = (FakeFs::new(), paths());
        let record = PauseRecord {
            version: PAUSED_VERSION,
            reason: PauseReason::ClinkCrash,
            at: 1_791_694_709_000,
            count: 4,
            by: PausedBy::App,
            resume: Some(Resume {
                theme_id: uuid::Uuid::nil(),
                target: Target::Wt,
            }),
        };
        write_pause(&fs, &p, &record).unwrap();
        let text = String::from_utf8(fs.file(p.paused_file()).unwrap()).unwrap();
        // camelCase, the names of recovery spec §3.1
        for name in [
            "\"version\": 1",
            "\"reason\": \"clinkCrash\"",
            "\"by\": \"app\"",
            "\"themeId\"",
            "\"target\": \"wt\"",
        ] {
            assert!(text.contains(name), "{name} in {text}");
        }
        assert_eq!(read_pause(&fs, &p).unwrap(), Some(Paused::from(record)));
    }

    #[test]
    fn a_pause_file_that_cannot_be_read_as_one_is_still_a_pause_in_cmd() {
        // init.lua only asks whether the file is there (Review Focus 3): a damaged one, a half
        // written one, one from a newer CMD Studio all keep the theme out of cmd, so the app must
        // say "paused" too, and turning the theme back on deletes the file.
        let in_cmd = Paused {
            by: PausedBy::Clink,
            reason: None,
            at_ms: None,
            count: 0,
            resume: None,
        };
        for text in [
            &b""[..],
            b"{ not json",
            b"{\"version\":1,\"reason\":\"cmdStarts\",\"at\":17916",
            b"{}",
            b"{\"version\":2,\"reason\":\"user\",\"at\":1,\"count\":0,\"by\":\"app\"}",
            b"{\"version\":1,\"reason\":\"later\",\"at\":1,\"count\":0,\"by\":\"clink\"}",
        ] {
            let (fs, p) = (FakeFs::new(), paths());
            fs.put(p.paused_file(), text);
            assert_eq!(
                read_pause(&fs, &p).unwrap(),
                Some(in_cmd.clone()),
                "{:?}",
                String::from_utf8_lossy(text)
            );
        }
    }

    #[test]
    fn no_file_is_no_pause_and_a_read_that_fails_is_an_error() {
        let (fs, p) = (FakeFs::new(), paths());
        assert_eq!(read_pause(&fs, &p).unwrap(), None);
        fs.put(p.paused_file(), FROM_LUA);
        fs.fail_reads_of(p.paused_file());
        assert!(read_pause(&fs, &p).is_err());
    }

    #[test]
    fn lifting_a_pause_deletes_the_file_and_every_tag_and_nothing_else() {
        let (fs, p) = (FakeFs::new(), paths());
        fs.put(p.paused_file(), FROM_LUA);
        for name in ["start-1", "start-4242", "start-", "start-x1", "notes.txt"] {
            fs.put(p.health().join(name), b"v1 1791694709\n");
        }
        fs.create_dir_all(&p.health().join("start-7")).unwrap();
        clear_pause(&fs, &p).unwrap();
        let left: Vec<String> = fs
            .list_dir(&p.health())
            .unwrap()
            .iter()
            .map(|e| e.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["notes.txt", "start-", "start-7", "start-x1"]);
        // Nothing to lift is no error, and the folder is made for init.lua.
        let (fs, p) = (FakeFs::new(), paths());
        clear_pause(&fs, &p).unwrap();
        assert!(fs.is_dir(&p.health()));
    }

    #[test]
    fn a_tag_is_start_and_the_digits_of_a_process_id() {
        for (name, tag) in [
            ("start-4242", true),
            ("start-1", true),
            ("start-", false),
            ("start-12a", false),
            ("Start-1", false),
            ("paused.json", false),
        ] {
            assert_eq!(is_tag(Path::new(name)), tag, "{name}");
        }
    }

    #[test]
    fn the_reasons_are_named_as_paused_json_names_them() {
        for reason in [
            PauseReason::CmdStarts,
            PauseReason::ClinkCrash,
            PauseReason::Recovery,
            PauseReason::User,
        ] {
            assert_eq!(
                serde_json::to_value(reason).unwrap(),
                serde_json::Value::from(reason.as_str())
            );
        }
    }
}
```

- [ ] **Step 3: Register the modules** in `crates/core/src/lib.rs`, in name order: after `pub mod preview;` add `pub mod recovery;`, and after `pub mod txn;` add `pub mod utc;`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cmdstudio-core --lib -- recovery:: utc::`
Expected: `11 passed` (8 in `recovery`, 3 in `utc`). Then the four clippy commands and `cargo fmt --all -- --check`: clean.

- [ ] **Step 5: Export the two new bindings**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\export-bindings.ps1 -NoTauri`, then `git status --porcelain -- app/ui/src/bindings`.
Expected: exactly two new files, `?? app/ui/src/bindings/PauseReason.ts` and `?? app/ui/src/bindings/PausedBy.ts`, holding

```ts
// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.

/**
 * Why the theme is paused (recovery spec §3.1), as `paused.json` names it.
 */
export type PauseReason = "cmdStarts" | "clinkCrash" | "recovery" | "user";
```

```ts
// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.

/**
 * Who paused the theme, which is how far (recovery spec §4.1): init.lua in cmd, nothing on the
 * computer changed; or the app, on this computer, through Revert everything.
 */
export type PausedBy = "clink" | "app";
```

Any other changed file under `bindings` means the core's exports drifted: stop and report it.

- [ ] **Step 6: Commit**

Stage `crates/core/src/recovery.rs crates/core/src/utc.rs crates/core/src/lib.rs app/ui/src/bindings/PauseReason.ts app/ui/src/bindings/PausedBy.ts`. Message: `core: the theme's pause and the start counter's files (recovery spec §3.1, §7), and UTC text`.

---

### Task 3: The start counter in `init.lua`, and the box that waits for no pause

Recovery spec §3.2–§3.3. `init_lua` gains the health folder and writes the counter (`HEALTH_BODY`) in front of the load of Oh My Posh; `banner.lua`'s inject callback prints nothing while `CMDSTUDIO_PAUSED` is set. Validated in Lua 5.2 (mlua 0.10.5), which also showed that `%d` cannot write a time in milliseconds there (`bad argument #2 to 'format' (not a number in proper range)`): the counter writes it with `%.0f`.

Spike results this task depends on: S1 (F1), S2 (F2), S3 (F3), S4 (F4), S6 (F5), S7 (F6), S8 (F7).

**Files:**
- Modify: `crates/core/src/generate/lua.rs`, `crates/core/src/plan.rs:403`
- Create: `crates/core/tests/health_lua.rs`
- Modify: `crates/core/tests/banner_lua.rs`, `crates/core/tests/golden.rs`, `crates/core/tests/english_in_cmd.rs`
- Regenerate: `crates/core/tests/golden/init.lua`, `crates/core/tests/golden/terracotta.banner.en.lua`

**Interfaces:**
- Consumes: Task 2's `recovery::{STALE_AFTER_SECS, PAUSE_AFTER, PauseRecord, PauseReason, PausedBy, PAUSED_VERSION}`, `Paths::health()`.
- Produces: `generate::lua::init_lua(omp_exe: &Path, omp_config: &Path, health: &Path) -> String`; `pub const PAUSED_NOW: &str` (with one `%d`) and `pub const PAUSED_STILL: &str`; in Lua, the global `CMDSTUDIO_PAUSED` (init.lua sets it, banner.lua reads it), the variable `CMDSTUDIO_PAUSE_SAID` (the paused line is said once for a window and the windows started from it), a prompt filter at priority `(_ZL_CLINK_PROMPT_PRIORITY or 0) + 2`, and `paused.json` written as `{"version":1,"reason":"cmdStarts","at":<ms>,"count":<n>,"by":"clink"}` and a newline.

- [ ] **Step 1: Write the failing tests** — `crates/core/tests/health_lua.rs` (17 tests: recovery spec §10's nine cases, criterion 1 end to end, and Review Focus 1, 2 and 3)

```rust
//! init.lua's start counter and the pause it reads (recovery spec §3, §10), run for real in Lua
//! 5.2 with stand-ins for Clink. Each window is a Lua state of its own, as Clink makes one for
//! each cmd window, over one disk of files kept here between windows. The files are in memory,
//! not in a temp folder: Lua's own `io.open` takes the ANSI code page on Windows, unlike Clink's,
//! so a real folder would test mlua and not Clink; Thai paths are the spike's (plan 3b-1 Task 1,
//! S2). banner.lua loads first and init.lua second, as Clink loads a folder in name order; then
//! the inject callbacks run, then the prompt filters, the lowest priority first.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use cmdstudio_core::generate::lua::{banner_lua, init_lua, PAUSED_NOW, PAUSED_STILL};
use cmdstudio_core::model::Theme;
use cmdstudio_core::presets;
use cmdstudio_core::recovery::{PauseReason, PauseRecord, PausedBy, PAUSED_VERSION};
use mlua::{Function, Lua, Table, Value};
use std::collections::BTreeMap;
use std::path::Path;

mod common;
use common::PRELUDE;

const HEALTH: &str = r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\health";
const OMP_EXE: &str =
    r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\runtime\omp-31.5.0\oh-my-posh.exe";
const OMP_CONFIG: &str =
    r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\generated\theme.omp.json";
/// `os.gethost()` in a cmd window, and in CMD Studio's own `clink set`.
const CMD: &str = r"C:\Windows\System32\cmd.exe";
const CLINK: &str =
    r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\runtime\clink-1.9.34\clink_x64.exe";
/// Friday 9 October 2026, 16:47 UTC, in seconds.
const T: u64 = 1_791_694_709;

/// Stand-ins for what Clink gives the scripts beyond `common::PRELUDE`: the files of `__files`
/// (a path to its text; `__fail[path]` or `__fail_all` makes a file fail), `os.globfiles` over
/// them (names only, or whole paths with `__glob_paths`, the two shapes Clink may give),
/// `os.getpid`, `os.gethost`, `os.time`, several inject callbacks, and the prompt filters with
/// `__prompt()` drawing the first prompt as Clink does.
const STUBS: &str = r#"
__fail = {}
__fail_all = false
__glob_paths = false
__injects = {}
__filters = {}
__popen_cmds = {}
__logs = {}
log = { info = function(s) __logs[#__logs + 1] = s end }
print = function(...) __out[#__out + 1] = "<print>" .. table.concat({...}, " ") end
clink.oninject = function(f) __injects[#__injects + 1] = f end
clink.promptfilter = function(priority)
  local f = { priority = priority }
  __filters[#__filters + 1] = f
  return f
end
os.time = function() return __now end
os.getpid = function() return __pid end
os.gethost = function() return __host end
io.open = function(path, mode)
  if __fail_all or __fail[path] then return nil, path .. ": Permission denied" end
  if mode:sub(1, 1) == "r" then
    local text = __files[path]
    if text == nil then return nil, path .. ": No such file or directory" end
    return { read = function() return text end, close = function() end }
  end
  local parts = {}
  return {
    write = function(self, ...)
      for _, s in ipairs({...}) do parts[#parts + 1] = tostring(s) end
      return self
    end,
    close = function() __files[path] = table.concat(parts) end,
  }
end
os.remove = function(path)
  if __fail_all or __fail[path] then return nil, path .. ": Permission denied" end
  if __files[path] == nil then return nil, path .. ": No such file or directory" end
  __files[path] = nil
  return true
end
os.globfiles = function(pattern)
  if __fail_all then error("globfiles: access denied") end
  local dir, prefix = pattern:match("^(.*)\\([^\\]*)%*$")
  local found = {}
  for path in pairs(__files) do
    local d, name = path:match("^(.*)\\([^\\]*)$")
    if d == dir and name:sub(1, #prefix) == prefix then
      found[#found + 1] = __glob_paths and path or name
    end
  end
  table.sort(found)
  return found
end
io.popen = function(cmd)
  __popen_cmds[#__popen_cmds + 1] = cmd
  if __popen_result == nil then return nil end
  return { read = function() return __popen_result end, close = function() end }
end
function __inject()
  for _, f in ipairs(__injects) do f() end
end
function __prompt()
  table.sort(__filters, function(a, b) return a.priority < b.priority end)
  local prompt = "C:\\>"
  for _, f in ipairs(__filters) do
    if f.filter then
      local text, go_on = f:filter(prompt)
      if text ~= nil then prompt = text end
      if go_on == false then break end
    end
  end
  return prompt
end
"#;

/// What `oh-my-posh init cmd` prints, cut down: a prompt filter at priority 1, as Oh My Posh
/// 31.5.0 registers its own, that notes whether the window's tag is still there when it runs.
const OMP: &str = r#"
local p = clink.promptfilter(1)
function p:filter(prompt)
  __tag_seen_by_omp = __files[__health .. "\\start-" .. __pid] ~= nil
  return "omp> "
end
"#;

/// The files, path to text, kept from one window to the next.
type Disk = BTreeMap<String, String>;

/// How a window is opened.
#[derive(Clone, Copy)]
struct How {
    pid: u32,
    now: u64,
    /// `os.gethost()`.
    host: &'static str,
    /// Clink injects: a cmd window. CMD Studio's own `clink set` never does.
    inject: bool,
    /// The first prompt is drawn.
    prompt: bool,
}

/// A cmd window, process `pid`, started at `now`, closed (or hung) before its first prompt.
fn window(pid: u32, now: u64) -> How {
    How {
        pid,
        now,
        host: CMD,
        inject: true,
        prompt: false,
    }
}

impl How {
    /// The same window, and it reaches its first prompt.
    fn prompted(self) -> How {
        How {
            prompt: true,
            ..self
        }
    }
}

/// What a window did.
struct Window {
    /// What it printed: the welcome box, the line of a paused theme.
    printed: Vec<String>,
    /// It ran Oh My Posh.
    omp: bool,
    /// init.lua set CMDSTUDIO_PAUSED.
    paused: bool,
    /// The first prompt as drawn, `None` when it was not drawn or a filter failed.
    prompt: Option<String>,
    /// Oh My Posh's filter found the window's tag still there.
    tag_seen_by_omp: bool,
    /// The variables it set.
    env: BTreeMap<String, String>,
}

impl Window {
    fn shows_the_box(&self) -> bool {
        self.printed.iter().any(|p| p.contains("Welcome back"))
    }
    /// What it printed besides the box.
    fn lines(&self) -> Vec<String> {
        self.printed
            .iter()
            .filter(|p| !p.contains("Welcome back"))
            .cloned()
            .collect()
    }
}

fn terracotta() -> Theme {
    presets::builtin()
        .unwrap()
        .into_iter()
        .find(|(key, _)| *key == "terracotta")
        .unwrap()
        .1
}

fn tag(pid: u32) -> String {
    format!(r"{HEALTH}\start-{pid}")
}

fn paused_file() -> String {
    format!(r"{HEALTH}\paused.json")
}

/// A disk with a tag for each of `pids`, written `age` seconds before [`T`].
fn tags_of_age(pids: &[u32], age: u64) -> Disk {
    pids.iter()
        .map(|pid| (tag(*pid), format!("v1 {}\n", T - age)))
        .collect()
}

fn truthy(lua: &Lua, name: &str) -> mlua::Result<bool> {
    let v: Value = lua.globals().get(name)?;
    Ok(!matches!(v, Value::Nil | Value::Boolean(false)))
}

/// Opens one window over `disk`, `before` setting the stage after the stand-ins, and writes the
/// files back to `disk` when it is done.
fn open_with(
    disk: &mut Disk,
    how: How,
    before: impl FnOnce(&Lua) -> mlua::Result<()>,
) -> mlua::Result<Window> {
    let lua = Lua::new();
    lua.load(PRELUDE).exec()?;
    let g = lua.globals();
    let files = lua.create_table()?;
    for (path, text) in disk.iter() {
        files.set(path.as_str(), text.as_str())?;
    }
    g.set("__files", files)?;
    lua.load(STUBS).exec()?;
    g.set("__now", how.now)?;
    g.set("__pid", how.pid)?;
    g.set("__host", how.host)?;
    g.set("__health", HEALTH)?;
    g.set("__popen_result", OMP)?;
    before(&lua)?;
    lua.load(banner_lua(&terracotta()))
        .set_name("banner.lua")
        .exec()?;
    lua.load(init_lua(
        Path::new(OMP_EXE),
        Path::new(OMP_CONFIG),
        Path::new(HEALTH),
    ))
    .set_name("init.lua")
    .exec()?;
    if how.inject {
        g.get::<Function>("__inject")?.call::<()>(())?;
    }
    let mut prompt = None;
    if how.prompt {
        lua.load("__prompt_ok, __prompt_text = pcall(__prompt)")
            .exec()?;
        if g.get::<bool>("__prompt_ok")? {
            prompt = Some(g.get::<String>("__prompt_text")?);
        }
    }
    let out: Table = g.get("__out")?;
    let printed = out
        .sequence_values::<String>()
        .collect::<mlua::Result<_>>()?;
    let files: Table = g.get("__files")?;
    disk.clear();
    for pair in files.pairs::<String, String>() {
        let (path, text) = pair?;
        disk.insert(path, text);
    }
    let env: Table = g.get("__env")?;
    Ok(Window {
        printed,
        omp: g.get::<Table>("__popen_cmds")?.raw_len() > 0,
        paused: truthy(&lua, "CMDSTUDIO_PAUSED")?,
        prompt,
        tag_seen_by_omp: truthy(&lua, "__tag_seen_by_omp")?,
        env: env.pairs::<String, String>().collect::<mlua::Result<_>>()?,
    })
}

fn open(disk: &mut Disk, how: How) -> mlua::Result<Window> {
    open_with(disk, how, |_| Ok(()))
}

#[test]
fn a_first_window_leaves_its_tag_and_loads_the_prompt_and_the_box() -> mlua::Result<()> {
    let mut disk = Disk::new();
    let w = open(&mut disk, window(100, T))?;
    assert_eq!(disk, Disk::from([(tag(100), format!("v1 {T}\n"))]));
    assert!(w.omp && !w.paused && w.shows_the_box());
    assert!(w.lines().is_empty(), "{:?}", w.printed);
    Ok(())
}

#[test]
fn the_first_prompt_takes_the_tag_away_after_oh_my_poshs_filter() -> mlua::Result<()> {
    let mut disk = Disk::new();
    let w = open(&mut disk, window(100, T).prompted())?;
    assert_eq!(w.prompt.as_deref(), Some("omp> "));
    // The tag was still there while Oh My Posh drew the prompt: CMD Studio's filter runs after
    // it, so a window whose Oh My Posh hangs at its prompt counts (recovery spec §3.2 step 5).
    assert!(w.tag_seen_by_omp);
    assert!(disk.is_empty(), "{disk:?}");
    Ok(())
}

#[test]
fn two_stale_tags_do_not_pause() -> mlua::Result<()> {
    let mut disk = tags_of_age(&[1, 2], 61);
    let w = open(&mut disk, window(3, T))?;
    assert!(!w.paused && w.omp && w.shows_the_box());
    assert_eq!(disk.len(), 3, "{disk:?}");
    assert!(disk.contains_key(&tag(3)));
    Ok(())
}

#[test]
fn three_windows_that_never_reach_a_prompt_pause_the_fourth() -> mlua::Result<()> {
    // Criterion 1 of recovery spec §1: the fourth window opens with cmd's own prompt, by itself,
    // and says so in one line of English.
    let mut disk = Disk::new();
    for (i, pid) in [1u32, 2, 3].into_iter().enumerate() {
        let w = open(&mut disk, window(pid, T + i as u64))?;
        assert!(!w.paused, "window {pid}");
    }
    let w = open(&mut disk, window(4, T + 70).prompted())?;
    assert!(w.paused && !w.omp && !w.shows_the_box());
    assert_eq!(w.lines(), [PAUSED_NOW.replace("%d", "3")]);
    assert_eq!(w.prompt.as_deref(), Some("C:\\>"), "cmd's own prompt");
    assert_eq!(
        disk,
        Disk::from([(
            paused_file(),
            format!(
                "{{\"version\":1,\"reason\":\"cmdStarts\",\"at\":{},\"count\":3,\"by\":\"clink\"}}\n",
                (T + 70) * 1000
            )
        )])
    );
    // Every window after it: no prompt, no box, the shorter line, and no tag.
    let w = open(&mut disk, window(5, T + 80).prompted())?;
    assert!(w.paused && !w.omp && !w.shows_the_box());
    assert_eq!(w.lines(), [PAUSED_STILL]);
    assert_eq!(disk.len(), 1, "{disk:?}");
    Ok(())
}

#[test]
fn the_pause_init_lua_writes_is_the_record_the_app_reads() -> mlua::Result<()> {
    let mut disk = tags_of_age(&[1, 2, 3, 4], 600);
    open(&mut disk, window(5, T))?;
    let record: PauseRecord = serde_json::from_str(&disk[&paused_file()]).unwrap();
    assert_eq!(
        record,
        PauseRecord {
            version: PAUSED_VERSION,
            reason: PauseReason::CmdStarts,
            at: T * 1000,
            count: 4,
            by: PausedBy::Clink,
            resume: None,
        }
    );
    Ok(())
}

#[test]
fn a_fresh_tag_of_another_window_is_neither_counted_nor_taken_away() -> mlua::Result<()> {
    // Windows Terminal opens several tabs at once (recovery spec §3.2 step 5).
    let mut disk = tags_of_age(&[1, 2], 61);
    disk.insert(tag(9), format!("v1 {}\n", T - 59));
    let w = open(&mut disk, window(3, T).prompted())?;
    assert!(!w.paused && w.omp);
    // A success: the stale ones go with its own, the other window's stays.
    assert_eq!(disk.keys().cloned().collect::<Vec<_>>(), [tag(9)]);
    Ok(())
}

#[test]
fn a_file_that_cannot_be_read_or_written_never_pauses() -> mlua::Result<()> {
    let stale = tags_of_age(&[1, 2, 3], 600);
    // Nothing can be read, written or listed: the counter is skipped, the theme loads.
    let mut disk = stale.clone();
    let w = open_with(&mut disk, window(4, T).prompted(), |lua| {
        lua.load("__fail_all = true").exec()
    })?;
    assert!(!w.paused && w.omp && w.shows_the_box());
    assert_eq!(w.prompt.as_deref(), Some("omp> "));
    assert_eq!(disk, stale);
    // paused.json alone can't be written: no pause, and the window counts as any other.
    let mut disk = stale.clone();
    let w = open_with(&mut disk, window(4, T), |lua| {
        lua.globals()
            .get::<Table>("__fail")?
            .set(paused_file(), true)
    })?;
    assert!(!w.paused && w.omp);
    assert!(disk.contains_key(&tag(4)) && !disk.contains_key(&paused_file()));
    Ok(())
}

#[test]
fn a_paused_theme_loads_no_prompt_and_no_welcome_box() -> mlua::Result<()> {
    let mut disk = Disk::from([(
        paused_file(),
        r#"{"version":1,"reason":"user","at":1,"count":0,"by":"app"}"#.to_string(),
    )]);
    let before = disk.clone();
    let w = open(&mut disk, window(7, T).prompted())?;
    assert!(w.paused && !w.omp && !w.shows_the_box());
    assert_eq!(w.lines(), [PAUSED_STILL]);
    assert_eq!(w.prompt.as_deref(), Some("C:\\>"));
    assert_eq!(disk, before, "no tag while paused");
    Ok(())
}

#[test]
fn a_damaged_pause_file_still_pauses() -> mlua::Result<()> {
    // Review Focus 3: init.lua asks only whether the file is there.
    for text in ["{ not json", "", "{\"version\":9}"] {
        let mut disk = Disk::from([(paused_file(), text.to_string())]);
        let w = open(&mut disk, window(7, T))?;
        assert!(w.paused && !w.omp, "{text:?}");
    }
    Ok(())
}

#[test]
fn the_lines_of_a_paused_window_are_one_line_of_english() -> mlua::Result<()> {
    let mut disk = tags_of_age(&[1, 2, 3, 4], 600);
    let w = open(&mut disk, window(5, T))?;
    assert_eq!(w.lines(), [PAUSED_NOW.replace("%d", "4")]);
    let w2 = open(&mut disk, window(6, T + 1))?;
    for line in w.lines().iter().chain(w2.lines().iter()) {
        assert!(line.is_ascii() && !line.contains('\n'), "{line:?}");
        assert!(
            !line.contains('\u{1b}'),
            "in the window's own colors: {line:?}"
        );
    }
    Ok(())
}

#[test]
fn a_tag_whose_time_cannot_be_read_is_not_counted_nor_one_from_the_future() -> mlua::Result<()> {
    // A clock set back makes every tag "from the future": never stale, never a pause.
    let mut disk = Disk::from([
        (tag(1), "v1 abc".to_string()),
        (tag(2), "v2 123\n".to_string()),
        (tag(3), "1791694000\n".to_string()),
        (tag(4), format!("v1 {}\n", T + 1000)),
    ]);
    let w = open(&mut disk, window(5, T).prompted())?;
    assert!(!w.paused && w.omp);
    assert_eq!(
        disk.len(),
        4,
        "they never go stale, so a success leaves them: {disk:?}"
    );
    Ok(())
}

#[test]
fn a_tag_another_window_is_writing_is_left_alone() -> mlua::Result<()> {
    // Review Focus 2: io.open makes the file before its text is written, so another window can
    // see it empty. It is not counted, and a success does not take it away.
    let mut disk = tags_of_age(&[1, 2], 600);
    disk.insert(tag(9), String::new());
    let w = open(&mut disk, window(3, T).prompted())?;
    assert!(!w.paused);
    assert_eq!(disk, Disk::from([(tag(9), String::new())]));
    Ok(())
}

#[test]
fn cmd_studios_own_clink_set_never_counts() -> mlua::Result<()> {
    // Review Focus 1: the apply's check and doctor D3 run `clink --profile <ours> set clink.logo`,
    // which loads init.lua in clink_x64.exe and never shows a prompt. Three of them in a row, or
    // three Home visits, must not pause the theme, nor leave a tag.
    let mut disk = tags_of_age(&[1, 2, 3], 600);
    let before = disk.clone();
    for pid in 10..13 {
        let how = How {
            host: CLINK,
            inject: false,
            ..window(pid, T)
        };
        let w = open(&mut disk, how)?;
        assert!(
            !w.paused && w.omp,
            "it still loads the prompt, as it did before"
        );
        assert!(w.printed.is_empty(), "{:?}", w.printed);
    }
    assert_eq!(disk, before);
    // And with the theme paused it says nothing there either.
    let mut disk = Disk::from([(paused_file(), "{}".to_string())]);
    let w = open(
        &mut disk,
        How {
            host: CLINK,
            inject: false,
            ..window(14, T)
        },
    )?;
    assert!(w.paused && w.printed.is_empty());
    Ok(())
}

#[test]
fn whole_paths_from_globfiles_count_the_same() -> mlua::Result<()> {
    let mut disk = tags_of_age(&[1, 2, 3], 600);
    let w = open_with(&mut disk, window(4, T), |lua| {
        lua.load("__glob_paths = true").exec()
    })?;
    assert!(w.paused);
    assert_eq!(disk.keys().cloned().collect::<Vec<_>>(), [paused_file()]);
    Ok(())
}

#[test]
fn a_cmd_started_from_a_paused_window_says_nothing_again() -> mlua::Result<()> {
    let mut disk = Disk::from([(paused_file(), "{}".to_string())]);
    let first = open(&mut disk, window(7, T))?;
    assert_eq!(
        first.env.get("CMDSTUDIO_PAUSE_SAID").map(String::as_str),
        Some("1")
    );
    let nested = open_with(&mut disk, window(8, T), |lua| {
        lua.globals()
            .get::<Table>("__env")?
            .set("CMDSTUDIO_PAUSE_SAID", "1")
    })?;
    assert!(nested.paused && nested.printed.is_empty());
    Ok(())
}

#[test]
fn a_window_whose_prompt_never_comes_keeps_its_tag() -> mlua::Result<()> {
    // Oh My Posh fails (or hangs) at the prompt: CMD Studio's filter after it never runs.
    let mut disk = Disk::new();
    let w = open_with(&mut disk, window(6, T).prompted(), |lua| {
        lua.globals().set(
            "__popen_result",
            "local p = clink.promptfilter(1)\nfunction p:filter() error('oh-my-posh hung') end",
        )
    })?;
    assert_eq!(w.prompt, None);
    assert!(disk.contains_key(&tag(6)));
    Ok(())
}

#[test]
fn loading_init_lua_prints_nothing_and_sets_no_variable() -> mlua::Result<()> {
    // Only the inject callbacks print, and only in a cmd window (spec §2.2 #4).
    let mut disk = Disk::from([(paused_file(), "{}".to_string())]);
    let w = open(
        &mut disk,
        How {
            inject: false,
            ..window(7, T)
        },
    )?;
    assert!(w.paused && w.printed.is_empty() && w.env.is_empty());
    Ok(())
}
```

In `crates/core/tests/banner_lua.rs`:

1. After the test `disabled_banner_registers_no_callback`, add:

```rust
#[test]
fn a_paused_theme_prints_no_box_and_leaves_its_variable_unset() -> mlua::Result<()> {
    // init.lua sets CMDSTUDIO_PAUSED when it loaded no prompt (recovery spec §3.3); the box
    // waits for it, and a window started later, once the theme is back on, still gets its box.
    let lua = load(&banner_lua(&terracotta()), &CMD_HEADER)?;
    lua.globals().set("CMDSTUDIO_PAUSED", true)?;
    inject(&lua)?;
    assert!(printed(&lua)?.is_empty());
    let env: Table = lua.globals().get("__env")?;
    assert_eq!(env.pairs::<Value, Value>().count(), 0);
    Ok(())
}
```

2. In `INIT_STUBS`'s doc comment, after `…like \`clink.print\`.`, add the two lines `/// \`io.open\` finds no file: the start counter's own tests are \`tests/health_lua.rs\`, and these` and `/// run as a \`clink set\` does, with no \`os.gethost\` to call this a cmd window.`; and in `INIT_STUBS` itself, after the `print = …` line, add `io.open = function(path) return nil, path .. ": No such file or directory" end`.

3. In `load_init`, replace `lua.load(init_lua(Path::new(exe), Path::new(config)))` with

```rust
    let health = Path::new(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\health");
    lua.load(init_lua(Path::new(exe), Path::new(config), health))
```

In `crates/core/tests/golden.rs`, in `init_script`, after `&under_data(r"generated\theme.omp.json"),` add `&under_data("health"),`. In `crates/core/tests/english_in_cmd.rs`, after `let scripts = under_data("generated");` add `let health = under_data("health");`, and replace `init_lua(&omp_exe, &omp_config)` with `init_lua(&omp_exe, &omp_config, &health)`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p cmdstudio-core --test health_lua`
Expected: compile errors: `init_lua` takes 2 arguments but 3 were supplied; `PAUSED_NOW`, `PAUSED_STILL` not found in `generate::lua`.

- [ ] **Step 3: Write the counter in `crates/core/src/generate/lua.rs`**

Replace the module doc and the imports at the top (up to and including `const HEADER …`) with:

```rust
//! Theme → Clink Lua scripts (spec §3.6): `init.lua` counts the window's start and loads Oh My
//! Posh, and `banner.lua` prints the welcome box. Every string goes through `lua_str`.
//!
//! Clink loads every script in its folder whenever it runs, `clink set` included (spec §2.2
//! #4), so neither script may do anything at load that a person would notice. `banner.lua`
//! only registers a callback for `clink.oninject`; everything else, the terminal check for
//! the icon and the reading of the variables included, happens when that callback runs.
//! init.lua's start counter (recovery spec §3) runs at load, but only in a cmd window: CMD
//! Studio's own `clink set` (the apply's check, doctor D3) never reaches a prompt.
//!
//! Everything CMD Studio writes into a cmd window is English, whatever the language of Windows
//! (spec §3.6): the date, the tips, the line of a paused theme. Thai is for the app's own UI
//! only.
use crate::color::Rgb;
use crate::model::Theme;
use crate::recovery::{PAUSE_AFTER, STALE_AFTER_SECS};
use crate::text::{lua_str, sanitize_text};
use std::path::Path;

const HEADER: &str = "-- Generated by CMD Studio. Do not edit: changes are overwritten.\n";
```

Then, right after `HEADER` and before `INIT_BODY`, add:

```rust
/// The one line a cmd window prints, in the window's own colors, when it pauses the theme
/// (recovery spec §3.3); `%d` is the number of windows in a row that never reached their prompt.
pub const PAUSED_NOW: &str = "CMD Studio paused its theme because cmd failed to start %d times. \
                              Open CMD Studio to turn it back on.";
/// The line of every window after that one, while the theme stays paused.
pub const PAUSED_STILL: &str = "CMD Studio's theme is paused. Open CMD Studio to turn it back on.";

/// init.lua's start counter (recovery spec §3.2), in front of the load of Oh My Posh. It reads
/// `health`, `STALE_AFTER`, `PAUSE_AFTER`, `PAUSED_NOW` and `PAUSED_STILL`, which [`init_lua`]
/// puts before it, and sets the global `CMDSTUDIO_PAUSED` that banner.lua reads.
///
/// - The host check is `os.gethost()`: Clink's own program runs `clink set`, so only a name
///   ending in `\cmd.exe` counts (spike S4).
/// - "The first prompt" is a prompt filter of its own that Clink calls after Oh My Posh's: Oh My
///   Posh 31.5.0 registers `clink.promptfilter(_ZL_CLINK_PROMPT_PRIORITY + 1)`, 1 when the
///   variable is unset, its `filter` returns one value and so never ends the chain, and Clink
///   calls the lower number first. `clink.onbeginedit` would not do: Clink sends it before the
///   prompt filters (spike S6).
/// - The time in `paused.json` is written with `%.0f`: Lua 5.2 built with MSVC formats `%d`
///   through a 32-bit `long`, and a time in milliseconds is out of its range.
/// - Every file step is under `pcall`, and one that fails skips the counter: the theme is never
///   paused because a file could not be read or written.
const HEALTH_BODY: &str = r#"-- The start counter (recovery spec §3): each cmd window leaves a tag health\start-<pid> that
-- holds "v1 <unix seconds>" until its first prompt. PAUSE_AFTER tags older than STALE_AFTER
-- seconds are that many windows in a row that never got there, and the theme is paused in cmd:
-- health\paused.json. Only a cmd window counts: CMD Studio's own `clink set` (the apply's check,
-- doctor D3) loads this file too and never shows a prompt. A file that cannot be read or written
-- skips the counter: the theme is never paused for that.
local paused_file = health .. "\\paused.json"
local own_tag = nil
local paused_count = 0
local function in_cmd()
  local host = os.gethost and os.gethost() or ""
  return host:lower():find("\\cmd%.exe$") ~= nil
end
local host_ok, is_cmd = pcall(in_cmd)
is_cmd = host_ok and is_cmd
local function tag_time(path)
  local f = io.open(path, "rb")
  if not f then return nil end
  local text = f:read("*a") or ""
  f:close()
  local t = text:match("^v1 (%d+)%s*$")
  return t and tonumber(t)
end
local function tags()
  local found = {}
  for _, name in ipairs(os.globfiles(health .. "\\start-*") or {}) do
    local file = name:match("([^\\/]+)$")
    if file and file:match("^start%-%d+$") then found[#found + 1] = health .. "\\" .. file end
  end
  return found
end
local function stale(now)
  local old = {}
  for _, path in ipairs(tags()) do
    local t = tag_time(path)
    if t and now - t > STALE_AFTER then old[#old + 1] = path end
  end
  return old
end
local function count_start()
  local f = io.open(paused_file, "rb")
  if f then
    f:close()
    return true
  end
  if not is_cmd then return false end
  local now = os.time()
  local old = stale(now)
  if #old >= PAUSE_AFTER then
    local p = io.open(paused_file, "wb")
    if p then
      p:write(string.format('{"version":1,"reason":"cmdStarts","at":%.0f,"count":%d,"by":"clink"}\n', now * 1000, #old))
      p:close()
      for _, path in ipairs(tags()) do os.remove(path) end
      paused_count = #old
      return true
    end
  end
  local tag = health .. "\\start-" .. os.getpid()
  local t = io.open(tag, "wb")
  if t then
    t:write("v1 " .. now .. "\n")
    t:close()
    own_tag = tag
  end
  return false
end
local counted, paused = pcall(count_start)
if counted and paused then
  -- No prompt and no welcome box (banner.lua reads CMDSTUDIO_PAUSED), and one English line in a
  -- cmd window, once for it and the cmd windows started from it.
  CMDSTUDIO_PAUSED = true
  if is_cmd and clink.oninject then
    clink.oninject(function()
      if os.getenv("CMDSTUDIO_PAUSE_SAID") then return end
      os.setenv("CMDSTUDIO_PAUSE_SAID", "1")
      if paused_count > 0 then
        clink.print(string.format(PAUSED_NOW, paused_count))
      else
        clink.print(PAUSED_STILL)
      end
    end)
  end
  return
end
if counted and is_cmd and clink.promptfilter then
  -- Oh My Posh registers its prompt filter at _ZL_CLINK_PROMPT_PRIORITY + 1 (1 when that is
  -- unset), and Clink calls the lower number first: this one runs once Oh My Posh's has returned.
  local first = clink.promptfilter((tonumber(os.getenv("_ZL_CLINK_PROMPT_PRIORITY") or "") or 0) + 2)
  local reached = false
  function first:filter()
    if reached then return end
    reached = true
    pcall(function()
      if own_tag then os.remove(own_tag) end
      for _, path in ipairs(stale(os.time())) do os.remove(path) end
    end)
  end
end
"#;
```

Replace `ON_INJECT_HEAD` and its doc comment with:

```rust
/// The start of the callback. `CMDSTUDIO_BANNER` is inherited by every cmd started from
/// this one, so a nested window does not print the box again (spec §3.6). `CMDSTUDIO_PAUSED`
/// is init.lua's: while the theme is paused the window prints no box (recovery spec §3.3).
const ON_INJECT_HEAD: &str = r#"clink.oninject(function()
  if CMDSTUDIO_PAUSED then return end
  if os.getenv("CMDSTUDIO_BANNER") then return end
  os.setenv("CMDSTUDIO_BANNER", "1")
"#;
```

Replace `init_lua` and its doc comment with:

```rust
/// `init.lua`: the start counter over `health` (recovery spec §3, [`HEALTH_BODY`]), then Oh My
/// Posh, run by its full path, so nothing depends on PATH, and what it prints loaded.
/// `io.popen` hands the command to `cmd /c`, which drops the outermost quotes, so the command
/// is `""<exe>" init cmd --config "<config>" 2>nul"` (spec §5.4, G17). The `2>nul` keeps the
/// error text of cmd (a missing exe) out of every window, because `io.popen` pipes stdout
/// only. The script runs in every cmd window, so it never raises and prints nothing but the
/// one line of a paused theme (recovery spec §3.3): a failure is logged. The paths are written
/// by `lua_str`, which keeps Thai letters, spaces, `\` and `]]` as they are. A path that is not
/// valid Unicode is written with U+FFFD in place of the parts that are not.
pub fn init_lua(omp_exe: &Path, omp_config: &Path, health: &Path) -> String {
    let mut s = String::from(HEADER);
    s.push_str(&format!(
        "local omp = {}\n",
        lua_str(&omp_exe.to_string_lossy())
    ));
    s.push_str(&format!(
        "local config = {}\n",
        lua_str(&omp_config.to_string_lossy())
    ));
    s.push_str(&format!(
        "local health = {}\n",
        lua_str(&health.to_string_lossy())
    ));
    s.push_str(&format!(
        "local STALE_AFTER = {STALE_AFTER_SECS}\nlocal PAUSE_AFTER = {PAUSE_AFTER}\n"
    ));
    s.push_str(&format!(
        "local PAUSED_NOW = {}\nlocal PAUSED_STILL = {}\n",
        lua_str(PAUSED_NOW),
        lua_str(PAUSED_STILL)
    ));
    s.push_str(HEALTH_BODY);
    s.push_str(INIT_BODY);
    s
}
```

In the module's tests: in `init_lua_quotes_paths_and_wraps_the_popen_command`, add `Path::new(r"C:\x\health"),` as `init_lua`'s third argument; replace the body of `init_lua_embeds_both_paths_only_through_lua_str` from `let exe = …` on with

```rust
        let exe = r"C:\Users\ทดสอบ ผู้ใช้\]]\--[[\omp.exe";
        let config = r"D:\a b\x]]\theme.omp.json";
        let health = r"C:\Users\ทดสอบ ผู้ใช้\]]\health";
        let s = init_lua(Path::new(exe), Path::new(config), Path::new(health));
        assert!(s.contains(&format!("local omp = {}\n", lua_str(exe))));
        assert!(s.contains(&format!("local config = {}\n", lua_str(config))));
        assert!(s.contains(&format!("local health = {}\n", lua_str(health))));
        assert!(s.starts_with(HEADER));
    }
```

and after it add:

```rust
    #[test]
    fn init_lua_takes_its_numbers_from_recovery_and_counts_before_it_loads_the_prompt() {
        // recovery spec §3.1: the two numbers live once in Rust, and the generator writes them.
        let s = init_lua(
            Path::new(r"C:\omp.exe"),
            Path::new(r"C:\t.json"),
            Path::new(r"C:\h"),
        );
        assert!(
            s.contains("\nlocal STALE_AFTER = 60\nlocal PAUSE_AFTER = 3\n"),
            "{s}"
        );
        assert!(s.contains(&format!("\nlocal PAUSED_NOW = {}\n", lua_str(PAUSED_NOW))));
        assert!(s.contains(&format!(
            "\nlocal PAUSED_STILL = {}\n",
            lua_str(PAUSED_STILL)
        )));
        let counter = s
            .find("local counted, paused = pcall(count_start)")
            .unwrap();
        let prompt = s.find("io.popen(").unwrap();
        assert!(
            counter < prompt,
            "the tag is written before Oh My Posh runs"
        );
        // The time in milliseconds through %.0f: Lua 5.2's %d is a 32-bit long on MSVC.
        assert!(s.contains("\"at\":%.0f,\"count\":%d,"), "{s}");
    }

    #[test]
    fn the_lines_of_a_paused_theme_are_one_line_of_english_each() {
        for line in [PAUSED_NOW, PAUSED_STILL] {
            assert!(line.is_ascii() && !line.contains('\n'), "{line}");
        }
        assert_eq!(PAUSED_NOW.matches("%d").count(), 1);
        assert_eq!(
            PAUSED_NOW.replace("%d", "3"),
            "CMD Studio paused its theme because cmd failed to start 3 times. Open CMD Studio to \
             turn it back on."
        );
    }

    #[test]
    fn the_box_waits_for_no_pause() {
        let t = terracotta();
        assert!(banner_lua(&t).contains(
            "clink.oninject(function()\n  if CMDSTUDIO_PAUSED then return end\n  if os.getenv(\"CMDSTUDIO_BANNER\")"
        ));
    }
```

- [ ] **Step 4: Give the planner the folder** — `crates/core/src/plan.rs:403`: `contents: lua::init_lua(&paths.omp_exe(), &paths.omp_config(), &paths.health()),`

- [ ] **Step 5: Run the tests**

Run: `cargo test -p cmdstudio-core --test health_lua --test banner_lua --test english_in_cmd`
Expected: `17 passed`, `31 passed`, `2 passed`.
Run: `cargo test -p cmdstudio-core --lib -- generate::lua::`
Expected: every test passes, the three new ones among them.

- [ ] **Step 6: Regenerate the golden files and read their diff**

```powershell
$env:UPDATE_GOLDEN = '1'; cargo test -p cmdstudio-core --test golden; Remove-Item Env:UPDATE_GOLDEN
cargo test -p cmdstudio-core --test golden
git diff --stat -- crates/core/tests/golden
```

Expected: `21 passed` both times; exactly two files changed: `terracotta.banner.en.lua` gains one line (`  if CMDSTUDIO_PAUSED then return end` after `clink.oninject(function()`), and `init.lua` gains the five `local` lines after `local config = …` (`local health = "C:\\Users\\ผู้ทดสอบ ไทย\\AppData\\Local\\ai.resincore.cmdstudio\\health"`, `STALE_AFTER = 60`, `PAUSE_AFTER = 3`, the two lines) and the counter, before the unchanged load of Oh My Posh. Read the whole diff: it is what every cmd window runs.

- [ ] **Step 7: The whole core, clean**

Run: `cargo test -p cmdstudio-core` (every test passes; `lib` has 737), `cargo fmt --all`, the four clippy commands: clean.

- [ ] **Step 8: See that the host check bites** — in `HEALTH_BODY`, change `return host:lower():find("\\cmd%.exe$") ~= nil` to `return true`; `cargo test -p cmdstudio-core --test health_lua` FAILS `cmd_studios_own_clink_set_never_counts` (tags written, the theme paused). Restore; it passes.

- [ ] **Step 9: Commit**

Stage `crates/core/src/generate/lua.rs crates/core/src/plan.rs crates/core/tests/health_lua.rs crates/core/tests/banner_lua.rs crates/core/tests/golden.rs crates/core/tests/english_in_cmd.rs crates/core/tests/golden/init.lua crates/core/tests/golden/terracotta.banner.en.lua`. Message: `core: init.lua counts cmd's starts and pauses the theme after three that never reached a prompt (recovery spec §3)`.

---


### Task 4: The `EventLog` adapter: Windows' crash reports, and a fake

Recovery spec §4.3: the Application log's "Application Error" events 1000, read through the Event Log API (`EvtQuery` with an XPath query, `EvtNext`, `EvtRender` as XML), within five seconds, on a thread of its own; only EventData's fields and the event's time are read, never the message Windows formats in the machine's language. `System` gains the adapter, so doctor (Task 5) and the app (Task 7) reach it as they reach every other.

Spike results this task depends on: S9 (F8), S10 (F9), S11 (F10).

**Files:**
- Modify: `crates/core/Cargo.toml` (one windows-sys feature)
- Create: `crates/core/src/eventlog.rs`, `crates/core/src/adapters/windows/eventlog.rs`
- Create: `crates/core/tests/fixtures/eventlog/event1000-en.xml`, `event1000-th.xml` (copies of Task 1's)
- Modify: `crates/core/src/lib.rs`, `crates/core/src/adapters/mod.rs`, `crates/core/src/adapters/fake.rs`, `crates/core/src/adapters/windows/mod.rs`
- Modify: `crates/cli/src/lib.rs:1046-1100` (`run`), `crates/cli/tests/system.rs`, `app/src-tauri/src/machine.rs`, `app/src-tauri/src/library.rs` (`DiskMachine`): every `System { … }` written out in full

**Interfaces:**
- Consumes: Task 2's `recovery::CRASH_WINDOW_MS`, `utc::parse`; `adapters::windows::{last_error, wide}`.
- Produces:
  - `adapters::AppCrash { at_ms: u64, app: String, module: String }`; `trait adapters::EventLog { fn app_crashes(&self, since_ms: u64, now_ms: u64) -> Result<Vec<AppCrash>, AdapterError>; }`; `System { …, event_log: &'a dyn EventLog }`;
  - `adapters::fake::FakeEventLog { new(), crash(&self, at_ms, app, module), fail_reads(&self, why), asked(&self) -> Vec<(u64, u64)> }` and `FakeMachine.event_log: FakeEventLog`;
  - `adapters::windows::eventlog::{WinEventLog, WinEventLog::real(), READ_TIMEOUT, MOST_EVENTS}`;
  - `eventlog::{crash_query(within_ms: u64) -> String, parse_crash(xml: &str, fallback_ms: u64) -> Option<AppCrash>, is_clink_crash(&AppCrash) -> bool, clink_crashes(log: &dyn EventLog, since_ms: u64, now_ms: u64) -> Result<Vec<AppCrash>, AdapterError>}` (oldest first; at most seven days back from `now_ms`).

- [ ] **Step 1: The feature** — in `crates/core/Cargo.toml`, add `"Win32_System_EventLog",` to windows-sys's features after `"Win32_System_Diagnostics_Debug",`, and extend the comment above the dependency: after `…for Windows' error` / `# text in English.` write ` eventlog.rs: System_EventLog (EvtQuery, EvtNext, EvtRender, EvtClose) and` and `# Foundation (GetLastError), for cmd's crashes in Windows' Application log (recovery spec §4.3).` (windows-sys stays `=0.61.2`; `Cargo.lock` does not change: the lock file holds no features).

- [ ] **Step 2: The fixtures** — copy `docs/spikes/phase3b-recovery/event1000-en.xml` and `event1000-th.xml` (Task 1 Step 11) to `crates/core/tests/fixtures/eventlog/`, byte for byte.

- [ ] **Step 3: Write `crates/core/src/eventlog.rs`** (its tests are in it; `reads_the_events_the_spike_recorded_on_english_and_thai_windows` reads the fixtures)

```rust
//! cmd's crashes in Clink as Windows records them (recovery spec §4.3): the Application log's
//! "Application Error" events 1000, read through [`EventLog`] as XML, and only their EventData
//! fields and their time. The message Windows formats for an event is in the language of the
//! machine, so it is never read.
use crate::adapters::{AdapterError, AppCrash, EventLog};
use crate::recovery::CRASH_WINDOW_MS;
use crate::utc;

/// The faulting module of a crash in Clink: `clink_dll_x64.dll`, ASCII case ignored.
const CLINK_DLL: &str = "clink_dll";
/// The EventData fields of the crashed program and of the faulting module, as Windows 10 and 11
/// name them in English and in Thai alike (spike S9: `docs/spikes/phase3b-recovery/`).
const APP_FIELD: &str = "AppName";
const MODULE_FIELD: &str = "ModuleName";

/// The XPath query of the Application log for the "Application Error" events 1000 of the last
/// `within_ms` milliseconds (`timediff` counts back from now).
pub fn crash_query(within_ms: u64) -> String {
    format!(
        "*[System[Provider[@Name='Application Error'] and (EventID=1000) and \
         TimeCreated[timediff(@SystemTime) <= {within_ms}]]]"
    )
}

/// One event as `EvtRender` writes it in XML: the crashed program and the faulting module from
/// its EventData, and its time. The fields are read by name (`AppName`, `ModuleName`); an event
/// whose fields have no names gives them in their order, the program first and the module fourth
/// (the order of the names).
/// An event whose time can't be read is given `fallback_ms`: the query asked for its time span
/// already. `None` when the program or the module is missing.
pub fn parse_crash(xml: &str, fallback_ms: u64) -> Option<AppCrash> {
    let fields = data_fields(xml);
    let named = |name: &str| {
        fields
            .iter()
            .find(|(n, _)| n.as_deref() == Some(name))
            .map(|(_, v)| v.clone())
    };
    let unnamed = fields.iter().all(|(n, _)| n.is_none());
    let nth = |i: usize| {
        unnamed
            .then(|| fields.get(i).map(|(_, v)| v.clone()))
            .flatten()
    };
    let app = named(APP_FIELD).or_else(|| nth(0))?;
    let module = named(MODULE_FIELD).or_else(|| nth(3))?;
    let at_ms = element_attribute(xml, "TimeCreated", "SystemTime")
        .and_then(|t| utc::parse(&t))
        .unwrap_or(fallback_ms);
    Some(AppCrash { at_ms, app, module })
}

/// Whether `crash` is cmd crashing in Clink's DLL: the program is `cmd.exe` and the module's
/// name starts with `clink_dll`, ASCII case ignored, a folder in front of either left out.
pub fn is_clink_crash(crash: &AppCrash) -> bool {
    let file = |name: &str| name.rsplit(['\\', '/']).next().unwrap_or(name).to_string();
    let app = file(&crash.app);
    let module = file(&crash.module);
    app.eq_ignore_ascii_case("cmd.exe")
        && module
            .get(..CLINK_DLL.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(CLINK_DLL))
}

/// cmd's crashes in Clink's DLL that Windows recorded since `since_ms`, and at most
/// [`CRASH_WINDOW_MS`] before `now_ms`, oldest first.
pub fn clink_crashes(
    log: &dyn EventLog,
    since_ms: u64,
    now_ms: u64,
) -> Result<Vec<AppCrash>, AdapterError> {
    let since = since_ms.max(now_ms.saturating_sub(CRASH_WINDOW_MS));
    let mut found: Vec<AppCrash> = log
        .app_crashes(since, now_ms)?
        .into_iter()
        .filter(|c| c.at_ms >= since && is_clink_crash(c))
        .collect();
    found.sort_by_key(|c| c.at_ms);
    Ok(found)
}

/// Each `<Data>` element of the event's EventData, in order: its `Name`, when it has one, and its
/// text with the XML entities read. An empty element (`<Data Name='x'/>`) gives an empty text.
fn data_fields(xml: &str) -> Vec<(Option<String>, String)> {
    let mut fields = Vec::new();
    let Some((_, after)) = xml.split_once("<EventData") else {
        return fields;
    };
    let mut rest = after
        .split_once("</EventData>")
        .map_or(after, |(body, _)| body);
    while let Some((_, after)) = rest.split_once("<Data") {
        let Some((head, tail)) = after.split_once('>') else {
            break;
        };
        // `<DataItem…>` and the like are other elements.
        if !(head.is_empty() || head.starts_with(char::is_whitespace) || head == "/") {
            rest = tail;
            continue;
        }
        let name = attribute(head, "Name");
        if head.ends_with('/') {
            fields.push((name, String::new()));
            rest = tail;
            continue;
        }
        let Some((value, after_value)) = tail.split_once("</Data>") else {
            break;
        };
        fields.push((name, decode(value)));
        rest = after_value;
    }
    fields
}

/// The attribute `attr` of the first `<element …>` in `xml`.
fn element_attribute(xml: &str, element: &str, attr: &str) -> Option<String> {
    let (_, after) = xml.split_once(&format!("<{element}"))?;
    let (head, _) = after.split_once('>')?;
    attribute(head, attr)
}

/// The value of the attribute `attr` in the inside of a start tag (`head`, the text after the
/// element's name), quoted with `'` (as `EvtRender` writes) or `"`, entities read.
fn attribute(head: &str, attr: &str) -> Option<String> {
    ['\'', '"'].into_iter().find_map(|quote| {
        let (_, after) = head.split_once(&format!(" {attr}={quote}"))?;
        let (value, _) = after.split_once(quote)?;
        Some(decode(value))
    })
}

/// XML text with its five named entities and its numeric ones read; an `&` that starts none of
/// them stays as it is.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after)) = rest.split_once('&') {
        out.push_str(before);
        let found = after.split_once(';').and_then(|(entity, tail)| {
            let c = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            };
            c.map(|c| (c, tail))
        });
        match found {
            Some((c, tail)) => {
                out.push(c);
                rest = tail;
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// An event 1000 in the shape `EvtRender` gives on Windows 10 and 11: single quotes, named
    /// EventData fields.
    fn event(time: &str, app: &str, module: &str) -> String {
        format!(
            "<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System>\
             <Provider Name='Application Error' Guid='{{a0e9b465-b939-57d7-b27d-95d8e925ff57}}'/>\
             <EventID>1000</EventID><Version>0</Version><Level>2</Level><Task>100</Task>\
             <Opcode>0</Opcode><Keywords>0x8000000000000000</Keywords>\
             <TimeCreated SystemTime='{time}'/><EventRecordID>1234</EventRecordID>\
             <Correlation/><Execution ProcessID='5678' ThreadID='9012'/><Channel>Application</Channel>\
             <Computer>CMDSTUDIO-WIN11</Computer><Security UserID='S-1-5-21-1-2-3-1001'/></System>\
             <EventData><Data Name='AppName'>{app}</Data><Data Name='AppVersion'>10.0.26100.1</Data>\
             <Data Name='AppTimeStamp'>12345678</Data><Data Name='ModuleName'>{module}</Data>\
             <Data Name='ModuleVersion'>1.9.34.0</Data><Data Name='ModuleTimeStamp'>87654321</Data>\
             <Data Name='ExceptionCode'>c000001d</Data><Data Name='FaultingOffset'>0000000000001010</Data>\
             <Data Name='ProcessId'>0x162e</Data><Data Name='ProcessCreationTime'>0x1db3f0c2d7f1a2b</Data>\
             <Data Name='AppPath'>C:\\Windows\\system32\\cmd.exe</Data>\
             <Data Name='ModulePath'>C:\\Users\\tester\\AppData\\Local\\Temp\\clink\\dll_cache\\1.9.34.4e8ae4_97d16ab3\\clink_dll_x64.dll</Data>\
             <Data Name='IntegratorReportId'>0b6f4c3e-2a35-4e3b-9a4b-1f2e3d4c5b6a</Data>\
             <Data Name='PackageFullName'></Data><Data Name='PackageRelativeAppId'/></EventData></Event>"
        )
    }

    #[test]
    fn reads_the_program_the_module_and_the_time_by_name() {
        let xml = event(
            "2026-10-11T04:58:29.1234567Z",
            "cmd.exe",
            "clink_dll_x64.dll",
        );
        assert_eq!(
            parse_crash(&xml, 7),
            Some(AppCrash {
                at_ms: 1_791_694_709_123,
                app: "cmd.exe".to_string(),
                module: "clink_dll_x64.dll".to_string(),
            })
        );
        // Double quotes, entities in the text, and a time that can't be read.
        let xml = xml
            .replace('\'', "\"")
            .replace("2026-10-11T04:58:29.1234567Z", "not a time")
            .replace(">cmd.exe<", ">c&amp;md&#46;exe &lt;x&gt;<");
        let c = parse_crash(&xml, 7).unwrap();
        assert_eq!((c.at_ms, c.app.as_str()), (7, "c&md.exe <x>"));
    }

    #[test]
    fn reads_the_events_the_spike_recorded_on_english_and_thai_windows() {
        // Real "Application Error" events of cmd.exe crashing in a module named
        // clink_dll_x64.dll, saved by plan 3b-1 Task 1 (spike S9) on VM-A and VM-C.
        for xml in [
            include_str!("../tests/fixtures/eventlog/event1000-en.xml"),
            include_str!("../tests/fixtures/eventlog/event1000-th.xml"),
        ] {
            let c = parse_crash(xml, 0).unwrap();
            assert!(is_clink_crash(&c), "{c:?}");
            // The spike ran after Clink 1.9.34 came out (2026-09-21).
            assert!(c.at_ms > 1_790_000_000_000, "{c:?}");
        }
    }

    #[test]
    fn fields_without_names_are_read_in_their_order() {
        let xml = "<Event><System><TimeCreated SystemTime='2026-10-11T04:58:29Z'/></System>\
                   <EventData><Data>cmd.exe</Data><Data>10.0</Data><Data>1</Data>\
                   <Data>CLINK_DLL_X64.DLL</Data><Data>1.9.34.0</Data></EventData></Event>";
        let c = parse_crash(xml, 0).unwrap();
        assert_eq!(
            (c.app.as_str(), c.module.as_str()),
            ("cmd.exe", "CLINK_DLL_X64.DLL")
        );
        assert!(is_clink_crash(&c));
        // Named fields without the two names are not read by position.
        let named = "<Event><EventData><Data Name='Other'>cmd.exe</Data></EventData></Event>";
        assert_eq!(parse_crash(named, 0), None);
        assert_eq!(parse_crash("<Event><System/></Event>", 0), None);
        assert_eq!(parse_crash("", 0), None);
        assert_eq!(
            parse_crash("<EventData><Data Name='AppName'>cmd.exe", 0),
            None
        );
    }

    #[test]
    fn only_cmd_crashing_in_clinks_dll_counts() {
        let crash = |app: &str, module: &str| AppCrash {
            at_ms: 0,
            app: app.to_string(),
            module: module.to_string(),
        };
        for (app, module) in [
            ("cmd.exe", "clink_dll_x64.dll"),
            ("CMD.EXE", "Clink_DLL_x64.dll"),
            (r"C:\Windows\System32\cmd.exe", r"C:\x\clink_dll_x86.dll"),
        ] {
            assert!(is_clink_crash(&crash(app, module)), "{app} {module}");
        }
        for (app, module) in [
            ("notepad.exe", "clink_dll_x64.dll"),
            ("cmd.exe", "ntdll.dll"),
            ("cmd.exe", "clink"),
            ("cmd.exe", "เสีย_clink_dll.dll"),
            ("cmd.exe.bak", "clink_dll_x64.dll"),
            ("", ""),
        ] {
            assert!(!is_clink_crash(&crash(app, module)), "{app} {module}");
        }
    }

    struct Log {
        crashes: Vec<AppCrash>,
        asked: RefCell<Vec<(u64, u64)>>,
    }

    impl EventLog for Log {
        fn app_crashes(&self, since_ms: u64, now_ms: u64) -> Result<Vec<AppCrash>, AdapterError> {
            self.asked.borrow_mut().push((since_ms, now_ms));
            Ok(self.crashes.clone())
        }
    }

    #[test]
    fn counts_from_the_apply_and_never_more_than_seven_days_back() {
        let now = 1_791_694_709_000;
        let day = 24 * 60 * 60 * 1000;
        let at = |ms: u64, module: &str| AppCrash {
            at_ms: ms,
            app: "cmd.exe".to_string(),
            module: module.to_string(),
        };
        let log = Log {
            crashes: vec![
                at(now - 1000, "clink_dll_x64.dll"),
                at(now - 2 * day, "clink_dll_x64.dll"),
                at(now - 8 * day, "clink_dll_x64.dll"),
                at(now - 3000, "ntdll.dll"),
            ],
            asked: RefCell::new(Vec::new()),
        };
        let found = clink_crashes(&log, now - 3 * day, now).unwrap();
        assert_eq!(
            found.iter().map(|c| c.at_ms).collect::<Vec<_>>(),
            [now - 2 * day, now - 1000]
        );
        assert_eq!(clink_crashes(&log, now - 1500, now).unwrap().len(), 1);
        // An apply longer ago than seven days: seven days back.
        assert_eq!(clink_crashes(&log, 0, now).unwrap().len(), 2);
        assert_eq!(
            log.asked.borrow().as_slice(),
            [
                (now - 3 * day, now),
                (now - 1500, now),
                (now - 7 * day, now)
            ]
        );
    }

    #[test]
    fn the_query_is_the_one_the_spike_ran() {
        assert_eq!(
            crash_query(604_800_000),
            "*[System[Provider[@Name='Application Error'] and (EventID=1000) and \
             TimeCreated[timediff(@SystemTime) <= 604800000]]]"
        );
    }
}
```

Register it in `crates/core/src/lib.rs` after `pub mod doctor;`: `pub mod eventlog;`.

- [ ] **Step 4: The trait and `System`** — in `crates/core/src/adapters/mod.rs`, right before `/// Everything that touches the machine, borrowed for one command.`, add

```rust
/// One "Application Error" event (ID 1000) of Windows' Application log: a program that crashed,
/// as the event's EventData names it (recovery spec §4.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppCrash {
    /// When Windows recorded it, in milliseconds since 1970.
    pub at_ms: u64,
    /// The program that crashed, as the event names it: `cmd.exe`.
    pub app: String,
    /// The module it crashed in: `clink_dll_x64.dll`.
    pub module: String,
}

/// Windows' crash reports (recovery spec §4.3). Read only.
pub trait EventLog {
    /// Every program's "Application Error" events 1000 from `since_ms` to `now_ms`
    /// (milliseconds since 1970). `Err` when the log can't be read: it is off, this user may not
    /// read it, or it did not answer in time. A caller never takes that for "no crash".
    fn app_crashes(&self, since_ms: u64, now_ms: u64) -> Result<Vec<AppCrash>, AdapterError>;
}
```

and in `struct System`, after `pub host: &'a dyn Host,`, add `pub event_log: &'a dyn EventLog,`.

- [ ] **Step 5: The fake** — in `crates/core/src/adapters/fake.rs`, add `AppCrash` and `EventLog` to the `use super::{…}` list (it becomes `AdapterError, AppCrash, Arch, AutoRun, Created, DefaultTerminal, DirEntry, DirState, EventLog, Fonts, Fs, FsLock, Host, Output, Process, RegValue, System, SystemInfo, WtLocation, WtSettings`); right before `/// One of each fake, for tests that need a whole \`System\`.` add

```rust
/// Windows' crash reports in memory: the crashes a test records, or a log that can't be read.
/// It starts empty: no program ever crashed.
#[derive(Default)]
pub struct FakeEventLog {
    state: RefCell<EventLogState>,
}

#[derive(Default)]
struct EventLogState {
    crashes: Vec<AppCrash>,
    unreadable: Option<String>,
    asked: Vec<(u64, u64)>,
}

impl FakeEventLog {
    pub fn new() -> Self {
        Self::default()
    }
    /// Records that `app` crashed in `module` at `at_ms`.
    pub fn crash(&self, at_ms: u64, app: &str, module: &str) {
        self.state.borrow_mut().crashes.push(AppCrash {
            at_ms,
            app: app.to_string(),
            module: module.to_string(),
        });
    }
    /// Every read fails with `why`, as a log that is off, or that this user may not read.
    pub fn fail_reads(&self, why: &str) {
        self.state.borrow_mut().unreadable = Some(why.to_string());
    }
    /// The span each read asked for, `(since_ms, now_ms)`, in order.
    pub fn asked(&self) -> Vec<(u64, u64)> {
        self.state.borrow().asked.clone()
    }
}

impl EventLog for FakeEventLog {
    fn app_crashes(&self, since_ms: u64, now_ms: u64) -> Result<Vec<AppCrash>, AdapterError> {
        let mut s = self.state.borrow_mut();
        s.asked.push((since_ms, now_ms));
        if let Some(why) = &s.unreadable {
            return Err(AdapterError::new(
                "read the Application event log",
                why.clone(),
            ));
        }
        Ok(s.crashes
            .iter()
            .filter(|c| (since_ms..=now_ms).contains(&c.at_ms))
            .cloned()
            .collect())
    }
}
```

and in `FakeMachine`: the field `pub event_log: FakeEventLog,` after `pub host: FakeHost,`, `event_log: FakeEventLog::new(),` after `host: FakeHost::default(),` in `new()`, and `event_log: &self.event_log,` after `host: &self.host,` in `system()`.

- [ ] **Step 6: The Windows adapter** — `crates/core/src/adapters/windows/eventlog.rs` (its tests are in it: `an_answer_in_time_is_the_answer_and_a_late_one_an_error` is Review Focus 4; `this_computers_application_log_answers_within_its_time` reads this computer's log only with `CMDSTUDIO_SYSTEM_TESTS=1`, CI's system step, and prints SKIPPED otherwise):

```rust
//! The Application event log through Windows' Event Log API (recovery spec §4.3): the
//! "Application Error" events 1000 (`EvtQuery` with an XPath query, `EvtNext`, `EvtRender` as
//! XML). Read only. The read runs on a thread of its own and is given up after five seconds: a
//! log that is off, that this user may not read, or that does not answer in time is an error,
//! which the callers take for "could not be read" and never for "no crash".
use super::{last_error, wide};
use crate::adapters::{AdapterError, AppCrash, EventLog};
use crate::eventlog::{crash_query, parse_crash};
use std::ffi::OsStr;
use std::sync::mpsc;
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS,
};
use windows_sys::Win32::System::EventLog::{
    EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection, EvtRender,
    EvtRenderEventXml, EVT_HANDLE,
};

/// How long one read of the log may take (recovery spec §4.3).
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// The most events one read takes, newest first: far more crashes than pause the theme.
pub const MOST_EVENTS: usize = 200;
/// How many handles `EvtNext` gives at a time.
const BATCH: usize = 16;

/// This computer's Application log.
pub struct WinEventLog {
    timeout: Duration,
}

impl WinEventLog {
    /// The log, read within [`READ_TIMEOUT`].
    pub fn real() -> Self {
        WinEventLog {
            timeout: READ_TIMEOUT,
        }
    }
}

impl EventLog for WinEventLog {
    fn app_crashes(&self, since_ms: u64, now_ms: u64) -> Result<Vec<AppCrash>, AdapterError> {
        let within = now_ms.saturating_sub(since_ms);
        let events = within_time(self.timeout, move || read_events(within))?;
        Ok(events
            .iter()
            .filter_map(|xml| parse_crash(xml, since_ms))
            .collect())
    }
}

/// Runs `work` on a thread of its own and waits `timeout` for its answer. One that does not
/// come in time is an error; the thread is left to end by itself.
fn within_time<T: Send + 'static>(
    timeout: Duration,
    work: impl FnOnce() -> Result<T, AdapterError> + Send + 'static,
) -> Result<T, AdapterError> {
    let fail = |message: String| AdapterError::new("read the Application event log", message);
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("event-log".to_string())
        .spawn(move || {
            // The receiver may have given up already; then nobody waits for the answer.
            let _ = tx.send(work());
        })
        .map_err(|e| fail(format!("could not start the reader: {e}")))?;
    match rx.recv_timeout(timeout) {
        Ok(answer) => answer,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(fail(format!(
            "it did not answer within {} seconds",
            timeout.as_secs()
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(fail("the reader stopped without an answer".to_string()))
        }
    }
}

/// An event log handle, closed when dropped.
struct Handle(EVT_HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: the handle came from EvtQuery or EvtNext and is closed here, once.
            unsafe { EvtClose(self.0) };
        }
    }
}

/// The XML of the "Application Error" events 1000 of the last `within_ms` milliseconds, newest
/// first, at most [`MOST_EVENTS`].
fn read_events(within_ms: u64) -> Result<Vec<String>, AdapterError> {
    let path = wide(OsStr::new("Application"));
    let query = wide(OsStr::new(&crash_query(within_ms)));
    // SAFETY: both strings are NUL-terminated UTF-16 that outlive the call; session 0 is this
    // computer.
    let results = Handle(unsafe {
        EvtQuery(
            0,
            path.as_ptr(),
            query.as_ptr(),
            EvtQueryChannelPath | EvtQueryReverseDirection,
        )
    });
    if results.0 == 0 {
        return Err(last_error("EvtQuery Application"));
    }
    let mut xmls = Vec::new();
    loop {
        let mut batch = [0 as EVT_HANDLE; BATCH];
        let mut returned = 0u32;
        // SAFETY: `batch` holds BATCH handles, the count given; the call writes at most that
        // many, and their number into `returned`. 1000 ms is the wait for each batch.
        let ok = unsafe {
            EvtNext(
                results.0,
                BATCH as u32,
                batch.as_mut_ptr(),
                1000,
                0,
                &mut returned,
            )
        };
        if ok == 0 {
            // SAFETY: GetLastError only reads this thread's last error, which EvtNext set.
            if unsafe { GetLastError() } == ERROR_NO_MORE_ITEMS {
                break;
            }
            return Err(last_error("EvtNext Application"));
        }
        let count = usize::try_from(returned).unwrap_or(0).min(BATCH);
        // Each handle is closed when its `Handle` goes, also when a render fails.
        let events: Vec<Handle> = batch.iter().take(count).map(|h| Handle(*h)).collect();
        for event in &events {
            xmls.push(render(event)?);
            if xmls.len() >= MOST_EVENTS {
                return Ok(xmls);
            }
        }
    }
    Ok(xmls)
}

/// One event as XML (`EvtRenderEventXml`): its size first, then its text.
fn render(event: &Handle) -> Result<String, AdapterError> {
    let (mut used, mut properties) = (0u32, 0u32);
    // SAFETY: a size query: no buffer (NULL, size 0); the call writes the size it needs, in
    // bytes, into `used`, and `properties`.
    let sized = unsafe {
        EvtRender(
            0,
            event.0,
            EvtRenderEventXml,
            0,
            std::ptr::null_mut(),
            &mut used,
            &mut properties,
        )
    };
    // SAFETY: GetLastError only reads this thread's last error, which EvtRender set.
    if sized == 0 && unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        return Err(last_error("EvtRender"));
    }
    let mut buffer = vec![0u16; usize::try_from(used).unwrap_or(0) / 2 + 1];
    let size = u32::try_from(buffer.len() * 2).unwrap_or(u32::MAX);
    // SAFETY: `buffer` holds `size` writable bytes; the call writes at most that many, the
    // UTF-16 text and its closing NUL.
    let rendered = unsafe {
        EvtRender(
            0,
            event.0,
            EvtRenderEventXml,
            size,
            buffer.as_mut_ptr().cast(),
            &mut used,
            &mut properties,
        )
    };
    if rendered == 0 {
        return Err(last_error("EvtRender"));
    }
    let text: Vec<u16> = buffer.iter().copied().take_while(|u| *u != 0).collect();
    Ok(String::from_utf16_lossy(&text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovery::CRASH_WINDOW_MS;

    #[test]
    fn an_answer_in_time_is_the_answer_and_a_late_one_an_error() {
        // Review Focus 4: a log that does not answer is "could not be read", never "no crash".
        assert_eq!(within_time(Duration::from_secs(5), || Ok(7)), Ok(7));
        let late = within_time(Duration::from_millis(50), || {
            std::thread::sleep(Duration::from_millis(500));
            Ok(7)
        });
        let e = late.unwrap_err();
        assert!(e.message.contains("did not answer within"), "{e:?}");
        let failed: Result<u8, _> = within_time(Duration::from_secs(5), || {
            Err(AdapterError::new(
                "EvtQuery Application",
                "Access is denied.",
            ))
        });
        assert_eq!(failed.unwrap_err().message, "Access is denied.");
    }

    #[test]
    fn this_computers_application_log_answers_within_its_time() {
        // Reads this computer's Application log: on CI's runner and the test VMs only
        // (CMDSTUDIO_SYSTEM_TESTS=1), never on a dev PC by default.
        if std::env::var("CMDSTUDIO_SYSTEM_TESTS").as_deref() != Ok("1") {
            eprintln!("SKIPPED the Application log read: it runs with CMDSTUDIO_SYSTEM_TESTS=1");
            return;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap())
            .unwrap();
        let started = std::time::Instant::now();
        let crashes = WinEventLog::real().app_crashes(now - CRASH_WINDOW_MS, now);
        assert!(crashes.is_ok(), "{crashes:?}");
        assert!(started.elapsed() < READ_TIMEOUT + Duration::from_secs(1));
    }
}
```

Register it in `crates/core/src/adapters/windows/mod.rs` before `pub mod fonts;`: `pub mod eventlog;`.

- [ ] **Step 7: Every `System` written out in full gets the real log**

- `crates/cli/src/lib.rs`, `run`: the `use cmdstudio_core::adapters::windows::{…}` inside it becomes `eventlog::WinEventLog, fonts::WinFonts, fs::WinFs, host::WinHost, process::WinProcess, registry::WinAutoRun, wt::WinWt`; after `let process = WinProcess::new(allowed, std::time::Duration::from_secs(60));` add `let event_log = WinEventLog::real();`; in the `System { … }` after `host: &host,` add `event_log: &event_log,`.
- `crates/cli/tests/system.rs`: the same three changes (its import list is at the top of the file, and its `System` follows `let process = WinProcess::new(vec![data.join("runtime"), program_data.clone()], Duration::from_secs(60));`).
- `app/src-tauri/src/machine.rs`: the import list as in `run`; the field `event_log: WinEventLog,` after `host: WinHost,` in `RealMachine`; `event_log: &self.event_log,` after `host: &self.host,` in `system()`; `event_log: WinEventLog::real(),` after `host,` where `real_backend` builds the `RealMachine`.
- `app/src-tauri/src/library.rs`, `mod on_disk`: `eventlog::WinEventLog,` first in its `use cmdstudio_core::adapters::windows::{…}`; the field `event_log: WinEventLog,` after `host: WinHost,` in `DiskMachine`; `event_log: WinEventLog::real(),` after `host: WinHost,` in `DiskMachine::under`; `event_log: &self.event_log,` after `host: &self.host,` in its `system()` (the library's tests never call it).

The `System { …, ..m.system() }` forms in the core's and the CLI's tests need nothing.

- [ ] **Step 8: Run the tests**

Run: `cargo test -p cmdstudio-core --lib -- eventlog`
Expected: `8 passed` (six in `eventlog`, two in `adapters::windows::eventlog`).
Run: `cargo test -p cmdstudio-core` and `cargo test -p cmdstudio-cli`: every test passes (the core's `lib` has 745). `cargo test -p cmdstudio-app --lib`: every test passes (WDAC: see Global Constraints).
Then `cargo fmt --all` and the four clippy commands: clean.

- [ ] **Step 9: Commit**

Stage `crates/core/Cargo.toml crates/core/src/eventlog.rs crates/core/src/lib.rs crates/core/src/adapters/mod.rs crates/core/src/adapters/fake.rs crates/core/src/adapters/windows/mod.rs crates/core/src/adapters/windows/eventlog.rs crates/core/tests/fixtures/eventlog/event1000-en.xml crates/core/tests/fixtures/eventlog/event1000-th.xml crates/cli/src/lib.rs crates/cli/tests/system.rs app/src-tauri/src/machine.rs app/src-tauri/src/library.rs`. Message: `core: Windows' crash reports through the Event Log API, and a fake (recovery spec §4.3)`.

---

### Task 5: Doctor D12, and its two codes in spec §5.9

Recovery spec §4.5 as D12 (D10 is taken): `warn` `E_THEME_PAUSED` while the theme is paused, with the level, the reason and since when; then, with a theme applied, cmd's crashes in Clink since the apply: none `ok`, one or two `warn` `E_CMD_CRASHED`, three or more `fail` `E_CMD_CRASHED`; crash reports that can't be read `skipped` with the reason. The two codes become rows of main spec §5.9 and of `error.rs`'s `MEANINGS` in this commit: the app's test compares the two, and the core's `error_codes` test wants every code the production code says to be a row.

**Files:**
- Modify: `crates/core/src/doctor.rs`
- Modify: `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§5.9: two rows)
- Modify: `app/src-tauri/src/error.rs` (`MEANINGS`)
- Modify: `crates/cli/src/lib.rs` (two tests count twelve checks), `app/src-tauri/src/backend/doctor.rs` (one test)

**Interfaces:**
- Consumes: Task 2's `recovery::{read_pause, PauseReason, Paused, PausedBy, PAUSE_AFTER}`, `utc::iso`; Task 4's `eventlog::clink_crashes`, `System.event_log`, `FakeMachine.event_log`.
- Produces: `doctor()` returns D1–D12, then R1–R3; D12's texts and codes as above; the §5.9 rows `E_THEME_PAUSED` and `E_CMD_CRASHED`.

- [ ] **Step 1: Write the failing tests** — at the end of `doctor.rs`'s `mod tests`, before its closing `}`:

```rust
    // ---- D12: the theme's pause and cmd's crashes in Clink (recovery spec §4.5) ----

    const CLINK_DLL: &str = "clink_dll_x64.dll";

    #[test]
    fn d12_is_ok_after_an_apply_and_skipped_before_one() {
        let (m, ctx) = machine();
        let d12 = by_id(&run(&m, &ctx), "D12");
        assert_eq!((d12.status, d12.code), (Status::Skipped, None));
        assert!(d12.message_en.contains("No theme"), "{d12:?}");
        let (m, ctx) = themed(Target::All);
        let d12 = by_id(&run(&m, &ctx), "D12");
        assert_eq!((d12.status, d12.code), (Status::Ok, None), "{d12:?}");
        // The crashes since the apply, and never more than seven days back.
        assert_eq!(m.event_log.asked(), [(ctx.now_ms, ctx.now_ms)]);
        let much_later = later(&ctx, 10 * 24 * 60 * 60 * 1000);
        run(&m, &much_later);
        assert_eq!(
            m.event_log.asked().last(),
            Some(&(
                much_later.now_ms - crate::recovery::CRASH_WINDOW_MS,
                much_later.now_ms
            ))
        );
    }

    #[test]
    fn d12_counts_cmds_crashes_in_clink_since_the_apply() {
        let (m, ctx) = themed(Target::All);
        let at = ctx.now_ms;
        let then = later(&ctx, 60_000);
        // Before the apply, another program, another module: none of them counts.
        m.event_log.crash(at - 1, "cmd.exe", CLINK_DLL);
        m.event_log.crash(at + 1, "notepad.exe", CLINK_DLL);
        m.event_log.crash(at + 2, "cmd.exe", "ntdll.dll");
        assert_eq!(by_id(&run(&m, &then), "D12").status, Status::Ok);
        m.event_log.crash(at + 10, "cmd.exe", CLINK_DLL);
        m.event_log.crash(at + 20, "CMD.EXE", "Clink_DLL_x64.dll");
        let d12 = by_id(&run(&m, &then), "D12");
        assert_eq!(
            (d12.status, d12.code),
            (Status::Warn, Some("E_CMD_CRASHED")),
            "{d12:?}"
        );
        assert!(d12.message_en.contains("2 time(s)"), "{d12:?}");
        assert_eq!(d12.details.lines().count(), 2, "{d12:?}");
        assert!(d12.details.starts_with(&utc::iso(at + 10)), "{d12:?}");
        m.event_log.crash(at + 30, "cmd.exe", CLINK_DLL);
        let d12 = by_id(&run(&m, &then), "D12");
        assert_eq!(
            (d12.status, d12.code),
            (Status::Fail, Some("E_CMD_CRASHED")),
            "{d12:?}"
        );
        assert!(d12.message_en.contains("the theme is still on"), "{d12:?}");
    }

    #[test]
    fn d12_never_takes_crash_reports_it_cannot_read_for_none() {
        // Review Focus 4: a log that is off, that this user may not read, or that is slow.
        let (m, ctx) = themed(Target::All);
        m.event_log.fail_reads("it did not answer within 5 seconds");
        let d12 = by_id(&run(&m, &ctx), "D12");
        assert_eq!((d12.status, d12.code), (Status::Skipped, None));
        assert!(
            d12.message_en.contains(
                "could not be read: read the Application event log: it did not answer within 5 \
                 seconds"
            ),
            "{d12:?}"
        );
    }

    #[test]
    fn d12_says_a_pause_first_with_its_level_its_reason_and_since_when() {
        let (m, ctx) = themed(Target::All);
        let pause = ctx.paths.paused_file();
        m.fs.put(
            &pause,
            br#"{"version":1,"reason":"cmdStarts","at":1791694709000,"count":3,"by":"clink"}"#,
        );
        // The crashes are what a pause is about: it is said first, and they are not read.
        for _ in 0..3 {
            m.event_log.crash(ctx.now_ms, "cmd.exe", CLINK_DLL);
        }
        let d12 = by_id(&run(&m, &ctx), "D12");
        assert_eq!(
            (d12.status, d12.code),
            (Status::Warn, Some("E_THEME_PAUSED")),
            "{d12:?}"
        );
        assert!(
            d12.message_en
                .contains("failed to reach its prompt 3 times in a row"),
            "{d12:?}"
        );
        assert_eq!(
            d12.details,
            "paused: cmd only (nothing on this computer was changed)\nreason: cmdStarts\n\
             since: 2026-10-11T04:58:29Z"
        );
        assert!(m.event_log.asked().is_empty());
        // The app's pause of this computer, for each of its reasons, and a record it can't read.
        for (text, says) in [
            (
                r#"{"version":1,"reason":"clinkCrash","at":1,"count":4,"by":"app"}"#,
                "crashed in its add-on 4 times",
            ),
            (
                r#"{"version":1,"reason":"user","at":1,"count":0,"by":"app"}"#,
                "as you asked",
            ),
            (
                r#"{"version":1,"reason":"recovery","at":1,"count":0,"by":"app"}"#,
                "CMD Studio Recovery shortcut",
            ),
            ("{ not json", "The theme is paused. Turn it back on"),
        ] {
            m.fs.put(&pause, text);
            let d12 = by_id(&run(&m, &ctx), "D12");
            assert_eq!(
                (d12.status, d12.code),
                (Status::Warn, Some("E_THEME_PAUSED")),
                "{text}"
            );
            assert!(d12.message_en.contains(says), "{text}: {d12:?}");
        }
        // A pause of this computer took the theme out: no theme is applied, and it is said.
        let (m, ctx) = machine();
        m.fs.put(
            ctx.paths.paused_file(),
            br#"{"version":1,"reason":"user","at":1,"count":0,"by":"app"}"#,
        );
        assert_eq!(by_id(&run(&m, &ctx), "D12").code, Some("E_THEME_PAUSED"));
    }

    #[test]
    fn d12_fails_with_the_read_error_when_the_pause_record_cannot_be_read() {
        let (m, ctx) = themed(Target::All);
        m.fs.put(ctx.paths.paused_file(), b"{}");
        m.fs.fail_reads_of(ctx.paths.paused_file());
        let d12 = by_id(&run(&m, &ctx), "D12");
        assert_eq!(
            (d12.status, d12.code),
            (Status::Fail, Some("E_MACHINE_READ")),
            "{d12:?}"
        );
        assert!(d12.message_en.contains("paused.json"), "{d12:?}");
    }
```

And update what counts the checks:
- `doctor.rs` tests: in `a_fresh_machine_skips_what_needs_a_theme` and in `run()`, the list `["D1", …, "D11"]` gets `"D12"` at its end, and `ids.get(..11)` becomes `ids.get(..12)`; in `a_fresh_machine_skips_what_needs_a_theme`, `for id in ["D1", "D2", "D3", "D4", "D7", "D8", "D9"]` gets `"D12"` at its end; in `doctor_only_reads_and_never_waits_for_the_lock`, both `assert_eq!(run(&m, &ctx).len(), 11);` become `12`; in `restore_points_that_cannot_be_read_are_reported_and_block_nothing` and `a_crash_that_cannot_be_checked_yet_is_reported_with_its_way_out`, `checks.len()`'s `12` becomes `13`; `run()`'s doc comment says `D1–D12 first, in order`.
- `crates/cli/src/lib.rs` tests: in `apply_doctor_undo_round_trip`, `assert_eq!(v["checks"].as_array().unwrap().len(), 11);` becomes `12`; in `a_journal_that_cannot_be_read_stops_every_change_and_set_aside_is_the_way_out`, `(EXIT_FAILED, 12)` becomes `(EXIT_FAILED, 13)` and the three `checks[11]` become `checks[12]`.
- `app/src-tauri/src/backend/doctor.rs` test `a_clean_machine_after_an_apply_needs_no_fix`: `assert_eq!(report.checks.len(), 11);` becomes `12`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p cmdstudio-core --lib -- doctor::`
Expected: FAIL: the five `d12_…` tests (no check D12) and the counts.

- [ ] **Step 3: Write D12** — in `crates/core/src/doctor.rs`:
- the module doc's first line says `Health checks D1–D12 (spec §5.8), …`; `doctor()`'s doc says `Runs D1–D12, always all twelve and in this order, …`;
- imports: add `use crate::eventlog::clink_crashes;` after `use crate::adapters::…;`, `use crate::recovery::{read_pause, PauseReason, Paused, PausedBy, PAUSE_AFTER};` after `use crate::plan::{…};`, and `use crate::utc;` after `use crate::txn::{Failure, JournalStore};`;
- in `doctor()`, after `d11(sys),` add `d12(sys, ctx, &state),`;
- after the function `d11`, add:

```rust
/// D12 (recovery spec §4.5, numbered after D11: D10 is the packaged-app check): the theme's
/// pause, then cmd's crashes in Clink since the theme was applied, as Windows' crash reports (the
/// Application event log) say. A pause comes first: while the theme is paused, its crashes are
/// what paused it. A pause record that can't be read is `E_MACHINE_READ`; crash reports that
/// can't be read are `skipped` with the reason, never "no crash".
fn d12(sys: System<'_>, ctx: &Ctx, state: &Result<State, StateError>) -> Check {
    match read_pause(sys.fs, &ctx.paths) {
        Ok(Some(paused)) => return paused_check(&paused),
        Ok(None) => {}
        Err(e) => {
            return unreadable(
                "D12",
                "บันทึกการหยุดธีมของ CMD Studio (paused.json)",
                "CMD Studio's record of the theme's pause (paused.json)",
                e,
            )
        }
    }
    let a = match applied("D12", state) {
        Ok(a) => a,
        Err(skipped) => return skipped,
    };
    let crashes = match clink_crashes(sys.event_log, a.applied_at_ms, ctx.now_ms) {
        Ok(crashes) => crashes,
        Err(e) => {
            return check(
                "D12",
                Status::Skipped,
                None,
                format!(
                "ตรวจข้อนี้ไม่ได้ เพราะอ่านรายงานแครชของ Windows (event log ของ Application) ไม่ได้: {e}"
            ),
                format!(
                    "Not checked: Windows' crash reports (the Application event log) could not be \
                     read: {e}"
                ),
                "",
            )
        }
    };
    let n = u32::try_from(crashes.len()).unwrap_or(u32::MAX);
    if n == 0 {
        return check(
            "D12",
            Status::Ok,
            None,
            "cmd ไม่ได้แครชในส่วนเสริมตั้งแต่ใช้ธีม และธีมไม่ได้ถูกหยุดไว้",
            "cmd has not crashed in its add-on since the theme was applied, and the theme is not \
             paused",
            "",
        );
    }
    let details = crashes
        .iter()
        .map(|c| format!("{} {} crashed in {}", utc::iso(c.at_ms), c.app, c.module))
        .collect::<Vec<_>>()
        .join("\n");
    if n < PAUSE_AFTER {
        return check(
            "D12",
            Status::Warn,
            Some("E_CMD_CRASHED"),
            format!(
                "cmd แครชในส่วนเสริม {n} ครั้งตั้งแต่ใช้ธีม ถ้ายังเกิดอีก ให้หยุดธีมใน CMD Studio เพื่อให้ cmd เปิดได้"
            ),
            format!(
                "cmd crashed in its add-on {n} time(s) since the theme was applied. If it happens \
                 again, pause the theme in CMD Studio so that cmd opens"
            ),
            details,
        );
    }
    check(
        "D12",
        Status::Fail,
        Some("E_CMD_CRASHED"),
        format!(
            "cmd แครชในส่วนเสริม {n} ครั้งตั้งแต่ใช้ธีม และธีมยังเปิดอยู่ ให้หยุดธีมใน CMD Studio เพื่อให้ cmd เปิดได้"
        ),
        format!(
            "cmd crashed in its add-on {n} times since the theme was applied, and the theme is \
             still on. Pause the theme in CMD Studio so that cmd opens"
        ),
        details,
    )
}

/// D12 while the theme is paused: who paused it, why and since when (recovery spec §4.5).
fn paused_check(p: &Paused) -> Check {
    let n = p.count;
    let (th, en) = match (p.by, p.reason) {
        (PausedBy::Clink, Some(PauseReason::CmdStarts)) => (
            format!(
                "ธีมถูกหยุดไว้ใน cmd เพราะ cmd เปิดไม่ถึงพร้อมท์ {n} ครั้งติดกัน เปิดธีมกลับได้ที่หน้าแรกของ CMD Studio"
            ),
            format!(
                "The theme is paused in cmd: cmd failed to reach its prompt {n} times in a row. \
                 Turn it back on from CMD Studio's home page"
            ),
        ),
        (PausedBy::App, Some(PauseReason::ClinkCrash)) => (
            format!(
                "CMD Studio หยุดธีมของเครื่องนี้ไว้ เพราะ cmd แครชในส่วนเสริม {n} ครั้ง เปิดธีมกลับได้ที่หน้าแรกของ CMD Studio"
            ),
            format!(
                "CMD Studio paused the theme on this computer: cmd crashed in its add-on {n} \
                 times. Turn it back on from CMD Studio's home page"
            ),
        ),
        (PausedBy::App, Some(PauseReason::Recovery)) => (
            "CMD Studio หยุดธีมของเครื่องนี้ไว้จากทางลัด CMD Studio Recovery เปิดธีมกลับได้ที่หน้าแรกของ CMD Studio"
                .to_string(),
            "CMD Studio paused the theme on this computer from the CMD Studio Recovery shortcut. \
             Turn it back on from CMD Studio's home page"
                .to_string(),
        ),
        (PausedBy::App, Some(PauseReason::User)) => (
            "ธีมของเครื่องนี้ถูกหยุดไว้ตามที่คุณสั่ง เปิดธีมกลับได้ที่หน้าแรกของ CMD Studio".to_string(),
            "The theme is paused on this computer, as you asked. Turn it back on from CMD \
             Studio's home page"
                .to_string(),
        ),
        _ => (
            "ธีมถูกหยุดไว้ เปิดธีมกลับได้ที่หน้าแรกของ CMD Studio".to_string(),
            "The theme is paused. Turn it back on from CMD Studio's home page".to_string(),
        ),
    };
    let level = match p.by {
        PausedBy::Clink => "cmd only (nothing on this computer was changed)",
        PausedBy::App => "this computer (CMD Studio took its changes out)",
    };
    let reason = p.reason.map_or(
        "not known (paused.json can't be read as a pause record)",
        PauseReason::as_str,
    );
    let mut details = vec![format!("paused: {level}"), format!("reason: {reason}")];
    if let Some(at) = p.at_ms {
        details.push(format!("since: {}", utc::iso(at)));
    }
    check(
        "D12",
        Status::Warn,
        Some("E_THEME_PAUSED"),
        th,
        en,
        details.join("\n"),
    )
}
```

- [ ] **Step 4: The two codes** — in the main spec's §5.9 table, after the row `| \`E_INTERNAL\` | … |`, add these two rows exactly:

```markdown
| `E_THEME_PAUSED` | ธีมถูกหยุดไว้ ใน cmd เพราะ cmd เปิดไม่ถึงพร้อมท์ซ้ำๆ หรือทั้งเครื่อง เพราะ cmd แครชในส่วนเสริมหรือผู้ใช้หยุดเอง เปิดธีมกลับได้ที่หน้าแรก (D12) | The theme is paused, in cmd because cmd failed to reach its prompt again and again, or on this computer because cmd crashed in its add-on or it was paused by hand; turn it back on from the home page (D12) |
| `E_CMD_CRASHED` | cmd แครชในส่วนเสริมของ cmd (Clink) ตั้งแต่ใช้ธีม ตามรายงานแครชของ Windows (D12) | cmd crashed in its add-on (Clink) since the theme was applied, as Windows' crash reports say (D12) |
```

and at the end of `MEANINGS` in `app/src-tauri/src/error.rs`, after the `E_INTERNAL` entry:

```rust
    (
        "E_THEME_PAUSED",
        r#"ธีมถูกหยุดไว้ ใน cmd เพราะ cmd เปิดไม่ถึงพร้อมท์ซ้ำๆ หรือทั้งเครื่อง เพราะ cmd แครชในส่วนเสริมหรือผู้ใช้หยุดเอง เปิดธีมกลับได้ที่หน้าแรก (D12)"#,
        r#"The theme is paused, in cmd because cmd failed to reach its prompt again and again, or on this computer because cmd crashed in its add-on or it was paused by hand; turn it back on from the home page (D12)"#,
    ),
    (
        "E_CMD_CRASHED",
        r#"cmd แครชในส่วนเสริมของ cmd (Clink) ตั้งแต่ใช้ธีม ตามรายงานแครชของ Windows (D12)"#,
        r#"cmd crashed in its add-on (Clink) since the theme was applied, as Windows' crash reports say (D12)"#,
    ),
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p cmdstudio-core` — every test passes (`lib` 750; `error_codes` 3). `cargo test -p cmdstudio-cli` — every test passes. `cargo test -p cmdstudio-app --lib -- error:: backend::doctor::` — every test passes (`the_meanings_are_exactly_the_rows_of_spec_5_9` reads the two new rows). Then `cargo fmt --all` and the four clippy commands: clean.

- [ ] **Step 6: See that the pause comes first** — in `d12`, move the `read_pause` match below the `applied(...)` match; `d12_says_a_pause_first_with_its_level_its_reason_and_since_when` FAILS (no theme applied: `skipped`, not `E_THEME_PAUSED`). Restore; it passes.

- [ ] **Step 7: Commit**

Stage `crates/core/src/doctor.rs docs/superpowers/specs/2026-10-10-cmd-studio-design.md app/src-tauri/src/error.rs crates/cli/src/lib.rs app/src-tauri/src/backend/doctor.rs`. Message: `core, spec: doctor D12, the theme's pause and cmd's crashes in Clink; E_THEME_PAUSED and E_CMD_CRASHED in §5.9`.

---

### Task 6: `--cleanup` knows `health\`, and the CLI's apply lifts the pause

Recovery spec §7: `--cleanup --data-dir` takes a data folder for CMD Studio's only when every entry in it is one CMD Studio puts there, so `health\` joins the list (the core's `remove_data` already deletes every entry it is given). And an apply from the command line is a theme applied like any other: its commit lifts the pause and its tags (Review Focus 5); one that cannot be lifted is said, and the apply stands.

**Files:**
- Modify: `crates/cli/src/lib.rs`

**Interfaces:**
- Consumes: Task 2's `recovery::clear_pause`, `Paths::health()`, `Paths::paused_file()`.
- Produces: `put_there_by_cmd_studio` takes a folder named `health` (any ASCII case) for CMD Studio's; `cmdstudio apply` deletes `health\paused.json` and every `health\start-*` after a commit and makes `health\`.

- [ ] **Step 1: Write the failing tests** — in `crates/cli/src/lib.rs`'s tests:
- in `cleanup_with_a_data_dir_takes_the_apps_own_entries_for_cmd_studios`, after `r"webview\EBWebView\Default\Cache\data_0",` add `r"health\start-4242",` and `r"health\paused.json",`;
- in `a_name_of_cmd_studios_with_the_wrong_kind_is_not_taken_for_it`, replace the comment `// The app's names too: a file called drafts, a folder called settings.json.` with `// The app's names too: a file called drafts, a folder called settings.json, and a` / `// file called health (the folder of the start tags and the pause).`, and after `(r"C:\x\d", r"C:\x\d\drafts", "\"drafts\""),` add `(r"C:\x\f", r"C:\x\f\health", "\"health\""),`;
- at the end of `mod tests`, before its closing `}`:

```rust
    /// The theme paused in cmd, as init.lua leaves it: `paused.json` and two start tags.
    fn paused_in_cmd(m: &FakeMachine, d: &Defaults) -> Paths {
        let paths = Paths {
            data: d.data_dir.clone(),
        };
        m.fs.put(
            paths.paused_file(),
            br#"{"version":1,"reason":"cmdStarts","at":1791694709000,"count":3,"by":"clink"}"#,
        );
        for pid in [11, 12] {
            m.fs.put(
                paths.health().join(format!("start-{pid}")),
                b"v1 1791694709\n",
            );
        }
        paths
    }

    #[test]
    fn an_apply_lifts_the_pause_and_every_tag() {
        // Review Focus 5: a theme applied from the command line is the theme on again; a pause
        // left behind would keep every cmd window plain under the new theme.
        let (m, d) = machine();
        let paths = paused_in_cmd(&m, &d);
        let (code, out, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!((code, err.as_str()), (EXIT_OK, ""), "{out}");
        assert!(!m.fs.exists(&paths.paused_file()));
        assert!(m.fs.list_dir(&paths.health()).unwrap().is_empty());
        assert!(
            m.fs.is_dir(&paths.health()),
            "init.lua writes its tags there"
        );
        // An apply that rolls back (here its check: Oh My Posh draws nothing) leaves the pause.
        let paths = paused_in_cmd(&m, &d);
        m.process.answer("oh-my-posh.exe", "print", 1, "");
        let (code, _, err) = cli(&m, &later(&d, 1), &["apply", THEME]);
        assert_eq!(code, EXIT_FAILED, "{err}");
        assert!(m.fs.exists(&paths.paused_file()));
    }

    #[test]
    fn a_pause_that_cannot_be_lifted_is_said_and_the_apply_stands() {
        let (m, d) = machine();
        let paths = paused_in_cmd(&m, &d);
        m.fs.fail_removes_of(paths.paused_file());
        let (code, out, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(out.contains("Applied \"Terracotta\""), "{out}");
        assert!(
            err.starts_with(
                "E_DATA_WRITE: the theme was applied, but its pause could not be lifted"
            ),
            "{err}"
        );
        assert!(err.is_ascii(), "English only in cmd: {err}");
        assert!(m.fs.exists(&paths.paused_file()));
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p cmdstudio-cli --lib`
Expected: FAIL: `cleanup_with_a_data_dir_takes_the_apps_own_entries_for_cmd_studios` (`"health\"` is not CMD Studio's), `an_apply_lifts_the_pause_and_every_tag` (paused.json still there), `a_pause_that_cannot_be_lifted_is_said_and_the_apply_stands` (nothing said).

- [ ] **Step 3: Write it**
- imports: after `use cmdstudio_core::plan::{…};` add `use cmdstudio_core::recovery::clear_pause;`;
- in `Say::apply`, replace its last line `self.finish(sys, ctx, &outcome, &ops, done)` with:

```rust
        let code = self.finish(sys, ctx, &outcome, &ops, done);
        // A theme applied is the theme on again (recovery spec §4.1): its pause goes, and with
        // it every start tag of the theme before. The apply stands either way.
        if outcome == Outcome::Committed {
            if let Err(e) = clear_pause(sys.fs, &ctx.paths) {
                self.err(format!(
                    "E_DATA_WRITE: the theme was applied, but its pause could not be lifted ({e}); \
                     cmd windows open without the theme until {} is deleted",
                    ctx.paths.paused_file().display()
                ));
            }
        }
        code
    }
```

(drop the closing `}` of that block when pasting: it is `apply`'s own);
- in `put_there_by_cmd_studio`'s list of folders, after `paths.webview(),` add `paths.health(),`; in its doc comment, `…and \`webview\\\` (WebView2's own data, put there by the app on purpose; spec §5.3);` becomes `…put there by the app on purpose; spec §5.3), and \`health\\\` (init.lua's start tags and the theme's pause, recovery spec §7);`, and `the drafts and the log, with their own temp files` becomes `the drafts, the log and the pause, with their own temp files`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cmdstudio-cli` — every test passes (`lib` 50). Then `cargo fmt --all` and the four clippy commands: clean.

- [ ] **Step 5: Commit**

Stage `crates/cli/src/lib.rs`. Message: `cli: --cleanup knows health\, and an apply lifts the theme's pause (recovery spec §4.1, §7)`.

---


### Task 7: The app's pause: status, pause, turn back on, and the start's crash check

Recovery spec §4.1–§4.3, §8. Two levels of pause: init.lua's in cmd (`by: clink`, nothing on the computer changed) and the app's on this computer (`by: app`: Revert everything, then `paused.json` noting the theme to apply again). "Turn the theme back on" lifts a pause in cmd at once, and for a pause of this computer hands the UI the apply request and its plan; that apply's commit lifts the pause, as every committed apply does (Review Focus 5). The start's check of Windows' crash reports runs once per start, never on the window's thread; three crashes since the apply pause the theme on this computer. "Undo everything" also lifts a pause in cmd (nothing is left for it to pause); a pause of this computer stays, because it holds the theme to turn back on.

Spike results this task depends on: S10 (F9) through Task 4's `clink_crashes`; nothing of its own.

**Files:**
- Create: `app/src-tauri/src/backend/recovery.rs` (its tests are in it: 17)
- Modify: `app/src-tauri/src/backend/mod.rs`, `app/src-tauri/src/backend/apply.rs`, `app/src-tauri/src/backend/doctor.rs`, `app/src-tauri/src/logs.rs`
- Generate: `app/ui/src/bindings/{CrashCheck,PauseInfo,PauseOutcome,RecoveryStatus,ResumeInfo,ResumeOutcome}.ts` (new), `FixKind.ts` (two values more)
- Modify: `app/ui/src/i18n/en.ts`, `app/ui/src/i18n/th.ts` (the names of the two new fixes: Help builds the key `help.doctor.fix.<kind>` from `FixKind`, so the UI's typecheck needs them in this commit)

**Interfaces:**
- Consumes: Task 2's `recovery::{clear_pause, read_pause, write_pause, PauseReason, PauseRecord, Paused, PausedBy, Resume, PAUSED_VERSION, PAUSE_AFTER}`, `utc::iso`, `Paths::{health, paused_file}`; Task 4's `eventlog::clink_crashes`, `FakeMachine.event_log`; Task 5's D12 and its codes.
- Produces (Task 8 wraps these, Tasks 9–10 read them through the bindings):
  - `backend::recovery::{CRASH_CHECK_WAIT: Duration = 30 s, ResumeInfo { theme_id: Uuid, target: Target, theme_name: Option<String> }, PauseInfo { by: PausedBy, reason: Option<PauseReason>, at_ms: Option<u64>, count: u32, resume: Option<ResumeInfo> }, CrashCheck::{NoTheme, Read { count: u32, paused_now: bool }, Unreadable { why: String }} (tag "kind"), RecoveryStatus { paused: Option<PauseInfo>, crashes: Option<CrashCheck> }, PauseOutcome { revert: Option<RevertOutcome>, refused: Option<AppError>, not_saved: Option<AppError> }, ResumeOutcome::{Resumed, Apply { request: ApplyRequest, plan: Box<PlanOutcome> }, PickTheme, NotPaused} (tag "kind")}`, all camelCase with `#[ts(export)]`;
  - `Backend::{recovery_status(&self) -> Result<RecoveryStatus, AppError>, theme_pause(&self, PauseReason) -> Result<PauseOutcome, AppError>` (only `user` and `clinkCrash`; the other two are `E_INTERNAL`), `theme_resume(&self) -> Result<ResumeOutcome, AppError>, crash_check_started(&self), crash_check_not_run(&self, why: &str), crash_check(&self) -> CrashCheck}`; `pub(crate) fn pause_machine(&self, PauseReason, count: u32) -> PauseOutcome` (3b-2's `--recovery` pauses through it with `recovery`);
  - `Backend::revert_outcome(&self, &Ctx, &cleanup::Report) -> RevertOutcome` (crate-visible), `Backend::note_line(&self, &str)`;
  - `FixKind::{Resume, Pause}` (`"resume"`, `"pause"`): D12 with `E_THEME_PAUSED` offers `resume`, with `E_CMD_CRASHED` `pause`;
  - `backend::testing::fake_events(&Backend<FakeMachine>) -> &FakeEventLog`;
  - `logs::iso_utc` is now `cmdstudio_core::utc::iso` (same text).

- [ ] **Step 1: Write `app/src-tauri/src/backend/recovery.rs`** (its tests: `a_pause_record_that_cannot_be_read_is_a_pause_in_cmd_and_turning_back_on_lifts_it` is Review Focus 3, `two_crashes_or_reports_that_cannot_be_read_pause_nothing` Review Focus 4, `an_apply_through_the_plan_lifts_any_pause` Review Focus 5)

```rust
//! The theme's pause, from the app (recovery spec §4.1–§4.3, §8): `recovery_status`,
//! `theme_pause` and `theme_resume`, and the check of Windows' crash reports that runs once at
//! every start ([`Backend::crash_check`], which `lib.rs` starts on a thread of its own once the
//! window is up, so the window never waits for it).
//!
//! Two levels of pause (spec §4.1): init.lua pauses the theme in cmd (`by: clink`) and changes
//! nothing on the computer; the app pauses it on this computer (`by: app`) through Revert
//! everything, and notes the theme to apply again. Turning the theme back on lifts a pause in cmd
//! at once (nothing on the computer changed, so no transaction), and for a pause of this computer
//! gives the UI the request to apply the paused theme again through the plan; that apply's
//! commit lifts the pause (`Backend::apply`).
use super::apply::{ApplyRequest, PlanOutcome, RevertOutcome};
use super::{Backend, Machine};
use crate::error::AppError;
use cmdstudio_core::cleanup;
use cmdstudio_core::eventlog::clink_crashes;
use cmdstudio_core::model::Target;
use cmdstudio_core::recovery::{
    clear_pause, read_pause, write_pause, PauseReason, PauseRecord, Paused, PausedBy, Resume,
    PAUSED_VERSION, PAUSE_AFTER,
};
use cmdstudio_core::state::State;
use serde::{Deserialize, Serialize};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;
use ts_rs::TS;
use uuid::Uuid;

/// How long `recovery_status` waits for the start's crash check (spec §4.3): the event log's
/// five seconds, a pause's Revert everything, and room to spare. Past it, the status says the
/// check has not finished, and the next read asks again.
pub const CRASH_CHECK_WAIT: Duration = Duration::from_secs(30);

/// The theme a pause of this computer applies again, and its name while it is still in the
/// library.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResumeInfo {
    pub theme_id: Uuid,
    pub target: Target,
    /// `None` when the theme is no longer in the library: turning back on goes to the Themes
    /// page instead (spec §4.1).
    pub theme_name: Option<String>,
}

/// The pause, as the home page's banner says it (spec §4.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PauseInfo {
    pub by: PausedBy,
    /// `None` when paused.json can't be read as a pause record: cmd still opens without the
    /// theme, and turning it back on deletes the file.
    pub reason: Option<PauseReason>,
    #[ts(type = "number | null")]
    pub at_ms: Option<u64>,
    pub count: u32,
    pub resume: Option<ResumeInfo>,
}

/// What the start's check of Windows' crash reports found (spec §4.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum CrashCheck {
    /// No theme is applied, or state.json can't be read: nothing to look for.
    NoTheme,
    /// `count` crashes of cmd in its add-on since the theme was applied; `paused_now` when this
    /// check paused the theme on this computer for them.
    Read { count: u32, paused_now: bool },
    /// The reports could not be read (the log is off, this user may not read it, or it did not
    /// answer in five seconds), so nothing was done.
    Unreadable { why: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecoveryStatus {
    /// `None` when the theme is not paused.
    pub paused: Option<PauseInfo>,
    /// `None` while the start's crash check has not finished, or did not run.
    pub crashes: Option<CrashCheck>,
}

/// What "Pause the theme" did. `paused.json` is written however the revert went (spec §4.1):
/// while it is there, init.lua loads no theme, and the home page says the state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PauseOutcome {
    /// What Revert everything did; `None` when it was refused before it changed anything.
    pub revert: Option<RevertOutcome>,
    /// Why Revert everything was refused (`E_BUSY`, `E_SNAPSHOT_READ`, `E_STATE_INVALID`).
    pub refused: Option<AppError>,
    /// `paused.json` could not be written (`E_DATA_WRITE`).
    pub not_saved: Option<AppError>,
}

/// What "Turn the theme back on" does (spec §4.1, §8).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ResumeOutcome {
    /// A pause in cmd was lifted: the next cmd window loads the theme.
    Resumed,
    /// A pause of this computer: apply the paused theme again, shown as this plan first.
    Apply {
        request: ApplyRequest,
        plan: Box<PlanOutcome>,
    },
    /// A pause of this computer whose theme is no longer in the library, or that noted none:
    /// choose one on the Themes page.
    PickTheme,
    /// The theme is not paused.
    NotPaused,
}

/// Where the start's crash check leaves its answer for `recovery_status`. A lock that a
/// panicking thread left is used as it is: the slot is only ever replaced whole.
pub(crate) struct CrashSlot {
    state: Mutex<Slot>,
    done: Condvar,
}

#[derive(Clone)]
enum Slot {
    NotStarted,
    Running,
    Done(CrashCheck),
}

impl CrashSlot {
    pub(crate) fn new() -> Self {
        CrashSlot {
            state: Mutex::new(Slot::NotStarted),
            done: Condvar::new(),
        }
    }

    fn start(&self) {
        let mut slot = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*slot, Slot::NotStarted) {
            *slot = Slot::Running;
        }
    }

    fn finish(&self, check: CrashCheck) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = Slot::Done(check);
        self.done.notify_all();
    }

    /// The answer, waiting up to `timeout` for a check on its way; at once `None` when none was
    /// started.
    fn wait(&self, timeout: Duration) -> Option<CrashCheck> {
        let slot = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let (slot, _) = self
            .done
            .wait_timeout_while(slot, timeout, |s| matches!(s, Slot::Running))
            .unwrap_or_else(PoisonError::into_inner);
        match &*slot {
            Slot::Done(check) => Some(check.clone()),
            Slot::NotStarted | Slot::Running => None,
        }
    }
}

impl<M: Machine> Backend<M> {
    /// Whether the theme is paused, and what the start's crash check found (spec §8). Changes
    /// nothing; waits up to [`CRASH_CHECK_WAIT`] for that check while it runs.
    pub fn recovery_status(&self) -> Result<RecoveryStatus, AppError> {
        let crashes = self.crash.wait(CRASH_CHECK_WAIT);
        Ok(RecoveryStatus {
            paused: self.pause_now()?,
            crashes,
        })
    }

    fn pause_now(&self) -> Result<Option<PauseInfo>, AppError> {
        let paused = read_pause(self.sys().fs, self.paths())
            .map_err(|e| AppError::coded("E_MACHINE_READ", e.to_string()))?;
        Ok(paused.map(|p| self.pause_info(p)))
    }

    fn pause_info(&self, p: Paused) -> PauseInfo {
        let resume = p.resume.map(|r| ResumeInfo {
            theme_id: r.theme_id,
            target: r.target,
            theme_name: self
                .find_theme(r.theme_id)
                .ok()
                .map(|(theme, _, _)| theme.name),
        });
        PauseInfo {
            by: p.by,
            reason: p.reason,
            at_ms: p.at_ms,
            count: p.count,
            resume,
        }
    }

    /// Marks the start's crash check as on its way, so that `recovery_status` waits for it.
    /// `lib.rs` calls this before it starts [`Backend::crash_check`] on a thread of its own.
    pub fn crash_check_started(&self) {
        self.crash.start();
    }

    /// The start's crash check could not run (no thread could be started): `recovery_status`
    /// must not wait for it.
    pub fn crash_check_not_run(&self, why: &str) {
        self.note_line("crash_check not_run");
        self.crash.finish(CrashCheck::Unreadable {
            why: why.to_string(),
        });
    }

    /// The start's check of Windows' crash reports (spec §4.3): cmd's crashes in Clink's DLL since
    /// the theme was applied, at most seven days back. [`PAUSE_AFTER`] or more, with a theme
    /// applied and no pause of this computer yet, pause it on this computer
    /// ([`PauseReason::ClinkCrash`]); a pause in cmd does not count, because Clink still injects
    /// and its DLL still loads. Reports that can't be read pause nothing. One log line, never the
    /// user's data. A panic in it is an `Unreadable` answer, never a `recovery_status` that waits
    /// for nothing.
    pub fn crash_check(&self) -> CrashCheck {
        self.crash.start();
        // UNWIND SAFETY: the check only reads, and a pause it starts is a journaled
        // transaction, which the next command that changes the machine rolls back if it was cut
        // off; nothing of the backend's memory is left half changed.
        let check = match catch_unwind(AssertUnwindSafe(|| self.run_crash_check())) {
            Ok(check) => check,
            Err(payload) => {
                // Dropping the payload of a panic runs its code, which could panic too.
                std::mem::forget(payload);
                CrashCheck::Unreadable {
                    why: "the check stopped on an internal error (a panic)".to_string(),
                }
            }
        };
        self.crash.finish(check.clone());
        check
    }

    fn run_crash_check(&self) -> CrashCheck {
        let sys = self.sys();
        let ctx = self.ctx();
        let Ok(State {
            applied: Some(applied),
            ..
        }) = State::load(sys.fs, &ctx.paths.state_file())
        else {
            self.note_line("crash_check theme=none");
            return CrashCheck::NoTheme;
        };
        let crashes = match clink_crashes(sys.event_log, applied.applied_at_ms, ctx.now_ms) {
            Ok(crashes) => crashes,
            Err(e) => {
                // Never a pause for reports that can't be read (spec §4.3).
                self.note_line("crash_check unreadable");
                return CrashCheck::Unreadable { why: e.to_string() };
            }
        };
        let count = u32::try_from(crashes.len()).unwrap_or(u32::MAX);
        let paused_here = matches!(
            read_pause(sys.fs, &ctx.paths),
            Ok(Some(Paused {
                by: PausedBy::App,
                ..
            }))
        );
        let paused_now = count >= PAUSE_AFTER
            && !paused_here
            && self
                .pause_machine(PauseReason::ClinkCrash, count)
                .not_saved
                .is_none();
        self.note_line(&format!("crash_check crashes={count} paused={paused_now}"));
        CrashCheck::Read { count, paused_now }
    }

    /// "Pause the theme" (spec §4.2, D12's fix): on this computer, for the user (`user`) or for
    /// cmd's crashes in its add-on (`clinkCrash`, with their count). The other reasons are not
    /// the app's to give: `cmdStarts` is init.lua's, `recovery` the Recovery shortcut's (phase
    /// 3b-2), and asking for them is `E_INTERNAL`, with nothing changed.
    pub fn theme_pause(&self, reason: PauseReason) -> Result<PauseOutcome, AppError> {
        match reason {
            PauseReason::User => Ok(self.pause_machine(PauseReason::User, 0)),
            PauseReason::ClinkCrash => {
                let count = self.crashes_now();
                Ok(self.pause_machine(PauseReason::ClinkCrash, count))
            }
            PauseReason::CmdStarts | PauseReason::Recovery => Err(AppError::internal(format!(
                "the app does not pause the theme for {}",
                reason.as_str()
            ))),
        }
    }

    /// cmd's crashes in its add-on since the theme was applied, as D12 counts them; 0 when they
    /// can't be read or no theme is applied.
    fn crashes_now(&self) -> u32 {
        let sys = self.sys();
        match State::load(sys.fs, &self.paths().state_file()) {
            Ok(State {
                applied: Some(a), ..
            }) => clink_crashes(sys.event_log, a.applied_at_ms, self.now_ms())
                .map_or(0, |c| u32::try_from(c.len()).unwrap_or(u32::MAX)),
            _ => 0,
        }
    }

    /// Pauses the theme on this computer (spec §4.1): Revert everything (a journaled
    /// transaction, under the lock), then `paused.json` with the theme to apply again: the one
    /// applied now, else the one an earlier pause of this computer noted. Written however the
    /// revert went. Phase 3b-2's `--recovery` pauses through here too (`recovery`).
    pub(crate) fn pause_machine(&self, reason: PauseReason, count: u32) -> PauseOutcome {
        let sys = self.sys();
        let ctx = self.ctx();
        let applied = match State::load(sys.fs, &ctx.paths.state_file()) {
            Ok(State {
                applied: Some(a), ..
            }) => Some(Resume {
                theme_id: a.theme_id,
                target: a.target,
            }),
            _ => None,
        };
        let earlier = match read_pause(sys.fs, &ctx.paths) {
            Ok(Some(Paused {
                by: PausedBy::App,
                resume,
                ..
            })) => resume,
            _ => None,
        };
        let (revert, refused) = match cleanup::revert_all(sys, &ctx) {
            Ok(report) => (Some(self.revert_outcome(&ctx, &report)), None),
            Err(e) => (None, Some(AppError::from(e))),
        };
        let record = PauseRecord {
            version: PAUSED_VERSION,
            reason,
            at: ctx.now_ms,
            count,
            by: PausedBy::App,
            resume: applied.or(earlier),
        };
        let not_saved = write_pause(sys.fs, &ctx.paths, &record)
            .err()
            .map(|e| AppError::coded("E_DATA_WRITE", e.to_string()));
        let reverted = match (&revert, &refused) {
            (Some(r), _) if r.ok => "ok",
            (Some(_), _) => "incomplete",
            (None, Some(e)) => e.code.as_str(),
            (None, None) => "none",
        };
        self.note_line(&format!(
            "pause reason={} count={count} revert={reverted} saved={}",
            reason.as_str(),
            not_saved.is_none()
        ));
        PauseOutcome {
            revert,
            refused,
            not_saved,
        }
    }

    /// "Turn the theme back on" (spec §4.1, §8): a pause in cmd is lifted at once; for a pause
    /// of this computer the request (the paused theme, its target, the settings' choice for
    /// Windows Terminal's default profile) and its plan, which the UI shows before `apply`.
    pub fn theme_resume(&self) -> Result<ResumeOutcome, AppError> {
        let fs = self.sys().fs;
        let paused = read_pause(fs, self.paths())
            .map_err(|e| AppError::coded("E_MACHINE_READ", e.to_string()))?;
        let Some(p) = paused else {
            return Ok(ResumeOutcome::NotPaused);
        };
        match (p.by, p.resume) {
            (PausedBy::Clink, _) => {
                clear_pause(fs, self.paths())
                    .map_err(|e| AppError::coded("E_DATA_WRITE", e.to_string()))?;
                self.note_line("resume by=clink");
                Ok(ResumeOutcome::Resumed)
            }
            (PausedBy::App, Some(r)) if self.find_theme(r.theme_id).is_ok() => {
                let request = ApplyRequest {
                    theme_id: r.theme_id,
                    target: r.target,
                    set_wt_default_profile: self.settings_or_default().set_wt_default_profile,
                    replace_other_clink: false,
                };
                let plan = self.plan_apply(request.clone())?;
                Ok(ResumeOutcome::Apply {
                    request,
                    plan: Box::new(plan),
                })
            }
            (PausedBy::App, _) => Ok(ResumeOutcome::PickTheme),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::apply::{ApplyStatus, TxnStatus};
    use crate::backend::doctor::FixKind;
    use crate::backend::testing::{
        apply_terracotta, backend, fake_events, fake_fs, plan_and_apply, terracotta_id,
    };
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::adapters::Fs;
    use cmdstudio_core::doctor::Status;
    use std::sync::Arc;

    const CLINK_DLL: &str = "clink_dll_x64.dll";
    /// What init.lua writes when it pauses the theme in cmd.
    const IN_CMD: &[u8] =
        br#"{"version":1,"reason":"cmdStarts","at":1791694709000,"count":3,"by":"clink"}"#;

    fn applied_at(b: &Backend<FakeMachine>) -> u64 {
        State::load(b.sys().fs, &b.paths().state_file())
            .unwrap()
            .applied
            .unwrap()
            .applied_at_ms
    }

    /// Terracotta applied, then the theme paused in cmd by init.lua, with two tags left.
    fn paused_in_cmd(b: &Backend<FakeMachine>) {
        assert_eq!(apply_terracotta(b).0.status, ApplyStatus::Committed);
        fake_fs(b).put(b.paths().paused_file(), IN_CMD);
        for pid in [11, 12] {
            fake_fs(b).put(
                b.paths().health().join(format!("start-{pid}")),
                b"v1 1791694709\n",
            );
        }
    }

    #[test]
    fn no_pause_and_no_check_yet_is_said_as_such_at_once() {
        let b = backend();
        let started = std::time::Instant::now();
        assert_eq!(
            b.recovery_status().unwrap(),
            RecoveryStatus {
                paused: None,
                crashes: None
            }
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "no wait for a check never started"
        );
        assert_eq!(b.theme_resume().unwrap(), ResumeOutcome::NotPaused);
    }

    #[test]
    fn a_pause_in_cmd_is_read_and_lifted_at_once() {
        let b = backend();
        paused_in_cmd(&b);
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(
            paused,
            PauseInfo {
                by: PausedBy::Clink,
                reason: Some(PauseReason::CmdStarts),
                at_ms: Some(1_791_694_709_000),
                count: 3,
                resume: None,
            }
        );
        let autorun = b.sys().autorun.read_user().unwrap();
        assert_eq!(b.theme_resume().unwrap(), ResumeOutcome::Resumed);
        assert!(!fake_fs(&b).exists(&b.paths().paused_file()));
        assert!(b.sys().fs.list_dir(&b.paths().health()).unwrap().is_empty());
        // Nothing on the computer changed: no transaction, AutoRun as it was.
        assert_eq!(b.sys().autorun.read_user().unwrap(), autorun);
        assert_eq!(b.recovery_status().unwrap().paused, None);
    }

    #[test]
    fn pausing_takes_the_theme_out_and_turning_it_back_on_applies_it_through_the_plan() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        let out = b.theme_pause(PauseReason::User).unwrap();
        let revert = out.revert.unwrap();
        assert!(revert.ok, "{revert:?}");
        assert_eq!(revert.status, Some(TxnStatus::Committed));
        assert_eq!((out.refused, out.not_saved), (None, None));
        assert_eq!(
            b.sys().autorun.read_user().unwrap(),
            None,
            "CMD Studio's part is out"
        );
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(
            (paused.by, paused.reason),
            (PausedBy::App, Some(PauseReason::User))
        );
        assert_eq!(
            paused.resume,
            Some(ResumeInfo {
                theme_id: terracotta_id(),
                target: Target::All,
                theme_name: Some("Terracotta".to_string()),
            })
        );
        let ResumeOutcome::Apply { request, plan } = b.theme_resume().unwrap() else {
            panic!("a pause of this computer applies the theme again");
        };
        assert_eq!(
            (
                request.theme_id,
                request.target,
                request.replace_other_clink
            ),
            (terracotta_id(), Target::All, false)
        );
        assert!(matches!(*plan, PlanOutcome::Ready { .. }));
        // The pause stays until that apply commits.
        assert!(fake_fs(&b).exists(&b.paths().paused_file()));
        assert_eq!(plan_and_apply(&b, request).0.status, ApplyStatus::Committed);
        assert_eq!(b.recovery_status().unwrap().paused, None);
        let log = b.log_tail(20);
        assert!(
            log.contains(" pause reason=user count=0 revert=ok saved=true"),
            "{log}"
        );
    }

    #[test]
    fn a_paused_theme_no_longer_in_the_library_sends_the_user_to_the_themes_page() {
        let b = backend();
        let gone = PauseRecord {
            version: PAUSED_VERSION,
            reason: PauseReason::ClinkCrash,
            at: 1,
            count: 3,
            by: PausedBy::App,
            resume: Some(Resume {
                theme_id: Uuid::nil(),
                target: Target::Wt,
            }),
        };
        write_pause(b.sys().fs, b.paths(), &gone).unwrap();
        let resume = b.recovery_status().unwrap().paused.unwrap().resume.unwrap();
        assert_eq!(resume.theme_name, None);
        assert_eq!(b.theme_resume().unwrap(), ResumeOutcome::PickTheme);
        // And one that noted no theme at all.
        let none = PauseRecord {
            resume: None,
            ..gone
        };
        write_pause(b.sys().fs, b.paths(), &none).unwrap();
        assert_eq!(b.theme_resume().unwrap(), ResumeOutcome::PickTheme);
    }

    #[test]
    fn a_pause_record_that_cannot_be_read_is_a_pause_in_cmd_and_turning_back_on_lifts_it() {
        // Review Focus 3: init.lua loads no theme while the file is there, whatever it holds.
        let b = backend();
        fake_fs(&b).put(b.paths().paused_file(), b"{ not json");
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!((paused.by, paused.reason), (PausedBy::Clink, None));
        assert_eq!(b.theme_resume().unwrap(), ResumeOutcome::Resumed);
        assert!(!fake_fs(&b).exists(&b.paths().paused_file()));
        // A file that can't be read at all is an error, never "not paused".
        fake_fs(&b).put(b.paths().paused_file(), IN_CMD);
        fake_fs(&b).fail_reads_of(b.paths().paused_file());
        assert_eq!(b.recovery_status().unwrap_err().code, "E_MACHINE_READ");
        assert_eq!(b.theme_resume().unwrap_err().code, "E_MACHINE_READ");
    }

    #[test]
    fn the_app_pauses_only_for_the_user_and_for_cmds_crashes() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        let before = fake_fs(&b).snapshot();
        for reason in [PauseReason::CmdStarts, PauseReason::Recovery] {
            assert_eq!(b.theme_pause(reason).unwrap_err().code, "E_INTERNAL");
        }
        assert_eq!(fake_fs(&b).snapshot(), before);
    }

    #[test]
    fn a_pause_whose_revert_is_refused_is_written_all_the_same() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        let held = fake_fs(&b)
            .try_lock(&b.paths().lock_file())
            .unwrap()
            .unwrap();
        let out = b.theme_pause(PauseReason::User).unwrap();
        drop(held);
        assert_eq!(out.revert, None);
        assert_eq!(out.refused.unwrap().code, "E_BUSY");
        assert_eq!(out.not_saved, None);
        // cmd loads no theme while it is there; the theme is still applied, to apply again.
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(paused.by, PausedBy::App);
        assert_eq!(paused.resume.unwrap().theme_id, terracotta_id());
    }

    #[test]
    fn a_pause_that_cannot_be_written_is_said() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        fake_fs(&b).fail_writes_to(b.paths().paused_file());
        let out = b.theme_pause(PauseReason::User).unwrap();
        assert!(out.revert.unwrap().ok);
        assert_eq!(out.not_saved.unwrap().code, "E_DATA_WRITE");
    }

    #[test]
    fn three_crashes_since_the_apply_pause_the_theme_at_start() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        let at = applied_at(&b);
        for _ in 0..3 {
            fake_events(&b).crash(at, "cmd.exe", CLINK_DLL);
        }
        assert_eq!(
            b.crash_check(),
            CrashCheck::Read {
                count: 3,
                paused_now: true
            }
        );
        let status = b.recovery_status().unwrap();
        let paused = status.paused.unwrap();
        assert_eq!(
            (paused.by, paused.reason, paused.count),
            (PausedBy::App, Some(PauseReason::ClinkCrash), 3)
        );
        assert_eq!(
            status.crashes,
            Some(CrashCheck::Read {
                count: 3,
                paused_now: true
            })
        );
        assert_eq!(b.sys().autorun.read_user().unwrap(), None);
        let log = b.log_tail(20);
        assert!(log.contains(" crash_check crashes=3 paused=true"), "{log}");
        // The next start finds the theme out: nothing to count from, nothing paused again.
        assert_eq!(b.crash_check(), CrashCheck::NoTheme);
    }

    #[test]
    fn two_crashes_or_reports_that_cannot_be_read_pause_nothing() {
        // Review Focus 4.
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        let at = applied_at(&b);
        for _ in 0..2 {
            fake_events(&b).crash(at, "cmd.exe", CLINK_DLL);
        }
        assert_eq!(
            b.crash_check(),
            CrashCheck::Read {
                count: 2,
                paused_now: false
            }
        );
        fake_events(&b).fail_reads("the event log service is off");
        let check = b.crash_check();
        assert!(
            matches!(&check, CrashCheck::Unreadable { why } if why.contains("service is off")),
            "{check:?}"
        );
        assert_eq!(b.recovery_status().unwrap().paused, None);
        assert!(
            b.sys().autorun.read_user().unwrap().is_some(),
            "the theme is still on"
        );
        assert!(b.log_tail(20).contains(" crash_check unreadable"));
    }

    #[test]
    fn no_theme_is_nothing_to_check() {
        let b = backend();
        assert_eq!(b.crash_check(), CrashCheck::NoTheme);
        assert!(fake_events(&b).asked().is_empty());
    }

    #[test]
    fn a_pause_in_cmd_is_written_over_by_a_pause_of_this_computer() {
        // Spec §4.3: Clink still injects while the theme is paused in cmd, and its DLL crashes
        // before any script runs.
        let b = backend();
        paused_in_cmd(&b);
        let at = applied_at(&b);
        for _ in 0..3 {
            fake_events(&b).crash(at, "cmd.exe", CLINK_DLL);
        }
        assert_eq!(
            b.crash_check(),
            CrashCheck::Read {
                count: 3,
                paused_now: true
            }
        );
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().by,
            PausedBy::App
        );
    }

    #[test]
    fn an_apply_through_the_plan_lifts_any_pause() {
        // Review Focus 5.
        let b = backend();
        paused_in_cmd(&b);
        let (done, _) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::Committed);
        assert!(!fake_fs(&b).exists(&b.paths().paused_file()));
        assert!(b.sys().fs.list_dir(&b.paths().health()).unwrap().is_empty());
        // One that can't be lifted is said, and the apply stands.
        fake_fs(&b).put(b.paths().paused_file(), IN_CMD);
        fake_fs(&b).fail_removes_of(b.paths().paused_file());
        let (done, _) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::Committed);
        assert!(
            done.notes.iter().any(|n| n.code == "E_DATA_WRITE"
                && n.details
                    .starts_with("the theme was applied, but its pause could not be lifted")),
            "{:?}",
            done.notes
        );
    }

    #[test]
    fn undo_everything_lifts_a_pause_in_cmd_but_not_one_of_this_computer() {
        let b = backend();
        paused_in_cmd(&b);
        assert!(b.revert_all().unwrap().ok);
        assert_eq!(b.recovery_status().unwrap().paused, None);
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        b.theme_pause(PauseReason::User).unwrap();
        assert!(b.revert_all().unwrap().ok);
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().by,
            PausedBy::App,
            "it holds the theme to turn back on"
        );
    }

    #[test]
    fn the_health_check_offers_to_turn_back_on_or_to_pause() {
        let b = backend();
        paused_in_cmd(&b);
        let d12 = |b: &Backend<FakeMachine>| {
            b.doctor_run()
                .unwrap()
                .checks
                .into_iter()
                .find(|c| c.id == "D12")
                .unwrap()
        };
        let check = d12(&b);
        assert_eq!(
            (check.status, check.code.as_deref(), check.fix),
            (Status::Warn, Some("E_THEME_PAUSED"), Some(FixKind::Resume))
        );
        assert_eq!(b.doctor_fix("D12").unwrap().kind, Some(FixKind::Resume));
        b.theme_resume().unwrap();
        let at = applied_at(&b);
        fake_events(&b).crash(at, "cmd.exe", CLINK_DLL);
        let check = d12(&b);
        assert_eq!(
            (check.status, check.code.as_deref(), check.fix),
            (Status::Warn, Some("E_CMD_CRASHED"), Some(FixKind::Pause))
        );
        // The D12 fix pauses for cmd's crashes, with their count.
        b.theme_pause(PauseReason::ClinkCrash).unwrap();
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(
            (paused.reason, paused.count),
            (Some(PauseReason::ClinkCrash), 1)
        );
    }

    #[test]
    fn the_slot_waits_for_a_check_on_its_way_and_never_for_one_not_started() {
        let slot = Arc::new(CrashSlot::new());
        assert_eq!(slot.wait(Duration::from_secs(5)), None);
        slot.start();
        assert_eq!(slot.wait(Duration::from_millis(20)), None, "still running");
        let finisher = slot.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            finisher.finish(CrashCheck::NoTheme);
        });
        assert_eq!(slot.wait(Duration::from_secs(5)), Some(CrashCheck::NoTheme));
        thread.join().unwrap();
        // Done stays done.
        slot.start();
        assert_eq!(slot.wait(Duration::ZERO), Some(CrashCheck::NoTheme));
    }

    #[test]
    fn a_check_that_could_not_start_is_not_waited_for() {
        let b = backend();
        b.crash_check_started();
        b.crash_check_not_run("the check could not start: out of threads");
        let started = std::time::Instant::now();
        let status = b.recovery_status().unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(matches!(
            status.crashes,
            Some(CrashCheck::Unreadable { .. })
        ));
    }
}
```

- [ ] **Step 2: Register it, and the backend's slot and log line** — in `app/src-tauri/src/backend/mod.rs`:
- after `pub mod preview;` add `pub mod recovery;`;
- in `struct Backend`, after the field `sample: SampleIdentity,` (and its doc comment) add

```rust
    /// Where the start's check of Windows' crash reports leaves its answer for
    /// `recovery_status` (recovery spec §4.3).
    crash: recovery::CrashSlot,
```

- in `Backend::new`, after `sample,` add `crash: recovery::CrashSlot::new(),`;
- right before `/// The last lines of the log, for the Advanced page and the diagnostics.` add

```rust
    /// Writes one line of the app's own, not a command's, to the log: the start's crash check, a
    /// pause (recovery spec §4.3). As with `logged`, names and codes only, never the user's data;
    /// a line that cannot be written is dropped.
    pub(crate) fn note_line(&self, line: &str) {
        let _ = self.log.write(self.sys().fs, self.now_ms(), line);
    }

```

- in `mod testing`, the import `use cmdstudio_core::adapters::fake::{FakeFonts, FakeFs, FakeMachine};` becomes `use cmdstudio_core::adapters::fake::{FakeEventLog, FakeFonts, FakeFs, FakeMachine};`, and right before `/// The id of the Terracotta preset.` add

```rust
    /// The fake crash reports under the backend (see [`fake_fs`]).
    pub(crate) fn fake_events(b: &Backend<FakeMachine>) -> &FakeEventLog {
        &b.machine.event_log
    }

```

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p cmdstudio-app --lib -- backend::recovery::`
Expected: compile errors: no method `revert_outcome` on `Backend`; no variant `Resume`/`Pause` in `FixKind`.

- [ ] **Step 4: Every committed apply lifts the pause; "Undo everything" lifts one in cmd** — in `app/src-tauri/src/backend/apply.rs`:
- after `use cmdstudio_core::plan::{…};` add `use cmdstudio_core::recovery::{clear_pause, read_pause, Paused, PausedBy};`;
- in `Backend::apply`, between `unrestored.extend(result.unrestored);` and `Ok(ApplyOutcome {`, add

```rust
        // A theme applied is the theme on again (recovery spec §4.1): its pause goes, and with
        // it every start tag of the theme before. The apply stands either way.
        if result.status == TxnStatus::Committed {
            if let Err(e) = clear_pause(self.sys().fs, &ctx.paths) {
                notes.push(note(
                    "E_DATA_WRITE",
                    format!(
                        "the theme was applied, but its pause could not be lifted ({e}): cmd \
                         windows open without it until it is turned back on from the home page"
                    ),
                ));
            }
        }
```

- replace the whole of `pub fn revert_all(&self) -> Result<RevertOutcome, AppError> { … }` (its doc comment stays) with

```rust
    pub fn revert_all(&self) -> Result<RevertOutcome, AppError> {
        let ctx = self.ctx();
        let report = cleanup::revert_all(self.sys(), &ctx)?;
        let mut out = self.revert_outcome(&ctx, &report);
        // Everything CMD Studio changed is out, so a pause in cmd has nothing left to pause (the
        // banner's "Undo everything", recovery spec §4.2). A pause of this computer stays: it
        // holds the theme to turn back on.
        if out.ok {
            if let Ok(Some(Paused {
                by: PausedBy::Clink,
                ..
            })) = read_pause(self.sys().fs, &ctx.paths)
            {
                if let Err(e) = clear_pause(self.sys().fs, &ctx.paths) {
                    out.notes.push(note(
                        "E_DATA_WRITE",
                        format!(
                            "everything was undone, but the pause in cmd could not be lifted ({e})"
                        ),
                    ));
                }
            }
        }
        Ok(out)
    }

    /// What the core's Revert everything did, for the UI: each step, what its start found, and
    /// how its transaction ended. `revert_all` and the theme's pause (`recovery`) say it alike.
    pub(crate) fn revert_outcome(&self, ctx: &Ctx, report: &cleanup::Report) -> RevertOutcome {
        let mut notes = start_notes(&report.start);
        let mut unrestored = start_unrestored(&report.start);
        let steps = report.steps.iter().map(step_report).collect();
        let (status, error) = match &report.outcome {
            None => (None, None),
            Some(outcome) => {
                let result = self.transaction_result(outcome, ctx);
                notes.extend(result.notes);
                unrestored.extend(result.unrestored);
                (Some(result.status), result.error)
            }
        };
        RevertOutcome {
            ok: report.ok(),
            steps,
            notes,
            status,
            error,
            unrestored,
        }
    }
```

- [ ] **Step 5: D12's two fixes** — in `app/src-tauri/src/backend/doctor.rs`:
- in the module doc's list of fixes, after the line `//!   folder that is there, nothing but D1's own hand step;` add `//! - the theme paused (D12): turn it back on; cmd crashed in its add-on (D12): pause the theme;`;
- `FixKind`'s doc comment becomes

```rust
/// What the app offers for a finding. `reapply`, `replaceOtherClink`, `installWindowsTerminal`,
/// `openNewCmdWindow`, `resume` and `pause` are buttons; `reinstall` is a step only the user can
/// take, shown as words. A Windows Terminal that was never opened needs no step of the user's:
/// an apply sets it up (founder 2026-10-17), so D6 offers `reapply` for it once a theme is
/// applied.
```

- after the variant `Reinstall,` add

```rust
    /// Turn the theme back on (D12 while it is paused): a pause in cmd is lifted at once, one of
    /// this computer applies the paused theme again through the plan (`theme_resume`).
    Resume,
    /// Pause the theme on this computer (D12 when cmd crashed in its add-on), after the user's
    /// yes (`theme_pause` with `clinkCrash`).
    Pause,
```

- in the `match` that picks the fix, after the `("D5", _) => …` arm add

```rust
        // The theme's pause and cmd's crashes in its add-on (recovery spec §4.5).
        ("D12", "E_THEME_PAUSED") => Some(FixKind::Resume),
        ("D12", "E_CMD_CRASHED") => Some(FixKind::Pause),
```

- [ ] **Step 6: One UTC text** — in `app/src-tauri/src/logs.rs`, replace the function `pub fn iso_utc(ms: u64) -> String { … }` and its doc comment (`/// \`YYYY-MM-DDTHH:MM:SSZ\` from milliseconds since 1970 (the civil-from-days algorithm).`) with

```rust
/// `YYYY-MM-DDTHH:MM:SSZ` from milliseconds since 1970: the core's, which doctor D12 writes too.
pub use cmdstudio_core::utc::iso as iso_utc;
```

(`logs.rs`'s own tests of `iso_utc` stay and pass: the core's function is the same algorithm.)

- [ ] **Step 7: Run the tests**

Run: `cargo test -p cmdstudio-app --lib -- backend::recovery::`
Expected: `17 passed`.
Run: `cargo test -p cmdstudio-app --lib`
Expected: every test passes (210). Then `cargo fmt --all` and the four clippy commands: clean.

- [ ] **Step 8: See Review Focus 5's test bite** — delete the `if result.status == TxnStatus::Committed { … }` block of Step 4 from `Backend::apply`; `cargo test -p cmdstudio-app --lib -- an_apply_through_the_plan_lifts_any_pause` FAILS on `assert!(!fake_fs(&b).exists(&b.paths().paused_file()))`. Restore it; it passes.

- [ ] **Step 9: Export the bindings**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\export-bindings.ps1`, then `git status --porcelain -- app/ui/src/bindings`.
Expected: exactly ` M app/ui/src/bindings/FixKind.ts` and six new files (`??`): `CrashCheck.ts`, `PauseInfo.ts`, `PauseOutcome.ts`, `RecoveryStatus.ts`, `ResumeInfo.ts`, `ResumeOutcome.ts`. If the app crate's test exe is blocked (WDAC, Global Constraints), run the script with `-NoTauri` and take the app crate's files from the check crate of `docs/superpowers/plans/2026-10-16-phase2b-shell.md` appendix B, run with `TS_RS_EXPORT_DIR` set to `app\ui\src\bindings`. Either way the seven files are exactly these:

`FixKind.ts`:

```ts
// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.

/**
 * What the app offers for a finding. `reapply`, `replaceOtherClink`, `installWindowsTerminal`,
 * `openNewCmdWindow`, `resume` and `pause` are buttons; `reinstall` is a step only the user can
 * take, shown as words. A Windows Terminal that was never opened needs no step of the user's:
 * an apply sets it up (founder 2026-10-17), so D6 offers `reapply` for it once a theme is
 * applied.
 */
export type FixKind = "reapply" | "replaceOtherClink" | "installWindowsTerminal" | "openNewCmdWindow" | "reinstall" | "resume" | "pause";
```

`CrashCheck.ts`:

```ts
// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.

/**
 * What the start's check of Windows' crash reports found (spec §4.3).
 */
export type CrashCheck = { "kind": "noTheme" } | { "kind": "read", count: number, pausedNow: boolean, } | { "kind": "unreadable", why: string, };
```

`PauseInfo.ts`:

```ts
// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { PauseReason } from "./PauseReason";
import type { PausedBy } from "./PausedBy";
import type { ResumeInfo } from "./ResumeInfo";

/**
 * The pause, as the home page's banner says it (spec §4.2).
 */
export type PauseInfo = { by: PausedBy, 
/**
 * `None` when paused.json can't be read as a pause record: cmd still opens without the
 * theme, and turning it back on deletes the file.
 */
reason: PauseReason | null, atMs: number | null, count: number, resume: ResumeInfo | null, };
```

`PauseOutcome.ts`:

```ts
// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { AppError } from "./AppError";
import type { RevertOutcome } from "./RevertOutcome";

/**
 * What "Pause the theme" did. `paused.json` is written however the revert went (spec §4.1):
 * while it is there, init.lua loads no theme, and the home page says the state.
 */
export type PauseOutcome = { 
/**
 * What Revert everything did; `None` when it was refused before it changed anything.
 */
revert: RevertOutcome | null, 
/**
 * Why Revert everything was refused (`E_BUSY`, `E_SNAPSHOT_READ`, `E_STATE_INVALID`).
 */
refused: AppError | null, 
/**
 * `paused.json` could not be written (`E_DATA_WRITE`).
 */
notSaved: AppError | null, };
```

`RecoveryStatus.ts`:

```ts
// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { CrashCheck } from "./CrashCheck";
import type { PauseInfo } from "./PauseInfo";

export type RecoveryStatus = { 
/**
 * `None` when the theme is not paused.
 */
paused: PauseInfo | null, 
/**
 * `None` while the start's crash check has not finished, or did not run.
 */
crashes: CrashCheck | null, };
```

`ResumeInfo.ts`:

```ts
// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { Target } from "./Target";

/**
 * The theme a pause of this computer applies again, and its name while it is still in the
 * library.
 */
export type ResumeInfo = { themeId: string, target: Target, 
/**
 * `None` when the theme is no longer in the library: turning back on goes to the Themes
 * page instead (spec §4.1).
 */
themeName: string | null, };
```

`ResumeOutcome.ts`:

```ts
// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { ApplyRequest } from "./ApplyRequest";
import type { PlanOutcome } from "./PlanOutcome";

/**
 * What "Turn the theme back on" does (spec §4.1, §8).
 */
export type ResumeOutcome = { "kind": "resumed" } | { "kind": "apply", request: ApplyRequest, plan: PlanOutcome, } | { "kind": "pickTheme" } | { "kind": "notPaused" };
```

- [ ] **Step 10: The names of the two fixes** — `FixKind` now has `resume` and `pause`, and Help's fix label is `t(\`help.doctor.fix.${c.fix}\`)`. In `app/ui/src/i18n/en.ts`, after `"help.doctor.fix.reinstall": "Reinstall CMD Studio",` add

```ts
  "help.doctor.fix.resume": "Turn the theme back on",
  "help.doctor.fix.pause": "Pause the theme",
```

and in `app/ui/src/i18n/th.ts`, after `"help.doctor.fix.reinstall": "ติดตั้ง CMD Studio ใหม่",` add

```ts
  "help.doctor.fix.resume": "เปิดธีมกลับ",
  "help.doctor.fix.pause": "หยุดธีม",
```

Run in `<repo>\app`: `npm run typecheck` (clean) and `npm test` (every test passes; Help shows the two as words until Task 10 makes them buttons).

- [ ] **Step 11: Commit**

Stage `app/src-tauri/src/backend/recovery.rs app/src-tauri/src/backend/mod.rs app/src-tauri/src/backend/apply.rs app/src-tauri/src/backend/doctor.rs app/src-tauri/src/logs.rs app/ui/src/i18n/en.ts app/ui/src/i18n/th.ts app/ui/src/bindings/FixKind.ts app/ui/src/bindings/CrashCheck.ts app/ui/src/bindings/PauseInfo.ts app/ui/src/bindings/PauseOutcome.ts app/ui/src/bindings/RecoveryStatus.ts app/ui/src/bindings/ResumeInfo.ts app/ui/src/bindings/ResumeOutcome.ts`. Message: `app: the theme's pause and its way back on, and the start's crash check (recovery spec §4.1-§4.3)`.

---

### Task 8: The three IPC commands, and the crash check at every start

Recovery spec §8: `recovery_status` (changes nothing; waits for the start's crash check on its worker, never on the window's thread), `theme_pause(reason)`, `theme_resume`. Each is a thin wrapper that logs once through `Backend::logged`. The crash check starts from `lib.rs`'s setup once the window is up, on a thread of its own; a thread that cannot start is said as a check that could not run, so `recovery_status` never waits for it.

**Files:**
- Modify: `app/src-tauri/src/command_names.rs`, `app/src-tauri/src/ipc.rs`, `app/src-tauri/capabilities/default.json`, `app/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Task 7's `Backend::{recovery_status, theme_pause, theme_resume, crash_check_started, crash_check_not_run, crash_check}` and its DTOs; core `recovery::PauseReason`.
- Produces: IPC commands `recovery_status() -> RecoveryStatus`, `theme_pause({ reason: PauseReason }) -> PauseOutcome`, `theme_resume() -> ResumeOutcome`; permissions `allow-recovery-status`, `allow-theme-pause`, `allow-theme-resume` (generated by `build.rs` from `command_names.rs`).

- [ ] **Step 1: Write the failing tests** — in `app/src-tauri/src/ipc.rs`'s tests: `assert_eq!(COMMANDS.len(), 27);` becomes `30`; `assert_eq!(logging, 23);` becomes `26`; the doc comment above that test says `each of the 26 commands that read or change the machine, …`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p cmdstudio-app --lib -- ipc::`
Expected: FAIL: `left: 27, right: 30` and `left: 23, right: 26`.

- [ ] **Step 3: The names** — in `app/src-tauri/src/command_names.rs`:
- the header comment's first lines become

```rust
// The IPC commands the window may call: spec §5.7's list, plus `preview_render`,
// `open_external` and `advanced_read` (Phase 2b; spec §5.7 records them), and
// `recovery_status`, `theme_pause` and `theme_resume` (phase 3b, recovery spec §8). One list,
// read by `build.rs`, which turns each into the permissions `allow-<command>` and `deny-<command>`
```

(the lines after it are unchanged);
- after `pub const ADVANCED_READ: &str = "advanced_read";` add

```rust
pub const RECOVERY_STATUS: &str = "recovery_status";
pub const THEME_PAUSE: &str = "theme_pause";
pub const THEME_RESUME: &str = "theme_resume";
```

- `/// The 27 IPC command names, in the order of spec §5.7.` becomes `/// The 30 IPC command names, in the order of spec §5.7.`, and `COMMANDS` gets `RECOVERY_STATUS,`, `THEME_PAUSE,`, `THEME_RESUME,` after `ADVANCED_READ,`.

- [ ] **Step 4: The commands** — in `app/src-tauri/src/ipc.rs`:
- the module doc's `(23 of` / `//! the 27)` becomes `(26 of` / `//! the 30)`;
- after `use crate::backend::preview::{…};` add `use crate::backend::recovery::{PauseOutcome, RecoveryStatus, ResumeOutcome};`, and after `use cmdstudio_core::preview::PreviewContext;` add `use cmdstudio_core::recovery::PauseReason;`;
- right before `/// Every command, for \`tauri::Builder::invoke_handler\`.` add

```rust
/// Whether the theme is paused, and what the start's crash check found (recovery spec §8): it
/// waits for that check while it runs, on its worker, never on the window's thread.
#[tauri::command]
pub async fn recovery_status(state: TauriState<'_, Shared>) -> Result<RecoveryStatus, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::RECOVERY_STATUS, |b| b.recovery_status())).await
}

#[tauri::command]
pub async fn theme_pause(
    state: TauriState<'_, Shared>,
    reason: PauseReason,
) -> Result<PauseOutcome, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::THEME_PAUSE, |b| b.theme_pause(reason))).await
}

#[tauri::command]
pub async fn theme_resume(state: TauriState<'_, Shared>) -> Result<ResumeOutcome, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::THEME_RESUME, |b| b.theme_resume())).await
}

```

- in `handler()`'s `tauri::generate_handler![…]`, after `advanced_read,` add `recovery_status,`, `theme_pause,`, `theme_resume,`.

- [ ] **Step 5: Allow them** — in `app/src-tauri/capabilities/default.json`, the last entry of `permissions` `"allow-advanced-read"` becomes `"allow-advanced-read",` followed by `"allow-recovery-status",`, `"allow-theme-pause",`, `"allow-theme-resume"` (same indentation, the last without a comma).

- [ ] **Step 6: Start the crash check** — in `app/src-tauri/src/lib.rs`, in `.setup(move |app| { … })`, after the `if let Err(why) = open_main_window(…) { … }` block and before `Ok(())`, add `start_crash_check(&for_window);`; and right before `/// The window labelled \`main\` in tauri.conf.json.` add

```rust
/// Windows' crash reports of cmd, read once the window is up, on a thread of its own (recovery
/// spec §4.3): the window never waits for them, and `recovery_status` waits for the answer. A
/// thread that cannot be started is said as a check that could not run, so nothing waits for
/// it. Phase 3b-2's recovery mode starts none.
#[cfg(windows)]
fn start_crash_check(backend: &std::sync::Arc<backend::Backend<machine::RealMachine>>) {
    backend.crash_check_started();
    let checker = backend.clone();
    let spawned = std::thread::Builder::new()
        .name("crash-check".to_string())
        .spawn(move || {
            let _ = checker.crash_check();
        });
    if let Err(e) = spawned {
        backend.crash_check_not_run(&format!("the check could not start: {e}"));
    }
}

```

- [ ] **Step 7: Run the tests**

Run: `cargo test -p cmdstudio-app --lib -- ipc::`
Expected: every test passes: each of the 30 names has its `#[tauri::command]`, is registered and allowed, and the 26 that touch the machine log under their constant.
Run: `cargo test -p cmdstudio-app --lib` (210 pass), `cargo build -p cmdstudio-app` (its `build.rs` writes the three new permissions; nothing is started), then `cargo fmt --all` and the four clippy commands: clean. Then `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\export-bindings.ps1` and `git status --porcelain -- app/ui/src/bindings`: prints nothing (no type changed).

- [ ] **Step 8: Commit**

Stage `app/src-tauri/src/command_names.rs app/src-tauri/src/ipc.rs app/src-tauri/capabilities/default.json app/src-tauri/src/lib.rs`. Message: `app: recovery_status, theme_pause and theme_resume, and the crash check at every start (recovery spec §8)`.

---


### Task 9: The UI's IPC: three methods, over Tauri and in the mock

The `Ipc` interface gains `recoveryStatus`, `themePause` and `themeResume`; `tauri.ts` calls the three commands by their Rust names; the mock does what the backend does with fixed data (no palette or crash logic in TypeScript): a pause of this computer reverts and notes the theme, turning back on lifts a pause in cmd at once and plans the paused theme for one of this computer, every apply lifts the pause, "Undo everything" lifts a pause in cmd; the start's crash check runs once, at the first `recoveryStatus` (the backend runs it once per start), and pauses the theme on this computer for three crashes or more; D12 says the pause first, then the crashes the mock is given (`cmdCrashes`), `warn` for 1–2 and `fail` from 3, copied from the core's rows.

**Files:**
- Modify: `app/ui/src/ipc/types.ts`, `app/ui/src/ipc/tauri.ts`, `app/ui/src/ipc/mock.ts`
- Test: `app/ui/src/ipc/tauri.test.ts`, `app/ui/src/ipc/mock.test.ts`

**Interfaces:**
- Consumes: Task 2's and Task 7's bindings `PauseReason`, `PauseInfo`, `PauseOutcome`, `RecoveryStatus`, `ResumeOutcome`; Task 8's command names.
- Produces:
  - `Ipc.recoveryStatus(): Promise<RecoveryStatus>`, `Ipc.themePause(reason: PauseReason): Promise<PauseOutcome>`, `Ipc.themeResume(): Promise<ResumeOutcome>`;
  - `MockOptions.paused?: PauseInfo`, `MockOptions.cmdCrashes?: number`, `MockOptions.failRecoveryWith?: string`; `MockState.paused: PauseInfo | null`;
  - the mock's calls log `recoveryStatus`, `themePause:<reason>`, `themeResume`; its D12 has the fix `resume` (paused) or `pause` (crashes);
  - its first `recoveryStatus` answers `crashes` once for the whole mock (`noTheme`, or `read` with `count` = `cmdCrashes` and `pausedNow` when it paused for three or more), as the backend's `CrashSlot` does.

- [ ] **Step 1: Write the failing tests** — at the end of `app/ui/src/ipc/tauri.test.ts`'s `describe`, before the file's last `});`:

```ts

  it("calls the three commands of the theme's pause by their names, the reason as Rust reads it", async () => {
    invoke.mockReset();
    invoke.mockResolvedValue(null);
    await tauriIpc.recoveryStatus();
    await tauriIpc.themePause("user");
    await tauriIpc.themeResume();
    expect(invoke.mock.calls).toEqual([
      ["recovery_status", undefined],
      ["theme_pause", { reason: "user" }],
      ["theme_resume", undefined],
    ]);
    invoke.mockReset();
  });
```

and at the end of `app/ui/src/ipc/mock.test.ts`'s `describe`, before the file's last `});`:

```ts

  it("pauses and turns the theme back on as the backend does: in cmd at once, on this computer through the plan", async () => {
    const ipc = createMockIpc();
    const terracotta = (await ipc.themesList()).themes[0]!;
    const request = { themeId: terracotta.id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    const planned = await ipc.planApply(request);
    if (planned.kind !== "ready") throw new Error("not ready");
    await ipc.apply(request, planned.plan.digest, () => {});
    expect(await ipc.recoveryStatus()).toEqual({ paused: null, crashes: { kind: "read", count: 0, pausedNow: false } });
    const paused = await ipc.themePause("user");
    expect(paused.revert?.status).toBe("committed");
    expect(ipc.state.applied).toBeNull();
    expect((await ipc.recoveryStatus()).paused).toMatchObject({
      by: "app",
      reason: "user",
      resume: { themeId: terracotta.id, target: "all", themeName: "Terracotta" },
    });
    const resumed = await ipc.themeResume();
    if (resumed.kind !== "apply" || resumed.plan.kind !== "ready") throw new Error(resumed.kind);
    expect(resumed.request).toEqual(request);
    expect((await ipc.apply(resumed.request, resumed.plan.plan.digest, () => {})).status).toBe("committed");
    expect((await ipc.recoveryStatus()).paused).toBeNull();
    // A pause in cmd is lifted at once, and Undo everything takes it too.
    ipc.state.paused = { by: "clink", reason: "cmdStarts", atMs: 1, count: 3, resume: null };
    expect(await ipc.themeResume()).toEqual({ kind: "resumed" });
    expect(await ipc.themeResume()).toEqual({ kind: "notPaused" });
    ipc.state.paused = { by: "clink", reason: "cmdStarts", atMs: 1, count: 3, resume: null };
    await ipc.revertAll();
    expect(ipc.state.paused).toBeNull();
    // Only the user and cmd's crashes are the app's reasons.
    await expect(ipc.themePause("cmdStarts")).rejects.toMatchObject({ code: "E_INTERNAL" });
  });

  it("says D12 as the core does: paused first, then the crashes it is given", async () => {
    const d12 = async (ipc: ReturnType<typeof createMockIpc>) => (await ipc.doctorRun()).checks.find((c) => c.id === "D12")!;
    const theme = PRESETS[0]!;
    const applied = async (options: Parameters<typeof createMockIpc>[0]) => {
      const ipc = createMockIpc(options);
      const request = { themeId: theme.id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
      const planned = await ipc.planApply(request);
      if (planned.kind !== "ready") throw new Error("not ready");
      await ipc.apply(request, planned.plan.digest, () => {});
      return ipc;
    };
    expect((await d12(createMockIpc())).status).toBe("skipped");
    expect(await d12(await applied({}))).toMatchObject({ status: "ok", code: null, fix: null });
    expect(await d12(await applied({ cmdCrashes: 2 }))).toMatchObject({ status: "warn", code: "E_CMD_CRASHED", fix: "pause" });
    expect(await d12(await applied({ cmdCrashes: 3 }))).toMatchObject({ status: "fail", code: "E_CMD_CRASHED", fix: "pause" });
    const paused = createMockIpc({ paused: { by: "clink", reason: "cmdStarts", atMs: 1, count: 3, resume: null } });
    expect(await d12(paused)).toMatchObject({ status: "warn", code: "E_THEME_PAUSED", fix: "resume" });
    for (const word of ["prompt", "เรา", "สำรอง", "รุ่น"]) expect((await d12(paused)).messageTh).not.toContain(word);
  });

  it("runs the start's crash check once, at the first status, and pauses for three crashes as the backend does", async () => {
    const ipc = createMockIpc({ cmdCrashes: 3 });
    const theme = PRESETS[0]!;
    const request = { themeId: theme.id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    const planned = await ipc.planApply(request);
    if (planned.kind !== "ready") throw new Error("not ready");
    await ipc.apply(request, planned.plan.digest, () => {});
    const first = await ipc.recoveryStatus();
    expect(first.crashes).toEqual({ kind: "read", count: 3, pausedNow: true });
    expect(first.paused).toMatchObject({ by: "app", reason: "clinkCrash", count: 3, resume: { themeId: theme.id, target: "all" } });
    expect(ipc.state.applied).toBeNull();
    // Once per start: the same answer, and nothing paused again.
    const pausedAt = first.paused?.atMs;
    const again = await ipc.recoveryStatus();
    expect(again.crashes).toEqual(first.crashes);
    expect(again.paused?.atMs).toBe(pausedAt);
  });
```

- [ ] **Step 2: Run them to see them fail**

Run in `<repo>\app`: `npm test -- src/ipc`
Expected: FAIL: `tauriIpc.recoveryStatus is not a function`, `ipc.recoveryStatus is not a function` (twice), and D12 not found (`Cannot read properties of undefined`).

- [ ] **Step 3: The interface** — in `app/ui/src/ipc/types.ts`:
- the header comment becomes

```ts
// The IPC surface the screens use (spec §5.7, with preview_render, open_external and
// advanced_read, and the theme's pause of recovery spec §8). One interface, two
// implementations: `tauri.ts` over Tauri's invoke, and `mock.ts` in memory for tests and the
// visual check. The types are the ts-rs bindings the Rust side exports (app/ui/src/bindings,
// committed; CI fails when they are stale).
```

- after `import type { Library } from "../bindings/Library";` add `import type { PauseOutcome } from "../bindings/PauseOutcome";` and `import type { PauseReason } from "../bindings/PauseReason";`; after `import type { PreviewResult } from "../bindings/PreviewResult";` add `import type { RecoveryStatus } from "../bindings/RecoveryStatus";`; after `import type { RestorePoints } from "../bindings/RestorePoints";` add `import type { ResumeOutcome } from "../bindings/ResumeOutcome";`;
- in `interface Ipc`, after `advancedRead(): Promise<AdvancedInfo>;` add

```ts
  /** Whether the theme is paused, and what the start's check of Windows' crash reports found
   * (recovery spec §4.2, §4.3). */
  recoveryStatus(): Promise<RecoveryStatus>;
  /** Pauses the theme on this computer (Revert everything underneath): `user` from Home,
   * `clinkCrash` from the health check's D12. */
  themePause(reason: PauseReason): Promise<PauseOutcome>;
  /** Turns the theme back on: a pause in cmd at once, one of this computer as a request to
   * apply through the plan. */
  themeResume(): Promise<ResumeOutcome>;
```

- [ ] **Step 4: Over Tauri** — in `app/ui/src/ipc/tauri.ts`, after `advancedRead: () => call("advanced_read"),` add

```ts
  recoveryStatus: () => call("recovery_status"),
  themePause: (reason) => call("theme_pause", { reason }),
  themeResume: () => call("theme_resume"),
```

- [ ] **Step 5: In the mock** — in `app/ui/src/ipc/mock.ts`:
- after `import type { Colors } from "../bindings/Colors";` add `import type { CrashCheck } from "../bindings/CrashCheck";`; after `import type { Line } from "../bindings/Line";` add `import type { PauseInfo } from "../bindings/PauseInfo";` and `import type { PauseOutcome } from "../bindings/PauseOutcome";`; after `import type { PreviewResult } from "../bindings/PreviewResult";` add `import type { RecoveryStatus } from "../bindings/RecoveryStatus";`; after `import type { RestorePoints } from "../bindings/RestorePoints";` add `import type { ResumeOutcome } from "../bindings/ResumeOutcome";`;
- in `MockOptions`, after `settings?: Partial<AppSettings>;` add

```ts
  /** The theme is paused when the mock starts, as `recovery_status` says it (recovery spec §4.2). */
  paused?: PauseInfo;
  /** cmd's crashes in its add-on since the theme was applied, as the backend's D12 and its start
   * check count them: a fixed number given here, never read from anything. D12 says `warn` for 1
   * and 2 and `fail` from 3, the core's rows (`recovery::PAUSE_AFTER`) copied as fixed data. */
  cmdCrashes?: number;
  /** `recovery_status` fails with this code. */
  failRecoveryWith?: string;
```

- in `MockState`, after `clipboardSuggestion: string;` add

```ts
  /** The theme's pause, as `paused.json` would hold it. */
  paused: PauseInfo | null;
```

- in the initial `state`, after `clipboardSuggestion: "",` add `paused: options.paused ? structuredClone(options.paused) : null,`;
- in `checks`, after the D11 row `ok("D11", "Windows Terminal is the default terminal", "แอปเทอร์มินัลเริ่มต้นคือ Windows Terminal"),` add `d12(),`; then replace the end of `checks`, the two lines `    return list;` and `  };`, with

```ts
    return list;

    /** The core's D12 in fixed words: paused first, then the crashes it is given (cmdCrashes). */
    function d12(): CheckReport {
      if (state.paused) {
        return {
          id: "D12",
          status: "warn",
          code: "E_THEME_PAUSED",
          messageTh: "ธีมถูกหยุดไว้ เปิดธีมกลับได้ที่หน้าแรกของ CMD Studio",
          messageEn: "The theme is paused. Turn it back on from CMD Studio's home page",
          details: "",
          detailsTh: null,
          fix: "resume",
        };
      }
      if (!applied) return skip("D12");
      const crashes = options.cmdCrashes ?? 0;
      if (crashes === 0) {
        return ok(
          "D12",
          "cmd has not crashed in its add-on since the theme was applied, and the theme is not paused",
          "cmd ไม่ได้แครชในส่วนเสริมตั้งแต่ใช้ธีม และธีมไม่ได้ถูกหยุดไว้",
        );
      }
      return {
        id: "D12",
        status: crashes >= 3 ? "fail" : "warn",
        code: "E_CMD_CRASHED",
        messageTh: `cmd แครชในส่วนเสริม ${crashes} ครั้งตั้งแต่ใช้ธีม`,
        messageEn: `cmd crashed in its add-on ${crashes} time(s) since the theme was applied`,
        details: "",
        detailsTh: null,
        fix: "pause",
      };
    }
  };

  /** What Revert everything does to the mock: every apply undone by a new revertAll point. */
  const revertState = () => {
    const id = `${now()}-${uuid().slice(0, 8)}`;
    for (const p of state.points) if (p.kind === "apply" && p.undoneBy === null) { p.undoneBy = id; p.restorable = false; }
    state.points.unshift({ id, kind: "revertAll", createdMs: Number(id.split("-")[0]), theme: null, target: null, summary: ["Revert everything"], undoes: null, undoneBy: null, restorable: false });
    state.applied = null;
  };
  const revertOutcome = (): RevertOutcome => ({
    ok: true,
    steps: [
      { what: "AutoRun and Windows Terminal's settings", ok: true, detail: "CMD Studio's part is taken out", code: null, fixEn: null, fixTh: null, error: null },
      { what: "Fonts", ok: true, detail: "nothing to uninstall", code: null, fixEn: null, fixTh: null, error: null },
    ],
    notes: [],
    status: "committed",
    error: null,
    unrestored: [],
  });
  /** As the backend's pause_machine: Revert everything, then the pause with the theme to apply
   * again (the one applied now, else the one an earlier pause of this computer noted). */
  const pauseHere = (reason: "user" | "clinkCrash", count: number) => {
    const resume = state.applied
      ? { themeId: state.applied.theme.id, target: state.applied.request.target, themeName: state.applied.theme.name }
      : state.paused?.by === "app"
        ? state.paused.resume
        : null;
    revertState();
    state.paused = { by: "app", reason, atMs: now(), count, resume };
  };
  /** The start's crash check, run once at the first `recoveryStatus` as the backend runs it once
   * per start: three crashes or more (cmdCrashes; the core's PAUSE_AFTER) with a theme applied
   * and no pause of this computer yet pause the theme here. */
  let startCheck: CrashCheck | null = null;
  const recovery = (): RecoveryStatus => {
    if (startCheck === null) {
      const count = options.cmdCrashes ?? 0;
      if (!state.applied) {
        startCheck = { kind: "noTheme" };
      } else {
        const pausedNow = count >= 3 && state.paused?.by !== "app";
        if (pausedNow) pauseHere("clinkCrash", count);
        startCheck = { kind: "read", count, pausedNow };
      }
    }
    return { paused: state.paused ? structuredClone(state.paused) : null, crashes: structuredClone(startCheck) };
  };
```

- in `apply`, after `state.applied = { theme: structuredClone(theme), request, snapshotId };` add

```ts
      // As the backend: a theme applied is the theme on again.
      state.paused = null;
```

- replace the whole body of `revertAll` with

```ts
    async revertAll(): Promise<RevertOutcome> {
      state.calls.push("revertAll");
      revertState();
      // As the backend: everything is out, so a pause in cmd goes too; one of this computer stays.
      if (state.paused?.by === "clink") state.paused = null;
      return revertOutcome();
    },
```

(its old lines, from `const id = …` to the returned object, now live in `revertState` and `revertOutcome` above);
- after the method `advancedRead`, the last of the object, add

```ts
    async recoveryStatus(): Promise<RecoveryStatus> {
      state.calls.push("recoveryStatus");
      if (options.failRecoveryWith) throw error(options.failRecoveryWith, "the mock was told to fail");
      return recovery();
    },
    async themePause(reason): Promise<PauseOutcome> {
      state.calls.push(`themePause:${reason}`);
      if (reason !== "user" && reason !== "clinkCrash") {
        throw error("E_INTERNAL", `the app does not pause the theme for ${reason}`);
      }
      pauseHere(reason, reason === "clinkCrash" ? (options.cmdCrashes ?? 0) : 0);
      return { revert: revertOutcome(), refused: null, notSaved: null };
    },
    async themeResume(): Promise<ResumeOutcome> {
      state.calls.push("themeResume");
      const paused = state.paused;
      if (!paused) return { kind: "notPaused" };
      if (paused.by === "clink") {
        state.paused = null;
        return { kind: "resumed" };
      }
      const theme = paused.resume ? all().find((t) => t.id === paused.resume?.themeId) : undefined;
      if (!paused.resume || !theme) return { kind: "pickTheme" };
      const request: ApplyRequest = {
        themeId: theme.id,
        target: paused.resume.target,
        setWtDefaultProfile: state.settings.setWtDefaultProfile,
        replaceOtherClink: false,
      };
      return { kind: "apply", request, plan: await ipc.planApply(request) };
    },
```

- [ ] **Step 6: Run the tests**

Run in `<repo>\app`: `npm run typecheck` (clean), then `npm test`.
Expected: every test passes, four more than before (`210` in all).

- [ ] **Step 7: Commit**

Stage `app/ui/src/ipc/types.ts app/ui/src/ipc/tauri.ts app/ui/src/ipc/tauri.test.ts app/ui/src/ipc/mock.ts app/ui/src/ipc/mock.test.ts`. Message: `ui: the theme's pause over the IPC, and in the mock as the backend does it (recovery spec §8)`.

---

### Task 10: Home's paused banner and "Pause the theme", D12's fixes, Help's "if cmd won't open"

Recovery spec §4.2, §4.3, §4.5, §6. While the theme is paused, Home shows a banner on top: why (the reason and count), since when, what it changed (cmd only, or this computer and the theme it will apply again), "Turn the theme back on", and "Undo everything" (a pause in cmd only) and "Report a problem", both to their Help tabs. Under the theme in use, "Pause the theme" asks first, then pauses on this computer. D12's fixes on the health check pause (`clinkCrash`) or turn back on. When the start's crash check paused the theme while Home was reading, Home reads again once, so it never shows the theme as in use beside a banner that says it was taken out. Help's short guide gains the paragraph on what to do when cmd won't open; the Start-menu shortcut's sentence is 3b-2's.

**Files:**
- Create: `app/ui/src/components/Pause.tsx`
- Modify: `app/ui/src/screens/Home.tsx`, `app/ui/src/screens/Help.tsx`, `app/ui/src/i18n/en.ts`, `app/ui/src/i18n/th.ts`
- Test: `app/ui/src/App.test.tsx`, `app/ui/src/screens/Help.test.tsx`

**Interfaces:**
- Consumes: Task 9's `Ipc.{recoveryStatus, themePause, themeResume}` and mock options; `useIpc`, `useI18n`/`formatTime`/`Key`, `useNav` (`{ name: "help", tab }`, `{ name: "themes" }`), `useAppState` (`bump`, `setLastApply`), `ApplyFlow`, `Dialog`, `ErrorNotice`, `NotesList`, `UnrestoredList`.
- Produces: `components/Pause.tsx`: `type PauseAsk = "user" | "clinkCrash"`, `PausedBanner({ paused: PauseInfo, onResume })`, `usePause(fallback: RefObject<HTMLElement | null>) -> { ask(reason: PauseAsk), view }`, `useResume(fallback) -> { resume(): Promise<void>, view }`; the i18n keys `home.active.pause`, `home.paused.*`, `pause.*`, `resume.*`, `help.guide.cmdWontOpen`.

- [ ] **Step 1: Write the failing tests**
- `app/ui/src/App.test.tsx`: after `import type { ApplyRequest } from "./bindings/ApplyRequest";` add `import type { PauseInfo } from "./bindings/PauseInfo";`; right before `describe("accessibility", () => {` add (with one empty line after it)

```tsx
describe("the theme's pause", () => {
  /** The theme paused in cmd by init.lua after three windows in a row (recovery spec §3). */
  const PAUSED_IN_CMD: PauseInfo = { by: "clink", reason: "cmdStarts", atMs: 1791694709000, count: 3, resume: null };

  /** Terracotta applied before the app mounts, then paused in cmd. */
  const pausedInCmd = async (m: MockIpc) => {
    await applyIn(m, "Terracotta");
    m.state.paused = structuredClone(PAUSED_IN_CMD);
  };

  /** The banner, found by its heading. */
  const banner = async () => (await screen.findByRole("heading", { name: "The theme is paused" })).closest(".card") as HTMLElement;

  it("says on Home that cmd paused the theme, and turns it back on in one click (criterion 1)", async () => {
    const { ipc, user } = await home({}, pausedInCmd);
    const card = await banner();
    expect(within(card).getByText("cmd failed to reach its prompt 3 times in a row, so CMD Studio paused the theme in cmd.")).toBeTruthy();
    expect(within(card).getByText(/^Since /)).toBeTruthy();
    expect(within(card).getByText("Only cmd windows open without it: nothing on this computer was changed.")).toBeTruthy();
    expect(within(card).getByRole("button", { name: "Undo everything" })).toBeTruthy();
    expect(within(card).getByRole("button", { name: "Report a problem" })).toBeTruthy();
    // No "Pause the theme" while it is paused.
    expect(screen.queryByRole("button", { name: "Pause the theme" })).toBeNull();
    await user.click(within(card).getByRole("button", { name: "Turn the theme back on" }));
    expect(await screen.findByText("The theme is back on. Open a new cmd window to see it.")).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole("heading", { name: "The theme is paused" })).toBeNull());
    expect(ipc.state.paused).toBeNull();
    // Nothing on the computer was changed: no apply besides the one before the app started.
    expect(ipc.state.calls.filter((c) => c === "apply")).toHaveLength(1);
    expect(await screen.findByRole("button", { name: "Pause the theme" })).toBeTruthy();
  });

  it("pauses the theme on this computer after asking, and turns it back on through the plan", async () => {
    const { ipc, user } = await home({}, (m) => applyIn(m, "Terracotta"));
    const active = (await screen.findByRole("heading", { name: "Theme in use" })).closest(".card") as HTMLElement;
    await user.click(await within(active).findByRole("button", { name: "Pause the theme" }));
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByText(
        "CMD Studio takes its changes out of this computer, so cmd windows open without the theme. You can turn it back on from the home page at any time.",
      ),
    ).toBeTruthy();
    expect(ipc.state.calls.some((c) => c.startsWith("themePause"))).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: "Pause the theme" }));
    expect(await within(dialog).findByText("The theme is paused: cmd windows open without it.")).toBeTruthy();
    expect(ipc.state.calls).toContain("themePause:user");
    expect(ipc.state.applied).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    const card = await banner();
    expect(within(card).getByText("You paused the theme.")).toBeTruthy();
    expect(within(card).getByText("CMD Studio took its changes out of this computer. Turning it back on applies Terracotta again.")).toBeTruthy();
    expect(within(card).queryByRole("button", { name: "Undo everything" })).toBeNull();
    await user.click(within(card).getByRole("button", { name: "Turn the theme back on" }));
    const plan = await screen.findByRole("dialog");
    await user.click(await within(plan).findByRole("button", { name: "Apply" }));
    expect(await within(plan).findByText(/Theme "Terracotta" applied/)).toBeTruthy();
    expect(ipc.state.applied?.theme.name).toBe("Terracotta");
    expect(ipc.state.paused).toBeNull();
    await user.click(within(plan).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("heading", { name: "The theme is paused" })).toBeNull());
  });

  it("takes the user to the Themes page when the paused theme is no longer there", async () => {
    const gone: PauseInfo = {
      by: "app",
      reason: "clinkCrash",
      atMs: 1791694709000,
      count: 3,
      resume: { themeId: "00000000-0000-4000-8000-000000000000", target: "all", themeName: null },
    };
    const { user } = await home({ paused: gone });
    const card = await banner();
    expect(within(card).getByText("cmd crashed in its add-on 3 times, so CMD Studio paused the theme on this computer.")).toBeTruthy();
    expect(
      within(card).getByText("CMD Studio took its changes out of this computer. Turning it back on takes you to the Themes page to choose a theme."),
    ).toBeTruthy();
    await user.click(within(card).getByRole("button", { name: "Turn the theme back on" }));
    expect(await screen.findByRole("heading", { name: "Themes" })).toBeTruthy();
  });

  it("takes Undo everything of a pause in cmd to Help, and the pause goes with the theme", async () => {
    const { ipc, user } = await home({}, pausedInCmd);
    await user.click(within(await banner()).getByRole("button", { name: "Undo everything" }));
    expect(await screen.findByRole("button", { name: "Undo everything", pressed: true })).toBeTruthy();
    const action = (await screen.findAllByRole("button", { name: "Undo everything" })).find((b) => !b.closest('[role="group"]'))!;
    await user.click(action);
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Undo everything" }));
    expect(await screen.findByText("Done: everything CMD Studio changed is back as it was.")).toBeTruthy();
    expect(ipc.state.paused).toBeNull();
  });

  it("says when whether the theme is paused could not be read, and the rest of Home is still there", async () => {
    await home({ failRecoveryWith: "E_MACHINE_READ" }, (m) => applyIn(m, "Terracotta"));
    expect(await screen.findByText("Whether the theme is paused could not be read")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Theme in use" })).toBeTruthy();
    // Not known, so no Pause: it could pause a theme that is paused already.
    expect(screen.queryByRole("button", { name: "Pause the theme" })).toBeNull();
  });

  it("offers on the health check to pause after cmd crashed in its add-on, and to turn back on while paused", async () => {
    const { ipc, user } = await home({ cmdCrashes: 2 }, (m) => applyIn(m, "Terracotta"));
    await open(user, "Help");
    await screen.findByText(/D12/);
    await user.click(await screen.findByRole("button", { name: "Pause the theme" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Pause the theme" }));
    expect(await within(dialog).findByText("The theme is paused: cmd windows open without it.")).toBeTruthy();
    expect(ipc.state.calls).toContain("themePause:clinkCrash");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    // Checked again: D12 says it is paused, and turning it back on goes through the plan.
    await user.click(await screen.findByRole("button", { name: "Turn the theme back on" }));
    const plan = await screen.findByRole("dialog");
    expect(await within(plan).findByRole("button", { name: "Apply" })).toBeTruthy();
  });

  it("reads Home again when the start's crash check paused the theme while Home was reading", async () => {
    const { ipc } = await home({ cmdCrashes: 3 }, (m) => applyIn(m, "Terracotta"));
    const card = await banner();
    expect(within(card).getByText("cmd crashed in its add-on 3 times, so CMD Studio paused the theme on this computer.")).toBeTruthy();
    expect(within(card).getByText("CMD Studio took its changes out of this computer. Turning it back on applies Terracotta again.")).toBeTruthy();
    // What Home read before the pause is out of date: it reads again, and no theme is applied now.
    expect(await screen.findByText("No theme is applied yet.")).toBeTruthy();
    expect(ipc.state.applied).toBeNull();
    expect(ipc.state.calls.filter((c) => c === "themesList").length).toBeGreaterThanOrEqual(2);
  });
});
```

and at the end of `describe("accessibility", …)`, the file's last block, before its closing `});`:

```tsx

  it("names every button of the pause banner and its dialogs, in English and in Thai", async () => {
    for (const language of ["en", "th"] as Language[]) {
      const d = language === "th" ? th : en;
      const { user, view } = await home({ settings: { language } }, (m) => applyIn(m, "Terracotta"));
      await user.click(await screen.findByRole("button", { name: d["home.active.pause"] }));
      const dialog = await screen.findByRole("dialog");
      expectNamedButtons(`${language}: the pause dialog`);
      await user.click(within(dialog).getByRole("button", { name: d["pause.button"] }));
      await within(dialog).findByText(d["pause.done"]);
      expectNamedButtons(`${language}: the pause dialog, done`);
      await user.click(within(dialog).getByRole("button", { name: d["a11y.closeDialog"] }));
      await screen.findByRole("heading", { name: d["home.paused.title"] });
      await settle();
      expectNamedButtons(`${language}: Home, paused`);
      await user.click(screen.getByRole("button", { name: d["resume.button"] }));
      const plan = await screen.findByRole("dialog");
      await within(plan).findByRole("button", { name: d["apply.start"] });
      expectNamedButtons(`${language}: the plan that turns the theme back on`);
      view.unmount();
    }
  });
```

- `app/ui/src/screens/Help.test.tsx`, in `describe("the guide and the credits", …)`: the test `it("has the five paragraphs of the guide", …)` becomes `it("has the five paragraphs of the guide and what to do when cmd won't open", …)`, and its list `["p1", "p2", "p3", "p4", "p5"] as const` becomes `["p1", "p2", "p3", "p4", "p5", "cmdWontOpen"] as const`.

- [ ] **Step 2: Run them to see them fail**

Run in `<repo>\app`: `npm test -- src/App.test.tsx src/screens/Help.test.tsx`
Expected: FAIL: no heading "The theme is paused", no button "Pause the theme", `en["help.guide.cmdWontOpen"]` undefined; `npm run typecheck` fails on the unknown keys `home.active.pause`, `pause.button`, `pause.done`, `home.paused.title`, `resume.button`.

- [ ] **Step 3: The words** — in `app/ui/src/i18n/en.ts`, after `"home.restore.all": "See all restore points",` add

```ts
  "home.active.pause": "Pause the theme",
  "home.paused.title": "The theme is paused",
  "home.paused.reason.cmdStarts": "cmd failed to reach its prompt {n} times in a row, so CMD Studio paused the theme in cmd.",
  "home.paused.reason.clinkCrash": "cmd crashed in its add-on {n} times, so CMD Studio paused the theme on this computer.",
  "home.paused.reason.recovery": "The theme was paused from the CMD Studio Recovery shortcut.",
  "home.paused.reason.user": "You paused the theme.",
  "home.paused.reason.unknown": "CMD Studio's note of the pause can't be read; cmd windows open without the theme until it is turned back on.",
  "home.paused.since": "Since {time}",
  "home.paused.levelClink": "Only cmd windows open without it: nothing on this computer was changed.",
  "home.paused.levelApp": "CMD Studio took its changes out of this computer. Turning it back on applies {name} again.",
  "home.paused.levelAppGone": "CMD Studio took its changes out of this computer. Turning it back on takes you to the Themes page to choose a theme.",
  "home.paused.revert": "Undo everything",
  "home.paused.report": "Report a problem",
  "home.paused.unreadable": "Whether the theme is paused could not be read",
  "pause.title": "Pause the theme",
  "pause.confirm": "CMD Studio takes its changes out of this computer, so cmd windows open without the theme. You can turn it back on from the home page at any time.",
  "pause.button": "Pause the theme",
  "pause.working": "Pausing the theme…",
  "pause.done": "The theme is paused: cmd windows open without it.",
  "pause.notOk": "The theme is paused, but not everything could be taken out: see below.",
  "pause.refused": "Nothing on this computer could be changed right now, but cmd windows open without the theme until you turn it back on.",
  "pause.notSaved": "CMD Studio took its changes out, but could not note the pause: to turn the theme back on, apply it again from the Themes page.",
  "resume.title": "Turn the theme back on",
  "resume.button": "Turn the theme back on",
  "resume.done": "The theme is back on. Open a new cmd window to see it.",
```

and after `"help.guide.p5": …,` add

```ts
  "help.guide.cmdWontOpen": "If cmd won't open: open CMD Studio. When cmd keeps failing to start because of the theme, CMD Studio pauses the theme by itself, and you can also press Pause the theme on the home page.",
```

In `app/ui/src/i18n/th.ts`, after `"home.restore.all": "ดูจุดย้อนกลับทั้งหมด",` add

```ts
  "home.active.pause": "หยุดธีม",
  "home.paused.title": "ธีมถูกหยุดไว้",
  "home.paused.reason.cmdStarts": "cmd เปิดไม่ถึงพร้อมท์ {n} ครั้งติดกัน CMD Studio จึงหยุดธีมใน cmd ไว้",
  "home.paused.reason.clinkCrash": "cmd แครชในส่วนเสริม {n} ครั้ง CMD Studio จึงหยุดธีมของเครื่องนี้ไว้",
  "home.paused.reason.recovery": "ธีมถูกหยุดไว้จากทางลัด CMD Studio Recovery",
  "home.paused.reason.user": "คุณหยุดธีมไว้",
  "home.paused.reason.unknown": "อ่านบันทึกการหยุดธีมของ CMD Studio ไม่ได้ หน้าต่าง cmd จะเปิดโดยไม่มีธีมจนกว่าจะเปิดธีมกลับ",
  "home.paused.since": "ตั้งแต่ {time}",
  "home.paused.levelClink": "มีแค่หน้าต่าง cmd ที่เปิดโดยไม่มีธีม ยังไม่ได้เปลี่ยนการตั้งค่าใดของเครื่องนี้",
  "home.paused.levelApp": "CMD Studio เอาสิ่งที่เปลี่ยนไว้ออกจากเครื่องนี้แล้ว เปิดธีมกลับจะใช้ธีม {name} อีกครั้ง",
  "home.paused.levelAppGone": "CMD Studio เอาสิ่งที่เปลี่ยนไว้ออกจากเครื่องนี้แล้ว เปิดธีมกลับจะพาไปหน้าธีมเพื่อเลือกธีม",
  "home.paused.revert": "ย้อนกลับทุกอย่าง",
  "home.paused.report": "แจ้งปัญหา",
  "home.paused.unreadable": "อ่านไม่ได้ว่าธีมถูกหยุดไว้หรือไม่",
  "pause.title": "หยุดธีม",
  "pause.confirm": "CMD Studio จะเอาสิ่งที่เปลี่ยนไว้ออกจากเครื่องนี้ หน้าต่าง cmd จะเปิดโดยไม่มีธีม และเปิดธีมกลับได้ที่หน้าแรกทุกเมื่อ",
  "pause.button": "หยุดธีม",
  "pause.working": "กำลังหยุดธีม…",
  "pause.done": "หยุดธีมแล้ว หน้าต่าง cmd จะเปิดโดยไม่มีธีม",
  "pause.notOk": "หยุดธีมแล้ว แต่เอาออกได้ไม่ครบ ดูด้านล่าง",
  "pause.refused": "ตอนนี้ยังเปลี่ยนการตั้งค่าของเครื่องนี้ไม่ได้ แต่หน้าต่าง cmd จะเปิดโดยไม่มีธีมจนกว่าคุณจะเปิดธีมกลับ",
  "pause.notSaved": "CMD Studio เอาสิ่งที่เปลี่ยนไว้ออกแล้ว แต่บันทึกการหยุดธีมไม่ได้ ถ้าจะเปิดธีมกลับ ให้ใช้ธีมอีกครั้งจากหน้าธีม",
  "resume.title": "เปิดธีมกลับ",
  "resume.button": "เปิดธีมกลับ",
  "resume.done": "เปิดธีมกลับแล้ว เปิดหน้าต่าง cmd ใหม่เพื่อดูธีม",
```

and after `"help.guide.p5": …,` add

```ts
  "help.guide.cmdWontOpen": 'ถ้า cmd เปิดไม่ขึ้น: เปิด CMD Studio เมื่อ cmd เปิดไม่ขึ้นซ้ำๆ เพราะธีม CMD Studio จะหยุดธีมให้เอง และกด "หยุดธีม" ที่หน้าแรกเองก็ได้',
```

(`help.doctor.fix.resume` and `help.doctor.fix.pause` came in Task 7.)

- [ ] **Step 4: Write `app/ui/src/components/Pause.tsx`**

```tsx
// The theme's pause (recovery spec §4.1, §4.2), shared by the home page (its banner and the
// "Pause the theme" button under the theme in use) and the health check's D12 fixes. Pausing
// takes CMD Studio's changes out of this computer after a confirmation (`theme_pause`, Revert
// everything underneath), and cmd windows then open without the theme. Turning it back on lifts
// a pause in cmd at once, and for a pause of this computer applies the paused theme again
// through the plan (`theme_resume`, then the ApplyFlow every apply goes through).
import { useEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { PauseInfo } from "../bindings/PauseInfo";
import type { PauseOutcome } from "../bindings/PauseOutcome";
import { formatTime, useI18n, type Key } from "../i18n";
import { useIpc } from "../ipc/context";
import { useNav } from "../router";
import { useAppState } from "../state";
import { ApplyFlow } from "./ApplyFlow";
import { Dialog } from "./Dialog";
import { ErrorNotice, NotesList, UnrestoredList } from "./Notices";

/** Why the user pauses: from the home page, or from the health check after cmd's crashes. */
export type PauseAsk = "user" | "clinkCrash";

/**
 * The banner over the home page while the theme is paused: why, since when, what it changed, and
 * the ways on. "Undo everything" is offered for a pause in cmd only: a pause of this computer has
 * taken everything out already.
 */
export function PausedBanner({ paused, onResume }: { paused: PauseInfo; onResume: () => void }) {
  const { t, language } = useI18n();
  const { go } = useNav();
  const reason =
    paused.reason === null ? t("home.paused.reason.unknown") : t(`home.paused.reason.${paused.reason}`, { n: paused.count });
  const level =
    paused.by === "clink"
      ? t("home.paused.levelClink")
      : paused.resume?.themeName
        ? t("home.paused.levelApp", { name: paused.resume.themeName })
        : t("home.paused.levelAppGone");
  return (
    <div className="card notice warn">
      <h2 style={{ marginTop: 0 }}>{t("home.paused.title")}</h2>
      <p>{reason}</p>
      {paused.atMs !== null && <p>{t("home.paused.since", { time: formatTime(paused.atMs, language) })}</p>}
      <p>{level}</p>
      <div className="row-actions">
        <button type="button" className="primary" onClick={onResume}>
          {t("resume.button")}
        </button>
        {paused.by === "clink" && (
          <button type="button" onClick={() => go({ name: "help", tab: "revert" })}>
            {t("home.paused.revert")}
          </button>
        )}
        <button type="button" onClick={() => go({ name: "help", tab: "report" })}>
          {t("home.paused.report")}
        </button>
      </div>
    </div>
  );
}

/** Once a dialog is gone with the button that opened it (the theme it paused is out), focus goes
 * to `fallback`, the page's heading, rather than to the page's body. */
function useFocusAfterClose(open: boolean, fallback: RefObject<HTMLElement | null>) {
  const wasOpen = useRef(false);
  useEffect(() => {
    if (open) {
      wasOpen.current = true;
      return;
    }
    if (!wasOpen.current) return;
    wasOpen.current = false;
    const active = document.activeElement;
    if (active === null || active === document.body || !active.isConnected) fallback.current?.focus();
  }, [open, fallback]);
}

/** "Pause the theme": `ask(reason)` opens the confirmation; render `view` where the dialog goes. */
export function usePause(fallback: RefObject<HTMLElement | null>): { ask: (reason: PauseAsk) => void; view: ReactNode } {
  const [asked, setAsked] = useState<{ key: number; reason: PauseAsk } | null>(null);
  const keys = useRef(0);
  useFocusAfterClose(asked !== null, fallback);
  const view = asked && <PauseDialog key={asked.key} reason={asked.reason} onClose={() => setAsked(null)} />;
  return {
    ask: (reason) => {
      keys.current += 1;
      setAsked({ key: keys.current, reason });
    },
    view,
  };
}

/** The confirmation, then what the pause did. No Close while it runs (ruling R7, as every change
 * of the machine); its outcome stays in the dialog, whose box takes focus. */
function PauseDialog({ reason, onClose }: { reason: PauseAsk; onClose: () => void }) {
  const ipc = useIpc();
  const { t } = useI18n();
  const { bump, setLastApply } = useAppState();
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ kind: "done"; outcome: PauseOutcome } | { kind: "error"; error: unknown } | null>(null);
  const running = useRef(false);
  const resultBox = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (result) resultBox.current?.focus();
  }, [result]);

  const pause = async () => {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    try {
      const outcome = await ipc.themePause(reason);
      setResult({ kind: "done", outcome });
      // The after-apply bar's theme is gone with it.
      if (outcome.revert?.status === "committed") setLastApply(null);
    } catch (e) {
      setResult({ kind: "error", error: e });
    } finally {
      running.current = false;
      setBusy(false);
      bump();
    }
  };

  return (
    <Dialog title={t("pause.title")} onClose={busy ? undefined : onClose}>
      {result === null && (
        <>
          <p>{t("pause.confirm")}</p>
          <div className="row-actions">
            <button type="button" className="danger" disabled={busy} onClick={() => void pause()}>
              {t("pause.button")}
            </button>
          </div>
        </>
      )}
      {/* Always there, its text toggled (2c item 10). */}
      <div role="status">{busy ? t("pause.working") : ""}</div>
      {result && (
        <div ref={resultBox} tabIndex={-1}>
          {result.kind === "error" ? <ErrorNotice error={result.error} live={false} /> : <PauseResult outcome={result.outcome} />}
        </div>
      )}
    </Dialog>
  );
}

/** The pause's headline: refused (nothing changed, cmd still opens plain), not noted, not every
 * change taken out, or done. */
function pauseHeadline(outcome: PauseOutcome): { tone: "ok" | "warn"; key: Key } {
  if (outcome.refused) return { tone: "warn", key: "pause.refused" };
  if (outcome.notSaved) return { tone: "warn", key: "pause.notSaved" };
  if (outcome.revert && !outcome.revert.ok) return { tone: "warn", key: "pause.notOk" };
  return { tone: "ok", key: "pause.done" };
}

function PauseResult({ outcome }: { outcome: PauseOutcome }) {
  const { t } = useI18n();
  const { tone, key } = pauseHeadline(outcome);
  const failed = outcome.revert?.steps.filter((s) => !s.ok && s.error) ?? [];
  return (
    <>
      <div className={`notice ${tone}`}>{t(key)}</div>
      {/* Right after the headline, whose "see below" points at it. */}
      {outcome.revert && <UnrestoredList steps={outcome.revert.unrestored} live={false} />}
      {failed.map((s, i) => (
        <ErrorNotice key={i} error={s.error} live={false} />
      ))}
      {outcome.revert?.error && <ErrorNotice error={outcome.revert.error} live={false} />}
      {outcome.refused && <ErrorNotice error={outcome.refused} live={false} />}
      {outcome.notSaved && <ErrorNotice error={outcome.notSaved} live={false} />}
      {outcome.revert && <NotesList notes={outcome.revert.notes} />}
    </>
  );
}

/**
 * "Turn the theme back on": `resume()` asks the backend, which lifts a pause in cmd at once
 * (said in the status line of `view`), gives a request for a pause of this computer (applied
 * through the plan in a dialog of `view`), or sends the user to the Themes page when the paused
 * theme is gone.
 */
export function useResume(fallback: RefObject<HTMLElement | null>): { resume: () => Promise<void>; view: ReactNode } {
  const ipc = useIpc();
  const { t } = useI18n();
  const { go } = useNav();
  const { bump, setLastApply } = useAppState();
  const [applying, setApplying] = useState<{ key: number; request: ApplyRequest } | null>(null);
  const [applyBusy, setApplyBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [resumed, setResumed] = useState(false);
  const keys = useRef(0);
  useFocusAfterClose(applying !== null, fallback);

  const resume = async () => {
    setError(null);
    setResumed(false);
    try {
      const outcome = await ipc.themeResume();
      if (outcome.kind === "resumed") {
        setResumed(true);
        bump();
      } else if (outcome.kind === "apply") {
        keys.current += 1;
        setApplyBusy(false);
        setApplying({ key: keys.current, request: outcome.request });
      } else if (outcome.kind === "pickTheme") {
        go({ name: "themes" });
      } else {
        bump();
      }
    } catch (e) {
      setError(e);
    }
  };
  const close = () => {
    setApplying(null);
    setApplyBusy(false);
  };

  const view = (
    <>
      {/* Always there, its text toggled (2c item 10). */}
      <p role="status">{resumed ? t("resume.done") : ""}</p>
      {error !== null && <ErrorNotice error={error} />}
      {applying && (
        // No Close while the apply runs (ruling R7), and the outcome is read in the dialog.
        <Dialog title={t("resume.title")} onClose={applyBusy ? undefined : close}>
          <ApplyFlow
            key={applying.key}
            titled={false}
            request={applying.request}
            onBusy={setApplyBusy}
            onCancel={close}
            onApplied={(outcome, request) => {
              setLastApply({ outcome, target: request.target });
              bump();
            }}
          />
        </Dialog>
      )}
    </>
  );
  return { resume, view };
}
```

- [ ] **Step 5: Home** — in `app/ui/src/screens/Home.tsx`:
- the header comment's last line `// is said as such, never as "no theme is applied" (spec §5.8 D9).` becomes

```tsx
// is said as such, never as "no theme is applied" (spec §5.8 D9). While the theme is paused a
// banner says why, since when and what it changed, with "Turn the theme back on"; under the theme
// in use, "Pause the theme" asks first (recovery spec §4.2).
```

- after `import { ErrorNotice } from "../components/Notices";` add `import { PausedBanner, usePause, useResume } from "../components/Pause";`;
- after `const readPoints = (ipc: Ipc) => ipc.snapshotsList();` add `const readRecovery = (ipc: Ipc) => ipc.recoveryStatus();`;
- in `Home`, after `const points = useRead(readPoints, version);` add `const recovery = useRead(readRecovery, version);`, and after `const closing = useRef(false);` add

```tsx
  const pause = usePause(heading);
  const resume = useResume(heading);
  const paused = recovery.value?.paused ?? null;
```

- after `const appliedId = lib?.applied?.themeId ?? null;` add

```tsx

  // The start's crash check paused the theme on this computer while this page was reading
  // (recovery spec §4.3): what it read before says a theme is applied, so it reads again, once.
  const reread = useRef(false);
  const crashes = recovery.value?.crashes ?? null;
  useEffect(() => {
    if (reread.current || crashes?.kind !== "read" || !crashes.pausedNow) return;
    if (paused?.by !== "app" || !lib?.applied) return;
    reread.current = true;
    bump();
  }, [crashes, paused, lib, bump]);
```

(`bump` is already taken from `useAppState()` in `Home`; the conditions make it one extra read at most per visit: after it, no theme is applied);
- right after the `<h1 ref={heading} tabIndex={-1}>…</h1>` element add

```tsx
      {recovery.error !== null && <ErrorNotice error={recovery.error} title={t("home.paused.unreadable")} />}
      {paused && <PausedBanner paused={paused} onResume={() => void resume.resume()} />}
      {resume.view}
      {pause.view}
```

- in the theme-in-use card's `row-actions`, after the button whose text is `{t("home.active.openCmd")}`, add

```tsx
              {/* Only once it is known that the theme is not paused already. */}
              {recovery.value !== null && recovery.value.paused === null && (
                <button type="button" onClick={() => pause.ask("user")}>
                  {t("home.active.pause")}
                </button>
              )}
```

- [ ] **Step 6: Help** — in `app/ui/src/screens/Help.tsx`:
- the header comment's lines `// copied for a GitHub issue); the short guide; the credits and licenses. Every failure, a window` / `// that could not be opened included, is shown (spec §5.9).` become

```tsx
// copied for a GitHub issue); the short guide, with what to do when cmd won't open; the credits
// and licenses. D12's fixes pause the theme or turn it back on as the home page does
// (components/Pause, recovery spec §4.5). Every failure, a window that could not be opened
// included, is shown (spec §5.9).
```

- after `import { ErrorNotice, Notice, NotesList, UnrestoredList } from "../components/Notices";` add `import { usePause, useResume } from "../components/Pause";`;
- `RUNNABLE`'s doc comment and set become

```tsx
/** The fixes this page runs (F2): an apply through the plan, the Store, a new cmd window, and
 * D12's pause and turning back on. Reinstalling only the user can do: it is said, not a button.
 * A Windows Terminal that was never opened needs nothing of the user's, since an apply sets it
 * up (founder 2026-10-17). */
const RUNNABLE: ReadonlySet<FixKind> = new Set<FixKind>([
  "reapply",
  "replaceOtherClink",
  "installWindowsTerminal",
  "openNewCmdWindow",
  "resume",
  "pause",
]);
```

- in `function Doctor()` (the health check), after `const closing = useRef(false);` add

```tsx
  const pause = usePause(heading);
  const resume = useResume(heading);
```

- in `Doctor`'s fix handler, after the branch `} else if (kind === "openNewCmdWindow") { await ipc.openCmd();` add

```tsx
      } else if (kind === "pause") {
        pause.ask("clinkCrash");
      } else if (kind === "resume") {
        await resume.resume();
```

- at the end of `Doctor`'s returned JSX, after the apply dialog (`</Dialog>` and `)}`) and before the closing `</div>`, add `{pause.view}` and `{resume.view}` (one per line);
- in `function Guide()`, after the `{(["p1", "p2", "p3", "p4", "p5"] as const).map(…)}` block add `<p>{t("help.guide.cmdWontOpen")}</p>`.

- [ ] **Step 7: Run the tests**

Run in `<repo>\app`: `npm run typecheck` (clean), then `npm test`.
Expected: every test passes (`218` in all: eight new in `App.test.tsx`). `i18n.test.ts` passes: both dictionaries have every key, no Thai in English, the Thai uses พร้อมท์ and ส่วนเสริม, and no text of 30 or more characters sits under two keys.

- [ ] **Step 8: The visual check (the controller, in the browser pane)** — `$env:VITE_MOCK_IPC = "1"; npm run dev` in `<repo>\app`, open `http://localhost:1420` in the built-in browser pane (never the Tauri window). Through the wizard to Terracotta applied; on Home press "Pause the theme": the dialog asks first, "Pause the theme" in it, then "The theme is paused: cmd windows open without it."; close it: focus is on Home's heading, the banner says "You paused the theme.", the time, and "…applies Terracotta again.", with no "Undo everything"; "Turn the theme back on" opens the plan, Apply, Close: the banner is gone and "Pause the theme" is back. Help → Health check: D12 is `ok`. Help → Short guide: the paragraph "If cmd won't open: …" is last. Switch to ไทย in Settings and repeat Home's pause: every text reads Thai, same layout, no clipped button at the window's smallest size, 900×620 (the pane's viewport set to it). No console error. Stop the server; `Remove-Item Env:VITE_MOCK_IPC`.

- [ ] **Step 9: Commit**

Stage `app/ui/src/components/Pause.tsx app/ui/src/screens/Home.tsx app/ui/src/screens/Help.tsx app/ui/src/i18n/en.ts app/ui/src/i18n/th.ts app/ui/src/App.test.tsx app/ui/src/screens/Help.test.tsx`. Message: `ui: Home's paused banner and Pause the theme, D12's fixes, and what to do when cmd won't open (recovery spec §4.2, §4.5)`.

---


### Task 11: The main spec records phase 3b-1

The main spec's own sections for what this plan adds: the data folder's `health\` (§5.3) and the log's new lines, the three IPC commands and the count of 30 (§5.7, §5.10), `cmdstudio apply` lifting the pause and `--cleanup --data-dir` knowing `health\` (§5.7), D12 (§5.8), and the decisions of this phase (appendix B). §5.9's two rows came with Task 5. The recovery spec is the founder's and is not edited.

Spike results this task depends on: S6 (F5) and S9 (F8), for two sentences named below.

**Files:**
- Modify: `docs/superpowers/specs/2026-10-10-cmd-studio-design.md`

**Interfaces:**
- Consumes: what Tasks 2–10 built, by name; `docs/spikes/phase3b-recovery/results.md`.
- Produces: nothing code reads, except the §5.9 table that `error.rs` and `crates/core/tests/error_codes.rs` read (unchanged here).

- [ ] **Step 1: §5.3** — in the tree of `<data>\`, after the line that starts `├─ webview\ `, add

```
├─ health\            ของ init.lua และการหยุดธีม (recovery spec §3.1, §7): ป้าย start-<pid> ที่หน้าต่าง cmd เขียนไว้จนถึงพร้อมท์แรก (บรรทัด ASCII `v1 <วินาที unix>`) และ paused.json เมื่อธีมถูกหยุดไว้ · revert_all ของ core ไม่ลบ paused.json ส่วน --cleanup ลบทั้งโฟลเดอร์ (phase 3b)
```

and at the end of the line that starts `└─ logs\ `, after `(phase 2b)`, add ` · บรรทัดของแอปเองที่ไม่ใช่คำสั่ง IPC มีแค่ผลการตรวจรายงานแครชตอนเปิดแอป (crash_check …) การหยุดธีม (pause reason=… count=… revert=… saved=…) และการเปิดธีมกลับจากการหยุดใน cmd (resume by=clink) ไม่มีข้อมูลของผู้ใช้เช่นกัน (phase 3b)`.

- [ ] **Step 2: §5.7 and §5.10**
- after the bullet ``- `diagnostics_copy`, `advanced_read` (…) (phase 2b)`` add

```markdown
- `recovery_status`, `theme_pause`, `theme_resume` (การหยุดธีมตาม recovery spec §8: `recovery_status` อ่านอย่างเดียว บอกว่าธีมถูกหยุดไว้หรือไม่และผลการตรวจรายงานแครชตอนเปิดแอป โดยรอผลนั้นไม่เกิน 30 วินาที · `theme_pause` รับเหตุผล `user` หรือ `clinkCrash` แล้วย้อนกลับทุกอย่างและบันทึก `paused.json` พร้อมธีมที่จะเปิดกลับ · `theme_resume` ยกเลิกการหยุดใน cmd ทันที หรือคืนคำขอและแผนของธีมที่หยุดไว้ให้ใช้ผ่านแผน) (phase 3b)
```

- `ไฟล์ capability อนุญาตเฉพาะ 27 คำสั่งนี้` becomes `ไฟล์ capability อนุญาตเฉพาะ 30 คำสั่งนี้`; in §5.10, `เปิด capability ของ Tauri เฉพาะ 27 คำสั่ง IPC ใน §5.7` becomes `เปิด capability ของ Tauri เฉพาะ 30 คำสั่ง IPC ใน §5.7`;
- in the CLI table's row ``| `cmdstudio apply <file.cmdtheme> …` |``, at the end of its second cell (after `เหมือนช่องที่ติ๊กไว้ในหน้า 4`) add ` · การใช้ธีมที่สำเร็จยกเลิกการหยุดธีมและลบป้ายใน `health\` (recovery spec §4.1) (phase 3b)`;
- in the `--cleanup --data-dir` sentence, `` `logs\`, `webview\` และไฟล์ `` becomes `` `logs\`, `webview\`, `health\` และไฟล์ ``, and `` `drafts\`, `logs\`, `webview\` และ `settings.json` เพิ่มใน phase 2b) `` becomes `` `drafts\`, `logs\`, `webview\` และ `settings.json` เพิ่มใน phase 2b · `health\` เพิ่มใน phase 3b) ``.

- [ ] **Step 3: §5.8** — after the row `| D11 | … |` add (if Task 1 took F8, write the EventData names it recorded in place of `AppName` and `ModuleName`)

```markdown
| D12 | ธีมถูกหยุดไว้หรือไม่ และ cmd แครชในส่วนเสริม (Clink) ตั้งแต่ใช้ธีมหรือไม่ ตามรายงานแครชของ Windows (event 1000 ของ `Application Error` ใน event log ของ Application ที่ `AppName` เป็น `cmd.exe` และ `ModuleName` เป็น `clink_dll_x64.dll` ย้อนไม่เกินเจ็ดวัน อ่านผ่าน Event Log API ไม่เกินห้าวินาที) · ถ้าธีมถูกหยุดไว้ให้เตือน `E_THEME_PAUSED` ก่อน พร้อมระดับ (เฉพาะใน cmd หรือทั้งเครื่อง) เหตุผล และเวลาที่หยุด ปุ่มแก้ของแอปคือ "เปิดธีมกลับ" · อ่าน `paused.json` ไม่ได้ให้แจ้ง `E_MACHINE_READ` · ยังไม่ได้ใช้ธีมให้ข้าม · ไม่เคยแครชคือ `ok` แครชหนึ่งหรือสองครั้งเตือน `E_CMD_CRASHED` สามครั้งขึ้นไปเป็น `fail` ด้วย `E_CMD_CRASHED` ปุ่มแก้คือ "หยุดธีม" · อ่านรายงานแครชไม่ได้ (ปิดอยู่ ไม่มีสิทธิ์ หรือไม่ตอบในห้าวินาที) ให้ข้าม (`skipped`) พร้อมเหตุผล ห้ามบอกว่าไม่เคยแครช · recovery spec §4.5 เรียกข้อนี้ว่า D10 แต่ D10 คือข้อแอปที่ไม่ได้แพ็ก (phase 3b) |
```

and `- หลัง D1–D11 จะมีบรรทัดเพิ่ม` becomes `- หลัง D1–D12 จะมีบรรทัดเพิ่ม`.

- [ ] **Step 4: Appendix B** — add as the table's last row (if Task 1 took F5, the third bullet reads `• "ถึงพร้อมท์แรก" ดูจากบรรทัดแรกที่พิมพ์ (\`clink.onendedit\`) เพราะ prompt filter หลังของ Oh My Posh ไม่ได้เห็นพร้อมท์ของมัน (spike S6, fallback F5) หน้าต่างที่ปิดตอนพร้อมท์แรกโดยไม่กด Enter จึงนับว่าเปิดไม่ถึง` instead):

```markdown
| 2026-10-17 | Phase 3b-1 โหมดกู้คืนฝั่ง cmd และการหยุดธีม (architect เลือก founder เปลี่ยนได้ แผนอยู่ใน `docs/superpowers/plans/2026-10-17-phase3b-1-recovery-cmd.md` ตาม `docs/superpowers/specs/2026-10-17-recovery-mode-design.md` ผล spike และการตรวจบน VM อยู่ใน `docs/spikes/phase3b-recovery/`):<br>• ข้อตรวจใหม่ชื่อ D12 (recovery spec §4.5 เรียก D10 แต่ D10 เป็นข้อแอปที่ไม่ได้แพ็กตั้งแต่ phase 1b) และมีรหัสใหม่ `E_THEME_PAUSED` กับ `E_CMD_CRASHED` ใน §5.9 เพราะทุกข้อของ doctor ที่เตือนหรือล้มเหลวต้องมีรหัส<br>• ตัวนับใน init.lua นับเฉพาะหน้าต่าง cmd (`os.gethost()`) เพราะ `clink set` ของ CMD Studio เอง (การตรวจหลังใช้ธีม และ D3 ที่หน้าแรกรันทุกครั้งที่เปิด) โหลด init.lua ด้วยและไม่มีพร้อมท์ ถ้านับด้วย เปิดหน้าแรกสามครั้งธีมจะถูกหยุด<br>• "ถึงพร้อมท์แรก" ดูจาก prompt filter ของ CMD Studio ที่ทำงานหลังของ Oh My Posh ไม่ใช่ `clink.onbeginedit` เพราะ Clink ส่ง onbeginedit ก่อน prompt filter (spike S6)<br>• การตรวจรายงานแครชตอนเปิดแอปรันใน thread ของตัวเองหลังหน้าต่างขึ้น และ `recovery_status` รอผลไม่เกิน 30 วินาที จึงไม่ต้องมี event ใหม่ (`apply-progress` ยังเป็น event เดียว)<br>• การใช้ธีมที่สำเร็จทุกทาง (วิซาร์ด หน้าธีม สลับธีมเร็ว การแก้ของ doctor และ `cmdstudio apply`) ยกเลิกการหยุดธีมและลบป้าย · "ย้อนกลับทุกอย่าง" ของแอปยกเลิกการหยุดใน cmd ด้วย แต่การหยุดทั้งเครื่องยังอยู่ เพราะเก็บธีมที่จะเปิดกลับไว้ ส่วน `revert_all` ของ core ไม่แตะ `paused.json`<br>• เวลา "ตั้งแต่ใช้ธีม" คือ `applied_at_ms` ใน `state.json` ซึ่งเป็นเวลาตอนเตรียมการใช้ธีม ก่อน commit เล็กน้อย<br>• ทางลัด CMD Studio Recovery ในเมนูเริ่ม และ `--recovery` เป็นงานของ 3b-2 คู่มือในแท็บช่วยเหลือจึงบอกแค่ให้เปิด CMD Studio และกด "หยุดธีม" |
```

- [ ] **Step 5: Run the tests that read the spec**

Run: `cargo test -p cmdstudio-core --test error_codes` and `cargo test -p cmdstudio-app --lib -- error::`
Expected: every test passes (the new rows of §5.8 and appendix B start with `| D12` and `| 2026`, not `` | `E_ ``, so the codes table is unchanged). Read the diff of the spec: it changes only the places of Steps 1–4.

- [ ] **Step 6: Commit**

Stage `docs/superpowers/specs/2026-10-10-cmd-studio-design.md`. Message: `spec: phase 3b-1 in §5.3, §5.7, §5.8, §5.10 and appendix B (health\, the pause's IPC, D12)`.

---

### Task 12: The VM check of phase 3b-1 (the scripts here, the run by the controller)

Recovery spec §0 and §10's checks on real Windows, with the app CI built from this branch: the counter pauses the theme in cmd after three windows that never reach their prompt (an Oh My Posh that never answers stands in for one that hangs), and CMD Studio's own checks never count; three crashes of cmd in Clink pause it on this computer at the app's next start; "Pause the theme", D12's fix, "Turn the theme back on" and `--cleanup --data-dir` do what the spec says; the counter's cost per start in the shipped app. It drives the VMs with the phase 3 VM check's tools (`docs/spikes/phase3-vm-check/helpers.ps1` and its `.vbs` files, in the branch since phase 3), and adds only the hang stand-in and one guest script.

**PC safety, for this whole task:** the scripts written here are for the VMs and are never run on this PC, not even as a dry run (Global Constraints). The implementer checks `recovery-hang.ps1` and `guard.test.ps1` by parsing only (Step 5) and builds the hang stand-in in the scratchpad without ever starting it (Step 2). The controller runs Step 8 on the VMs.

**Files:**
- Create: `spikes/recovery/hang/Cargo.toml`, `spikes/recovery/hang/src/main.rs`
- Create: `spikes/vm/guest/recovery-hang.ps1`
- Modify: `spikes/vm/guest/guard.test.ps1` (`$changers`)
- Create: `docs/spikes/phase3b-recovery/vm-check.md` (the protocol; the controller fills its results)

**Interfaces:**
- Consumes: Task 1's `recovery-crash.ps1 -Swap|-Restore|-Events -Data <dir>`, `recovery-timing.ps1 -InstallMarker -Data <dir>`, `spikes/recovery/timing.lua`, the crash stand-in `clink_dll_x64.dll`, `timing-vm-a.txt`; `spikes/vm/vm.ps1` (`Copy-ToGuest`, `Invoke-GuestPs`, `Invoke-GuestDesktop`, `Send-Run`, `Save-Shot`, `$Vbm`); `docs/spikes/phase3-vm-check/helpers.ps1` (`Start-CleanVm`, `Install-Phase3App`, `Send-Key`, `Invoke-GuestButton`, `Get-GuestWindows`, `Get-GuestCmdWindows`, `Get-GuestAutoRun`, `Get-GuestText`, `Get-ExitCode`, `$P3`), its `start-app.vbs`, and its protocol's "Setup" and "Driving the app"; CI's artifact `cmdstudio-exe` of this branch's head.
- Produces: `recovery-hang.ps1 -Swap|-Restore [-Data <dir>]`; `docs/spikes/phase3b-recovery/vm-check.md` with PASS/FAIL per step.

- [ ] **Step 1: The hang stand-in**

`spikes/recovery/hang/Cargo.toml`:

```toml
[package]
name = "hang"
version = "0.0.0"
edition = "2021"
publish = false

# Not a member of CMD Studio's workspace: a test-only program for the VMs.
[workspace]
```

`spikes/recovery/hang/src/main.rs`:

```rust
//! Stands in for oh-my-posh.exe on a test VM (plan 3b-1 Task 12): it never answers, so a cmd
//! window whose init.lua runs `oh-my-posh init cmd` never reaches its prompt, as a cmd that
//! fails to start. Test VMs only: never started on the dev PC.
fn main() {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
```

- [ ] **Step 2: Build it into the scratchpad (building only, never started)** — PowerShell, in `<repo>\spikes\recovery\hang` (so `rust-toolchain.toml`'s 1.99.0 applies):

```powershell
$out = "<scratchpad>\hang"   # the session's scratchpad folder
New-Item -ItemType Directory -Force $out | Out-Null
$env:CARGO_TARGET_DIR = "$out\target"
cargo build --release
cargo clippy --release -- -D warnings
Remove-Item Env:CARGO_TARGET_DIR
Copy-Item "$out\target\release\hang.exe" "$out\oh-my-posh.exe"
```

Expected: both finish with no warning (checked while this plan was written); `$out\oh-my-posh.exe` exists. Do not start it.

- [ ] **Step 3: Write `spikes/vm/guest/recovery-hang.ps1`**

```powershell
# Runs inside a test VM (plan 3b-1 Task 12). -Swap puts the hang stand-in (spikes\recovery\hang,
# built on the host and copied to C:\cmdstudio\hang\oh-my-posh.exe) in the place of
# oh-my-posh.exe in CMD Studio's runtime under $Data, keeping the real one beside it as
# oh-my-posh.exe.real: a cmd window then never reaches its prompt, as one that fails to start.
# -Restore stops every stand-in still running and puts the real one back. Close CMD Studio and
# every cmd window before either.
param([switch] $Swap, [switch] $Restore,
  [string] $Data = (Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio'))
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script replaces Oh My Posh with a program that never answers" }
$omp = Join-Path $Data 'runtime\omp-31.5.0\oh-my-posh.exe'
$real = "$omp.real"
if ($Swap) {
  if (-not (Test-Path -LiteralPath $real)) { Copy-Item -LiteralPath $omp $real }
  Copy-Item 'C:\cmdstudio\hang\oh-my-posh.exe' $omp -Force
  "swapped: $((Get-FileHash -LiteralPath $omp).Hash)"
}
if ($Restore) {
  Get-Process oh-my-posh -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Seconds 1
  Copy-Item -LiteralPath $real $omp -Force
  Remove-Item -LiteralPath $real
  "restored: $((Get-FileHash -LiteralPath $omp).Hash)"
}
```

- [ ] **Step 4: List it in `spikes/vm/guest/guard.test.ps1`** — in `$changers`, after the line Task 1 added (`'recovery-setup.ps1' = @(); 'recovery-crash.ps1' = @('-Swap'); …`), add

```powershell
  'recovery-hang.ps1' = @('-Swap')
```

- [ ] **Step 5: Check both by parsing only (this PC)**

```powershell
$guest = 'spikes\vm\guest'
foreach ($s in 'recovery-hang.ps1', 'guard.test.ps1') {
  $t = $e = $null
  $ast = [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path "$guest\$s"), [ref]$t, [ref]$e)
  $first = ($ast.EndBlock.Statements | Select-Object -First 3 | ForEach-Object { $_.Extent.Text }) -join "`n"
  $guarded = $first -match 'COMPUTERNAME -notlike ''CMDSTUDIO-\*''\) \{ throw'
  "$s errors=$($e.Count) guarded=$guarded"
}
```

Expected: `recovery-hang.ps1 errors=0 guarded=True` (checked while this plan was written), `guard.test.ps1 errors=0 guarded=False`.

- [ ] **Step 6: Write `docs/spikes/phase3b-recovery/vm-check.md`**

````markdown
# Phase 3b-1 — VM check of the start counter, the crash check and the pause (controller)

- **The build:** only CI's artifact `cmdstudio-exe` of this branch's head (`cmdstudio.exe` + `resources\`), downloaded and checked as `docs/spikes/phase3-vm-check/protocol.md` "Setup" says (its `git diff --stat` must print nothing). Record the commit and the run id here.
- **VMs:** VM-A (`cmdstudio-win11`, `tester`) and VM-C (`cmdstudio-th`, the Thai user with a space), each from `clean`.
- **Tools:** `spikes/vm/vm.ps1` and `docs/spikes/phase3-vm-check/helpers.ps1`, dot-sourced in that order on the host. The app is driven with the keyboard and `Invoke-GuestButton` as that protocol's "Driving the app" says; a button that `Invoke-GuestButton` cannot press alone (the dialog's "Pause the theme" shares its name with Home's) is reached with `Send-Key $vm Tab` and pressed with `Send-Key $vm Enter`, a screenshot confirming the focus first.
- **Rules:** nothing here runs on the dev PC, not even as a dry run; every guest script refuses a machine whose name does not start with `CMDSTUDIO-`. Inside the VMs cmd runs without `/d` (AutoRun is under test). Do not repair a VM: record the step and go on. Close the app with `Invoke-GuestPs $vm 'Get-Process cmdstudio -ErrorAction SilentlyContinue | ForEach-Object { [void]$_.CloseMainWindow() }' $u`, never by killing it.
- **Date / commit / run:** <date> / <sha> / <run id>

## Setup (each VM)

```powershell
. .\spikes\vm\vm.ps1
. .\docs\spikes\phase3-vm-check\helpers.ps1
# $vm, $u, $data as phase3-vm-check/protocol.md "Setup" sets them (VM-C's user name as code points).
Start-CleanVm $vm $u -Type separate
Install-Phase3App $vm $u $artifact
$repo = (Resolve-Path .).Path   # whole paths: VBoxManage's working folder is not PowerShell's location
Copy-ToGuest $vm @("$repo\spikes\vm", "$repo\spikes\recovery") 'C:\cmdstudio\spikes\' $u
Copy-ToGuest $vm @("$repo\resources\clink") 'C:\cmdstudio\resources\' $u
Copy-ToGuest $vm @('<scratchpad>\crash-dll\clink_dll_x64.dll') 'C:\cmdstudio\crash-dll\' $u
Copy-ToGuest $vm @('<scratchpad>\hang\oh-my-posh.exe') 'C:\cmdstudio\hang\' $u
$g = 'C:\cmdstudio\spikes\vm\guest'
$dataArg = '-Data (Join-Path $env:LOCALAPPDATA ''ai.resincore.cmdstudio'')'   # read in the guest's own session
$startApp = { Send-Run $vm 'wscript.exe //B //Nologo C:\cmdstudio\phase3\check\start-app.vbs'; Start-Sleep -Seconds 10 }
$closeApp = { Invoke-GuestPs $vm 'Get-Process cmdstudio -ErrorAction SilentlyContinue | ForEach-Object { [void]$_.CloseMainWindow() }' $u | Out-Null; Start-Sleep -Seconds 5 }
$closeCmd = { Invoke-GuestPs $vm 'Get-Process cmd -ErrorAction SilentlyContinue | Stop-Process -Force' $u | Out-Null }
$health = { Invoke-GuestPs $vm "if (Test-Path -LiteralPath '$data\health') { Get-ChildItem -Force -LiteralPath '$data\health' | ForEach-Object { `$_.Name + ' = ' + (Get-Content -Raw -LiteralPath `$_.FullName).Trim() } } else { '(no health folder)' }" $u }
$logTail = { Invoke-GuestPs $vm "Get-Content -Tail 8 -Encoding UTF8 -LiteralPath '$data\logs\cmdstudio.log'" $u }
```

`& $health` lists `health\` with each file's text: "no tag" below means it lists no `start-…`. `Invoke-GuestPs` sends its command encoded, so `$data` may hold Thai; `Invoke-GuestDesktop`'s arguments may not, hence `$dataArg`.

## Steps (VM-A all; VM-C 1, 3, 3b, 4, 8)

| # | Do | PASS when | Result |
|---|---|---|---|
| 1 | `& $startApp`; the wizard's walk to Terracotta applied (every cmd window), as phase3-vm-check/protocol.md Run 2 walks it. Then `& $health`. | The apply commits; `health\` exists and lists no tag: the apply's own `clink set` check did not count. | |
| 2 | Visit Home, Settings, Home, Help, Home (`Send-Key $vm ShiftTab 5` from a page's heading reaches Home, then Enter; each Home visit runs doctor and D3 runs `clink set`). Then `Send-Run $vm 'cmd'`, wait 10 s, `Get-GuestCmdWindows $vm $u`, `& $closeCmd`. `& $health`. | No tag after the visits. The cmd window had Clink loaded and reached its prompt (screenshot: the box and the prompt), and left no tag. | |
| 3 | **Hang.** `& $closeApp; & $closeCmd`; `Invoke-GuestDesktop $vm "$g\recovery-hang.ps1" '-Swap' $u`. Three times: `Send-Run $vm 'cmd'`, wait 10 s (`Save-Shot $vm 'p3b1-hang'` after the first). `& $closeCmd; & $health`. Wait until 65 s have passed since the third window opened, then `Invoke-GuestDesktop $vm "$g\recovery-hang.ps1" '-Restore' $u`. `Send-Run $vm 'cmd'`, wait 10 s, `Save-Shot $vm 'p3b1-paused-now'`; `Send-Text $vm 'cmd' -Enter`, `Save-Shot $vm 'p3b1-paused-child'`; `Send-Run $vm 'cmd'`, wait 10 s, `Save-Shot $vm 'p3b1-paused-still'`. `& $health`. | The three windows never show a prompt; `& $health` lists three `start-<pid> = v1 <secs>`. The fourth window opens plain (cmd's own `C:\Users\…>` prompt, no box) with exactly one line in the window's colors, `CMD Studio paused its theme because cmd failed to start 3 times. Open CMD Studio to turn it back on.`; the `cmd` typed in it prints no second line; the next window prints `CMD Studio's theme is paused. Open CMD Studio to turn it back on.` `health\` holds only `paused.json = {"version":1,"reason":"cmdStarts","at":<ms>,"count":3,"by":"clink"}`. | |
| 3b | `& $closeCmd; & $startApp`; `Save-Shot $vm 'p3b1-banner-cmd'`. Help → Health check (`Save-Shot`). Home → `Invoke-GuestButton $vm 'Turn the theme back on' $u`; `Save-Shot`. `Send-Run $vm 'cmd'`, wait 10 s, `Save-Shot`. `& $health`. | Home's banner "The theme is paused": "cmd failed to reach its prompt 3 times in a row, so CMD Studio paused the theme in cmd.", "Since <time>", "Only cmd windows open without it: nothing on this computer was changed.", and the buttons "Turn the theme back on", "Undo everything", "Report a problem"; no "Pause the theme". D12 is `warn` `E_THEME_PAUSED` with the button "Turn the theme back on". After the button: "The theme is back on. Open a new cmd window to see it." and no banner; the new window has the box and the prompt; `health\` lists nothing. | |
| 4 | **Crash.** `& $closeApp; & $closeCmd`; `Invoke-GuestDesktop $vm "$g\recovery-crash.ps1" "-Swap $dataArg" $u`. Three times `Send-Run $vm 'cmd'`, 15 s apart (`Save-Shot $vm 'p3b1-crash'` after the first). `& $closeCmd`; `Invoke-GuestDesktop $vm "$g\recovery-crash.ps1" "-Restore $dataArg" $u`; `Invoke-GuestDesktop $vm "$g\recovery-crash.ps1" "-Events $dataArg" $u` (save its output as `vm-check-crash-<vm>.txt`). `& $startApp`, wait 30 s, `Save-Shot $vm 'p3b1-banner-crash'`. `Get-GuestAutoRun $vm 'after-crash-pause' $u`; `& $logTail`; `Send-Run $vm 'cmd'`, wait 10 s, `Get-GuestCmdWindows $vm $u`, `Save-Shot`. | Each window crashes (Windows Terminal says the process exited with `0xc000001d`, or the window closes); `-Events` counts 3 and the app's query finds them. Home's banner: "cmd crashed in its add-on 3 times, so CMD Studio paused the theme on this computer." and "CMD Studio took its changes out of this computer. Turning it back on applies Terracotta again."; no "Undo everything"; the theme card says "No theme is applied yet." AutoRun holds no CMD Studio part (`AutoRun: (absent)` on a clean VM). The log has `crash_check crashes=3 paused=true` and `pause reason=clinkCrash count=3 revert=ok saved=true`. The new cmd window is plain, prints no line, and has no Clink loaded. | |
| 4b | `Invoke-GuestButton $vm 'Turn the theme back on' $u` → the plan → Apply → Close. `& $closeApp; & $startApp`; `& $logTail`. | The apply commits; the banner goes; a new cmd window is themed. After the restart there is no banner, and the log has `crash_check crashes=0 paused=false` (the three crashes are before this apply). | |
| 5 | Home → `Invoke-GuestButton $vm 'Pause the theme' $u`; `Save-Shot` (the question); the dialog's own "Pause the theme" by Tab and Enter; `Save-Shot`; Close. `Send-Run $vm 'cmd'`, wait 10 s, `Save-Shot`. Then "Turn the theme back on" → Apply → Close. | The dialog asks before anything changes ("CMD Studio takes its changes out of this computer, …"); then "The theme is paused: cmd windows open without it."; the banner says "You paused the theme."; the cmd window is plain; turning back on applies Terracotta through the plan. | |
| 6 | **D12's fix.** `& $closeApp; & $closeCmd`; crash once as in 4 (one `Send-Run`), restore. `& $startApp`; Help → Health check, `Save-Shot`. Press D12's "Pause the theme" (`Invoke-GuestButton`), then the dialog's by Tab and Enter, Close. Help → Health check again. Home → "Turn the theme back on" → Apply → Close. | No pause at start (one crash is under three). D12 is `warn` `E_CMD_CRASHED`, "cmd crashed in its add-on 1 time(s) since the theme was applied…", with the button "Pause the theme". After it, D12 is `warn` `E_THEME_PAUSED`, and Home's banner says "cmd crashed in its add-on 1 times, so CMD Studio paused the theme on this computer." The theme goes back on through the plan. | |
| 7 | **Cost.** Theme applied, `& $closeApp; & $closeCmd`: `Invoke-GuestDesktop $vm "$g\recovery-timing.ps1" "-InstallMarker $dataArg" $u` (save as `vm-check-timing-<vm>.txt`). Then `Invoke-GuestPs $vm "Remove-Item -LiteralPath '$data\generated\zz-timing.lua'; Remove-Item -Path '$data\first-prompt-*.txt'" $u`. | Ten times and their average, against Task 1's number without the counter (`timing-vm-a.txt`): at most 5 ms more is the spec's target; more is recorded for the founder (fallback F7), not a FAIL. The marker files are gone. | |
| 8 | **Cleanup.** In the app press "Pause the theme" as in 5 (so `health\paused.json` is there); `& $closeApp; & $closeCmd`. `Get-ExitCode $vm 'cleanup' '--cleanup --data-dir @LOCALAPPDATA@\ai.resincore.cmdstudio' $u`. `Invoke-GuestPs $vm "Get-ChildItem -Force -LiteralPath '$data' \| ForEach-Object Name" $u`. | `exit=0`; nothing is left but `themes\` (or nothing): `health\` was taken for CMD Studio's own. | |
| 9 | Help → Short guide, in English, then in Thai (Settings → ไทย, as phase 3's protocol does it). `Save-Shot` each. | The last paragraph is "If cmd won't open: open CMD Studio. …" / "ถ้า cmd เปิดไม่ขึ้น: เปิด CMD Studio …". | |

On VM-C, steps 3 and 3b are also spike S2 in the shipped app (the tags and `paused.json` under the Thai user's folder), and step 4 is S9 on Thai Windows.

## The steps that stay visual

The one line of a paused window and its colors (3), the banner's words (3b, 4, 5, 6) and the guide's paragraph (9): read the screenshot and attach it. Each still has its printed line: `& $health`, `& $logTail`, `Get-GuestAutoRun`, `Get-GuestCmdWindows`, `Get-ExitCode`.

## Result

PASS/FAIL per step in the table, with the images `p3b1-*.png` beside this file and the saved outputs (no user name of the dev PC in any text). Phase 3b-1 is done when VM-A 1–6, 8, 9 and VM-C 1, 3, 3b, 4, 8 pass; step 7 is recorded. A FAIL goes back to the task that owns the code before the PR.
````

- [ ] **Step 7: Commit the scripts and the protocol**

Stage `spikes/recovery/hang/Cargo.toml spikes/recovery/hang/src/main.rs spikes/vm/guest/recovery-hang.ps1 spikes/vm/guest/guard.test.ps1 docs/spikes/phase3b-recovery/vm-check.md`. Message: `spikes: the phase 3b-1 VM check, with an Oh My Posh that never answers (recovery spec §0, §10)`.

- [ ] **Step 8: Run it on VM-A, then VM-C (the controller)** — with CI's artifact of the head commit, as the protocol says. First, on VM-A, `Invoke-GuestDesktop $vm "$g\guard.test.ps1" '' $u` (the guard test runs on a VM only; it is never run on the dev PC): PASS when its last line is `guest guards: 29/29 checks pass` (thirteen listed scripts, `recovery-hang.ps1` among them, each guarded and refusing a machine name that is not a test VM's). Fill the Result column, add the images and outputs, and restore each VM's `clean` snapshot at the end. Commit: stage `docs/spikes/phase3b-recovery/`. Message: `spikes: phase 3b-1 VM check on VM-A and VM-C`.
