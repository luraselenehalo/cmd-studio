# Phase 2a — Preview Emulator, Parity Tests and the Twelve Presets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

## Revision (2026-10-13)

This is the plan of 2026-10-12 (`.superpowers/sdd/phase2a-plan.md`) revised against two things that happened after it was written. Every place that moved carries a `> **Revised 2026-10-13:**` note saying what and why; everything else is as it was. The code of every task was compiled and run again on the shipped code (see "Checked before this revision").

1. **Phase 1b merged** (PR #4, `ce878f1`), and with it the adapters, transactions, doctor, the `cmdstudio` CLI and CI. The plan was compiled against `05d4968` (Phase 1a only) and predicted "Phase 1b's counts plus ours". It is now compiled against **main `4a86f6b`**, the commit `phase2a-preview` is cut from: every "replace" quotes the text shipped there, and every count is measured there (core unit tests 624, not 323; `golden` 17, not 18; `hostile_themes` 32; the gate's totals in Task 11).
2. **The founder's language rule** (2026-10-13, PR #5, `4a86f6b`): "ภาษาหลักเป็นภาษาอังกฤษนะ ห้ามเป็นภาษาไทยใน CMD เด็ดขาด แต่หน้าโปรแกรมเรามีภาษาไทยได้ หน้าปรับแต่งอ่ะ". Nothing CMD Studio writes into a cmd window is Thai; English is the main language; Thai only in the app's own UI (spec §1, §3.6, §5.7, §5.9, appendix B 2026-10-13). Shipped: `generate::lua::banner_lua(&Theme)` is English only (no `Lang`, no `DATE_TH`, no `TIPS_TH`; `DateStyle` holds only `DATE_EN` with `year_offset: 0`), the CLI is English only, the golden `terracotta.banner.th.lua` is gone and `tests/english_in_cmd.rs` guards the generators. The preview shows exactly what cmd shows, so it is English too. What moved:
   - Decision 1 and Task 6: `render_banner(&Theme, Lang, &PreviewContext)` is `render_banner(&Theme, &PreviewContext)`; `DateStyle::of(lang)` and `tips(lang)` go; `DATE_EN`, `TIPS_EN`, `ICON` and `ICON_FALLBACK` are shared as `pub(crate)` constants; Task 6's replacements quote `lua.rs` at `4a86f6b`.
   - Task 6's tests: `thai_has_the_buddhist_year_and_thai_tips` and the Thai rows of `the_date_is_written_the_same_in_rust_and_in_lua` become English tests (`the_box_is_english_and_thai_shows_only_where_the_user_put_it`); a new guard `the_preview_of_every_preset_holds_no_thai_character` in `tests/english_in_cmd.rs` checks the box and the prompt of every preset in every layout in the ASCII sample. The tests with Thai in the user name, the folder, the git branch and the theme's text stay everywhere (Tasks 2, 3, 4, 6, 7): that is the user's own data, shown as it is, and they prove the cell counting (Review Focus 1).
   - Task 7: the banner parity cases are every preset × 9 banners × 4 samples (no language axis), and every day of 2024 in the one English style.
   - Task 10: the four goldens are `<preset>.preview.txt` (no `.en`/`.th`), all English; the given content of `terracotta.preview.txt` is unchanged apart from the name.
   - Decision 6 and Task 11's spec text: Phase 2b's VM check on VM-C shows an English welcome box whose user name and folder are Thai (user data), which must still line up; §5.1's tree no longer says the emulator is in the UI; the appendix B rows of Tasks 8 and 9 are dated 2026-10-13 so the table stays in order.
   - A Global Constraint and a Review Focus item (6) record the rule.
3. Smaller corrections found while re-verifying: the gate's `cargo doc` wording (the shipped code already prints two rustdoc warnings from Phase 1b, which this phase does not touch), the measured parity run time, the dev machine's host name no longer quoted, the Phase 1a constraints file named by its real place.

**Goal:** Give `cmdstudio-core` a preview emulator that draws the prompt and the welcome box exactly as Oh My Posh 31.5.0 and the generated `banner.lua` print them, prove it cell by cell and color by color against the real programs, and ship the other 11 presets of spec §3.2.

**Architecture:** A new module `cmdstudio_core::preview` turns a theme and a `PreviewContext` (the sample values the app picks: user, host, folder, clock, git, last exit code and duration, window width) into lines of styled spans `{text, fg, bg, bold}`; the app (Phase 2b) only paints them. The emulator does not keep its own copy of the prompt's design: `generate::omp` now builds its JSON from one shared table (`drawn`, `left_parts`, `Swatch`, the frame characters), and the emulator reads the same table; the banner reads `generate::lua`'s date tables, tips, icons and template cutter. `tests/parity.rs` runs the pinned `oh-my-posh.exe` on the generated config for every layout × every set of segments (plus git, status, duration, width, path, user and preset variations), reads the SGR colors back into spans and compares; `tests/banner_parity.rs` runs the generated `banner.lua` in Lua 5.2 and compares. The presets are `.cmdtheme` files embedded by `presets::builtin()`.

**Tech Stack:**
- Rust `1.99.0` (pinned by `rust-toolchain.toml`), edition 2021. Already in the crate: serde 1, serde_json 1, thiserror 2, uuid 1 (`v4`), schemars 1; dev: mlua 0.10 (`lua52`, `vendored`), proptest 1.
- New: `unicode-width = "0.2"` — 0.2.2 is the newest on crates.io (checked with `cargo info unicode-width`, 2026-10-12, and again 2026-10-13), license MIT OR Apache-2.0. Used: `UnicodeWidthChar::width` and `UnicodeWidthStr::width`. The crate was already in the dev machine's registry cache, so the 2026-10-13 check compiled it without a download.
- The parity test runs Oh My Posh **31.5.0** from `resources\omp\oh-my-posh.exe` (SHA-256 `9dd2c1297d5ae89f23f19b02f74a25510fcb56d84da5139fa3c3ca6e6d008a73`, put there by Phase 1b's `scripts/fetch-resources.ps1`) and git 2.28 or later (`git init -b`).

**Spec:** `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (Thai; binding): §1 and §3.6 (English in cmd), §3.2, §3.4 (readability), §3.5 (prompt), §3.6 (welcome box), §5.6 (preview), §7 (parity test). Controller decisions: `.superpowers/sdd/phase2-3-design-brief.md` (the emulator lives in Rust; preset decisions D1–D7) and `.superpowers/sdd/presets-research/README.md` (upstream values and sources). Earlier phases' rulings: `docs/superpowers/plans/2026-10-10-phase1a-core-rulings.md` and `docs/superpowers/plans/2026-10-11-phase1b-system-rulings.md` (its last ruling is the language rule).

**Prerequisite:** main at `4a86f6b`: Phase 1b merged (PR #4, `ce878f1`: `scripts/fetch-resources.ps1`, CI, adapters, transactions, doctor, the CLI) and the English-only change (PR #5, `4a86f6b`: `generate::lua` English only, the CLI English only, `tests/english_in_cmd.rs`, spec §1/§3.6/§5.7/§5.9). Phase 2a works on the branch `phase2a-preview`, cut from `4a86f6b` and already checked out. Neither PR touches `generate/omp.rs` or `presets.rs`; `generate/lua.rs`, `tests/golden.rs` and `tests/english_in_cmd.rs` are quoted below as `4a86f6b` has them. The shipped code is the ground truth for every interface this plan consumes.

> **Revised 2026-10-13:** the section below replaces "Checked before this plan was written (2026-10-12)": the same checks run again on `4a86f6b` with the revised code. The 2026-10-12 run on `05d4968` is kept as history at the end.

**Checked before this revision (2026-10-13):**
- Every code block of Tasks 1–10, as revised, was compiled and run together on the dev machine in a scratch copy of `4a86f6b` (`git archive`, with its own `CARGO_TARGET_DIR`), the eleven preset files included: **670 core unit tests** (`4a86f6b`'s 624 and this plan's 46), and the integration tests `banner_lua` 30, `banner_parity` 3, `english_in_cmd` 2, `error_codes` 2, `golden` 21 (17 + 4, the four preview goldens written with `UPDATE_GOLDEN=1` in the scratch copy only), `hostile_themes` 32, `omp_render` 1 (its gate unset, so it printed `SKIPPED`), `palette_props` 1, `parity` 2, `schema_file` 11, and 2 doc tests — all passing. `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings` and `cargo fmt --all -- --check` were clean. `cargo doc -p cmdstudio-core --no-deps` printed the same two warnings as `4a86f6b` itself does (`pending_incomplete` links to the private `latest_record`; `fonts_left` links to the private `SystemExecutor::font_not_ours` — Phase 1b's docs, untouched here) and none from the preview.
- `tests/parity.rs` ran against the pinned binary (its SHA-256 checked: `9dd2c129…6d008a73`): **746 cases matched Oh My Posh 31.5.0**, in 172 s on this run (the 2026-10-12 runs took 22–75 s; the machine was busier). `tests/banner_parity.rs` failed when the lines' indent lost a space (`terracotta, as the preset has it, sample 0: line 1 of 8/8`, the emulator's top line one `─` short), and passed again after the change back.
- Nothing was written outside the tests' temp folder: `%LOCALAPPDATA%\oh-my-posh` of the dev machine was listed with timestamps before and after and the listings are identical (15 files, none newer than 2026-10-12), and no `cmdstudio-parity-*` folder was left in `%TEMP%`.
- The four preview goldens the scratch run wrote hold no Thai character; `terracotta.preview.txt` is exactly the content Task 10 Step 3 gives.
- Not run again: the breathing-room and branch-icon mutations of Task 4 (the code of Tasks 1–5 is unchanged from 2026-10-12, when they failed 2 and 114 cases).

History, 2026-10-12 (on `05d4968`, Phase 1a only): 259 core unit tests (213 + 46), `banner_lua` 30, `banner_parity` 3, `golden` 20, `hostile_themes` 28, `palette_props` 1, `parity` 2 (746 cases, 22–75 s with 8 workers), `schema_file` 10, 2 doc tests; clippy, fmt and doc clean; making the breathing room 4 cells failed 2 parity cases (the two boundary widths), dropping powerline's branch icon failed 114; `banner_parity` failed on a lost indent space and a weekday off by one; `%LOCALAPPDATA%\oh-my-posh` got no new file.

**What was measured on the real Oh My Posh 31.5.0** (the pinned binary above; `oh-my-posh version` printed `31.5.0`. 31.7.0 is also installed on the dev machine; nothing in this plan was taken from it):
1. `oh-my-posh print --help` lists `--status int` ("last known status code"), `--execution-time float` ("timing of the last command"), `--terminal-width int`, `--pwd string`, `--shell string`, `--no-status`, `--plain`, `-c/--config` and others. There is no flag for git: the git segment reads the repository at `--pwd`.
2. `print primary --shell cmd` writes colors as SGR only: `ESC[38;2;r;g;bm`, `ESC[48;2;r;g;bm`, `ESC[0m`, `ESC[49m`, a combined `ESC[38;2;r;g;b;49m`, and `ESC[7m` (reverse video). Blocks on separate lines are joined by LF alone. The window title (`console_title_template`) comes as `ESC]0;<folder>BEL` right before the final space. No cursor movement anywhere.
3. Powerline: the first powerline segment of a line gets a leading U+E0B0 written as `ESC[0m ESC[38;2;<its bg>;49m ESC[7m U+E0B0` — the terminal's background on the segment's color. Between two powerline segments the arrow is the left one's color on the right one's; after the last one, its color on the terminal's background. A hidden segment leaves no arrow.
4. The box's right side: the filler fills `width − written − right` cells when at least 5 stay free; otherwise `width − right` (as if on a new row); a left side wider than the window counts from its last row (`width − written mod width`); never negative. Measured at widths 1–200 (e.g. 32 → 23 cells of filler, 33 → 5, 18 → 8, 9 → 0). A width of 0 is not a width (Oh My Posh then guesses 120).
5. Cells are counted code point by code point: Thai vowel and tone marks 0, Chinese and most emoji 2, ZWJ and U+FE0F 0, ambiguous characters (`·`, `×`, `①`) 1 — measured through the filler with paths of each.
6. A segment is hidden, spaces and all, when git is outside a repository, the status is 0, or the duration is under 2000 ms. `--execution-time 2000` shows `2s`; `1999.9` shows nothing (the fraction is cut off).
7. `austin`: `2s`, `2.001s`, `2.01s`, `2.1s`, `59.999s`, `1m 0s`, `1m 0.001s`, `1h 0m 0s`, `1d 1h 1m 1.001s`.
8. The path (`style: full`, `home_icon: ~`): the home folder in any case becomes `~`; `/` becomes `\`; the drive letter is upper case; a trailing `\` goes (`D:\work\` → `D:\work`), except a drive's root (`D:` → `D:\`); `.` and `..` stay. The folder does not have to exist.
9. `session`: the user name is `USER`, else `USERNAME`; the host name is Go's `os.Hostname()` (lower case on the dev machine, whose `COMPUTERNAME` is the same name in upper case; the parity test reads it from Oh My Posh instead of guessing).
10. Git: an untracked file and a staged-only change both count as changed (`*`, `p:dirty`); a branch named `feature/x-ทดสอบ` prints as it is; powerline keeps Oh My Posh's own branch icon U+E0A0, with no space after it.
11. `print right` of the box prints only `│` in the frame color; line and powerline print nothing. `print transient` prints `> ` in the accent with no final space and no title.
12. Each run writes `%LOCALAPPDATA%\oh-my-posh\cmd.<POSH_SESSION_ID>.omp.cache` (a new random id per run when the variable is unset; 50 KB each) and `omp.cache`.

**Read in Clink 1.9.34's source, not run** (`chrisant996/clink` at tag `v1.9.34`): the right prompt ends in the window's last column and shows only while `m_lastcol + 1 + rprompt_width < screen_width`, with `m_lastcol` the column after the input (`clink/lib/src/display_readline.cpp`, `can_show_rprompt`, `print_rprompt`); on Windows 10 and 11 a combining mark has no width and joins the cell before it, and with color emoji a joined emoji sequence is 2 cells (`clink/terminal/src/wcwidth.cpp`, `wcwidth_iter.cpp`). A real Clink 1.9.34 run on the dev machine on 2026-10-11 (a hidden **conhost** window, the scratchpad's `cellprobe`) counted each Thai mark as one cell (`console.cellcount("มี.ค.")` = 5): conhost draws each mark in a cell of its own. How Clink counts in a Windows Terminal tab was not run here; see Decision 6.

## Decisions this plan takes

The architect's, made where the spec and the brief are silent; the founder may change any of them.

1. **The emulator is a Rust module, `cmdstudio_core::preview`** (brief: "Preview emulator lives in Rust"). Its API: `prompt(&Theme, &PreviewContext) -> Prompt` (the three things Oh My Posh prints), `render_prompt(&Theme, &PreviewContext) -> Vec<Line>` (the rows of the window), `render_banner(&Theme, &PreviewContext) -> Vec<Line>`. Spec §5.6 says the emulator is "in the UI"; Task 11 changes that sentence (see "Spec conflicts"). *Revised 2026-10-13:* `render_banner` took a `Lang`; the welcome box is English on every Windows now (PR #5), so there is nothing to choose.
2. **One description of the prompt for the generator and the emulator.** `generate::omp` gets a small shared table (`Swatch` for the palette's names, `Part`, `Look`, `Drawn`, `drawn()`, `left_parts()`, `has_time()`, `prompt_char()` and the frame characters as constants) and builds its JSON from it; the JSON it writes does not change by one byte (the goldens hold it). The emulator draws from the same table, so a color or a space changed in the generator moves the preview too, and the parity test checks the result against the real Oh My Posh.
3. **Spans.** A `Line` holds `spans: Vec<Span>` with `Span { text, fg: Rgb, bg: Option<Rgb>, bold }`, and serializes as a plain JSON array of camelCase objects. `bg: None` is the terminal's own background. Reverse video is resolved: a span is what the terminal draws. `Line::push` joins text of the same style and drops empty text, so two lines that look the same are equal; the parity test reads Oh My Posh's output through the same `push`. `bold` is always false today (nothing we generate is bold); the parity test would see it if Oh My Posh ever wrote it.
4. **`PreviewContext`.** `cwd` is the real folder (`C:\Users\you\projects\cmd-studio`) and `home` the profile folder: the prompt shows `~\projects\cmd-studio` as Oh My Posh does, and the welcome box's `{cwd}` shows the folder as `os.getcwd()` does. One `host` serves both, although a real window shows `COMPUTERNAME` in the box and the DNS host name in the prompt. The day of the week is computed from the date (Sakamoto's method), so a context cannot hold a wrong one. `windows_terminal` picks the box's icon (`✻` or `*`, spec §3.6). `command` is what was typed at the prompt before.
5. **The window's rows.** `render_prompt` shows the earlier command (shrunk to `> ` with the transient prompt on; with it off, the earlier prompt in full as in a fresh window: exit code 0, no duration), then the empty line cmd prints before every prompt after a command, then the prompt now, with the right prompt where Clink 1.9.34 puts it; every row is wrapped at the width. Without an earlier command the transient setting would change nothing in the preview.
6. **The welcome box counts cells as Clink does in Windows Terminal** (`banner_cells`: Unicode width of the whole string — a Thai mark 0, a joined emoji 2), from Clink's source; conhost counts a Thai mark as 1 (seen on the dev machine). The preview shows Windows Terminal. The banner parity test gives the Lua stand-in the same count, so it holds the script's layout, colors, dates, tips and icon; whether Clink in a real Windows Terminal tab counts the same is not checked here — Phase 2b's VM checks should show the welcome box on VM-C with its Thai user name and a Thai folder in it (noted in the spec by Task 11). *Revised 2026-10-13:* the box itself is English (PR #5); the Thai in it is the user's name and folder, which are user data and still must line up, so the cell-count tests of Tasks 2, 6 and 7 stay as they were.
9. **The preview is English, as cmd is** (founder, 2026-10-13; spec §1, §3.6). Everything the emulator draws on its own — the frame, the date, the tips, the prompt's marks — is English; Thai appears only where the user's data holds it (the user name, the computer name, the folder, a git branch, the theme's banner text), shown as it is. Guards: `the_box_is_english_and_thai_shows_only_where_the_user_put_it` (Task 6) and `the_preview_of_every_preset_holds_no_thai_character` in `tests/english_in_cmd.rs` (Task 6, over every preset as Tasks 8 and 9 add them). The goldens of Task 10 hold no Thai character.
7. **The parity test fails without the binary**, saying how to get it, unless `CMDSTUDIO_ALLOW_PARITY_SKIP=1` is set; then it prints `SKIPPED` and passes. CI never sets it (Phase 1b's CI already fails a run that prints `SKIPPED` in its system-test step). It needs git as well. The clock is read from Oh My Posh before and after each batch of 16 cases and the batch runs again if the minute changed; the host name is read from Oh My Posh once. Each worker thread has its own cache folder and session id inside the temp folder.
8. **Presets.** D1–D7 as the brief decides them (values in Task 8). Every preset's `author` is `CMD Studio` (the copyright lines are in `docs/credits.md`), its banner is Terracotta's, `transient` is on, `cursorShape` is `filledBox` (the only shape G6 has seen in a real window), `fontSize` 12, `padding` [14, 10]; dark presets `opacity` 96 with acrylic, light ones 100 without. Layouts, for variety: Tokyo Night and One Dark `powerline` (so their font is `CaskaydiaCove NF`), Catppuccin Mocha, Gruvbox, Catppuccin Latte and Daylight `box`, Dracula, Nord, Retro green, Solarized Light and Paper `line`. Our own three are `palette::generate` plus hand tuning (Task 9); their colors pass the readability check.

**Spec conflicts and how this plan resolves them**
- §3.5 says `duration` shows when a command takes **more than** 2 seconds; Oh My Posh 31.5.0 with the generator's `threshold: 2000` shows it **from** 2000 ms (fact 6). The emulator follows Oh My Posh; Task 11 changes the spec's words to "ตั้งแต่ 2 วินาที".
- §5.6 says the emulator is in the UI; the brief puts it in Rust and §5.6's last line forbids prompt logic in TypeScript. Task 11 rewrites §5.6 and adds `preview` to §5.2's module table.
- §3.2 says an upstream preset uses the project's values for every color; D1 takes Tokyo Night's editor background and text instead of its terminal block, and D7 takes another of the project's colors for `muted` where the comment color is unreadable. Every value is still the project's own; Task 8 records the choices in appendix B and the test lists the 14 contrast exceptions that remain.

## Global Constraints

Every task's requirements include these. Values are copied from the spec, the Phase 1a plan's Global Constraints (`docs/superpowers/plans/2026-10-10-phase1a-core.md`), the Phase 1b plan and the Phase 1b rulings where they bind here.

- Crate `cmdstudio-core`, Rust edition 2021, license MIT. Pinned runtimes: Oh My Posh `31.5.0`, Clink `1.9.34`.
- Language (founder 2026-10-13; spec §1, §3.6, appendix B 2026-10-13): nothing CMD Studio writes into a cmd window is Thai, and the preview shows what cmd shows, so no text the generators or the emulator write on their own holds a Thai character — not in a golden, a test expectation or a doc example of generated output. Thai stays where it is the user's data (names, paths, branch names, the theme's own texts) and in the app's UI (Phase 2b, not this plan). `tests/english_in_cmd.rs` guards both the generated files and the preview; it must pass after every task.
- Production code (everything outside `#[cfg(test)]` and `tests/`): no `unwrap`, `expect`, `panic!`, unchecked indexing or slicing, string slicing, or `serde_json::Value` IndexMut; the workspace denies clippy `unwrap_used`, `expect_used`, `panic`, `indexing_slicing` and `string_slice` (ruling R1). Errors are typed. Division and `%` only by a value proven non-zero.
- Test code: `clippy.toml` exempts `#[test]` functions and `#[cfg(test)]` modules from unwrap/expect/panic/indexing, but not top-level helpers in `crates/core/tests/*.rs` and never `string_slice`. Integration test files start with `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]`; a `&str` is cut with `split_at` at a found index or `get(..)`, never `&s[a..b]`.
- Tests never touch the machine outside their own temp folder. They may read repo files (`presets/`, goldens, `resources/`); they write repo files only behind `UPDATE_GOLDEN=1`. Child processes get `CREATE_NO_WINDOW` (spec §2.2 lesson 7: never steal focus).
- Oh My Posh config: `"version": 4`; segment types only `path git time session status executiontime text`, never `command`; `text` holds only the generator's own frame characters (spec §3.5, §3.7 #2). The goldens of the configs must not change in this phase.
- Readability (spec §3.4): foreground ≥ 4.5:1 against the background; muted, accent, ansi 1–6 and 9–14 ≥ 3:1. Our own presets pass it; an upstream preset keeps every upstream value and each failure is listed in a test (spec §3.2).
- Preset ids: canonical UUID v4, `8-4-4-4-12` lower-case hex, third group starting `4`, fourth group starting `8`, `9`, `a` or `b` (spec §3.3). Names: no company brand names (spec §3.2).
- Fonts (brief): a preset uses `Cascadia Mono` unless its layout is `powerline`, then `CaskaydiaCove NF`.
- JSON CMD Studio writes uses camelCase field names (ruling P21).
- Palette, contrast and prompt logic live in Rust; the UI never repeats them in TypeScript (spec §5.6).
- Implementation Gate (spec §0): before calling an external crate's API, read it for the version in `Cargo.lock` (`cargo doc -p unicode-width --no-deps --open`, or its source under `~/.cargo/registry/src`); if a signature differs from this plan, follow the real API and say so in the commit message.
- Each task ends clean: `cargo fmt --all`, then `cargo clippy --workspace --all-targets -- -D warnings` and, as Phase 1b's CI also runs it, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`. `cargo doc -p cmdstudio-core --no-deps` prints no warning from this phase's code (a public doc may not link a private item). *Revised 2026-10-13:* `4a86f6b` itself prints two such warnings from Phase 1b's docs (`pending_incomplete` → `latest_record`, `fonts_left` → `SystemExecutor::font_not_ours`); they are not this phase's to fix and are reported as a deferred minor, so "clean" means those two and no other.
- Scratch copies and mutation runs use their own `CARGO_TARGET_DIR`, never the repository's `target/`.
- Commits: write the English message to `.git/CMDSTUDIO_COMMIT_MSG` with the Write tool, then `git commit -F .git/CMDSTUDIO_COMMIT_MSG`. Stage only the task's files with `git add -- <files>` and check `git diff --cached --name-only`. Never add Co-Authored-By, "Generated with" or any Claude/claude.ai line; never `--no-verify`; never push. Follow the `auroraz-pr-workflow` skill.
- `CMDSTUDIO_ALLOW_PARITY_SKIP` is never set in CI or in a script that CI runs.

## Review Focus

The six kinds of input the spec implies but no requirement spells out that are most likely to bite someone using the preview. Each has a test in the task that owns the code.

1. **Thai letters with their marks, Chinese and emoji in the user name, the folder or the banner text** (user data, which the language rule does not touch). The box's filler must count cells the way Oh My Posh does (a Thai mark takes none), and the welcome box's frame the way Clink does, or the right edge of either lands in the wrong place. Pinned in Task 2 (`prompt_cells_count_thai_marks_as_nothing_and_wide_characters_as_two`, `banner_cells_join_emoji_sequences_like_clink_in_windows_terminal`), Task 4 (the parity cases with Thai, Chinese and emoji paths and the Thai user) and Task 6 (`the_frame_pads_by_cells_so_thai_marks_and_emoji_line_up`).
2. **A preview narrower than the prompt.** The editor's preview pane can be narrow and a path long: Oh My Posh then switches to its "as if on a new row" filler, Clink hides the right prompt, and the window wraps. Pinned in Task 3 (`the_filler_follows_oh_my_posh_at_every_width`), Task 4 (widths 1 to 200 against the real binary) and Task 5 (`rows_wrap_at_the_width_of_the_window`, `the_right_prompt_shows_only_while_two_cells_stay_free_before_it`).
3. **Git states other than an edited file.** A staged-only change, an untracked file, a branch with `/` or Thai letters, and no repository at all must give `*` or `✓` and the right color, or nothing. Pinned in Task 3 (`git_is_ok_colored_while_clean_and_dirty_colored_once_changed`) and Task 4 (five real repositories: clean, edited, staged, untracked, Thai branch, and a folder in none).
4. **The edges of the status and duration segments.** Exactly 2000 ms shows, 1999 does not; days-long durations; negative exit codes and NTSTATUS values like −1073741510. Pinned in Task 3 (`durations_are_written_in_the_austin_style`, `segments_the_sample_hides_leave_nothing_behind`) and Task 4 (durations 1999 ms to 25 hours and seven exit codes against the real binary).
5. **Text that must never reach the screen raw.** An escape sequence or a bidi control in the theme's banner text, or in a user, computer or folder name, must be cleaned exactly as `banner.lua` cleans it (a whole sequence from the theme's text; only the control character from a name). Pinned in Task 6 (`lua_clean_drops_exactly_what_banner_lua_drops`, `names_and_the_folder_lose_what_banner_lua_cleans`) and Task 7 (hostile names and hostile banner text run through the real script).
6. **A Thai word of CMD Studio's own slipping into what cmd shows** (added 2026-10-13). The welcome box and the prompt are English on every Windows; a Thai date, tip or label would break the founder's rule without failing any layout test. Pinned in Task 6 (`the_box_is_english_and_thai_shows_only_where_the_user_put_it`, `the_date_is_written_the_same_in_rust_and_in_lua`, and `the_preview_of_every_preset_holds_no_thai_character` in `tests/english_in_cmd.rs`, which runs over every preset in every layout) and Task 10 (the goldens, read before they are committed).

## File map

```
crates/core/Cargo.toml                        + unicode-width = "0.2" (Task 2)
Cargo.lock                                    unicode-width 0.2.2 (Task 2)
crates/core/src/lib.rs                        + pub mod preview; (Task 2)
crates/core/src/generate/omp.rs               the shared table (Task 1), Look::changed (Task 3)
crates/core/src/generate/lua.rs               DateStyle::format, DATE_EN, TIPS_EN, ICON, ICON_FALLBACK, Var/Piece/template_pieces (Task 6)
crates/core/src/preview/mod.rs                Style, Span, Line, PreviewContext, Clock, GitState (Task 2); module list grows (Tasks 3, 5, 6)
crates/core/src/preview/cells.rs              prompt_cells, banner_cells (Task 2); wrap (Task 5)
crates/core/src/preview/values.rs             prompt_path, austin, hh_mm (Task 3); weekday, lua_clean (Task 6)
crates/core/src/preview/prompt.rs             Prompt, prompt (Task 3); render_prompt (Task 5)
crates/core/src/preview/banner.rs             render_banner (Task 6)
crates/core/tests/sgr/mod.rs                  SGR reader shared by the parity tests (Task 4)
crates/core/tests/parity.rs                   the emulator against Oh My Posh 31.5.0 (Task 4)
crates/core/tests/banner_parity.rs            the welcome box against banner.lua in Lua 5.2 (Task 7)
crates/core/tests/english_in_cmd.rs           + the preview guard: no Thai in the box or the prompt of any preset (Task 6)
presets/{tokyo-night,catppuccin-mocha,dracula,nord,gruvbox,one-dark,catppuccin-latte,solarized-light}.cmdtheme   (Task 8)
presets/{retro-green,daylight,paper}.cmdtheme (Task 9)
crates/core/src/presets.rs                    SOURCES, OWN, preset tests (Tasks 8, 9)
docs/credits.md                               Dracula, Catppuccin and Nord sources, pinned commits (Task 8)
crates/core/tests/golden.rs                   + preview goldens (Task 10)
crates/core/tests/golden/{terracotta,tokyo-night,catppuccin-latte,paper}.preview.txt   (Task 10)
docs/superpowers/specs/2026-10-10-cmd-studio-design.md   appendix B (Tasks 8, 9); G2, §3.5, §5.1, §5.2, §5.6, §7 (Task 11)
```

---

### Task 1: One description of the prompt, shared by the generator and the preview

The emulator must draw what the generator writes. Today `generate::omp` spells each segment's colors and spaces inside its JSON-building functions, so a second copy in the emulator could drift away unseen. This task moves those facts into one table in `omp.rs` — the palette names (`Swatch`), the parts of the left side (`Part`, `left_parts`), how each layout draws each part (`Look`, `Drawn`, `drawn`), the prompt character and the frame characters — and makes the generator build its JSON from it. The JSON does not change by one byte: the golden files hold it.

**Files:**
- Modify: `crates/core/src/generate/omp.rs` (everything above `#[cfg(test)]` is replaced; four tests are added at the end of `mod tests`)

**Interfaces:**
- Consumes: Phase 1a's `model::{Layout, Segment, Theme}` (`Layout` and `Segment` are `#[non_exhaustive]`, which does not apply inside the crate: matches stay exhaustive with no `_` arm), `palette::Colors`, `color::Rgb`.
- Produces (all `pub(crate)` in `generate::omp`, used by `preview` from Task 3 on; the public API `config`, `config_text`, `ALLOWED_TYPES` is unchanged):
  - `enum Swatch { Bg, Fg, Accent, Muted, Frame, Red, Ok, Dirty, Blue }` (`Clone + Copy + Debug + PartialEq + Eq`) with `fn name(self) -> &'static str` (`"bg"`, …) and `fn color(self, colors: &Colors) -> Rgb` (Red/Ok/Dirty/Blue are ansi 1–4); private `const ALL: [Swatch; 9]` and `fn reference(self) -> String` (`"p:accent"`).
  - `enum Part { Path, Git, Time, Session { user: bool, host: bool }, Status, Duration }` (`Clone + Copy + Debug + PartialEq + Eq`).
  - `enum Look { Plain { fg: Swatch }, Powerline { bg: Swatch, fg: Swatch } }` (`Clone + Copy + Debug + PartialEq + Eq`).
  - `struct Drawn { look: Look, lead: Option<(Swatch, &'static str)>, before: &'static str, after: &'static str }` (fields `pub(crate)`).
  - `fn drawn(layout: Layout, part: Part) -> Drawn`, `fn left_parts(theme: &Theme) -> Vec<Part>`, `fn has_time(theme: &Theme) -> bool`, `fn prompt_char(layout: Layout) -> &'static str`.
  - `const BOX_TOP = "╭─ "`, `BOX_CLOSE = "─╮"`, `BOX_CLOSE_AFTER_TIME = " ─╮"`, `BOX_FILLER = "─"`, `BOX_SIDE = "│ "`, `BOX_PROMPT = ">"`, `BOX_RIGHT = "│"`, `LINE_PROMPT = "❯"`, `POWERLINE_PROMPT = " ❯"`, `POWERLINE_SYMBOL = U+E0B0`, `TRANSIENT_PROMPT = "> "`, `GIT_SEPARATOR = "· "`, `GIT_DIRTY_MARK = " *"`, `GIT_CLEAN_MARK = " ✓"`, `STATUS_MARK = "✗ "` (all `&str`) and `DURATION_THRESHOLD_MS: u64 = 2000`.
  - private `fn segment(layout: Layout, part: Part) -> Value` (one segment of the config with its options), which the new tests call.

- [ ] **Step 1: Write the failing tests.** In `crates/core/src/generate/omp.rs`, add these tests at the end of `mod tests` (before its closing `}`). They use the module's existing test helpers `themed`, `terracotta` and `LAYOUTS`:
```rust
    // ---- the parts the preview shares (crate::preview draws the same prompt) ----

    /// Every part, `Session` with both names.
    const PARTS: [Part; 6] = [
        Part::Path,
        Part::Git,
        Part::Time,
        Part::Session {
            user: true,
            host: true,
        },
        Part::Status,
        Part::Duration,
    ];

    #[test]
    fn every_swatch_is_in_the_palette_with_its_theme_color() {
        let t = terracotta();
        let c = config(&t);
        let palette = c["palette"].as_object().unwrap();
        assert_eq!(palette.len(), Swatch::ALL.len());
        for swatch in Swatch::ALL {
            assert_eq!(
                palette[swatch.name()],
                swatch.color(&t.colors).to_string(),
                "{swatch:?}"
            );
        }
        // the four ansi colors the prompt uses: red, green, yellow and blue
        assert_eq!(Swatch::Red.color(&t.colors), t.colors.ansi[1]);
        assert_eq!(Swatch::Ok.color(&t.colors), t.colors.ansi[2]);
        assert_eq!(Swatch::Dirty.color(&t.colors), t.colors.ansi[3]);
        assert_eq!(Swatch::Blue.color(&t.colors), t.colors.ansi[4]);
    }

    #[test]
    fn the_looks_are_the_ones_spec_3_5_was_built_with() {
        use Swatch as S;
        let plain = |fg| Look::Plain { fg };
        let block = |bg, fg| Look::Powerline { bg, fg };
        let session = Part::Session {
            user: true,
            host: false,
        };
        for (layout, part, look) in [
            (Layout::Box, Part::Path, plain(S::Muted)),
            (Layout::Box, Part::Git, plain(S::Ok)),
            (Layout::Box, Part::Time, plain(S::Muted)),
            (Layout::Box, session, plain(S::Fg)),
            (Layout::Box, Part::Status, plain(S::Red)),
            (Layout::Box, Part::Duration, plain(S::Dirty)),
            (Layout::Line, Part::Path, plain(S::Blue)),
            (Layout::Line, Part::Git, plain(S::Ok)),
            (Layout::Line, Part::Time, plain(S::Muted)),
            (Layout::Line, session, plain(S::Fg)),
            (Layout::Line, Part::Status, plain(S::Red)),
            (Layout::Line, Part::Duration, plain(S::Dirty)),
            (Layout::Powerline, Part::Path, block(S::Frame, S::Fg)),
            (Layout::Powerline, Part::Git, block(S::Ok, S::Bg)),
            (Layout::Powerline, Part::Time, block(S::Frame, S::Fg)),
            (Layout::Powerline, session, block(S::Accent, S::Bg)),
            (Layout::Powerline, Part::Status, block(S::Red, S::Bg)),
            (Layout::Powerline, Part::Duration, block(S::Dirty, S::Bg)),
        ] {
            assert_eq!(drawn(layout, part).look, look, "{layout:?} {part:?}");
        }
        // only git in the box has a lead, and only the box's time has its space before
        assert_eq!(
            drawn(Layout::Box, Part::Git).lead,
            Some((S::Muted, GIT_SEPARATOR))
        );
        assert_eq!(
            (
                drawn(Layout::Box, Part::Time).before,
                drawn(Layout::Box, Part::Time).after
            ),
            (" ", "")
        );
    }

    #[test]
    fn every_segment_is_drawn_as_the_shared_table_says() {
        for layout in LAYOUTS {
            for part in PARTS {
                let d = drawn(layout, part);
                let seg = segment(layout, part);
                let template = seg["template"].as_str().unwrap();
                let lead = d
                    .lead
                    .map(|(swatch, text)| format!("<p:{}>{text}</>", swatch.name()))
                    .unwrap_or_default();
                let why = format!("{layout:?} {part:?}: {template:?}");
                assert!(
                    template.starts_with(&format!("{lead}{}", d.before)),
                    "{why}"
                );
                assert!(template.ends_with(d.after), "{why}");
                match d.look {
                    Look::Plain { fg } => {
                        assert_eq!(seg["style"], "plain", "{why}");
                        assert_eq!(seg["foreground"], format!("p:{}", fg.name()), "{why}");
                        assert!(seg.get("background").is_none(), "{why}");
                    }
                    Look::Powerline { bg, fg } => {
                        assert_eq!(seg["style"], "powerline", "{why}");
                        assert_eq!(seg["background"], format!("p:{}", bg.name()), "{why}");
                        assert_eq!(seg["foreground"], format!("p:{}", fg.name()), "{why}");
                    }
                }
            }
        }
    }

    #[test]
    fn left_parts_follow_the_list_with_one_session_and_time_last() {
        let list = [
            Segment::Time,
            Segment::Host,
            Segment::Path,
            Segment::User,
            Segment::Git,
        ];
        let session = Part::Session {
            user: true,
            host: true,
        };
        assert_eq!(
            left_parts(&themed(Layout::Line, &list)),
            [session, Part::Path, Part::Git, Part::Time]
        );
        // the box keeps time for its right side
        assert_eq!(
            left_parts(&themed(Layout::Box, &list)),
            [session, Part::Path, Part::Git]
        );
        assert!(has_time(&themed(Layout::Box, &list)));
        assert!(left_parts(&themed(Layout::Powerline, &[])).is_empty());
    }

```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p cmdstudio-core --lib -- generate::omp`
Expected: the test build fails with errors like "failed to resolve: use of undeclared type Part", "cannot find function drawn in this scope" and "use of undeclared type Swatch".

- [ ] **Step 3: Replace the generator's code.** Replace everything in `crates/core/src/generate/omp.rs` above the line `#[cfg(test)]` with:
```rust
//! Theme → Oh My Posh config, format version 4 (spec §3.5). Built only from
//! whitelisted segment types; `text` carries only our own frame glyphs.
//!
//! What each segment looks like in each layout (`drawn`), which segments the left side
//! holds (`left_parts`), the palette (`Swatch`) and the frame characters are shared with
//! the preview emulator (`crate::preview`), which draws the same prompt without running Oh
//! My Posh. The parity test (`tests/parity.rs`) holds the two together.
use crate::color::Rgb;
use crate::model::{Layout, Segment, Theme};
use crate::palette::Colors;
use serde_json::{json, Map, Value};

/// Segment types this generator may emit (doctrine guard, spec §3.7 #2).
pub const ALLOWED_TYPES: &[&str] = &[
    "path",
    "git",
    "time",
    "session",
    "status",
    "executiontime",
    "text",
];

const TIME_FORMAT: &str = "15:04";
/// Opens a Go template `if` that is true when the working tree or the index has changes. The
/// text that uses it closes the `if` with `{{ end }}`.
const IF_CHANGED: &str = "{{ if or .Working.Changed .Staging.Changed }}";

/// The top left corner of the box and the line that starts it.
pub(crate) const BOX_TOP: &str = "\u{256d}\u{2500} ";
/// What closes the box's top line when nothing comes before it but the filler.
pub(crate) const BOX_CLOSE: &str = "\u{2500}\u{256e}";
/// What closes the box's top line after the time: a space sets the corner off from it.
pub(crate) const BOX_CLOSE_AFTER_TIME: &str = " \u{2500}\u{256e}";
/// The character the box's top line is filled with, up to the right side.
pub(crate) const BOX_FILLER: &str = "\u{2500}";
/// The start of the box's second line, the line the command is typed on.
pub(crate) const BOX_SIDE: &str = "\u{2502} ";
/// The prompt character of the box, after [`BOX_SIDE`].
pub(crate) const BOX_PROMPT: &str = ">";
/// The right prompt of the box: its right side, drawn by Clink at the right edge.
pub(crate) const BOX_RIGHT: &str = "\u{2502}";
/// The prompt character of the line layout.
pub(crate) const LINE_PROMPT: &str = "\u{276f}";
/// The prompt character of the powerline layout; the space sets it off from the last arrow.
pub(crate) const POWERLINE_PROMPT: &str = " \u{276f}";
/// The arrow between powerline segments (a Nerd Font glyph).
pub(crate) const POWERLINE_SYMBOL: &str = "\u{e0b0}";
/// What an earlier prompt shrinks to when the theme turns the transient prompt on.
pub(crate) const TRANSIENT_PROMPT: &str = "> ";
/// Git's separator from the path in the box layout, in the muted color.
pub(crate) const GIT_SEPARATOR: &str = "\u{b7} ";
/// After the branch: something changed.
pub(crate) const GIT_DIRTY_MARK: &str = " *";
/// After the branch: nothing changed.
pub(crate) const GIT_CLEAN_MARK: &str = " \u{2713}";
/// Before the exit code of the status segment.
pub(crate) const STATUS_MARK: &str = "\u{2717} ";
/// The duration segment shows from this many milliseconds on (spec §3.5).
pub(crate) const DURATION_THRESHOLD_MS: u64 = 2000;

/// A color of the config's palette, which the segments name as `p:<name>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Swatch {
    Bg,
    Fg,
    Accent,
    Muted,
    Frame,
    Red,
    Ok,
    Dirty,
    Blue,
}

impl Swatch {
    const ALL: [Swatch; 9] = [
        Swatch::Bg,
        Swatch::Fg,
        Swatch::Accent,
        Swatch::Muted,
        Swatch::Frame,
        Swatch::Red,
        Swatch::Ok,
        Swatch::Dirty,
        Swatch::Blue,
    ];

    /// The name in the palette.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Swatch::Bg => "bg",
            Swatch::Fg => "fg",
            Swatch::Accent => "accent",
            Swatch::Muted => "muted",
            Swatch::Frame => "frame",
            Swatch::Red => "red",
            Swatch::Ok => "ok",
            Swatch::Dirty => "dirty",
            Swatch::Blue => "blue",
        }
    }

    /// The theme color the palette gives this name.
    pub(crate) fn color(self, colors: &Colors) -> Rgb {
        // ansi 1 to 4 are red, green, yellow and blue; destructuring cannot index out of range
        let [_, red, green, yellow, blue, ..] = colors.ansi;
        match self {
            Swatch::Bg => colors.background,
            Swatch::Fg => colors.foreground,
            Swatch::Accent => colors.accent,
            Swatch::Muted => colors.muted,
            Swatch::Frame => colors.frame,
            Swatch::Red => red,
            Swatch::Ok => green,
            Swatch::Dirty => yellow,
            Swatch::Blue => blue,
        }
    }

    /// How a segment names the color: `p:accent`.
    fn reference(self) -> String {
        format!("p:{}", self.name())
    }
}

/// One Oh My Posh segment of the left side, or the box's right side: one per theme segment,
/// except that `user` and `host` share one `Session`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Path,
    Git,
    Time,
    Session { user: bool, host: bool },
    Status,
    Duration,
}

/// How a segment is colored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    /// Text in `fg`, with nothing behind it.
    Plain { fg: Swatch },
    /// Text in `fg` on a block of `bg`, joined to the next segment by an arrow.
    Powerline { bg: Swatch, fg: Swatch },
}

/// A segment as a layout draws it: its colors, and the text on either side of its data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Drawn {
    pub(crate) look: Look,
    /// Text in a color of its own at the very start (`<p:muted>· </>` before git in the box).
    pub(crate) lead: Option<(Swatch, &'static str)>,
    pub(crate) before: &'static str,
    pub(crate) after: &'static str,
}

/// How `layout` draws `part`. Powerline puts a space on each side of the data, inside the
/// block of color. Box and line put one space after it, except the time on the box's right
/// side, which has its space before.
pub(crate) fn drawn(layout: Layout, part: Part) -> Drawn {
    match layout {
        Layout::Powerline => {
            let (bg, fg) = match part {
                Part::Path | Part::Time => (Swatch::Frame, Swatch::Fg),
                Part::Git => (Swatch::Ok, Swatch::Bg),
                Part::Session { .. } => (Swatch::Accent, Swatch::Bg),
                Part::Status => (Swatch::Red, Swatch::Bg),
                Part::Duration => (Swatch::Dirty, Swatch::Bg),
            };
            Drawn {
                look: Look::Powerline { bg, fg },
                lead: None,
                before: " ",
                after: " ",
            }
        }
        Layout::Box | Layout::Line => {
            let in_box = layout == Layout::Box;
            let fg = match part {
                Part::Path if in_box => Swatch::Muted,
                Part::Path => Swatch::Blue,
                Part::Git => Swatch::Ok,
                Part::Time => Swatch::Muted,
                Part::Session { .. } => Swatch::Fg,
                Part::Status => Swatch::Red,
                Part::Duration => Swatch::Dirty,
            };
            let lead = (in_box && part == Part::Git).then_some((Swatch::Muted, GIT_SEPARATOR));
            let (before, after) = if in_box && part == Part::Time {
                (" ", "")
            } else {
                ("", " ")
            };
            Drawn {
                look: Look::Plain { fg },
                lead,
                before,
                after,
            }
        }
    }
}

/// The character the user types after, in the accent color: on the box's second line, or at
/// the end of the one line of the other layouts.
pub(crate) fn prompt_char(layout: Layout) -> &'static str {
    match layout {
        Layout::Box => BOX_PROMPT,
        Layout::Line => LINE_PROMPT,
        Layout::Powerline => POWERLINE_PROMPT,
    }
}

/// The Oh My Posh config for `theme`, format version 4. It reads the theme's colors, layout
/// and segment list and nothing else, so no text from the theme file ends up in it. The
/// transient prompt is always described; whether Clink uses it is Clink's own
/// `prompt.transient` setting.
pub fn config(theme: &Theme) -> Value {
    let layout = theme.prompt.layout;
    let blocks = match layout {
        Layout::Box => box_blocks(theme),
        Layout::Line | Layout::Powerline => single_line(theme),
    };
    json!({
        "$schema": "https://raw.githubusercontent.com/JanDeDobbeleer/oh-my-posh/main/themes/schema.json",
        "version": 4,
        "final_space": true,
        "console_title_template": "{{ .Folder }}",
        "palette": palette(&theme.colors),
        "blocks": blocks,
        "transient_prompt": {
            "foreground": Swatch::Accent.reference(),
            "background": "transparent",
            "template": TRANSIENT_PROMPT
        }
    })
}

/// [`config`] as the text of the file: pretty-printed, with one final newline.
pub fn config_text(theme: &Theme) -> Result<String, serde_json::Error> {
    let mut text = serde_json::to_string_pretty(&config(theme))?;
    text.push('\n');
    Ok(text)
}

fn palette(colors: &Colors) -> Value {
    let names: Map<String, Value> = Swatch::ALL
        .iter()
        .map(|swatch| {
            (
                swatch.name().to_string(),
                Value::String(swatch.color(colors).to_string()),
            )
        })
        .collect();
    Value::Object(names)
}

/// One Oh My Posh segment under construction. It holds a JSON object, so setting a key
/// cannot fail the way writing `v["key"]` into a `Value` that is not an object would.
#[derive(Default)]
struct Seg(Map<String, Value>);

impl Seg {
    fn set(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.0.insert(key.to_string(), value.into());
        self
    }

    /// A segment of `kind` with `look` and `template`.
    fn new(kind: &str, look: Look, template: &str) -> Self {
        let seg = Seg::default().set("type", kind).set("template", template);
        match look {
            Look::Plain { fg } => seg.set("style", "plain").set("foreground", fg.reference()),
            // the arrow to the next segment is a Nerd Font glyph
            Look::Powerline { bg, fg } => seg
                .set("style", "powerline")
                .set("powerline_symbol", POWERLINE_SYMBOL)
                .set("background", bg.reference())
                .set("foreground", fg.reference()),
        }
    }
}

impl From<Seg> for Value {
    fn from(seg: Seg) -> Value {
        Value::Object(seg.0)
    }
}

/// A `text` segment: one of our own frame characters, in `swatch`.
fn text(swatch: Swatch, template: &str) -> Value {
    Seg::new("text", Look::Plain { fg: swatch }, template).into()
}

/// Git's icons are Nerd Font glyphs unless they are replaced; only powerline keeps them.
fn git_options(layout: Layout) -> Value {
    match layout {
        Layout::Powerline => json!({ "fetch_status": true }),
        Layout::Box | Layout::Line => json!({
            "branch_icon": "", "commit_icon": "@", "tag_icon": "#", "rebase_icon": "rebase ",
            "cherry_pick_icon": "pick ", "revert_icon": "revert ", "merge_icon": "merge ",
            "no_commits_icon": "", "fetch_status": true
        }),
    }
}

/// The Oh My Posh type of a part, and the Go template of its data.
fn kind_and_body(part: Part) -> (&'static str, String) {
    match part {
        Part::Path => ("path", "{{ .Path }}".to_string()),
        // the branch, then `*` when something changed or `✓` when nothing did (spec §3.5)
        Part::Git => (
            "git",
            [
                "{{ .HEAD }}",
                IF_CHANGED,
                GIT_DIRTY_MARK,
                "{{ else }}",
                GIT_CLEAN_MARK,
                "{{ end }}",
            ]
            .concat(),
        ),
        Part::Time => ("time", "{{ .CurrentDate | date .Format }}".to_string()),
        // only built when the theme lists `user` or `host`, so `(false, _)` is host alone
        Part::Session { user, host } => (
            "session",
            match (user, host) {
                (true, true) => "{{ .UserName }}@{{ .HostName }}",
                (true, false) => "{{ .UserName }}",
                (false, _) => "{{ .HostName }}",
            }
            .to_string(),
        ),
        Part::Status => ("status", format!("{STATUS_MARK}{{{{ .Code }}}}")),
        Part::Duration => ("executiontime", "{{ .FormattedMs }}".to_string()),
    }
}

/// The segment of `part` in `layout`, with the options it needs.
fn segment(layout: Layout, part: Part) -> Value {
    let drawn = drawn(layout, part);
    let (kind, body) = kind_and_body(part);
    let lead = drawn
        .lead
        .map(|(swatch, text)| format!("<{}>{text}</>", swatch.reference()))
        .unwrap_or_default();
    let template = [lead.as_str(), drawn.before, body.as_str(), drawn.after].concat();
    let seg = Seg::new(kind, drawn.look, &template);
    let seg = match part {
        Part::Path => seg.set("options", json!({ "style": "full", "home_icon": "~" })),
        Part::Git => {
            // Oh My Posh uses the first color template that gives a non-empty result, and the
            // plain color when none does. So git is `p:ok` while nothing has changed, and this
            // template makes it `p:dirty` once something has: the text in box and line, the
            // block behind the text in powerline (whose text stays `p:bg`, readable on both).
            let when_changed = [IF_CHANGED, "p:dirty{{ end }}"].concat();
            let dirty = json!([when_changed]);
            let seg = match drawn.look {
                Look::Plain { .. } => seg.set("foreground_templates", dirty),
                Look::Powerline { .. } => seg.set("background_templates", dirty),
            };
            seg.set("options", git_options(layout))
        }
        Part::Time => seg.set("options", json!({ "time_format": TIME_FORMAT })),
        Part::Session { .. } | Part::Status => seg,
        Part::Duration => seg.set(
            "options",
            json!({ "threshold": DURATION_THRESHOLD_MS, "style": "austin" }),
        ),
    };
    seg.into()
}

/// The parts drawn on the left side, one per theme segment, in theme order. `user` and `host`
/// share one `session` segment, placed where the first of them is listed. Time is not drawn in
/// list order: the box layout puts it on the right side, and the other two put it last on the
/// left, whatever its place in the list (spec §3.5).
pub(crate) fn left_parts(theme: &Theme) -> Vec<Part> {
    let list = &theme.prompt.segments;
    let user = list.contains(&Segment::User);
    let host = list.contains(&Segment::Host);
    let mut session_done = false;
    let mut out = Vec::new();
    for seg in list {
        let part = match seg {
            Segment::Time => continue,
            Segment::User | Segment::Host => {
                if session_done {
                    continue;
                }
                session_done = true;
                Part::Session { user, host }
            }
            Segment::Path => Part::Path,
            Segment::Git => Part::Git,
            Segment::Status => Part::Status,
            Segment::Duration => Part::Duration,
        };
        out.push(part);
    }
    if has_time(theme) {
        match theme.prompt.layout {
            Layout::Box => {} // drawn on the right side, by `box_blocks`
            Layout::Line | Layout::Powerline => out.push(Part::Time),
        }
    }
    out
}

/// Whether the theme lists the time.
pub(crate) fn has_time(theme: &Theme) -> bool {
    theme.prompt.segments.contains(&Segment::Time)
}

fn left_segments(theme: &Theme) -> Vec<Value> {
    let layout = theme.prompt.layout;
    left_parts(theme)
        .into_iter()
        .map(|part| segment(layout, part))
        .collect()
}

fn box_blocks(theme: &Theme) -> Vec<Value> {
    let mut left = vec![text(Swatch::Frame, BOX_TOP)];
    left.extend(left_segments(theme));
    // With time, a space sets the closing corner off from it. Without time only the filler
    // comes before the corner, and a space there would show as a gap.
    let right = if has_time(theme) {
        vec![
            segment(Layout::Box, Part::Time),
            text(Swatch::Frame, BOX_CLOSE_AFTER_TIME),
        ]
    } else {
        vec![text(Swatch::Frame, BOX_CLOSE)]
    };
    let filler = format!("<{}>{BOX_FILLER}</>", Swatch::Frame.reference());
    vec![
        json!({ "type": "prompt", "alignment": "left", "segments": left }),
        json!({ "type": "prompt", "alignment": "right", "filler": filler, "segments": right }),
        json!({ "type": "prompt", "alignment": "left", "newline": true, "segments": [text(Swatch::Frame, BOX_SIDE), text(Swatch::Accent, BOX_PROMPT)] }),
        json!({ "type": "rprompt", "segments": [text(Swatch::Frame, BOX_RIGHT)] }),
    ]
}

/// The one-line layouts: the left segments, then the prompt character.
fn single_line(theme: &Theme) -> Vec<Value> {
    let mut list = left_segments(theme);
    list.push(text(Swatch::Accent, prompt_char(theme.prompt.layout)));
    vec![json!({ "type": "prompt", "alignment": "left", "segments": list })]
}

```
What moved where, for the reviewer: `palette()` now walks `Swatch::ALL`; `Seg::plain` and `Seg::powerline` became `Seg::new(kind, look, template)`; `path`, `git`, `time`, `session`, `status` and `duration` became one `segment(layout, part)` that reads `drawn()` and `kind_and_body()`; `left_segments` maps `left_parts()`. Nothing that reaches the JSON changed: same keys, same templates, same options.

- [ ] **Step 4: Run the tests and the goldens**

Run: `cargo test -p cmdstudio-core --lib -- generate::omp`
Expected: `test result: ok. 25 passed` (the 21 Phase 1a tests and the 4 new ones).

Run: `cargo test -p cmdstudio-core --test golden`, then `git status --short crates/core/tests/golden/`
Expected: `test result: ok. 17 passed` (the count at `4a86f6b`; PR #5 dropped the Thai banner golden), and `git status` prints nothing: every Oh My Posh config golden is byte for byte what it was.

- [ ] **Step 5: Format, lint, docs**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo doc -p cmdstudio-core --no-deps`
Expected: no diff after `fmt`, no warnings. (The module doc names `drawn`, `left_parts` and `Swatch` in plain backticks on purpose: an intra-doc link from the public module to a `pub(crate)` item is a rustdoc warning.)

- [ ] **Step 6: Commit**

Message (`.git/CMDSTUDIO_COMMIT_MSG`):
```text
refactor(core): build the Oh My Posh config from one shared table

How each layout draws each segment (its palette colors, the text on
either side of its data, git's separator in the box), which segments
the left side holds, the palette's names and the frame characters
now live in one table in generate::omp, and the config is built from
it. The preview emulator will draw from the same table, so the two
cannot drift apart. The generated JSON is unchanged: every golden
file is byte for byte the same.
```
Run:
```bash
git add -- crates/core/src/generate/omp.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 2: The preview's spans, its sample context, and how cells are counted

**Files:**
- Modify: `crates/core/Cargo.toml` (`unicode-width`), `Cargo.lock`, `crates/core/src/lib.rs` (`pub mod preview;`)
- Create: `crates/core/src/preview/mod.rs`, `crates/core/src/preview/cells.rs`

**Interfaces:**
- Consumes: `color::Rgb` (serializes as `"#rrggbb"`); serde.
- Produces (public, in `cmdstudio_core::preview`):
  - `struct Style { pub fg: Rgb, pub bg: Option<Rgb>, pub bold: bool }` (`Clone + Copy + Debug + PartialEq + Eq`) with `fn fg(fg: Rgb) -> Style` (no background, not bold).
  - `struct Span { pub text: String, pub fg: Rgb, pub bg: Option<Rgb>, pub bold: bool }` (serde camelCase, `deny_unknown_fields`) with `fn style(&self) -> Style`.
  - `struct Line { pub spans: Vec<Span> }` (`#[serde(transparent)]`: a JSON array; `Default`) with `fn push(&mut self, text: &str, style: Style)` (drops empty text, joins the last span of the same style), `fn append(&mut self, other: &Line)`, `fn text(&self) -> String`.
  - `struct PreviewContext { pub user: String, pub host: String, pub home: String, pub cwd: String, pub clock: Clock, pub git: Option<GitState>, pub exit_code: i32, pub duration_ms: u64, pub width: u16, pub windows_terminal: bool, pub command: String }` (serde camelCase, `deny_unknown_fields`) with `fn sample() -> PreviewContext` (`you`, `PC`, `C:\Users\you`, `C:\Users\you\projects\cmd-studio`, Friday 9 October 2026 16:47, branch `main` dirty, exit code 1, 3500 ms, width 80, Windows Terminal, command `npm test`).
  - `struct Clock { pub year: u16, pub month: u8, pub day: u8, pub hour: u8, pub minute: u8 }` and `struct GitState { pub branch: String, pub dirty: bool }` (both serde camelCase, `deny_unknown_fields`).
  - `fn prompt_cells(text: &str) -> usize` (Oh My Posh's count, character by character) and `fn banner_cells(text: &str) -> usize` (Clink's count in Windows Terminal: the whole string's Unicode width).

- [ ] **Step 1: Add the dependency and the module, and write the failing tests.** In `crates/core/Cargo.toml`, under `[dependencies]`, add after the `schemars = ...` line:
```toml
unicode-width = "0.2"
```
In `crates/core/src/lib.rs` add `pub mod preview;` to the module list, keeping it alphabetical (after `pub mod presets;`). Create `crates/core/src/preview/mod.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(s: &str) -> Rgb {
        s.parse().unwrap()
    }

    #[test]
    fn push_joins_text_of_the_same_style_and_drops_empty_text() {
        let (a, b) = (Style::fg(rgb("#111111")), Style::fg(rgb("#222222")));
        let mut line = Line::default();
        line.push("ab", a);
        line.push("", b);
        line.push("c", a);
        line.push("d", b);
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].text, "abc");
        assert_eq!(line.text(), "abcd");
        // the background and bold are part of the style
        line.push(
            "e",
            Style {
                bg: Some(rgb("#333333")),
                ..b
            },
        );
        line.push("f", Style { bold: true, ..b });
        assert_eq!(line.spans.len(), 4);
    }

    #[test]
    fn two_lines_that_look_the_same_are_equal_however_they_were_built() {
        let s = Style::fg(rgb("#123456"));
        let mut one = Line::default();
        one.push("hello", s);
        let mut two = Line::default();
        for piece in ["he", "", "l", "lo"] {
            two.push(piece, s);
        }
        assert_eq!(one, two);
        let mut three = Line::default();
        three.append(&two);
        assert_eq!(three, one);
    }

    #[test]
    fn spans_and_lines_serialize_as_the_app_reads_them() {
        let mut line = Line::default();
        line.push("~ ", Style::fg(rgb("#8a8780")));
        line.push(
            "x",
            Style {
                bg: Some(rgb("#5f5d58")),
                ..Style::fg(rgb("#faf9f5"))
            },
        );
        let json = serde_json::to_string(&line).unwrap();
        assert_eq!(
            json,
            r##"[{"text":"~ ","fg":"#8a8780","bg":null,"bold":false},{"text":"x","fg":"#faf9f5","bg":"#5f5d58","bold":false}]"##
        );
        assert_eq!(serde_json::from_str::<Line>(&json).unwrap(), line);
    }

    #[test]
    fn the_context_round_trips_through_json_in_camel_case() {
        let ctx = PreviewContext::sample();
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(json["exitCode"], 1);
        assert_eq!(json["durationMs"], 3500);
        assert_eq!(json["windowsTerminal"], true);
        assert_eq!(json["git"]["dirty"], true);
        assert_eq!(serde_json::from_value::<PreviewContext>(json).unwrap(), ctx);
        // a field the format does not know is refused, like a theme's
        let mut extra = serde_json::to_value(&ctx).unwrap();
        extra["shell"] = serde_json::json!("pwsh");
        assert!(serde_json::from_value::<PreviewContext>(extra).is_err());
    }
}
```
and `crates/core/src/preview/cells.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_cells_count_thai_marks_as_nothing_and_wide_characters_as_two() {
        // measured on Oh My Posh 31.5.0 through the box filler (see tests/parity.rs)
        for (text, cells) in [
            ("abc", 3),
            ("ผู้ทดสอบ", 6),
            ("ที่นี่", 2),
            ("中文", 4),
            ("📁x", 3),
            ("·×①", 3),
            ("\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}", 6),
            ("\u{2764}\u{fe0f}", 1),
            ("ﾊ가", 3),
            ("\u{e0b0}\u{e0a0}╭─✓✗❯", 7),
        ] {
            assert_eq!(prompt_cells(text), cells, "{text:?}");
        }
    }

    #[test]
    fn banner_cells_join_emoji_sequences_like_clink_in_windows_terminal() {
        for (text, cells) in [
            ("abc", 3),
            ("มี.ค.", 4),
            ("ผู้ทดสอบ", 6),
            ("中文", 4),
            ("👋", 2),
            ("\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}", 2),
            ("\u{2764}\u{fe0f}", 2),
            ("✻ Welcome", 9),
        ] {
            assert_eq!(banner_cells(text), cells, "{text:?}");
        }
    }
}
```
The cell counts in `prompt_cells_count_thai_marks_as_nothing_and_wide_characters_as_two` were measured on Oh My Posh 31.5.0 through the box's filler (Task 4 checks them against the binary again); the ones in `banner_cells_join_emoji_sequences_like_clink_in_windows_terminal` follow Clink 1.9.34's source for a Windows Terminal tab (Decision 6).

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p cmdstudio-core --lib -- preview::`
Expected: the build fails in the tests of `preview/mod.rs`: `Style`, `Line`, `PreviewContext` and `Rgb` are not found. (`cells.rs` is not compiled yet: Step 3 declares `mod cells;`.)

- [ ] **Step 3: Write the code above the tests.** In `crates/core/src/preview/mod.rs`, above `#[cfg(test)]`, put:
```rust
//! The preview emulator (spec §5.6): the prompt and the welcome box as cmd shows them, drawn
//! from the theme without running Oh My Posh or Clink, so the editor can redraw on every
//! change and show git even on a machine without git. It gives lines of styled spans; the
//! app only paints them. Palette, contrast and prompt logic stay here, in Rust, and never in
//! TypeScript (spec §5.6). The preview is English, as everything CMD Studio writes into a
//! cmd window is (spec §3.6); Thai shows only where the user's own data holds it.
//!
//! The parity tests hold it to the real thing: `tests/parity.rs` compares the prompt, cell
//! by cell and color by color, with what the pinned Oh My Posh prints for the config
//! `generate::omp` writes, and `tests/banner_parity.rs` compares the welcome box with what
//! the generated `banner.lua` prints.
mod cells;

pub use cells::{banner_cells, prompt_cells};

use crate::color::Rgb;
use serde::{Deserialize, Serialize};

/// The colors of a run of text: what the terminal draws, after reverse video.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub fg: Rgb,
    /// `None` is the terminal's own background.
    pub bg: Option<Rgb>,
    pub bold: bool,
}

impl Style {
    /// Text in `fg` on the terminal's background.
    pub fn fg(fg: Rgb) -> Style {
        Style {
            fg,
            bg: None,
            bold: false,
        }
    }
}

/// A run of text in one style.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Span {
    pub text: String,
    pub fg: Rgb,
    pub bg: Option<Rgb>,
    pub bold: bool,
}

impl Span {
    pub fn style(&self) -> Style {
        Style {
            fg: self.fg,
            bg: self.bg,
            bold: self.bold,
        }
    }
}

/// One line of the preview: spans from left to right. [`Line::push`] keeps it in one shape,
/// with no empty span and no two neighbours in the same style, so two lines that look the
/// same are equal.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Line {
    pub spans: Vec<Span>,
}

impl Line {
    /// Adds `text` in `style` at the end. Empty text adds nothing, and text in the style of
    /// the last span joins it.
    pub fn push(&mut self, text: &str, style: Style) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut() {
            if last.style() == style {
                last.text.push_str(text);
                return;
            }
        }
        self.spans.push(Span {
            text: text.to_string(),
            fg: style.fg,
            bg: style.bg,
            bold: style.bold,
        });
    }

    /// Adds every span of `other` at the end.
    pub fn append(&mut self, other: &Line) {
        for span in &other.spans {
            self.push(&span.text, span.style());
        }
    }

    /// The text of the line, without its colors.
    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

/// The sample values the preview shows: who, where, when, and how the command before went.
/// The app chooses them; the welcome box and the prompt read the same ones.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewContext {
    /// The Windows user name: `{user}` in the welcome box, and the prompt's `user`.
    pub user: String,
    /// The computer name: `{host}`, and the prompt's `host`. (A real window shows
    /// `COMPUTERNAME` in the box and the DNS host name in the prompt; the preview shows one.)
    pub host: String,
    /// The user's profile folder (`C:\Users\you`), which the prompt shows as `~`.
    pub home: String,
    /// The folder cmd is in: `{cwd}` as it is, and the prompt's `path` with `home` as `~`.
    pub cwd: String,
    /// The local date and time: `{date}`, `{time}` and the prompt's `time`.
    pub clock: Clock,
    /// The git repository `cwd` is in, or `None` when it is in none (git is then hidden).
    pub git: Option<GitState>,
    /// The exit code of the command before; the prompt's `status` shows when it is not 0.
    pub exit_code: i32,
    /// How long the command before ran; the prompt's `duration` shows from 2 seconds on.
    pub duration_ms: u64,
    /// The width of the window in cells. The box's top line is filled to it, and the right
    /// prompt sits at its right edge. 0 is taken as 1.
    pub width: u16,
    /// Whether the window is a Windows Terminal tab: the welcome box's icon is `✻` there and
    /// `*` elsewhere (spec §3.6).
    pub windows_terminal: bool,
    /// The command typed at the prompt before this one, shown after it (spec §3.5: with
    /// the transient prompt on, that prompt has shrunk to `> `).
    pub command: String,
}

/// A local date and time, as `os.date("*t")` gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Clock {
    pub year: u16,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
    /// 0 to 23.
    pub hour: u8,
    /// 0 to 59.
    pub minute: u8,
}

/// A git repository as the prompt shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitState {
    pub branch: String,
    /// Whether anything changed: an edited, staged or untracked file (`*`, else `✓`).
    pub dirty: bool,
}

impl PreviewContext {
    /// The sample the editor starts from: a failed command that ran 3.5 seconds, so the
    /// status and duration segments show when the theme has them, and Friday 9 October
    /// 2026 at 16:47, the date spec §3.6 writes out (`Fri 09 Oct 2026`).
    pub fn sample() -> PreviewContext {
        PreviewContext {
            user: "you".to_string(),
            host: "PC".to_string(),
            home: r"C:\Users\you".to_string(),
            cwd: r"C:\Users\you\projects\cmd-studio".to_string(),
            clock: Clock {
                year: 2026,
                month: 10,
                day: 9,
                hour: 16,
                minute: 47,
            },
            git: Some(GitState {
                branch: "main".to_string(),
                dirty: true,
            }),
            exit_code: 1,
            duration_ms: 3500,
            width: 80,
            windows_terminal: true,
            command: "npm test".to_string(),
        }
    }
}

```
In `crates/core/src/preview/cells.rs`, above `#[cfg(test)]`, put:
```rust
//! How many cells of the window a text takes, as the two programs that lay out the preview
//! count them.
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The cells of one character as Oh My Posh counts them: 0 for a combining mark (the Thai
/// vowel and tone marks above and below a letter), a joiner or a variation selector, 2 for a
/// wide character (Chinese, Japanese, Korean, most emoji), 1 for the rest.
fn char_cells(c: char) -> usize {
    c.width().unwrap_or(0)
}

/// The cells `text` takes as Oh My Posh 31.5.0 counts them when it fills the box's top line:
/// character by character, so a joined emoji counts each of its parts. Checked against the
/// real binary in `tests/parity.rs` (Thai, Chinese, an emoji and a family emoji in the path).
pub fn prompt_cells(text: &str) -> usize {
    text.chars().map(char_cells).sum()
}

/// The cells `text` takes as `console.cellcount` counts them in a Windows Terminal tab, which
/// banner.lua pads the welcome box by: Clink 1.9.34 on Windows 10 and 11 gives a combining
/// mark no cell of its own and a joined emoji two cells (its `wcwidth_iter`). conhost is
/// different: there Clink gives each combining mark a cell, because conhost draws it in one.
/// The preview draws Windows Terminal.
pub fn banner_cells(text: &str) -> usize {
    text.width()
}

```
Implementation Gate: `unicode-width` 0.2.2's `UnicodeWidthChar::width(self) -> Option<usize>` is `None` for control characters (counted 0 here) and `UnicodeWidthStr::width(&self) -> usize` treats emoji presentation and ZWJ sequences as one wide glyph; read both in `~/.cargo/registry/src/*/unicode-width-0.2.2/src/lib.rs` before relying on them.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cmdstudio-core --lib -- preview::`
Expected: `test result: ok. 6 passed`. The crate's unit tests are 6 more than before this task (`4a86f6b`'s 624 + Task 1's 4 + 6 = 634).

- [ ] **Step 5: Format, lint, docs**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo doc -p cmdstudio-core --no-deps`
Expected: no warnings. `git diff --stat Cargo.lock` shows only `unicode-width 0.2.2` added.

- [ ] **Step 6: Commit**

Message:
```text
feat(core): add the preview's spans, sample context and cell counts

preview::Line is a list of styled spans {text, fg, bg, bold}, kept in
one shape by Line::push so that two lines that look the same are
equal. PreviewContext holds the sample values the app picks (user,
host, home and folder, clock, git, last exit code and duration, width,
whether the window is a Windows Terminal tab, the command typed
before), with a sample of its own. prompt_cells counts cells as Oh My
Posh 31.5.0 does (a Thai mark takes none, a wide character two), and
banner_cells as Clink does in Windows Terminal. New dependency:
unicode-width 0.2.
```
Run:
```bash
git add -- crates/core/Cargo.toml Cargo.lock crates/core/src/lib.rs crates/core/src/preview/mod.rs crates/core/src/preview/cells.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 3: The prompt as Oh My Posh prints it

**Files:**
- Create: `crates/core/src/preview/values.rs`, `crates/core/src/preview/prompt.rs`
- Modify: `crates/core/src/preview/mod.rs` (module list), `crates/core/src/generate/omp.rs` (`Look::changed` and one test)

**Interfaces:**
- Consumes: Task 1's `generate::omp::{Swatch, Part, Look, drawn, left_parts, has_time, prompt_char, BOX_*, POWERLINE_SYMBOL, TRANSIENT_PROMPT, GIT_DIRTY_MARK, GIT_CLEAN_MARK, STATUS_MARK, DURATION_THRESHOLD_MS}`; Task 2's `preview::{Line, Style, PreviewContext, Clock, GitState, prompt_cells}`; `presets::terracotta()` (test helper).
- Produces:
  - `pub struct Prompt { pub primary: Vec<Line>, pub right: Line, pub transient: Line }` (serde camelCase) and `pub fn prompt(theme: &Theme, ctx: &PreviewContext) -> Prompt` in `preview` — what `oh-my-posh print primary`, `print right` and `print transient` print for `cmd`.
  - `pub(crate) fn Look::changed(self) -> Look` in `generate::omp`: the look git switches to when the repository changed.
  - `pub(crate)` in `preview::values`: `fn prompt_path(home: &str, cwd: &str) -> String`, `fn austin(ms: u64) -> String`, `fn hh_mm(clock: &Clock) -> String`.
  - private in `preview::prompt`: `fn filler_cells(width: usize, current: usize, length: usize) -> usize`, `struct Block` (joins one block's segments), `const POWERLINE_BRANCH_ICON` (U+E0A0), `const BREATHING_ROOM: usize = 5`.

The facts this code follows are the ones listed at the top of this plan under "What was measured on the real Oh My Posh 31.5.0"; Task 4 checks every one of them against the binary.

- [ ] **Step 1: Write the failing tests.** Create `crates/core/src/preview/values.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_is_shown_as_oh_my_posh_shows_it() {
        // each pair measured on Oh My Posh 31.5.0 with home C:\Users\someone
        let home = r"C:\Users\someone";
        for (cwd, shown) in [
            (r"C:\Users\someone", "~"),
            (r"C:\Users\someone\", "~"),
            (r"C:\Users\someone\projects\x", r"~\projects\x"),
            (r"c:\users\SOMEONE\projects", r"~\projects"),
            (r"C:\Users\someoneelse\x", r"C:\Users\someoneelse\x"),
            (r"D:\work\x", r"D:\work\x"),
            (r"D:\work\", r"D:\work"),
            ("D:", r"D:\"),
            (r"C:\", r"C:\"),
            (r"c:\x", r"C:\x"),
            ("C:/Users/someone/fwd", r"~\fwd"),
            (r"C:\Users\someone\a\..\b", r"~\a\..\b"),
            (r"C:\Users\someone\a b\c", r"~\a b\c"),
        ] {
            assert_eq!(prompt_path(home, cwd), shown, "{cwd}");
        }
        // a home with a trailing separator, and one in another case
        assert_eq!(
            prompt_path(r"C:\Users\someone\", r"C:\Users\someone\x"),
            r"~\x"
        );
        assert_eq!(
            prompt_path(r"c:\users\someone", r"C:\Users\Someone\X"),
            r"~\X"
        );
        // Thai letters in the user name and the folder
        assert_eq!(
            prompt_path(r"C:\Users\ผู้ทดสอบ ไทย", r"C:\Users\ผู้ทดสอบ ไทย\เอกสาร"),
            r"~\เอกสาร"
        );
        // no home: the folder as it is
        assert_eq!(prompt_path("", r"C:\x\"), r"C:\x");
    }

    #[test]
    fn durations_are_written_in_the_austin_style() {
        // each pair measured on Oh My Posh 31.5.0
        for (ms, shown) in [
            (2000, "2s"),
            (2001, "2.001s"),
            (2010, "2.01s"),
            (2049, "2.049s"),
            (2100, "2.1s"),
            (3500, "3.5s"),
            (59_999, "59.999s"),
            (60_000, "1m 0s"),
            (60_001, "1m 0.001s"),
            (61_500, "1m 1.5s"),
            (119_999, "1m 59.999s"),
            (3_599_999, "59m 59.999s"),
            (3_600_000, "1h 0m 0s"),
            (3_661_001, "1h 1m 1.001s"),
            (86_399_999, "23h 59m 59.999s"),
            (86_400_000, "1d 0h 0m 0s"),
            (90_061_001, "1d 1h 1m 1.001s"),
            (172_800_000, "2d 0h 0m 0s"),
        ] {
            assert_eq!(austin(ms), shown, "{ms}");
        }
    }

    #[test]
    fn the_time_has_two_digits_for_hours_and_minutes() {
        let clock = |hour, minute| Clock {
            year: 2026,
            month: 1,
            day: 1,
            hour,
            minute,
        };
        assert_eq!(hh_mm(&clock(7, 5)), "07:05");
        assert_eq!(hh_mm(&clock(16, 47)), "16:47");
        assert_eq!(hh_mm(&clock(0, 0)), "00:00");
    }
}
```
Create `crates/core/src/preview/prompt.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Segment;
    use crate::presets::terracotta;
    use crate::preview::GitState;

    const ALL: [Segment; 7] = [
        Segment::Path,
        Segment::Git,
        Segment::Time,
        Segment::User,
        Segment::Host,
        Segment::Status,
        Segment::Duration,
    ];

    // Terracotta's palette, by the names the config gives it
    const BG: &str = "#1f1e1d";
    const FG: &str = "#faf9f5";
    const ACCENT: &str = "#d97757";
    const MUTED: &str = "#8a8780";
    const FRAME: &str = "#5f5d58";
    const RED: &str = "#e5736a";
    const OK: &str = "#8fbf7f";
    const DIRTY: &str = "#e3b66b";
    const BLUE: &str = "#7fa7d9";

    fn themed(layout: Layout, segments: &[Segment]) -> Theme {
        let mut t = terracotta();
        t.prompt.layout = layout;
        t.prompt.segments = segments.to_vec();
        t
    }

    /// A line as (text, foreground, background), for short comparisons.
    fn spans(line: &Line) -> Vec<(String, String, Option<String>)> {
        line.spans
            .iter()
            .map(|s| {
                (
                    s.text.clone(),
                    s.fg.to_string(),
                    s.bg.map(|b| b.to_string()),
                )
            })
            .collect()
    }

    /// Text in `fg` on the terminal's background.
    fn s(text: &str, fg: &str) -> (String, String, Option<String>) {
        (text.to_string(), fg.to_string(), None)
    }

    /// Text in `fg` on `bg`.
    fn on(text: &str, fg: &str, bg: &str) -> (String, String, Option<String>) {
        (text.to_string(), fg.to_string(), Some(bg.to_string()))
    }

    fn sample(width: u16) -> PreviewContext {
        PreviewContext {
            width,
            ..PreviewContext::sample()
        }
    }

    #[test]
    fn the_box_fills_its_top_line_to_the_width_and_closes_after_the_time() {
        // Terracotta's own prompt as Oh My Posh 31.5.0 prints it 60 cells wide: the left side
        // is 34 cells and the right side 9, so the filler is 17
        let p = prompt(&terracotta(), &sample(60));
        assert_eq!(p.primary.len(), 2);
        assert_eq!(
            spans(&p.primary[0]),
            vec![
                s("\u{256d}\u{2500} ", FRAME),
                s("~\\projects\\cmd-studio \u{b7} ", MUTED),
                s("main * ", DIRTY),
                s(&"\u{2500}".repeat(17), FRAME),
                s(" 16:47", MUTED),
                s(" \u{2500}\u{256e}", FRAME),
            ]
        );
        assert_eq!(prompt_cells(&p.primary[0].text()), 60);
        assert_eq!(
            spans(&p.primary[1]),
            vec![s("\u{2502} ", FRAME), s(">", ACCENT), s(" ", FG)]
        );
        assert_eq!(spans(&p.right), vec![s("\u{2502}", FRAME)]);
        assert_eq!(spans(&p.transient), vec![s("> ", ACCENT)]);
    }

    #[test]
    fn without_time_the_filler_runs_into_the_corner() {
        let p = prompt(&themed(Layout::Box, &[Segment::Path]), &sample(40));
        let top = spans(&p.primary[0]);
        let close = format!("{}\u{2500}\u{256e}", "\u{2500}".repeat(13));
        assert_eq!(top.last().unwrap(), &s(&close, FRAME));
        assert_eq!(prompt_cells(&p.primary[0].text()), 40);
    }

    #[test]
    fn the_line_layout_is_one_line_with_time_last() {
        let theme = themed(Layout::Line, &[Segment::Time, Segment::Path, Segment::Git]);
        let p = prompt(&theme, &sample(80));
        assert_eq!(p.primary.len(), 1);
        assert_eq!(
            spans(&p.primary[0]),
            vec![
                s("~\\projects\\cmd-studio ", BLUE),
                s("main * ", DIRTY),
                s("16:47 ", MUTED),
                s("\u{276f}", ACCENT),
                s(" ", FG),
            ]
        );
        assert!(p.right.spans.is_empty());
    }

    #[test]
    fn powerline_starts_with_a_reversed_arrow_and_joins_blocks_with_arrows() {
        let theme = themed(
            Layout::Powerline,
            &[Segment::Path, Segment::Git, Segment::Time],
        );
        let p = prompt(&theme, &sample(80));
        assert_eq!(
            spans(&p.primary[0]),
            vec![
                // the transparent arrow: the terminal's background on the first block's color
                on("\u{e0b0}", BG, FRAME),
                on(" ~\\projects\\cmd-studio ", FG, FRAME),
                on("\u{e0b0}", FRAME, DIRTY),
                on(" \u{e0a0}main * ", BG, DIRTY),
                on("\u{e0b0}", DIRTY, FRAME),
                on(" 16:47 ", FG, FRAME),
                s("\u{e0b0}", FRAME),
                s(" \u{276f}", ACCENT),
                s(" ", FG),
            ]
        );
    }

    #[test]
    fn segments_the_sample_hides_leave_nothing_behind() {
        // no repository, exit code 0, a duration under 2 seconds
        let ctx = PreviewContext {
            git: None,
            exit_code: 0,
            duration_ms: 1999,
            ..sample(80)
        };
        for layout in [Layout::Line, Layout::Powerline] {
            let text = prompt(&themed(layout, &ALL), &ctx).primary[0].text();
            assert!(!text.contains("main"), "{layout:?}: {text}");
            assert!(!text.contains('\u{2717}'), "{layout:?}: {text}");
            assert!(!text.contains("1.999"), "{layout:?}: {text}");
            assert!(
                !text.contains("  "),
                "{layout:?}: a gap where a segment was: {text:?}"
            );
        }
        // and they show when the sample has them: 2000 ms is enough
        let ctx = PreviewContext {
            exit_code: -1,
            duration_ms: 2000,
            ..sample(80)
        };
        let theme = themed(Layout::Line, &[Segment::Status, Segment::Duration]);
        assert_eq!(
            prompt(&theme, &ctx).primary[0].text(),
            "\u{2717} -1 2s \u{276f} "
        );
    }

    #[test]
    fn git_is_ok_colored_while_clean_and_dirty_colored_once_changed() {
        let clean = PreviewContext {
            git: Some(GitState {
                branch: "feature/x".to_string(),
                dirty: false,
            }),
            ..sample(80)
        };
        let line = &prompt(&themed(Layout::Line, &[Segment::Git]), &clean).primary[0];
        assert_eq!(spans(line)[0], s("feature/x \u{2713} ", OK));
        // git's text is the terminal's background color, like the arrow in front of it, so
        // the two are one span
        let block = &prompt(&themed(Layout::Powerline, &[Segment::Git]), &clean).primary[0];
        assert_eq!(
            spans(block),
            vec![
                on("\u{e0b0} \u{e0a0}feature/x \u{2713} ", BG, OK),
                s("\u{e0b0}", OK),
                s(" \u{276f}", ACCENT),
                s(" ", FG),
            ]
        );
        // the box puts a muted separator first
        let boxed = &prompt(&themed(Layout::Box, &[Segment::Git]), &clean).primary[0];
        assert_eq!(spans(boxed)[1], s("\u{b7} ", MUTED));
        assert_eq!(spans(boxed)[2], s("feature/x \u{2713} ", OK));
    }

    #[test]
    fn user_and_host_are_one_segment_in_every_combination() {
        for (list, text) in [
            (&[Segment::User, Segment::Host][..], "you@PC "),
            (&[Segment::User][..], "you "),
            (&[Segment::Host][..], "PC "),
        ] {
            let line = &prompt(&themed(Layout::Line, list), &sample(80)).primary[0];
            assert_eq!(spans(line)[0], s(text, FG), "{list:?}");
        }
        // the arrow out of the accent block and the prompt character are one span too
        let theme = themed(Layout::Powerline, &[Segment::Host, Segment::User]);
        let block = &prompt(&theme, &sample(80)).primary[0];
        assert_eq!(
            spans(block),
            vec![
                on("\u{e0b0} you@PC ", BG, ACCENT),
                s("\u{e0b0} \u{276f}", ACCENT),
                s(" ", FG),
            ]
        );
    }

    #[test]
    fn status_and_duration_use_red_and_the_dirty_color() {
        let list = [Segment::Status, Segment::Duration];
        let line = &prompt(&themed(Layout::Line, &list), &sample(80)).primary[0];
        assert_eq!(spans(line)[0], s("\u{2717} 1 ", RED));
        assert_eq!(spans(line)[1], s("3.5s ", DIRTY));
        let block = &prompt(&themed(Layout::Powerline, &list), &sample(80)).primary[0];
        assert_eq!(
            spans(block),
            vec![
                on("\u{e0b0} \u{2717} 1 ", BG, RED),
                on("\u{e0b0}", RED, DIRTY),
                on(" 3.5s ", BG, DIRTY),
                s("\u{e0b0}", DIRTY),
                s(" \u{276f}", ACCENT),
                s(" ", FG),
            ]
        );
    }

    #[test]
    fn the_filler_follows_oh_my_posh_at_every_width() {
        // (width, cells written, cells of the right side, filler), measured on Oh My Posh
        // 31.5.0 with Terracotta's box: a left side of 19 cells and a right side of 9, and
        // the empty box (3 and 2)
        for (width, current, length, filler) in [
            (60, 19, 9, 32),
            (33, 19, 9, 5),  // five cells free: room enough
            (32, 19, 9, 23), // four free: the right side goes as if on a new row
            (19, 19, 9, 10),
            (18, 19, 9, 8), // the left side is wider than the window: its last row counts
            (10, 19, 9, 1),
            (9, 19, 9, 0),
            (1, 19, 9, 0),
            (5, 3, 2, 3),
            (30, 3, 2, 25),
        ] {
            assert_eq!(
                filler_cells(width, current, length),
                filler,
                "{width} {current} {length}"
            );
        }
        // a width of 0 is taken as 1
        assert_eq!(filler_cells(0, 19, 9), 0);
    }

    #[test]
    fn the_branch_icon_is_the_one_the_config_leaves() {
        // the box and line configs set `branch_icon` to nothing; powerline keeps Oh My Posh's
        // own, a Nerd Font glyph
        for layout in [Layout::Box, Layout::Line, Layout::Powerline] {
            let theme = themed(layout, &[Segment::Git]);
            let config = omp::config(&theme);
            let git = config["blocks"][0]["segments"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["type"] == "git")
                .unwrap()
                .clone();
            let text = prompt(&theme, &sample(80)).primary[0].text();
            match git["options"].get("branch_icon") {
                Some(icon) => {
                    assert_eq!(icon, "", "{layout:?}");
                    assert!(!text.contains(POWERLINE_BRANCH_ICON), "{layout:?}: {text}");
                }
                None => assert!(
                    text.contains(&format!("{POWERLINE_BRANCH_ICON}main *")),
                    "{layout:?}: {text}"
                ),
            }
        }
    }
}
```
In `crates/core/src/preview/mod.rs`, replace
```rust
mod cells;

pub use cells::{banner_cells, prompt_cells};
```
with
```rust
mod cells;
mod prompt;
mod values;

pub use cells::{banner_cells, prompt_cells};
pub use prompt::{prompt, Prompt};
```
In `crates/core/src/generate/omp.rs`, add this test at the end of `mod tests` (it uses the module's helpers `themed`, `segment_of` and `LAYOUTS`):
```rust
    #[test]
    fn look_changed_is_the_color_the_git_template_switches() {
        // the preview colors a changed repository with `Look::changed`; Oh My Posh with the
        // color template the config gives git
        for layout in LAYOUTS {
            let c = config(&themed(layout, &[Segment::Git]));
            let git = segment_of(&c, "git");
            let look = drawn(layout, Part::Git).look;
            match (look, look.changed()) {
                (Look::Plain { .. }, Look::Plain { fg }) => {
                    assert_eq!(fg, Swatch::Dirty);
                    let t = git["foreground_templates"][0].as_str().unwrap();
                    assert!(t.ends_with("p:dirty{{ end }}"), "{t}");
                }
                (Look::Powerline { fg: before, .. }, Look::Powerline { bg, fg }) => {
                    assert_eq!((bg, fg), (Swatch::Dirty, before));
                    let t = git["background_templates"][0].as_str().unwrap();
                    assert!(t.ends_with("p:dirty{{ end }}"), "{t}");
                }
                other => panic!("{layout:?}: {other:?}"),
            }
        }
    }
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p cmdstudio-core --lib -- preview:: generate::omp`
Expected: the build fails: `prompt`, `Prompt`, `prompt_path`, `austin`, `hh_mm`, `filler_cells` and `Look::changed` are not found.

- [ ] **Step 3: Write the code.** In `crates/core/src/generate/omp.rs`, right after the `enum Look { ... }` definition, add:
```rust
impl Look {
    /// The look git switches to once something has changed: the color its color template
    /// gives (`foreground_templates` in box and line, `background_templates` in powerline).
    pub(crate) fn changed(self) -> Look {
        match self {
            Look::Plain { .. } => Look::Plain { fg: Swatch::Dirty },
            Look::Powerline { fg, .. } => Look::Powerline {
                bg: Swatch::Dirty,
                fg,
            },
        }
    }
}
```
In `crates/core/src/preview/values.rs`, above `#[cfg(test)]`, put:
```rust
//! The text Oh My Posh and banner.lua make of the sample values.
use super::Clock;

/// The `path` segment's text for `cwd` (style `full`, `home_icon` `~`), as Oh My Posh 31.5.0
/// writes it on Windows: `/` becomes `\`, the drive letter is upper case, a trailing `\` goes
/// (a drive's root keeps it: `C:\`, and `D:` becomes `D:\`), and `home` or a folder under it
/// starts with `~`. Windows paths ignore case, so `home` matches in any case and the rest keeps
/// its own. `.` and `..` stay as they are.
pub(crate) fn prompt_path(home: &str, cwd: &str) -> String {
    let home = normalize(home);
    let cwd = normalize(cwd);
    if home.is_empty() {
        return cwd;
    }
    match strip_prefix_ignoring_case(&cwd, &home) {
        Some("") => "~".to_string(),
        Some(rest) if rest.starts_with('\\') => format!("~{rest}"),
        _ => cwd,
    }
}

/// `text` without `prefix` at its start, with letters compared regardless of case.
fn strip_prefix_ignoring_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let mut rest = text.chars();
    for p in prefix.chars() {
        let c = rest.next()?;
        if c != p && !c.to_lowercase().eq(p.to_lowercase()) {
            return None;
        }
    }
    Some(rest.as_str())
}

/// A Windows path the way the path segment starts from it: `\` only, an upper case drive
/// letter, no trailing `\` except after a drive (`C:\`).
fn normalize(path: &str) -> String {
    let slashes = path.replace('/', "\\");
    let mut chars = slashes.chars();
    let lettered = match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic() => {
            format!("{}:{}", letter.to_ascii_uppercase(), chars.as_str())
        }
        _ => slashes.clone(),
    };
    let trimmed = lettered.trim_end_matches('\\');
    let mut rest = trimmed.chars();
    let is_drive = matches!(
        (rest.next(), rest.next(), rest.next()),
        (Some(letter), Some(':'), None) if letter.is_ascii_alphabetic()
    );
    if is_drive {
        format!("{trimmed}\\")
    } else if trimmed.is_empty() && !lettered.is_empty() {
        "\\".to_string()
    } else {
        trimmed.to_string()
    }
}

/// `.FormattedMs` in Oh My Posh's `austin` style: `2s`, `2.5s`, `1m 0s`, `1h 1m 1.001s`,
/// `1d 0h 0m 0s`. The seconds keep up to three decimals with no trailing zeros. Measured on
/// Oh My Posh 31.5.0 from 2000 ms to 2 days (`tests/parity.rs`); the segment shows nothing
/// under 2000 ms, so shorter durations never reach this.
pub(crate) fn austin(ms: u64) -> String {
    const SECOND: u64 = 1000;
    const MINUTE: u64 = 60 * SECOND;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    let within_minute = ms % MINUTE;
    let (whole, thousandths) = (within_minute / SECOND, within_minute % SECOND);
    let seconds = if thousandths == 0 {
        format!("{whole}s")
    } else {
        let digits = format!("{thousandths:03}");
        format!("{whole}.{}s", digits.trim_end_matches('0'))
    };
    if ms >= DAY {
        format!(
            "{}d {}h {}m {seconds}",
            ms / DAY,
            ms % DAY / HOUR,
            ms % HOUR / MINUTE
        )
    } else if ms >= HOUR {
        format!("{}h {}m {seconds}", ms / HOUR, ms % HOUR / MINUTE)
    } else if ms >= MINUTE {
        format!("{}m {seconds}", ms / MINUTE)
    } else {
        seconds
    }
}

/// `HH:MM`: the time segment (Go's `15:04`) and banner.lua's `os.date("%H:%M")` alike.
pub(crate) fn hh_mm(clock: &Clock) -> String {
    format!("{:02}:{:02}", clock.hour, clock.minute)
}

```
In `crates/core/src/preview/prompt.rs`, above `#[cfg(test)]`, put:
```rust
//! The prompt as Oh My Posh 31.5.0 prints the config `generate::omp` writes, for cmd
//! (spec §3.5). Measured on the pinned binary, and checked again by `tests/parity.rs`:
//! - A segment the sample hides (git outside a repository, status 0, a duration under 2 s)
//!   leaves nothing behind, not even its spaces.
//! - A powerline segment starts with the arrow U+E0B0 in the color of the segment before it,
//!   on its own color. When no powerline segment comes before it (it starts the line), Oh My
//!   Posh draws that arrow with a transparent foreground, which it writes as reverse video:
//!   the terminal's background on the segment's color. After the last powerline segment the
//!   arrow is that segment's color on the terminal's background.
//! - The box's top line is filled with `─` up to its right side ([`filler_cells`]).
//! - `final_space` puts one space in the terminal's colors at the end of the last line. The
//!   window title Oh My Posh prints just before that space is not drawn.
//! - The right prompt and the transient prompt are printed alone, with no final space.
use super::cells::prompt_cells;
use super::values::{austin, hh_mm, prompt_path};
use super::{Line, PreviewContext, Style};
use crate::color::Rgb;
use crate::generate::omp::{self, Look, Part, Swatch};
use crate::model::{Layout, Theme};
use crate::palette::Colors;
use serde::{Deserialize, Serialize};

/// Oh My Posh's own branch icon (a Nerd Font glyph). The config replaces it with nothing in
/// the box and line layouts; powerline keeps it.
const POWERLINE_BRANCH_ICON: &str = "\u{e0a0}";

/// The cells Oh My Posh wants to stay free on the line before it draws a right-aligned block
/// on it (engine.go, `promptBreathingRoom`).
const BREATHING_ROOM: usize = 5;

/// What Oh My Posh prints when Clink asks it for the prompt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prompt {
    /// `print primary`: one line, or two for the box. The command is typed at the end of
    /// the last one, after the final space.
    pub primary: Vec<Line>,
    /// `print right`: the box's right side, which Clink draws at the right edge of the line
    /// the command is typed on. Empty in the line and powerline layouts.
    pub right: Line,
    /// `print transient`: what Clink shrinks an earlier prompt to, when the theme asks for it.
    pub transient: Line,
}

/// The prompt Oh My Posh 31.5.0 prints for `theme` and the sample `ctx`.
pub fn prompt(theme: &Theme, ctx: &PreviewContext) -> Prompt {
    let colors = &theme.colors;
    let layout = theme.prompt.layout;
    let paint = |swatch: Swatch| Style::fg(swatch.color(colors));
    let mut primary = Vec::new();
    let mut right = Line::default();
    match layout {
        Layout::Box => {
            let mut left = Block::new(colors);
            left.text(Swatch::Frame, omp::BOX_TOP);
            for part in omp::left_parts(theme) {
                left.part(layout, part, ctx);
            }
            let mut top = left.finish();
            let mut side = Block::new(colors);
            if omp::has_time(theme) {
                side.part(layout, Part::Time, ctx);
                side.text(Swatch::Frame, omp::BOX_CLOSE_AFTER_TIME);
            } else {
                side.text(Swatch::Frame, omp::BOX_CLOSE);
            }
            let side = side.finish();
            let cells = filler_cells(
                usize::from(ctx.width),
                prompt_cells(&top.text()),
                prompt_cells(&side.text()),
            );
            // as `shouldFill` does: the filler as often as it fits, then spaces
            let unit = prompt_cells(omp::BOX_FILLER).max(1);
            top.push(&omp::BOX_FILLER.repeat(cells / unit), paint(Swatch::Frame));
            top.push(&" ".repeat(cells % unit), Style::fg(colors.foreground));
            top.append(&side);
            let mut second = Line::default();
            second.push(omp::BOX_SIDE, paint(Swatch::Frame));
            second.push(omp::BOX_PROMPT, paint(Swatch::Accent));
            primary.push(top);
            primary.push(second);
            right.push(omp::BOX_RIGHT, paint(Swatch::Frame));
        }
        Layout::Line | Layout::Powerline => {
            let mut line = Block::new(colors);
            for part in omp::left_parts(theme) {
                line.part(layout, part, ctx);
            }
            line.text(Swatch::Accent, omp::prompt_char(layout));
            primary.push(line.finish());
        }
    }
    // final_space
    if let Some(last) = primary.last_mut() {
        last.push(" ", Style::fg(colors.foreground));
    }
    let mut transient = Line::default();
    transient.push(omp::TRANSIENT_PROMPT, paint(Swatch::Accent));
    Prompt {
        primary,
        right,
        transient,
    }
}

/// How many cells Oh My Posh 31.5.0 fills before a right-aligned block of `length` cells,
/// when `current` cells of the line are already written (engine.go: `canWriteRightBlock`,
/// then `shouldFill`): the cells that are left, if at least [`BREATHING_ROOM`] of them stay
/// free; otherwise the whole width less the block, as if the block began a new row. A left
/// side wider than the window counts from where its last row ends. A width of 0 is taken as
/// 1.
fn filler_cells(width: usize, current: usize, length: usize) -> usize {
    let width = width.max(1);
    let free = if current <= width {
        width - current
    } else {
        width - current % width
    };
    match free.checked_sub(length) {
        Some(room) if room >= BREATHING_ROOM => room,
        _ => width.saturating_sub(length),
    }
}

/// The data of `part` for the sample, or `None` when Oh My Posh hides the segment.
fn data(layout: Layout, part: Part, ctx: &PreviewContext) -> Option<String> {
    match part {
        Part::Path => Some(prompt_path(&ctx.home, &ctx.cwd)),
        Part::Git => ctx.git.as_ref().map(|git| {
            let icon = match layout {
                Layout::Powerline => POWERLINE_BRANCH_ICON,
                Layout::Box | Layout::Line => "",
            };
            let mark = if git.dirty {
                omp::GIT_DIRTY_MARK
            } else {
                omp::GIT_CLEAN_MARK
            };
            format!("{icon}{}{mark}", git.branch)
        }),
        Part::Time => Some(hh_mm(&ctx.clock)),
        Part::Session { user, host } => Some(match (user, host) {
            (true, true) => format!("{}@{}", ctx.user, ctx.host),
            (true, false) => ctx.user.clone(),
            (false, _) => ctx.host.clone(),
        }),
        Part::Status => {
            (ctx.exit_code != 0).then(|| format!("{}{}", omp::STATUS_MARK, ctx.exit_code))
        }
        Part::Duration => {
            (ctx.duration_ms >= omp::DURATION_THRESHOLD_MS).then(|| austin(ctx.duration_ms))
        }
    }
}

/// The segments of one block, joined the way Oh My Posh joins them.
struct Block<'a> {
    colors: &'a Colors,
    line: Line,
    /// The color behind the last segment, while that segment is a powerline one.
    open: Option<Rgb>,
}

impl<'a> Block<'a> {
    fn new(colors: &'a Colors) -> Self {
        Block {
            colors,
            line: Line::default(),
            open: None,
        }
    }

    /// One of the generator's own `text` segments: `text` in `swatch`.
    fn text(&mut self, swatch: Swatch, text: &str) {
        self.close();
        self.line.push(text, Style::fg(swatch.color(self.colors)));
    }

    /// The segment of `part`, unless the sample hides it.
    fn part(&mut self, layout: Layout, part: Part, ctx: &PreviewContext) {
        let Some(data) = data(layout, part, ctx) else {
            return;
        };
        let drawn = omp::drawn(layout, part);
        let changed = part == Part::Git && ctx.git.as_ref().is_some_and(|git| git.dirty);
        let look = if changed {
            drawn.look.changed()
        } else {
            drawn.look
        };
        let style = match look {
            Look::Plain { fg } => {
                self.close();
                Style::fg(fg.color(self.colors))
            }
            Look::Powerline { bg, fg } => {
                let bg = bg.color(self.colors);
                // the arrow in: from the segment before, or transparent (reverse video)
                let from = self.open.unwrap_or(self.colors.background);
                self.line.push(
                    omp::POWERLINE_SYMBOL,
                    Style {
                        fg: from,
                        bg: Some(bg),
                        bold: false,
                    },
                );
                self.open = Some(bg);
                Style {
                    fg: fg.color(self.colors),
                    bg: Some(bg),
                    bold: false,
                }
            }
        };
        if let Some((swatch, lead)) = drawn.lead {
            let lead_style = Style {
                fg: swatch.color(self.colors),
                ..style
            };
            self.line.push(lead, lead_style);
        }
        self.line.push(drawn.before, style);
        self.line.push(&data, style);
        self.line.push(drawn.after, style);
    }

    /// Ends a run of powerline segments: the arrow out of the last one.
    fn close(&mut self) {
        if let Some(bg) = self.open.take() {
            self.line.push(omp::POWERLINE_SYMBOL, Style::fg(bg));
        }
    }

    fn finish(mut self) -> Line {
        self.close();
        self.line
    }
}

```
Two things a reviewer should know about this code:
- **Spans join.** In powerline, git's, the session's, the status' and the duration's text is `p:bg`, the terminal's background color, which is also the color the transparent leading arrow shows. `Line::push` therefore joins the leading arrow and that segment's text into one span when the segment comes first, and the parity test reads Oh My Posh's output through the same `push`, so both sides join alike. The tests above spell those joined spans out.
- **No `_` arms.** `Layout` and `Segment` are `#[non_exhaustive]` for other crates only; inside `cmdstudio-core` every `match` names each variant, so a new layout or segment fails to compile here instead of drawing nothing.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cmdstudio-core --lib -- preview:: generate::omp`
Expected: `test result: ok. 45 passed` (preview: 6 from Task 2, 3 in `values`, 10 in `prompt`; generate::omp: 25 from Task 1 and `look_changed_is_the_color_the_git_template_switches`).

- [ ] **Step 5: Format, lint, docs**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo doc -p cmdstudio-core --no-deps`
Expected: no warnings.

- [ ] **Step 6: Commit**

Message:
```text
feat(core): draw the prompt as Oh My Posh 31.5.0 prints it

preview::prompt gives what `oh-my-posh print primary`, `print right`
and `print transient` print for cmd, as spans, from the shared table
of generate::omp: hidden segments leave nothing, powerline starts with
the reversed arrow Oh My Posh draws and joins blocks with arrows, the
box's top line is filled to the width with Oh My Posh's breathing room
of five cells, the path shows home as ~, durations are in the austin
style, and the final space ends the last line. Each of these was
measured on the pinned binary; the parity test checks them again.
```
Run:
```bash
git add -- crates/core/src/preview/mod.rs crates/core/src/preview/values.rs crates/core/src/preview/prompt.rs crates/core/src/generate/omp.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 4: The parity test against the real Oh My Posh 31.5.0

Spec §5.6 and §7: the emulator against `oh-my-posh print` itself, character by character and color by color, for every layout × every set of segments. This task adds no production code; the emulator of Task 3 must pass it as it stands. If a case fails, the message names the case and shows both lines; fix the emulator (Task 3's code), never the test's expectations.

**Files:**
- Create: `crates/core/tests/sgr/mod.rs` (reads printed SGR into spans; Task 7 uses it too), `crates/core/tests/parity.rs`

**Interfaces:**
- Consumes: `preview::{prompt, Prompt, PreviewContext, Clock, GitState, Line, Style}` (Tasks 2–3), `generate::omp::config_text`, `model::{Layout, Segment, Theme}`, `presets::builtin()`, `color::Rgb`; `uuid` (a dependency of the crate, so tests can use it); Phase 1b's `resources\omp\oh-my-posh.exe`; `git` on `PATH`.
- Produces: `tests/sgr/mod.rs` with `pub fn parse_sgr(out: &str, fg: Rgb, bg: Rgb) -> Result<Vec<Line>, String>` and `pub fn difference(want: &[Line], got: &[Line]) -> Option<String>` (a `mod sgr;` in a test file includes it; `tests/sgr/` is not a test target of its own); the test `the_emulator_draws_what_oh_my_posh_prints`; the environment variable `CMDSTUDIO_ALLOW_PARITY_SKIP`.

How the test works (the code says the same in its comments):
- It refuses to run anything but `31.5.0` (`oh-my-posh version`).
- It builds a stage in a new temp folder: a fake home folder with a folder in no repository and five repositories made with git (`main` clean; `feature/preview` with an edited file; `main` with a staged file; `main` with an untracked file; `งาน/ทดสอบ` clean). git runs with `HOME` and `USERPROFILE` pointed at the fake home and `GIT_CONFIG_NOSYSTEM=1`, so the developer's own git settings (commit signing, say) play no part.
- Every Oh My Posh run gets a cleared environment: `PATH`, `SYSTEMROOT` and `WINDIR` from the parent; `USERPROFILE`/`HOME` = the fake home; `USER`/`USERNAME` = the sample user; `LOCALAPPDATA` = a cache folder of the worker thread's own inside the temp folder, with `POSH_SESSION_ID` = `parity-<worker>` (fact 12: otherwise each run would leave a 50 KB file in the real `%LOCALAPPDATA%\oh-my-posh`); `TEMP`/`TMP` = the temp folder; `CREATE_NO_WINDOW`.
- Two probe configs, written once, read what the test cannot set: the host name (`{{ .HostName }}`) and the minute (`{{ .CurrentDate | date .Format }}` with `15:04`). Each batch of 16 cases runs between two readings of the minute and runs again when they differ (at most five times).
- The cases (713 with Terracotta alone, 746 once Tasks 8 and 9 add the other presets): (1) every layout × all 128 subsets of the seven segments in the file format's order, the git, status and duration states taking turns, with `print primary`, `print right` and `print transient`; (2) three other orders of the segments × every git state × exit codes 0 and 2 × durations 0 and 2000 ms; (3) the box at widths 1, 9, 10, 18, 20, 32, 33, 34, 40, 61, 80 and 200 with four segment lists; (4) eight folders (home itself, under it, home in upper case, another drive, `C:\`, Thai with marks, Chinese, emoji and a joined family emoji); (5) eleven durations from 1999 ms to 25 hours; (6) seven exit codes including −1 and −1073741510; (7) three user names alone and with the host; (8) every built-in preset in every layout with every segment showing.

- [ ] **Step 1: Write the SGR reader.** Create `crates/core/tests/sgr/mod.rs`:
```rust
//! Reads what a program printed for a terminal into the preview's lines of spans, so the
//! parity tests can compare it with the emulator. Shared by `parity.rs` and
//! `banner_parity.rs`.
use cmdstudio_core::color::Rgb;
use cmdstudio_core::preview::{Line, Style};

/// The colors SGR has set so far.
#[derive(Clone, Copy, Default)]
struct Sgr {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    bold: bool,
    reverse: bool,
}

impl Sgr {
    /// Applies the parameters of one `ESC [ ... m`.
    fn apply(&mut self, params: &str) -> Result<(), String> {
        let mut codes = params.split(';').map(|p| {
            if p.is_empty() {
                Ok(0)
            } else {
                p.parse::<u16>().map_err(|e| format!("SGR {params:?}: {e}"))
            }
        });
        while let Some(code) = codes.next() {
            match code? {
                0 => *self = Sgr::default(),
                1 => self.bold = true,
                22 => self.bold = false,
                7 => self.reverse = true,
                27 => self.reverse = false,
                39 => self.fg = None,
                49 => self.bg = None,
                which @ (38 | 48) => {
                    let mut next = || {
                        codes
                            .next()
                            .ok_or_else(|| format!("SGR {params:?}: a color cut short"))?
                    };
                    if next()? != 2 {
                        return Err(format!("SGR {params:?}: only 24-bit colors are expected"));
                    }
                    let mut channel = || {
                        next().and_then(|v| {
                            u8::try_from(v).map_err(|_| format!("SGR {params:?}: {v} > 255"))
                        })
                    };
                    let rgb = Rgb {
                        r: channel()?,
                        g: channel()?,
                        b: channel()?,
                    };
                    if which == 38 {
                        self.fg = Some(rgb);
                    } else {
                        self.bg = Some(rgb);
                    }
                }
                other => return Err(format!("SGR {params:?}: code {other} is not expected")),
            }
        }
        Ok(())
    }

    /// What the terminal draws: the default foreground in `fg`, the default background as
    /// `None`, and the two swapped by reverse video (which brings the background color out
    /// as a foreground: `bg`).
    fn style(&self, fg: Rgb, bg: Rgb) -> Style {
        let front = self.fg.unwrap_or(fg);
        if self.reverse {
            Style {
                fg: self.bg.unwrap_or(bg),
                bg: Some(front),
                bold: self.bold,
            }
        } else {
            Style {
                fg: front,
                bg: self.bg,
                bold: self.bold,
            }
        }
    }
}

/// `out` as lines of spans. SGR sets the colors (24-bit foreground and background, the
/// defaults, reset, bold, reverse video); `\n` starts a line; an OSC sequence (the window
/// title) is dropped. Anything else that is not text is an error, so nothing the emulator
/// does not model can slip through. `fg` and `bg` are the terminal's own colors.
pub fn parse_sgr(out: &str, fg: Rgb, bg: Rgb) -> Result<Vec<Line>, String> {
    let mut lines = vec![Line::default()];
    let mut sgr = Sgr::default();
    let mut rest = out;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("\u{1b}[") {
            let end = after
                .find(|c: char| !(c.is_ascii_digit() || c == ';'))
                .ok_or("a CSI sequence cut short")?;
            let (params, tail) = after.split_at(end);
            rest = tail.strip_prefix('m').ok_or_else(|| {
                format!("a CSI sequence that is not SGR: {:?}", tail.chars().next())
            })?;
            sgr.apply(params)?;
        } else if let Some(after) = rest.strip_prefix("\u{1b}]") {
            let end = after
                .find(['\u{7}', '\u{1b}'])
                .ok_or("an OSC sequence cut short")?;
            let (_, tail) = after.split_at(end);
            rest = tail
                .strip_prefix('\u{7}')
                .or_else(|| tail.strip_prefix("\u{1b}\\"))
                .ok_or("an OSC sequence with no end")?;
        } else if let Some(after) = rest.strip_prefix('\n') {
            lines.push(Line::default());
            rest = after;
        } else {
            let end = rest.find(['\u{1b}', '\n']).unwrap_or(rest.len());
            let (text, tail) = rest.split_at(end);
            if text.is_empty() {
                return Err(format!(
                    "an escape sequence that is not expected: {:?}",
                    tail.chars().take(4).collect::<String>()
                ));
            }
            if let Some(bad) = text.chars().find(|c| c.is_control()) {
                return Err(format!(
                    "the control character U+{:04X} in {text:?}",
                    u32::from(bad)
                ));
            }
            if let Some(line) = lines.last_mut() {
                line.push(text, sgr.style(fg, bg));
            }
            rest = tail;
        }
    }
    Ok(lines)
}

/// A short description of how two lists of lines differ: the first line that does, with
/// both versions.
pub fn difference(want: &[Line], got: &[Line]) -> Option<String> {
    if want == got {
        return None;
    }
    let shown = |line: Option<&Line>| match line {
        None => "(no line)".to_string(),
        Some(line) => line
            .spans
            .iter()
            .map(|s| {
                let bg = s.bg.map_or("-".to_string(), |bg| bg.to_string());
                format!(
                    "[{} on {bg}{}] {:?}",
                    s.fg,
                    if s.bold { " bold" } else { "" },
                    s.text
                )
            })
            .collect::<Vec<_>>()
            .join(" "),
    };
    let row = (0..want.len().max(got.len()))
        .find(|&i| want.get(i) != got.get(i))
        .unwrap_or(0);
    Some(format!(
        "line {} of {}/{}:\n  emulator: {}\n  real:     {}",
        row + 1,
        want.len(),
        got.len(),
        shown(want.get(row)),
        shown(got.get(row))
    ))
}

#[test]
fn the_parser_reads_colors_reverse_video_and_titles() {
    let fg = Rgb {
        r: 250,
        g: 249,
        b: 245,
    };
    let bg = Rgb {
        r: 31,
        g: 30,
        b: 29,
    };
    let frame = Rgb {
        r: 95,
        g: 93,
        b: 88,
    };
    // the leading arrow of a powerline prompt, then a segment, the title and the final space
    let out = "\u{1b}[0m\u{1b}[38;2;95;93;88;49m\u{1b}[7m\u{e0b0}\u{1b}[0m\u{1b}[48;2;95;93;88m\u{1b}[38;2;250;249;245m ~ \u{1b}[0m\u{1b}]0;title\u{7} ";
    let lines = parse_sgr(out, fg, bg).unwrap();
    assert_eq!(lines.len(), 1);
    let spans = &lines[0].spans;
    assert_eq!(spans.len(), 3, "{spans:?}");
    assert_eq!(
        (spans[0].text.as_str(), spans[0].fg, spans[0].bg),
        ("\u{e0b0}", bg, Some(frame))
    );
    assert_eq!(
        (spans[1].text.as_str(), spans[1].fg, spans[1].bg),
        (" ~ ", fg, Some(frame))
    );
    assert_eq!(
        (spans[2].text.as_str(), spans[2].fg, spans[2].bg),
        (" ", fg, None)
    );
    // lines, and what is refused
    assert_eq!(parse_sgr("a\nb", fg, bg).unwrap().len(), 2);
    assert!(parse_sgr("\u{1b}[2J", fg, bg).is_err());
    assert!(parse_sgr("\u{1b}[38;5;3m", fg, bg).is_err());
    assert!(parse_sgr("a\rb", fg, bg).is_err());
    assert!(parse_sgr("\u{1b}(B", fg, bg).is_err());
}
```

- [ ] **Step 2: Write the parity test.** Create `crates/core/tests/parity.rs`:
```rust
//! Parity test (spec §5.6, §7): the preview emulator against the real Oh My Posh, cell by
//! cell and color by color, for every layout × every set of segments.
//!
//! It runs the pinned binary that `scripts/fetch-resources.ps1` puts in
//! `resources\omp\oh-my-posh.exe` on the config `generate::omp` writes:
//! `oh-my-posh print primary|right|transient --config <file> --shell cmd --terminal-width <n>
//! --pwd <dir> --status <code> --execution-time <ms>`, reads the colors out of what it prints
//! and compares that with `preview::prompt`. Git's states come from throwaway repositories in
//! a temp folder; the user name, the home folder and Oh My Posh's cache folder are set for
//! each run, so nothing of this machine's own setup leaks in and nothing is written outside
//! the temp folder. The host name and the clock are read from Oh My Posh itself.
//!
//! Without the binary the test fails, saying how to get it, unless
//! `CMDSTUDIO_ALLOW_PARITY_SKIP=1` is set: then it prints `SKIPPED` and passes. CI never sets
//! it.
#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod sgr;

use cmdstudio_core::generate::omp::config_text;
use cmdstudio_core::model::{Layout, Segment, Theme};
use cmdstudio_core::presets;
use cmdstudio_core::preview::{prompt, Clock, GitState, Line, PreviewContext, Prompt};
use sgr::{difference, parse_sgr};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// The version the emulator follows (spec §6.4: OMP is pinned).
const PINNED: &str = "31.5.0";
/// Set to `1` to let the test pass on a machine without the binary.
const ALLOW_SKIP: &str = "CMDSTUDIO_ALLOW_PARITY_SKIP";
/// Keeps the child processes from opening console windows (spec §2.2 #7).
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Every segment, in the order of the file format.
const ALL: [Segment; 7] = [
    Segment::Path,
    Segment::Git,
    Segment::Time,
    Segment::User,
    Segment::Host,
    Segment::Status,
    Segment::Duration,
];
const LAYOUTS: [Layout; 3] = [Layout::Box, Layout::Line, Layout::Powerline];

/// A folder in the temp folder, removed again when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        let path = std::env::temp_dir().join(format!("cmdstudio-parity-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The pinned binary, or `None` when it is missing and skipping is allowed.
fn pinned_omp() -> Option<PathBuf> {
    let exe = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(r"..\..\resources\omp\oh-my-posh.exe");
    if exe.is_file() {
        return Some(exe);
    }
    if std::env::var(ALLOW_SKIP).is_ok_and(|v| v == "1") {
        println!(
            "SKIPPED parity: {} is missing ({ALLOW_SKIP}=1)",
            exe.display()
        );
        return None;
    }
    panic!(
        "the parity test needs Oh My Posh {PINNED} at {}. Run scripts\\fetch-resources.ps1, \
         or set {ALLOW_SKIP}=1 to skip the test on a machine without it.",
        exe.display()
    );
}

/// Runs git with no global or system settings of this machine.
fn git(home: &Path, dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "core.autocrlf=false", "-c", "commit.gpgsign=false"])
        .args([
            "-c",
            "user.name=CMD Studio",
            "-c",
            "user.email=parity@example.invalid",
        ])
        .args(args)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("git runs (the parity test needs git on PATH)");
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repository in `home\name` on `branch` with one commit, then `change` made to it.
fn repo(home: &Path, name: &str, branch: &str, change: &str) -> PathBuf {
    let dir = home.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    git(home, &dir, &["init", "-q", "-b", branch]);
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    git(home, &dir, &["add", "a.txt"]);
    git(home, &dir, &["commit", "-q", "-m", "parity"]);
    match change {
        "none" => {}
        "edited" => std::fs::write(dir.join("a.txt"), "a\nb\n").unwrap(),
        "staged" => {
            std::fs::write(dir.join("b.txt"), "b\n").unwrap();
            git(home, &dir, &["add", "b.txt"]);
        }
        "untracked" => std::fs::write(dir.join("u.txt"), "u\n").unwrap(),
        other => panic!("no change {other}"),
    }
    dir
}

/// A config of one segment of `kind` with `template`, written once: how the test learns
/// what it cannot set, the host name and the clock.
fn probe(root: &Path, kind: &str, template: &str) -> PathBuf {
    let config = root.join(format!("probe-{kind}.omp.json"));
    let json = serde_json::json!({
        "version": 4,
        "blocks": [{ "type": "prompt", "alignment": "left", "segments": [
            { "type": kind, "style": "plain", "template": template, "options": { "time_format": "15:04" } }
        ] }]
    });
    std::fs::write(&config, json.to_string()).unwrap();
    config
}

/// Where Oh My Posh may run: the folders it reads and writes, and the git states.
struct Stage {
    exe: PathBuf,
    root: TempDir,
    home: PathBuf,
    /// A folder in no repository.
    plain: PathBuf,
    /// The repositories, with the state the prompt shows for each.
    repos: Vec<(PathBuf, GitState)>,
    host: String,
    /// A config of one time segment, which prints the minute Oh My Posh sees.
    clock: PathBuf,
}

impl Stage {
    fn new(exe: PathBuf) -> Stage {
        let root = TempDir::new();
        let home = root.0.join("home");
        let plain = home.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let state = |branch: &str, dirty| GitState {
            branch: branch.to_string(),
            dirty,
        };
        let repos = vec![
            (repo(&home, "clean", "main", "none"), state("main", false)),
            (
                repo(&home, "edited", "feature/preview", "edited"),
                state("feature/preview", true),
            ),
            (repo(&home, "staged", "main", "staged"), state("main", true)),
            (
                repo(&home, "untracked", "main", "untracked"),
                state("main", true),
            ),
            (
                repo(&home, "thai", "งาน/ทดสอบ", "none"),
                state("งาน/ทดสอบ", false),
            ),
        ];
        let host = probe(&root.0, "session", "{{ .HostName }}");
        let clock = probe(&root.0, "time", "{{ .CurrentDate | date .Format }}");
        let mut stage = Stage {
            exe,
            root,
            home,
            plain,
            repos,
            host: String::new(),
            clock,
        };
        stage.host = stage.read(&host, "setup");
        stage
    }

    /// What Oh My Posh prints, as plain text, for one of the `probe` configs.
    fn read(&self, config: &Path, worker: &str) -> String {
        let ctx = self.context(&self.plain, None);
        let out = self.run(config, "primary", &ctx, &["--plain"], worker);
        out.trim().to_string()
    }

    /// The sample for a run in `cwd`.
    fn context(&self, cwd: &Path, git: Option<GitState>) -> PreviewContext {
        PreviewContext {
            user: "tester".to_string(),
            host: self.host.clone(),
            home: self.home.to_string_lossy().into_owned(),
            cwd: cwd.to_string_lossy().into_owned(),
            clock: Clock {
                year: 2026,
                month: 10,
                day: 9,
                hour: 0,
                minute: 0,
            },
            git,
            exit_code: 0,
            duration_ms: 0,
            width: 80,
            windows_terminal: true,
            command: String::new(),
        }
    }

    /// One run of Oh My Posh. `worker` names the cache folder, so parallel runs never share
    /// one.
    fn run(
        &self,
        config: &Path,
        kind: &str,
        ctx: &PreviewContext,
        extra: &[&str],
        worker: &str,
    ) -> String {
        let cache = self.root.0.join("cache").join(worker);
        std::fs::create_dir_all(&cache).unwrap();
        let mut cmd = Command::new(&self.exe);
        cmd.args(["print", kind, "--shell", "cmd", "--config"])
            .arg(config)
            .arg("--pwd")
            .arg(&ctx.cwd)
            .args(["--terminal-width", &ctx.width.to_string()])
            .args(["--status", &ctx.exit_code.to_string()])
            .args(["--execution-time", &ctx.duration_ms.to_string()])
            .args(extra)
            .env_clear()
            .env("USERPROFILE", &ctx.home)
            .env("HOME", &ctx.home)
            .env("USER", &ctx.user)
            .env("USERNAME", &ctx.user)
            .env("LOCALAPPDATA", &cache)
            .env("POSH_SESSION_ID", format!("parity-{worker}"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("TEMP", &self.root.0)
            .env("TMP", &self.root.0)
            .creation_flags(CREATE_NO_WINDOW);
        for key in ["PATH", "SYSTEMROOT", "WINDIR"] {
            if let Some(value) = std::env::var_os(key) {
                cmd.env(key, value);
            }
        }
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "oh-my-posh print {kind}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}

/// One comparison: a theme, the sample, and which prompts to print.
struct Case {
    what: String,
    theme: Theme,
    config: PathBuf,
    ctx: PreviewContext,
    /// `print right` and `print transient` too, not only `print primary`.
    all_three: bool,
}

/// Builds the cases, writing one config file for each.
struct Cases<'a> {
    stage: &'a Stage,
    list: Vec<Case>,
}

impl Cases<'_> {
    fn add(&mut self, what: String, theme: Theme, ctx: PreviewContext, all_three: bool) {
        theme.validate().unwrap();
        let config = self
            .stage
            .root
            .0
            .join(format!("{}.omp.json", self.list.len()));
        std::fs::write(&config, config_text(&theme).unwrap()).unwrap();
        self.list.push(Case {
            what,
            theme,
            config,
            ctx,
            all_three,
        });
    }
}

/// The Terracotta preset with this layout and these segments.
fn terracotta(layout: Layout, segments: &[Segment]) -> Theme {
    let mut theme = presets::builtin()
        .unwrap()
        .into_iter()
        .find(|(key, _)| *key == "terracotta")
        .unwrap()
        .1;
    theme.prompt.layout = layout;
    theme.prompt.segments = segments.to_vec();
    theme
}

/// Every subset of `ALL`, each in the order of `ALL`.
fn subsets() -> Vec<Vec<Segment>> {
    (0u32..1 << ALL.len())
        .map(|mask| {
            ALL.iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, s)| *s)
                .collect()
        })
        .collect()
}

fn all_cases(stage: &Stage) -> Vec<Case> {
    let mut cases = Cases {
        stage,
        list: Vec::new(),
    };
    // the git states: none, and each repository
    let mut gits: Vec<(PathBuf, Option<GitState>)> = vec![(stage.plain.clone(), None)];
    gits.extend(
        stage
            .repos
            .iter()
            .map(|(dir, state)| (dir.clone(), Some(state.clone()))),
    );
    let with =
        |cwd: &Path, git: Option<GitState>, exit_code: i32, duration_ms: u64| PreviewContext {
            exit_code,
            duration_ms,
            ..stage.context(cwd, git)
        };

    // 1. every layout × every set of segments, the states taking turns; all three prompts
    for layout in LAYOUTS {
        for (i, segments) in subsets().into_iter().enumerate() {
            let (cwd, git) = &gits[i % gits.len()];
            let exit_code = [0, 1][i / gits.len() % 2];
            let duration_ms = [1999, 3500][i / (2 * gits.len()) % 2];
            let ctx = with(cwd, git.clone(), exit_code, duration_ms);
            let what =
                format!("{layout:?} {segments:?} git={git:?} exit={exit_code} ms={duration_ms}");
            cases.add(what, terracotta(layout, &segments), ctx, true);
        }
    }
    // 2. orders that are not the file format's, in every state
    let orders: [&[Segment]; 3] = [
        &[
            Segment::Time,
            Segment::Status,
            Segment::User,
            Segment::Path,
            Segment::Duration,
            Segment::Host,
            Segment::Git,
        ],
        &[
            Segment::Duration,
            Segment::Status,
            Segment::Host,
            Segment::User,
            Segment::Time,
            Segment::Git,
            Segment::Path,
        ],
        &[Segment::Host, Segment::Git, Segment::Path, Segment::User],
    ];
    for layout in LAYOUTS {
        for order in orders {
            for (cwd, git) in &gits {
                for exit_code in [0, 2] {
                    for duration_ms in [0, 2000] {
                        let ctx = with(cwd, git.clone(), exit_code, duration_ms);
                        let what = format!(
                            "{layout:?} {order:?} git={git:?} exit={exit_code} ms={duration_ms}"
                        );
                        cases.add(what, terracotta(layout, order), ctx, false);
                    }
                }
            }
        }
    }
    // 3. the box's filler at every kind of width: room to spare, just enough (5 cells free),
    //    one short, a left side wider than the window
    let lists: [&[Segment]; 4] = [
        &[],
        &[Segment::Path],
        &[Segment::Path, Segment::Git, Segment::Time],
        &ALL,
    ];
    for list in lists {
        for width in [1, 9, 10, 18, 20, 32, 33, 34, 40, 61, 80, 200] {
            let ctx = PreviewContext {
                width,
                ..with(&stage.repos[0].0, Some(stage.repos[0].1.clone()), 1, 2500)
            };
            cases.add(
                format!("Box {list:?} width {width}"),
                terracotta(Layout::Box, list),
                ctx,
                false,
            );
        }
    }
    // 4. paths: the home folder itself, under it, elsewhere, a drive's root, Thai letters with
    //    their marks, Chinese, emoji (the filler counts their cells); none of them needs to exist
    let home = stage.home.to_string_lossy().into_owned();
    for cwd in [
        home.clone(),
        format!(r"{home}\projects\cmd-studio"),
        home.to_uppercase(),
        r"D:\work\x".to_string(),
        r"C:\".to_string(),
        format!(r"{home}\ผู้ทดสอบ\ที่นี่"),
        r"D:\中文\文件".to_string(),
        "D:\\\u{1f4c1}\\\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\\\u{2764}\u{fe0f}".to_string(),
    ] {
        for (layout, list) in [
            (Layout::Box, &[Segment::Path, Segment::Time][..]),
            (Layout::Line, &[Segment::Path][..]),
            (Layout::Powerline, &[Segment::Path][..]),
        ] {
            let ctx = with(Path::new(&cwd), None, 0, 0);
            cases.add(
                format!("{layout:?} path {cwd}"),
                terracotta(layout, list),
                ctx,
                false,
            );
        }
    }
    // 5. durations in the austin style, around the threshold and across units
    for ms in [
        1999, 2000, 2001, 2010, 2100, 59_999, 60_000, 61_500, 3_600_000, 3_661_001, 90_061_001,
    ] {
        for layout in [Layout::Line, Layout::Powerline] {
            let ctx = with(&stage.plain, None, 0, ms);
            cases.add(
                format!("{layout:?} duration {ms}"),
                terracotta(layout, &[Segment::Duration]),
                ctx,
                false,
            );
        }
    }
    // 6. exit codes, negative ones and an NTSTATUS among them
    for code in [1, -1, 2, 127, 255, 9009, -1_073_741_510] {
        let ctx = with(&stage.plain, None, code, 0);
        cases.add(
            format!("Line status {code}"),
            terracotta(Layout::Line, &[Segment::Status]),
            ctx,
            false,
        );
    }
    // 7. user names, alone and with the host
    for user in ["tester", "ผู้ทดสอบ ไทย", "a.b-c"] {
        for list in [
            &[Segment::User][..],
            &[Segment::Host][..],
            &[Segment::User, Segment::Host][..],
        ] {
            let ctx = PreviewContext {
                user: user.to_string(),
                ..with(&stage.plain, None, 0, 0)
            };
            cases.add(
                format!("Powerline {list:?} user {user}"),
                terracotta(Layout::Powerline, list),
                ctx,
                false,
            );
        }
    }
    // 8. every built-in preset in every layout, with every segment showing
    for (key, preset) in presets::builtin().unwrap() {
        for layout in LAYOUTS {
            let mut theme = preset.clone();
            theme.prompt.layout = layout;
            theme.prompt.segments = ALL.to_vec();
            let ctx = with(&stage.repos[1].0, Some(stage.repos[1].1.clone()), 1, 3500);
            cases.add(format!("{key} {layout:?}"), theme, ctx, true);
        }
    }
    cases.list
}

/// What Oh My Posh printed for one case.
struct Printed {
    primary: String,
    right: Option<String>,
    transient: Option<String>,
}

/// Runs one batch of cases between two readings of Oh My Posh's clock, until a batch starts
/// and ends in the same minute. Returns the minute and what was printed.
fn run_batch(stage: &Stage, batch: &[Case], worker: &str) -> (Clock, Vec<Printed>) {
    for _ in 0..5 {
        let before = stage.read(&stage.clock, worker);
        let printed: Vec<Printed> = batch
            .iter()
            .map(|case| {
                let run = |kind| stage.run(&case.config, kind, &case.ctx, &[], worker);
                Printed {
                    primary: run("primary"),
                    right: case.all_three.then(|| run("right")),
                    transient: case.all_three.then(|| run("transient")),
                }
            })
            .collect();
        let after = stage.read(&stage.clock, worker);
        if before == after {
            let (hour, minute) = before.split_once(':').unwrap();
            let clock = Clock {
                hour: hour.parse().unwrap(),
                minute: minute.parse().unwrap(),
                ..batch[0].ctx.clock
            };
            return (clock, printed);
        }
    }
    panic!("the clock moved to a new minute during five batches in a row");
}

/// How `case` differs from what was printed, if it does.
fn compare(case: &Case, clock: Clock, printed: &Printed) -> Option<String> {
    let ctx = PreviewContext {
        clock,
        ..case.ctx.clone()
    };
    let want: Prompt = prompt(&case.theme, &ctx);
    let colors = &case.theme.colors;
    let read = |out: &str| parse_sgr(out, colors.foreground, colors.background);
    let mut checks: Vec<(&str, Vec<Line>, &str)> =
        vec![("primary", want.primary.clone(), &printed.primary)];
    if let Some(right) = &printed.right {
        checks.push(("right", vec![want.right.clone()], right));
    }
    if let Some(transient) = &printed.transient {
        checks.push(("transient", vec![want.transient.clone()], transient));
    }
    for (kind, want, out) in checks {
        match read(out) {
            Err(e) => return Some(format!("{}: print {kind}: {e}: {out:?}", case.what)),
            Ok(got) => {
                if let Some(diff) = difference(&want, &got) {
                    return Some(format!("{}: print {kind}: {diff}", case.what));
                }
            }
        }
    }
    None
}

#[test]
fn the_emulator_draws_what_oh_my_posh_prints() {
    let Some(exe) = pinned_omp() else {
        return;
    };
    let version = Command::new(&exe)
        .arg("version")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        PINNED,
        "{} is not the pinned Oh My Posh",
        exe.display()
    );
    let stage = Stage::new(exe);
    let cases = all_cases(&stage);
    let batches: Vec<&[Case]> = cases.chunks(16).collect();
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    std::thread::scope(|scope| {
        for w in 0..workers {
            let (stage, batches, next, failures) = (&stage, &batches, &next, &failures);
            scope.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(batch) = batches.get(i) else {
                    break;
                };
                let (clock, printed) = run_batch(stage, batch, &w.to_string());
                for (case, out) in batch.iter().zip(&printed) {
                    if let Some(failure) = compare(case, clock, out) {
                        failures.lock().unwrap().push(failure);
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().unwrap();
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from Oh My Posh {PINNED}; the first ones:\n{}",
        failures.len(),
        cases.len(),
        failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    println!("parity: {} cases match Oh My Posh {PINNED}", cases.len());
}
```

- [ ] **Step 3: Check what happens without the binary.** `resources\` is ignored by git, so it can be moved aside for a moment. In PowerShell, from the repository root (skip the two `Rename-Item` lines if `resources\` does not exist yet):
```powershell
Rename-Item resources resources.off
cargo test -p cmdstudio-core --test parity
$env:CMDSTUDIO_ALLOW_PARITY_SKIP = '1'; cargo test -p cmdstudio-core --test parity -- --nocapture; Remove-Item Env:\CMDSTUDIO_ALLOW_PARITY_SKIP
Rename-Item resources.off resources
```
Expected: the first run fails with `the parity test needs Oh My Posh 31.5.0 at …\resources\omp\oh-my-posh.exe. Run scripts\fetch-resources.ps1, or set CMDSTUDIO_ALLOW_PARITY_SKIP=1 to skip the test on a machine without it.` (`1 passed; 1 failed`: the SGR reader's own test passes). The second prints `SKIPPED parity: … is missing (CMDSTUDIO_ALLOW_PARITY_SKIP=1)` and `2 passed`.

- [ ] **Step 4: Run it for real**

Run (PowerShell, repository root):
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\fetch-resources.ps1
cargo test -p cmdstudio-core --test parity -- --nocapture
```
Expected: `resources are complete: …` (or `resources ready: …` after a download), then `parity: 713 cases match Oh My Posh 31.5.0` and `test result: ok. 2 passed` in about 20 seconds to 3 minutes (22–75 s on 2026-10-12; 172 s for the full 746 cases on a busier machine on 2026-10-13). No `cmdstudio-parity-*` folder is left in `%TEMP%`, and `Get-ChildItem $env:LOCALAPPDATA\oh-my-posh | Where-Object LastWriteTime -gt (Get-Date).AddMinutes(-5)` lists nothing.

- [ ] **Step 5: Show that it can fail.** In `crates/core/src/preview/prompt.rs` change `const BREATHING_ROOM: usize = 5;` to `4`, run `cargo test -p cmdstudio-core --test parity`, then change it back to `5`.
Expected: `2 of 713 cases differ from Oh My Posh 31.5.0; the first ones:` naming `Box [] width 9` and `Box [Path, Git, Time] width 33`, each with an `emulator:` and a `real:` line whose filler differs. After the change back, `git diff --stat` shows only the two new test files.

- [ ] **Step 6: Format and lint**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

Message:
```text
test(core): hold the preview to the real Oh My Posh 31.5.0

tests/parity.rs runs the pinned oh-my-posh.exe on the generated
config for every layout and every set of segments, with real git
repositories (clean, edited, staged, untracked, a Thai branch, none),
exit codes, durations, widths, folders and user names, reads the SGR
colors of print primary, right and transient back into spans and
compares them with preview::prompt: 713 cases. Every run gets its own
environment and cache folder in the temp folder, so nothing of the
machine leaks in or is written outside it. Without the binary the test
fails and says how to get it, unless CMDSTUDIO_ALLOW_PARITY_SKIP=1.
```
Run:
```bash
git add -- crates/core/tests/sgr/mod.rs crates/core/tests/parity.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 5: The rows of the window: the earlier command, the right prompt, wrapping

`prompt()` gives what Oh My Posh prints; the editor shows a window. This task adds `render_prompt`, which lays those prints out as cmd and Clink 1.9.34 show them (Decision 5): the command typed at the prompt before (shrunk to `> ` when the theme turns the transient prompt on, so the setting shows in the preview), the empty line cmd prints before every prompt after a command, and the prompt now with the right prompt at the right edge where Clink puts it. Rows longer than the window wrap.

**Files:**
- Modify: `crates/core/src/preview/cells.rs` (`wrap`), `crates/core/src/preview/prompt.rs` (`render_prompt`, `typed`), `crates/core/src/preview/mod.rs` (export)

**Interfaces:**
- Consumes: Task 3's `prompt`, `Prompt`, `prompt_cells`; Task 2's `Line`, `Style`, `PreviewContext`.
- Produces: `pub fn render_prompt(theme: &Theme, ctx: &PreviewContext) -> Vec<Line>` in `preview` (Phase 2b's IPC `preview_render` calls it); `pub(crate) fn wrap(line: &Line, width: usize) -> Vec<Line>` in `preview::cells`; private `fn typed(p: Prompt, command: &str, width: usize, plain: Style) -> Vec<Line>` in `preview::prompt`.

Clink's rule, read in `clink/lib/src/display_readline.cpp` at `v1.9.34`: the right prompt is drawn so that it ends in the last column (`col = screen_width − rprompt_width`), and only while `m_lastcol + 1 + rprompt_width < screen_width`, where `m_lastcol` is the column just after the input (the prompt's last line and what was typed). So at least two cells stay free in front of it.

- [ ] **Step 1: Write the failing tests.** In `crates/core/src/preview/cells.rs`, inside `mod tests`, add `use crate::color::Rgb;` under `use super::*;`, and add these tests at the end of `mod tests`:
```rust
    #[test]
    fn wrap_cuts_at_the_width_and_keeps_wide_characters_whole() {
        let s = Style::fg(Rgb { r: 1, g: 2, b: 3 });
        let mut line = Line::default();
        line.push("abcde中f", s);
        let rows: Vec<String> = wrap(&line, 3).iter().map(Line::text).collect();
        // `中` needs two cells and only one is left after `abcde`, so it starts a row
        assert_eq!(rows, ["abc", "de", "中f"]);
        // a combining mark stays on the row of its letter
        let mut thai = Line::default();
        thai.push("กี่ขค", s);
        let rows: Vec<String> = wrap(&thai, 2).iter().map(Line::text).collect();
        assert_eq!(rows, ["กี่ข", "ค"]);
        // a line that fits is one row, and an empty line is one empty row
        assert_eq!(wrap(&line, 80).len(), 1);
        assert_eq!(wrap(&Line::default(), 80), vec![Line::default()]);
        // the width is at least 1
        assert_eq!(wrap(&line, 0).len(), 7);
    }

    #[test]
    fn wrap_keeps_every_style() {
        let (a, b) = (
            Style::fg(Rgb { r: 1, g: 1, b: 1 }),
            Style::fg(Rgb { r: 2, g: 2, b: 2 }),
        );
        let mut line = Line::default();
        line.push("ab", a);
        line.push("cd", b);
        let rows = wrap(&line, 3);
        assert_eq!(rows[0].spans.len(), 2);
        assert_eq!(rows[0].spans[1].text, "c");
        assert_eq!(rows[1].spans[0].fg, Rgb { r: 2, g: 2, b: 2 });
    }
```
In `crates/core/src/preview/prompt.rs`, add these tests at the end of `mod tests`:
```rust
    // ---- the rows of the window ----

    #[test]
    fn with_the_transient_prompt_the_command_before_shrinks_to_the_arrow() {
        let rows = render_prompt(&terracotta(), &sample(60));
        assert_eq!(
            rows.len(),
            4,
            "the command, an empty line, the box's two lines"
        );
        assert_eq!(spans(&rows[0]), vec![s("> ", ACCENT), s("npm test", FG)]);
        assert!(rows[1].spans.is_empty());
        assert_eq!(prompt_cells(&rows[2].text()), 60);
        // the right prompt ends in the last cell
        assert_eq!(
            rows[3].text(),
            format!("\u{2502} > {}\u{2502}", " ".repeat(55))
        );
        assert_eq!(spans(&rows[3]).last().unwrap(), &s("\u{2502}", FRAME));
    }

    #[test]
    fn without_it_the_prompt_before_stays_whole_with_no_status_or_duration() {
        let mut theme = themed(Layout::Line, &ALL);
        theme.prompt.transient = false;
        let rows = render_prompt(&theme, &sample(120));
        assert_eq!(rows.len(), 3);
        let before = rows[0].text();
        assert!(before.ends_with("\u{276f} npm test"), "{before}");
        assert!(
            !before.contains('\u{2717}') && !before.contains("3.5s"),
            "{before}"
        );
        let now = rows[2].text();
        assert!(
            now.contains("\u{2717} 1 ") && now.contains("3.5s "),
            "{now}"
        );
    }

    #[test]
    fn the_right_prompt_shows_only_while_two_cells_stay_free_before_it() {
        // `│ > ` is 4 cells and the right prompt 1: Clink draws it while 4 + 1 + 1 < width
        let p = prompt(&terracotta(), &sample(80));
        let plain = Style::fg(terracotta().colors.foreground);
        for (width, shown) in [(7, true), (6, false), (5, false)] {
            let lines = typed(p.clone(), "", width, plain);
            let last = lines.last().unwrap().text();
            assert_eq!(last.ends_with('\u{2502}'), shown, "{width}: {last:?}");
            if shown {
                assert_eq!(prompt_cells(&last), width);
            }
        }
        // a long command leaves no room for it
        let lines = typed(p, &"x".repeat(80), 80, plain);
        assert!(!lines.last().unwrap().text().ends_with('\u{2502}'));
    }

    #[test]
    fn rows_wrap_at_the_width_of_the_window() {
        let rows = render_prompt(&terracotta(), &sample(20));
        assert!(rows.len() > 4, "{rows:?}");
        for row in &rows {
            assert!(prompt_cells(&row.text()) <= 20, "{:?}", row.text());
        }
    }
```
In `crates/core/src/preview/mod.rs`, replace `pub use prompt::{prompt, Prompt};` with `pub use prompt::{prompt, render_prompt, Prompt};`.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p cmdstudio-core --lib -- preview::`
Expected: the build fails: `wrap`, `render_prompt` and `typed` are not found.

- [ ] **Step 3: Write the code.** In `crates/core/src/preview/cells.rs`, add `use super::{Line, Style};` above the `use unicode_width::...` line, change the module doc's last line from `//! count them.` to `//! count them, and how a line longer than the window wraps.`, and add above `#[cfg(test)]`:
```rust
/// `line` cut into rows of at most `width` cells, the way the window wraps it: a character
/// that does not fit starts the next row, and a character with no cells of its own stays
/// with the one before it. A width of 0 is taken as 1.
pub(crate) fn wrap(line: &Line, width: usize) -> Vec<Line> {
    let width = width.max(1);
    let mut rows = vec![Line::default()];
    let mut col = 0;
    for span in &line.spans {
        let style: Style = span.style();
        for c in span.text.chars() {
            let cells = char_cells(c);
            if cells > 0 && col > 0 && col + cells > width {
                rows.push(Line::default());
                col = 0;
            }
            if let Some(row) = rows.last_mut() {
                row.push(c.encode_utf8(&mut [0; 4]), style);
            }
            col += cells;
        }
    }
    rows
}
```
In `crates/core/src/preview/prompt.rs`, replace the first two lines of the module doc
```rust
//! The prompt as Oh My Posh 31.5.0 prints the config `generate::omp` writes, for cmd
//! (spec §3.5). Measured on the pinned binary, and checked again by `tests/parity.rs`:
```
with
```rust
//! The prompt as Oh My Posh 31.5.0 prints the config `generate::omp` writes, for cmd
//! (spec §3.5), and the rows of the window around it. Measured on the pinned binary, and
//! checked again by `tests/parity.rs`:
```
replace `use super::cells::prompt_cells;` with `use super::cells::{prompt_cells, wrap};`, and add right after the closing `}` of `pub fn prompt`:
```rust
/// The rows of the window the editor shows under the welcome box (spec §5.6), wrapped at its
/// width:
/// 1. the command typed at the prompt before this one ([`PreviewContext::command`]). With the
///    transient prompt on, Clink has shrunk that prompt to `> ` (spec §3.5); with it off, the
///    prompt stays whole, as it was when the window had just opened: exit code 0, no duration;
/// 2. the empty line cmd prints before every prompt after a command;
/// 3. the prompt now, with the right prompt at the right edge where Clink draws it.
pub fn render_prompt(theme: &Theme, ctx: &PreviewContext) -> Vec<Line> {
    let width = usize::from(ctx.width.max(1));
    let plain = Style::fg(theme.colors.foreground);
    let earlier = if theme.prompt.transient {
        let mut line = prompt(theme, ctx).transient;
        line.push(&ctx.command, plain);
        vec![line]
    } else {
        let first = PreviewContext {
            exit_code: 0,
            duration_ms: 0,
            ..ctx.clone()
        };
        typed(prompt(theme, &first), &ctx.command, width, plain)
    };
    let now = typed(prompt(theme, ctx), "", width, plain);
    earlier
        .iter()
        .chain(std::iter::once(&Line::default()))
        .chain(&now)
        .flat_map(|line| wrap(line, width))
        .collect()
}

/// `p`'s lines with `command` typed after the last one, and the right prompt on that line so
/// that it ends in the window's last cell, when Clink 1.9.34 shows it: only while the typed
/// line, one free cell and the right prompt leave at least one more cell
/// (display_readline.cpp: `can_show_rprompt`, `print_rprompt`).
fn typed(p: Prompt, command: &str, width: usize, plain: Style) -> Vec<Line> {
    let mut lines = p.primary;
    if let Some(last) = lines.last_mut() {
        last.push(command, plain);
        let used = prompt_cells(&last.text());
        let right = prompt_cells(&p.right.text());
        if right > 0 && used + 1 + right < width {
            last.push(&" ".repeat(width - used - right), plain);
            last.append(&p.right);
        }
    }
    lines
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cmdstudio-core --lib -- preview::`
Expected: `test result: ok. 25 passed` (19 before this task, 2 `wrap` tests and 4 `render_prompt` tests now).

Run: `cargo test -p cmdstudio-core --test parity`
Expected: `2 passed` (nothing it checks has changed).

- [ ] **Step 5: Format, lint, docs**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo doc -p cmdstudio-core --no-deps`
Expected: no warnings.

- [ ] **Step 6: Commit**

Message:
```text
feat(core): lay the prompt out as rows of the cmd window

preview::render_prompt shows the command typed before (shrunk to the
transient prompt when the theme turns it on, else the whole earlier
prompt as a fresh window has it), the empty line cmd prints after a
command, and the prompt now with the box's right side where Clink
1.9.34 draws it: ending in the last column, and only while two cells
stay free before it. Rows wrap at the window's width, a wide character
whole and a Thai mark with its letter.
```
Run:
```bash
git add -- crates/core/src/preview/cells.rs crates/core/src/preview/prompt.rs crates/core/src/preview/mod.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 6: The welcome box as banner.lua prints it

> **Revised 2026-10-13 (the founder's language rule, PR #5).** `banner.lua` as shipped at `4a86f6b` is English only: `generate::lua` has no `Lang`, no `DATE_TH` and no `TIPS_TH`, and `DateStyle` has the one style `DATE_EN`. So `render_banner` takes no language — `render_banner(&Theme, &PreviewContext)` — and `DateStyle::of(lang)` and `tips(lang)` are gone; `DATE_EN`, `TIPS_EN`, `ICON` and `ICON_FALLBACK` are shared as `pub(crate)` constants instead. The Thai test `thai_has_the_buddhist_year_and_thai_tips` is replaced by `the_box_is_english_and_thai_shows_only_where_the_user_put_it`, and a guard in `tests/english_in_cmd.rs` (the file PR #5 added) holds the preview of every preset to the rule. The tests with Thai in the user name, the folder and the theme's text stay: that is the user's own data, which spec §3.6 shows as it is, and they prove the cell counting. Step 3's replacements quote the shipped text at `4a86f6b`, not Phase 1a's.

`render_banner` repeats what the generated `banner.lua` does when a cmd window starts (spec §3.6): `build()` makes the icon and title line, then, when the theme has lines, an empty line and the lines in the muted color set in by two spaces; `print_banner()` frames them two cells wider than the widest (by `console.cellcount`), puts the tips under the frame and ends with one space. The date table, the tips, the icons and the way a template is cut into text and variables come from `generate::lua` itself, so the two cannot drift; Task 7 runs the real script against it. The box is English, as everything CMD Studio writes into a cmd window is; Thai shows only where the user's data holds it.

**Files:**
- Modify: `crates/core/src/generate/lua.rs` (shares `DateStyle::format`, `DATE_EN`, `TIPS_EN`, `ICON`, `ICON_FALLBACK`, `Var`, `Piece`, `template_pieces`; two tests), `crates/core/src/preview/values.rs` (`weekday`, `lua_clean`), `crates/core/src/preview/mod.rs` (module list), `crates/core/tests/english_in_cmd.rs` (the preview guard)
- Create: `crates/core/src/preview/banner.rs`

**Interfaces:**
- Consumes: `generate::lua::banner_lua(&Theme) -> String` (English only at `4a86f6b`), `text::sanitize_text`, Task 2's `Line`, `Style`, `PreviewContext`, `Clock`, `banner_cells`, Task 3's `hh_mm`, Task 5's `render_prompt` (in the guard test only); `presets::builtin()` and `model::Layout` in the guard test.
- Produces:
  - `pub fn render_banner(theme: &Theme, ctx: &PreviewContext) -> Vec<Line>` in `preview` — empty when the theme turns the welcome box off.
  - `pub(crate)` in `generate::lua`: `struct DateStyle` with `fn format(&self, wday: u8, day: u8, month: u8, year: u16) -> String` (`wday` 1 is Sunday; a name out of range is `?`) and private `fn pattern(&self) -> &'static str` (`"%s %02d %s %d"`, made from a new field `pad_day: bool` that replaces the shipped `pattern` field, so the Lua pattern and the Rust text are made from one fact and cannot say different things); `const DATE_EN: DateStyle`; `const TIPS_EN: &str`; `const ICON` (`✻`), `const ICON_FALLBACK` (`*`); `enum Var { User, Host, Cwd, Date, Time }` (private `fn call(self) -> &'static str`); `enum Piece<'a> { Text(&'a str), Var(Var) }`; `fn template_pieces(template: &str) -> Vec<Piece<'_>>`. `template_expr` now builds on `template_pieces`; what it returns does not change, and `banner_lua`'s output does not change by one byte (the golden `terracotta.banner.en.lua` holds it).
  - `pub(crate)` in `preview::values`: `fn weekday(clock: &Clock) -> u8` (1 is Sunday) and `fn lua_clean(text: &str) -> String` (what banner.lua's `clean()` leaves of a name or folder).
  - the test `the_preview_of_every_preset_holds_no_thai_character` in `tests/english_in_cmd.rs`: every built-in preset in every layout, transient on and off, drawn in `PreviewContext::sample()` (ASCII names, paths, branch and command), holds no Thai character in any row of the box or the prompt. It runs over `presets::builtin()`, so it covers the eleven presets of Tasks 8 and 9 as they land.

- [ ] **Step 1: Write the failing tests.** Create `crates/core/src/preview/banner.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::terracotta;

    const ACCENT: &str = "#d97757";
    const MUTED: &str = "#8a8780";
    const FG: &str = "#faf9f5";

    fn texts(rows: &[Line]) -> Vec<String> {
        rows.iter().map(Line::text).collect()
    }

    /// A row as (text, foreground) pairs.
    fn spans(row: &Line) -> Vec<(String, String)> {
        row.spans
            .iter()
            .map(|s| (s.text.clone(), s.fg.to_string()))
            .collect()
    }

    fn s(text: &str, fg: &str) -> (String, String) {
        (text.to_string(), fg.to_string())
    }

    /// The Thai characters in `text` (U+0E00 to U+0E7F), in order.
    fn thai(text: &str) -> String {
        text.chars()
            .filter(|c| ('\u{0e00}'..='\u{0e7f}').contains(c))
            .collect()
    }

    #[test]
    fn terracotta_draws_the_box_of_the_sample() {
        // the widest line is `  cwd: C:\Users\you\projects\cmd-studio`, 39 cells, so the
        // box is 41 cells inside and 45 across
        let rows = render_banner(&terracotta(), &PreviewContext::sample());
        let rule = "\u{2500}".repeat(43);
        assert_eq!(
            texts(&rows),
            vec![
                format!("\u{256d}{rule}\u{256e}"),
                format!(
                    "\u{2502} \u{273b} Welcome back, you!{}\u{2502}",
                    " ".repeat(22)
                ),
                format!("\u{2502}{}\u{2502}", " ".repeat(43)),
                "\u{2502}   cwd: C:\\Users\\you\\projects\\cmd-studio   \u{2502}".to_string(),
                format!(
                    "\u{2502}   Fri 09 Oct 2026 \u{b7} 16:47 \u{b7} PC{}\u{2502}",
                    " ".repeat(12)
                ),
                format!("\u{2570}{rule}\u{256f}"),
                " Tips: help for commands \u{b7} cls to clear \u{b7} \u{2191} for history"
                    .to_string(),
                " ".to_string(),
            ]
        );
        // the frame and the icon in the accent, the title in the terminal's own color, the
        // lines and the tips muted
        assert_eq!(
            spans(&rows[1]),
            vec![
                s("\u{2502}", ACCENT),
                s(" ", FG),
                s("\u{273b}", ACCENT),
                s(&format!(" Welcome back, you!{}", " ".repeat(22)), FG),
                s("\u{2502}", ACCENT),
            ]
        );
        assert_eq!(
            spans(&rows[3])[2],
            s("  cwd: C:\\Users\\you\\projects\\cmd-studio", MUTED)
        );
        assert_eq!(spans(&rows[6]), vec![s(&texts(&rows)[6], MUTED)]);
        assert_eq!(spans(&rows[7]), vec![s(" ", FG)]);
    }

    #[test]
    fn the_box_is_english_and_thai_shows_only_where_the_user_put_it() {
        // The founder's rule (spec §3.6): what CMD Studio writes into cmd is English on every
        // Windows, so the date is `Fri 09 Oct 2026` and the tips are English, and the sample
        // (ASCII names and paths) gives a box with no Thai character in it at all.
        let rows = texts(&render_banner(&terracotta(), &PreviewContext::sample()));
        assert!(
            rows[4].contains("Fri 09 Oct 2026 \u{b7} 16:47 \u{b7} PC"),
            "{}",
            rows[4]
        );
        assert_eq!(
            rows[6],
            " Tips: help for commands \u{b7} cls to clear \u{b7} \u{2191} for history"
        );
        assert_eq!(thai(&rows.concat()), "", "{rows:?}");
        // A Thai user name is the user's own data, shown as it is: the name is the only Thai
        // in the box, and the date beside it stays English.
        let ctx = PreviewContext {
            user: "ผู้ทดสอบ".to_string(),
            ..PreviewContext::sample()
        };
        let rows = texts(&render_banner(&terracotta(), &ctx));
        assert_eq!(thai(&rows.concat()), "ผู้ทดสอบ", "{rows:?}");
        assert!(rows[1].contains("Welcome back, ผู้ทดสอบ!"), "{}", rows[1]);
        assert!(rows[4].contains("Fri 09 Oct 2026"), "{}", rows[4]);
    }

    #[test]
    fn outside_windows_terminal_the_icon_is_an_asterisk() {
        let ctx = PreviewContext {
            windows_terminal: false,
            ..PreviewContext::sample()
        };
        let rows = texts(&render_banner(&terracotta(), &ctx));
        assert!(rows[1].starts_with("\u{2502} * Welcome"), "{}", rows[1]);
    }

    #[test]
    fn a_turned_off_banner_draws_nothing_and_no_lines_no_tips_is_a_small_box() {
        let mut theme = terracotta();
        theme.banner.enabled = false;
        assert!(render_banner(&theme, &PreviewContext::sample()).is_empty());
        theme.banner.enabled = true;
        theme.banner.lines.clear();
        theme.banner.tips = false;
        let rows = texts(&render_banner(&theme, &PreviewContext::sample()));
        assert_eq!(
            rows.len(),
            4,
            "top, title, bottom and the closing space: {rows:?}"
        );
        assert_eq!(rows[3], " ");
    }

    #[test]
    fn the_frame_pads_by_cells_so_thai_marks_and_emoji_line_up() {
        // Thai in the theme's text and in the user name is user data (spec §3.6) and still has
        // to line up: the marks take no cell and a joined emoji takes two
        let mut theme = terracotta();
        theme.banner.title = "สวัสดี {user} ที่นี่ \u{1f44b}".to_string();
        theme.banner.lines = vec!["\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} 中文".to_string()];
        let ctx = PreviewContext {
            user: "ผู้ทดสอบ".to_string(),
            ..PreviewContext::sample()
        };
        let rows = texts(&render_banner(&theme, &ctx));
        let framed = &rows[..rows.len() - 2];
        let cells: Vec<usize> = framed.iter().map(|r| banner_cells(r)).collect();
        assert!(
            cells.windows(2).all(|w| w[0] == w[1]),
            "{cells:?} {framed:?}"
        );
        // the Thai marks take no cell, so the title row has more characters than cells
        assert!(rows[1].chars().count() > cells[1]);
    }

    #[test]
    fn variables_are_filled_once_and_other_braces_stay() {
        let mut theme = terracotta();
        theme.banner.title = "{{user}} {foo} {USER} {host}".to_string();
        theme.banner.lines = vec!["{time}|{cwd}".to_string()];
        // a user whose name looks like a variable stays as it is
        let ctx = PreviewContext {
            user: "{host}".to_string(),
            ..PreviewContext::sample()
        };
        let rows = texts(&render_banner(&theme, &ctx));
        assert!(rows[1].contains("{{host}} {foo} {USER} PC"), "{}", rows[1]);
        assert!(
            rows[3].contains("16:47|C:\\Users\\you\\projects\\cmd-studio"),
            "{}",
            rows[3]
        );
    }

    #[test]
    fn names_and_the_folder_lose_what_banner_lua_cleans() {
        let ctx = PreviewContext {
            user: "ad\u{202e}min\u{85}x\u{1b}[31mred".to_string(),
            cwd: "C:\\work\u{2066}\\\u{7}proj".to_string(),
            ..PreviewContext::sample()
        };
        let rows = texts(&render_banner(&terracotta(), &ctx));
        assert!(
            rows[1].contains("Welcome back, adminx[31mred!"),
            "{}",
            rows[1]
        );
        assert!(rows[3].contains("cwd: C:\\work\\proj"), "{}", rows[3]);
        // and the theme's own text loses whole escape sequences, as `sanitize_text` does
        let mut theme = terracotta();
        theme.banner.title = "hi\u{1b}[31m there".to_string();
        let rows = texts(&render_banner(&theme, &PreviewContext::sample()));
        assert!(rows[1].contains("hi there"), "{}", rows[1]);
    }
}
```
In `crates/core/src/preview/values.rs`, add at the end of `mod tests`:
```rust
    #[test]
    fn weekdays_follow_the_calendar() {
        let on = |year, month, day| {
            weekday(&Clock {
                year,
                month,
                day,
                hour: 0,
                minute: 0,
            })
        };
        assert_eq!(on(2026, 10, 9), 6, "spec §3.6: Friday 9 October 2026");
        assert_eq!(on(2026, 10, 11), 1, "a Sunday");
        assert_eq!(on(2000, 1, 1), 7, "a Saturday in January");
        assert_eq!(on(2000, 2, 29), 3, "a leap day, Tuesday");
        assert_eq!(on(1900, 3, 1), 5, "1900 was no leap year: Thursday");
        assert_eq!(on(2024, 12, 31), 3, "a Tuesday");
        // out of range never panics
        assert!((1..=7).contains(&on(2026, 0, 0)));
        assert!((1..=7).contains(&on(2026, 13, 40)));
        assert!((1..=7).contains(&on(0, 1, 1)));
    }

    #[test]
    fn lua_clean_drops_exactly_what_banner_lua_drops() {
        let dropped: Vec<char> = (0x00..=0x1F)
            .chain(0x7F..=0x9F)
            .chain(0x2028..=0x202E)
            .chain(0x2066..=0x2069)
            .filter_map(char::from_u32)
            .collect();
        for c in dropped {
            assert_eq!(lua_clean(&format!("a{c}b")), "ab", "U+{:04X}", u32::from(c));
        }
        for c in [
            ' ', '~', '\u{a0}', '\u{2027}', '\u{202f}', '\u{2065}', '\u{206a}', 'ก', '👋',
        ] {
            let s = format!("a{c}b");
            assert_eq!(lua_clean(&s), s, "U+{:04X}", u32::from(c));
        }
        // the escape character goes, the rest of its sequence stays
        assert_eq!(lua_clean("x\u{1b}[31my"), "x[31my");
    }
```
In `crates/core/src/generate/lua.rs`, add at the end of `mod tests`:
```rust
    #[test]
    fn templates_are_cut_into_text_and_variables() {
        assert_eq!(
            template_pieces("Hi {user}!"),
            [Piece::Text("Hi "), Piece::Var(Var::User), Piece::Text("!")]
        );
        assert_eq!(
            template_pieces("{{date}}{time}"),
            [
                Piece::Text("{"),
                Piece::Var(Var::Date),
                Piece::Text("}"),
                Piece::Var(Var::Time)
            ]
        );
        assert_eq!(
            template_pieces("{foo} {USER}"),
            [Piece::Text("{foo} {USER}")]
        );
        assert!(template_pieces("").is_empty());
    }

    #[test]
    fn the_date_is_written_the_same_in_rust_and_in_lua() {
        // spec §3.6: Friday 9 October 2026 is `Fri 09 Oct 2026` on every Windows, and the Lua
        // pattern says the same (the day padded to two digits, the Gregorian year)
        assert_eq!(DATE_EN.format(6, 9, 10, 2026), "Fri 09 Oct 2026");
        assert_eq!(DATE_EN.pattern(), "%s %02d %s %d");
        assert_eq!(
            DATE_EN.year_offset, 0,
            "no Buddhist era: the year is the calendar's"
        );
        // every name is English: ASCII, with no Thai character anywhere in the tables
        for name in DATE_EN.days.iter().chain(DATE_EN.months.iter()) {
            assert!(name.is_ascii(), "{name:?}");
        }
        assert!(!TIPS_EN
            .chars()
            .any(|c| ('\u{e00}'..='\u{e7f}').contains(&c)));
        // a day or month out of range is a question mark, not a panic
        assert_eq!(DATE_EN.format(0, 1, 13, 2026), "? 01 ? 2026");
    }
```
In `crates/core/src/preview/mod.rs`, replace
```rust
mod cells;
mod prompt;
mod values;

pub use cells::{banner_cells, prompt_cells};
```
with
```rust
mod banner;
mod cells;
mod prompt;
mod values;

pub use banner::render_banner;
pub use cells::{banner_cells, prompt_cells};
```
In `crates/core/tests/english_in_cmd.rs`, add `use cmdstudio_core::preview::{render_banner, render_prompt, PreviewContext};` after the line `use cmdstudio_core::presets;`, and add at the end of the file (it uses the file's own `thai` helper):
```rust
#[test]
fn the_preview_of_every_preset_holds_no_thai_character() {
    // The preview shows what cmd shows (spec §5.6), so the same rule holds for it. The sample's
    // names, paths, branch and command are ASCII, so a Thai character in a row could only be
    // CMD Studio's own: the date, the tips, the frame, the prompt.
    let ctx = PreviewContext::sample();
    for text in [&ctx.user, &ctx.host, &ctx.home, &ctx.cwd, &ctx.command] {
        assert!(text.is_ascii(), "the sample is not ASCII: {text:?}");
    }
    assert!(ctx.git.as_ref().is_some_and(|git| git.branch.is_ascii()));
    let presets = presets::builtin().unwrap();
    assert!(!presets.is_empty());
    let mut found = Vec::new();
    for (key, preset) in presets {
        for layout in [Layout::Box, Layout::Line, Layout::Powerline] {
            for transient in [true, false] {
                let mut theme = preset.clone();
                theme.prompt.layout = layout;
                theme.prompt.transient = transient;
                let rows = render_banner(&theme, &ctx)
                    .into_iter()
                    .chain(render_prompt(&theme, &ctx));
                for (i, row) in rows.enumerate() {
                    let letters = thai(&row.text());
                    if !letters.is_empty() {
                        found.push(format!(
                            "{key}, {layout:?}, transient {transient}: row {} holds \"{letters}\"",
                            i + 1
                        ));
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "Thai in what the preview shows for cmd:\n{}",
        found.join("\n")
    );
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p cmdstudio-core --lib -- preview:: generate::lua`
Expected: the build fails: `render_banner`, `weekday`, `lua_clean`, `template_pieces`, `Piece`, `Var`, `DateStyle::format` and `DateStyle::pattern` are not found, and `DATE_EN` and `TIPS_EN` are private.

- [ ] **Step 3: Share the banner's pieces in `generate/lua.rs`.** Five replacements, each quoting the text shipped at `4a86f6b` (English only; Phase 1a's `Lang`, `DATE_TH` and `TIPS_TH` are no longer there):

(a) Replace
```rust
/// The `{date}` format (spec §3.6): the names of the days and the months, how the day of the
/// month is written, and the year. The generator embeds these itself and never takes a name
/// from `os.date`: on Windows that follows the language of the machine (Thai Windows gives
/// Thai names), and what CMD Studio writes into cmd is English.
struct DateStyle {
    /// Sunday first, the way Lua's `os.date("*t").wday` counts.
    days: [&'static str; 7],
    months: [&'static str; 12],
    /// A `string.format` pattern for the day name, the day, the month name and the year.
    pattern: &'static str,
    /// Added to the year of `os.date("*t")`: 0, so the year is the Gregorian one.
    year_offset: u32,
}
```
with
```rust
/// The `{date}` format (spec §3.6): the names of the days and the months, how the day of the
/// month is written, and the year. The generator embeds these itself and never takes a name
/// from `os.date`: on Windows that follows the language of the machine (Thai Windows gives
/// Thai names), and what CMD Studio writes into cmd is English. The preview emulator writes
/// the date from the same tables ([`DateStyle::format`]), and `tests/banner_parity.rs` holds
/// the two to each other for every day of a year.
pub(crate) struct DateStyle {
    /// Sunday first, the way Lua's `os.date("*t").wday` counts.
    days: [&'static str; 7],
    months: [&'static str; 12],
    /// Whether the day of the month always has two digits (`09`). The Lua pattern and the
    /// Rust text are both made from this, so they cannot say different things.
    pad_day: bool,
    /// Added to the year of `os.date("*t")`: 0, so the year is the Gregorian one.
    year_offset: u32,
}

impl DateStyle {
    /// The `string.format` pattern banner.lua uses: the day name, the day, the month name
    /// and the year.
    fn pattern(&self) -> &'static str {
        if self.pad_day {
            "%s %02d %s %d"
        } else {
            "%s %d %s %d"
        }
    }

    /// The date as banner.lua's `today()` writes it. `wday` counts from 1 for Sunday, as
    /// `os.date("*t")` does; `month` from 1 for January. A day or month out of range (which
    /// `os.date` never gives) is written `?`.
    pub(crate) fn format(&self, wday: u8, day: u8, month: u8, year: u16) -> String {
        let name = |list: &[&'static str], number: u8| {
            usize::from(number)
                .checked_sub(1)
                .and_then(|i| list.get(i))
                .copied()
                .unwrap_or("?")
        };
        let year = u32::from(year) + self.year_offset;
        let (day_name, month_name) = (name(&self.days, wday), name(&self.months, month));
        if self.pad_day {
            format!("{day_name} {day:02} {month_name} {year}")
        } else {
            format!("{day_name} {day} {month_name} {year}")
        }
    }
}
```
(b) In `const DATE_EN`, replace the line `/// \`Fri 09 Oct 2026\`` with `/// \`Fri 09 Oct 2026\`: the one style, on every Windows.`, the line `const DATE_EN: DateStyle = DateStyle {` with `pub(crate) const DATE_EN: DateStyle = DateStyle {`, and the line `    pattern: "%s %02d %s %d",` with `    pad_day: true,`.

(c) Replace
```rust
const TIPS_EN: &str = " Tips: help for commands \u{b7} cls to clear \u{b7} \u{2191} for history";

/// The icon of the box where the terminal can draw it. Windows Terminal sets `WT_SESSION` in
/// every tab and falls back to another font for a glyph its font lacks. conhost has no
/// fallback: on Windows 10 with its default font it draws U+273B as an empty box (spec §3.6).
/// The check is the variable and nothing more, so a conhost window started from a Windows
/// Terminal tab inherits it and gets the glyph too.
const ICON: &str = "\u{273b}";
/// The icon everywhere else: an asterisk, which every font has, one cell wide like the star.
const ICON_FALLBACK: &str = "*";
```
with
```rust
/// The tips line under the box, English on every Windows (spec §3.6).
pub(crate) const TIPS_EN: &str =
    " Tips: help for commands \u{b7} cls to clear \u{b7} \u{2191} for history";

/// The icon of the box where the terminal can draw it. Windows Terminal sets `WT_SESSION` in
/// every tab and falls back to another font for a glyph its font lacks. conhost has no
/// fallback: on Windows 10 with its default font it draws U+273B as an empty box (spec §3.6).
/// The check is the variable and nothing more, so a conhost window started from a Windows
/// Terminal tab inherits it and gets the glyph too.
pub(crate) const ICON: &str = "\u{273b}";
/// The icon everywhere else: an asterisk, which every font has, one cell wide like the star.
pub(crate) const ICON_FALLBACK: &str = "*";
```
(d) In `fn vars_lua`, replace `        lua_str(style.pattern),` with `        lua_str(style.pattern()),` (`let style = &DATE_EN;` stays as shipped).

(e) Replace
```rust
/// The banner variables and the Lua call that stands for each (spec §3.3). They are lower
/// case and keep their braces; any other `{word}` is text.
const VARS: [(&str, &str); 5] = [
    ("{user}", "V.user()"),
    ("{host}", "V.host()"),
    ("{cwd}", "V.cwd()"),
    ("{date}", "V.date()"),
    ("{time}", "V.time()"),
];

/// The first variable in `text`: the text before it, its Lua call and the text after it.
fn next_variable(text: &str) -> Option<(&str, &'static str, &str)> {
    text.match_indices('{').find_map(|(at, _)| {
        let (before, from) = text.split_at_checked(at)?;
        VARS.iter()
            .find_map(|(name, call)| from.strip_prefix(*name).map(|after| (before, *call, after)))
    })
}

/// A banner template as a Lua expression. Known `{variables}` become calls;
/// everything else stays literal (spec §3.3), written by `lua_str`.
pub(crate) fn template_expr(template: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut rest = template;
    while let Some((before, call, after)) = next_variable(rest) {
        if !before.is_empty() {
            parts.push(lua_str(before));
        }
        parts.push(call.to_string());
        rest = after;
    }
    if !rest.is_empty() {
        parts.push(lua_str(rest));
    }
    if parts.is_empty() {
        lua_str("")
    } else {
        parts.join(" .. ")
    }
}
```
with
```rust
/// A banner variable (spec §3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Var {
    User,
    Host,
    Cwd,
    Date,
    Time,
}

impl Var {
    /// The Lua call that stands for the variable in banner.lua.
    fn call(self) -> &'static str {
        match self {
            Var::User => "V.user()",
            Var::Host => "V.host()",
            Var::Cwd => "V.cwd()",
            Var::Date => "V.date()",
            Var::Time => "V.time()",
        }
    }
}

/// The banner variables as a template writes them. They are lower case and keep their
/// braces; any other `{word}` is text.
const VARS: [(&str, Var); 5] = [
    ("{user}", Var::User),
    ("{host}", Var::Host),
    ("{cwd}", Var::Cwd),
    ("{date}", Var::Date),
    ("{time}", Var::Time),
];

/// A piece of a banner template: text, or a variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Piece<'a> {
    Text(&'a str),
    Var(Var),
}

/// The first variable in `text`: the text before it, the variable and the text after it.
fn next_variable(text: &str) -> Option<(&str, Var, &str)> {
    text.match_indices('{').find_map(|(at, _)| {
        let (before, from) = text.split_at_checked(at)?;
        VARS.iter()
            .find_map(|(name, var)| from.strip_prefix(*name).map(|after| (before, *var, after)))
    })
}

/// A banner template cut into text and variables, from left to right. The text a variable
/// stands for is never read again, so a user named `{host}` stays `{host}`. banner.lua and
/// the preview both cut templates here.
pub(crate) fn template_pieces(template: &str) -> Vec<Piece<'_>> {
    let mut pieces = Vec::new();
    let mut rest = template;
    while let Some((before, var, after)) = next_variable(rest) {
        if !before.is_empty() {
            pieces.push(Piece::Text(before));
        }
        pieces.push(Piece::Var(var));
        rest = after;
    }
    if !rest.is_empty() {
        pieces.push(Piece::Text(rest));
    }
    pieces
}

/// A banner template as a Lua expression. Known `{variables}` become calls;
/// everything else stays literal (spec §3.3), written by `lua_str`.
pub(crate) fn template_expr(template: &str) -> String {
    let parts: Vec<String> = template_pieces(template)
        .into_iter()
        .map(|piece| match piece {
            Piece::Text(text) => lua_str(text),
            Piece::Var(var) => var.call().to_string(),
        })
        .collect();
    if parts.is_empty() {
        lua_str("")
    } else {
        parts.join(" .. ")
    }
}
```
Nothing else in `lua.rs` changes: `banner_lua` already writes `let tips = b.tips.then_some(TIPS_EN);` and `vars_lua` already reads `&DATE_EN`.

- [ ] **Step 4: Write the rest.** In `crates/core/src/preview/values.rs`, add above `#[cfg(test)]`:
```rust
/// The day of the week, 1 for Sunday as `os.date("*t").wday` counts, by Tomohiko Sakamoto's
/// method for the Gregorian calendar.
pub(crate) fn weekday(clock: &Clock) -> u8 {
    const OFFSETS: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let month = usize::from(clock.month);
    let offset = month
        .checked_sub(1)
        .and_then(|i| OFFSETS.get(i))
        .copied()
        .unwrap_or(0);
    let year = u32::from(clock.year) - u32::from(clock.month < 3 && clock.year > 0);
    let day = (year + year / 4 - year / 100 + year / 400 + offset + u32::from(clock.day)) % 7;
    // `day` is 0 to 6, 0 for Sunday
    u8::try_from(day).map_or(1, |d| d + 1)
}

/// What banner.lua's `clean` leaves of a user name, a computer name or a folder: it drops the
/// C0 controls and DEL, the C1 controls (U+0080 to U+009F), U+2028 to U+202E and U+2066 to
/// U+2069, each on its own (the rest of an escape sequence stays as text).
pub(crate) fn lua_clean(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !matches!(
                u32::from(*c),
                0x00..=0x1F | 0x7F..=0x9F | 0x2028..=0x202E | 0x2066..=0x2069
            )
        })
        .collect()
}
```
In `crates/core/src/preview/banner.rs`, above `#[cfg(test)]`, put:
```rust
//! The welcome box as the generated banner.lua prints it (spec §3.6), line for line:
//! `build()` gives the title and the lines, `print_banner()` frames them. The parity test
//! (`tests/banner_parity.rs`) runs the real script and compares.
//!
//! The box is English, as everything CMD Studio writes into a cmd window is (spec §3.6): the
//! date and the tips come from `generate::lua`'s own tables. Thai or any other script shows
//! only where the user's own data holds it: the theme's texts and the names the variables
//! stand for.
use super::cells::banner_cells;
use super::values::{hh_mm, lua_clean, weekday};
use super::{Line, PreviewContext, Style};
use crate::generate::lua::{self, Piece, Var, DATE_EN, TIPS_EN};
use crate::model::Theme;
use crate::text::sanitize_text;

/// What banner.lua prints when a cmd window starts, one line per row: nothing when the theme
/// turns the welcome box off. When `clearLogo` is off, a real window shows cmd's own two
/// header lines and an empty line above it; they are cmd's, not the script's, and are not
/// here.
pub fn render_banner(theme: &Theme, ctx: &PreviewContext) -> Vec<Line> {
    let banner = &theme.banner;
    if !banner.enabled {
        return Vec::new();
    }
    let accent = Style::fg(theme.colors.accent);
    let muted = Style::fg(theme.colors.muted);
    // RESET: the terminal's own colors
    let plain = Style::fg(theme.colors.foreground);
    let fill = |template: &str| -> String {
        lua::template_pieces(&sanitize_text(template))
            .into_iter()
            .map(|piece| match piece {
                Piece::Text(text) => text.to_string(),
                Piece::Var(var) => value(var, ctx),
            })
            .collect()
    };

    // build(): the icon and the title; then, when there are lines, an empty line and the
    // lines in the muted color, each set in by two spaces
    let mut content = Vec::new();
    let mut title = Line::default();
    let icon = if ctx.windows_terminal {
        lua::ICON
    } else {
        lua::ICON_FALLBACK
    };
    title.push(icon, accent);
    title.push(" ", plain);
    title.push(&fill(&banner.title), plain);
    content.push(title);
    if !banner.lines.is_empty() {
        content.push(Line::default());
        for text in &banner.lines {
            let mut line = Line::default();
            line.push("  ", muted);
            line.push(&fill(text), muted);
            content.push(line);
        }
    }

    // print_banner(): a frame two cells wider than the widest line, by console.cellcount
    let width = content
        .iter()
        .map(|line| banner_cells(&line.text()))
        .max()
        .unwrap_or(0)
        + 2;
    let rule = "\u{2500}".repeat(width + 2);
    let mut rows = Vec::new();
    let mut top = Line::default();
    top.push(&format!("\u{256d}{rule}\u{256e}"), accent);
    rows.push(top);
    for line in &content {
        let mut row = Line::default();
        row.push("\u{2502}", accent);
        row.push(" ", plain);
        row.append(line);
        let pad = width.saturating_sub(banner_cells(&line.text()));
        row.push(&" ".repeat(pad), plain);
        row.push(" ", plain);
        row.push("\u{2502}", accent);
        rows.push(row);
    }
    let mut bottom = Line::default();
    bottom.push(&format!("\u{2570}{rule}\u{256f}"), accent);
    rows.push(bottom);
    if banner.tips {
        let mut tips = Line::default();
        tips.push(TIPS_EN, muted);
        rows.push(tips);
    }
    // one space, never an empty line (spec §2.2 #3)
    let mut last = Line::default();
    last.push(" ", plain);
    rows.push(last);
    rows
}

/// What banner.lua puts in place of a variable: `V.user()`, `V.host()` and `V.cwd()` clean
/// what they read, `V.date()` writes the date in English (the one style banner.lua has),
/// `V.time()` is `HH:MM`.
fn value(var: Var, ctx: &PreviewContext) -> String {
    let clock = &ctx.clock;
    match var {
        Var::User => lua_clean(&ctx.user),
        Var::Host => lua_clean(&ctx.host),
        Var::Cwd => lua_clean(&ctx.cwd),
        Var::Date => DATE_EN.format(weekday(clock), clock.day, clock.month, clock.year),
        Var::Time => hh_mm(clock),
    }
}
```

- [ ] **Step 5: Run the tests, and check that nothing generated changed**

Run: `cargo test -p cmdstudio-core --lib -- preview:: generate::lua`
Expected: `test result: ok. 43 passed` (preview: 25 + 2 in `values` + 7 in `banner`; generate::lua: 7 + 2).

Run: `cargo test -p cmdstudio-core --test golden --test banner_lua --test english_in_cmd`, then `git status --short crates/core/tests/golden/`
Expected: `golden` 17 passed, `banner_lua` 30 passed, `english_in_cmd` 2 passed, and no golden file changed: `banner.lua` is byte for byte what it was.

- [ ] **Step 6: Format, lint, docs**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo doc -p cmdstudio-core --no-deps`
Expected: no warnings.

- [ ] **Step 7: Commit**

Message:
```text
feat(core): draw the welcome box as banner.lua prints it

preview::render_banner builds the icon and title, the empty line and
the muted lines, frames them by Clink's cell count in Windows
Terminal, and adds the tips and the closing space, as the generated
banner.lua does. It takes the date table, the tips, the icons and the
cutting of a template into text and variables from generate::lua
itself, which now shares them; the generated script is unchanged.
The box is English, as everything written into cmd is; names and the
folder lose what banner.lua's clean() drops, the theme's text what
sanitize_text drops. tests/english_in_cmd.rs now also holds the
preview of every preset to the rule.
```
Run:
```bash
git add -- crates/core/src/generate/lua.rs crates/core/src/preview/values.rs crates/core/src/preview/banner.rs crates/core/src/preview/mod.rs crates/core/tests/english_in_cmd.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 7: The banner parity test against the generated banner.lua

> **Revised 2026-10-13:** `banner_lua(theme)` has no language argument (PR #5), so the cases are every preset × 9 banners × 4 samples, not × 2 languages as well, and `every_date_is_written_as_banner_lua_writes_it` checks the one English style. The Thai names and the Thai folder in the third sample stay: user data, shown as it is.

The welcome box against the real script: `banner_lua(theme)` runs in Lua 5.2 (mlua, as `tests/banner_lua.rs` runs it) with stand-ins for Clink that answer with the sample — `USERNAME`, `COMPUTERNAME`, `WT_SESSION`, the folder and the clock — and what it prints is read back with Task 4's SGR reader and compared with `render_banner`. The stand-in's `console.cellcount` is `preview::banner_cells` on the text without its colors (Decision 6), so this test holds the box's layout, colors, variables, dates, tips and icon to the script. Its stand-ins are its own and not the shared `PRELUDE` of Phase 1b's `tests/common`: they must answer from the sample, and `cellcount` must be the emulator's count.

**Files:**
- Create: `crates/core/tests/banner_parity.rs`

**Interfaces:**
- Consumes: `generate::lua::banner_lua`, `preview::{render_banner, banner_cells, PreviewContext, Clock}`, `presets::builtin()`, `tests/sgr/mod.rs` (Task 4), mlua 0.10 (`Lua`, `Function`, `Table`, `create_function`, `sequence_values`).
- Produces: the tests `the_emulator_draws_what_banner_lua_prints` (every preset × 9 banners × 4 samples: the editor's own, conhost, Thai names and folder, names full of control and direction characters; the dates take turns among five whose weekday is known) and `every_date_is_written_as_banner_lua_writes_it` (every day of 2024).

- [ ] **Step 1: Write the test.** Create `crates/core/tests/banner_parity.rs`:
```rust
//! Parity test for the welcome box (spec §3.6, §5.6): `preview::render_banner` against the
//! banner.lua that `generate::lua` writes, run in Lua 5.2 with stand-ins for Clink as
//! `tests/banner_lua.rs` runs it. The stand-ins answer with the sample: the environment
//! (`USERNAME`, `COMPUTERNAME`, `WT_SESSION`), the folder and the clock. `console.cellcount`
//! is `preview::banner_cells` on the text without its colors, so this test holds the box's
//! layout, colors, variables, dates, tips and icon to the script; how Clink counts cells in a
//! real window is a separate question (see `preview::banner_cells`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod sgr;

use cmdstudio_core::generate::lua::banner_lua;
use cmdstudio_core::model::Theme;
use cmdstudio_core::presets;
use cmdstudio_core::preview::{banner_cells, render_banner, Clock, PreviewContext};
use mlua::{Function, Lua, Table};
use sgr::{difference, parse_sgr};

const STAND_INS: &str = r#"
__out = {}
NONL = {}
clink = {
  print = function(s, flag) if flag ~= NONL then __out[#__out + 1] = s end end,
  oninject = function(f) __oninject = f end,
}
console = {
  cellcount = function(s) return __cells((s:gsub("\27%[[%d;]*m", ""))) end,
  getlinetext = function() return nil end,
}
log = { info = function() end }
os.getenv = function(k) return __env[k] end
os.setenv = function(k, v) __env[k] = v end
os.getcwd = function() return __cwd end
os.date = function(fmt)
  if fmt == "*t" then return __now end
  if fmt == "%H:%M" then return string.format("%02d:%02d", __now.hour, __now.min) end
  error("os.date(" .. tostring(fmt) .. ") follows the language of Windows")
end
"#;

/// What the generated banner.lua prints for `theme` in a window like `ctx`, whose date falls
/// on `wday` (1 is Sunday). Empty when it prints nothing.
fn printed(theme: &Theme, ctx: &PreviewContext, wday: u8) -> String {
    let lua = Lua::new();
    let cells = lua
        .create_function(|_, s: String| Ok(banner_cells(&s)))
        .unwrap();
    lua.globals().set("__cells", cells).unwrap();
    let env = lua.create_table().unwrap();
    env.set("USERNAME", ctx.user.as_str()).unwrap();
    env.set("COMPUTERNAME", ctx.host.as_str()).unwrap();
    if ctx.windows_terminal {
        env.set("WT_SESSION", "8b6e7d11-0000-4000-8000-000000000000")
            .unwrap();
    }
    lua.globals().set("__env", env).unwrap();
    lua.globals().set("__cwd", ctx.cwd.as_str()).unwrap();
    let now = lua.create_table().unwrap();
    let c = &ctx.clock;
    now.set("year", c.year).unwrap();
    now.set("month", c.month).unwrap();
    now.set("day", c.day).unwrap();
    now.set("wday", wday).unwrap();
    now.set("hour", c.hour).unwrap();
    now.set("min", c.minute).unwrap();
    lua.globals().set("__now", now).unwrap();
    lua.load(STAND_INS).exec().unwrap();
    lua.load(banner_lua(theme))
        .set_name("banner.lua")
        .exec()
        .unwrap();
    if let Ok(callback) = lua.globals().get::<Function>("__oninject") {
        callback.call::<()>(()).unwrap();
    }
    let out: Table = lua.globals().get("__out").unwrap();
    let all: Vec<String> = out.sequence_values().collect::<mlua::Result<_>>().unwrap();
    assert!(all.len() <= 1, "printed more than once: {all:?}");
    all.concat()
}

/// Dates whose day of the week is known (1 is Sunday).
const DATES: [(u16, u8, u8, u8); 5] = [
    (2026, 10, 9, 6),  // Friday, spec §3.6
    (2026, 10, 11, 1), // Sunday
    (2024, 2, 29, 5),  // Thursday, a leap day
    (1999, 12, 31, 6), // Friday
    (2026, 1, 1, 5),   // Thursday
];

/// The samples: the editor's own, conhost (no WT_SESSION), Thai names and a Thai folder (the
/// user's own data, shown as it is), names with control and direction characters that
/// banner.lua cleans.
fn contexts() -> Vec<PreviewContext> {
    let sample = PreviewContext::sample();
    vec![
        sample.clone(),
        PreviewContext {
            windows_terminal: false,
            ..sample.clone()
        },
        PreviewContext {
            user: "ผู้ทดสอบ ไทย".to_string(),
            host: "เครื่อง-01".to_string(),
            cwd: r"C:\Users\ผู้ทดสอบ ไทย\เอกสาร".to_string(),
            ..sample.clone()
        },
        PreviewContext {
            user: "ad\u{202e}min\u{85}x\u{1b}[31mred".to_string(),
            host: "PC\u{2066}-\u{1b}01\u{7}".to_string(),
            cwd: "C:\\work\u{202e}fdp.exe\\\u{85}proj".to_string(),
            ..sample
        },
    ]
}

/// The banners: each preset's own, and changes that move the box's layout.
fn banners(preset: &Theme) -> Vec<(String, Theme)> {
    let with = |what: &str, edit: &dyn Fn(&mut Theme)| {
        let mut theme = preset.clone();
        edit(&mut theme);
        (what.to_string(), theme)
    };
    vec![
        with("as the preset has it", &|_| {}),
        with("turned off", &|t| t.banner.enabled = false),
        with("no lines and no tips", &|t| {
            t.banner.lines.clear();
            t.banner.tips = false;
        }),
        with("four lines of every variable", &|t| {
            t.banner.lines = vec![
                "{user}@{host} in {cwd}".to_string(),
                "{date} {time}".to_string(),
                "{{user}} {USER} {foo} {".to_string(),
                String::new(),
            ];
        }),
        with("Thai with marks", &|t| {
            t.banner.title = "สวัสดี {user} ที่นี่ปี {date}".to_string()
        }),
        with("emoji", &|t| {
            t.banner.title =
                "\u{1f44b} {user} \u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} \u{2764}\u{fe0f}"
                    .to_string()
        }),
        with("escape sequences in the theme's text", &|t| {
            t.banner.title = "hi\u{1b}]0;evil\u{7} {user}".to_string();
            t.banner.lines = vec!["a\u{1b}[31mb\u{202e}c".to_string()];
        }),
        with("the longest title", &|t| t.banner.title = "W".repeat(80)),
        with("clearLogo off", &|t| t.banner.clear_logo = false),
    ]
}

#[test]
fn the_emulator_draws_what_banner_lua_prints() {
    let mut count = 0;
    for (key, preset) in presets::builtin().unwrap() {
        for (what, theme) in banners(&preset) {
            for (i, base) in contexts().into_iter().enumerate() {
                let (year, month, day, wday) = DATES[(count + i) % DATES.len()];
                let ctx = PreviewContext {
                    clock: Clock {
                        year,
                        month,
                        day,
                        hour: 7,
                        minute: 5,
                    },
                    ..base
                };
                let out = printed(&theme, &ctx, wday);
                let want = render_banner(&theme, &ctx);
                let why = format!("{key}, {what}, sample {i}");
                if out.is_empty() {
                    assert!(want.is_empty(), "{why}: banner.lua printed nothing");
                } else {
                    let got = parse_sgr(&out, theme.colors.foreground, theme.colors.background)
                        .unwrap_or_else(|e| panic!("{why}: {e}"));
                    if let Some(diff) = difference(&want, &got) {
                        panic!("{why}: {diff}");
                    }
                }
                count += 1;
            }
        }
    }
    // every preset, every banner, every sample
    assert_eq!(count, presets::builtin().unwrap().len() * 9 * 4);
}

#[test]
fn every_date_is_written_as_banner_lua_writes_it() {
    // every day of a leap year: the English day and month names and the Gregorian year
    let theme = {
        let mut t = presets::builtin().unwrap().remove(0).1;
        t.banner.title = "{date}".to_string();
        t.banner.lines.clear();
        t
    };
    // 1 January 2024 was a Monday (wday 2)
    let mut wday = 2;
    let days_in = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for (m, days) in days_in.iter().enumerate() {
        for day in 1..=*days {
            let month = u8::try_from(m + 1).unwrap();
            let ctx = PreviewContext {
                clock: Clock {
                    year: 2024,
                    month,
                    day,
                    hour: 0,
                    minute: 0,
                },
                ..PreviewContext::sample()
            };
            let got = parse_sgr(
                &printed(&theme, &ctx, wday),
                theme.colors.foreground,
                theme.colors.background,
            )
            .unwrap();
            assert_eq!(render_banner(&theme, &ctx), got, "2024-{month}-{day}");
            wday = wday % 7 + 1;
        }
    }
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p cmdstudio-core --test banner_parity`
Expected: `test result: ok. 3 passed` (the SGR reader's own test and the two above), in under a second.

- [ ] **Step 3: Show that it can fail.** In `crates/core/src/preview/banner.rs` change `line.push("  ", muted);` to `line.push(" ", muted);`, run `cargo test -p cmdstudio-core --test banner_parity`, and change it back.
Expected: `the_emulator_draws_what_banner_lua_prints` panics with `terracotta, as the preset has it, sample 0: line 1 of …` and an `emulator:` top line one `─` shorter than the `real:` one. After the change back, `git diff --stat` shows only the new test file.

- [ ] **Step 4: Format and lint**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

Message:
```text
test(core): hold the welcome box to the generated banner.lua

tests/banner_parity.rs runs banner_lua in Lua 5.2 with stand-ins for
Clink that answer with the sample (user, computer, folder, clock,
Windows Terminal or not) and compares what it prints, span by span,
with preview::render_banner: every preset, nine banners, four samples
including Thai and hostile names, and every day of 2024. cellcount is
preview::banner_cells, so the test holds the box's layout, colors,
dates, tips and icon to the script.
```
Run:
```bash
git add -- crates/core/tests/banner_parity.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 8: The eight upstream presets

Spec §3.2's presets from other projects (seven projects; Catppuccin gives two), with the architect's decisions D1–D7 from the brief applied to the research's values:
- **D1 Tokyo Night:** the editor pair (background `#1a1b26`, text `#a9b1d6`, 8.10:1) instead of the terminal block's (`#16161e`/`#787c99`, 4.40:1), with the `terminal.*` ANSI block, cursor `#c0caf5`, and the selection `#515c7e` at alpha 0x4d flattened over `#1a1b26`: `#2b2f41` (each channel `round(c × 77/255 + bg × 178/255)`).
- **D2 Catppuccin:** the official `catppuccin/windows-terminal` port (its bright colors repeat the normal ones).
- **D3 Dracula:** the `dracula/windows-terminal` scheme; credit "Copyright (c) 2019 thismat" plus "Copyright (c) 2023 Dracula Theme" for the palette's names.
- **D4 Nord:** selection nord3 `#4c566a` (the Alacritty port).
- **D5 Gruvbox:** dark, medium, as researched.
- **D6 One Dark:** Option A: One Dark's own hex values in our slot mapping (One Dark has no terminal colors).
- **D7 muted:** the project's comment color when it passes 3:1, else the nearest of the project's colors that does (Tokyo Night `#787c99`, Nord `#a5abb6`, One Dark `#828997`), else the comment color as a recorded exception (Solarized Light `#93a1a1`).
- accent and frame as researched: Tokyo Night `#7aa2f7`/`#414868`, Mocha mauve `#cba6f7`/overlay0 `#6c7086`, Dracula purple `#bd93f9`/`#44475a`, Nord nord8 `#88c0d0`/nord1 `#3b4252`, Gruvbox orange `#fe8019`/`#665c54`, One Dark `#528bff`/`#3e4451`, Latte mauve `#8839ef`/overlay0 `#9ca0b0`, Solarized blue `#268bd2`/base0 `#839496`.

What stays under the readability minimum (computed with `palette::contrast_issues` on these files) is 14 colors, all upstream: Gruvbox ansi 1 (2.69); Catppuccin Latte ansi 2 and 10 (2.96), 3 and 11 (2.31), 5 and 13 (2.34); Solarized Light foreground (4.13), muted (2.48), ansi 2 (2.97), 3 (2.98), 6 (2.93), 12 (2.93) and 14 (2.48). The test lists them, so a new failure or a vanished one both fail it (spec §3.2: never fix an upstream value quietly).

**Files:**
- Create: `presets/tokyo-night.cmdtheme`, `presets/catppuccin-mocha.cmdtheme`, `presets/dracula.cmdtheme`, `presets/nord.cmdtheme`, `presets/gruvbox.cmdtheme`, `presets/one-dark.cmdtheme`, `presets/catppuccin-latte.cmdtheme`, `presets/solarized-light.cmdtheme`
- Modify: `crates/core/src/presets.rs`, `docs/credits.md`, `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (appendix B)

**Interfaces:**
- Consumes: `model::{parse_theme, Layout}`, `palette::{contrast_issues, Slot}`.
- Produces: `presets::builtin()` returns 9 presets in spec order (`terracotta`, `tokyo-night`, `catppuccin-mocha`, `dracula`, `nord`, `gruvbox`, `one-dark`, `catppuccin-latte`, `solarized-light`); `OWN` stays `["terracotta"]` (Task 9 adds ours). A key is its file's name without `.cmdtheme`.

- [ ] **Step 1: Write the failing tests.** In `crates/core/src/presets.rs`, replace the whole `#[cfg(test)] mod tests { ... }` block at the end of the file (keep the `#[cfg(test)] pub(crate) fn terracotta()` helper above it) with:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Layout;
    use crate::palette::{contrast_issues, Slot};
    use std::collections::{BTreeSet, HashSet};

    /// The 23 colors of a theme in one line: background, foreground, cursor, selection,
    /// accent, muted, frame, then ansi 0 to 15.
    fn colors_of(theme: &Theme) -> String {
        let c = &theme.colors;
        let mut all = vec![
            c.background,
            c.foreground,
            c.cursor,
            c.selection,
            c.accent,
            c.muted,
            c.frame,
        ];
        all.extend(c.ansi);
        all.iter()
            .map(|rgb| rgb.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn preset(key: &str) -> Theme {
        builtin()
            .unwrap()
            .into_iter()
            .find(|(k, _)| *k == key)
            .unwrap_or_else(|| panic!("{key} is not built in"))
            .1
    }

    #[test]
    fn every_preset_parses_with_unique_ids_and_names() {
        let all = builtin().unwrap();
        assert_eq!(all[0].0, "terracotta");
        let ids: HashSet<_> = all.iter().map(|(_, t)| t.id).collect();
        let names: HashSet<_> = all.iter().map(|(_, t)| t.name.clone()).collect();
        let keys: HashSet<_> = all.iter().map(|(key, _)| *key).collect();
        assert_eq!(ids.len(), all.len());
        assert_eq!(names.len(), all.len());
        assert_eq!(keys.len(), all.len());
    }

    #[test]
    fn our_own_presets_pass_the_readability_check() {
        let all = builtin().unwrap();
        assert!(
            !OWN.is_empty(),
            "OWN names no preset, so the readability check would check nothing"
        );
        for own in OWN {
            assert!(
                all.iter().any(|(key, _)| key == own),
                "OWN names {own}, which builtin() does not return"
            );
        }
        for (key, theme) in &all {
            if OWN.contains(key) {
                let issues = contrast_issues(&theme.colors);
                assert!(issues.is_empty(), "{key}: {issues:?}");
            }
        }
    }

    #[test]
    fn upstream_presets_keep_the_upstream_colors() {
        // spec §3.2: every value as the project has it, typed again here so that a changed
        // file fails. Sources and the choices for accent, muted and frame: spec appendix B.
        for (key, colors) in [
            ("tokyo-night", "#1a1b26 #a9b1d6 #c0caf5 #2b2f41 #7aa2f7 #787c99 #414868 #363b54 #f7768e #73daca #e0af68 #7aa2f7 #bb9af7 #7dcfff #787c99 #363b54 #f7768e #73daca #e0af68 #7aa2f7 #bb9af7 #7dcfff #acb0d0"),
            ("catppuccin-mocha", "#1e1e2e #cdd6f4 #f5e0dc #585b70 #cba6f7 #9399b2 #6c7086 #45475a #f38ba8 #a6e3a1 #f9e2af #89b4fa #f5c2e7 #94e2d5 #bac2de #585b70 #f38ba8 #a6e3a1 #f9e2af #89b4fa #f5c2e7 #94e2d5 #a6adc8"),
            ("dracula", "#282a36 #f8f8f2 #f8f8f2 #44475a #bd93f9 #6272a4 #44475a #21222c #ff5555 #50fa7b #f1fa8c #bd93f9 #ff79c6 #8be9fd #f8f8f2 #6272a4 #ff6e6e #69ff94 #ffffa5 #d6acff #ff92df #a4ffff #ffffff"),
            ("nord", "#2e3440 #d8dee9 #d8dee9 #4c566a #88c0d0 #a5abb6 #3b4252 #3b4252 #bf616a #a3be8c #ebcb8b #81a1c1 #b48ead #88c0d0 #e5e9f0 #4c566a #bf616a #a3be8c #ebcb8b #81a1c1 #b48ead #8fbcbb #eceff4"),
            ("gruvbox", "#282828 #ebdbb2 #ebdbb2 #665c54 #fe8019 #928374 #665c54 #282828 #cc241d #98971a #d79921 #458588 #b16286 #689d6a #a89984 #928374 #fb4934 #b8bb26 #fabd2f #83a598 #d3869b #8ec07c #ebdbb2"),
            ("one-dark", "#282c34 #abb2bf #528bff #3e4451 #528bff #828997 #3e4451 #282c34 #e06c75 #98c379 #e5c07b #61afef #c678dd #56b6c2 #abb2bf #5c6370 #e06c75 #98c379 #e5c07b #61afef #c678dd #56b6c2 #ffffff"),
            ("catppuccin-latte", "#eff1f5 #4c4f69 #dc8a78 #acb0be #8839ef #7c7f93 #9ca0b0 #5c5f77 #d20f39 #40a02b #df8e1d #1e66f5 #ea76cb #179299 #acb0be #acb0be #d20f39 #40a02b #df8e1d #1e66f5 #ea76cb #179299 #bcc0cc"),
            ("solarized-light", "#fdf6e3 #657b83 #657b83 #eee8d5 #268bd2 #93a1a1 #839496 #073642 #dc322f #859900 #b58900 #268bd2 #d33682 #2aa198 #eee8d5 #002b36 #cb4b16 #586e75 #657b83 #839496 #6c71c4 #93a1a1 #fdf6e3"),
        ] {
            assert_eq!(colors_of(&preset(key)), colors, "{key}");
        }
    }

    #[test]
    fn every_contrast_exception_is_a_known_upstream_one() {
        // spec §3.2: an upstream color under the readability minimum stays as the project has
        // it, and is listed here. A new failure, or one that went away, fails this test.
        let known: [(&str, &[Slot]); 8] = [
            ("tokyo-night", &[]),
            ("catppuccin-mocha", &[]),
            ("dracula", &[]),
            ("nord", &[]),
            ("gruvbox", &[Slot::Ansi(1)]),
            ("one-dark", &[]),
            (
                "catppuccin-latte",
                &[
                    Slot::Ansi(2),
                    Slot::Ansi(3),
                    Slot::Ansi(5),
                    Slot::Ansi(10),
                    Slot::Ansi(11),
                    Slot::Ansi(13),
                ],
            ),
            (
                "solarized-light",
                &[
                    Slot::Foreground,
                    Slot::Muted,
                    Slot::Ansi(2),
                    Slot::Ansi(3),
                    Slot::Ansi(6),
                    Slot::Ansi(12),
                    Slot::Ansi(14),
                ],
            ),
        ];
        let all = builtin().unwrap();
        for (key, theme) in &all {
            let failing: Vec<Slot> = contrast_issues(&theme.colors)
                .iter()
                .map(|i| i.slot)
                .collect();
            match known.iter().find(|(k, _)| k == key) {
                Some((_, slots)) => assert_eq!(failing, slots.to_vec(), "{key}"),
                None => assert!(OWN.contains(key), "{key} is neither ours nor in the list"),
            }
        }
        // the list names upstream presets only, and every one of them
        assert_eq!(known.len() + OWN.len(), all.len());
        assert_eq!(known.iter().map(|(_, s)| s.len()).sum::<usize>(), 14);
    }

    #[test]
    fn presets_use_cascadia_mono_unless_their_layout_needs_the_nerd_font() {
        for (key, theme) in builtin().unwrap() {
            let want = if theme.prompt.layout == Layout::Powerline {
                "CaskaydiaCove NF"
            } else {
                "Cascadia Mono"
            };
            assert_eq!(theme.terminal.font, want, "{key}");
        }
    }

    #[test]
    fn every_preset_file_is_built_in() {
        // a file in presets/ that no key embeds would never reach the app
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets");
        let files: BTreeSet<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let keys: BTreeSet<String> = SOURCES
            .iter()
            .map(|(key, _)| format!("{key}.cmdtheme"))
            .collect();
        assert_eq!(files, keys);
    }

    #[test]
    fn terracotta_matches_the_prototype() {
        let t = terracotta();
        assert_eq!(t.colors.background.to_string(), "#1f1e1d");
        assert_eq!(t.colors.accent.to_string(), "#d97757");
        assert_eq!(t.colors.ansi[15].to_string(), "#faf9f5");
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p cmdstudio-core --lib -- presets::`
Expected: `upstream_presets_keep_the_upstream_colors` panics with `tokyo-night is not built in`, and `every_contrast_exception_is_a_known_upstream_one` fails its `assert_eq!(known.len() + OWN.len(), all.len())` (9 against 1); the other five pass.

- [ ] **Step 3: Write the preset files.** Each is UTF-8 with LF line ends and one final newline, in the layout of `presets/terracotta.cmdtheme`; the `·` in the banner line is U+00B7. Each `id` is a fresh version 4 UUID (generated 2026-10-12; never reuse one for another theme).

`presets/tokyo-night.cmdtheme`:
```json
{
  "schemaVersion": 1,
  "id": "6f1b7a9a-30fa-4e3a-945f-59ea95616054",
  "name": "Tokyo Night",
  "author": "CMD Studio",
  "tone": "dark",
  "simple": null,
  "colors": {
    "background": "#1a1b26",
    "foreground": "#a9b1d6",
    "cursor": "#c0caf5",
    "selection": "#2b2f41",
    "accent": "#7aa2f7",
    "muted": "#787c99",
    "frame": "#414868",
    "ansi": [
      "#363b54", "#f7768e", "#73daca", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#787c99",
      "#363b54", "#f7768e", "#73daca", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#acb0d0"
    ]
  },
  "terminal": {
    "font": "CaskaydiaCove NF",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 96,
    "acrylic": true,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "powerline",
    "segments": ["path", "git", "time"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

`presets/catppuccin-mocha.cmdtheme`:
```json
{
  "schemaVersion": 1,
  "id": "21afd671-b402-4f1b-bb4f-abcd4a8ec226",
  "name": "Catppuccin Mocha",
  "author": "CMD Studio",
  "tone": "dark",
  "simple": null,
  "colors": {
    "background": "#1e1e2e",
    "foreground": "#cdd6f4",
    "cursor": "#f5e0dc",
    "selection": "#585b70",
    "accent": "#cba6f7",
    "muted": "#9399b2",
    "frame": "#6c7086",
    "ansi": [
      "#45475a", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#bac2de",
      "#585b70", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#a6adc8"
    ]
  },
  "terminal": {
    "font": "Cascadia Mono",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 96,
    "acrylic": true,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "box",
    "segments": ["path", "git", "time"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

`presets/dracula.cmdtheme`:
```json
{
  "schemaVersion": 1,
  "id": "ac0249b4-3747-4f4b-991b-f0378b5ec276",
  "name": "Dracula",
  "author": "CMD Studio",
  "tone": "dark",
  "simple": null,
  "colors": {
    "background": "#282a36",
    "foreground": "#f8f8f2",
    "cursor": "#f8f8f2",
    "selection": "#44475a",
    "accent": "#bd93f9",
    "muted": "#6272a4",
    "frame": "#44475a",
    "ansi": [
      "#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2",
      "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff"
    ]
  },
  "terminal": {
    "font": "Cascadia Mono",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 96,
    "acrylic": true,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "line",
    "segments": ["path", "git", "status", "time"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

`presets/nord.cmdtheme`:
```json
{
  "schemaVersion": 1,
  "id": "1daf005c-9f7e-41ec-8b3e-e45127ce6808",
  "name": "Nord",
  "author": "CMD Studio",
  "tone": "dark",
  "simple": null,
  "colors": {
    "background": "#2e3440",
    "foreground": "#d8dee9",
    "cursor": "#d8dee9",
    "selection": "#4c566a",
    "accent": "#88c0d0",
    "muted": "#a5abb6",
    "frame": "#3b4252",
    "ansi": [
      "#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0",
      "#4c566a", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#eceff4"
    ]
  },
  "terminal": {
    "font": "Cascadia Mono",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 96,
    "acrylic": true,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "line",
    "segments": ["path", "git"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

`presets/gruvbox.cmdtheme`:
```json
{
  "schemaVersion": 1,
  "id": "a90defff-a3cb-4533-8aaf-b5d0fd27d2e0",
  "name": "Gruvbox",
  "author": "CMD Studio",
  "tone": "dark",
  "simple": null,
  "colors": {
    "background": "#282828",
    "foreground": "#ebdbb2",
    "cursor": "#ebdbb2",
    "selection": "#665c54",
    "accent": "#fe8019",
    "muted": "#928374",
    "frame": "#665c54",
    "ansi": [
      "#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984",
      "#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2"
    ]
  },
  "terminal": {
    "font": "Cascadia Mono",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 96,
    "acrylic": true,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "box",
    "segments": ["user", "path", "git", "time"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

`presets/one-dark.cmdtheme`:
```json
{
  "schemaVersion": 1,
  "id": "ad58c741-ac44-4c13-bb77-7738ce29c0ef",
  "name": "One Dark",
  "author": "CMD Studio",
  "tone": "dark",
  "simple": null,
  "colors": {
    "background": "#282c34",
    "foreground": "#abb2bf",
    "cursor": "#528bff",
    "selection": "#3e4451",
    "accent": "#528bff",
    "muted": "#828997",
    "frame": "#3e4451",
    "ansi": [
      "#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf",
      "#5c6370", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#ffffff"
    ]
  },
  "terminal": {
    "font": "CaskaydiaCove NF",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 96,
    "acrylic": true,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "powerline",
    "segments": ["path", "git", "duration", "status"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

`presets/catppuccin-latte.cmdtheme`:
```json
{
  "schemaVersion": 1,
  "id": "df271d95-c80a-4635-be46-efb1710ad12e",
  "name": "Catppuccin Latte",
  "author": "CMD Studio",
  "tone": "light",
  "simple": null,
  "colors": {
    "background": "#eff1f5",
    "foreground": "#4c4f69",
    "cursor": "#dc8a78",
    "selection": "#acb0be",
    "accent": "#8839ef",
    "muted": "#7c7f93",
    "frame": "#9ca0b0",
    "ansi": [
      "#5c5f77", "#d20f39", "#40a02b", "#df8e1d", "#1e66f5", "#ea76cb", "#179299", "#acb0be",
      "#acb0be", "#d20f39", "#40a02b", "#df8e1d", "#1e66f5", "#ea76cb", "#179299", "#bcc0cc"
    ]
  },
  "terminal": {
    "font": "Cascadia Mono",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 100,
    "acrylic": false,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "box",
    "segments": ["path", "git", "time"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

`presets/solarized-light.cmdtheme`:
```json
{
  "schemaVersion": 1,
  "id": "c3ac47c7-d75e-4d88-be32-6eb01f5bfd71",
  "name": "Solarized Light",
  "author": "CMD Studio",
  "tone": "light",
  "simple": null,
  "colors": {
    "background": "#fdf6e3",
    "foreground": "#657b83",
    "cursor": "#657b83",
    "selection": "#eee8d5",
    "accent": "#268bd2",
    "muted": "#93a1a1",
    "frame": "#839496",
    "ansi": [
      "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
      "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3"
    ]
  },
  "terminal": {
    "font": "Cascadia Mono",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 100,
    "acrylic": false,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "line",
    "segments": ["path", "git", "time"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

- [ ] **Step 4: Register them.** In `crates/core/src/presets.rs`, replace everything from the top of the file down to and including the line `pub const OWN: &[&str] = &["terracotta"];` with:
```rust
//! Built-in presets, embedded at compile time from `/presets` (spec §3.2). The upstream ones
//! keep their projects' colors (licenses confirmed in spec gate G14, credits in
//! `docs/credits.md`); the choices made where a project has no terminal color for a slot
//! are recorded in spec appendix B (2026-10-13).
use crate::model::{parse_theme, Theme, ThemeError};

/// Every preset by key, in the order of spec §3.2: the dark ones, then the light ones.
const SOURCES: &[(&str, &str)] = &[
    (
        "terracotta",
        include_str!("../../../presets/terracotta.cmdtheme"),
    ),
    (
        "tokyo-night",
        include_str!("../../../presets/tokyo-night.cmdtheme"),
    ),
    (
        "catppuccin-mocha",
        include_str!("../../../presets/catppuccin-mocha.cmdtheme"),
    ),
    ("dracula", include_str!("../../../presets/dracula.cmdtheme")),
    ("nord", include_str!("../../../presets/nord.cmdtheme")),
    ("gruvbox", include_str!("../../../presets/gruvbox.cmdtheme")),
    (
        "one-dark",
        include_str!("../../../presets/one-dark.cmdtheme"),
    ),
    (
        "catppuccin-latte",
        include_str!("../../../presets/catppuccin-latte.cmdtheme"),
    ),
    (
        "solarized-light",
        include_str!("../../../presets/solarized-light.cmdtheme"),
    ),
];

/// Keys of presets whose colors are our own; they must pass the
/// readability check (spec §3.2).
pub const OWN: &[&str] = &["terracotta"];
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p cmdstudio-core --lib -- presets::`
Expected: `test result: ok. 7 passed`.

Run: `cargo test -p cmdstudio-core --test parity --test banner_parity -- --nocapture`
Expected: `parity: 737 cases match Oh My Posh 31.5.0` (713 plus each new preset in all three layouts) and both test binaries pass: every new palette draws the same in the emulator as in Oh My Posh and in banner.lua.

- [ ] **Step 6: Credits.** In `docs/credits.md`, replace the section `## Color schemes used by presets` — its table and the paragraph under it, up to the line before `## Code signing` — with:
```markdown
## Color schemes used by presets

| Preset | Colors from | License | Copyright |
|---|---|---|---|
| Tokyo Night | https://github.com/tokyo-night/tokyo-night-vscode-theme | MIT | Copyright (c) 2018-present Enkia |
| Catppuccin Mocha, Catppuccin Latte | https://github.com/catppuccin/windows-terminal (color names: https://github.com/catppuccin/palette) | MIT | Copyright (c) 2021 Catppuccin |
| Dracula | https://github.com/dracula/windows-terminal (color names: https://github.com/dracula/dracula-theme) | MIT | Copyright (c) 2019 thismat (the Windows Terminal scheme); Copyright (c) 2023 Dracula Theme (the palette's names) |
| Nord | https://github.com/nordtheme/alacritty | MIT | Copyright (c) 2016-present Sven Greb <development@svengreb.de> (https://www.svengreb.de) |
| Gruvbox | https://github.com/gruvbox-community/gruvbox | MIT | Copyright (c) 2018 Pavel Pertsev |
| One Dark | https://github.com/atom/one-dark-syntax | MIT | Copyright (c) 2016 GitHub Inc. |
| Solarized Light | https://github.com/altercation/solarized | MIT | Copyright (c) 2011 Ethan Schoonover |
| Terracotta, Retro green, Daylight, Paper | CMD Studio | MIT | CMD Studio contributors |

Take Tokyo Night's colors from the MIT-licensed VS Code theme, not from `folke/tokyonight.nvim`, which is Apache-2.0. Take Gruvbox's colors from gruvbox-community, because the original `morhetz/gruvbox` has no license file. Nord has no Windows Terminal scheme of its own; its Alacritty port gives the terminal colors.

The values were taken on 2026-10-11 from these commits:

| Preset | Repository at commit | File |
|---|---|---|
| Tokyo Night | `tokyo-night/tokyo-night-vscode-theme@7c0f11eaef322f293621ca7befe462214b7ea468` | `themes/tokyo-night-color-theme.json` |
| Catppuccin Mocha, Latte | `catppuccin/windows-terminal@4d8bb2f00fb86927a98dd3502cdec74a76d25d7b` | `mocha.json`, `latte.json` |
| Dracula | `dracula/windows-terminal@06f63714a0477a7b5c12cf0aa94ecf92d78f32ca` | `dracula.json` |
| Nord | `nordtheme/alacritty@9949642f3903e8fcb62bfc03f09410e3d78440c2` | `src/nord.yaml` |
| Gruvbox | `gruvbox-community/gruvbox@180ad85971343df68be3422a5630fa84e45a9ab2` | `colors/gruvbox.vim` |
| One Dark | `atom/one-dark-syntax@9c96f4454362267ac45322063e193ccf9d2debb1` | `styles/colors.less`, `styles/syntax-variables.less` |
| Solarized Light | `altercation/solarized@62f656a02f93c5190a8753159e34b385588d5ff3` | `README.md` ("The Values") |

Where a project has no color for one of a theme's slots, CMD Studio took one of the project's own colors: the accent, the muted text and the frame of every upstream preset, Tokyo Night's background and text (its editor's, D1), and One Dark's terminal colors (One Dark has none; the hex values are One Dark's, which goes in which ANSI slot is CMD Studio's). The design spec's appendix B lists these choices.
```

- [ ] **Step 7: Record D1–D7 in the spec.** In `docs/superpowers/specs/2026-10-10-cmd-studio-design.md`, add this row at the end of the table under `## ภาคผนวก B: บันทึกการตัดสินใจ` (after its last row, the language rule of 2026-10-13; the row is dated 2026-10-13, the day this plan was revised and adopted, so the table stays in date order — *revised 2026-10-13*, it was dated 2026-10-12):
```markdown
| 2026-10-13 | ค่าสีของ preset จากโปรเจกต์อื่น 8 ชุด (architect เลือกตอนวางแผน phase 2a เมื่อ 2026-10-12 founder เปลี่ยนได้ ค่าตาม research วันที่ 2026-10-11 และ commit ที่ระบุใน `docs/credits.md`):<br>• Tokyo Night ใช้คู่สีของ editor (พื้น `#1a1b26` ตัวอักษร `#a9b1d6`) เพราะตัวอักษรของชุด `terminal.*` อ่านยาก (4.40:1) ส่วน 16 สีใช้ชุด `terminal.*` เคอร์เซอร์ `#c0caf5` และ selection คือ `#515c7e` ที่ความทึบ 0x4d ทับบนพื้น ได้ `#2b2f41`<br>• Catppuccin Mocha และ Latte ใช้ค่าจาก `catppuccin/windows-terminal`<br>• Dracula ใช้ scheme ของ `dracula/windows-terminal` เครดิต "Copyright (c) 2019 thismat" และชื่อสีจาก "Copyright (c) 2023 Dracula Theme"<br>• Nord ใช้ selection เป็น nord3 `#4c566a` ตาม port ของ Alacritty<br>• Gruvbox ใช้ค่าตาม research<br>• One Dark ใช้ค่าสีของต้นทาง ส่วนการจับคู่ช่อง ansi เป็นของเรา เพราะต้นทางไม่มีชุดสีสำหรับเทอร์มินัล<br>• muted ใช้สี comment ของโปรเจกต์ถ้าผ่าน 3:1 ถ้าไม่ผ่านใช้สีอื่นของโปรเจกต์ที่ใกล้ที่สุดซึ่งผ่าน (Tokyo Night `#787c99`, Nord `#a5abb6`, One Dark `#828997`) ถ้าไม่มีให้ใช้สี comment แล้วบันทึกเป็นข้อยกเว้น (Solarized Light)<br>• สีที่ยังต่ำกว่าเกณฑ์ 14 จุดเป็นค่าของต้นทางทั้งหมด (Gruvbox 1, Catppuccin Latte 6, Solarized Light 7) และอยู่ในเทสต์ `presets::tests::every_contrast_exception_is_a_known_upstream_one`<br>• ฟอนต์เป็น Cascadia Mono ยกเว้น preset ที่ใช้ layout `powerline` (Tokyo Night, One Dark) ใช้ CaskaydiaCove NF · `author` เป็น CMD Studio ทุกชุด ส่วนบรรทัด copyright อยู่ในหน้าเครดิต |
```

- [ ] **Step 8: Format and lint**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 9: Commit**

Message:
```text
feat(presets): add the eight presets from other projects

Tokyo Night, Catppuccin Mocha, Dracula, Nord, Gruvbox, One Dark,
Catppuccin Latte and Solarized Light, with the projects' values and
the architect's decisions D1-D7 (spec appendix B): Tokyo Night's
editor pair, the Catppuccin and Dracula Windows Terminal ports, Nord's
Alacritty port, One Dark's colors in our ANSI slots, and a readable
project color for muted where the comment color is not. A test pins
every value and lists the 14 upstream colors that stay under the
readability minimum. Powerline presets use CaskaydiaCove NF.
docs/credits.md names the Dracula, Catppuccin and Nord sources and
the commits the values came from.
```
Run:
```bash
git add -- presets/tokyo-night.cmdtheme presets/catppuccin-mocha.cmdtheme presets/dracula.cmdtheme presets/nord.cmdtheme presets/gruvbox.cmdtheme presets/one-dark.cmdtheme presets/catppuccin-latte.cmdtheme presets/solarized-light.cmdtheme crates/core/src/presets.rs docs/credits.md docs/superpowers/specs/2026-10-10-cmd-studio-design.md
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 9: Our own presets: Retro green, Daylight and Paper

Spec §3.2: three presets of our own, which must pass the readability check (`presets::OWN`). Each was made with `palette::generate` and then tuned by hand (the architect's design; the founder may change it). `simple` is `null` in each file because the colors were edited after generating (spec §3.3); every contrast was checked with `palette::contrast_issues`, and the powerline text colors were checked too (`p:bg` on `p:ok`, `p:dirty`, `p:accent`, `p:red` and `p:fg` on `p:frame` all ≥ 4.6:1), although none of the three uses powerline.

| Preset | `palette::generate` with | Changed by hand afterwards | Layout |
|---|---|---|---|
| Retro green (dark) | `Tone::Dark`, accent `#33ff66`, tint 0.30 | background `#0f1910` → `#0b140c`, foreground `#e7ede7` → phosphor green `#b6f2c0`, muted `#919c91` → `#6fb07b`, frame `#505850` → `#2e5a38`, selection `#2c362d` → `#1d3a24` | `line`: path, git |
| Daylight (light) | `Tone::Light`, accent `#0969da`, tint 0 | background `#f5f5f5` → white `#ffffff`, selection `#cfd8e5` → `#cfe3fa` | `box`: path, git, time |
| Paper (light) | `Tone::Light`, accent `#2f5d8a`, tint 0.25 | background `#f0f6fc` → paper `#f6f1e5`, foreground and ansi 0 `#1e2226` → ink `#2b2622`, muted `#5b646f` → `#6f665c`, frame `#b7bfc7` → `#bfb5a1`, selection `#ced9e5` → `#e3dac6`, ansi 7/8/15 warmed to `#d4ccc0`/`#5a5451`/`#fffcf5` | `line`: path, git |

Lowest contrasts against the background: Retro green foreground 14.69, muted 7.30, lowest ansi 7.15; Daylight foreground 15.97, muted 5.99, accent 5.19, lowest ansi 5.25; Paper foreground 13.28, muted 4.99, accent 6.10, lowest ansi 4.66.

**Files:**
- Create: `presets/retro-green.cmdtheme`, `presets/daylight.cmdtheme`, `presets/paper.cmdtheme`
- Modify: `crates/core/src/presets.rs`, `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (appendix B)

**Interfaces:**
- Consumes: Task 8's `presets.rs`.
- Produces: `presets::builtin()` returns the 12 presets of spec §3.2 in its order; `OWN = ["terracotta", "retro-green", "daylight", "paper"]`.

- [ ] **Step 1: Write the failing test.** In `crates/core/src/presets.rs`, in `mod tests`, replace `use crate::palette::{contrast_issues, Slot};` with `use crate::palette::{contrast_issues, Slot, Tone};`, and add after the test `every_preset_parses_with_unique_ids_and_names`:
```rust
    #[test]
    fn the_presets_are_the_twelve_of_the_spec_in_its_order() {
        // spec §3.2, typed again here
        let want = [
            ("Terracotta", Tone::Dark),
            ("Tokyo Night", Tone::Dark),
            ("Catppuccin Mocha", Tone::Dark),
            ("Dracula", Tone::Dark),
            ("Nord", Tone::Dark),
            ("Gruvbox", Tone::Dark),
            ("One Dark", Tone::Dark),
            ("Retro green", Tone::Dark),
            ("Catppuccin Latte", Tone::Light),
            ("Solarized Light", Tone::Light),
            ("Daylight", Tone::Light),
            ("Paper", Tone::Light),
        ];
        let got: Vec<(String, Tone)> = builtin()
            .unwrap()
            .into_iter()
            .map(|(_, t)| (t.name, t.tone))
            .collect();
        let want: Vec<(String, Tone)> = want.iter().map(|(n, t)| (n.to_string(), *t)).collect();
        assert_eq!(got, want);
    }
```

- [ ] **Step 2: Run it and see it fail**

Run: `cargo test -p cmdstudio-core --lib -- presets::`
Expected: `the_presets_are_the_twelve_of_the_spec_in_its_order` fails (`left` has 9 entries, `right` 12); the other seven pass.

- [ ] **Step 3: Write the preset files.**

`presets/retro-green.cmdtheme`:
```json
{
  "schemaVersion": 1,
  "id": "aaba8cee-58c1-4709-951c-3cc04f7475a3",
  "name": "Retro green",
  "author": "CMD Studio",
  "tone": "dark",
  "simple": null,
  "colors": {
    "background": "#0b140c",
    "foreground": "#b6f2c0",
    "cursor": "#33ff66",
    "selection": "#1d3a24",
    "accent": "#33ff66",
    "muted": "#6fb07b",
    "frame": "#2e5a38",
    "ansi": [
      "#2b2f2b", "#e6857e", "#72b875", "#c2a140", "#6fa7ee", "#cf88c8", "#14bbc2", "#cad0ca",
      "#606560", "#ffa098", "#8bd28d", "#dcba5c", "#91c1ff", "#e9a1e3", "#43d5dc", "#f1f7f1"
    ]
  },
  "terminal": {
    "font": "Cascadia Mono",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 96,
    "acrylic": true,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "line",
    "segments": ["path", "git"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

`presets/daylight.cmdtheme`:
```json
{
  "schemaVersion": 1,
  "id": "3611b4e8-1793-42ed-8162-7195aa0e21a9",
  "name": "Daylight",
  "author": "CMD Studio",
  "tone": "light",
  "simple": null,
  "colors": {
    "background": "#ffffff",
    "foreground": "#1f2226",
    "cursor": "#0969da",
    "selection": "#cfe3fa",
    "accent": "#0969da",
    "muted": "#5c646f",
    "frame": "#b8bec8",
    "ansi": [
      "#1f2226", "#a34945", "#357a3a", "#816500", "#346aac", "#8f4d89", "#00787d", "#caced4",
      "#52565b", "#8c3432", "#1e6626", "#695200", "#1f5596", "#793974", "#006266", "#f4f9ff"
    ]
  },
  "terminal": {
    "font": "Cascadia Mono",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 100,
    "acrylic": false,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "box",
    "segments": ["path", "git", "time"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

`presets/paper.cmdtheme`:
```json
{
  "schemaVersion": 1,
  "id": "55b736fb-2909-43c8-ace0-086f0e5ec662",
  "name": "Paper",
  "author": "CMD Studio",
  "tone": "light",
  "simple": null,
  "colors": {
    "background": "#f6f1e5",
    "foreground": "#2b2622",
    "cursor": "#2f5d8a",
    "selection": "#e3dac6",
    "accent": "#2f5d8a",
    "muted": "#6f665c",
    "frame": "#bfb5a1",
    "ansi": [
      "#2b2622", "#a34945", "#357a3a", "#816500", "#346aac", "#8f4d89", "#00787d", "#d4ccc0",
      "#5a5451", "#8c3432", "#1e6626", "#695200", "#1f5596", "#793974", "#006266", "#fffcf5"
    ]
  },
  "terminal": {
    "font": "Cascadia Mono",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 100,
    "acrylic": false,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "line",
    "segments": ["path", "git"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

- [ ] **Step 4: Register them.** In `crates/core/src/presets.rs`, replace everything from the top of the file down to and including the line `pub const OWN: &[&str] = &["terracotta"];` with:
```rust
//! Built-in presets, embedded at compile time from `/presets` (spec §3.2). The upstream ones
//! keep their projects' colors (licenses confirmed in spec gate G14, credits in
//! `docs/credits.md`); the choices made where a project has no terminal color for a slot
//! are recorded in spec appendix B (2026-10-13).
use crate::model::{parse_theme, Theme, ThemeError};

/// Every preset by key, in the order of spec §3.2: the dark ones, then the light ones.
const SOURCES: &[(&str, &str)] = &[
    (
        "terracotta",
        include_str!("../../../presets/terracotta.cmdtheme"),
    ),
    (
        "tokyo-night",
        include_str!("../../../presets/tokyo-night.cmdtheme"),
    ),
    (
        "catppuccin-mocha",
        include_str!("../../../presets/catppuccin-mocha.cmdtheme"),
    ),
    ("dracula", include_str!("../../../presets/dracula.cmdtheme")),
    ("nord", include_str!("../../../presets/nord.cmdtheme")),
    ("gruvbox", include_str!("../../../presets/gruvbox.cmdtheme")),
    (
        "one-dark",
        include_str!("../../../presets/one-dark.cmdtheme"),
    ),
    (
        "retro-green",
        include_str!("../../../presets/retro-green.cmdtheme"),
    ),
    (
        "catppuccin-latte",
        include_str!("../../../presets/catppuccin-latte.cmdtheme"),
    ),
    (
        "solarized-light",
        include_str!("../../../presets/solarized-light.cmdtheme"),
    ),
    (
        "daylight",
        include_str!("../../../presets/daylight.cmdtheme"),
    ),
    ("paper", include_str!("../../../presets/paper.cmdtheme")),
];

/// Keys of presets whose colors are our own; they must pass the
/// readability check (spec §3.2).
pub const OWN: &[&str] = &["terracotta", "retro-green", "daylight", "paper"];
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p cmdstudio-core --lib -- presets::`
Expected: `test result: ok. 8 passed` (`our_own_presets_pass_the_readability_check` now checks four presets).

Run: `cargo test -p cmdstudio-core --test parity --test banner_parity -- --nocapture`
Expected: `parity: 746 cases match Oh My Posh 31.5.0` and both binaries pass.

- [ ] **Step 6: Record our three presets in the spec.** In `docs/superpowers/specs/2026-10-10-cmd-studio-design.md`, add this row at the end of the table under `## ภาคผนวก B: บันทึกการตัดสินใจ` (after Task 8's row; dated 2026-10-13 like it — *revised 2026-10-13*):
```markdown
| 2026-10-13 | preset ของเราเอง 3 ชุด (architect ออกแบบตอนวางแผน phase 2a เมื่อ 2026-10-12 founder เปลี่ยนได้) สร้างด้วย `palette::generate` แล้วปรับด้วยมือ ทุกสีผ่านตัวตรวจความอ่านง่าย (§3.4) และ `simple` เป็น `null` เพราะแก้สีหลังสร้าง:<br>• Retro green (มืด): สร้างจากสีหลัก `#33ff66` ย้อม 0.30 แล้วปรับพื้นเป็น `#0b140c` ตัวอักษรเป็นเขียวแบบจอฟอสเฟอร์ `#b6f2c0` muted `#6fb07b` frame `#2e5a38` selection `#1d3a24` · layout `line`<br>• Daylight (สว่าง): สร้างจาก `#0969da` ย้อม 0 แล้วปรับพื้นเป็นขาว `#ffffff` และ selection `#cfe3fa` · layout `box`<br>• Paper (สว่าง): สร้างจาก `#2f5d8a` ย้อม 0.25 แล้วปรับพื้นเป็นสีกระดาษ `#f6f1e5` ตัวอักษรสีหมึก `#2b2622` muted `#6f665c` frame `#bfb5a1` selection `#e3dac6` และ ansi 0/7/8/15 ให้อุ่นตามพื้น · layout `line` |
```

- [ ] **Step 7: Format and lint**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

Message:
```text
feat(presets): add Retro green, Daylight and Paper

Our own three presets, made with palette::generate and tuned by hand:
a phosphor-green dark theme, a white light theme with a blue accent,
and a warm paper light theme with ink-blue. All three pass the
readability check (presets::OWN), and builtin() now returns the
twelve presets of spec 3.2 in its order, which a test pins.
```
Run:
```bash
git add -- presets/retro-green.cmdtheme presets/daylight.cmdtheme presets/paper.cmdtheme crates/core/src/presets.rs docs/superpowers/specs/2026-10-10-cmd-studio-design.md
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 10: Golden snapshots of the preview

> **Revised 2026-10-13:** the preview has no language (PR #5), so the four goldens are `<preset>.preview.txt` with no `.en`/`.th` tag, and all four are English — the preview shows exactly what cmd shows. Tokyo Night and Paper still cover powerline and line, dark and light; `preview_text` takes no `Lang`. The count at `4a86f6b` is 17 golden tests (PR #5 dropped `thai_banner`), so this task makes 21.

Spec §7 wants a snapshot test for each generator; the preview is now one. Four presets between them cover the box, powerline and line layouts, dark and light: Terracotta (box), Tokyo Night (powerline), Catppuccin Latte (box, light) and Paper (line, light), each in `PreviewContext::sample()`. A golden file holds the welcome box's rows and then the prompt's rows, one row to a line as the JSON the app gets; a Nerd Font glyph is written as its JSON escape (a backslash, `u` and four hex digits) so the file shows it. The approach is `tests/golden.rs`'s own: a plain run only compares, `UPDATE_GOLDEN=1` writes first, and a golden file no test checks fails `the_golden_folder_holds_no_file_that_no_test_checks`.

**Files:**
- Modify: `crates/core/tests/golden.rs`
- Create: `crates/core/tests/golden/terracotta.preview.txt`, `crates/core/tests/golden/tokyo-night.preview.txt`, `crates/core/tests/golden/catppuccin-latte.preview.txt`, `crates/core/tests/golden/paper.preview.txt` (written by the test)

**Interfaces:**
- Consumes: `preview::{render_banner, render_prompt, PreviewContext}`, the presets of Tasks 8 and 9.
- Produces: the golden tests `terracotta_preview`, `tokyo_night_preview`, `catppuccin_latte_preview`, `paper_preview`; a helper `preset(key)` in `golden.rs`.

- [ ] **Step 1: Write the failing golden tests.** In `crates/core/tests/golden.rs`:

Add `use cmdstudio_core::preview::{render_banner, render_prompt, PreviewContext};` after `use cmdstudio_core::presets;`.

Replace
```rust
/// The Terracotta preset, picked by its key.
fn terracotta() -> Theme {
    presets::builtin()
        .unwrap()
        .into_iter()
        .find(|(key, _)| *key == "terracotta")
        .unwrap()
        .1
}
```
with
```rust
/// The Terracotta preset, picked by its key.
fn terracotta() -> Theme {
    preset("terracotta")
}

/// A built-in preset, picked by its key.
fn preset(key: &str) -> Theme {
    presets::builtin()
        .unwrap()
        .into_iter()
        .find(|(k, _)| *k == key)
        .unwrap()
        .1
}

/// What the editor's preview shows for `theme` in the sample (spec §5.6): the welcome box,
/// then the rows of the prompt, one row to a line as the JSON the app gets. Nerd Font glyphs
/// are written as JSON escapes (a backslash, `u` and four hex digits), so the file shows them.
fn preview_text(theme: &Theme) -> String {
    let ctx = PreviewContext::sample();
    let mut text = String::new();
    for (part, rows) in [
        ("banner", render_banner(theme, &ctx)),
        ("prompt", render_prompt(theme, &ctx)),
    ] {
        for (i, row) in rows.iter().enumerate() {
            let json: String = serde_json::to_string(row)
                .unwrap()
                .chars()
                .map(|c| match u32::from(c) {
                    code @ 0xE000..=0xF8FF => format!("\\u{code:04x}"),
                    _ => c.to_string(),
                })
                .collect();
            text.push_str(&format!("{part} {:>2} {json}\n", i + 1));
        }
    }
    text
}
```
At the end of `golden_tests! { ... }`, after its last row (`saved_theme_file: …`), add:
```rust

    // editor shows them in the sample (English, as cmd shows them)
    terracotta_preview: "terracotta.preview.txt" = preview_text(&terracotta());
    tokyo_night_preview: "tokyo-night.preview.txt" = preview_text(&preset("tokyo-night"));
    catppuccin_latte_preview: "catppuccin-latte.preview.txt" = preview_text(&preset("catppuccin-latte"));
    paper_preview: "paper.preview.txt" = preview_text(&preset("paper"));
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p cmdstudio-core --test golden`
Expected: `FAILED. 17 passed; 4 failed`, each failure `missing golden file <name>: run with UPDATE_GOLDEN=1 once` for the four new files.

- [ ] **Step 3: Write the golden files and read them**

Run, in PowerShell: `$env:UPDATE_GOLDEN = '1'; cargo test -p cmdstudio-core --test golden; Remove-Item Env:\UPDATE_GOLDEN`, then `cargo test -p cmdstudio-core --test golden` and `git status --short crates/core/tests/golden/`.
Expected: both runs `test result: ok. 21 passed`, and `git status` shows the four new files and nothing else. Read them before committing; they are what the editor will show, and none of them holds a Thai character (`tests/english_in_cmd.rs` checks the same rows). `terracotta.preview.txt` is exactly:
```text
banner  1 [{"text":"╭───────────────────────────────────────────╮","fg":"#d97757","bg":null,"bold":false}]
banner  2 [{"text":"│","fg":"#d97757","bg":null,"bold":false},{"text":" ","fg":"#faf9f5","bg":null,"bold":false},{"text":"✻","fg":"#d97757","bg":null,"bold":false},{"text":" Welcome back, you!                      ","fg":"#faf9f5","bg":null,"bold":false},{"text":"│","fg":"#d97757","bg":null,"bold":false}]
banner  3 [{"text":"│","fg":"#d97757","bg":null,"bold":false},{"text":"                                           ","fg":"#faf9f5","bg":null,"bold":false},{"text":"│","fg":"#d97757","bg":null,"bold":false}]
banner  4 [{"text":"│","fg":"#d97757","bg":null,"bold":false},{"text":" ","fg":"#faf9f5","bg":null,"bold":false},{"text":"  cwd: C:\\Users\\you\\projects\\cmd-studio","fg":"#8a8780","bg":null,"bold":false},{"text":"   ","fg":"#faf9f5","bg":null,"bold":false},{"text":"│","fg":"#d97757","bg":null,"bold":false}]
banner  5 [{"text":"│","fg":"#d97757","bg":null,"bold":false},{"text":" ","fg":"#faf9f5","bg":null,"bold":false},{"text":"  Fri 09 Oct 2026 · 16:47 · PC","fg":"#8a8780","bg":null,"bold":false},{"text":"            ","fg":"#faf9f5","bg":null,"bold":false},{"text":"│","fg":"#d97757","bg":null,"bold":false}]
banner  6 [{"text":"╰───────────────────────────────────────────╯","fg":"#d97757","bg":null,"bold":false}]
banner  7 [{"text":" Tips: help for commands · cls to clear · ↑ for history","fg":"#8a8780","bg":null,"bold":false}]
banner  8 [{"text":" ","fg":"#faf9f5","bg":null,"bold":false}]
prompt  1 [{"text":"> ","fg":"#d97757","bg":null,"bold":false},{"text":"npm test","fg":"#faf9f5","bg":null,"bold":false}]
prompt  2 []
prompt  3 [{"text":"╭─ ","fg":"#5f5d58","bg":null,"bold":false},{"text":"~\\projects\\cmd-studio · ","fg":"#8a8780","bg":null,"bold":false},{"text":"main * ","fg":"#e3b66b","bg":null,"bold":false},{"text":"─────────────────────────────────────","fg":"#5f5d58","bg":null,"bold":false},{"text":" 16:47","fg":"#8a8780","bg":null,"bold":false},{"text":" ─╮","fg":"#5f5d58","bg":null,"bold":false}]
prompt  4 [{"text":"│ ","fg":"#5f5d58","bg":null,"bold":false},{"text":">","fg":"#d97757","bg":null,"bold":false},{"text":"                                                                            ","fg":"#faf9f5","bg":null,"bold":false},{"text":"│","fg":"#5f5d58","bg":null,"bold":false}]
```
In the other three: `tokyo-night.preview.txt` has the same English box in Tokyo Night's colors (`#a9b1d6` text, `#7aa2f7` frame and icon, `#787c99` lines and tips), and its `prompt  3` starts with the powerline arrow (written as its JSON escape ``) in `#1a1b26` on `#414868`, the reversed leading arrow, then `~\projects\cmd-studio`, the branch with Oh My Posh's own icon `` on the dirty color `#e0af68`, the time, and ` ❯`; `catppuccin-latte.preview.txt` is a box like Terracotta's in Latte's colors (`#4c4f69` text, `#9ca0b0` frame, `#8839ef` accent); `paper.preview.txt` has a one-line `prompt  3`: `~\projects\cmd-studio ` in `#346aac`, `main * ` in `#816500`, `❯` in `#2f5d8a` and the final space, after the transient `> npm test`.

- [ ] **Step 4: Format and lint**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

Message:
```text
test(core): golden snapshots of the preview

Four presets in the editor's sample, as the rows the app paints:
Terracotta's box, Tokyo Night's powerline, Catppuccin Latte's light
box and Paper's light line prompt, all English as cmd shows them. A
change to the emulator now shows up as a diff of what the editor
draws.
```
Run:
```bash
git add -- crates/core/tests/golden.rs crates/core/tests/golden/terracotta.preview.txt crates/core/tests/golden/tokyo-night.preview.txt crates/core/tests/golden/catppuccin-latte.preview.txt crates/core/tests/golden/paper.preview.txt
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 11: The spec catches up, and the phase's gate

**Files:**
- Modify: `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§0 G2, §3.5, §5.2, §5.6, §7)

**Interfaces:**
- Consumes: everything above.
- Produces: the spec as built. No code.

- [ ] **Step 1: G2.** In the table of §0, in the row that starts `| G2 |`, replace its last cell `| ✅ |` with:
```markdown
| ✅ · phase 2a: `print` ของ 31.5.0 รับ `--status <int>` และ `--execution-time <ms>` ด้วย (ดูจาก `oh-my-posh print --help` ของไฟล์ที่ pin) ทศนิยมของ `--execution-time` ถูกตัดทิ้ง และ parity test ใช้ทั้งสองตัว |
```

- [ ] **Step 2: §3.5.** Replace the three rows
```markdown
| `user`, `host` | `session` | ⬜ |
| `status` | `status` แสดงเมื่อ exit code ไม่ใช่ 0 | ⬜ |
| `duration` | `executiontime` แสดงเมื่อคำสั่งใช้เวลาเกิน 2 วินาที | ⬜ |
```
with
```markdown
| `user`, `host` | `session` | ✅ OMP 31.5.0 เรนเดอร์ตรงกับตัวจำลอง (parity test, phase 2a) · ในหน้าต่างจริง ⬜ |
| `status` | `status` แสดงเมื่อ exit code ไม่ใช่ 0 | ✅ OMP 31.5.0 (parity test, phase 2a) · ในหน้าต่างจริง ⬜ |
| `duration` | `executiontime` แสดงเมื่อคำสั่งใช้เวลาตั้งแต่ 2 วินาที (2000 ms) ขึ้นไป ในรูปแบบ `austin` เช่น `3.5s` และ `1m 1.5s` | ✅ OMP 31.5.0 (parity test, phase 2a): 2000 ms แสดงแล้ว 1999 ms ยังไม่แสดง (เดิมสเปกเขียนว่า "เกิน 2 วินาที") · ในหน้าต่างจริง ⬜ |
```
and replace the two lines
```markdown
- **`line`:** บรรทัดเดียว `<path> <git> ❯ ` ⬜
- **`powerline`:** แต่ละ segment มีพื้นสีของตัวเอง ต่อกันด้วยลูกศร ต้องใช้ Nerd Font ⬜
```
with
```markdown
- **`line`:** บรรทัดเดียว `<path> <git> ❯ ` ✅ OMP 31.5.0 (parity test, phase 2a) · ในหน้าต่างจริง ⬜
- **`powerline`:** แต่ละ segment มีพื้นสีของตัวเอง ต่อกันด้วยลูกศร ต้องใช้ Nerd Font ✅ OMP 31.5.0 (parity test, phase 2a): OMP วาดลูกศรตัวแรกหน้า segment แรกด้วยแบบกลับสี (reverse video) คือสีพื้นของเทอร์มินัลบนสีของ segment · ในหน้าต่างจริง ⬜
```

- [ ] **Step 3: §5.2.** In the module table, after the row that starts ``| `generate` |``, add:
```markdown
| `preview` | ตัวจำลองพรีวิว (§5.6): วาด prompt และกล่องต้อนรับจากธีมกับค่าตัวอย่าง เป็นบรรทัดของ span `{text, fg, bg, bold}` โดยไม่เรียก OMP หรือ Clink | ไม่แตะ |
```

- [ ] **Step 4: §5.6.** Replace the four bullets under `### 5.6 พรีวิว`
```markdown
- ใน UI มีตัวจำลองที่วาด prompt และกล่องต้อนรับจาก theme model โดยไม่เรียก OMP ทุกครั้ง เหตุผลคือให้ลื่นตอนลากเลือกสี และให้เห็นส่วน git ได้แม้เครื่องไม่มี git
- ข้อนี้**เปลี่ยนจาก mock แรก** ที่เขียนว่า "เรนเดอร์ด้วย oh-my-posh จริง"
- มี parity test (§7) เทียบผลของตัวจำลองกับ `oh-my-posh print` ตัวจริงทีละตัวอักษรและทีละสี ครบทุก layout คูณทุก segment
- palette และ contrast คำนวณใน Rust ผ่าน IPC ห้ามเขียนสูตรซ้ำใน TypeScript
```
with
```markdown
- ตัวจำลองพรีวิวอยู่ใน `cmdstudio-core` (โมดูล `preview`, phase 2a): `render_prompt` และ `render_banner` วาด prompt และกล่องต้อนรับจาก theme model กับค่าตัวอย่างที่ UI เลือก (ผู้ใช้ เครื่อง โฟลเดอร์ วันเวลา สถานะ git exit code และเวลาที่คำสั่งก่อนหน้าใช้ ความกว้างหน้าต่าง) เป็นบรรทัดของ span `{text, fg, bg, bold}` โดยไม่เรียก OMP ทุกครั้ง ส่วน UI แค่ระบายสีตาม span เหตุผลคือให้ลื่นตอนลากเลือกสี และให้เห็นส่วน git ได้แม้เครื่องไม่มี git
- ข้อนี้**เปลี่ยนจาก mock แรก** ที่เขียนว่า "เรนเดอร์ด้วย oh-my-posh จริง"
- ตัวจำลองกับตัวสร้าง config ของ OMP ใช้ตารางเดียวกัน (สีและช่องว่างของแต่ละ segment ในแต่ละ layout) ใน `generate::omp` จึงแยกกันเปลี่ยนไม่ได้
- มี parity test (§7) เทียบผลของตัวจำลองกับ `oh-my-posh print primary|right|transient` ตัวจริงที่ pin ไว้ ทีละตัวอักษรและทีละสี ครบทุก layout คูณทุกชุดของ segment สถานะ git มาจาก repo ทิ้งได้ใน temp ส่วน exit code และเวลาที่คำสั่งใช้ส่งผ่าน `--status` และ `--execution-time` ✅ 746 กรณีตรงกับ OMP 31.5.0 (phase 2a) และเทียบกล่องต้อนรับกับ `banner.lua` ที่สร้างจริงซึ่งรันใน Lua 5.2 ✅
- ความกว้างของกรอบกล่องต้อนรับนับแบบที่ Clink 1.9.34 นับใน Windows Terminal: สระและวรรณยุกต์ที่ซ้อนบนล่างไม่กินช่อง และ emoji ที่ต่อกันนับเป็น 2 ช่อง (จากซอร์สของ Clink) ⬜ ยังไม่ได้ลองในหน้าต่างจริง ต้องลองกล่องต้อนรับบน VM-C ที่ชื่อผู้ใช้และโฟลเดอร์เป็นภาษาไทย (ข้อมูลของผู้ใช้ ส่วนข้อความของกล่องเองเป็นภาษาอังกฤษตาม §3.6) · ใน conhost Clink นับเครื่องหมายเหล่านั้นตัวละ 1 ช่อง (เห็นจริงบนเครื่อง dev 2026-10-11) พรีวิวจึงตรงกับ WT ไม่ใช่ conhost
- พรีวิวแสดงสิ่งที่ cmd แสดง จึงเป็นภาษาอังกฤษเหมือนกัน (§1, §3.6) มีเทสต์ `tests/english_in_cmd.rs` ตรวจว่ากล่องต้อนรับและ prompt ของทุก preset ในพรีวิวไม่มีอักษรไทยเมื่อชื่อและ path ตัวอย่างเป็น ASCII ✅ (phase 2a)
- palette, contrast และตรรกะของ prompt คำนวณใน Rust ผ่าน IPC ห้ามเขียนสูตรซ้ำใน TypeScript
```

- [ ] **Step 4b: §5.1** (*added 2026-10-13*: the tree still put the emulator in the UI). In the repo tree under `### 5.1 โครงสร้าง repo`, replace the line
```text
│  └─ ui/              React + TypeScript (Vite): หน้าจอ, ตัวจำลองพรีวิว, i18n
```
with
```text
│  └─ ui/              React + TypeScript (Vite): หน้าจอ (ระบายสี span ของพรีวิวที่ core วาด), i18n
```

- [ ] **Step 5: §7.** Replace the line
```markdown
- **parity test:** เทียบตัวจำลองพรีวิวกับ OMP ตัวจริงที่ pin ไว้ ครบทุก layout คูณทุก segment
```
with
```markdown
- **parity test:** เทียบตัวจำลองพรีวิวกับ OMP ตัวจริงที่ pin ไว้ ครบทุก layout คูณทุก segment (`crates/core/tests/parity.rs`) และกล่องต้อนรับกับ `banner.lua` (`crates/core/tests/banner_parity.rs`) · ถ้าไม่มีไฟล์ OMP เทสต์จะล้มพร้อมบอกวิธีเอามา (`scripts/fetch-resources.ps1`) เว้นแต่ตั้ง `CMDSTUDIO_ALLOW_PARITY_SKIP=1` ซึ่ง CI ไม่ตั้งเด็ดขาด
```

- [ ] **Step 6: The phase's gate.** Run, from the repository root:
```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings
cargo doc -p cmdstudio-core --no-deps
cargo test --workspace
cargo test -p cmdstudio-core --test parity -- --nocapture
```
Expected: no format diff, no clippy warning, no rustdoc warning beyond the two `4a86f6b` already prints (Global Constraints), every test passes — the core unit tests are 670 (`4a86f6b`'s 624 + 46), `golden` 21, `banner_lua` 30, `banner_parity` 3, `english_in_cmd` 2, `parity` 2, and the rest as at `4a86f6b` (`hostile_themes` 32, `schema_file` 11, `error_codes` 2, `palette_props` 1, `omp_render` 1 printing `SKIPPED` without its gate, 2 doc tests; `cmdstudio-cli` unchanged) — and `parity: 746 cases match Oh My Posh 31.5.0`. (*Revised 2026-10-13*: the counts were predicted on top of Phase 1b's plan; these are measured on `4a86f6b` with the revised code.)

Then:
```bash
git grep -n -E "std::fs|winreg|windows_sys|std::process|Command::new" -- crates/core/src ':!crates/core/src/adapters/windows'
git log --format=%B origin/main..HEAD | grep -i -E "co-authored-by|generated with|claude"
git ls-files resources
git status --short
```
Expected: the first three print nothing (the preview reads no file and runs no program; no commit carries an attribution line; the binaries stay out of git), and `git status` shows only this task's spec edit.

- [ ] **Step 7: Commit**

Message:
```text
docs: describe the preview emulator and its parity tests as built

Spec 5.6 now says where the emulator lives (cmdstudio-core, module
preview; the UI paints its spans), that it shares one table with the
Oh My Posh generator, what the parity tests compare, how the welcome
box counts cells, and that the preview is English like cmd. 5.1's
tree no longer puts the emulator in the UI. 5.2 lists the preview
module, 3.5 marks the
session, status and duration segments and the line and powerline
layouts as rendered by Oh My Posh 31.5.0, and says duration shows from
2 seconds, not after. G2 records the --status and --execution-time
flags of 31.5.0, and 7 the parity tests' skip switch, which CI never
sets.
```
Run:
```bash
git add -- docs/superpowers/specs/2026-10-10-cmd-studio-design.md
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

- [ ] **Step 8: Hand-off (controller).** The pull request (the `auroraz-pr-workflow` skill; implementers never push) must show CI green, with the system-test step's log carrying `parity: 746 cases match Oh My Posh 31.5.0` and no `SKIPPED` line. Put on Phase 2b's VM checklist: the welcome box on VM-C in Windows Terminal with its Thai user `ผู้ทดสอบ ไทย`, a Thai folder and a Thai title the tester types (all user data; the box's own date and tips must be English), to confirm Decision 6 (its frame's right edge must line up), and the same in conhost on VM-B, where Clink counts the marks as cells. (*Revised 2026-10-13*: it said "a Thai welcome box"; the box is English, the Thai in it is the user's.)

---

## Not in this plan

- The Tauri app, its IPC command `preview_render`, ts-rs bindings and the screens that paint the spans (Phase 2b). `Span`, `Line`, `Prompt` and `PreviewContext` already serialize in camelCase for it.
- cmd's own header lines (`Microsoft Windows [Version …]`, `(c) Microsoft Corporation…` and an empty line), which a real window shows above the welcome box when `clearLogo` is off: they are cmd's, not banner.lua's. `render_banner`'s doc says so; Phase 2b decides whether the editor draws them.
- How Clink 1.9.34 counts cells in a real Windows Terminal tab (Decision 6), and the prompt and box in real windows: Phase 2b's VM checks (Task 11 Step 8).
- The cursor (`terminal.cursorShape`, `colors.cursor`) and the selection color: the app draws them where it puts the caret; they are not printed text.

## Self-Review (2026-10-12, revised 2026-10-13)

**1. Spec coverage**

| Spec, brief or rule | Where |
|---|---|
| §5.6: prompt and welcome box drawn from the theme without running Oh My Posh; git visible without git | Tasks 2, 3, 5, 6 (`PreviewContext.git` is a sample, no repository needed) |
| §5.6 + §7: parity with the pinned Oh My Posh, character by character and color by color, every layout × every segment | Task 4: all 3 × 128 subsets with `print primary`, `right` and `transient`, plus orders, states, widths, folders, durations, exit codes, users and presets; 746 cases |
| §5.6: palette, contrast and prompt logic never in TypeScript | Decision 1 (spans only); Task 11's §5.6 text |
| Brief: `render_prompt(theme, ctx)`, `render_banner(theme, ctx)` (the brief's `Lang` went with PR #5), spans `{text, fg, bg, bold}`, the context's fields | Tasks 2, 3, 5, 6 |
| Founder's language rule (spec §1, §3.6, appendix B 2026-10-13): nothing in cmd is Thai, the preview included; Thai only as user data | Decision 9; Task 6 (`the_box_is_english_and_thai_shows_only_where_the_user_put_it`, the guard in `tests/english_in_cmd.rs`), Task 7 (English dates for every day of 2024), Task 10 (English goldens), Task 11 (spec text without Thai cmd text) |
| Brief: the box filler and right block, transient prompt, powerline arrows with the leading reversed U+E0B0, git `✓`/`*`, status and duration visibility, `user@host`, time | Task 3 (each a test of its own), Task 4 (the binary), Task 1 (the shared table) |
| Brief: the banner exactly as banner.lua prints it: frame widths by cells, the English date (`Fri 09 Oct 2026`, Gregorian year), tips, icon choice | Task 6, Task 7 (the real script; every day of 2024) |
| Brief: the parity test skips only when the binary is absent and an env var allows it; CI always runs it | Task 4 Step 3; Global Constraints; Task 11 Step 8 |
| §3.5 rule: time last on the left in line and powerline, on the right in the box; one session for user and host | Task 1 (`left_parts`, its test), Task 3 tests, Task 4 order cases |
| §3.6 / §2.2 #3: the banner ends with one space; the icon falls back to `*` outside Windows Terminal | Task 6 tests, Task 7 |
| §3.7 #3, #4: text cleaned before it reaches the screen | Task 6 (`sanitize_text` for the theme's text, `lua_clean` for names), Task 7 (hostile samples through the real script) |
| §3.2: twelve presets, dark and light, upstream values kept, exceptions listed, ours readable, no brand names | Tasks 8 and 9 (tests pin names, tones, order, every upstream value, the 14 exceptions, `OWN`) |
| §3.3: canonical v4 ids; fonts; brief: Cascadia Mono unless powerline | Tasks 8, 9 (`parse_theme` refuses another id form; the fonts test) |
| §3.4: readability thresholds | Task 8 (exceptions test), Task 9 (`OWN`) |
| §6.6 and the research: credits for Dracula, Catppuccin and Nord fixed | Task 8 Step 6 |
| Research: D1–D7 recorded in appendix B | Task 8 Step 7; our presets in Task 9 Step 6 |
| §7: a snapshot test for each generator | Task 10 |

Spec conflicts are resolved in the plan and written back into the spec: duration "from 2 s" (Task 11 Step 2), the emulator's home (Task 11 Steps 3–4), the upstream-value rule with D1/D7 (Task 8 Step 7).

**2. Placeholder scan.** No "TBD", "TODO", "implement later", "similar to Task N" or unspecified error handling. Every code block is complete; the only text produced at run time is the four golden files, written by the test itself (Task 10) and checked by eye against the given content of `terracotta.preview.txt` (which the 2026-10-13 scratch run reproduced byte for byte).

**3. Type consistency.** On 2026-10-12 Tasks 1–10 were applied in order to a clean copy of `05d4968` and each was built, linted and tested alone (unit test counts per task: +4, +6, +14, +0, +6, +11, +0, +4, +1; the revised Task 6 still adds 11). On 2026-10-13 the revised tasks' final state was applied together to a clean copy of `4a86f6b` and built, linted and tested as one (670 unit tests, every integration test, the parity run), so the names agree across tasks and with the shipped code: `Swatch`, `Part`, `Look`, `Drawn`, `drawn`, `left_parts`, `has_time`, `prompt_char` and the frame constants (Task 1) in Task 3; `Look::changed` (Task 3) in `Block::part`; `Style`, `Span`, `Line`, `PreviewContext`, `Clock`, `GitState`, `prompt_cells`, `banner_cells` (Task 2) in Tasks 3–7 and 10; `prompt`, `Prompt` (Task 3) in Tasks 4 and 5; `render_prompt` (Task 5) and `render_banner` (Task 6) in Task 10; `parse_sgr`, `difference` (Task 4) in Task 7; `DateStyle::format`, `DATE_EN`, `TIPS_EN`, `ICON`, `ICON_FALLBACK`, `Var`, `Piece`, `template_pieces` (Task 6) in `preview::banner`; `render_banner` and `render_prompt` in `tests/english_in_cmd.rs` (Task 6) and `tests/golden.rs` (Task 10); preset keys (Tasks 8, 9) in Tasks 4, 7 and 10.

**4. Review Focus.** Each of the six has named tests in its owning task, listed at the top; 1, 2 and 3 are also checked against the real binary in Task 4, and 6 against every preset as Tasks 8 and 9 add them.

**5. Language (2026-10-13).** The plan file itself was searched for `Lang`, `DATE_TH`, `TIPS_TH`, `Buddhist`, `.th`, `both languages` and the Thai date `ศ. 9 ต.ค. 2569`: the only Thai left in code, test expectations and spec edits is user data (user names, folders, branch names, banner text a test types) or the spec's own Thai prose. No golden, doc example or test expects a Thai word of CMD Studio's own.

## Execution handoff

Recommended: **subagent-driven**, one fresh implementer and one fresh reviewer per task, then a whole-branch review. The eleven tasks hang on each other's exact names (the shared table, the span types, the SGR reader), the emulator is subtle enough that a reviewer reading Task 3 against the measured facts at the top of this plan is worth the cost, and Task 4's real run needs the pinned binary from Phase 1b's fetch script on the machine that runs it. A wrong preview would show users a prompt they will not get, which the parity test now makes a CI failure.
