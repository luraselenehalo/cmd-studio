# Phase 1b — Windows Adapters, Restore Points, Doctor and CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make CMD Studio change a real Windows machine and put it back: the Phase 1a follow-ups, Windows adapters, `detect`, restore points on disk, the real txn executor, verifier and journal store, doctor D1–D11, the bundled-runtime pipeline and the `cmdstudio` CLI, so that `cmdstudio apply` → `cmdstudio undo` leaves the registry, Windows Terminal's `settings.json` and the font list byte-identical on CI and on the Windows 10 and 11 VMs (spec §8, Phase 1 done-criterion).

**Architecture:** `cmdstudio-core` gains `adapters`: six traits (`Fs`, `AutoRun`, `Fonts`, `WtSettings`, `Process`, `Host`), their Windows implementations behind `#[cfg(windows)]`, and in-memory fakes behind `cfg(test)` or the `fake` feature. Everything else stays pure over those traits: `wt_edit` (settings.json through the jsonc-parser CST), `state` (state.json and the baseline), `snapshot` (restore points, the on-disk `txn::JournalStore`, and the record of a rollback that could not finish), `detect`, `exec` and `verify` (the real `txn::Executor` and `txn::Verifier`), `actions` (apply, undo, crash recovery), `cleanup` (revert everything, `--cleanup`) and `doctor`. `cleanup` and `doctor` reuse Phase 1a's AutoRun editor (`plan::autorun`, which is `pub(crate)`), so they live in the core; the CLI calls only public core functions. A new `cmdstudio-cli` crate is a thin library (`run`, `run_with`) plus the `cmdstudio` binary; Phase 2's app links that library and calls `run` when started with arguments (spec §5.7: one exe).

**Tech Stack:**
- Rust `1.99.0` (pinned by `rust-toolchain.toml`), edition 2021. Already in the workspace: serde 1, serde_json 1, thiserror 2, uuid 1 (v4), schemars 1 (1.2.2 in `Cargo.lock`); dev: proptest 1, mlua 0.10 (`lua52`, `vendored`).
- `jsonc-parser = { version = "=0.34.0", features = ["cst", "serde_json"] }` — spec G9; the `serde_json` feature is what provides `to_serde_value()` on CST nodes.
- `sha2 = "0.11"` (0.11.0 is the newest on crates.io, checked 2026-10-11).
- Windows only: `winreg = "=0.56.0"` (newest, checked 2026-10-11; it depends on `windows-sys >=0.59, <=0.61`) and `windows-sys = "=0.61.2"` (newest, checked 2026-10-11) with the features listed in Task 5.
- Dev: `tempfile = "3"` (3.27.0).
- `scripts/fetch-resources.ps1` runs in Windows PowerShell 5.1 and PowerShell 7. CI: GitHub Actions `windows-latest`.

**Spec:** `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (Thai; binding). Controller decisions: `docs/superpowers/plans/2026-10-11-phase1b-design-brief.md` (binding where the spec is silent). Phase 1a decisions: `docs/superpowers/plans/2026-10-10-phase1a-core-rulings.md` (rulings P1–P71 and F1–F5, and the deferred minors).

**Prerequisite:** Phase 1a as shipped: branch `phase1a-core` at `05d4968` (PR #3), 300 tests passing. Phase 1b works on branch `phase1b-system`, which starts at that commit.

**Checked before this plan was written (2026-10-12):** every code block in Tasks 0–17 was compiled and run on the dev machine in a scratch copy of `05d4968` with its own `CARGO_TARGET_DIR` (Rust 1.99.0, the crate versions above). After Task 17 the workspace runs 323 core unit tests (217 of Phase 1a after Task 0, 106 of Phase 1b), the integration tests `banner_lua` 30, `golden` 18, `hostile_themes` 32, `palette_props` 1, `schema_file` 11 and `omp_render` 1, 2 doc tests, and in `cmdstudio-cli` 10 unit tests and `system` 1 — all passing. `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings` (without the `fake` feature), `cargo fmt --all -- --check` and `cargo doc -p cmdstudio-core --no-deps` were clean. Task 17's render test ran for real against the pinned `oh-my-posh.exe` and Phase 1a's real generator: all three layouts, transient on and off, rendered with no config error. `scripts/fetch-resources.ps1` (checked for the draft on 2026-10-11, unchanged since) built `resources\` from local copies of the pinned downloads, ran a second time as a no-op, and rebuilt after one byte of `clink_dll_x64.dll` was changed; the Rust side then read that `manifest.json` and checked every hash. Task 16's read-only smoke test (`--help`, `doctor`, an unknown command) ran against this machine, and `guard.test.ps1` passed with Task 19's guest script. The tests that change the machine (Task 17's apply/undo and the `system_*` tests in Tasks 3–6) were compiled but **not** run; CI (Task 18) and the VMs (Task 19) are their first real runs. Code in new files is shown formatted at a width of 120 for reading; `cargo fmt --all` reflows it to the repository style, which is expected. Code that edits shipped files (Task 0, and the few edits to Phase 1a files later) is shown in the repository style and quotes the shipped text exactly.

## Phase 1a interfaces this plan relies on

Read from the shipped code at `05d4968` (2026-10-12). Task 0 changes only what its own section says.

| Phase 1a item | As shipped | Used in |
|---|---|---|
| `plan::Paths { data }` | `Clone + Debug + PartialEq + Eq`; `generated()`, `clink_profile()`, `omp_dir()` (`<data>\runtime\omp-31.5.0`), `clink_dir()` (`<data>\runtime\clink-1.9.34`), `omp_exe()`, `clink_exe()` (`clink_x64.exe`), `omp_config()` (`generated\theme.omp.json`). Task 8 adds more helpers in `state.rs`. | Tasks 8–17 |
| `plan::Machine` | `wt_settings: Vec<PathBuf>`, `autorun_hkcu: Option<String>`, `autorun_hklm: Option<String>` (raw text, never expanded; `None` when absent or not text), `fonts: Vec<String>` (GDI families), `data_short: Option<PathBuf>`, `program_data_dir: Option<PathBuf>`; `Clone + Debug + Default + PartialEq + Eq` | Tasks 10, 12 |
| `plan::ClinkLaunch` | `Direct(PathBuf)`, `ShortPath(PathBuf)`, `ProgramData { dir }`; `exe()`, `runtime_dir(&Paths)` | Tasks 8, 12, 13, 15 |
| `plan::Request` | `{ target: Target, set_wt_default_profile: bool, lang: generate::lua::Lang, replace_foreign_clink: bool }` | Tasks 13, 16 |
| `plan::Component` | `Omp`, `Clink`; `Copy`, serde lowercase | Tasks 2, 11 |
| `plan::Op` | `InstallRuntime { component, dir }`, `WriteFile { path, contents: String }`, `WtApply { settings, scheme: Value, profile: Map<String, Value>, set_default }` (an object value such as `font` merges into the user's object key by key), `SetAutoRun { value: Option<String> }` (raw; the write keeps the value's REG type, a new value is `REG_SZ`), `InstallFont { family }`; `Clone + Debug + PartialEq + Eq + Serialize + Deserialize` with `#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]` (ruling P21) | Tasks 9–17 |
| `plan::Plan`, `plan::Warning` | `Plan { ops, summary_th, summary_en: Vec<&'static str>, warnings: Vec<Warning>, launch: ClinkLaunch }`; `Warning::{NoWindowsTerminal, MachineWideClink, FontFallback { wanted }}` | Tasks 13, 15, 16 |
| `plan::PlanError` | `ForeignClink { existing: Vec<String> }` (`E_AUTORUN_FOREIGN_CLINK`; `existing` is every command that turning the other Clink off takes out), `UnsupportedPath(PathBuf)` and `NoClinkPath(PathBuf)` (`E_CLINK_PATH`), `Generate(String)` (`E_THEME_INVALID`), `AutoRunNeedsHandEdit { part: String }` (`E_AUTORUN_WRITE`: a value cmd rejects, a command that holds our profile path without being our entry, or another Clink combined with other commands); `code()`; `Clone + Debug + PartialEq + Eq + Error` | Tasks 13, 14, 16 |
| `plan::{plan, clink_launch, autorun_entry}` | `plan(&Theme, &Request, &Machine, &Paths) -> Result<Plan, PlanError>`; `clink_launch(&Paths, &Machine) -> Result<ClinkLaunch, PlanError>`; `autorun_entry(&ClinkLaunch, &Paths) -> Result<String, PlanError>` gives `(if exist "<exe>" "<exe>" inject --autorun --quiet --profile "<profile>")` and refuses `%`, `"`, `;` and non-Unicode paths. The planner always puts the entry first: `<entry> & <value>`, or the entry alone (ruling P59). | Tasks 12–15 |
| `plan::autorun` (all `pub(crate)`) | `scan(&str) -> Scan { parts: Vec<Part>, rejected: bool }`; `Part { text, combined, .. }`; `is_our_entry(command, profile: &str) -> bool`; `remove_ours(current, profile) -> Result<AutoRunEdit, PlanError>` (a value with none of ours comes back unchanged); `edit_autorun(..)`; `AutoRunEdit { value, removed_ours, removed_foreign }`; `runs_clink(command) -> bool` (Task 0 tightens it). | Tasks 14, 15 |
| `plan::{OMP_VERSION, CLINK_VERSION, NERD_FONT_FAMILY}` | `"31.5.0"`, `"1.9.34"`, `"CaskaydiaCove NF"` | Tasks 2, 11, 13, 15 |
| `txn` | `Failure { code: &'static str, message: String }`; `Snapshot { op_index, prior: Option<Vec<u8>> }` (JSON `opIndex`); `Stage::{Planned, Snapshotted, Applying { done }, Verifying, Committed, RollingBack { from }, RolledBack, RollbackIncomplete { failed: Vec<usize> }}`, JSON `{"state": "applying", "done": 2}`; `Journal { id, ops, snapshots, stage }` (camelCase) with `is_finished()`; traits `Executor { capture, apply, restore }`, `Verifier { verify(&[Op]) }`, `JournalStore { save, load_unfinished }`, all `&mut self`; `#[must_use] Outcome::{Committed, RolledBack { code, reason }, RollbackIncomplete { code, reason, failed: Vec<(usize, Failure)> }}` (`failed` newest first; empty when the journal could not be read); `run(&str, Vec<Op>, &mut dyn Executor, &mut dyn Verifier, &mut dyn JournalStore) -> Outcome`; `#[must_use] recover(&mut dyn Executor, &mut dyn JournalStore) -> Option<Outcome>` (the recovery table of spec §5.5, cause `E_TXN_INTERRUPTED`) | Tasks 9–17 |
| `model` | `Theme { id: uuid::Uuid, name, terminal.{font, cursor_shape, ..}, prompt.{layout, segments, transient}, banner, .. }` with public fields; `Target::{All, Wt}` (`Copy`, serde lowercase); `Layout::{Box, Line, Powerline}`, `Segment::{Path, Git, Time, User, Host, Status, Duration}`, `CursorShape::{FilledBox, Bar, Underscore}` (all `#[non_exhaustive]`); `parse_theme(&[u8]) -> Result<Theme, ThemeError>`; `ThemeError::code()`; `Theme::to_file_bytes()` | Tasks 0, 8, 13, 16, 17 |
| `generate::wt` | `SCHEME_NAME`, `CMD_PROFILE_GUID`, `CMD_EXE`; `scheme(&Theme) -> Value`; `profile_values(&Theme, Target, clink_launch: &Path, clink_profile: &Path) -> Map<String, Value>` (`font` is an object `{ "face", "size" }`) | Tasks 7, 12, 15 |
| `generate::lua` | `Lang::{Th, En}` (`Copy + Eq`); `init_lua(omp_exe, omp_config)` runs `""<exe>" init cmd --config "<config>" 2>nul"` and its script under `pcall` (ruling P47); `banner_lua(&Theme, Lang)` picks its icon at run time from `WT_SESSION` | Tasks 13, 16, 19 |
| `generate::clink::settings(transient, scripts_dir)` | `clink.path = <data>\generated` (a `;`-separated list, which is why the planner refuses `;` in paths) | Task 19 |
| `generate::omp::config_text(&Theme) -> Result<String, serde_json::Error>` | | Task 17 |
| `presets` | `builtin() -> Result<Vec<(&'static str, Theme)>, ThemeError>`, Terracotta under the key `"terracotta"`, font `Cascadia Mono`. `presets::terracotta()` is `#[cfg(test)] pub(crate)`: unit tests inside the core call it; integration tests and other crates pick `"terracotta"` from `builtin()` by key. | Tests in Tasks 12–17 |
| Files | `presets/terracotta.cmdtheme` (`"font": "Cascadia Mono"`, `"layout": "box"`, `"opacity": 96`, `"acrylic": true`, `"cursorShape": "filledBox"`); `clippy.toml` and `[workspace.lints.clippy]` (ruling R1); `.gitattributes` `* text=auto eol=lf` (ruling R2); `rust-toolchain.toml`; `crates/core/tests/common/mod.rs` (Task 0) | Tasks 14, 16, 17, 19 |

## Decisions this plan takes

1. **`plan::autorun` stays `pub(crate)`.** `cleanup.rs` (Task 14) and `doctor.rs` (Task 15) are core modules and call `scan`, `is_our_entry`, `remove_ours` and `runs_clink` directly, so the planner, the cleanup and the doctor read AutoRun the same way. The CLI calls only `cleanup::cleanup` and `doctor::doctor`. No visibility is widened.
2. **Journal storage (decision F6).** `DiskJournal` (Task 9) writes each prior state once to `snapshots\<id>\payload\<opIndex>.bin`, before the journal that names it, and `journal.json` holds `{"opIndex": 3, "payload": "payload/3.bin"}` (or `null` for a target that did not exist) instead of a byte array. `txn::Snapshot`'s serde stays as it is for memory and tests.
3. **A rollback that could not finish survives a restart.** After any transaction (apply, undo, revert-all, crash recovery) that ends `RollbackIncomplete`, `actions::record_incomplete` (Task 13) writes `snapshots\<id>\incomplete.json`: `{ id, code, reason, steps: [{ opIndex, target, code, message, fix }] }`, where `fix` is a plain-words way to put that target back by hand (`exec::fix_by_hand`, Task 11), naming the payload file to copy back. It is not kept in `state.json`, which may itself be one of the targets the rollback could not put back. `actions::pending_incomplete` and `recover_at_start` read the newest one back; the CLI prints it on every run until a newer transaction runs (Task 16), and Phase 2's app can show the same record.
4. **"AutoRun never reaches our entry" (ruling P68).** The planner's reading of AutoRun accepts a few values cmd itself rejects. `state.json` records `clink.log`'s length at apply time (`Applied.clink_log_len`, Task 8), and doctor D3 (Task 15) reads only what Clink logged after it: no inject since the apply is a warning with the new code `E_THEME_NOT_LOADED`, which says in plain words to open a new cmd window, and if it stays, to fix the AutoRun value by hand in Registry Editor. D4 fails with `E_AUTORUN_WRITE` when cmd rejects the value, when our entry is gone, or when it is not the first command or is an older form.
5. **`runs_clink` (ruling F4).** F4 asked for `inject` as the word right after the launcher. Task 0 makes it Clink's verb: the first word after the launcher that is not an option, with `--profile <dir>` skipped, because `clink --profile <dir> inject` is a real injection that the strict rule would miss.
6. **Numbering.** The follow-ups are Task 0, so Tasks 1–19 keep the draft's numbers.

**Not in this plan:**
- The Tauri app, its IPC commands and the screens (Phases 2–3). `actions`, `cleanup` and `doctor` are written so those commands can call them directly.
- `logs\cmdstudio.log` with rotation and `drafts\` (spec §5.3): nothing reads them before the UI exists.
- The preview emulator and its parity test (spec §8 puts them in Phase 2). Task 17 only proves the real Oh My Posh renders every layout the generator writes.
- The uninstaller hook, updater and signing (Phase 4). `--cleanup` is ready for the hook.
- The VM tiers spec §7 still marks ⬜ (a machine with Anaconda; machines with Clink already installed).
- The Phase 1a minors the rulings doc defers past F2 (for example the Lua `clean()` single-scan pass and the invisible-only theme names): they stay deferred.

## Global Constraints

Every task's requirements include these. Values are copied from the spec, the design brief and Phase 1a's global constraints.

- App identifier `ai.resincore.cmdstudio`. Data folder `%LOCALAPPDATA%\ai.resincore.cmdstudio` (spec §5.3). The user's ProgramData folder is `%ProgramData%\ai.resincore.cmdstudio-<SID>`, directly under `%ProgramData%`, with no shared parent (spec §5.4).
- Pinned runtimes: Oh My Posh `31.5.0`, Clink `1.9.34` (x64 files only), Nerd Fonts `v3.5.1` `CascadiaCode.zip` (SHA256 `1298BF92698AFA06185CF1D05E6AE05F2D8A1E8C3CB45DDF4C3035168AB342A1`); `jsonc-parser` `0.34.0` with feature `cst` (G9).
- Only `crates/core/src/adapters/windows/` touches the machine; every other module goes through the adapter traits (spec §5.2 "adapters: ชั้นเดียวที่แตะเครื่องจริง"). This replaces Phase 1a's "Code in this crate never touches the machine".
- Windows Terminal (spec §5.4): settings files, only those that exist — Store `%LOCALAPPDATA%\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json`, Preview `%LOCALAPPDATA%\Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json`, unpackaged `%LOCALAPPDATA%\Microsoft\Windows Terminal\settings.json`. Scheme `CMD Studio`; cmd profile guid `{0caa0dad-35be-5f56-a8ff-afceeeaa6101}`; `defaultProfile` only when the user chose it. Comments and formatting stay (jsonc-parser CST). A file that does not parse is never written: `E_WT_PARSE`.
- AutoRun (spec §5.4): `HKCU\Software\Microsoft\Command Processor`, value `AutoRun`. Keep the existing value's type (`REG_SZ` or `REG_EXPAND_SZ`); a new value is `REG_SZ`; values are read and written raw, never expanded. Open the existing key and change only that value; never recreate the key (spec §2.2 lesson 9). Our entry is `(if exist "<clink-launch>" "<clink-launch>" inject --autorun --quiet --profile "<data>\clink-profile")`, always the first command (`<entry> & <value>`). Other programs' commands never change by a byte; a value that cannot be edited safely is `E_AUTORUN_WRITE` with hand-edit advice, never a guess. Only `plan::autorun` edits a value. HKLM is read only.
- Fonts (spec §5.4, G12): the file goes in `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, the value in `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts`; `AddFontResourceW`/`RemoveFontResourceW` and a `WM_FONTCHANGE` broadcast. Family `CaskaydiaCove NF`. Uninstall only fonts CMD Studio installed.
- Files (spec §5.4): atomic write = temp file in the same folder → flush → rename over the target. Runtimes go into new versioned folders; an existing runtime folder is never overwritten.
- ProgramData folder (spec §5.4 step 3, brief): created with `CreateDirectoryW` and the security descriptor `D:P(A;OICI;FA;;;<SID>)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)` in one call. An existing folder is usable only when its owner is the user, SYSTEM or Administrators and no other SID may write, append or delete. If it appears between `detect` and the install, check it again before writing, else `E_CLINK_PATH`.
- Transactions (spec §5.5): the journal is written before every stage change; every op can run again safely; verify = (1) read back every written value, (2) `oh-my-posh print primary --config <generated> --shell cmd` succeeds, (3) `<clink-launch> --profile <p> set clink.logo` prints no line matching `\.lua:\d+:` (its exit code is always 0, G5), (4) every written settings.json parses. Failures: `E_VERIFY_READBACK`, `E_VERIFY_OMP`, `E_VERIFY_CLINK`, `E_WT_PARSE`. A crash is rolled back at the next start. A rollback that cannot finish says which parts and how to fix each by hand, keeps saying so after a restart, and is never reported as success.
- Restore points (spec §5.3, §5.5, brief): `<data>\snapshots\<id>\manifest.json`, `journal.json` (camelCase), `payload\<opIndex>.bin`, and `incomplete.json` after a rollback that could not finish; keep the 20 newest plus the first ever.
- Doctor (spec §5.8, brief): D1–D11, each `{id, status: ok|warn|fail|skipped, code, messageTh, messageEn, details}`; `ok` only from a real check (spec §2.1). D3 includes the `clink.log` scan for `Unable to get DLL version` / `Failed to copy DLL` (G19) and the inject since the apply (ruling P68). D5 reads AutoRun with `runs_clink`. D8 counts GDI fonts only (brief; a known limitation).
- CLI (spec §5.7): `cmdstudio apply <file.cmdtheme> [--target all|wt]`, `cmdstudio doctor [--json]`, `cmdstudio undo`, `cmdstudio --cleanup [--remove-themes]`; exit `0` done, `1` failed and rolled back, `2` invalid input, `3` rollback incomplete. `--data-dir <dir>` and `--resources <dir>` override the defaults `%LOCALAPPDATA%\ai.resincore.cmdstudio` and `<exe dir>\resources`. The plan summary (English) prints before applying.
- CMD Studio runs only its own bundled programs from the runtime folders, with fixed arguments, no shell and no console window (spec §5.10; §2.2 lesson 7: never steal focus).
- Test safety (brief, non-negotiable): by default no test changes the developer's real AutoRun, Windows Terminal settings or fonts. Registry tests use `HKCU\Software\CmdStudioTest\<uuid>` and delete it; file tests use temp folders; font tests leave the session font table alone. Tests that need the real per-user places run only when `CMDSTUDIO_SYSTEM_TESTS=1` (set in CI and on the test VMs) and otherwise print `SKIPPED …` and pass. Never set it on a development machine except for Task 17's `omp_render`, which writes only to a temp folder. Tests may read repo files (fixtures, goldens, `schema/`, `presets/`); they write repo files only behind `UPDATE_GOLDEN=1` or `UPDATE_SCHEMA=1`, never by default.
- Production code (everything outside `#[cfg(test)]` and `tests/`): no `unwrap`, `expect`, `panic!`, unchecked indexing or slicing, or `serde_json::Value` IndexMut; the workspace denies clippy `unwrap_used`, `expect_used`, `panic`, `indexing_slicing` and `string_slice` (ruling R1). Errors are typed. Every `unsafe` block has a `// SAFETY:` comment. JSON that CMD Studio writes uses camelCase field names (ruling P21).
- Test code: `clippy.toml` lets `#[test]` functions and `#[cfg(test)]` modules use unwrap, expect, panic and indexing, but not top-level helper functions in `tests/*.rs`, and never `string_slice`. An integration test file may start with `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]`; slicing a `&str` in tests uses `.get(..)`.
- Line endings: `.gitattributes` sets `* text=auto eol=lf` (`.bat` and `.cmd` CRLF). A fixture that must keep CRLF bytes needs its own `-text` line there.
- Scratch copies and mutation runs use their own `CARGO_TARGET_DIR`, never the repository's `target/`; if one ever shared it, run `cargo clean -p cmdstudio-core` before trusting a repository run.
- Implementation Gate (spec §0): before calling an external crate API, read it for the version in `Cargo.lock` (`cargo doc -p <crate> --no-deps --open`, or its source under `~/.cargo/registry/src`). If a signature differs from this plan, follow the real API and say so in the commit message.
- Each task ends clean: `cargo fmt --all`, then `cargo clippy -p cmdstudio-core --all-targets -- -D warnings` (the core as a release build sees it), from Task 1 on also `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings` (the fakes are production code for other crates), and from Task 16 on also `cargo clippy --workspace --all-targets -- -D warnings`. In a workspace build the CLI's dev-dependency turns the core's `fake` feature on, so the workspace run alone never lints the core without it.
- Commits: write the English message to `.git/CMDSTUDIO_COMMIT_MSG` with the Write tool, then `git commit -F .git/CMDSTUDIO_COMMIT_MSG`. Stage only the task's files with `git add -- <files>` and check `git diff --cached --name-only`. Never add Co-Authored-By, "Generated with" or any Claude/claude.ai line; never `--no-verify`; never push. Follow the `auroraz-pr-workflow` skill.

## Review Focus

The inputs most likely to bite a person using this, which the spec implies but no requirement spells out. Each one has a test in the task that owns the code.

1. **Windows Terminal installed but never opened** — a fresh Windows 11 has no `settings.json` until WT first starts. Apply must not create or guess the file: the plan warns, and doctor D6 says "open it once, then apply again" (`E_WT_NOT_FOUND`). Pinned in Task 15: `windows_terminal_never_opened_is_a_warning_with_a_way_out`.
2. **A cmd window or Windows Terminal open during undo or `--cleanup`** — Clink's files or the font file may be in use. Undo must never leave a half-deleted runtime folder (rename first, so it fails whole), and the font must end unregistered even when Windows Terminal still holds its file. Pinned in Task 11: `a_runtime_in_use_is_not_half_deleted`, `a_font_file_in_use_still_ends_uninstalled`; Task 19 repeats it on VM-A with a real cmd window holding Clink's DLL.
3. **settings.json in real-world shapes** — comments, trailing commas, a UTF-8 BOM, CRLF, the legacy `"profiles": [ … ]`, an uppercase GUID, the user's own cmd profile with `font.weight`, no `schemes` or `profiles` at all. Untouched bytes stay, re-applying changes nothing, and revert gives the original bytes back. Pinned in Task 7, for example `bom_and_crlf_are_kept` and `a_file_with_no_profiles_or_schemes_gets_them_and_loses_them_again`.
4. **A Windows user name with Thai letters and a space** (VM-C) — the registry value, the font registration, process arguments, the 8.3 path, `clink.path` and file writes all carry Unicode paths. Pinned in Task 3 (`write_atomic_replaces_creates_parents_and_leaves_no_temp_file` under a Thai folder), Task 4 (`raw_values_come_back_byte_for_byte_with_their_type`), Task 5 (`short_path_names_the_same_folder`), Task 6 (`registration_values_live_in_the_given_key`), Task 10 (`fills_the_planner_facts`, a Thai data folder), Task 17 (the theme file `ธีม ทดสอบ.cmdtheme`) and Task 19 (the whole flow as `ผู้ทดสอบ ไทย` on VM-C, with a check that Clink found the scripts through the Thai `clink.path`).
5. **A settings.json that is a symbolic link** into the user's dotfiles — writing it must write the link's target and keep the link. Pinned in Task 3: `writing_through_a_symlink_keeps_the_link`, which must run (not skip) when `CMDSTUDIO_SYSTEM_TESTS=1`.
6. **An AutoRun that stops before our entry** — another program's value that cmd rejects, or a command put in front of ours later. The user must hear it in plain words with the hand edit, not "applied". Pinned in Task 15: `only_a_cmd_window_started_since_the_apply_counts_for_d3`, `our_entry_must_come_first_in_autorun`; Task 16: `an_autorun_cmd_rejects_needs_a_hand_edit_and_nothing_changes`; Task 19 checks the parenthesized entry with a real interactive inject.
7. **A rollback that cannot finish, then a restart** — the "fix by hand" text must still be there when the user comes back. Pinned in Task 13: `a_rollback_that_cannot_finish_says_how_to_fix_it_even_after_a_restart`; Task 16: `a_failed_verify_exits_1_and_a_failed_rollback_exits_3`.

## File map

```
Cargo.toml                                    + member crates/cli (Task 16)
.gitignore                                    + /resources/ (Task 2)
scripts/fetch-resources.ps1                   pinned downloads → resources\ (Task 2)
.github/workflows/ci.yml                      fmt, clippy, tests, system tests (Task 18)
crates/core/Cargo.toml                        publish = false (Task 0); dependencies, `fake` feature (Tasks 1–7)
crates/core/src/lib.rs                        crate doc and module list (Task 1, then every task)
crates/core/src/model.rs, schema.rs, txn.rs, oklch.rs, palette.rs, generate/lua.rs   follow-ups (Task 0)
crates/core/src/plan.rs, plan/autorun.rs      docs, runs_clink (Task 0)
schema/cmdtheme-1.schema.json                 no writeOnly (Task 0)
crates/core/src/adapters/mod.rs               traits, AdapterError, RegValue, SystemInfo, System (Task 1)
crates/core/src/adapters/fake.rs              in-memory fakes (Task 1)
crates/core/src/adapters/windows/mod.rs       wide strings, error helpers, module list (Task 3)
crates/core/src/adapters/windows/fs.rs        WinFs: atomic writes (Task 3)
crates/core/src/adapters/windows/registry.rs  RegLocation, WinAutoRun, TestKey (Task 4)
crates/core/src/adapters/windows/host.rs      WinHost: SID, known folders, 8.3, private folder, system facts (Task 5)
crates/core/src/adapters/windows/fonts.rs     WinFonts (Task 6)
crates/core/src/adapters/windows/wt.rs        WinWt: where settings.json lives (Task 7)
crates/core/src/adapters/windows/process.rs   WinProcess (Task 12)
crates/core/src/resources.rs                  Resources, Manifest, sha256_hex (Task 2)
crates/core/src/wt_edit.rs                    settings.json edit, read, revert (Task 7)
crates/core/src/state.rs                      state.json, baseline, clink_log_len, Paths helpers (Task 8)
crates/core/src/snapshot.rs                   restore points, DiskJournal, incomplete.json (Task 9)
crates/core/src/detect.rs                     detect (Task 10)
crates/core/src/exec.rs                       SystemExecutor, UndoExecutor, UndoVerifier, sweep, fix_by_hand (Task 11)
crates/core/src/verify.rs                     SystemVerifier (Task 12)
crates/core/src/actions.rs                    prepare_apply, run_apply, undo, recover_at_start, record_incomplete (Task 13)
crates/core/src/cleanup.rs                    revert_all, cleanup (Task 14)
crates/core/src/doctor.rs                     D1–D11 (Task 15)
crates/core/tests/common/mod.rs               the shared Lua PRELUDE (Task 0)
crates/core/tests/fixtures/hostile/*, tests/golden/*   new samples and goldens (Task 0)
crates/core/tests/fixtures/wt/*               settings.json shapes (Task 7)
crates/core/tests/omp_render.rs               gated: the real OMP renders every layout (Task 17)
crates/cli/Cargo.toml                         (Task 16)
crates/cli/src/args.rs, lib.rs, main.rs       the CLI (Task 16)
crates/cli/tests/system.rs                    gated: apply → doctor → undo on this machine (Task 17)
docs/superpowers/specs/2026-10-10-cmd-studio-design.md   §5.9 codes, §5.4/§5.8 D6 and D8 (Task 15); §5.7 CLI options (Task 16); G6, §5.4 checks and appendix B results (Task 19)
spikes/vm/guest/phase1b-check.ps1             VM evidence script (Task 19)
spikes/vm/guest/guard.test.ps1, spikes/vm/README.md   + phase1b-check.ps1 (Task 19)
```

---
### Task 0: Phase 1a follow-ups (ruling F2)

The final review of Phase 1a left 13 minors and a few cheap tests for this task (rulings F2 and F4 in `docs/superpowers/plans/2026-10-10-phase1a-core-rulings.md`). Every change edits shipped Phase 1a code, so the code below is in the repository's own format (the default `rustfmt`), and each "replace" quotes the shipped text exactly. Nothing here changes what Phase 1b's later tasks rely on, except `runs_clink`, which doctor D5 (Task 15) calls.

**Files:**
- Modify: `crates/core/Cargo.toml`, `crates/core/src/model.rs`, `crates/core/src/schema.rs`, `schema/cmdtheme-1.schema.json`, `crates/core/src/generate/lua.rs`, `crates/core/src/plan.rs`, `crates/core/src/plan/autorun.rs`, `crates/core/src/txn.rs`, `crates/core/src/oklch.rs`, `crates/core/src/palette.rs`, `crates/core/tests/schema_file.rs`, `crates/core/tests/banner_lua.rs`, `crates/core/tests/hostile_themes.rs`, `crates/core/tests/golden.rs`, `crates/core/tests/golden/terracotta.banner.en.lua`, `crates/core/tests/golden/terracotta.banner.th.lua`, `crates/core/tests/fixtures/hostile/lua-breakers.cmdtheme`
- Create: `crates/core/tests/common/mod.rs`, `crates/core/tests/fixtures/hostile/c1-and-nul.cmdtheme`, `crates/core/tests/fixtures/hostile/bidi-isolates.cmdtheme`, `crates/core/tests/golden/terracotta.cmdtheme`, `crates/core/tests/golden/terracotta.no-transient.clink_settings`

**Interfaces:**
- Consumes: the shipped Phase 1a code at `05d4968`.
- Produces: no new public items. Behaviour that changes:
  - `parse_theme` refuses a word of the format written as an object (`"layout": {"box": null}`) with `E_THEME_INVALID` and `prompt.layout: expected a string`.
  - `ThemeError::Invalid`'s whole message, `not a valid theme file: ` included, is at most 300 characters (spec §3.3 says so; it could be 324).
  - `plan::autorun::runs_clink` (pub(crate)) counts a command as another Clink only when `inject` is Clink's verb: the first word after the launcher that is not an option, `--profile <dir>` skipped. `type "C:\clink\clink.bat" | find "inject"` and `clink.bat set inject` are no longer another Clink.
  - `txn::recover` of a journal cut off in `Planned` says `the previous apply was interrupted before it changed anything`.
  - The published schema's `$schema` property loses `writeOnly`; `UPDATE_SCHEMA` rewrites the schema file only when it is exactly `1`.
  - `banner.lua` writes NUL as `\0` in its first pattern instead of Lua 5.1's `%z` (same behaviour in Lua 5.2).

**Where each item comes from:**

| Item | Rulings doc | Step |
|---|---|---|
| Unit-variant words in object form refused (`tone`, `cursorShape`, `layout`, `segments`) | Task 5 minor | 1 |
| Whole refusal ≤ 300 characters | Task 5 minor | 2 |
| `#[doc(hidden)]` on `Theme::schema` | Task 5 minor | 3 |
| No `writeOnly` on `$schema`; `UPDATE_SCHEMA` must be `1` | Task 6 minor | 4 |
| `runs_clink`: `inject` must be Clink's verb | F4 | 6 |
| A `Planned` interruption says nothing had changed | final review | 7 |
| Lua `clean()` uses `\0`, not `%z` | final review | 8 |
| Docs: `Machine::autorun_hkcu`/`Op::SetAutoRun` are raw and keep the REG type; `WtApply` merges `font` key by key; `Scan::rejected` and `(echo a) )` | Task 11 minor, final review | 9 |
| `publish = false` (presets embed files from outside the crate) | Task 7 minor | 9 |
| Shared `tests/common` PRELUDE | Task 12b minor | 11 |
| Hue wrap 0–360; the P23 mutant killers (a coloured accent tints the neutrals; cursor == accent after a fix) | Tasks 2 and 3 minors | 12 |
| Goldens for `Theme::to_file_bytes` and `clink::settings(false, ..)` | Task 12b minor | 13 |
| Hostile samples for U+0000, U+007F–U+009F and U+2066–U+2069; a `lua-breakers` line that really sets the global | Tasks 4 and 12b minors | 14 |

- [ ] **Step 1: A word of the format is a string, never an object.** serde reads a unit variant from a one-key object too, so `"layout": {"box": null}` parses today, while the format and its schema say string.

Add this test to `crates/core/src/model.rs`'s `tests`, right before `fn strips_control_characters_from_text`:
```rust
    #[test]
    fn a_word_of_the_format_is_a_string_and_never_an_object() {
        // serde reads `{"box": null}` as the word `box`; the format and its schema do not.
        for (pointer, word, path) in [
            ("/tone", "dark", "tone"),
            ("/terminal/cursorShape", "filledBox", "terminal.cursorShape"),
            ("/prompt/layout", "box", "prompt.layout"),
        ] {
            let mut object = serde_json::Map::new();
            object.insert(word.to_string(), Value::Null);
            assert_eq!(
                at(pointer, Value::Object(object)).unwrap_err(),
                ThemeError::Invalid(format!("{path}: expected a string")),
                "{pointer}"
            );
            assert!(at(pointer, json!(word)).is_ok(), "{pointer}");
        }
        assert_eq!(
            at("/prompt/segments", json!(["path", { "git": null }])).unwrap_err(),
            ThemeError::Invalid("prompt.segments[1]: expected a string".into())
        );
        // any other wrong value is still reported by the reader, with its line and column
        let text = invalid_text_with(|v| v["prompt"]["layout"] = json!(5));
        assert!(
            text.starts_with("prompt.layout: expected value at line 1"),
            "{text}"
        );
    }
```
Run: `cargo test -p cmdstudio-core --lib -- model::tests::a_word_of_the_format`
Expected: FAIL — `called Result::unwrap_err() on an Ok value` for `/tone`.

Then, in `model.rs`, add above `/// An id is a random UUID: version 4, of the RFC 4122 variant (spec §3.3).`:
```rust
/// The words of the format, as JSON pointers and as the paths an error names them by. Each of
/// `prompt.segments` is a word as well.
const WORDS: [(&str, &str); 3] = [
    ("/tone", "tone"),
    ("/terminal/cursorShape", "terminal.cursorShape"),
    ("/prompt/layout", "prompt.layout"),
];

/// A word of the format (`tone`, `terminal.cursorShape`, `prompt.layout` and each of
/// `prompt.segments`) is a string. serde also reads a word from a one-key object, so without
/// this `"layout": {"box": null}` would parse; the format and its published schema say string.
/// Only objects are refused here: any other wrong value is for serde to report.
fn check_words(value: &serde_json::Value) -> Result<(), ThemeError> {
    for (pointer, path) in WORDS {
        if value
            .pointer(pointer)
            .is_some_and(serde_json::Value::is_object)
        {
            return Err(invalid(&format!("{path}: expected a string")));
        }
    }
    let segments = value
        .pointer("/prompt/segments")
        .and_then(serde_json::Value::as_array);
    if let Some(i) = segments.and_then(|s| s.iter().position(serde_json::Value::is_object)) {
        return Err(invalid(&format!("prompt.segments[{i}]: expected a string")));
    }
    Ok(())
}
```
In `parse_theme`, after `check_objects(&value)?;` add the line `check_words(&value)?;`, and in its doc comment replace
```text
/// object`). The message quotes the file at times, so it is cleaned and cut short. Text is
```
with
```text
/// object`), and a word of the format written as an object (`prompt.layout: expected a
/// string`). The message quotes the file at times, so it is cleaned and cut short. Text is
```
Run: `cargo test -p cmdstudio-core --lib -- model::tests`
Expected: PASS (57 tests).

- [ ] **Step 2: The whole refusal is at most 300 characters.** Today the 300-character cap is on the reader's text, and `not a valid theme file: ` (24 characters) comes on top.

In `model.rs`'s test `error_messages_are_cleaned_and_capped`, replace
```rust
        // the cap counts characters, not bytes: 300 stay whole, 301 are cut to 300 with dots
        for c in ["x", "ก", "👋"] {
            assert_eq!(invalid(&c.repeat(300)), ThemeError::Invalid(c.repeat(300)));
            assert_eq!(
                invalid(&c.repeat(301)),
                ThemeError::Invalid(format!("{}...", c.repeat(297)))
            );
        }
        // the cut is made after the cleaning, so controls do not use up the room
        assert_eq!(
            invalid(&format!("{}{}", "\u{7}".repeat(1000), "y".repeat(300))),
            ThemeError::Invalid("y".repeat(300))
        );
```
with
```rust
        // the whole message is at most 300 characters (spec §3.3), and its start takes 24 of
        // them; the cap counts characters, not bytes: 276 stay whole, 277 are cut to 276 with
        // dots
        assert_eq!(INVALID_START.chars().count(), 24);
        assert_eq!(
            ThemeError::Invalid(String::new()).to_string(),
            INVALID_START
        );
        for c in ["x", "ก", "👋"] {
            assert_eq!(invalid(&c.repeat(276)), ThemeError::Invalid(c.repeat(276)));
            let cut = invalid(&c.repeat(277));
            assert_eq!(cut, ThemeError::Invalid(format!("{}...", c.repeat(273))));
            assert_eq!(cut.to_string().chars().count(), 300);
            assert!(invalid(&c.repeat(10_000)).to_string().chars().count() <= 300);
        }
        // the cut is made after the cleaning, so controls do not use up the room
        assert_eq!(
            invalid(&format!("{}{}", "\u{7}".repeat(1000), "y".repeat(276))),
            ThemeError::Invalid("y".repeat(276))
        );
```
In `crates/core/tests/hostile_themes.rs`, replace
```rust
/// The longest refusal: spec §3.3 allows a message 300 characters, and `E_THEME_INVALID` puts
/// "not a valid theme file: " (24 characters) in front of it.
const MAX_REFUSAL_CHARS: usize = 300 + 24;
```
with
```rust
/// The longest refusal: spec §3.3 allows a message 300 characters, the "not a valid theme
/// file: " (24 characters) that `E_THEME_INVALID` puts in front of the reader's text included.
const MAX_REFUSAL_CHARS: usize = 300;
```
and in `a_hostile_key_of_400_letters_gives_a_message_that_is_clean_and_cut_short` replace
```rust
    // the escape sequence is cut out of the key, the key is the path, and the message is cut
    // at 300 characters with three dots
    assert!(text.starts_with("red0;pwnedkkkk"), "{text}");
    assert_eq!(text.chars().count(), 300, "{text}");
```
with
```rust
    // the escape sequence is cut out of the key, the key is the path, and the reader's text is
    // cut at 276 characters with three dots, so that the whole message is 300
    assert!(text.starts_with("red0;pwnedkkkk"), "{text}");
    assert_eq!(text.chars().count(), 276, "{text}");
```
Run: `cargo test -p cmdstudio-core --lib -- model::tests::error_messages` and `cargo test -p cmdstudio-core --test hostile_themes -- a_hostile_key`
Expected: the first does not compile (`cannot find value INVALID_START`); the second FAILS (`left: 300, right: 276`).

Then in `model.rs` replace
```rust
/// The most characters of a parser's message that an `Invalid` error keeps.
const MAX_MESSAGE_CHARS: usize = 300;

/// The `Invalid` error for the message a parser gave. That message can quote the file
/// itself (an unknown key, an unknown variant), and the file is not to be trusted with
/// what a screen or a log prints, so the message is cleaned like the file's own text and
/// cut to `MAX_MESSAGE_CHARS`. The field path is what the message starts with, and the cut
/// is made at the end, so the path stays.
fn invalid(message: &str) -> ThemeError {
    let clean = sanitize_text(message);
    if clean.chars().count() <= MAX_MESSAGE_CHARS {
        return ThemeError::Invalid(clean);
    }
    let mut cut: String = clean.chars().take(MAX_MESSAGE_CHARS - 3).collect();
```
with
```rust
/// What an `Invalid` error says in front of the parser's message (see its `#[error]`).
const INVALID_START: &str = "not a valid theme file: ";
/// The most characters an `Invalid` error's whole message has, its start included (spec §3.3).
const MAX_MESSAGE_CHARS: usize = 300;
/// The most characters of the parser's message that an `Invalid` error keeps. The start is
/// ASCII, so its length in bytes is its length in characters.
const MAX_KEPT_CHARS: usize = MAX_MESSAGE_CHARS - INVALID_START.len();

/// The `Invalid` error for the message a parser gave. That message can quote the file
/// itself (an unknown key, an unknown variant), and the file is not to be trusted with
/// what a screen or a log prints, so the message is cleaned like the file's own text and
/// cut to `MAX_KEPT_CHARS`, which keeps the whole message within `MAX_MESSAGE_CHARS`. The
/// field path is what the message starts with, and the cut is made at the end, so the path
/// stays.
fn invalid(message: &str) -> ThemeError {
    let clean = sanitize_text(message);
    if clean.chars().count() <= MAX_KEPT_CHARS {
        return ThemeError::Invalid(clean);
    }
    let mut cut: String = clean.chars().take(MAX_KEPT_CHARS - 3).collect();
```
and in the doc of `ThemeError::Invalid` replace `/// file's own text, and it is at most 300 characters.` with
```rust
    /// file's own text, and the whole message, `not a valid theme file: ` included, is at
    /// most 300 characters (spec §3.3).
```
Run the same two commands.
Expected: PASS.

- [ ] **Step 3: Hide `Theme::schema` from the API docs.** In `model.rs`, on the field `pub schema: ()`, add `#[doc(hidden)]` after `#[schemars(with = "String")]`, and after the comment line `// The unit type is the point: nothing is kept, so this never changes what a theme equals.` add:
```rust
    // Hidden from the API docs: it holds nothing, and code that builds a theme writes
    // `schema: ()` only because a struct literal has to name every field.
```
Run: `cargo test -p cmdstudio-core --lib -- model::tests`
Expected: PASS (`#[doc(hidden)]` changes nothing else).

- [ ] **Step 4: The schema's `$schema` property is a plain string, and only `UPDATE_SCHEMA=1` rewrites the file.** `schemars` marks the property `writeOnly`, which an editor may hide or flag; in a file a person writes it means nothing.

In `crates/core/tests/schema_file.rs`, add after `schema_file_matches_the_rust_types`:
```rust
#[test]
fn the_schema_line_is_a_plain_string() {
    // An editor may hide or flag a writeOnly property, and in a file a person writes it means
    // nothing: the line is only read and dropped.
    let schema = schema();
    let line = schema.pointer("/properties/$schema").unwrap();
    assert_eq!(line.get("type"), Some(&json!("string")));
    assert_eq!(line.get("writeOnly"), None, "{line}");
}
```
and in `schema_file_matches_the_rust_types` replace
```rust
    // A plain run only reads the committed file. It is rewritten when UPDATE_SCHEMA is set, and
    // then only to hold exactly what the types generate.
    if std::env::var_os("UPDATE_SCHEMA").is_some() {
```
with
```rust
    // A plain run only reads the committed file. It is rewritten when UPDATE_SCHEMA is 1, and
    // then only to hold exactly what the types generate. Only the exact value `1` does it, as
    // with UPDATE_GOLDEN, so that UPDATE_SCHEMA=0 cannot rewrite it by mistake.
    if std::env::var("UPDATE_SCHEMA").is_ok_and(|value| value == "1") {
```
Run: `cargo test -p cmdstudio-core --test schema_file`
Expected: FAIL — `the_schema_line_is_a_plain_string`: `left: Some(Bool(true)), right: None`.

Then in `crates/core/src/schema.rs`, in the doc of `theme_schema_json`, add this paragraph before the one that starts `/// The text is the same on every run`:
```rust
/// `schemars` marks the optional `$schema` line `writeOnly`, because CMD Studio reads it and
/// does not write it back. In JSON Schema `writeOnly` speaks of a server and its clients, and an
/// editor may hide or flag such a property; in a file a person writes it means nothing, so it is
/// taken out.
///
```
and replace its body's first lines
```rust
    let schema = SchemaSettings::draft2020_12()
        .with_transform(RestrictFormats::default())
        .into_generator()
        .into_root_schema_for::<crate::model::Theme>();
```
with
```rust
    let mut schema = SchemaSettings::draft2020_12()
        .with_transform(RestrictFormats::default())
        .into_generator()
        .into_root_schema_for::<crate::model::Theme>();
    // Edited on the `Schema` itself: it writes its keywords in an order of its own, which a
    // trip through `serde_json::Value` would sort.
    if let Some(line) = schema
        .pointer_mut("/properties/$schema")
        .and_then(serde_json::Value::as_object_mut)
    {
        line.remove("writeOnly");
    }
```
(`schemars::Schema::pointer_mut` exists in schemars 1.2.2, the version in `Cargo.lock`; editing a `serde_json::Value` copy instead would reorder every keyword of the file.)
Run, in PowerShell: `$env:UPDATE_SCHEMA = '1'; cargo test -p cmdstudio-core --test schema_file; Remove-Item Env:\UPDATE_SCHEMA`, then `git diff --stat -- schema/` and `cargo test -p cmdstudio-core --test schema_file`.
Expected: `schema/cmdtheme-1.schema.json` changes in one place only — under `"$schema"`, the lines `"type": "string",` and `"writeOnly": true` become `"type": "string"`; then `11 passed`.

- [ ] **Step 5: Commit**

Run `cargo fmt --all` and `cargo clippy -p cmdstudio-core --all-targets -- -D warnings` (no warnings). Message:
```text
fix(core): tighten the theme reader and the published schema

A word of the format written as an object ({"box": null}) is refused
with "prompt.layout: expected a string"; serde accepted it. The whole
E_THEME_INVALID message, its "not a valid theme file: " start included,
is now at most 300 characters, as spec 3.3 says. Theme::schema is
hidden from the API docs. The schema's $schema property loses
writeOnly, and only UPDATE_SCHEMA=1 rewrites the schema file.

Phase 1a follow-ups (ruling F2).
```
Run:
```bash
git add -- crates/core/src/model.rs crates/core/src/schema.rs schema/cmdtheme-1.schema.json crates/core/tests/schema_file.rs crates/core/tests/hostile_themes.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

- [ ] **Step 6: `inject` must be Clink's verb (ruling F4).** Today `runs_clink` accepts `inject` anywhere after a launcher word, so `type "C:\clink\clink.bat" | find "inject"` is listed as another Clink. Clink's command line is `clink [--profile <dir>] <verb> ...`, so the verb is the first word after the launcher that is not an option. The rule is not "the very next word", because `clink --profile <dir> inject` is a real injection that rule would miss, and two Clinks would then load into every cmd window.

In `crates/core/src/plan/autorun.rs`, test `only_a_command_that_injects_clink_is_another_clink`, replace the comment `// after it the word \`inject\`, in any case.` with
```rust
        // `inject` as its verb, in any case. Clink takes `--profile <dir>` before its verb.
```
add to the first list, after `r#""C:\clink\clink.bat" inject --autorun >nul 2>&1"#,`:
```rust
            r#""C:\clink\clink.bat" --profile "D:\my profile" inject --autorun"#,
            "clink --PROFILE p INJECT",
```
replace the comment `// does not inject it into cmd.` with
```rust
        // does not inject it into cmd, and one where `inject` is not Clink's verb.
```
and add to the second list, after `r#""C:\x.bat & clink inject"#,`:
```rust
            r#"type "C:\clink\clink.bat" | find "inject""#,
            "clink.bat set inject",
            "clink --profile inject",
            "clink",
```
Run: `cargo test -p cmdstudio-core --lib -- plan::autorun::tests::only_a_command`
Expected: FAIL — `type "C:\clink\clink.bat" | find "inject"`.

Then replace `runs_clink` and its doc with:
```rust
/// Whether `command` injects Clink into cmd: a word that names one of [`CLINK_LAUNCHERS`],
/// with or without a folder, quoted or not, whose verb (see [`clink_verb`]) is `inject`, both
/// in any case. Only `inject` loads Clink into cmd, so a call of Clink with another verb
/// (`clink.bat set clink.logo none`) is not an injection, nor is a command where `inject` is
/// not Clink's verb (`type "C:\clink\clink.bat" | find "inject"`), and a command that only
/// mentions Clink, such as `doskey /macrofile="C:\tools\clink\macros.txt"` or
/// `cd /d C:\clink-notes`, is another program's own. The AutoRun editor (another Clink, asked
/// about and taken out with the user's agreement), the warning about Clink in HKLM and the
/// doctor (D5) all read commands this way.
pub(crate) fn runs_clink(command: &str) -> bool {
    let words = command_words(command);
    words.iter().enumerate().any(|(i, word)| {
        let name = word.rsplit(['\\', '/']).next().unwrap_or(word);
        CLINK_LAUNCHERS.iter().any(|l| name.eq_ignore_ascii_case(l))
            && clink_verb(words.get(i + 1..).unwrap_or_default())
                .is_some_and(|verb| verb.eq_ignore_ascii_case("inject"))
    })
}

/// The verb of a Clink command, given the words after the launcher: the first word that is not
/// one of Clink's options. `--profile` takes the word after it as its value (`clink --profile
/// <dir> set clink.logo`, as the verifier runs it); any other word that starts with `-` is an
/// option on its own. `None` when the words hold no verb.
fn clink_verb(after_launcher: &[String]) -> Option<&str> {
    let mut words = after_launcher.iter();
    while let Some(word) = words.next() {
        if word.eq_ignore_ascii_case("--profile") {
            words.next();
        } else if !word.starts_with('-') {
            return Some(word);
        }
    }
    None
}
```
Run: `cargo test -p cmdstudio-core --lib -- plan::`
Expected: PASS (49 tests: 30 in `plan::tests`, 19 in `plan::autorun::tests`).

- [ ] **Step 7: A run cut off in `Planned` says nothing had changed.** In `crates/core/src/txn.rs`, test `recovery_restores_the_ops_a_crash_may_have_touched_newest_first` (it loops over every stage), replace
```rust
            assert_eq!(out, Some(interrupted()), "{stage:?}");
```
with
```rust
            // A run cut off before it applied anything says so: the user has nothing to check.
            let want = if stage == Stage::Planned {
                Outcome::RolledBack {
                    code: "E_TXN_INTERRUPTED",
                    reason: "the previous apply was interrupted before it changed anything".into(),
                }
            } else {
                interrupted()
            };
            assert_eq!(out, Some(want), "{stage:?}");
```
Run: `cargo test -p cmdstudio-core --lib -- txn::tests`
Expected: FAIL — `Planned`: the reason is `the previous apply was interrupted`.

Then in `recover` replace
```rust
    let cause = Failure {
        code: "E_TXN_INTERRUPTED",
        message: "the previous apply was interrupted".to_string(),
    };
```
with
```rust
    let message = if j.stage == Stage::Planned {
        "the previous apply was interrupted before it changed anything"
    } else {
        "the previous apply was interrupted"
    };
    let cause = Failure {
        code: "E_TXN_INTERRUPTED",
        message: message.to_string(),
    };
```
and add to `recover`'s doc, before the paragraph that starts `/// The result must be looked at`:
```rust
/// The reason says what the user can count on: a run cut off in `Planned` had not changed
/// anything yet, and its reason says so.
///
```
Run: `cargo test -p cmdstudio-core --lib -- txn::tests`
Expected: PASS (25 tests).

- [ ] **Step 8: Name NUL as `\0` in `banner.lua`'s `clean()`.** Lua 5.2, which Clink runs, takes `\0` in a pattern and has deprecated Lua 5.1's `%z`. In `crates/core/src/generate/lua.rs`, in `VARS_BODY`, replace `  s = s:gsub("[%z\1-\31\127]", "")` with `  s = s:gsub("[\0\1-\31\127]", "")`, and in the doc above it replace `/// can match in the middle of another character.` with
```rust
/// can match in the middle of another character. The first class names NUL as `\0`: Lua 5.2,
/// which Clink runs, takes it in a pattern, and has deprecated the `%z` of Lua 5.1.
```
Run: `cargo test -p cmdstudio-core --test golden`
Expected: FAIL — `golden file terracotta.banner.th.lua is not what the generator makes. First difference, line 10` (and the same for `.en.lua`).

Run, in PowerShell: `$env:UPDATE_GOLDEN = '1'; cargo test -p cmdstudio-core --test golden; Remove-Item Env:\UPDATE_GOLDEN`, then `git diff -- crates/core/tests/golden/`.
Expected: in each of the two banner goldens exactly line 10 changes, from `  s = s:gsub("[%z\1-\31\127]", "")` to `  s = s:gsub("[\0\1-\31\127]", "")`. Then `cargo test -p cmdstudio-core --test banner_lua --test golden` passes: `exactly_the_control_and_direction_characters_are_dropped` runs the script in Lua 5.2 and still sees U+0000 dropped.

- [ ] **Step 9: Docs, and `publish = false`.** No behaviour changes; each is a comment a later reader needs.

In `crates/core/src/plan.rs`, in `Machine`, replace
```rust
    /// Current HKCU AutoRun value.
    pub autorun_hkcu: Option<String>,
    /// Current HKLM AutoRun value (read only; CMD Studio never writes HKLM).
    pub autorun_hklm: Option<String>,
```
with
```rust
    /// Current HKCU AutoRun value as the registry holds it: the raw text, never expanded, so a
    /// `%USERPROFILE%` in it stays as written. `None` when there is no value, or one that is
    /// not text.
    pub autorun_hkcu: Option<String>,
    /// Current HKLM AutoRun value, raw in the same way (read only; CMD Studio never writes
    /// HKLM).
    pub autorun_hklm: Option<String>,
```
in `Op`, put this doc above `WtApply {`:
```rust
    /// Writes the `CMD Studio` scheme and the cmd profile's keys into one settings.json. A key
    /// whose value is an object (`font`) is merged into the user's object key by key, so keys
    /// of the user's own in it, such as `font.weight`, stay.
```
and replace `    /// The complete new HKCU AutoRun value; \`None\` deletes it.` with
```rust
    /// The complete new HKCU AutoRun value, raw as the registry holds it (never expanded);
    /// `None` deletes it. Writing it keeps the value's registry type: a `REG_EXPAND_SZ` stays
    /// `REG_EXPAND_SZ`, and a value that was not there is written as `REG_SZ` (spec §5.4).
```
In `crates/core/src/plan/autorun.rs`, in the doc of `Scan::rejected`, replace `    /// single \`&\` last is fine, and so is a \`)\` that closes nothing, which cmd reads as text.` with
```rust
    /// single `&` last is fine, and so is a `)` that closes nothing, which cmd reads as text
    /// in a command (`echo a)`). Right after a block, as in `(echo a) )`, cmd refuses such a
    /// `)`, and this reading does not: it is one of the few values cmd rejects that pass here
    /// (ruling P68), and the doctor finds them from a real cmd window (D3).
```
In `crates/core/Cargo.toml`, after `license.workspace = true` add:
```toml
# Built into CMD Studio only. presets.rs embeds files from outside this crate's folder
# (`/presets`), which a published crate could not hold.
publish = false
```
Run: `cargo doc -p cmdstudio-core --no-deps` and `cargo test -p cmdstudio-core`.
Expected: the docs build with no warnings; every test passes.

- [ ] **Step 10: Commit**

Run `cargo fmt --all` and `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`. Message:
```text
fix(core): count only Clink's inject verb, and other 1a follow-ups

runs_clink now needs inject to be Clink's verb: the first word after
the launcher that is not an option (--profile <dir> is skipped), so
`type clink.bat | find "inject"` is no longer another Clink, while
`clink --profile <dir> inject` still is (ruling F4, tightened this way
because the strict next-word rule misses that form).

A run cut off before it changed anything says so on recovery.
banner.lua names NUL as \0 instead of Lua 5.1's %z (the goldens change
in that one line). Docs: AutoRun values are raw and keep their REG
type, WtApply merges font key by key, and Scan::rejected names the
`(echo a) )` case the doctor catches. The core crate is publish =
false: its presets come from outside the crate folder.

Phase 1a follow-ups (ruling F2).
```
Run:
```bash
git add -- crates/core/Cargo.toml crates/core/src/plan.rs crates/core/src/plan/autorun.rs crates/core/src/txn.rs crates/core/src/generate/lua.rs crates/core/tests/golden/terracotta.banner.en.lua crates/core/tests/golden/terracotta.banner.th.lua
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

- [ ] **Step 11: One copy of the Lua stand-ins for the integration tests.** `tests/banner_lua.rs` and `tests/hostile_themes.rs` each keep a copy of the same `PRELUDE`. Create `crates/core/tests/common/mod.rs`:
```rust
//! What the integration tests share. Each file under `tests/` is a crate of its own; a folder
//! under it is not, so `mod common;` brings this into the files that use it.

/// Stand-ins for the parts of Clink and Windows that `banner.lua` and `init.lua` use, so that
/// they run in plain Lua 5.2: `clink.print` collects what is printed in `__out` (marking a
/// print without a newline with `<NONL>`), `clink.oninject` keeps its callback in
/// `__oninject`, `console` counts cells and reads the screen lines from `__lines`,
/// `os.getenv` reads `__env` first and then gives a Thai user name and a computer name,
/// `os.getcwd` gives a Thai folder, and `log.info` does nothing.
pub const PRELUDE: &str = r#"
__out = {}
__env = {}
__lines = {}
NONL = {}
clink = {
  print = function(s, flag)
    if flag == NONL then s = "<NONL>" .. s end
    __out[#__out + 1] = s
  end,
  oninject = function(f) __oninject = f end,
}
console = {
  cellcount = function(s)
    local plain = s:gsub("\27%[[%d;]*m", "")
    local _, n = plain:gsub("[^\128-\191]", "")
    return n
  end,
  getlinetext = function(n) return __lines[n] end,
}
os.getenv = function(k)
  if __env[k] ~= nil then return __env[k] end
  if k == "USERNAME" then return "ทดสอบ" end
  if k == "COMPUTERNAME" then return "PC-01" end
  return nil
end
os.setenv = function(k, v) __env[k] = v end
os.getcwd = function() return "C:\\Users\\ทดสอบ" end
log = { info = function() end }
"#;
```
The string is the shipped `PRELUDE` byte for byte; only the doc is new. In `tests/banner_lua.rs` delete `const PRELUDE: &str = r#"` … `"#;` and in its place write:
```rust
mod common;
use common::PRELUDE;
```
In `tests/hostile_themes.rs` delete the doc comment that starts `/// Stand-ins for the parts of Clink and Windows that \`banner.lua\` uses` and the `const PRELUDE` under it, and add the same two lines after the `use std::path::{Path, PathBuf};` line.
Run: `cargo test -p cmdstudio-core --test banner_lua --test hostile_themes`
Expected: PASS (30 and 28 tests), with no `dead_code` warning.

- [ ] **Step 12: Pin the hue wrap and the palette's tint and cursor.** These tests pin behaviour that is already right, so they pass at once; each is checked against a temporary break so it is known to be able to fail.

In `crates/core/src/oklch.rs`'s tests, add before `fn round_trips_within_one_step_per_channel`:
```rust
    #[test]
    fn hue_is_always_from_0_up_to_360() {
        // atan2 gives -180 to 180 degrees; the hues below 0 are moved up by a full turn. Blue's
        // is one of them: 264.05 in CSS Color 4, -95.95 without the turn.
        let blue = Oklch::from_rgb(c("#0000ff"));
        assert!((blue.h - 264.05).abs() < 0.05, "{blue:?}");
        for r in (0..=255u8).step_by(17) {
            for g in (0..=255u8).step_by(17) {
                for b in (0..=255u8).step_by(17) {
                    let o = Oklch::from_rgb(Rgb { r, g, b });
                    assert!((0.0..360.0).contains(&o.h), "#{r:02x}{g:02x}{b:02x}: {o:?}");
                }
            }
        }
    }
```
In `crates/core/src/palette.rs`'s tests, add before `fn achromatic_accents_still_give_valid_palettes`:
```rust
    #[test]
    fn the_cursor_follows_the_accent_when_the_fix_moves_it() {
        // The readability fix may move the accent; the cursor is set to the accent after it, so
        // the cursor never keeps the color the fix took away.
        let p = generate(Tone::Light, c("#e3b341"), 0.0);
        assert_ne!(p.accent, c("#e3b341"), "the fix moved this accent");
        assert_eq!(p.cursor, p.accent);
    }

    #[test]
    fn a_coloured_accent_tints_the_neutrals_with_its_hue() {
        // Only an accent without chroma leaves the neutrals grey (ruling P23). A real one gives
        // them a little of its hue, the background as much as the tint asks for.
        let accent = c("#d97757");
        let hue = Oklch::from_rgb(accent).h;
        let p = generate(Tone::Dark, accent, 0.30);
        for (name, color) in [
            ("background", p.background),
            ("muted", p.muted),
            ("selection", p.selection),
        ] {
            let o = Oklch::from_rgb(color);
            assert!(o.c > 0.01, "{name} {color}: chroma {}", o.c);
            assert!(
                (o.h - hue).abs() < 15.0,
                "{name} {color}: hue {} for {hue}",
                o.h
            );
        }
        let untinted = generate(Tone::Dark, accent, 0.0);
        assert!(
            Oklch::from_rgb(untinted.background).c < 0.005,
            "no tint, no chroma in the background"
        );
    }
```
Run: `cargo test -p cmdstudio-core --lib -- oklch::tests palette::tests`
Expected: PASS (6 and 11 tests). Then break each pinned line for one run and put it back: delete `h += 360.0;` in `Oklch::from_rgb` → `hue_is_always_from_0_up_to_360` fails; delete `colors.cursor = colors.accent;` at the end of `palette::generate` → `the_cursor_follows_the_accent_when_the_fix_moves_it` fails; change `let achromatic = accent_lch.c < ACHROMATIC_BELOW;` to `let achromatic = true;` → `a_coloured_accent_tints_the_neutrals_with_its_hue` fails. Run `git diff -- crates/core/src/oklch.rs crates/core/src/palette.rs` afterwards: only the added tests show.

- [ ] **Step 13: Goldens for the saved theme file and for Clink's settings without transient.** In `crates/core/tests/golden.rs`, in `golden_tests!`, add after the `clink_settings:` row:
```rust
    clink_settings_without_transient: "terracotta.no-transient.clink_settings" =
        clink::settings(false, &under_data("generated"));
```
and after the `wt_profile_for_windows_terminal_only:` row:
```rust

    // the theme file itself, spec §3.3: what saving and exporting write
    saved_theme_file: "terracotta.cmdtheme" =
        String::from_utf8(terracotta().to_file_bytes().unwrap()).unwrap();
```
Run: `cargo test -p cmdstudio-core --test golden`
Expected: FAIL — `missing golden file terracotta.no-transient.clink_settings: run with UPDATE_GOLDEN=1 once` and the same for `terracotta.cmdtheme`.

Run, in PowerShell: `$env:UPDATE_GOLDEN = '1'; cargo test -p cmdstudio-core --test golden; Remove-Item Env:\UPDATE_GOLDEN`, then `git status --short crates/core/tests/golden/`.
Expected: the two new files and nothing else. Read them: `terracotta.no-transient.clink_settings` is `terracotta.clink_settings` with `prompt.transient = off`; `terracotta.cmdtheme` is the preset's content written pretty-printed with two-space indents (each array item on its own line) and a final newline. Then `cargo test -p cmdstudio-core --test golden` passes: `18 passed`.

- [ ] **Step 14: Hostile samples for NUL, DEL, C1 and the bidi isolates, and a `lua-breakers` line that can really get out.** Spec §3.7 #3 drops these characters from every text field; until now only `text::tests` pinned them, and no sample file carried them. The first `lua-breakers` line could not set the global even if the quoting were broken (the text lands inside a table constructor); the new line closes the string, the table and the function, and still leaves a script that parses.

In `tests/hostile_themes.rs`, in `sample_tests!`, add after the `bidi_name:` row:
```rust
    // NUL, DEL and C1 controls (among them U+009B and U+009D, the one-character CSI and OSC
    // some terminals take) in every text field
    c1_and_nul: "c1-and-nul" => Accepted;
    // the bidi isolates U+2066 to U+2069 in every text field
    bidi_isolates: "bidi-isolates" => Accepted;
```
add these tests before the line `// -- §3.7 #1 and #4: the text is data, and Lua keeps it as text --` that comes after the table:
```rust
#[test]
fn nul_del_and_c1_controls_are_cut_out_of_every_text_field() -> mlua::Result<()> {
    let theme = accepted("c1-and-nul");
    // U+009B is a whole CSI in some terminals: it goes, and the `31m` after it is only text
    assert_eq!(theme.name, "Calm31m Theme");
    assert_eq!(theme.author, "xy");
    assert_eq!(theme.terminal.font, "Cascadia Mono");
    assert_eq!(theme.banner.title, "0;pwnedHi {user}");
    assert_eq!(theme.banner.lines, ["31mred", "nul and DEL."]);
    for lang in [Lang::Th, Lang::En] {
        let rows = banner_rows(&theme, lang)?;
        assert!(
            rows[1].contains(&format!("0;pwnedHi {STAND_IN_USER}")),
            "{lang:?}: {}",
            rows[1]
        );
    }
    Ok(())
}

#[test]
fn bidi_isolates_are_cut_out_of_every_text_field() {
    let theme = accepted("bidi-isolates");
    assert_eq!(theme.name, "Calmtxt.exe");
    assert_eq!(theme.author, "ab");
    assert_eq!(theme.terminal.font, "Cascadia Mono");
    assert_eq!(theme.banner.title, "Hi {user}");
    assert_eq!(theme.banner.lines, ["cwd: {cwd}", "{date} - {time}"]);
}
```
in the file's doc, replace
```rust
//!    controls: `escape-in-name`, `escape-in-text-fields`, `newline-in-banner` and `bidi-name`.
```
with
```rust
//!    controls: `escape-in-name`, `escape-in-text-fields`, `newline-in-banner`, `bidi-name`,
//!    `c1-and-nul` (NUL, DEL and the C1 controls U+0080 to U+009F) and `bidi-isolates`
//!    (U+2066 to U+2069).
```
and in `text_that_breaks_a_lua_string_is_printed_as_it_is_and_never_runs` replace the first item of `lines`, `"\"); __pwned = true; --",`, with `"\")} end __pwned = true local function f() return {--",`, and add after `assert_eq!(theme.banner.lines, lines);`:
```rust

    // The first line is a real attack, so the check below can fail: put into a table of the
    // banner's shape between plain quotes, it closes the string, the table and the function,
    // sets the global as the script loads, and opens a function that the rest of the script
    // closes. The script still parses, so a quoting bug would not show as a load error.
    let naive = format!(
        "local function build()\n  return {{\n    (\"{}\"),\n  }}\nend\n",
        lines[0]
    );
    let lua = Lua::new();
    lua.load(naive.as_str()).exec()?;
    let pwned: LuaValue = lua.globals().get("__pwned")?;
    assert!(
        matches!(pwned, LuaValue::Boolean(true)),
        "the sample line no longer gets out of a plain string: {pwned:?}"
    );
```
Run: `cargo test -p cmdstudio-core --test hostile_themes`
Expected: FAIL — `c1_and_nul` and `bidi_isolates` (the sample files do not exist), the two new tests, and `text_that_breaks_a_lua_string_is_printed_as_it_is_and_never_runs` (the file still has the old line).

Then create the samples. Both are `tests/fixtures/terracotta-mine.cmdtheme` with five lines changed; copy that file and change only these lines. `tests/fixtures/hostile/c1-and-nul.cmdtheme`:
```json
  "name": "Calm\u0000\u007f\u0085\u009b31m Theme",
  "author": "\u0080x\u009fy",
```
```json
    "font": "Cascadia\u0000 Mono\u009d",
```
```json
    "title": "\u009d0;pwned\u009cHi {user}",
    "lines": ["\u009b31mred\u0000", "nul\u0000 and DEL\u007f."],
```
`tests/fixtures/hostile/bidi-isolates.cmdtheme` (written with `\u` escapes, so the file shows what it holds):
```json
  "name": "Calm\u2066txt\u2069.exe",
  "author": "\u2067a\u2068b\u2069",
```
```json
    "font": "Cascadia\u2066 Mono\u2069",
```
```json
    "title": "Hi \u2066{user}\u2069",
    "lines": ["\u2068cwd: {cwd}\u2069", "{date} \u2067-\u2069 {time}"],
```
In `tests/fixtures/hostile/lua-breakers.cmdtheme`, replace the line `      "\"); __pwned = true; --",` with:
```json
      "\")} end __pwned = true local function f() return {--",
```
Run: `cargo test -p cmdstudio-core --test hostile_themes`
Expected: PASS (32 tests).

- [ ] **Step 15: The whole crate**

Run: `cargo fmt --all -- --check`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo test -p cmdstudio-core`.
Expected: clean, and every test passes — 311 in all, 11 more than Phase 1a's 300: the library 217, `banner_lua` 30, `golden` 18, `hostile_themes` 32, `palette_props` 1, `schema_file` 11, doc tests 2.

- [ ] **Step 16: Commit**

Message:
```text
test(core): pin the hue wrap, the palette tint, two goldens and samples

Phase 1a follow-ups (ruling F2), tests only:
- hue is always 0 up to 360 (blue is the hue atan2 gives below 0);
- a coloured accent tints the neutrals, and the cursor follows an
  accent the readability fix moved (the P23 mutants);
- goldens for Theme::to_file_bytes and clink::settings(false, ..);
- hostile samples with NUL, DEL, C1 controls and the bidi isolates in
  every text field, and a lua-breakers line that really sets the
  global when it is quoted naively;
- tests/common holds the one PRELUDE that banner_lua.rs and
  hostile_themes.rs share.
```
Run:
```bash
git add -- crates/core/src/oklch.rs crates/core/src/palette.rs crates/core/tests/common/mod.rs crates/core/tests/banner_lua.rs crates/core/tests/hostile_themes.rs crates/core/tests/golden.rs crates/core/tests/golden/terracotta.cmdtheme crates/core/tests/golden/terracotta.no-transient.clink_settings crates/core/tests/fixtures/hostile/c1-and-nul.cmdtheme crates/core/tests/fixtures/hostile/bidi-isolates.cmdtheme crates/core/tests/fixtures/hostile/lua-breakers.cmdtheme
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 1: Adapter traits and in-memory fakes

**Files:**
- Create: `crates/core/src/adapters/mod.rs`, `crates/core/src/adapters/fake.rs`
- Modify: `crates/core/Cargo.toml` (a `fake` feature), `crates/core/src/lib.rs` (`pub mod adapters;` and the crate doc)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces (all in `cmdstudio_core::adapters`):
  - `AdapterError { context: String, message: String }` with `AdapterError::new(impl Into<String>, impl Into<String>)`; `Display` is `"{context}: {message}"`.
  - `DirEntry { path: PathBuf, is_dir: bool }`.
  - `trait Fs` (every method `&self`): `read(&Path) -> Result<Option<Vec<u8>>, AdapterError>`, `write_atomic(&Path, &[u8])`, `remove_file(&Path)`, `exists(&Path) -> bool`, `is_dir(&Path) -> bool`, `create_dir_all(&Path)`, `remove_dir(&Path)`, `remove_dir_all(&Path)`, `rename(from: &Path, to: &Path)`, `list_dir(&Path) -> Result<Vec<DirEntry>, AdapterError>`; the unit-returning ones return `Result<(), AdapterError>`.
  - `REG_SZ = 1`, `REG_EXPAND_SZ = 2`; `RegValue { kind: u32, bytes: Vec<u8> }` with `RegValue::from_text(kind, &str) -> RegValue` and `RegValue::text(&self) -> Option<String>`.
  - `trait AutoRun`: `user_key_exists() -> Result<bool, _>`, `read_user() -> Result<Option<RegValue>, _>`, `write_user(&RegValue)`, `delete_user()`, `delete_user_key_if_empty()`, `read_machine() -> Result<Option<RegValue>, _>`.
  - `trait Fonts`: `families() -> Result<Vec<String>, _>`, `user_fonts_dir() -> PathBuf`, `registration(&str) -> Result<Option<String>, _>`, `register(value_name: &str, file: &str)`, `unregister(&str)`, `registration_key_exists() -> Result<bool, _>`, `delete_registration_key_if_empty()`, `load(&Path)`, `unload(&Path)`. A new account may not have the registration key; undo deletes the one apply created (Task 11).
  - `WtVariant::{Store, Preview, Unpackaged}`, `WtLocation { variant, installed: bool, settings: PathBuf }`, `trait WtSettings { fn locations(&self) -> Vec<WtLocation>; }`.
  - `Output { status: i32, stdout: String, stderr: String }`, `trait Process { fn run(&self, exe: &Path, args: &[&OsStr]) -> Result<Output, AdapterError>; }`.
  - `DirState::{Absent, Private, Unsafe(String)}`, `Created::{New, AlreadyExisted}`, `Arch::{X64, Arm64, Other}`, `DefaultTerminal::{LetWindowsDecide, WindowsTerminal, WindowsTerminalPreview, Conhost, Other}`, `SystemInfo { build: Option<u32>, arch, default_terminal, webview2: Option<String>, packaged: bool, ui_language: u16 }`.
  - `trait Host`: `local_app_data() -> Result<PathBuf, _>`, `program_data() -> Result<PathBuf, _>`, `user_sid() -> Result<String, _>`, `short_path(&Path) -> Result<PathBuf, _>`, `private_dir_state(&Path) -> Result<DirState, _>`, `create_private_dir(&Path) -> Result<Created, _>`, `system_info() -> SystemInfo`.
  - `System<'a> { fs: &'a dyn Fs, autorun: &'a dyn AutoRun, fonts: &'a dyn Fonts, wt: &'a dyn WtSettings, process: &'a dyn Process, host: &'a dyn Host }` (`Clone + Copy`).
  - `program_data_dir(program_data: &Path, sid: &str) -> PathBuf`.
  - `adapters::fake` (under `cfg(any(test, feature = "fake"))`): `FakeFs` (`new`, `put`, `file`, `snapshot`, `fail_writes_to`, `fail_removes_of`), `FakeAutoRun` (`new`, `set_machine`, `set_key_exists`, `fail_writes`, `snapshot`), `FakeFonts` (`new(dir, &[&str])`, `knows_file`, `set_key_exists`, `snapshot() -> (registrations, loaded files, key exists)`), `FakeWt { pub locations }`, `FakeProcess` (`new`, `answer(exe_name, first_arg, status, stdout)`, `calls() -> Vec<Call>`), `Call { exe, args: Vec<String> }`, `FakeHost` (pub fields `local_app_data`, `program_data`, `sid`, `short`, `info`; `set_dir_state`, `dir_state`), `FakeMachine` (pub fields `fs`, `autorun`, `fonts`, `wt`, `process`, `host`; `new()`, `system() -> System<'_>`).

Why `&self` everywhere: `txn::run` holds the executor, the verifier and the journal store at the same time, and all three need the same `Fs`. Shared references plus interior mutability in the fakes avoid three `&mut` borrows of one adapter. The Windows adapters are stateless.

- [ ] **Step 1: Add the `fake` feature and the module**

In `crates/core/Cargo.toml`, add after `[lints]`:
```toml
[features]
# In-memory adapters for tests in other crates (crates/cli). Never enabled in a release build.
fake = []
```
In `crates/core/src/lib.rs`, replace the crate doc comment (its three `//!` lines, which end "Nothing here touches the machine directly; side effects go through the traits in `txn`.") with:
```rust
//! CMD Studio core: theme model, palette, generators, planner, the transactional apply
//! engine, and what runs it on a real machine. Only `adapters::windows` touches the machine;
//! everything else goes through the adapter traits in `adapters`.
```
and add `pub mod adapters;` to the module list (keep the list alphabetical). Later tasks only add modules to that list.

- [ ] **Step 2: Write the failing tests**

Create `crates/core/src/adapters/mod.rs` with only:
```rust
#[cfg(any(test, feature = "fake"))]
pub mod fake;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_text_round_trips_thai_and_stops_at_the_first_nul() {
        let v = RegValue::from_text(REG_SZ, "if exist \"C:\\ผู้ใช้\\x.exe\" x");
        assert_eq!(v.text().as_deref(), Some("if exist \"C:\\ผู้ใช้\\x.exe\" x"));
        assert_eq!(v.bytes.len() % 2, 0);
        assert_eq!(&v.bytes[v.bytes.len() - 2..], &[0, 0]);
        let mut odd = RegValue::from_text(REG_EXPAND_SZ, "%USERPROFILE%\\a");
        odd.bytes.extend_from_slice(&[0x41, 0x00, 0x00, 0x00]);
        assert_eq!(odd.text().as_deref(), Some("%USERPROFILE%\\a"));
        assert_eq!(RegValue { kind: 4, bytes: vec![1, 0, 0, 0] }.text(), None);
        assert_eq!(RegValue { kind: REG_SZ, bytes: vec![0x00, 0xD8, 0, 0] }.text(), None);
    }

    #[test]
    fn program_data_dir_has_no_shared_parent() {
        let d = program_data_dir(Path::new(r"C:\ProgramData"), "S-1-5-21-1-2-3-1001");
        assert_eq!(d, PathBuf::from(r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001"));
    }
}
```
Create `crates/core/src/adapters/fake.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::REG_SZ;

    #[test]
    fn fake_fs_behaves_like_a_disk() {
        let fs = FakeFs::new();
        let f = Path::new(r"C:\d\sub\a.txt");
        fs.write_atomic(f, b"one").unwrap();
        assert!(fs.is_dir(Path::new(r"C:\d\sub")));
        assert_eq!(fs.read(f).unwrap(), Some(b"one".to_vec()));
        assert!(fs.remove_dir(Path::new(r"C:\d\sub")).is_err());
        fs.rename(Path::new(r"C:\d\sub"), Path::new(r"C:\d\moved")).unwrap();
        assert_eq!(fs.read(Path::new(r"C:\d\moved\a.txt")).unwrap(), Some(b"one".to_vec()));
        assert_eq!(fs.read(f).unwrap(), None);
        assert_eq!(
            fs.list_dir(Path::new(r"C:\d")).unwrap(),
            vec![DirEntry { path: PathBuf::from(r"C:\d\moved"), is_dir: true }]
        );
        fs.remove_dir_all(Path::new(r"C:\d")).unwrap();
        assert!(!fs.exists(Path::new(r"C:\d\moved\a.txt")));
        fs.remove_file(f).unwrap();
        fs.fail_writes_to(f);
        assert!(fs.write_atomic(f, b"x").is_err());
    }

    #[test]
    fn fake_autorun_keeps_kind_and_bytes() {
        let a = FakeAutoRun::new();
        let v = RegValue { kind: 2, bytes: vec![0x41, 0, 0, 0, 0x42, 0] };
        a.write_user(&v).unwrap();
        assert_eq!(a.read_user().unwrap(), Some(v));
        a.delete_user().unwrap();
        a.delete_user_key_if_empty().unwrap();
        assert_eq!(a.snapshot(), (false, None));
        a.write_user(&RegValue::from_text(REG_SZ, "x")).unwrap();
        assert!(a.user_key_exists().unwrap());
    }

    #[test]
    fn fake_fonts_report_loaded_files_and_the_registration_key() {
        let f = FakeFonts::new(r"C:\fonts", &["Arial"]);
        f.knows_file("n.ttf", "CaskaydiaCove NF");
        f.load(Path::new(r"C:\fonts\n.ttf")).unwrap();
        assert_eq!(f.families().unwrap(), vec!["Arial".to_string(), "CaskaydiaCove NF".to_string()]);
        f.unload(Path::new(r"C:\fonts\n.ttf")).unwrap();
        assert_eq!(f.families().unwrap(), vec!["Arial".to_string()]);

        f.set_key_exists(false);
        f.register("N (TrueType)", r"C:\fonts\n.ttf").unwrap();
        assert!(f.registration_key_exists().unwrap());
        f.delete_registration_key_if_empty().unwrap();
        assert!(f.registration_key_exists().unwrap(), "a key with values stays");
        f.unregister("N (TrueType)").unwrap();
        f.delete_registration_key_if_empty().unwrap();
        assert!(!f.registration_key_exists().unwrap());
    }

    #[test]
    fn fake_process_answers_by_exe_and_first_argument() {
        let p = FakeProcess::new();
        p.answer("oh-my-posh.exe", "version", 0, "31.5.0\n");
        let out = p.run(Path::new(r"C:\r\oh-my-posh.exe"), &[OsStr::new("version")]).unwrap();
        assert_eq!(out.stdout, "31.5.0\n");
        assert_eq!(p.calls().len(), 1);
    }
}
```

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- adapters::tests adapters::fake`
Expected: compile errors (`cannot find type RegValue`, `FakeFs`, …).

- [ ] **Step 4: Implement the traits** — in `crates/core/src/adapters/mod.rs`, put this above `#[cfg(any(test, feature = "fake"))] pub mod fake;` (Task 3 adds `#[cfg(windows)] pub mod windows;` next to it):
```rust
//! Machine adapters (spec §5.2 `adapters`): the only layer that touches the
//! machine. Every method takes `&self`, so the executor, the verifier and the
//! journal store can share one adapter; the fakes use interior mutability.
//! Each Windows adapter takes its locations as parameters, so tests point it
//! at a throwaway registry key and temp folders (Phase 1b test safety).
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// A failure inside an adapter. Callers map it to a spec §5.9 code, because
/// only they know which step failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{context}: {message}")]
pub struct AdapterError {
    pub context: String,
    pub message: String,
}

impl AdapterError {
    pub fn new(context: impl Into<String>, message: impl Into<String>) -> Self {
        AdapterError { context: context.into(), message: message.into() }
    }
}

/// One entry of a folder listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Files and folders (spec §5.4 `Fs`).
pub trait Fs {
    /// The file's bytes; `None` when it does not exist.
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, AdapterError>;
    /// Replaces the file in one step: a temp file in the same folder, flushed,
    /// then renamed over the target. Creates missing parent folders. When the
    /// target is a symbolic link, the link's target is written and the link stays.
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), AdapterError>;
    /// Deletes a file; `Ok` when it is already gone.
    fn remove_file(&self, path: &Path) -> Result<(), AdapterError>;
    fn exists(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn create_dir_all(&self, path: &Path) -> Result<(), AdapterError>;
    /// Deletes an empty folder; `Ok` when it is gone; an error when it is not empty.
    fn remove_dir(&self, path: &Path) -> Result<(), AdapterError>;
    /// Deletes a folder and everything in it; `Ok` when it is already gone.
    fn remove_dir_all(&self, path: &Path) -> Result<(), AdapterError>;
    /// Renames a file or folder; `to` must not exist.
    fn rename(&self, from: &Path, to: &Path) -> Result<(), AdapterError>;
    /// The entries directly inside a folder, sorted by path; empty when the folder does not exist.
    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, AdapterError>;
}

/// `REG_SZ`.
pub const REG_SZ: u32 = 1;
/// `REG_EXPAND_SZ`.
pub const REG_EXPAND_SZ: u32 = 2;

/// A registry value exactly as stored: its type and its raw bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegValue {
    pub kind: u32,
    pub bytes: Vec<u8>,
}

impl RegValue {
    /// UTF-16LE text plus one NUL, the way Windows stores `REG_SZ`.
    pub fn from_text(kind: u32, text: &str) -> RegValue {
        let mut bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        bytes.extend_from_slice(&[0, 0]);
        RegValue { kind, bytes }
    }

    /// The text of a `REG_SZ` or `REG_EXPAND_SZ` value, up to its first NUL,
    /// unexpanded. `None` for any other type or for bytes that are not UTF-16.
    pub fn text(&self) -> Option<String> {
        if self.kind != REG_SZ && self.kind != REG_EXPAND_SZ {
            return None;
        }
        let (pairs, _) = self.bytes.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|p| u16::from_le_bytes(*p)).take_while(|u| *u != 0).collect();
        String::from_utf16(&units).ok()
    }
}

/// cmd's AutoRun value under `Command Processor` (spec §5.4 AutoRun).
pub trait AutoRun {
    /// Whether the user's `Command Processor` key exists.
    fn user_key_exists(&self) -> Result<bool, AdapterError>;
    /// The user's AutoRun value as stored; `None` when absent.
    fn read_user(&self) -> Result<Option<RegValue>, AdapterError>;
    /// Writes the user's AutoRun value exactly as given. Opens the existing key
    /// and creates it only when missing; never recreates it (spec §2.2 lesson 9).
    fn write_user(&self, value: &RegValue) -> Result<(), AdapterError>;
    /// Deletes the user's AutoRun value; `Ok` when it is absent.
    fn delete_user(&self) -> Result<(), AdapterError>;
    /// Deletes the user's `Command Processor` key only when it holds no values and no subkeys.
    fn delete_user_key_if_empty(&self) -> Result<(), AdapterError>;
    /// The machine-wide (HKLM) AutoRun value. Read only: CMD Studio never writes HKLM.
    fn read_machine(&self) -> Result<Option<RegValue>, AdapterError>;
}

/// Per-user fonts (spec §5.4 `Fonts`, G12).
pub trait Fonts {
    /// Family names GDI lists in this session (`EnumFontFamiliesExW`), sorted, without duplicates.
    fn families(&self) -> Result<Vec<String>, AdapterError>;
    /// `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, or the test's folder.
    fn user_fonts_dir(&self) -> PathBuf;
    /// Data of a per-user font registration value (the font file's path); `None` when absent.
    fn registration(&self, value_name: &str) -> Result<Option<String>, AdapterError>;
    fn register(&self, value_name: &str, file: &str) -> Result<(), AdapterError>;
    /// Deletes the registration value; `Ok` when it is absent.
    fn unregister(&self, value_name: &str) -> Result<(), AdapterError>;
    /// Whether the registration key exists; a new account may not have it yet.
    fn registration_key_exists(&self) -> Result<bool, AdapterError>;
    /// Deletes the registration key only when it holds no values and no subkeys.
    fn delete_registration_key_if_empty(&self) -> Result<(), AdapterError>;
    /// Makes the font usable now: `AddFontResourceW`, then broadcast `WM_FONTCHANGE`.
    fn load(&self, file: &Path) -> Result<(), AdapterError>;
    /// `RemoveFontResourceW`, then broadcast; `Ok` when it was not loaded.
    fn unload(&self, file: &Path) -> Result<(), AdapterError>;
}

/// The Windows Terminal variants spec §5.4 lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WtVariant {
    Store,
    Preview,
    Unpackaged,
}

/// Where one Windows Terminal variant keeps its settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WtLocation {
    pub variant: WtVariant,
    /// The variant's own folder exists.
    pub installed: bool,
    pub settings: PathBuf,
}

/// Windows Terminal's settings files (spec §5.4 `WtSettings`). Reading and
/// writing go through `Fs`; editing is `wt_edit`.
pub trait WtSettings {
    fn locations(&self) -> Vec<WtLocation>;
}

/// What a finished process printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Runs CMD Studio's own bundled programs with fixed arguments: no shell,
/// no window, a time limit (spec §5.10 last bullet).
pub trait Process {
    fn run(&self, exe: &Path, args: &[&OsStr]) -> Result<Output, AdapterError>;
}

/// Who can change a folder that must belong to this user alone (spec §5.4 step 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirState {
    Absent,
    /// Owned by the user, SYSTEM or Administrators, and nobody else may write, append or delete.
    Private,
    /// Anything else, with the reason.
    Unsafe(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Created {
    New,
    AlreadyExisted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Arch {
    X64,
    Arm64,
    Other,
}

/// Windows 11's "default terminal application" (spec §5.8 D11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultTerminal {
    /// Not set: Windows decides (Windows Terminal on Windows 11 22H2 and later).
    LetWindowsDecide,
    WindowsTerminal,
    WindowsTerminalPreview,
    Conhost,
    Other,
}

/// Facts for the wizard's machine check and for doctor (spec §4.2 page 2, §5.8).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    /// `CurrentBuildNumber`; `None` when it could not be read.
    pub build: Option<u32>,
    pub arch: Arch,
    pub default_terminal: DefaultTerminal,
    /// WebView2 Runtime version (`pv`), for Phase 4.
    pub webview2: Option<String>,
    /// The app runs inside an MSIX package (spec §2.2 lesson 1, D10).
    pub packaged: bool,
    /// `GetUserDefaultUILanguage`; Thai is 0x041E.
    pub ui_language: u16,
}

/// Identity, known folders, 8.3 names and the private ProgramData folder.
pub trait Host {
    /// `FOLDERID_LocalAppData`.
    fn local_app_data(&self) -> Result<PathBuf, AdapterError>;
    /// `FOLDERID_ProgramData`.
    fn program_data(&self) -> Result<PathBuf, AdapterError>;
    /// The current user's SID as text, such as `S-1-5-21-1-2-3-1001`.
    fn user_sid(&self) -> Result<String, AdapterError>;
    /// `GetShortPathNameW` of an existing path. On a volume that makes no 8.3
    /// names it returns the long path itself (G19).
    fn short_path(&self, path: &Path) -> Result<PathBuf, AdapterError>;
    fn private_dir_state(&self, dir: &Path) -> Result<DirState, AdapterError>;
    /// Creates the folder and its protected security descriptor in one call:
    /// `D:P(A;OICI;FA;;;<user SID>)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)`.
    fn create_private_dir(&self, dir: &Path) -> Result<Created, AdapterError>;
    fn system_info(&self) -> SystemInfo;
}

/// Everything that touches the machine, borrowed for one command.
#[derive(Clone, Copy)]
pub struct System<'a> {
    pub fs: &'a dyn Fs,
    pub autorun: &'a dyn AutoRun,
    pub fonts: &'a dyn Fonts,
    pub wt: &'a dyn WtSettings,
    pub process: &'a dyn Process,
    pub host: &'a dyn Host,
}

/// `%ProgramData%\ai.resincore.cmdstudio-<SID>`: the user's own folder for
/// Clink when no ASCII path under the data folder exists (spec §5.4 step 3).
pub fn program_data_dir(program_data: &Path, sid: &str) -> PathBuf {
    program_data.join(format!("ai.resincore.cmdstudio-{sid}"))
}
```

- [ ] **Step 5: Implement the fakes** — in `crates/core/src/adapters/fake.rs`, above the tests:
```rust
//! In-memory adapters for tests (this crate's and, through the `fake`
//! feature, crates/cli's). They never touch the machine.
use super::{
    AdapterError, Arch, AutoRun, Created, DefaultTerminal, DirEntry, DirState, Fonts, Fs, Host, Output, Process,
    RegValue, System, SystemInfo, WtLocation, WtSettings,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

fn err(context: &Path, message: &str) -> AdapterError {
    AdapterError::new(context.display().to_string(), message)
}

#[derive(Default)]
struct FsState {
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeSet<PathBuf>,
    fail_writes: BTreeSet<PathBuf>,
    fail_removes: BTreeSet<PathBuf>,
}

/// Files and folders in memory. Paths compare exactly (case-sensitive).
#[derive(Default)]
pub struct FakeFs {
    state: RefCell<FsState>,
}

impl FakeFs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Puts a file in place (and its parent folders), bypassing failure injection.
    pub fn put(&self, path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
        let mut s = self.state.borrow_mut();
        add_parents(&mut s.dirs, path.as_ref());
        s.files.insert(path.as_ref().to_path_buf(), bytes.as_ref().to_vec());
    }

    pub fn file(&self, path: impl AsRef<Path>) -> Option<Vec<u8>> {
        self.state.borrow().files.get(path.as_ref()).cloned()
    }

    /// Every file and folder, for byte-for-byte comparisons of the whole fake disk.
    pub fn snapshot(&self) -> (BTreeMap<PathBuf, Vec<u8>>, BTreeSet<PathBuf>) {
        let s = self.state.borrow();
        (s.files.clone(), s.dirs.clone())
    }

    /// Makes every later write to `path` fail.
    pub fn fail_writes_to(&self, path: impl AsRef<Path>) {
        self.state.borrow_mut().fail_writes.insert(path.as_ref().to_path_buf());
    }

    /// Makes every later removal of `path` (file or folder) fail, as when a file is in use.
    pub fn fail_removes_of(&self, path: impl AsRef<Path>) {
        self.state.borrow_mut().fail_removes.insert(path.as_ref().to_path_buf());
    }
}

fn add_parents(dirs: &mut BTreeSet<PathBuf>, path: &Path) {
    for a in path.ancestors().skip(1) {
        if !a.as_os_str().is_empty() {
            dirs.insert(a.to_path_buf());
        }
    }
}

impl Fs for FakeFs {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, AdapterError> {
        Ok(self.state.borrow().files.get(path).cloned())
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.fail_writes.contains(path) {
            return Err(err(path, "write failed (injected)"));
        }
        if s.dirs.contains(path) {
            return Err(err(path, "is a folder"));
        }
        add_parents(&mut s.dirs, path);
        s.files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.fail_removes.contains(path) {
            return Err(err(path, "file is in use (injected)"));
        }
        s.files.remove(path);
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        let s = self.state.borrow();
        s.files.contains_key(path) || s.dirs.contains(path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.state.borrow().dirs.contains(path)
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.files.contains_key(path) {
            return Err(err(path, "is a file"));
        }
        add_parents(&mut s.dirs, path);
        s.dirs.insert(path.to_path_buf());
        Ok(())
    }

    fn remove_dir(&self, path: &Path) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        let busy = s.files.keys().any(|f| f.parent() == Some(path)) || s.dirs.iter().any(|d| d.parent() == Some(path));
        if busy {
            return Err(err(path, "folder is not empty"));
        }
        s.dirs.remove(path);
        Ok(())
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.fail_removes.iter().any(|f| f.starts_with(path)) {
            return Err(err(path, "a file inside is in use (injected)"));
        }
        s.files.retain(|f, _| !f.starts_with(path));
        s.dirs.retain(|d| !d.starts_with(path));
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.files.contains_key(to) || s.dirs.contains(to) {
            return Err(err(to, "already exists"));
        }
        if s.fail_removes.iter().any(|f| f.starts_with(from)) {
            return Err(err(from, "in use (injected)"));
        }
        let moved = |p: &Path| p.strip_prefix(from).ok().map(|rest| to.join(rest));
        let files: Vec<(PathBuf, Vec<u8>)> =
            s.files.iter().filter_map(|(p, b)| moved(p).map(|n| (n, b.clone()))).collect();
        let dirs: Vec<PathBuf> = s.dirs.iter().filter_map(|p| moved(p)).collect();
        if files.is_empty() && dirs.is_empty() {
            return Err(err(from, "not found"));
        }
        s.files.retain(|p, _| !p.starts_with(from));
        s.dirs.retain(|p| !p.starts_with(from));
        add_parents(&mut s.dirs, to);
        s.files.extend(files);
        s.dirs.extend(dirs);
        Ok(())
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, AdapterError> {
        let s = self.state.borrow();
        let mut out: Vec<DirEntry> = s
            .files
            .keys()
            .filter(|f| f.parent() == Some(path))
            .map(|f| DirEntry { path: f.clone(), is_dir: false })
            .chain(
                s.dirs.iter().filter(|d| d.parent() == Some(path)).map(|d| DirEntry { path: d.clone(), is_dir: true }),
            )
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }
}

#[derive(Default)]
struct AutoRunState {
    key: bool,
    user: Option<RegValue>,
    machine: Option<RegValue>,
    fail_writes: bool,
}

/// The `Command Processor` keys in memory. The user key exists by default.
pub struct FakeAutoRun {
    state: RefCell<AutoRunState>,
}

impl Default for FakeAutoRun {
    fn default() -> Self {
        FakeAutoRun { state: RefCell::new(AutoRunState { key: true, ..AutoRunState::default() }) }
    }
}

impl FakeAutoRun {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_machine(&self, value: Option<RegValue>) {
        self.state.borrow_mut().machine = value;
    }
    pub fn set_key_exists(&self, exists: bool) {
        self.state.borrow_mut().key = exists;
    }
    pub fn fail_writes(&self) {
        self.state.borrow_mut().fail_writes = true;
    }
    /// `(key exists, user value)`, for byte-for-byte comparisons.
    pub fn snapshot(&self) -> (bool, Option<RegValue>) {
        let s = self.state.borrow();
        (s.key, s.user.clone())
    }
}

impl AutoRun for FakeAutoRun {
    fn user_key_exists(&self) -> Result<bool, AdapterError> {
        Ok(self.state.borrow().key)
    }
    fn read_user(&self) -> Result<Option<RegValue>, AdapterError> {
        Ok(self.state.borrow().user.clone())
    }
    fn write_user(&self, value: &RegValue) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.fail_writes {
            return Err(AdapterError::new("AutoRun", "access denied (injected)"));
        }
        s.key = true;
        s.user = Some(value.clone());
        Ok(())
    }
    fn delete_user(&self) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.fail_writes {
            return Err(AdapterError::new("AutoRun", "access denied (injected)"));
        }
        s.user = None;
        Ok(())
    }
    fn delete_user_key_if_empty(&self) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.user.is_none() {
            s.key = false;
        }
        Ok(())
    }
    fn read_machine(&self) -> Result<Option<RegValue>, AdapterError> {
        Ok(self.state.borrow().machine.clone())
    }
}

#[derive(Default)]
struct FontsState {
    families: BTreeSet<String>,
    registry: BTreeMap<String, String>,
    loaded: BTreeSet<PathBuf>,
    /// File name → the family GDI would report once the file is loaded.
    file_families: BTreeMap<String, String>,
    /// The registration key is absent, as on an account that never had a per-user font.
    key_missing: bool,
}

/// Per-user fonts in memory.
pub struct FakeFonts {
    dir: PathBuf,
    state: RefCell<FontsState>,
}

impl FakeFonts {
    pub fn new(dir: impl Into<PathBuf>, families: &[&str]) -> Self {
        let state =
            FontsState { families: families.iter().map(|f| (*f).to_string()).collect(), ..FontsState::default() };
        FakeFonts { dir: dir.into(), state: RefCell::new(state) }
    }
    /// Declares which family a font file holds.
    pub fn knows_file(&self, file_name: &str, family: &str) {
        self.state.borrow_mut().file_families.insert(file_name.to_string(), family.to_string());
    }
    pub fn set_key_exists(&self, exists: bool) {
        self.state.borrow_mut().key_missing = !exists;
    }
    /// `(registry values, loaded files, whether the registration key exists)`, for comparisons.
    pub fn snapshot(&self) -> (BTreeMap<String, String>, BTreeSet<PathBuf>, bool) {
        let s = self.state.borrow();
        (s.registry.clone(), s.loaded.clone(), !s.key_missing)
    }
}

impl Fonts for FakeFonts {
    fn families(&self) -> Result<Vec<String>, AdapterError> {
        let s = self.state.borrow();
        let mut all = s.families.clone();
        for file in &s.loaded {
            let name = file.file_name().and_then(OsStr::to_str).unwrap_or_default();
            if let Some(family) = s.file_families.get(name) {
                all.insert(family.clone());
            }
        }
        Ok(all.into_iter().collect())
    }
    fn user_fonts_dir(&self) -> PathBuf {
        self.dir.clone()
    }
    fn registration(&self, value_name: &str) -> Result<Option<String>, AdapterError> {
        Ok(self.state.borrow().registry.get(value_name).cloned())
    }
    fn register(&self, value_name: &str, file: &str) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        s.key_missing = false;
        s.registry.insert(value_name.to_string(), file.to_string());
        Ok(())
    }
    fn unregister(&self, value_name: &str) -> Result<(), AdapterError> {
        self.state.borrow_mut().registry.remove(value_name);
        Ok(())
    }
    fn registration_key_exists(&self) -> Result<bool, AdapterError> {
        Ok(!self.state.borrow().key_missing)
    }
    fn delete_registration_key_if_empty(&self) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.registry.is_empty() {
            s.key_missing = true;
        }
        Ok(())
    }
    fn load(&self, file: &Path) -> Result<(), AdapterError> {
        self.state.borrow_mut().loaded.insert(file.to_path_buf());
        Ok(())
    }
    fn unload(&self, file: &Path) -> Result<(), AdapterError> {
        self.state.borrow_mut().loaded.remove(file);
        Ok(())
    }
}

/// Windows Terminal locations given by the test.
#[derive(Default)]
pub struct FakeWt {
    pub locations: Vec<WtLocation>,
}

impl WtSettings for FakeWt {
    fn locations(&self) -> Vec<WtLocation> {
        self.locations.clone()
    }
}

/// One recorded call to `FakeProcess::run`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub exe: PathBuf,
    pub args: Vec<String>,
}

#[derive(Default)]
struct ProcessState {
    calls: Vec<Call>,
    /// (exe file name, first argument) → output.
    answers: BTreeMap<(String, String), Output>,
}

/// Answers each call from a table keyed by the exe's file name and the first
/// argument. Unknown calls succeed with empty output.
#[derive(Default)]
pub struct FakeProcess {
    state: RefCell<ProcessState>,
}

impl FakeProcess {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn answer(&self, exe_name: &str, first_arg: &str, status: i32, stdout: &str) {
        let out = Output { status, stdout: stdout.to_string(), stderr: String::new() };
        self.state.borrow_mut().answers.insert((exe_name.to_string(), first_arg.to_string()), out);
    }
    pub fn calls(&self) -> Vec<Call> {
        self.state.borrow().calls.clone()
    }
}

impl Process for FakeProcess {
    fn run(&self, exe: &Path, args: &[&OsStr]) -> Result<Output, AdapterError> {
        let args: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let name = exe.file_name().and_then(OsStr::to_str).unwrap_or_default().to_string();
        let first = args.first().cloned().unwrap_or_default();
        let mut s = self.state.borrow_mut();
        s.calls.push(Call { exe: exe.to_path_buf(), args });
        Ok(s.answers.get(&(name, first)).cloned().unwrap_or(Output {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        }))
    }
}

/// Identity and system facts given by the test.
pub struct FakeHost {
    pub local_app_data: PathBuf,
    pub program_data: PathBuf,
    pub sid: String,
    /// Long path → what `GetShortPathNameW` returns. Paths not listed come back unchanged.
    pub short: BTreeMap<PathBuf, PathBuf>,
    pub info: SystemInfo,
    private: RefCell<BTreeMap<PathBuf, DirState>>,
}

impl Default for FakeHost {
    fn default() -> Self {
        FakeHost {
            local_app_data: PathBuf::from(r"C:\Users\tester\AppData\Local"),
            program_data: PathBuf::from(r"C:\ProgramData"),
            sid: "S-1-5-21-1-2-3-1001".to_string(),
            short: BTreeMap::new(),
            info: SystemInfo {
                build: Some(26100),
                arch: Arch::X64,
                default_terminal: DefaultTerminal::LetWindowsDecide,
                webview2: None,
                packaged: false,
                ui_language: 0x0409,
            },
            private: RefCell::new(BTreeMap::new()),
        }
    }
}

impl FakeHost {
    pub fn set_dir_state(&self, dir: impl Into<PathBuf>, state: DirState) {
        self.private.borrow_mut().insert(dir.into(), state);
    }
    pub fn dir_state(&self, dir: &Path) -> DirState {
        self.private.borrow().get(dir).cloned().unwrap_or(DirState::Absent)
    }
}

impl Host for FakeHost {
    fn local_app_data(&self) -> Result<PathBuf, AdapterError> {
        Ok(self.local_app_data.clone())
    }
    fn program_data(&self) -> Result<PathBuf, AdapterError> {
        Ok(self.program_data.clone())
    }
    fn user_sid(&self) -> Result<String, AdapterError> {
        Ok(self.sid.clone())
    }
    fn short_path(&self, path: &Path) -> Result<PathBuf, AdapterError> {
        Ok(self.short.get(path).cloned().unwrap_or_else(|| path.to_path_buf()))
    }
    fn private_dir_state(&self, dir: &Path) -> Result<DirState, AdapterError> {
        Ok(self.dir_state(dir))
    }
    fn create_private_dir(&self, dir: &Path) -> Result<Created, AdapterError> {
        let mut map = self.private.borrow_mut();
        if map.get(dir).is_some_and(|s| *s != DirState::Absent) {
            return Ok(Created::AlreadyExisted);
        }
        map.insert(dir.to_path_buf(), DirState::Private);
        Ok(Created::New)
    }
    fn system_info(&self) -> SystemInfo {
        self.info.clone()
    }
}

/// One of each fake, for tests that need a whole `System`.
pub struct FakeMachine {
    pub fs: FakeFs,
    pub autorun: FakeAutoRun,
    pub fonts: FakeFonts,
    pub wt: FakeWt,
    pub process: FakeProcess,
    pub host: FakeHost,
}

impl FakeMachine {
    pub fn new() -> Self {
        FakeMachine {
            fs: FakeFs::new(),
            autorun: FakeAutoRun::new(),
            fonts: FakeFonts::new(
                r"C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts",
                &["Arial", "Cascadia Mono"],
            ),
            wt: FakeWt::default(),
            process: FakeProcess::new(),
            host: FakeHost::default(),
        }
    }

    pub fn system(&self) -> System<'_> {
        System {
            fs: &self.fs,
            autorun: &self.autorun,
            fonts: &self.fonts,
            wt: &self.wt,
            process: &self.process,
            host: &self.host,
        }
    }
}

impl Default for FakeMachine {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 6: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- adapters::tests adapters::fake`
Expected: `6 passed`.

- [ ] **Step 7: Format and lint**

Run: `cargo fmt --all`, then `cargo clippy -p cmdstudio-core --all-targets -- -D warnings` and `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: `Finished` with no warnings for both. The second run compiles `fake.rs` as production code, so it must be lint-clean too.

- [ ] **Step 8: Commit**

Message:
```text
feat(core): add the adapter traits and in-memory fakes

Six traits are the only way core code reaches the machine: Fs, AutoRun,
Fonts, WtSettings, Process and Host. Every method takes &self so the
txn executor, verifier and journal store can share one adapter. The
fakes keep their state behind RefCell and are available to other
crates through the `fake` feature.
```
Run:
```bash
git add -- crates/core/Cargo.toml crates/core/src/lib.rs crates/core/src/adapters/mod.rs crates/core/src/adapters/fake.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 2: Bundled runtimes — the manifest model and `scripts/fetch-resources.ps1`

**Files:**
- Create: `crates/core/src/resources.rs`, `scripts/fetch-resources.ps1`
- Modify: `crates/core/Cargo.toml` (`sha2`), `crates/core/src/lib.rs` (`pub mod resources;`), `.gitignore` (`/resources/`), `Cargo.lock`

**Interfaces:**
- Consumes: `adapters::{Fs, fake::FakeFs}` (Task 1); `plan::{Component, OMP_VERSION, CLINK_VERSION, NERD_FONT_FAMILY}` (Phase 1a).
- Produces (in `cmdstudio_core::resources`):
  - `MANIFEST = "manifest.json"`, `NERD_FONT_FILE = "CaskaydiaCoveNerdFont-Regular.ttf"`, `NERD_FONTS_VERSION = "3.5.1"`.
  - `Bundle { version: String, files: BTreeMap<String, String> }` (file name → lowercase SHA-256), `Manifest { omp, clink, fonts: Bundle }` with `bundle(Component) -> &Bundle`.
  - `ResourceError::{Missing(String), Invalid(String), WrongVersion { component, found, pinned }, FileMissing(PathBuf), FileChanged(PathBuf), Io(String)}`; `code()` is always `"E_RUNTIME_MISSING"`.
  - `sha256_hex(&[u8]) -> String`.
  - `Resources { root: PathBuf }` with `dir(Component) -> PathBuf` (`root\omp`, `root\clink`), `font_file(family: &str) -> Option<PathBuf>` (`root\fonts\CaskaydiaCoveNerdFont-Regular.ttf` for `CaskaydiaCove NF`, any ASCII case), `manifest(&dyn Fs) -> Result<Manifest, ResourceError>`.
  - `check_bundle(&dyn Fs, dir: &Path, &Bundle) -> Result<(), ResourceError>`.
  - `fake_resources(&FakeFs, root: &Path) -> Resources` under `cfg(any(test, feature = "fake"))`: a tiny bundle with real hashes, for later tests.
- The `resources\` layout every later task relies on:
  ```
  resources\manifest.json
  resources\omp\oh-my-posh.exe
  resources\clink\clink_x64.exe, clink_dll_x64.dll
  resources\fonts\CaskaydiaCoveNerdFont-Regular.ttf
  resources\LICENSES\oh-my-posh-MIT.txt, clink-GPL-3.0.txt, clink-SOURCE.txt, CaskaydiaCove-NF-OFL-1.1.txt
  ```

**Pinned files and where each hash comes from** (all checked on 2026-10-11; "computed" means hashed from a download made into a new empty folder):

| File | URL | SHA-256 | Source of the hash |
|---|---|---|---|
| `posh-windows-amd64.exe` (15,134,472 bytes) | `https://github.com/JanDeDobbeleer/oh-my-posh/releases/download/v31.5.0/posh-windows-amd64.exe` | `9dd2c1297d5ae89f23f19b02f74a25510fcb56d84da5139fa3c3ca6e6d008a73` | publisher (`checksums.txt` of the release) = GitHub asset digest = computed; also the value `spikes/vm/guest/proto-setup.ps1` pinned |
| Oh My Posh `COPYING` (MIT, 1,056 bytes) | `https://raw.githubusercontent.com/JanDeDobbeleer/oh-my-posh/v31.5.0/COPYING` | `a5308c4e51268229a039ec4ec9a251a4cdb89d9380383e6e13aeba64a74f19ad` | computed (GitHub reports git blob `0052a411de0b7179803ca0723ddc782fe8249227` for it) |
| `clink.1.9.34.4e8ae4.zip` (3,976,407 bytes) | `https://github.com/chrisant996/clink/releases/download/v1.9.34/clink.1.9.34.4e8ae4.zip` | `50f6ee0474e3bd02b48ce8c8f63ed933a14aee5127fc7072cb976ce10a43ce23` | GitHub asset digest = computed (Clink publishes an Authenticode `.cat`, not a checksum file) |
| `clink_x64.exe` (in the zip) | | `becacc801270b8282c4cdff9378dc40f061bfe1b76bb87ca621c4bc572c2d8eb` | computed |
| `clink_dll_x64.dll` (in the zip) | | `889b712037886dfde205e4b637a021848734ed218ee21c5718e2bf23a9e9d8f2` | computed |
| Clink `LICENSE` (GPL-3.0, in the zip) | | `5f631fae467c82b8cd28fd1ec425c816895a35f9d94e36bee0e0164570e8e0f6` | computed |
| `CascadiaCode.zip` (56,585,471 bytes) | `https://github.com/ryanoasis/nerd-fonts/releases/download/v3.5.1/CascadiaCode.zip` | `1298bf92698afa06185cf1d05e6ae05f2d8a1e8c3cb45ddf4c3035168ab342a1` | publisher (`SHA-256.txt` of the release) = GitHub asset digest = computed = spike G12 |
| `CaskaydiaCoveNerdFont-Regular.ttf` (in the zip, 2,959,676 bytes) | | `fae37ce4fdc965ba5d3caea122b856418dc5dec2e4e638a024c49d681ffce5e9` | computed |
| Nerd Fonts `LICENSE` (SIL OFL 1.1 with Microsoft's Cascadia Code notice, in the zip) | | `82c05d6c53dfa0c9025985c19e371810020b74ed2c61d51d370f2a8ab2506d52` | computed |

Only `clink_x64.exe` and `clink_dll_x64.dll` are bundled from Clink (spec §6.1 counts "Clink x64 2.9 MB"; Clink's Lua scripts are inside the DLL). Phase 0 proved Clink from the whole unpacked zip; Task 19 proves this two-file layout on the VMs (D3 must see `Inject completed`).

- [ ] **Step 1: Add the dependency and the module**

In `crates/core/Cargo.toml` under `[dependencies]` add `sha2 = "0.11"`. Add `pub mod resources;` to `crates/core/src/lib.rs`. Add this line to `.gitignore`:
```text
/resources/
```

- [ ] **Step 2: Write the failing tests** — create `crates/core/src/resources.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeFs;

    #[test]
    fn sha256_matches_the_known_value() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn manifest_loads_and_bundles_check_out() {
        let fs = FakeFs::new();
        let res = fake_resources(&fs, Path::new(r"C:\app\resources"));
        let m = res.manifest(&fs).unwrap();
        check_bundle(&fs, &res.dir(Component::Clink), m.bundle(Component::Clink)).unwrap();
        fs.put(r"C:\app\resources\clink\clink_dll_x64.dll", b"tampered");
        assert!(matches!(check_bundle(&fs, &res.dir(Component::Clink), &m.clink), Err(ResourceError::FileChanged(_))));
        fs.remove_file(Path::new(r"C:\app\resources\omp\oh-my-posh.exe")).unwrap_or(());
        assert!(matches!(check_bundle(&fs, &res.dir(Component::Omp), &m.omp), Err(ResourceError::FileMissing(_))));
    }

    #[test]
    fn missing_or_mismatched_manifests_are_refused() {
        let fs = FakeFs::new();
        let res = Resources { root: PathBuf::from(r"C:\nowhere") };
        assert!(matches!(res.manifest(&fs), Err(ResourceError::Missing(_))));
        let res = fake_resources(&fs, Path::new(r"C:\r"));
        let text = String::from_utf8(fs.file(r"C:\r\manifest.json").unwrap()).unwrap().replace(OMP_VERSION, "31.4.0");
        fs.put(r"C:\r\manifest.json", text);
        assert!(matches!(res.manifest(&fs), Err(ResourceError::WrongVersion { component: "Oh My Posh", .. })));
        assert_eq!(res.manifest(&fs).unwrap_err().code(), "E_RUNTIME_MISSING");
    }

    #[test]
    fn only_the_nerd_font_is_bundled() {
        let res = Resources { root: PathBuf::from(r"C:\r") };
        assert_eq!(res.font_file("caskaydiacove nf"), Some(PathBuf::from(r"C:\r\fonts\CaskaydiaCoveNerdFont-Regular.ttf")));
        assert_eq!(res.font_file("Cascadia Mono"), None);
    }
}
```
(`fs.remove_file` needs the `Fs` trait in scope: `use super::*;` brings it, because the implementation imports `crate::adapters::Fs`.)

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- resources::tests`
Expected: compile errors (`cannot find function sha256_hex`, …).

- [ ] **Step 4: Implement it above the tests**
```rust
//! The runtime files CMD Studio ships (spec §5.1 `resources/`, §6.2): Oh My
//! Posh, Clink (x64) and the Nerd Font, with the SHA-256 of every file in
//! `manifest.json`, written by `scripts/fetch-resources.ps1`.
use crate::adapters::Fs;
use crate::plan::{Component, CLINK_VERSION, NERD_FONT_FAMILY, OMP_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "manifest.json";
/// The bundled Nerd Font file; its GDI family is `CaskaydiaCove NF` (G12).
pub const NERD_FONT_FILE: &str = "CaskaydiaCoveNerdFont-Regular.ttf";
pub const NERD_FONTS_VERSION: &str = "3.5.1";

/// One bundled component: its version and `file name → lowercase SHA-256`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Bundle {
    pub version: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub omp: Bundle,
    pub clink: Bundle,
    pub fonts: Bundle,
}

impl Manifest {
    pub fn bundle(&self, component: Component) -> &Bundle {
        match component {
            Component::Omp => &self.omp,
            Component::Clink => &self.clink,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResourceError {
    #[error("the bundled files are missing: {0} (run scripts/fetch-resources.ps1)")]
    Missing(String),
    #[error("resources/manifest.json is not valid: {0}")]
    Invalid(String),
    #[error("resources/manifest.json is for {component} {found}, but this build pins {pinned}")]
    WrongVersion { component: &'static str, found: String, pinned: &'static str },
    #[error("{0} is missing")]
    FileMissing(PathBuf),
    #[error("{0} does not match the pinned hash")]
    FileChanged(PathBuf),
    #[error("{0}")]
    Io(String),
}

impl ResourceError {
    /// Stable code shown to users (spec §5.9).
    pub fn code(&self) -> &'static str {
        "E_RUNTIME_MISSING"
    }
}

/// Lowercase hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Where the bundled files are: `<exe dir>\resources`, or `--resources <dir>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resources {
    pub root: PathBuf,
}

impl Resources {
    pub fn dir(&self, component: Component) -> PathBuf {
        match component {
            Component::Omp => self.root.join("omp"),
            Component::Clink => self.root.join("clink"),
        }
    }

    /// The bundled font file for `family`; only the Nerd Font is bundled.
    pub fn font_file(&self, family: &str) -> Option<PathBuf> {
        family.eq_ignore_ascii_case(NERD_FONT_FAMILY).then(|| self.root.join("fonts").join(NERD_FONT_FILE))
    }

    /// Reads `manifest.json` and checks it pins the versions this build expects.
    pub fn manifest(&self, fs: &dyn Fs) -> Result<Manifest, ResourceError> {
        let path = self.root.join(MANIFEST);
        let bytes = fs
            .read(&path)
            .map_err(|e| ResourceError::Io(e.to_string()))?
            .ok_or_else(|| ResourceError::Missing(path.display().to_string()))?;
        let m: Manifest = serde_json::from_slice(&bytes).map_err(|e| ResourceError::Invalid(e.to_string()))?;
        for (component, found, pinned) in [
            ("Oh My Posh", &m.omp.version, OMP_VERSION),
            ("Clink", &m.clink.version, CLINK_VERSION),
            ("Nerd Fonts", &m.fonts.version, NERD_FONTS_VERSION),
        ] {
            if found != pinned {
                return Err(ResourceError::WrongVersion { component, found: found.clone(), pinned });
            }
        }
        Ok(m)
    }
}

/// Checks that every file of `bundle` sits in `dir` with its pinned hash.
pub fn check_bundle(fs: &dyn Fs, dir: &Path, bundle: &Bundle) -> Result<(), ResourceError> {
    for (name, hash) in &bundle.files {
        let path = dir.join(name);
        let bytes = fs
            .read(&path)
            .map_err(|e| ResourceError::Io(e.to_string()))?
            .ok_or_else(|| ResourceError::FileMissing(path.clone()))?;
        if sha256_hex(&bytes) != *hash {
            return Err(ResourceError::FileChanged(path));
        }
    }
    Ok(())
}

/// A tiny bundle on a fake disk: one or two files per component with their
/// real hashes, and a manifest that pins this build's versions.
#[cfg(any(test, feature = "fake"))]
pub fn fake_resources(fs: &crate::adapters::fake::FakeFs, root: &Path) -> Resources {
    let files = |dir: &str, name: &str, body: &[u8]| {
        fs.put(root.join(dir).join(name), body);
        BTreeMap::from([(name.to_string(), sha256_hex(body))])
    };
    let omp = files("omp", "oh-my-posh.exe", b"omp-binary");
    let mut clink = files("clink", "clink_x64.exe", b"clink-exe");
    clink.extend(files("clink", "clink_dll_x64.dll", b"clink-dll"));
    let fonts = files("fonts", NERD_FONT_FILE, b"font-bytes");
    let m = Manifest {
        omp: Bundle { version: OMP_VERSION.into(), files: omp },
        clink: Bundle { version: CLINK_VERSION.into(), files: clink },
        fonts: Bundle { version: NERD_FONTS_VERSION.into(), files: fonts },
    };
    fs.put(root.join(MANIFEST), serde_json::to_vec_pretty(&m).unwrap_or_default());
    Resources { root: root.to_path_buf() }
}
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- resources::tests`
Expected: `4 passed`.

- [ ] **Step 6: Write the fetch script** `scripts/fetch-resources.ps1`. Keep it ASCII-only: Windows PowerShell 5.1 reads a `.ps1` without a BOM in the system code page.
```powershell
<#
.SYNOPSIS
  Fills resources\ with the runtimes CMD Studio ships (spec sections 5.1, 6.1, 6.6):
  Oh My Posh 31.5.0, Clink 1.9.34 (x64 files only), the CaskaydiaCove NF font
  from Nerd Fonts 3.5.1, and their licenses. Writes resources\manifest.json
  with the SHA-256 of every file, which the app checks (doctor D1).
.DESCRIPTION
  Every download is checked against the SHA-256 pinned below before it is
  used, and every file taken out of an archive against its own pin. A
  resources\ folder that already matches is left alone, so CI can cache it.
  Works in Windows PowerShell 5.1 and PowerShell 7.
.PARAMETER Destination
  Where to put the files. Default: resources\ at the repo root.
.PARAMETER Cache
  A folder that may already hold the downloads below under their file names;
  a file there is used only when its hash matches.
#>
param(
  [string] $Destination = (Join-Path (Split-Path $PSScriptRoot -Parent) 'resources'),
  [string] $Cache = ''
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # Invoke-WebRequest is very slow with a progress bar in 5.1
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# Downloads. Hashes checked on 2026-10-11:
#  - posh-windows-amd64.exe: publisher's checksums.txt, GitHub's asset digest, and a local download agree.
#  - COPYING: computed from a local download of the v31.5.0 tag (GitHub blob 0052a411de0b7179803ca0723ddc782fe8249227).
#  - clink zip: GitHub's asset digest and a local download agree (Clink publishes a .cat catalog, not a checksum file).
#  - CascadiaCode.zip: publisher's SHA-256.txt, GitHub's asset digest, a local download and spike G12 agree.
$Downloads = @(
  @{ Name = 'posh-windows-amd64.exe'; Url = 'https://github.com/JanDeDobbeleer/oh-my-posh/releases/download/v31.5.0/posh-windows-amd64.exe'; Sha256 = '9dd2c1297d5ae89f23f19b02f74a25510fcb56d84da5139fa3c3ca6e6d008a73' },
  @{ Name = 'oh-my-posh-COPYING'; Url = 'https://raw.githubusercontent.com/JanDeDobbeleer/oh-my-posh/v31.5.0/COPYING'; Sha256 = 'a5308c4e51268229a039ec4ec9a251a4cdb89d9380383e6e13aeba64a74f19ad' },
  @{ Name = 'clink.1.9.34.4e8ae4.zip'; Url = 'https://github.com/chrisant996/clink/releases/download/v1.9.34/clink.1.9.34.4e8ae4.zip'; Sha256 = '50f6ee0474e3bd02b48ce8c8f63ed933a14aee5127fc7072cb976ce10a43ce23' },
  @{ Name = 'CascadiaCode.zip'; Url = 'https://github.com/ryanoasis/nerd-fonts/releases/download/v3.5.1/CascadiaCode.zip'; Sha256 = '1298bf92698afa06185cf1d05e6ae05f2d8a1e8c3cb45ddf4c3035168ab342a1' }
)
# Files taken out of the archives (computed 2026-10-11 from the archives above).
$Pinned = @{
  'omp\oh-my-posh.exe'                        = '9dd2c1297d5ae89f23f19b02f74a25510fcb56d84da5139fa3c3ca6e6d008a73'
  'clink\clink_x64.exe'                       = 'becacc801270b8282c4cdff9378dc40f061bfe1b76bb87ca621c4bc572c2d8eb'
  'clink\clink_dll_x64.dll'                   = '889b712037886dfde205e4b637a021848734ed218ee21c5718e2bf23a9e9d8f2'
  'fonts\CaskaydiaCoveNerdFont-Regular.ttf'   = 'fae37ce4fdc965ba5d3caea122b856418dc5dec2e4e638a024c49d681ffce5e9'
  'LICENSES\oh-my-posh-MIT.txt'               = 'a5308c4e51268229a039ec4ec9a251a4cdb89d9380383e6e13aeba64a74f19ad'
  'LICENSES\clink-GPL-3.0.txt'                = '5f631fae467c82b8cd28fd1ec425c816895a35f9d94e36bee0e0164570e8e0f6'
  'LICENSES\CaskaydiaCove-NF-OFL-1.1.txt'     = '82c05d6c53dfa0c9025985c19e371810020b74ed2c61d51d370f2a8ab2506d52'
}
$ClinkSource = "Clink 1.9.34 is GPL-3.0. Its source code for this exact release:`nhttps://github.com/chrisant996/clink/tree/v1.9.34`n"

function Hash([string] $Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function NewDir([string] $Parent) {
  $d = Join-Path $Parent ("cmdstudio-" + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Path $d | Out-Null
  $d
}
function Manifest([hashtable] $files) {
  # Hand-written JSON: stable order and LF endings in both PowerShell versions.
  $bundle = {
    param($version, $prefix)
    $lines = @($files.Keys | Where-Object { $_.StartsWith("$prefix\") } | Sort-Object | ForEach-Object {
        '      "{0}": "{1}"' -f $_.Substring($prefix.Length + 1), $files[$_] })
    "    `"version`": `"$version`",`n    `"files`": {`n" + ($lines -join ",`n") + "`n    }"
  }
  "{`n  `"omp`": {`n" + (& $bundle '31.5.0' 'omp') + "`n  },`n  `"clink`": {`n" + (& $bundle '1.9.34' 'clink') +
    "`n  },`n  `"fonts`": {`n" + (& $bundle '3.5.1' 'fonts') + "`n  }`n}`n"
}
function Complete([string] $Dir) {
  $m = Join-Path $Dir 'manifest.json'
  if (-not (Test-Path -LiteralPath $m) -or [IO.File]::ReadAllText($m) -ne (Manifest $Pinned)) { return $false }
  foreach ($rel in $Pinned.Keys) {
    $p = Join-Path $Dir $rel
    if (-not (Test-Path -LiteralPath $p) -or (Hash $p) -ne $Pinned[$rel]) { return $false }
  }
  $true
}

if (Complete $Destination) { "resources are complete: $Destination"; return }

$work = NewDir ([IO.Path]::GetTempPath())
$stage = ''
try {
  $got = @{}
  foreach ($d in $Downloads) {
    $cached = if ($Cache) { Join-Path $Cache $d.Name } else { '' }
    $dir = NewDir $work   # one new, empty folder per download
    $file = Join-Path $dir $d.Name
    if ($cached -and (Test-Path -LiteralPath $cached) -and (Hash $cached) -eq $d.Sha256) {
      Copy-Item -LiteralPath $cached -Destination $file
    } else {
      "downloading $($d.Url)"
      Invoke-WebRequest -Uri $d.Url -OutFile $file -UseBasicParsing
    }
    $h = Hash $file
    if ($h -ne $d.Sha256) { throw "$($d.Name): SHA-256 $h, expected $($d.Sha256)" }
    $got[$d.Name] = $file
  }
  $clinkX = NewDir $work
  Expand-Archive -LiteralPath $got['clink.1.9.34.4e8ae4.zip'] -DestinationPath $clinkX
  $fontX = NewDir $work
  Expand-Archive -LiteralPath $got['CascadiaCode.zip'] -DestinationPath $fontX

  # Staged next to the destination, so the final step is a rename on one volume.
  $parent = Split-Path $Destination -Parent
  New-Item -ItemType Directory -Force -Path $parent | Out-Null
  $stage = NewDir $parent
  $copy = @{
    'omp\oh-my-posh.exe'                      = $got['posh-windows-amd64.exe']
    'clink\clink_x64.exe'                     = Join-Path $clinkX 'clink_x64.exe'
    'clink\clink_dll_x64.dll'                 = Join-Path $clinkX 'clink_dll_x64.dll'
    'fonts\CaskaydiaCoveNerdFont-Regular.ttf' = Join-Path $fontX 'CaskaydiaCoveNerdFont-Regular.ttf'
    'LICENSES\oh-my-posh-MIT.txt'             = $got['oh-my-posh-COPYING']
    'LICENSES\clink-GPL-3.0.txt'              = Join-Path $clinkX 'LICENSE'
    'LICENSES\CaskaydiaCove-NF-OFL-1.1.txt'   = Join-Path $fontX 'LICENSE'
  }
  foreach ($rel in $copy.Keys) {
    $to = Join-Path $stage $rel
    New-Item -ItemType Directory -Force -Path (Split-Path $to -Parent) | Out-Null
    Copy-Item -LiteralPath $copy[$rel] -Destination $to
    $h = Hash $to
    if ($h -ne $Pinned[$rel]) { throw "${rel}: SHA-256 $h, expected $($Pinned[$rel])" }
  }
  $utf8 = New-Object Text.UTF8Encoding $false
  [IO.File]::WriteAllText((Join-Path $stage 'LICENSES\clink-SOURCE.txt'), $ClinkSource, $utf8)
  [IO.File]::WriteAllText((Join-Path $stage 'manifest.json'), (Manifest $Pinned), $utf8)

  if (Test-Path -LiteralPath $Destination) { Remove-Item -LiteralPath $Destination -Recurse -Force }
  Move-Item -LiteralPath $stage -Destination $Destination
  $stage = ''
  if (-not (Complete $Destination)) { throw "resources at $Destination are incomplete after the copy" }
  "resources ready: $Destination"
} finally {
  Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
  if ($stage) { Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue }
}
```

- [ ] **Step 7: Run the script and check what it made**

Run (PowerShell, repo root; downloads about 76 MB from GitHub):
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\fetch-resources.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\fetch-resources.ps1
Get-Content resources\manifest.json -Raw
git status --short resources
```
Expected: the first run prints four `downloading …` lines and `resources ready: …\resources`; the second prints `resources are complete: …\resources`; `manifest.json` is exactly:
```json
{
  "omp": {
    "version": "31.5.0",
    "files": {
      "oh-my-posh.exe": "9dd2c1297d5ae89f23f19b02f74a25510fcb56d84da5139fa3c3ca6e6d008a73"
    }
  },
  "clink": {
    "version": "1.9.34",
    "files": {
      "clink_dll_x64.dll": "889b712037886dfde205e4b637a021848734ed218ee21c5718e2bf23a9e9d8f2",
      "clink_x64.exe": "becacc801270b8282c4cdff9378dc40f061bfe1b76bb87ca621c4bc572c2d8eb"
    }
  },
  "fonts": {
    "version": "3.5.1",
    "files": {
      "CaskaydiaCoveNerdFont-Regular.ttf": "fae37ce4fdc965ba5d3caea122b856418dc5dec2e4e638a024c49d681ffce5e9"
    }
  }
}
```
and `git status` prints nothing for `resources` (it is ignored). Then change one byte and confirm the script notices and rebuilds the folder (it downloads again):
```powershell
$dll = 'resources\clink\clink_dll_x64.dll'; $b = [IO.File]::ReadAllBytes($dll); $b[100] = $b[100] -bxor 0xFF; [IO.File]::WriteAllBytes($dll, $b)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\fetch-resources.ps1
(Get-FileHash $dll -Algorithm SHA256).Hash.ToLower()
```
Expected: `resources ready: …`, and the hash is `889b712037886dfde205e4b637a021848734ed218ee21c5718e2bf23a9e9d8f2` again.

- [ ] **Step 8: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 9: Commit**

Message:
```text
feat: pin and fetch the bundled runtimes

scripts/fetch-resources.ps1 downloads Oh My Posh 31.5.0, Clink 1.9.34
and Nerd Fonts 3.5.1, checks every download and every extracted file
against a pinned SHA-256, and writes resources/manifest.json. The
posh and Nerd Fonts hashes match the publishers' checksum files; the
Clink zip hash matches GitHub's asset digest. resources/ is ignored.
core::resources reads the manifest and checks a bundle against it.
```
Run:
```bash
git add -- .gitignore scripts/fetch-resources.ps1 crates/core/Cargo.toml Cargo.lock crates/core/src/lib.rs crates/core/src/resources.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 3: Files on Windows — atomic writes (`WinFs`)

**Files:**
- Create: `crates/core/src/adapters/windows/mod.rs`, `crates/core/src/adapters/windows/fs.rs`
- Modify: `crates/core/src/adapters/mod.rs` (add `#[cfg(windows)] pub mod windows;` next to `pub mod fake;`), `crates/core/Cargo.toml` (dev-dependency `tempfile`), `Cargo.lock`

**Interfaces:**
- Consumes: `adapters::{AdapterError, DirEntry, Fs}` (Task 1); `resources::{Resources, check_bundle}` (Task 2) in one test.
- Produces:
  - `adapters::windows::io_err(op: &str, path: &Path, e: &std::io::Error) -> AdapterError` (`pub(crate)`).
  - `adapters::windows::fs::WinFs` (unit struct, `Copy + Default`) implementing `Fs`. `write_atomic` writes `.<name>.<uuid>.cmdstudio-tmp` in the target's folder, `sync_all` (`FlushFileBuffers`), then `std::fs::rename` over the target (on Windows `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`, or the POSIX-semantics rename), retrying up to 20 × 50 ms on `ERROR_ACCESS_DENIED` (5) or `ERROR_SHARING_VIOLATION` (32). A symbolic link is resolved with `std::fs::canonicalize` first, so the link stays and its target is written. `rename` refuses when `to` exists.

- [ ] **Step 1: Add the test dependency and the module tree**

In `crates/core/Cargo.toml`, add to the end of the existing `[dev-dependencies]` table (after its `proptest` and `mlua` lines):
```toml
tempfile = "3"
```
In `crates/core/src/adapters/mod.rs`, next to `pub mod fake;`, add:
```rust
#[cfg(windows)]
pub mod windows;
```
Create `crates/core/src/adapters/windows/mod.rs`:
```rust
//! Windows implementations of the adapters (spec §5.2). Each one takes the
//! places it touches as parameters, so tests can aim it at a throwaway
//! registry key and temp folders; `real()` constructors use the user's own.
use super::AdapterError;
use std::path::Path;

pub mod fs;

pub(crate) fn io_err(op: &str, path: &Path, e: &std::io::Error) -> AdapterError {
    AdapterError::new(format!("{op} {}", path.display()), e.to_string())
}
```

- [ ] **Step 2: Write the failing tests** — create `crates/core/src/adapters/windows/fs.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomic_replaces_creates_parents_and_leaves_no_temp_file() {
        // Review Focus 4: a folder with Thai letters and a space.
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("ไทย dir").join("settings.json");
        WinFs.write_atomic(&f, b"one").unwrap();
        WinFs.write_atomic(&f, b"two").unwrap();
        assert_eq!(WinFs.read(&f).unwrap(), Some(b"two".to_vec()));
        let names: Vec<_> = std::fs::read_dir(f.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from("settings.json")]);
    }

    #[test]
    fn a_failed_write_keeps_the_old_file() {
        let tmp = tempfile::tempdir().unwrap();
        // A folder where the file should go makes the rename fail.
        let f = tmp.path().join("x.json");
        std::fs::create_dir(&f).unwrap();
        std::fs::write(f.join("inner"), b"keep").unwrap();
        assert!(WinFs.write_atomic(&f, b"new").is_err());
        assert_eq!(std::fs::read(f.join("inner")).unwrap(), b"keep");
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1, "temp file left behind");
    }

    #[test]
    fn writing_through_a_symlink_keeps_the_link() {
        // Review Focus 5: settings.json linked into the user's dotfiles.
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("dotfiles-settings.json");
        std::fs::write(&real, b"old").unwrap();
        let link = tmp.path().join("settings.json");
        if let Err(e) = std::os::windows::fs::symlink_file(&real, &link) {
            // Creating links needs Developer Mode or admin rights; CI runners and
            // the test VMs have them, so there the test must run, not skip.
            assert!(std::env::var_os("CMDSTUDIO_SYSTEM_TESTS").is_none(), "cannot create a symlink: {e}");
            println!("SKIPPED writing_through_a_symlink_keeps_the_link: cannot create a symlink here ({e})");
            return;
        }
        WinFs.write_atomic(&link, b"new").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(&real).unwrap(), b"new");
    }

    #[test]
    fn missing_things_are_not_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let gone = tmp.path().join("gone");
        assert_eq!(WinFs.read(&gone).unwrap(), None);
        WinFs.remove_file(&gone).unwrap();
        WinFs.remove_dir(&gone).unwrap();
        WinFs.remove_dir_all(&gone).unwrap();
        assert!(WinFs.list_dir(&gone).unwrap().is_empty());
    }

    #[test]
    fn rename_refuses_to_overwrite_and_lists_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("b-dir"), tmp.path().join("a-dir"));
        WinFs.create_dir_all(&a).unwrap();
        WinFs.create_dir_all(&b).unwrap();
        assert!(WinFs.rename(&a, &b).is_err());
        WinFs.write_atomic(&tmp.path().join("c.txt"), b"c").unwrap();
        let listed: Vec<_> = WinFs
            .list_dir(tmp.path())
            .unwrap()
            .into_iter()
            .map(|e| (e.path.file_name().unwrap().to_owned(), e.is_dir))
            .collect();
        assert_eq!(listed, vec![("a-dir".into(), true), ("b-dir".into(), true), ("c.txt".into(), false)]);
        WinFs.rename(&a, &tmp.path().join("z-dir")).unwrap();
        assert!(!a.exists());
    }

    /// Real-data seam: the files scripts\fetch-resources.ps1 put in resources\
    /// match their pins, read through the real adapter. Reads only.
    #[test]
    fn the_fetched_resources_match_their_pins() {
        use crate::plan::Component;
        use crate::resources::{check_bundle, Resources};
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("resources");
        if !root.join("manifest.json").is_file() {
            assert!(std::env::var_os("CMDSTUDIO_SYSTEM_TESTS").is_none(), "run scripts/fetch-resources.ps1 first");
            println!("SKIPPED the_fetched_resources_match_their_pins: no resources folder yet");
            return;
        }
        let res = Resources { root };
        let m = res.manifest(&WinFs).unwrap();
        check_bundle(&WinFs, &res.dir(Component::Omp), &m.omp).unwrap();
        check_bundle(&WinFs, &res.dir(Component::Clink), &m.clink).unwrap();
        check_bundle(&WinFs, &res.root.join("fonts"), &m.fonts).unwrap();
    }
}
```

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- adapters::windows::fs`
Expected: compile errors (`cannot find value WinFs`).

- [ ] **Step 4: Implement it above the tests**
```rust
//! Files on the real disk, with atomic replacement (spec §5.4 `Fs`).
use super::io_err;
use crate::adapters::{AdapterError, DirEntry, Fs};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Windows error codes worth a short retry: another process (Windows
/// Terminal, an antivirus scanner) has the file open for a moment.
const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_SHARING_VIOLATION: i32 = 32;
const RETRIES: u32 = 20;
const RETRY_DELAY: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, Default)]
pub struct WinFs;

/// The path to write: a symbolic link's target, so a settings.json that is a
/// link into the user's dotfiles stays a link.
fn write_target(path: &Path) -> Result<PathBuf, AdapterError> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            std::fs::canonicalize(path).map_err(|e| io_err("resolve link", path, &e))
        }
        _ => Ok(path.to_path_buf()),
    }
}

fn busy(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION))
}

/// `std::fs::rename` replaces an existing file in one step on Windows
/// (`MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`, or the POSIX-semantics
/// rename on newer builds). Retries while another process holds the file.
fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut attempt = 0;
    loop {
        match std::fs::rename(from, to) {
            Err(e) if busy(&e) && attempt < RETRIES => {
                attempt += 1;
                std::thread::sleep(RETRY_DELAY);
            }
            other => return other,
        }
    }
}

impl Fs for WinFs {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, AdapterError> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_err("read", path, &e)),
        }
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), AdapterError> {
        let target = write_target(path)?;
        let dir =
            target.parent().ok_or_else(|| AdapterError::new(path.display().to_string(), "has no parent folder"))?;
        std::fs::create_dir_all(dir).map_err(|e| io_err("create folder", dir, &e))?;
        let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let tmp = dir.join(format!(".{name}.{}.cmdstudio-tmp", uuid::Uuid::new_v4().simple()));
        let written = (|| -> std::io::Result<()> {
            let mut f = std::fs::File::create_new(&tmp)?;
            f.write_all(bytes)?;
            // FlushFileBuffers: the bytes are on disk before the rename makes them visible.
            f.sync_all()
        })();
        let result = written.and_then(|()| replace(&tmp, &target));
        if let Err(e) = result {
            let _ = std::fs::remove_file(&tmp);
            return Err(io_err("write", path, &e));
        }
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<(), AdapterError> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_err("delete", path, &e)),
        }
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
        std::fs::create_dir_all(path).map_err(|e| io_err("create folder", path, &e))
    }

    fn remove_dir(&self, path: &Path) -> Result<(), AdapterError> {
        match std::fs::remove_dir(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_err("delete folder", path, &e)),
        }
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
        match std::fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_err("delete folder", path, &e)),
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), AdapterError> {
        if to.exists() {
            return Err(AdapterError::new(format!("rename to {}", to.display()), "it already exists"));
        }
        std::fs::rename(from, to).map_err(|e| io_err("rename", from, &e))
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, AdapterError> {
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_err("list", path, &e)),
        };
        let mut out = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| io_err("list", path, &e))?;
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            out.push(DirEntry { path: entry.path(), is_dir });
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }
}
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- adapters::windows::fs --nocapture`
Expected: `6 passed`. On a dev machine without Developer Mode the output also shows `SKIPPED writing_through_a_symlink_keeps_the_link: …` (error 1314); after Task 2's Step 7 `the_fetched_resources_match_their_pins` runs for real (no `SKIPPED` line for it). `a_failed_write_keeps_the_old_file` takes about a second: the rename into a folder fails with access denied, which is retried 20 times.

- [ ] **Step 6: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 7: Commit**

Message:
```text
feat(core): add WinFs with atomic file replacement

Writes go to a temp file in the target's folder, are flushed, then
renamed over the target, with a short retry while another program
holds the file. A settings.json that is a symlink keeps being a
symlink: its target is written. A test checks the fetched runtimes
against their pinned hashes through this adapter.
```
Run:
```bash
git add -- crates/core/Cargo.toml Cargo.lock crates/core/src/adapters/mod.rs crates/core/src/adapters/windows/mod.rs crates/core/src/adapters/windows/fs.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 4: The AutoRun registry adapter (`WinAutoRun`)

**Files:**
- Create: `crates/core/src/adapters/windows/registry.rs`
- Modify: `crates/core/Cargo.toml` (`winreg`), `crates/core/src/adapters/windows/mod.rs` (`pub mod registry;`), `Cargo.lock`

**Interfaces:**
- Consumes: `adapters::{AdapterError, AutoRun, RegValue, REG_SZ, REG_EXPAND_SZ}` (Task 1).
- Produces (in `cmdstudio_core::adapters::windows::registry`):
  - `COMMAND_PROCESSOR = r"Software\Microsoft\Command Processor"`, `AUTORUN = "AutoRun"`.
  - `Hive::{CurrentUser, LocalMachine}`, `RegLocation { hive, subkey: String }` with `RegLocation::user(&str)`, `RegLocation::machine(&str)` and, for other Windows adapters, `pub(crate)` methods `open(write: bool) -> Result<Option<winreg::RegKey>, AdapterError>`, `open_or_create() -> Result<RegKey, _>`, `read_raw(name) -> Result<Option<RegValue>, _>`, `write_raw(name, &RegValue)`, `delete_value(name)` (`Ok` when absent), `exists() -> Result<bool, _>`, `delete_if_empty()` (only a key with no values and no subkeys; `Ok` when absent).
  - `WinAutoRun` with `WinAutoRun::real()` (HKCU and HKLM `COMMAND_PROCESSOR`) and `WinAutoRun::at(user: RegLocation, machine: RegLocation)` (tests); implements `AutoRun`.
  - `#[cfg(test)] pub(crate) TestKey` — a throwaway `HKCU\Software\CmdStudioTest\<uuid>` deleted on drop; `TestKey::new()`, `location(child: &str) -> RegLocation`. Task 6 reuses it.

The rules this pins (spec §5.4, §2.2 lesson 9): values are read and written raw (type number plus bytes, via `RegQueryValueExW`/`RegSetValueExW` in winreg's `get_raw_value`/`set_raw_value`), so `REG_EXPAND_SZ` stays `REG_EXPAND_SZ` and the text stays unexpanded. A key is opened with `RegOpenKeyExW` (`KEY_READ | KEY_SET_VALUE`); only a missing key is created, with `RegCreateKeyExW`, which never clears an existing key. Nothing ever deletes a key that still has values or subkeys.

- [ ] **Step 1: Add the dependency**

In `crates/core/Cargo.toml` add a Windows-only table (Implementation Gate: winreg 0.56.0's `RegValue` holds `bytes: Cow<'a, [u8]>` and `vtype: RegType`; `RegKey::query_info()` returns `RegKeyMetadata` with `sub_keys` and `values`):
```toml
[target.'cfg(windows)'.dependencies]
winreg = "=0.56.0"
```
Add `pub mod registry;` to `crates/core/src/adapters/windows/mod.rs`.

- [ ] **Step 2: Write the failing tests** — create `crates/core/src/adapters/windows/registry.rs` with only:
```rust
/// A throwaway key `HKCU\Software\CmdStudioTest\<uuid>`, deleted on drop.
#[cfg(test)]
pub(crate) struct TestKey {
    pub(crate) subkey: String,
}

#[cfg(test)]
impl TestKey {
    pub(crate) fn new() -> Self {
        TestKey { subkey: format!(r"Software\CmdStudioTest\{}", uuid::Uuid::new_v4().simple()) }
    }
    pub(crate) fn location(&self, child: &str) -> RegLocation {
        RegLocation::user(&format!(r"{}\{child}", self.subkey))
    }
}

#[cfg(test)]
impl Drop for TestKey {
    fn drop(&mut self) {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.subkey);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{REG_EXPAND_SZ as EXPAND, REG_SZ as SZ};

    fn adapter(t: &TestKey) -> WinAutoRun {
        WinAutoRun::at(t.location("Command Processor"), t.location("Machine"))
    }

    #[test]
    fn a_missing_key_reads_as_absent_and_is_created_on_write() {
        let t = TestKey::new();
        let a = adapter(&t);
        assert!(!a.user_key_exists().unwrap());
        assert_eq!(a.read_user().unwrap(), None);
        a.delete_user().unwrap();
        a.write_user(&RegValue::from_text(SZ, "echo hi")).unwrap();
        assert!(a.user_key_exists().unwrap());
        assert_eq!(a.read_user().unwrap().unwrap().text().as_deref(), Some("echo hi"));
    }

    #[test]
    fn raw_values_come_back_byte_for_byte_with_their_type() {
        // Review Focus 4: Thai text. REG_EXPAND_SZ stays REG_EXPAND_SZ, unexpanded.
        let t = TestKey::new();
        let a = adapter(&t);
        let v = RegValue::from_text(EXPAND, "%USERPROFILE%\\x.cmd & ผู้ใช้");
        a.write_user(&v).unwrap();
        assert_eq!(a.read_user().unwrap(), Some(v.clone()));
        let read = a.read_user().unwrap().unwrap();
        a.write_user(&read).unwrap();
        assert_eq!(a.read_user().unwrap(), Some(v));
    }

    #[test]
    fn other_values_and_subkeys_survive_write_and_delete() {
        // spec §2.2 lesson 9: never recreate the key.
        let t = TestKey::new();
        let loc = t.location("Command Processor");
        let key = loc.open_or_create().unwrap();
        key.set_value("CompletionChar", &0x40u32).unwrap();
        key.create_subkey("Child").unwrap();
        drop(key);
        let a = adapter(&t);
        a.write_user(&RegValue::from_text(SZ, "x")).unwrap();
        a.delete_user().unwrap();
        a.delete_user_key_if_empty().unwrap();
        let key = loc.open(false).unwrap().expect("key still there");
        assert_eq!(key.get_value::<u32, _>("CompletionChar").unwrap(), 0x40);
        assert!(key.open_subkey("Child").is_ok());
    }

    #[test]
    fn an_empty_key_we_created_can_go_again() {
        let t = TestKey::new();
        let a = adapter(&t);
        a.write_user(&RegValue::from_text(SZ, "x")).unwrap();
        a.delete_user().unwrap();
        a.delete_user_key_if_empty().unwrap();
        assert!(!a.user_key_exists().unwrap());
    }

    #[test]
    fn the_machine_value_is_read_from_its_own_key() {
        let t = TestKey::new();
        t.location("Machine").write_raw(AUTORUN, &RegValue::from_text(SZ, "clink inject")).unwrap();
        let a = adapter(&t);
        assert_eq!(a.read_machine().unwrap().unwrap().text().as_deref(), Some("clink inject"));
        assert_eq!(a.read_user().unwrap(), None);
    }

    /// Real-location check: reads the user's real AutoRun and writes the same
    /// bytes back. Runs only with CMDSTUDIO_SYSTEM_TESTS=1 (CI and test VMs).
    #[test]
    fn system_real_autorun_round_trips() {
        if std::env::var_os("CMDSTUDIO_SYSTEM_TESTS").is_none() {
            println!("SKIPPED system_real_autorun_round_trips: set CMDSTUDIO_SYSTEM_TESTS=1");
            return;
        }
        let a = WinAutoRun::real();
        let existed = a.user_key_exists().unwrap();
        let before = a.read_user().unwrap();
        let probe = RegValue::from_text(SZ, "rem cmdstudio system test");
        a.write_user(&probe).unwrap();
        assert_eq!(a.read_user().unwrap(), Some(probe));
        match &before {
            Some(v) => a.write_user(v).unwrap(),
            None => a.delete_user().unwrap(),
        }
        if !existed {
            a.delete_user_key_if_empty().unwrap();
        }
        assert_eq!(a.read_user().unwrap(), before);
        assert_eq!(a.user_key_exists().unwrap(), existed);
    }
}
```

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- adapters::windows::registry`
Expected: compile errors (`cannot find type RegLocation`, `WinAutoRun`, …).

- [ ] **Step 4: Implement it above the tests**
```rust
//! cmd's AutoRun value (spec §5.4 AutoRun). Values are read and written raw,
//! so the type (`REG_SZ` or `REG_EXPAND_SZ`) and every byte survive a
//! restore. Keys are opened, never recreated (spec §2.2 lesson 9).
use crate::adapters::{AdapterError, AutoRun, RegValue};
use std::borrow::Cow;
use std::io::ErrorKind;
use winreg::enums::{
    RegType, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_BINARY, REG_DWORD,
    REG_DWORD_BIG_ENDIAN, REG_EXPAND_SZ, REG_FULL_RESOURCE_DESCRIPTOR, REG_LINK, REG_MULTI_SZ, REG_NONE, REG_QWORD,
    REG_RESOURCE_LIST, REG_RESOURCE_REQUIREMENTS_LIST, REG_SZ,
};
use winreg::RegKey;

pub const COMMAND_PROCESSOR: &str = r"Software\Microsoft\Command Processor";
pub const AUTORUN: &str = "AutoRun";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

/// A registry key: hive plus subkey path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegLocation {
    pub hive: Hive,
    pub subkey: String,
}

impl RegLocation {
    pub fn user(subkey: &str) -> Self {
        RegLocation { hive: Hive::CurrentUser, subkey: subkey.to_string() }
    }
    pub fn machine(subkey: &str) -> Self {
        RegLocation { hive: Hive::LocalMachine, subkey: subkey.to_string() }
    }

    fn root(&self) -> RegKey {
        RegKey::predef(match self.hive {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        })
    }

    fn err(&self, what: &str, e: &std::io::Error) -> AdapterError {
        AdapterError::new(format!("{what} {:?}\\{}", self.hive, self.subkey), e.to_string())
    }

    /// The existing key, or `None` when it does not exist.
    pub(crate) fn open(&self, write: bool) -> Result<Option<RegKey>, AdapterError> {
        let access = if write { KEY_READ | KEY_SET_VALUE } else { KEY_READ };
        match self.root().open_subkey_with_flags(&self.subkey, access) {
            Ok(key) => Ok(Some(key)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(self.err("open", &e)),
        }
    }

    /// The key for writing. An existing key is opened as it is; a missing one
    /// is created (`RegCreateKeyExW`, which never clears an existing key).
    pub(crate) fn open_or_create(&self) -> Result<RegKey, AdapterError> {
        if let Some(key) = self.open(true)? {
            return Ok(key);
        }
        self.root()
            .create_subkey_with_flags(&self.subkey, KEY_READ | KEY_SET_VALUE)
            .map(|(key, _)| key)
            .map_err(|e| self.err("create", &e))
    }

    pub(crate) fn exists(&self) -> Result<bool, AdapterError> {
        Ok(self.open(false)?.is_some())
    }

    /// Deletes the key only when it holds no values and no subkeys; `Ok` when it is absent.
    pub(crate) fn delete_if_empty(&self) -> Result<(), AdapterError> {
        let Some(key) = self.open(false)? else {
            return Ok(());
        };
        let info = key.query_info().map_err(|e| self.err("query", &e))?;
        drop(key);
        if info.values != 0 || info.sub_keys != 0 {
            return Ok(());
        }
        match self.root().delete_subkey(&self.subkey) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(self.err("delete", &e)),
        }
    }

    pub(crate) fn read_raw(&self, name: &str) -> Result<Option<RegValue>, AdapterError> {
        let Some(key) = self.open(false)? else {
            return Ok(None);
        };
        match key.get_raw_value(name) {
            Ok(v) => Ok(Some(RegValue { kind: v.vtype.clone() as u32, bytes: v.bytes.to_vec() })),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(self.err("read", &e)),
        }
    }

    pub(crate) fn write_raw(&self, name: &str, value: &RegValue) -> Result<(), AdapterError> {
        let vtype = reg_type(value.kind).ok_or_else(|| {
            AdapterError::new(format!("write {name}"), format!("unknown registry type {}", value.kind))
        })?;
        let raw = winreg::RegValue { bytes: Cow::Borrowed(value.bytes.as_slice()), vtype };
        self.open_or_create()?.set_raw_value(name, &raw).map_err(|e| self.err("write", &e))
    }

    pub(crate) fn delete_value(&self, name: &str) -> Result<(), AdapterError> {
        let Some(key) = self.open(true)? else {
            return Ok(());
        };
        match key.delete_value(name) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(self.err("delete value in", &e)),
        }
    }
}

fn reg_type(kind: u32) -> Option<RegType> {
    Some(match kind {
        0 => REG_NONE,
        1 => REG_SZ,
        2 => REG_EXPAND_SZ,
        3 => REG_BINARY,
        4 => REG_DWORD,
        5 => REG_DWORD_BIG_ENDIAN,
        6 => REG_LINK,
        7 => REG_MULTI_SZ,
        8 => REG_RESOURCE_LIST,
        9 => REG_FULL_RESOURCE_DESCRIPTOR,
        10 => REG_RESOURCE_REQUIREMENTS_LIST,
        11 => REG_QWORD,
        _ => return None,
    })
}

/// AutoRun under two `Command Processor` keys: the user's (read and write)
/// and the machine's (read only).
#[derive(Clone, Debug)]
pub struct WinAutoRun {
    user: RegLocation,
    machine: RegLocation,
}

impl WinAutoRun {
    /// `HKCU\Software\Microsoft\Command Processor` and its HKLM twin.
    pub fn real() -> Self {
        WinAutoRun { user: RegLocation::user(COMMAND_PROCESSOR), machine: RegLocation::machine(COMMAND_PROCESSOR) }
    }

    /// Other keys, for tests.
    pub fn at(user: RegLocation, machine: RegLocation) -> Self {
        WinAutoRun { user, machine }
    }
}

impl AutoRun for WinAutoRun {
    fn user_key_exists(&self) -> Result<bool, AdapterError> {
        self.user.exists()
    }

    fn read_user(&self) -> Result<Option<RegValue>, AdapterError> {
        self.user.read_raw(AUTORUN)
    }

    fn write_user(&self, value: &RegValue) -> Result<(), AdapterError> {
        self.user.write_raw(AUTORUN, value)
    }

    fn delete_user(&self) -> Result<(), AdapterError> {
        self.user.delete_value(AUTORUN)
    }

    fn delete_user_key_if_empty(&self) -> Result<(), AdapterError> {
        self.user.delete_if_empty()
    }

    fn read_machine(&self) -> Result<Option<RegValue>, AdapterError> {
        self.machine.read_raw(AUTORUN)
    }
}
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- adapters::windows::registry --nocapture`
Expected: `6 passed`, with `SKIPPED system_real_autorun_round_trips: set CMDSTUDIO_SYSTEM_TESTS=1` in the output. Then check nothing was left behind:
```powershell
Test-Path 'HKCU:\Software\CmdStudioTest\*'
```
Expected: `False` (every test key was deleted).

- [ ] **Step 6: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 7: Commit**

Message:
```text
feat(core): add the AutoRun registry adapter

Values are read and written raw, so REG_EXPAND_SZ stays REG_EXPAND_SZ
and every byte survives a restore. Keys are opened, and only a missing
key is created (RegCreateKeyExW); the Command Processor key is never
recreated. Tests use a throwaway HKCU\Software\CmdStudioTest\<uuid> key;
the real key is touched only with CMDSTUDIO_SYSTEM_TESTS=1.
```
Run:
```bash
git add -- crates/core/Cargo.toml Cargo.lock crates/core/src/adapters/windows/mod.rs crates/core/src/adapters/windows/registry.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 5: The host adapter — SID, known folders, 8.3 names, the private ProgramData folder, system facts (`WinHost`)

**Files:**
- Create: `crates/core/src/adapters/windows/host.rs`
- Modify: `crates/core/Cargo.toml` (`windows-sys`), `crates/core/src/adapters/windows/mod.rs` (`wide`, `last_error`, `pub mod host;`), `Cargo.lock`

**Interfaces:**
- Consumes: `adapters::{AdapterError, Arch, Created, DefaultTerminal, DirState, Host, SystemInfo}` (Task 1); `registry::RegLocation` with `read_raw` (Task 4); `windows::io_err` (Task 3).
- Produces:
  - `adapters::windows::wide(&OsStr) -> Vec<u16>` (NUL-terminated UTF-16) and `adapters::windows::last_error(impl Into<String>) -> AdapterError` (both `pub(crate)`; Task 6 uses them too).
  - `adapters::windows::host::WinHost` (unit struct) implementing `Host`.
  - `pub(crate) fn create_dir_with_sddl(dir: &Path, sddl: &str) -> Result<Created, AdapterError>` and `pub(crate) fn judge(me: &str, owner: &str, allowed: Option<&[(String, u32)]>) -> DirState` (tested directly).

**Win32 calls, exactly** (windows-sys 0.61.2; Implementation Gate: read each signature in `~/.cargo/registry/src/*/windows-sys-0.61.2/src/Windows/Win32/...`):
- Known folders: `SHGetKnownFolderPath(&FOLDERID_LocalAppData | &FOLDERID_ProgramData, KF_FLAG_DEFAULT as u32, null, &mut PWSTR)`, then `CoTaskMemFree` on every path, success or not.
- SID: `OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut h)`; `GetTokenInformation(h, TokenUser, null, 0, &mut len)` for the size, again into a `Vec<u64>` buffer (keeps `TOKEN_USER`'s pointer aligned); `CloseHandle`; `ConvertSidToStringSidW` + `LocalFree`.
- 8.3: `GetShortPathNameW(long, null, 0)` for the size (includes the NUL), then into a buffer of that size; the result is shorter than the size.
- Private folder: `ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl, SDDL_REVISION_1, &mut sd, null)`; `CreateDirectoryW(path, &SECURITY_ATTRIBUTES { nLength, lpSecurityDescriptor: sd, bInheritHandle: 0 })`; read `GetLastError()` at once (`ERROR_ALREADY_EXISTS` = 183 → `Created::AlreadyExisted`); `LocalFree(sd)`. SDDL `D:P(A;OICI;FA;;;<SID>)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)`: `P` = protected (no inheritance from `%ProgramData%`), `OICI` = children inherit.
- Checking a folder: `symlink_metadata` first — a reparse point (`FILE_ATTRIBUTE_REPARSE_POINT`, junction or symlink) or a file is `Unsafe`; then `GetNamedSecurityInfoW(path, SE_FILE_OBJECT, OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION, &mut owner, null, &mut dacl, null, &mut sd)` (returns a `WIN32_ERROR`, 0 = success), `GetAclInformation(dacl, &mut ACL_SIZE_INFORMATION, size, AclSizeInformation)`, `GetAce(dacl, i, &mut ace)` for each entry; type `ACCESS_ALLOWED_ACE_TYPE` (0) reads `ACCESS_ALLOWED_ACE { Header, Mask, SidStart }` with the SID at `&SidStart`; `ACCESS_DENIED_ACE_TYPE` (1) is skipped (it only takes rights away); any other type counts as "unknown entry, may write". Owner and trustees compare as SID strings: the user's, `S-1-5-18` (SYSTEM), `S-1-5-32-544` (Administrators). A NULL DACL means everyone has full access → `Unsafe`. "May write" = any of `FILE_WRITE_DATA` 0x2, `FILE_APPEND_DATA` 0x4, `FILE_WRITE_EA` 0x10, `FILE_DELETE_CHILD` 0x40, `FILE_WRITE_ATTRIBUTES` 0x100, `DELETE` 0x10000, `WRITE_DAC` 0x40000, `WRITE_OWNER` 0x80000, `GENERIC_ALL` 0x10000000, `GENERIC_WRITE` 0x40000000.
- System facts: build from `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion` value `CurrentBuildNumber`; arch from `IsWow64Process2(GetCurrentProcess(), &mut process, &mut native)` (`IMAGE_FILE_MACHINE_AMD64` 0x8664, `IMAGE_FILE_MACHINE_ARM64` 0xAA64 — the native machine, so an x64 exe emulated on ARM64 still says ARM64); default terminal from `HKCU\Console\%%Startup` value `DelegationConsole` (`{00000000-…}` or absent = let Windows decide, `{2EACA947-7F5F-4CFA-BA87-8F7FBEEFBE69}` = Windows Terminal, `{06EC847C-C0A5-46B8-92CB-7C92F6E35CD5}` = WT Preview, `{B23D10C0-E52E-411E-9D5B-C09FDF709C7D}` = conhost; values from `policies/WindowsTerminal.admx` in microsoft/terminal); WebView2 from value `pv` of `HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}` or the same path under HKCU without `WOW6432Node` (`0.0.0.0` = none); packaged = `GetCurrentPackageFullName(&mut len, null) != APPMODEL_ERROR_NO_PACKAGE` (15700); UI language `GetUserDefaultUILanguage()`.

- [ ] **Step 1: Add the dependency and the helpers**

In `crates/core/Cargo.toml`, in the `[target.'cfg(windows)'.dependencies]` table, add (the comment says who uses what; Task 6 needs the Gdi and WindowsAndMessaging features):
```toml
# host.rs: Foundation, Globalization, Security(+Authorization), Storage_FileSystem,
# Storage_Packaging_Appx, System_Com, System_SystemInformation, System_SystemServices,
# System_Threading, UI_Shell. fonts.rs: Foundation, Graphics_Gdi, UI_WindowsAndMessaging.
windows-sys = { version = "=0.61.2", features = [
    "Win32_Foundation",
    "Win32_Globalization",
    "Win32_Graphics_Gdi",
    "Win32_Security",
    "Win32_Security_Authorization",
    "Win32_Storage_FileSystem",
    "Win32_Storage_Packaging_Appx",
    "Win32_System_Com",
    "Win32_System_SystemInformation",
    "Win32_System_SystemServices",
    "Win32_System_Threading",
    "Win32_UI_Shell",
    "Win32_UI_WindowsAndMessaging",
] }
```
Replace `crates/core/src/adapters/windows/mod.rs` with:
```rust
//! Windows implementations of the adapters (spec §5.2). Each one takes the
//! places it touches as parameters, so tests can aim it at a throwaway
//! registry key and temp folders; `real()` constructors use the user's own.
use super::AdapterError;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

pub mod fs;
pub mod host;
pub mod registry;

/// NUL-terminated UTF-16 for Win32 `PCWSTR` parameters.
pub(crate) fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

pub(crate) fn io_err(op: &str, path: &Path, e: &std::io::Error) -> AdapterError {
    AdapterError::new(format!("{op} {}", path.display()), e.to_string())
}

/// The calling thread's last Win32 error as text.
pub(crate) fn last_error(context: impl Into<String>) -> AdapterError {
    AdapterError::new(context, std::io::Error::last_os_error().to_string())
}
```

- [ ] **Step 2: Write the failing tests** — create `crates/core/src/adapters/windows/host.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_sid_and_known_folders_are_real() {
        let sid = WinHost.user_sid().unwrap();
        assert!(sid.starts_with("S-1-5-"), "{sid}");
        assert!(WinHost.local_app_data().unwrap().is_dir());
        assert!(WinHost.program_data().unwrap().is_dir());
    }

    #[test]
    fn short_path_names_the_same_folder() {
        // Review Focus 4: the VM-C user folder name.
        let tmp = tempfile::tempdir().unwrap();
        let long = tmp.path().join("ผู้ทดสอบ ไทย");
        std::fs::create_dir(&long).unwrap();
        let short = WinHost.short_path(&long).unwrap();
        assert_eq!(std::fs::canonicalize(&short).unwrap(), std::fs::canonicalize(&long).unwrap());
        println!("8.3 names on this volume: {}", short.to_str().is_some_and(str::is_ascii));
        assert!(WinHost.short_path(&tmp.path().join("missing")).is_err());
    }

    #[test]
    fn a_private_folder_is_private_and_a_second_create_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("ai.resincore.cmdstudio-test");
        assert_eq!(WinHost.private_dir_state(&dir).unwrap(), DirState::Absent);
        assert_eq!(WinHost.create_private_dir(&dir).unwrap(), Created::New);
        assert_eq!(WinHost.create_private_dir(&dir).unwrap(), Created::AlreadyExisted);
        assert_eq!(WinHost.private_dir_state(&dir).unwrap(), DirState::Private);
        // Children inherit the protected list (OICI).
        let child = dir.join("clink-1.9.34");
        std::fs::create_dir(&child).unwrap();
        assert_eq!(WinHost.private_dir_state(&child).unwrap(), DirState::Private);
    }

    #[test]
    fn a_folder_others_can_write_is_unsafe() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("everyone");
        create_dir_with_sddl(&dir, "D:P(A;OICI;FA;;;WD)").unwrap();
        assert!(matches!(WinHost.private_dir_state(&dir).unwrap(), DirState::Unsafe(r) if r.contains("S-1-1-0")));
        let file = tmp.path().join("file");
        std::fs::write(&file, b"x").unwrap();
        assert!(matches!(WinHost.private_dir_state(&file).unwrap(), DirState::Unsafe(_)));
    }

    #[test]
    fn a_junction_is_unsafe() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let link = tmp.path().join("link");
        let status = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        assert!(matches!(WinHost.private_dir_state(&link).unwrap(), DirState::Unsafe(r) if r.contains("link")));
    }

    #[test]
    fn judge_follows_the_spec_rule() {
        let me = "S-1-5-21-1-2-3-1001";
        let full = 0x001F_01FF;
        let ok = vec![(me.to_string(), full), (LOCAL_SYSTEM.to_string(), full), (ADMINISTRATORS.to_string(), full)];
        assert_eq!(judge(me, ADMINISTRATORS, Some(&ok)), DirState::Private);
        let mut users_read = ok.clone();
        users_read.push(("S-1-5-32-545".to_string(), 0x0012_00A9));
        assert_eq!(judge(me, me, Some(&users_read)), DirState::Private);
        let mut users_write = ok.clone();
        users_write.push(("S-1-5-32-545".to_string(), 0x0000_0006));
        assert!(matches!(judge(me, me, Some(&users_write)), DirState::Unsafe(_)));
        assert!(matches!(judge(me, "S-1-5-21-9-9-9-1002", Some(&ok)), DirState::Unsafe(_)));
        assert!(matches!(judge(me, me, None), DirState::Unsafe(_)));
    }

    #[test]
    fn system_info_reads_this_machine() {
        let info = WinHost.system_info();
        assert!(info.build.is_some_and(|b| b >= 17763), "{info:?}");
        assert_eq!(info.arch, Arch::X64);
        assert!(!info.packaged, "a test runner is not an MSIX package");
    }
}
```
(`0x0006` is `FILE_WRITE_DATA | FILE_APPEND_DATA`, what `BUILTIN\Users` gets on `%ProgramData%`; `0x0012_00A9` is read and execute.)

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- adapters::windows::host`
Expected: compile errors (`cannot find value WinHost`, …).

- [ ] **Step 4: Implement it above the tests**
```rust
//! Identity, known folders, 8.3 names, the private ProgramData folder
//! (spec §5.4 `<clink-launch>` step 3) and system facts.
use super::registry::RegLocation;
use super::{last_error, wide};
use crate::adapters::{AdapterError, Arch, Created, DefaultTerminal, DirState, Host, SystemInfo};
use std::ffi::{c_void, OsString};
use std::io::ErrorKind;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use windows_sys::core::{GUID, PWSTR};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, APPMODEL_ERROR_NO_PACKAGE, ERROR_ALREADY_EXISTS, HANDLE,
};
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
    SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    AclSizeInformation, GetAce, GetAclInformation, GetTokenInformation, TokenUser, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL,
    ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{CreateDirectoryW, GetShortPathNameW, FILE_ATTRIBUTE_REPARSE_POINT};
use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::SystemInformation::{IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64};
use windows_sys::Win32::System::SystemServices::{ACCESS_ALLOWED_ACE_TYPE, ACCESS_DENIED_ACE_TYPE};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2, OpenProcessToken};
use windows_sys::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_ProgramData, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};

const LOCAL_SYSTEM: &str = "S-1-5-18";
const ADMINISTRATORS: &str = "S-1-5-32-544";

/// Rights that let a SID change what is in a folder or who may: write,
/// append, extended attributes, delete child, attributes, delete, change the
/// access list, take ownership, and the generic forms.
const WRITE_RIGHTS: u32 = 0x0000_0002 // FILE_WRITE_DATA / FILE_ADD_FILE
    | 0x0000_0004 // FILE_APPEND_DATA / FILE_ADD_SUBDIRECTORY
    | 0x0000_0010 // FILE_WRITE_EA
    | 0x0000_0040 // FILE_DELETE_CHILD
    | 0x0000_0100 // FILE_WRITE_ATTRIBUTES
    | 0x0001_0000 // DELETE
    | 0x0004_0000 // WRITE_DAC
    | 0x0008_0000 // WRITE_OWNER
    | 0x1000_0000 // GENERIC_ALL
    | 0x4000_0000; // GENERIC_WRITE

const CONSOLE_STARTUP: &str = r"Console\%%Startup";
const WEBVIEW2_CLIENT: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

#[derive(Clone, Copy, Debug, Default)]
pub struct WinHost;

/// The UTF-16 text a Win32 call returned, up to its NUL.
///
/// # Safety
/// `p` must point to a NUL-terminated UTF-16 string.
unsafe fn from_wide_ptr(p: *const u16) -> OsString {
    let mut len = 0usize;
    // SAFETY: the caller promises a terminating NUL.
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` units before the NUL are readable.
    OsString::from_wide(unsafe { std::slice::from_raw_parts(p, len) })
}

fn known_folder(id: &GUID, name: &str) -> Result<PathBuf, AdapterError> {
    let mut p: PWSTR = std::ptr::null_mut();
    // SAFETY: `p` receives a CoTaskMem string that is freed below in every case.
    let hr = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut p) };
    let out = if hr >= 0 && !p.is_null() {
        // SAFETY: on success `p` is a NUL-terminated path.
        Ok(PathBuf::from(unsafe { from_wide_ptr(p) }))
    } else {
        Err(AdapterError::new(format!("SHGetKnownFolderPath {name}"), format!("HRESULT {hr:#010x}")))
    };
    // SAFETY: the docs require freeing the buffer even when the call failed.
    unsafe { CoTaskMemFree(p as *const c_void) };
    out
}

/// # Safety
/// `sid` must point to a valid SID.
unsafe fn sid_string(sid: PSID) -> Result<String, AdapterError> {
    let mut s: PWSTR = std::ptr::null_mut();
    // SAFETY: valid SID in, LocalAlloc'd string out, freed below.
    if unsafe { ConvertSidToStringSidW(sid, &mut s) } == 0 {
        return Err(last_error("ConvertSidToStringSidW"));
    }
    // SAFETY: success gives a NUL-terminated string.
    let text = unsafe { from_wide_ptr(s) }.to_string_lossy().into_owned();
    // SAFETY: allocated by ConvertSidToStringSidW with LocalAlloc.
    unsafe { LocalFree(s as *mut c_void) };
    Ok(text)
}

fn current_user_sid() -> Result<String, AdapterError> {
    // SAFETY: the token handle is closed before returning; the buffer is sized by
    // the first call and held in u64 words so TOKEN_USER's pointer is aligned.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(last_error("OpenProcessToken"));
        }
        let mut len = 0u32;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len);
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        let ok = GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len);
        CloseHandle(token);
        if ok == 0 {
            return Err(last_error("GetTokenInformation"));
        }
        let user = &*(buf.as_ptr().cast::<TOKEN_USER>());
        sid_string(user.User.Sid)
    }
}

/// Allow-entries of an access list: (SID, rights).
type AllowEntries = Vec<(String, u32)>;

/// The folder's owner SID and its allow-entries; `None` for a NULL access list.
fn owner_and_allowed(dir: &Path) -> Result<(String, Option<AllowEntries>), AdapterError> {
    let path = wide(dir.as_os_str());
    let mut owner: PSID = std::ptr::null_mut();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: out pointers point into `sd`, which is freed with LocalFree below.
    let rc = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut sd,
        )
    };
    if rc != 0 {
        return Err(AdapterError::new(
            format!("GetNamedSecurityInfoW {}", dir.display()),
            std::io::Error::from_raw_os_error(rc as i32).to_string(),
        ));
    }
    let result = (|| {
        // SAFETY: `owner` points into `sd`.
        let owner = unsafe { sid_string(owner) }?;
        if dacl.is_null() {
            return Ok((owner, None));
        }
        let mut info = ACL_SIZE_INFORMATION { AceCount: 0, AclBytesInUse: 0, AclBytesFree: 0 };
        // SAFETY: `dacl` points into `sd`; `info` is the matching struct.
        let ok = unsafe {
            GetAclInformation(
                dacl,
                (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
                std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        };
        if ok == 0 {
            return Err(last_error("GetAclInformation"));
        }
        let mut allowed = Vec::new();
        for i in 0..info.AceCount {
            let mut ace: *mut c_void = std::ptr::null_mut();
            // SAFETY: index below AceCount; `ace` points into the ACL.
            if unsafe { GetAce(dacl, i, &mut ace) } == 0 {
                return Err(last_error("GetAce"));
            }
            // SAFETY: every ACE starts with an ACE_HEADER.
            let header = unsafe { *(ace as *const ACE_HEADER) };
            match u32::from(header.AceType) {
                ACCESS_ALLOWED_ACE_TYPE => {
                    // SAFETY: type 0 is ACCESS_ALLOWED_ACE; the SID starts at SidStart.
                    let entry = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
                    let sid = unsafe { sid_string(&entry.SidStart as *const u32 as PSID) }?;
                    allowed.push((sid, entry.Mask));
                }
                ACCESS_DENIED_ACE_TYPE => {}
                other => allowed.push((format!("<ACE type {other}>"), u32::MAX)),
            }
        }
        Ok((owner, Some(allowed)))
    })();
    // SAFETY: allocated by GetNamedSecurityInfoW.
    unsafe { LocalFree(sd) };
    result
}

/// Creates `dir` with the security descriptor `sddl` in one call.
pub(crate) fn create_dir_with_sddl(dir: &Path, sddl: &str) -> Result<Created, AdapterError> {
    let text = wide(std::ffi::OsStr::new(sddl));
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: `sd` is LocalAlloc'd by the call and freed below.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(last_error("ConvertStringSecurityDescriptorToSecurityDescriptorW"));
    }
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd,
        bInheritHandle: 0,
    };
    let path = wide(dir.as_os_str());
    // SAFETY: `path` and `sa` live across the call.
    let ok = unsafe { CreateDirectoryW(path.as_ptr(), &sa) };
    // SAFETY: read right after the failing call, before anything else runs.
    let code = unsafe { GetLastError() };
    // SAFETY: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
    unsafe { LocalFree(sd) };
    match (ok, code) {
        (0, ERROR_ALREADY_EXISTS) => Ok(Created::AlreadyExisted),
        (0, _) => Err(AdapterError::new(
            format!("CreateDirectoryW {}", dir.display()),
            std::io::Error::from_raw_os_error(code as i32).to_string(),
        )),
        _ => Ok(Created::New),
    }
}

/// Owner in {user, SYSTEM, Administrators} and nobody else may write (spec §5.4).
pub(crate) fn judge(me: &str, owner: &str, allowed: Option<&[(String, u32)]>) -> DirState {
    let trusted = |sid: &str| sid == me || sid == LOCAL_SYSTEM || sid == ADMINISTRATORS;
    if !trusted(owner) {
        return DirState::Unsafe(format!("owned by {owner}"));
    }
    let Some(allowed) = allowed else {
        return DirState::Unsafe("it has no access list, so everyone has full access".into());
    };
    match allowed.iter().find(|(sid, mask)| mask & WRITE_RIGHTS != 0 && !trusted(sid)) {
        Some((sid, _)) => DirState::Unsafe(format!("{sid} may change it")),
        None => DirState::Private,
    }
}

fn read_text(loc: &RegLocation, name: &str) -> Option<String> {
    loc.read_raw(name).ok().flatten().and_then(|v| v.text())
}

fn default_terminal() -> DefaultTerminal {
    match read_text(&RegLocation::user(CONSOLE_STARTUP), "DelegationConsole").map(|s| s.to_ascii_uppercase()).as_deref()
    {
        None | Some("{00000000-0000-0000-0000-000000000000}") => DefaultTerminal::LetWindowsDecide,
        Some("{2EACA947-7F5F-4CFA-BA87-8F7FBEEFBE69}") => DefaultTerminal::WindowsTerminal,
        Some("{06EC847C-C0A5-46B8-92CB-7C92F6E35CD5}") => DefaultTerminal::WindowsTerminalPreview,
        Some("{B23D10C0-E52E-411E-9D5B-C09FDF709C7D}") => DefaultTerminal::Conhost,
        Some(_) => DefaultTerminal::Other,
    }
}

fn webview2() -> Option<String> {
    let keys = [
        RegLocation::machine(&format!(r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT}")),
        RegLocation::user(&format!(r"Software\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT}")),
    ];
    keys.iter().filter_map(|k| read_text(k, "pv")).find(|v| !v.is_empty() && v != "0.0.0.0")
}

fn arch() -> Arch {
    let (mut process, mut native) = (0u16, 0u16);
    // SAFETY: two out-parameters for the current process.
    if unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) } == 0 {
        return Arch::Other;
    }
    match native {
        IMAGE_FILE_MACHINE_AMD64 => Arch::X64,
        IMAGE_FILE_MACHINE_ARM64 => Arch::Arm64,
        _ => Arch::Other,
    }
}

fn packaged() -> bool {
    let mut len = 0u32;
    // SAFETY: a length query with no buffer.
    unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) != APPMODEL_ERROR_NO_PACKAGE }
}

impl Host for WinHost {
    fn local_app_data(&self) -> Result<PathBuf, AdapterError> {
        known_folder(&FOLDERID_LocalAppData, "LocalAppData")
    }

    fn program_data(&self) -> Result<PathBuf, AdapterError> {
        known_folder(&FOLDERID_ProgramData, "ProgramData")
    }

    fn user_sid(&self) -> Result<String, AdapterError> {
        current_user_sid()
    }

    fn short_path(&self, path: &Path) -> Result<PathBuf, AdapterError> {
        let long = wide(path.as_os_str());
        // SAFETY: a size query, then a call with a buffer of that size.
        let need = unsafe { GetShortPathNameW(long.as_ptr(), std::ptr::null_mut(), 0) };
        if need == 0 {
            return Err(last_error(format!("GetShortPathNameW {}", path.display())));
        }
        let mut buf = vec![0u16; need as usize];
        let got = unsafe { GetShortPathNameW(long.as_ptr(), buf.as_mut_ptr(), need) };
        if got == 0 || got >= need {
            return Err(last_error(format!("GetShortPathNameW {}", path.display())));
        }
        buf.truncate(got as usize);
        Ok(PathBuf::from(OsString::from_wide(&buf)))
    }

    fn private_dir_state(&self, dir: &Path) -> Result<DirState, AdapterError> {
        let meta = match std::fs::symlink_metadata(dir) {
            Ok(m) => m,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(DirState::Absent),
            Err(e) => return Err(super::io_err("inspect", dir, &e)),
        };
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Ok(DirState::Unsafe("it is a link to another place".into()));
        }
        if !meta.is_dir() {
            return Ok(DirState::Unsafe("it is not a folder".into()));
        }
        let me = current_user_sid()?;
        let (owner, allowed) = owner_and_allowed(dir)?;
        Ok(judge(&me, &owner, allowed.as_deref()))
    }

    fn create_private_dir(&self, dir: &Path) -> Result<Created, AdapterError> {
        let sid = current_user_sid()?;
        create_dir_with_sddl(dir, &format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"))
    }

    fn system_info(&self) -> SystemInfo {
        let build =
            read_text(&RegLocation::machine(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"), "CurrentBuildNumber")
                .and_then(|b| b.trim().parse().ok());
        SystemInfo {
            build,
            arch: arch(),
            default_terminal: default_terminal(),
            webview2: webview2(),
            packaged: packaged(),
            // SAFETY: no arguments.
            ui_language: unsafe { GetUserDefaultUILanguage() },
        }
    }
}
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- adapters::windows::host --nocapture`
Expected: `7 passed`, and a line `8.3 names on this volume: true` or `false` (both are fine: `GetShortPathNameW` returns the long path when the volume makes no 8.3 names, and the test only checks both name the same folder).

- [ ] **Step 6: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 7: Commit**

Message:
```text
feat(core): add the host adapter and the private ProgramData folder

WinHost reads the user's SID, LocalAppData and ProgramData, the 8.3
form of a path, and system facts (build, arch, default terminal,
WebView2, packaged, UI language). The per-user ProgramData folder is
created with its protected security descriptor in one CreateDirectoryW
call; an existing folder counts as private only when the user, SYSTEM
or Administrators own it, nobody else may write to it, and it is not
a junction or symlink (spec 5.4 step 3).
```
Run:
```bash
git add -- crates/core/Cargo.toml Cargo.lock crates/core/src/adapters/windows/mod.rs crates/core/src/adapters/windows/host.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 6: The fonts adapter (`WinFonts`)

**Files:**
- Create: `crates/core/src/adapters/windows/fonts.rs`
- Modify: `crates/core/src/adapters/windows/mod.rs` (`pub mod fonts;`)

**Interfaces:**
- Consumes: `adapters::{AdapterError, Fonts, RegValue, REG_SZ}` (Task 1); `registry::{RegLocation, TestKey}` with `RegLocation::{read_raw, write_raw, delete_value, exists, delete_if_empty}` (Task 4); `windows::{wide, last_error}` and `host::WinHost` (Task 5, the last only in the system test).
- Produces (in `cmdstudio_core::adapters::windows::fonts`):
  - `FONTS_SUBKEY = r"Software\Microsoft\Windows NT\CurrentVersion\Fonts"`.
  - `WinFonts` with `WinFonts::real(local_app_data: &Path)` (folder `<local_app_data>\Microsoft\Windows\Fonts`, `HKCU\FONTS_SUBKEY`, session on) and `WinFonts::at(dir: PathBuf, reg: RegLocation, session: bool)`; implements `Fonts`.

**Win32 calls, exactly** (G12, `spikes/windows/fonts.ps1`): families — `GetDC(null)`, `EnumFontFamiliesExW(hdc, &LOGFONTW { lfCharSet: DEFAULT_CHARSET, .. }, Some(callback), &mut Vec<String> as LPARAM, 0)`, `ReleaseDC`; the callback copies `lfFaceName` up to its NUL and returns 1; names starting with `@` (vertical CJK twins) are dropped; sorted, deduplicated. Load — `AddFontResourceW(path)` (0 = failure), then `SendMessageTimeoutW(HWND_BROADCAST, WM_FONTCHANGE, 0, 0, SMTO_ABORTIFHUNG, 1000, &mut result)`. Unload — `RemoveFontResourceW(path)` (FALSE only means it was not loaded) and the same broadcast. Registration — a `REG_SZ` value named `<family> (TrueType)` whose data is the font file's full path, in a key a new account may not have yet; `delete_registration_key_if_empty` removes that key again once it holds nothing (undo, Task 11). With `session: false`, load and unload do nothing: `AddFontResourceW` and the broadcast reach every program the developer has open, so ordinary tests never use them (brief, test safety).

- [ ] **Step 1: Write the failing tests**

Add `pub mod fonts;` to `crates/core/src/adapters/windows/mod.rs`. Create `crates/core/src/adapters/windows/fonts.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::windows::registry::TestKey;

    #[test]
    fn families_lists_gdi_names_without_vertical_twins() {
        let f = WinFonts::at(PathBuf::from(r"C:\unused"), RegLocation::user(r"Software\CmdStudioTest\unused"), false);
        let names = f.families().unwrap();
        assert!(names.iter().any(|n| n == "Arial"), "{names:?}");
        assert!(names.iter().all(|n| !n.starts_with('@')));
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted);
    }

    #[test]
    fn registration_values_live_in_the_given_key() {
        // Review Focus 4: a Thai user folder in the registration data. A new account
        // may not have the key; it goes again once empty.
        let t = TestKey::new();
        let f = WinFonts::at(PathBuf::from(r"C:\unused"), t.location("Fonts"), false);
        let name = "CaskaydiaCove NF (TrueType)";
        assert_eq!(f.registration(name).unwrap(), None);
        assert!(!f.registration_key_exists().unwrap());
        f.register(name, r"C:\Users\ผู้ใช้\AppData\Local\Microsoft\Windows\Fonts\a.ttf").unwrap();
        assert_eq!(
            f.registration(name).unwrap().as_deref(),
            Some(r"C:\Users\ผู้ใช้\AppData\Local\Microsoft\Windows\Fonts\a.ttf")
        );
        assert!(f.registration_key_exists().unwrap());
        f.delete_registration_key_if_empty().unwrap();
        assert!(f.registration_key_exists().unwrap(), "a key with values stays");
        f.unregister(name).unwrap();
        f.unregister(name).unwrap();
        assert_eq!(f.registration(name).unwrap(), None);
        f.delete_registration_key_if_empty().unwrap();
        assert!(!f.registration_key_exists().unwrap());
    }

    #[test]
    fn test_mode_never_touches_the_session_font_table() {
        let f = WinFonts::at(PathBuf::from(r"C:\unused"), RegLocation::user(r"Software\CmdStudioTest\unused"), false);
        // The file does not exist; with the session on, AddFontResourceW would fail.
        f.load(Path::new(r"C:\no\such\font.ttf")).unwrap();
        f.unload(Path::new(r"C:\no\such\font.ttf")).unwrap();
    }

    /// Real-location check (G12): installs the bundled CaskaydiaCove NF for
    /// this user, sees GDI list it, then removes it. Runs only with
    /// CMDSTUDIO_SYSTEM_TESTS=1 (CI and test VMs); needs resources\ filled by
    /// scripts\fetch-resources.ps1.
    #[test]
    fn system_real_font_install_and_remove() {
        if std::env::var_os("CMDSTUDIO_SYSTEM_TESTS").is_none() {
            println!("SKIPPED system_real_font_install_and_remove: set CMDSTUDIO_SYSTEM_TESTS=1");
            return;
        }
        use crate::adapters::Host;
        let family = "CaskaydiaCove NF";
        let src =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(r"..\..\resources\fonts\CaskaydiaCoveNerdFont-Regular.ttf");
        let f = WinFonts::real(&crate::adapters::windows::host::WinHost.local_app_data().unwrap());
        assert!(
            !f.families().unwrap().iter().any(|n| n == family),
            "{family} is already installed; run this on a machine without it"
        );
        let dest = f.user_fonts_dir().join("CaskaydiaCoveNerdFont-Regular.ttf");
        std::fs::create_dir_all(f.user_fonts_dir()).unwrap();
        std::fs::copy(&src, &dest).unwrap();
        f.register("CaskaydiaCove NF (TrueType)", dest.to_str().unwrap()).unwrap();
        f.load(&dest).unwrap();
        let seen = f.families().unwrap().iter().any(|n| n == family);
        f.unload(&dest).unwrap();
        f.unregister("CaskaydiaCove NF (TrueType)").unwrap();
        std::fs::remove_file(&dest).unwrap();
        assert!(seen, "GDI did not list {family} after the install");
        assert!(!f.families().unwrap().iter().any(|n| n == family));
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- adapters::windows::fonts`
Expected: compile errors (`cannot find type WinFonts`).

- [ ] **Step 3: Implement it above the tests**
```rust
//! Per-user fonts (spec §5.4 `Fonts`, G12): the file lives in
//! `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, the registration value in
//! `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts`, and
//! `AddFontResourceW` plus a `WM_FONTCHANGE` broadcast make it usable now.
use super::registry::RegLocation;
use super::{last_error, wide};
use crate::adapters::{AdapterError, Fonts};
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::LPARAM;
use windows_sys::Win32::Graphics::Gdi::{
    AddFontResourceW, EnumFontFamiliesExW, GetDC, ReleaseDC, RemoveFontResourceW, DEFAULT_CHARSET, LOGFONTW,
    TEXTMETRICW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_FONTCHANGE,
};

pub const FONTS_SUBKEY: &str = r"Software\Microsoft\Windows NT\CurrentVersion\Fonts";

#[derive(Clone, Debug)]
pub struct WinFonts {
    dir: PathBuf,
    reg: RegLocation,
    /// When false, `load`/`unload` leave the session's font table alone.
    /// Tests use false: `AddFontResourceW` and the broadcast reach every
    /// program the developer has open.
    session: bool,
}

impl WinFonts {
    /// The user's own fonts folder and registration key, with session loading on.
    pub fn real(local_app_data: &Path) -> Self {
        WinFonts {
            dir: local_app_data.join("Microsoft").join("Windows").join("Fonts"),
            reg: RegLocation::user(FONTS_SUBKEY),
            session: true,
        }
    }

    /// Other places, for tests.
    pub fn at(dir: PathBuf, reg: RegLocation, session: bool) -> Self {
        WinFonts { dir, reg, session }
    }
}

unsafe extern "system" fn collect(lf: *const LOGFONTW, _tm: *const TEXTMETRICW, _kind: u32, lparam: LPARAM) -> i32 {
    // SAFETY: `lparam` is the `&mut Vec<String>` passed by `families` below and
    // outlives the EnumFontFamiliesExW call; `lf` is valid during the callback.
    let (names, face) = unsafe { (&mut *(lparam as *mut Vec<String>), (*lf).lfFaceName) };
    let units: Vec<u16> = face.iter().copied().take_while(|c| *c != 0).collect();
    names.push(String::from_utf16_lossy(&units));
    1
}

/// Tells every top-level window that the font list changed; hung windows are skipped after 1 s.
fn broadcast() {
    let mut result = 0usize;
    // SAFETY: plain message send with no pointers; the out value is a local.
    unsafe { SendMessageTimeoutW(HWND_BROADCAST, WM_FONTCHANGE, 0, 0, SMTO_ABORTIFHUNG, 1000, &mut result) };
}

impl Fonts for WinFonts {
    fn families(&self) -> Result<Vec<String>, AdapterError> {
        let mut names: Vec<String> = Vec::new();
        // SAFETY: a screen DC is fetched and released here; LOGFONTW is plain data;
        // the callback only touches `names`, which lives across the call.
        unsafe {
            let hdc = GetDC(std::ptr::null_mut());
            if hdc.is_null() {
                return Err(last_error("GetDC"));
            }
            let mut lf: LOGFONTW = std::mem::zeroed();
            lf.lfCharSet = DEFAULT_CHARSET;
            EnumFontFamiliesExW(hdc, &lf, Some(collect), &mut names as *mut Vec<String> as LPARAM, 0);
            ReleaseDC(std::ptr::null_mut(), hdc);
        }
        // '@' names are the vertical-writing twins of CJK fonts.
        names.retain(|n| !n.starts_with('@') && !n.is_empty());
        names.sort();
        names.dedup();
        Ok(names)
    }

    fn user_fonts_dir(&self) -> PathBuf {
        self.dir.clone()
    }

    fn registration(&self, value_name: &str) -> Result<Option<String>, AdapterError> {
        Ok(self.reg.read_raw(value_name)?.and_then(|v| v.text()))
    }

    fn register(&self, value_name: &str, file: &str) -> Result<(), AdapterError> {
        self.reg.write_raw(value_name, &crate::adapters::RegValue::from_text(crate::adapters::REG_SZ, file))
    }

    fn unregister(&self, value_name: &str) -> Result<(), AdapterError> {
        self.reg.delete_value(value_name)
    }

    fn registration_key_exists(&self) -> Result<bool, AdapterError> {
        self.reg.exists()
    }

    fn delete_registration_key_if_empty(&self) -> Result<(), AdapterError> {
        self.reg.delete_if_empty()
    }

    fn load(&self, file: &Path) -> Result<(), AdapterError> {
        if !self.session {
            return Ok(());
        }
        let path = wide(file.as_os_str());
        // SAFETY: `path` is a NUL-terminated UTF-16 string that outlives the call.
        let added = unsafe { AddFontResourceW(path.as_ptr()) };
        if added == 0 {
            return Err(last_error(format!("AddFontResourceW {}", file.display())));
        }
        broadcast();
        Ok(())
    }

    fn unload(&self, file: &Path) -> Result<(), AdapterError> {
        if !self.session {
            return Ok(());
        }
        let path = wide(file.as_os_str());
        // SAFETY: as in `load`. FALSE only means the font was not loaded, which is the goal.
        unsafe { RemoveFontResourceW(path.as_ptr()) };
        broadcast();
        Ok(())
    }
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- adapters::windows::fonts --nocapture`
Expected: `4 passed`, with `SKIPPED system_real_font_install_and_remove: set CMDSTUDIO_SYSTEM_TESTS=1`.

- [ ] **Step 5: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 6: Commit**

Message:
```text
feat(core): add the per-user fonts adapter

Families come from EnumFontFamiliesExW (GDI names, as G12 found:
"CaskaydiaCove NF"). Install and removal follow G12: the HKCU Fonts
value, AddFontResourceW / RemoveFontResourceW and a WM_FONTCHANGE
broadcast. Tests leave the session font table alone; the real install
runs only with CMDSTUDIO_SYSTEM_TESTS=1.
```
Run:
```bash
git add -- crates/core/src/adapters/windows/mod.rs crates/core/src/adapters/windows/fonts.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 7: Windows Terminal settings — the CST editor (`wt_edit`) and where the files are (`WinWt`)

**Files:**
- Create: `crates/core/src/wt_edit.rs`, `crates/core/src/adapters/windows/wt.rs`, `crates/core/tests/fixtures/wt/commented.jsonc`, `default.json`, `no-cmd-profile.jsonc`, `legacy-profiles-array.jsonc`, `user-cmd-profile.jsonc`, `broken.jsonc` (all in that folder)
- Modify: `crates/core/Cargo.toml` (`jsonc-parser`), `crates/core/src/lib.rs` (`pub mod wt_edit;`), `crates/core/src/adapters/windows/mod.rs` (`pub mod wt;`), `Cargo.lock`

**Interfaces:**
- Consumes: `generate::wt::{CMD_PROFILE_GUID, SCHEME_NAME}` (Phase 1a); `adapters::{WtLocation, WtSettings, WtVariant}` (Task 1).
- Produces (in `cmdstudio_core::wt_edit`):
  - `wt_parse_options() -> jsonc_parser::ParseOptions` — comments and trailing commas only.
  - `WtKey::{Scheme, DefaultProfile, Profile { key: String }, ProfileSub { key: String, sub: String }}` — `Ord` (a parent `Profile{font}` sorts before `ProfileSub{font, face}`), serde `{"at": "profileSub", "key": …, "sub": …}`.
  - `WtChange { key: WtKey, original: Option<Value>, written: Value }`.
  - `WtCreated::{CmdProfile, ProfileList, Profiles, Schemes}` (`Copy + Ord`, serde camelCase).
  - `WtEdited { text: String, changes: Vec<WtChange>, created: Vec<WtCreated> }`.
  - `WtEdit<'a> { scheme: &'a Value, profile: &'a Map<String, Value>, set_default: bool }` (`Copy`).
  - `WtError::{Parse(String), Shape(String)}`; `code()` is always `"E_WT_PARSE"`.
  - `to_cst(&Value) -> CstInputValue`; `check_parses(&str) -> Result<(), WtError>`; `apply_edit(&str, &WtEdit) -> Result<WtEdited, WtError>`; `read_value(&str, &WtKey) -> Result<Option<Value>, WtError>`; `revert(&str, originals: &BTreeMap<WtKey, Option<Value>>, written: &BTreeMap<WtKey, Value>, created: &[WtCreated]) -> Result<String, WtError>`.
- Produces (in `cmdstudio_core::adapters::windows::wt`): `STORE_PACKAGE`, `PREVIEW_PACKAGE`, `WinWt::new(local_app_data: PathBuf)` implementing `WtSettings` — three locations in spec §5.4 order, `installed` = the variant's own folder exists.

**What the editor does** (spec §5.4; G9 lists the cases this must handle):
1. A UTF-8 BOM is split off before parsing and put back after; CRLF and every byte outside the touched values stay (the CST keeps them).
2. Parse with `wt_parse_options()`: comments and trailing commas, nothing looser (single quotes, hex numbers, missing commas → `E_WT_PARSE`). The top level must be an object (`Shape`). A file that fails here is never written.
3. Scheme: every element of `schemes` named `CMD Studio` is replaced by ours (WT uses the last of duplicate names, so all are replaced); none → append. A missing `schemes` is created and recorded as `WtCreated::Schemes`. A `schemes` that is not a list → `Shape`.
4. cmd profile: `profiles.list`, or the legacy form where `profiles` is the list itself; the profile is the element whose `guid` equals `{0caa0dad-35be-5f56-a8ff-afceeeaa6101}` ignoring ASCII case. Missing → append `{ "guid", "name": "Command Prompt", …our keys }` (`CmdProfile`), creating `profiles`/`list` when absent (`Profiles`, `ProfileList`). Present → each key is set; an object value (`font`) merges key by key into an existing object so `font.weight` survives, recorded as `ProfileSub`. A value that already equals ours is not rewritten, so re-applying gives the same bytes.
5. `defaultProfile` only when `set_default`, unless it already names the cmd guid (any case).
6. Every touched key is recorded with its value before (`None` = absent) and the value written.
7. Revert (key by key, spec §6.3 step 2): every `CMD Studio` scheme goes; a key whose current value still equals what CMD Studio last wrote gets its original back (or is removed when it was absent); a key the user changed since stays; then created containers go if nothing of the user's is left (`CmdProfile` when only `guid` and `name` remain; `list`, `profiles`, `schemes` when empty). An untouched file reverts to its original bytes, except that a one-line object CMD Studio added a key to (`"font": { … }`) keeps the multi-line layout the library gave it — the values come back. Undo restores whole-file bytes, so it is always exact.

- [ ] **Step 1: Add the dependency and the fixtures**

In `crates/core/Cargo.toml` under `[dependencies]` add (G9; Implementation Gate: the `serde_json` feature is what adds `to_serde_value()` to CST nodes; `ParseOptions` in 0.34.0 has exactly the ten fields used below, and its `Default` turns all of them on):
```toml
jsonc-parser = { version = "=0.34.0", features = ["cst", "serde_json"] }
```
Add `pub mod wt_edit;` to `crates/core/src/lib.rs` and `pub mod wt;` to `crates/core/src/adapters/windows/mod.rs`.

Create the fixtures (LF line endings; `.gitattributes` keeps them LF). `commented.jsonc` is the Phase 0 spike fixture, copied byte for byte: `Copy-Item spikes\jsonc-cst\fixtures\settings.jsonc crates\core\tests\fixtures\wt\commented.jsonc`. Its content, for reference:
```jsonc
// User's own comment at the top
{
    "$schema": "https://aka.ms/terminal-profiles-schema",
    "defaultProfile": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
    "profiles":
    {
        "defaults": {},
        "list":
        [
            {
                "guid": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
                "name": "Windows PowerShell" // inline comment
            },
            {
                "commandline": "%SystemRoot%\\System32\\cmd.exe",
                "guid": "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}",
                "hidden": false,
                "name": "Command Prompt",
            }
        ]
    },
    /* block comment the user wrote */
    "schemes": []
}
```
`default.json` — the shape of the file Windows Terminal 1.24 writes on first start (sorted keys; `"key":` then a newline before `[`/`{`):
```json
{
    "$help": "https://aka.ms/terminal-documentation",
    "$schema": "https://aka.ms/terminal-profiles-schema",
    "actions":
    [
        {
            "command":
            {
                "action": "copy",
                "singleLine": false
            },
            "id": "User.copy.644BA8F2"
        },
        {
            "command": "paste",
            "id": "User.paste"
        }
    ],
    "copyFormatting": "none",
    "copyOnSelect": false,
    "defaultProfile": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
    "keybindings":
    [
        {
            "id": "User.copy.644BA8F2",
            "keys": "ctrl+c"
        },
        {
            "id": "User.paste",
            "keys": "ctrl+v"
        }
    ],
    "newTabMenu":
    [
        {
            "type": "remainingProfiles"
        }
    ],
    "profiles":
    {
        "defaults": {},
        "list":
        [
            {
                "commandline": "%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
                "guid": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
                "hidden": false,
                "name": "Windows PowerShell"
            },
            {
                "commandline": "%SystemRoot%\\System32\\cmd.exe",
                "guid": "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}",
                "hidden": false,
                "name": "Command Prompt"
            },
            {
                "guid": "{b453ae62-4e3d-5e58-b989-0a998ec441b8}",
                "hidden": false,
                "name": "Azure Cloud Shell",
                "source": "Windows.Terminal.Azure"
            }
        ]
    },
    "schemes": [],
    "themes": []
}
```
No line in the fixtures ends with a space.

`no-cmd-profile.jsonc`:
```jsonc
// A user who deleted the cmd profile and never had schemes.
{
    "defaultProfile": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
    "profiles": {
        "list": [
            {
                "guid": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
                "name": "Windows PowerShell"
            }
        ]
    }
}
```
`legacy-profiles-array.jsonc`:
```jsonc
{
    "defaultProfile": "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}",
    // Old form: "profiles" is the list itself.
    "profiles": [
        {
            "guid": "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}",
            "name": "cmd",
            "commandline": "cmd.exe"
        }
    ],
    "schemes": [
        { "name": "Campbell", "background": "#0C0C0C" }
    ]
}
```
`user-cmd-profile.jsonc`:
```jsonc
{
  "defaultProfile": "{0CAA0DAD-35BE-5F56-A8FF-AFCEEEAA6101}",
  "profiles": {
    "defaults": { "fontSize": 11 },
    "list": [
      {
        // The user's own cmd profile, uppercase guid, two-space indent.
        "guid": "{0CAA0DAD-35BE-5F56-A8FF-AFCEEEAA6101}",
        "name": "My cmd",
        "colorScheme": "Campbell",
        "font": { "face": "Consolas", "weight": "bold" },
        "commandline": "%SystemRoot%\\System32\\cmd.exe",
      },
    ],
  },
  "schemes": [
    { "name": "Campbell", "background": "#0C0C0C" },
  ],
}
```
`broken.jsonc` (cut off mid-file, as after a crash; three lines and a final newline):
```text
{
    "profiles": {
        "list": [
```

- [ ] **Step 2: Write the failing tests** — create `crates/core/src/wt_edit.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const COMMENTED: &str = include_str!("../tests/fixtures/wt/commented.jsonc");
    const DEFAULT: &str = include_str!("../tests/fixtures/wt/default.json");
    const NO_CMD: &str = include_str!("../tests/fixtures/wt/no-cmd-profile.jsonc");
    const LEGACY: &str = include_str!("../tests/fixtures/wt/legacy-profiles-array.jsonc");
    const USER_CMD: &str = include_str!("../tests/fixtures/wt/user-cmd-profile.jsonc");
    const BROKEN: &str = include_str!("../tests/fixtures/wt/broken.jsonc");

    fn scheme() -> Value {
        json!({ "name": "CMD Studio", "background": "#1f1e1d", "foreground": "#faf9f5" })
    }

    fn profile() -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("colorScheme".into(), json!("CMD Studio"));
        m.insert("commandline".into(), json!(r"%SystemRoot%\System32\cmd.exe"));
        m.insert("font".into(), json!({ "face": "Cascadia Mono", "size": 12 }));
        m.insert("opacity".into(), json!(96));
        m
    }

    fn edit<'a>(scheme: &'a Value, profile: &'a Map<String, Value>, set_default: bool) -> WtEdit<'a> {
        WtEdit { scheme, profile, set_default }
    }

    fn originals(e: &WtEdited) -> BTreeMap<WtKey, Option<Value>> {
        e.changes.iter().map(|c| (c.key.clone(), c.original.clone())).collect()
    }

    fn written(e: &WtEdited) -> BTreeMap<WtKey, Value> {
        e.changes.iter().map(|c| (c.key.clone(), c.written.clone())).collect()
    }

    #[test]
    fn comments_and_untouched_bytes_survive() {
        let (s, p) = (scheme(), profile());
        let out = apply_edit(COMMENTED, &edit(&s, &p, false)).unwrap().text;
        assert!(out.contains("// User's own comment at the top"));
        assert!(out.contains("\"name\": \"Windows PowerShell\" // inline comment"));
        assert!(out.contains("/* block comment the user wrote */"));
        assert!(out.starts_with("// User's own comment at the top\n{\n    \"$schema\""));
        assert_eq!(read_value(&out, &WtKey::Profile { key: "colorScheme".into() }).unwrap(), Some(json!("CMD Studio")));
        assert_eq!(read_value(&out, &WtKey::Scheme).unwrap(), Some(s));
    }

    #[test]
    fn reapplying_the_same_values_changes_nothing() {
        let (s, p) = (scheme(), profile());
        let once = apply_edit(DEFAULT, &edit(&s, &p, true)).unwrap().text;
        let twice = apply_edit(&once, &edit(&s, &p, true)).unwrap().text;
        assert_eq!(once, twice);
    }

    #[test]
    fn an_existing_cmd_studio_scheme_is_replaced_not_duplicated() {
        let (s, p) = (scheme(), profile());
        let first = apply_edit(DEFAULT, &edit(&s, &p, false)).unwrap().text;
        let s2 = json!({ "name": "CMD Studio", "background": "#000000", "foreground": "#ffffff" });
        let second = apply_edit(&first, &edit(&s2, &p, false)).unwrap();
        assert_eq!(second.text.matches("\"CMD Studio\"").count(), first.matches("\"CMD Studio\"").count());
        assert_eq!(read_value(&second.text, &WtKey::Scheme).unwrap(), Some(s2));
        assert_eq!(second.changes.first().unwrap().original, Some(s));
    }

    #[test]
    fn a_missing_cmd_profile_is_created_and_reverted_away() {
        let (s, p) = (scheme(), profile());
        let e = apply_edit(NO_CMD, &edit(&s, &p, false)).unwrap();
        assert!(e.created.contains(&WtCreated::CmdProfile));
        assert!(e.text.contains("\"schemes\""));
        assert_eq!(read_value(&e.text, &WtKey::Profile { key: "name".into() }).unwrap(), Some(json!("Command Prompt")));
        let back = revert(&e.text, &originals(&e), &written(&e), &e.created).unwrap();
        assert!(!back.contains("0caa0dad"));
        assert!(!back.contains("CMD Studio"));
        jsonc_parser::parse_to_value(&back, &wt_parse_options()).unwrap();
    }

    #[test]
    fn legacy_profiles_array_is_edited_in_place() {
        let (s, p) = (scheme(), profile());
        let e = apply_edit(LEGACY, &edit(&s, &p, false)).unwrap();
        assert!(!e.created.contains(&WtCreated::CmdProfile));
        assert_eq!(read_value(&e.text, &WtKey::Profile { key: "opacity".into() }).unwrap(), Some(json!(96)));
    }

    #[test]
    fn font_merges_into_the_users_font_object() {
        let (s, p) = (scheme(), profile());
        let e = apply_edit(USER_CMD, &edit(&s, &p, false)).unwrap();
        assert_eq!(
            read_value(&e.text, &WtKey::ProfileSub { key: "font".into(), sub: "weight".into() }).unwrap(),
            Some(json!("bold"))
        );
        assert_eq!(
            read_value(&e.text, &WtKey::ProfileSub { key: "font".into(), sub: "face".into() }).unwrap(),
            Some(json!("Cascadia Mono"))
        );
        assert!(e.changes.iter().any(|c| c.key == WtKey::ProfileSub { key: "font".into(), sub: "face".into() }
            && c.original == Some(json!("Consolas"))));
    }

    #[test]
    fn uppercase_guid_finds_the_users_profile() {
        let (s, p) = (scheme(), profile());
        let e = apply_edit(USER_CMD, &edit(&s, &p, false)).unwrap();
        assert!(!e.created.contains(&WtCreated::CmdProfile));
        assert_eq!(read_value(&e.text, &WtKey::Profile { key: "name".into() }).unwrap(), Some(json!("My cmd")));
    }

    #[test]
    fn bom_and_crlf_are_kept() {
        // Review Focus 3.
        let (s, p) = (scheme(), profile());
        let crlf = format!("\u{feff}{}", COMMENTED.replace('\n', "\r\n"));
        let out = apply_edit(&crlf, &edit(&s, &p, true)).unwrap().text;
        assert!(out.starts_with('\u{feff}'));
        assert!(!out.replace("\r\n", "").contains('\n'), "a bare LF slipped in");
    }

    #[test]
    fn unparseable_files_are_refused() {
        let (s, p) = (scheme(), profile());
        let err = apply_edit(BROKEN, &edit(&s, &p, false)).unwrap_err();
        assert_eq!(err.code(), "E_WT_PARSE");
        assert!(matches!(apply_edit("[]", &edit(&s, &p, false)), Err(WtError::Shape(_))));
        assert!(matches!(apply_edit("{'a': 1}", &edit(&s, &p, false)), Err(WtError::Parse(_))));
        assert!(matches!(apply_edit(r#"{"profiles": 3}"#, &edit(&s, &p, false)), Err(WtError::Shape(_))));
    }

    #[test]
    fn default_profile_is_set_only_on_request_and_recorded() {
        let (s, p) = (scheme(), profile());
        let off = apply_edit(DEFAULT, &edit(&s, &p, false)).unwrap();
        assert!(off.text.contains("\"defaultProfile\": \"{61c54bbd-c2c6-5271-96e7-009a87ff44bf}\""));
        let on = apply_edit(DEFAULT, &edit(&s, &p, true)).unwrap();
        assert_eq!(read_value(&on.text, &WtKey::DefaultProfile).unwrap(), Some(json!(CMD_PROFILE_GUID)));
        assert!(on
            .changes
            .iter()
            .any(|c| c.key == WtKey::DefaultProfile
                && c.original == Some(json!("{61c54bbd-c2c6-5271-96e7-009a87ff44bf}"))));
    }

    #[test]
    fn revert_of_an_untouched_edit_gives_back_the_original_bytes() {
        let (s, p) = (scheme(), profile());
        for (name, fixture) in [("default", DEFAULT), ("commented", COMMENTED), ("legacy", LEGACY), ("nocmd", NO_CMD)] {
            let e = apply_edit(fixture, &edit(&s, &p, true)).unwrap();
            let back = revert(&e.text, &originals(&e), &written(&e), &e.created).unwrap();
            assert_eq!(back, fixture, "{name}");
        }
    }

    #[test]
    fn a_file_with_no_profiles_or_schemes_gets_them_and_loses_them_again() {
        // Review Focus 3.
        let (s, p) = (scheme(), profile());
        for fixture in ["{}\n", "{\n    \"copyOnSelect\": true\n}\n"] {
            let e = apply_edit(fixture, &edit(&s, &p, true)).unwrap();
            assert_eq!(
                e.created,
                vec![WtCreated::CmdProfile, WtCreated::ProfileList, WtCreated::Profiles, WtCreated::Schemes]
            );
            let back = revert(&e.text, &originals(&e), &written(&e), &e.created).unwrap();
            assert_eq!(back, fixture);
        }
    }

    #[test]
    fn revert_after_growing_a_one_line_object_restores_values_not_layout() {
        // Adding `size` to `"font": { "face": …, "weight": … }` makes the
        // library lay the object out over several lines; revert brings the
        // values back but keeps that layout. Undo (snapshot restore) is the
        // byte-exact path.
        let (s, p) = (scheme(), profile());
        let e = apply_edit(USER_CMD, &edit(&s, &p, true)).unwrap();
        let back = revert(&e.text, &originals(&e), &written(&e), &e.created).unwrap();
        let value =
            |t: &str| CstRootNode::parse(t, &wt_parse_options()).unwrap().value().unwrap().to_serde_value().unwrap();
        assert_eq!(value(&back), value(USER_CMD));
    }

    #[test]
    fn revert_keeps_a_value_the_user_changed_after_us() {
        let (s, p) = (scheme(), profile());
        let e = apply_edit(DEFAULT, &edit(&s, &p, false)).unwrap();
        let user_edit = e.text.replace("\"opacity\": 96", "\"opacity\": 70");
        assert_ne!(user_edit, e.text);
        let back = revert(&user_edit, &originals(&e), &written(&e), &e.created).unwrap();
        assert_eq!(read_value(&back, &WtKey::Profile { key: "opacity".into() }).unwrap(), Some(json!(70)));
        assert_eq!(read_value(&back, &WtKey::Profile { key: "colorScheme".into() }).unwrap(), None);
        assert_eq!(read_value(&back, &WtKey::Scheme).unwrap(), None);
    }

    #[test]
    fn to_cst_round_trips_every_json_kind() {
        let v = json!({ "name": "n", "a": [1, 2.5, -3], "b": { "c": null, "d": true }, "e": "x\"y\\z" });
        let root = CstRootNode::parse("{}", &wt_parse_options()).unwrap();
        root.object_value_or_set().append("v", to_cst(&v));
        let text = root.to_string();
        let back = CstRootNode::parse(&text, &wt_parse_options()).unwrap().value().unwrap().to_serde_value().unwrap();
        assert_eq!(back["v"], v);
        assert!(text.find("\"name\"").unwrap() < text.find("\"a\"").unwrap());
    }
}
```
Create `crates/core/src/adapters/windows/wt.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_three_variants_under_the_given_root() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("Packages").join(STORE_PACKAGE).join("LocalState");
        std::fs::create_dir_all(&store).unwrap();
        let locs = WinWt::new(tmp.path().to_path_buf()).locations();
        assert_eq!(locs.len(), 3);
        assert_eq!(
            locs[0],
            WtLocation { variant: WtVariant::Store, installed: true, settings: store.join("settings.json") }
        );
        assert!(!locs[1].installed && !locs[2].installed);
        assert_eq!(locs[2].settings, tmp.path().join(r"Microsoft\Windows Terminal\settings.json"));
    }
}
```

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- wt_edit adapters::windows::wt`
Expected: compile errors (`cannot find function apply_edit`, `WinWt`, …).

- [ ] **Step 4: Implement the editor** in `crates/core/src/wt_edit.rs`, above the tests:
```rust
//! Edits Windows Terminal's settings.json in place (spec §5.4, G9). Only the
//! keys CMD Studio owns change; comments, order, spacing, newlines and a
//! UTF-8 BOM stay byte-for-byte. Pure: text in, text out.
use crate::generate::wt::{CMD_PROFILE_GUID, SCHEME_NAME};
use jsonc_parser::cst::{CstArray, CstInputValue, CstNode, CstObject, CstRootNode};
use jsonc_parser::ParseOptions;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// What Windows Terminal itself accepts: comments and trailing commas, nothing looser.
pub fn wt_parse_options() -> ParseOptions {
    ParseOptions {
        allow_comments: true,
        allow_trailing_commas: true,
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
        allow_bare_decimal_point_numbers: false,
        allow_non_finite_numbers: false,
        allow_extended_string_escapes: false,
    }
}

/// One setting CMD Studio owns in a settings.json.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WtKey {
    /// The color scheme named `CMD Studio`, as a whole.
    Scheme,
    /// The top-level `defaultProfile`.
    DefaultProfile,
    /// A key of the cmd profile (guid `{0caa0dad-…}`).
    Profile { key: String },
    /// A key inside an object-valued key of the cmd profile, such as `font.face`.
    ProfileSub { key: String, sub: String },
}

/// A key's value before and after one edit. `original: None` means the key was absent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WtChange {
    pub key: WtKey,
    pub original: Option<Value>,
    pub written: Value,
}

/// A container the file lacked, so the edit added it. Revert removes it again
/// once nothing of the user's is in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WtCreated {
    /// The cmd profile object.
    CmdProfile,
    /// `profiles.list`.
    ProfileList,
    /// `profiles`.
    Profiles,
    /// `schemes`.
    Schemes,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WtEdited {
    pub text: String,
    pub changes: Vec<WtChange>,
    pub created: Vec<WtCreated>,
}

/// What `apply_edit` writes: the values from `plan::Op::WtApply`.
#[derive(Clone, Copy, Debug)]
pub struct WtEdit<'a> {
    pub scheme: &'a Value,
    pub profile: &'a Map<String, Value>,
    pub set_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WtError {
    #[error("settings.json is not valid JSON with comments: {0}")]
    Parse(String),
    #[error("settings.json has an unexpected shape: {0}")]
    Shape(String),
}

impl WtError {
    /// Stable code shown to users (spec §5.9).
    pub fn code(&self) -> &'static str {
        "E_WT_PARSE"
    }
}

/// Converts a generator value into the CST input type (G9: the crate has no
/// `From<serde_json::Value>`). Object keys keep `guid` and `name` first, then
/// the map's own order.
pub fn to_cst(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(b) => CstInputValue::Bool(*b),
        Value::Number(n) => CstInputValue::Number(n.to_string()),
        Value::String(s) => CstInputValue::String(s.clone()),
        Value::Array(items) => CstInputValue::Array(items.iter().map(to_cst).collect()),
        Value::Object(map) => {
            let first = ["guid", "name"];
            let mut props: Vec<(String, CstInputValue)> =
                first.iter().filter_map(|k| map.get(*k).map(|v| ((*k).to_string(), to_cst(v)))).collect();
            props.extend(map.iter().filter(|(k, _)| !first.contains(&k.as_str())).map(|(k, v)| (k.clone(), to_cst(v))));
            CstInputValue::Object(props)
        }
    }
}

/// Fails with `E_WT_PARSE` when Windows Terminal could not read the text.
pub fn check_parses(text: &str) -> Result<(), WtError> {
    let (_, body) = split_bom(text);
    let root = parse(body)?;
    root_object(&root).map(|_| ())
}

/// Writes CMD Studio's scheme, cmd profile keys and (when asked) the default
/// profile. Records every touched key's value before and after.
pub fn apply_edit(text: &str, edit: &WtEdit<'_>) -> Result<WtEdited, WtError> {
    let (bom, body) = split_bom(text);
    let root = parse(body)?;
    let obj = root_object(&root)?;
    let mut changes = Vec::new();
    let mut created = Vec::new();

    // 1. The scheme: replace every scheme named "CMD Studio", or append one.
    if obj.get("schemes").is_none() {
        created.push(WtCreated::Schemes);
    }
    let schemes =
        obj.array_value_or_create("schemes").ok_or_else(|| WtError::Shape("\"schemes\" is not a list".into()))?;
    let ours: Vec<CstObject> = schemes.elements().into_iter().filter_map(|e| our_scheme(&e)).collect();
    let original = ours.first().and_then(CstObject::to_serde_value);
    if ours.is_empty() {
        schemes.append(to_cst(edit.scheme));
    } else if original.as_ref() != Some(edit.scheme) || ours.len() > 1 {
        for scheme in ours {
            scheme.replace_with(to_cst(edit.scheme));
        }
    }
    changes.push(WtChange { key: WtKey::Scheme, original, written: edit.scheme.clone() });

    // 2. The cmd profile: create it, or set each key on the existing one.
    let list = profiles_list(&obj, &mut created)?;
    match find_cmd_profile(&list) {
        Some(profile) => set_profile_keys(&profile, edit.profile, &mut changes)?,
        None => {
            let mut map = Map::new();
            map.insert("guid".into(), Value::String(CMD_PROFILE_GUID.into()));
            map.insert("name".into(), Value::String("Command Prompt".into()));
            for (k, v) in edit.profile {
                map.insert(k.clone(), v.clone());
                changes.push(WtChange { key: WtKey::Profile { key: k.clone() }, original: None, written: v.clone() });
            }
            list.append(to_cst(&Value::Object(map)));
            created.push(WtCreated::CmdProfile);
        }
    }

    // 3. The default profile, only when the user asked for it.
    if edit.set_default {
        let new = Value::String(CMD_PROFILE_GUID.into());
        let original = obj.get("defaultProfile").and_then(|p| p.value()).and_then(|v| v.to_serde_value());
        let already =
            original.as_ref().and_then(Value::as_str).is_some_and(|s| s.eq_ignore_ascii_case(CMD_PROFILE_GUID));
        if !already {
            set_prop(&obj, "defaultProfile", &new);
        }
        changes.push(WtChange { key: WtKey::DefaultProfile, original, written: new });
    }

    let text = format!("{bom}{}", root);
    check_parses(&text)?;
    created.sort();
    Ok(WtEdited { text, changes, created })
}

/// The current value of one owned key; `None` when it is absent.
pub fn read_value(text: &str, key: &WtKey) -> Result<Option<Value>, WtError> {
    let (_, body) = split_bom(text);
    let root = parse(body)?;
    let obj = root_object(&root)?;
    Ok(match key {
        WtKey::Scheme => obj
            .array_value("schemes")
            .and_then(|s| s.elements().into_iter().find_map(|e| our_scheme(&e)))
            .and_then(|o| o.to_serde_value()),
        WtKey::DefaultProfile => obj.get("defaultProfile").and_then(|p| p.value()).and_then(|v| v.to_serde_value()),
        WtKey::Profile { key } => {
            existing_profile(&obj).and_then(|p| p.get(key)).and_then(|p| p.value()).and_then(|v| v.to_serde_value())
        }
        WtKey::ProfileSub { key, sub } => existing_profile(&obj)
            .and_then(|p| p.object_value(key))
            .and_then(|o| o.get(sub))
            .and_then(|p| p.value())
            .and_then(|v| v.to_serde_value()),
    })
}

/// Undoes CMD Studio's keys one by one (spec §6.3 step 2): every scheme named
/// `CMD Studio` goes; a key still holding what CMD Studio last wrote gets its
/// original back (or is removed when it was absent); a key the user changed
/// since keeps the user's value; containers CMD Studio created go when
/// nothing of the user's is left in them.
pub fn revert(
    text: &str,
    originals: &BTreeMap<WtKey, Option<Value>>,
    written: &BTreeMap<WtKey, Value>,
    created: &[WtCreated],
) -> Result<String, WtError> {
    let (bom, body) = split_bom(text);
    let root = parse(body)?;
    let obj = root_object(&root)?;

    if let Some(schemes) = obj.array_value("schemes") {
        for scheme in schemes.elements().into_iter().filter_map(|e| our_scheme(&e)) {
            scheme.remove();
        }
    }

    // BTreeMap order puts a parent key (`font`) before its sub-keys (`font.face`).
    for (key, original) in originals {
        let Some(last) = written.get(key) else {
            continue;
        };
        match key {
            WtKey::Scheme => {}
            WtKey::DefaultProfile => restore_prop(&obj, "defaultProfile", original, last),
            WtKey::Profile { key } => {
                if let Some(profile) = existing_profile(&obj) {
                    restore_prop(&profile, key, original, last);
                }
            }
            WtKey::ProfileSub { key, sub } => {
                if let Some(parent) = existing_profile(&obj).and_then(|p| p.object_value(key)) {
                    restore_prop(&parent, sub, original, last);
                }
            }
        }
    }

    if created.contains(&WtCreated::CmdProfile) {
        if let Some(profile) = existing_profile(&obj) {
            let only_ours =
                profile.properties().iter().all(|p| p.decoded_name().is_some_and(|n| n == "guid" || n == "name"));
            if only_ours {
                profile.remove();
            }
        }
    }
    if created.contains(&WtCreated::ProfileList) {
        if let Some(profiles) = obj.object_value("profiles") {
            remove_if_empty(&profiles, "list");
        }
    }
    if created.contains(&WtCreated::Profiles) {
        remove_if_empty(&obj, "profiles");
    }
    if created.contains(&WtCreated::Schemes) {
        remove_if_empty(&obj, "schemes");
    }

    let text = format!("{bom}{}", root);
    check_parses(&text)?;
    Ok(text)
}

fn split_bom(text: &str) -> (&'static str, &str) {
    match text.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", text),
    }
}

fn parse(text: &str) -> Result<CstRootNode, WtError> {
    CstRootNode::parse(text, &wt_parse_options()).map_err(|e| WtError::Parse(e.to_string()))
}

fn root_object(root: &CstRootNode) -> Result<CstObject, WtError> {
    root.object_value().ok_or_else(|| WtError::Shape("the top level is not an object".into()))
}

fn our_scheme(node: &CstNode) -> Option<CstObject> {
    let obj = node.as_object()?;
    let name = obj.get("name")?.value()?.as_string_lit()?.decoded_value().ok()?;
    (name == SCHEME_NAME).then_some(obj)
}

/// `profiles.list`, or the legacy form where `profiles` is the list itself.
/// Adds whichever of the two is missing and records it in `created`.
fn profiles_list(obj: &CstObject, created: &mut Vec<WtCreated>) -> Result<CstArray, WtError> {
    let shape = || WtError::Shape("\"profiles\" is neither an object with a \"list\" nor a list".into());
    match obj.get("profiles").and_then(|p| p.value()) {
        Some(node) => match (node.as_array(), node.as_object()) {
            (Some(list), _) => Ok(list),
            (None, Some(profiles)) => {
                if profiles.get("list").is_none() {
                    created.push(WtCreated::ProfileList);
                }
                profiles.array_value_or_create("list").ok_or_else(shape)
            }
            (None, None) => Err(shape()),
        },
        None => {
            created.push(WtCreated::Profiles);
            created.push(WtCreated::ProfileList);
            obj.object_value_or_create("profiles").and_then(|p| p.array_value_or_create("list")).ok_or_else(shape)
        }
    }
}

/// Removes `name` when it holds an empty list or an empty object.
fn remove_if_empty(obj: &CstObject, name: &str) {
    let empty = match current(obj, name) {
        Some(Value::Array(items)) => items.is_empty(),
        Some(Value::Object(map)) => map.is_empty(),
        _ => false,
    };
    if empty {
        if let Some(prop) = obj.get(name) {
            prop.remove();
        }
    }
}

fn find_cmd_profile(list: &CstArray) -> Option<CstObject> {
    list.elements().into_iter().filter_map(|e| e.as_object()).find(|o| {
        o.get("guid")
            .and_then(|p| p.value())
            .and_then(|v| v.as_string_lit())
            .and_then(|s| s.decoded_value().ok())
            .is_some_and(|g| g.eq_ignore_ascii_case(CMD_PROFILE_GUID))
    })
}

fn existing_profile(obj: &CstObject) -> Option<CstObject> {
    let node = obj.get("profiles")?.value()?;
    let list = match node.as_array() {
        Some(list) => list,
        None => node.as_object()?.array_value("list")?,
    };
    find_cmd_profile(&list)
}

fn current(obj: &CstObject, name: &str) -> Option<Value> {
    obj.get(name).and_then(|p| p.value()).and_then(|v| v.to_serde_value())
}

/// Sets `name` to `value`, leaving the text alone when it already holds it.
fn set_prop(obj: &CstObject, name: &str, value: &Value) {
    match obj.get(name) {
        Some(prop) => {
            if current(obj, name).as_ref() != Some(value) {
                prop.set_value(to_cst(value));
            }
        }
        None => {
            obj.append(name, to_cst(value));
        }
    }
}

fn set_profile_keys(
    profile: &CstObject,
    values: &Map<String, Value>,
    changes: &mut Vec<WtChange>,
) -> Result<(), WtError> {
    for (key, value) in values {
        let existing = current(profile, key);
        match (value.as_object(), existing.as_ref()) {
            // An object value (font) merges key by key into an existing object, so
            // the user's other font keys (weight, features) survive.
            (Some(subs), Some(Value::Object(_))) => {
                let parent = profile
                    .object_value(key)
                    .ok_or_else(|| WtError::Shape(format!("cmd profile key {key:?} changed while editing")))?;
                for (sub, sub_value) in subs {
                    changes.push(WtChange {
                        key: WtKey::ProfileSub { key: key.clone(), sub: sub.clone() },
                        original: current(&parent, sub),
                        written: sub_value.clone(),
                    });
                    set_prop(&parent, sub, sub_value);
                }
            }
            _ => {
                changes.push(WtChange {
                    key: WtKey::Profile { key: key.clone() },
                    original: existing.clone(),
                    written: value.clone(),
                });
                set_prop(profile, key, value);
            }
        }
    }
    Ok(())
}

/// Puts `original` back when the key still holds what CMD Studio wrote.
fn restore_prop(obj: &CstObject, name: &str, original: &Option<Value>, last_written: &Value) {
    if current(obj, name).as_ref() != Some(last_written) {
        return;
    }
    match original {
        Some(value) => set_prop(obj, name, value),
        None => {
            if let Some(prop) = obj.get(name) {
                prop.remove();
            }
        }
    }
}
```

- [ ] **Step 5: Implement the locator** in `crates/core/src/adapters/windows/wt.rs`, above the tests:
```rust
//! Where each Windows Terminal variant keeps settings.json (spec §5.4).
use crate::adapters::{WtLocation, WtSettings, WtVariant};
use std::path::PathBuf;

pub const STORE_PACKAGE: &str = "Microsoft.WindowsTerminal_8wekyb3d8bbwe";
pub const PREVIEW_PACKAGE: &str = "Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe";

/// Locates settings under a `%LOCALAPPDATA%` root. Tests pass a temp folder.
#[derive(Clone, Debug)]
pub struct WinWt {
    local_app_data: PathBuf,
}

impl WinWt {
    pub fn new(local_app_data: PathBuf) -> Self {
        WinWt { local_app_data }
    }
}

impl WtSettings for WinWt {
    fn locations(&self) -> Vec<WtLocation> {
        let packaged = |package: &str, variant| {
            let dir = self.local_app_data.join("Packages").join(package);
            WtLocation { variant, installed: dir.is_dir(), settings: dir.join("LocalState").join("settings.json") }
        };
        let unpackaged = self.local_app_data.join("Microsoft").join("Windows Terminal");
        vec![
            packaged(STORE_PACKAGE, WtVariant::Store),
            packaged(PREVIEW_PACKAGE, WtVariant::Preview),
            WtLocation {
                variant: WtVariant::Unpackaged,
                installed: unpackaged.is_dir(),
                settings: unpackaged.join("settings.json"),
            },
        ]
    }
}
```

- [ ] **Step 6: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- wt_edit adapters::windows::wt`
Expected: `16 passed` (15 in `wt_edit`, 1 in `adapters::windows::wt`).

- [ ] **Step 7: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 8: Commit**

Message:
```text
feat(core): edit Windows Terminal settings.json through the CST

wt_edit sets the CMD Studio scheme, the cmd profile keys and, on
request, defaultProfile, with jsonc-parser 0.34.0's CST so comments,
layout, CRLF and a BOM stay. It records every touched key's value
before and after, and reverts key by key: keys the user changed later
keep the user's value, and containers it added go when empty. A file
Windows Terminal could not read is refused with E_WT_PARSE and never
written. Fixtures cover WT 1.24's first-run file, comments, trailing
commas, the legacy profiles array, an uppercase guid and a user font
object. WinWt finds the Store, Preview and unpackaged settings.json.
```
Run:
```bash
git add -- crates/core/Cargo.toml Cargo.lock crates/core/src/lib.rs crates/core/src/wt_edit.rs crates/core/src/adapters/windows/mod.rs crates/core/src/adapters/windows/wt.rs crates/core/tests/fixtures/wt
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 8: `state.json` and the baseline

**Files:**
- Create: `crates/core/src/state.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod state;`)

**Interfaces:**
- Consumes: `adapters::{Fs, RegValue}` (Task 1); `wt_edit::{WtCreated, WtEdited, WtKey}` (Task 7); `model::Target`, `plan::{ClinkLaunch, Paths}` (Phase 1a).
- Produces (in `cmdstudio_core::state`):
  - More `Paths` methods (an inherent `impl Paths` here; allowed in the same crate): `state_file()` = `<data>\state.json`, `snapshots()` = `<data>\snapshots`, `themes()` = `<data>\themes`, `runtime_root()` = `<data>\runtime`.
  - `STATE_VERSION = 1`; `State { version: u32, applied: Option<Applied>, baseline: Baseline }` with `Default`, `State::load(&dyn Fs, &Path) -> Result<State, StateError>` (absent file → `State::default()`), `State::to_bytes() -> Result<Vec<u8>, StateError>` (pretty JSON + `\n`).
  - `Applied { theme_id: Uuid, theme_name, target: Target, set_default: bool, lang: String ("th"/"en"), launch: Launch, omp_version, clink_version, generated: BTreeMap<PathBuf, String> (SHA-256 of each file under generated\), wt_font: Option<String>, clink_log_len: u64, snapshot_id: String, applied_at_ms: u64 }`. `clink_log_len` is how many bytes `clink-profile\clink.log` held at apply time: doctor D3 (Task 15) reads only what Clink logged after it, to tell whether a cmd window has loaded the theme since (ruling P68).
  - `Launch::{Direct { exe }, ShortPath { exe }, ProgramData { dir }}` with `From<&ClinkLaunch>` and `to_clink() -> ClinkLaunch`.
  - `RegValueRecord { kind: u32, data_hex: String }` with `from_reg(&RegValue)` and `to_reg() -> Option<RegValue>`; `AutoRunRecord { key_existed: bool, value: Option<RegValueRecord> }`; `AutoRunWritten { value: Option<String> }`.
  - `Was::{Absent, Present { value }}`, `WtOriginal { key, original: Was }`, `WtWritten { key, value }`, `WtFileBaseline { settings: PathBuf, originals, written, created: Vec<WtCreated> }` with `originals_map() -> BTreeMap<WtKey, Option<Value>>` and `written_map() -> BTreeMap<WtKey, Value>`.
  - `FontRecord { family, file: PathBuf, value_name }`.
  - `Baseline { autorun: Option<AutoRunRecord>, autorun_written: Option<AutoRunWritten>, wt: Vec<WtFileBaseline>, fonts: Vec<FontRecord>, program_data_dir: Option<PathBuf> }` (`Default`) with `note_autorun(&AutoRunRecord, Option<&str>)`, `note_wt(&Path, &WtEdited)`, `note_font(FontRecord)`.
  - `StateError::{Invalid(String) → "E_STATE_INVALID", Io(String) → "E_DATA_WRITE"}` with `code()`.

**The baseline rule** (brief; spec §5.4 "จุดย้อนกลับ", §6.3): the first time CMD Studio changes something, its original goes into the baseline and is never overwritten by later applies; what CMD Studio wrote last is always recorded. "Revert everything" (Task 14) compares the current value with "last written" to tell CMD Studio's value from a later change by the user. `Was` exists because `Option<Value>` cannot tell "absent" from a JSON `null` once serialized. AutoRun's raw bytes are stored as hex so a restore can be byte-exact. `E_STATE_INVALID` and `E_DATA_WRITE` are new codes; Task 15 adds them to spec §5.9.

- [ ] **Step 1: Write the failing tests** — add `pub mod state;` to `crates/core/src/lib.rs` and create `crates/core/src/state.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeFs;
    use crate::adapters::{REG_EXPAND_SZ, REG_SZ};
    use crate::wt_edit::WtChange;
    use serde_json::json;

    #[test]
    fn a_missing_file_is_a_fresh_state_and_junk_is_refused() {
        let fs = FakeFs::new();
        let p = Path::new(r"C:\d\state.json");
        assert_eq!(State::load(&fs, p).unwrap(), State::default());
        fs.put(p, b"{ nope");
        assert_eq!(State::load(&fs, p).unwrap_err().code(), "E_STATE_INVALID");
    }

    #[test]
    fn reg_records_round_trip_every_byte() {
        let mut v = RegValue::from_text(REG_EXPAND_SZ, "%X% & ไทย");
        v.bytes.push(0xff);
        let rec = RegValueRecord::from_reg(&v);
        assert_eq!(rec.to_reg(), Some(v));
        assert_eq!(RegValueRecord { kind: REG_SZ, data_hex: "zz".into() }.to_reg(), None);
        assert_eq!(RegValueRecord { kind: REG_SZ, data_hex: "abc".into() }.to_reg(), None);
    }

    #[test]
    fn the_first_original_wins_and_the_last_write_is_kept() {
        let mut b = Baseline::default();
        let before = AutoRunRecord { key_existed: true, value: None };
        b.note_autorun(&before, Some("first"));
        b.note_autorun(
            &AutoRunRecord { key_existed: true, value: Some(RegValueRecord { kind: 1, data_hex: "00".into() }) },
            None,
        );
        assert_eq!(b.autorun, Some(before));
        assert_eq!(b.autorun_written, Some(AutoRunWritten { value: None }));

        let key = WtKey::Profile { key: "opacity".into() };
        let first = WtEdited {
            text: String::new(),
            changes: vec![WtChange { key: key.clone(), original: Some(json!(80)), written: json!(96) }],
            created: vec![WtCreated::Schemes],
        };
        let second = WtEdited {
            text: String::new(),
            changes: vec![WtChange { key: key.clone(), original: Some(json!(96)), written: json!(90) }],
            created: vec![],
        };
        let f = Path::new(r"C:\wt\settings.json");
        b.note_wt(f, &first);
        b.note_wt(f, &second);
        let file = &b.wt[0];
        assert_eq!(file.originals_map()[&key], Some(json!(80)));
        assert_eq!(file.written_map()[&key], json!(90));
        assert_eq!(file.created, vec![WtCreated::Schemes]);
    }

    #[test]
    fn json_uses_camel_case_and_tells_absent_from_null() {
        let mut s = State::default();
        let edited = WtEdited {
            text: String::new(),
            changes: vec![
                WtChange {
                    key: WtKey::Profile { key: "icon".into() },
                    original: Some(Value::Null),
                    written: json!("x"),
                },
                WtChange {
                    key: WtKey::ProfileSub { key: "font".into(), sub: "face".into() },
                    original: None,
                    written: json!("F"),
                },
            ],
            created: vec![WtCreated::CmdProfile],
        };
        s.baseline.note_wt(Path::new(r"C:\wt\settings.json"), &edited);
        s.baseline.note_font(FontRecord {
            family: "CaskaydiaCove NF".into(),
            file: PathBuf::from(r"C:\f.ttf"),
            value_name: "CaskaydiaCove NF (TrueType)".into(),
        });
        let text = String::from_utf8(s.to_bytes().unwrap()).unwrap();
        for word in [
            "\"originals\"",
            "\"valueName\"",
            "\"programDataDir\"",
            "\"was\": \"absent\"",
            "\"was\": \"present\"",
            "\"at\": \"profileSub\"",
            "\"cmdProfile\"",
        ] {
            assert!(text.contains(word), "{word} missing in {text}");
        }
        let back: State = serde_json::from_str(&text).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.baseline.wt[0].originals_map()[&WtKey::Profile { key: "icon".into() }], Some(Value::Null));
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- state::tests`
Expected: compile errors (`cannot find type State`, …).

- [ ] **Step 3: Implement it above the tests**
```rust
//! `state.json` (spec §5.3): the applied theme, hashes of the generated
//! files, the runtime versions, `<clink-launch>`, and the baseline: each
//! setting's value before CMD Studio first changed it, plus what CMD Studio
//! last wrote, so "revert all" and uninstall can undo key by key (spec §6.3).
use crate::adapters::{Fs, RegValue};
use crate::model::Target;
use crate::plan::{ClinkLaunch, Paths};
use crate::wt_edit::{WtCreated, WtEdited, WtKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const STATE_VERSION: u32 = 1;

impl Paths {
    pub fn state_file(&self) -> PathBuf {
        self.data.join("state.json")
    }
    pub fn snapshots(&self) -> PathBuf {
        self.data.join("snapshots")
    }
    pub fn themes(&self) -> PathBuf {
        self.data.join("themes")
    }
    pub fn runtime_root(&self) -> PathBuf {
        self.data.join("runtime")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub version: u32,
    pub applied: Option<Applied>,
    #[serde(default)]
    pub baseline: Baseline,
}

impl Default for State {
    fn default() -> Self {
        State { version: STATE_VERSION, applied: None, baseline: Baseline::default() }
    }
}

/// The theme that is on the machine now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub theme_id: uuid::Uuid,
    pub theme_name: String,
    pub target: Target,
    pub set_default: bool,
    /// `th` or `en`: the language the banner was generated in.
    pub lang: String,
    pub launch: Launch,
    pub omp_version: String,
    pub clink_version: String,
    /// Path of each generated file → its SHA-256 (doctor D9).
    pub generated: BTreeMap<PathBuf, String>,
    /// The font face written to Windows Terminal, if any (doctor D8).
    pub wt_font: Option<String>,
    /// How many bytes `clink-profile\clink.log` held when the theme was applied. Doctor D3
    /// reads only what Clink logged after them: the cmd windows started since.
    pub clink_log_len: u64,
    pub snapshot_id: String,
    pub applied_at_ms: u64,
}

/// `plan::ClinkLaunch` as stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Launch {
    Direct { exe: PathBuf },
    ShortPath { exe: PathBuf },
    ProgramData { dir: PathBuf },
}

impl From<&ClinkLaunch> for Launch {
    fn from(l: &ClinkLaunch) -> Self {
        match l {
            ClinkLaunch::Direct(exe) => Launch::Direct { exe: exe.clone() },
            ClinkLaunch::ShortPath(exe) => Launch::ShortPath { exe: exe.clone() },
            ClinkLaunch::ProgramData { dir } => Launch::ProgramData { dir: dir.clone() },
        }
    }
}

impl Launch {
    pub fn to_clink(&self) -> ClinkLaunch {
        match self {
            Launch::Direct { exe } => ClinkLaunch::Direct(exe.clone()),
            Launch::ShortPath { exe } => ClinkLaunch::ShortPath(exe.clone()),
            Launch::ProgramData { dir } => ClinkLaunch::ProgramData { dir: dir.clone() },
        }
    }
}

/// A registry value as stored, kept exactly (type + raw bytes as hex).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegValueRecord {
    pub kind: u32,
    pub data_hex: String,
}

impl RegValueRecord {
    pub fn from_reg(v: &RegValue) -> Self {
        RegValueRecord { kind: v.kind, data_hex: v.bytes.iter().map(|b| format!("{b:02x}")).collect() }
    }

    /// `None` when `data_hex` is not hex.
    pub fn to_reg(&self) -> Option<RegValue> {
        let (pairs, rest) = self.data_hex.as_bytes().as_chunks::<2>();
        if !rest.is_empty() {
            return None;
        }
        let bytes = pairs
            .iter()
            .map(|pair| std::str::from_utf8(pair).ok().and_then(|s| u8::from_str_radix(s, 16).ok()))
            .collect::<Option<Vec<u8>>>()?;
        Some(RegValue { kind: self.kind, bytes })
    }
}

/// The user's AutoRun at one moment: whether the key existed and the value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRunRecord {
    pub key_existed: bool,
    pub value: Option<RegValueRecord>,
}

/// What CMD Studio last wrote to AutoRun; `value: None` means it deleted it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRunWritten {
    pub value: Option<String>,
}

/// A key's value before CMD Studio first touched it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "was", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Was {
    Absent,
    Present { value: Value },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WtOriginal {
    pub key: WtKey,
    pub original: Was,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WtWritten {
    pub key: WtKey,
    pub value: Value,
}

/// One settings.json: first-touch originals, last-written values, and the
/// containers CMD Studio added.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WtFileBaseline {
    pub settings: PathBuf,
    pub originals: Vec<WtOriginal>,
    pub written: Vec<WtWritten>,
    pub created: Vec<WtCreated>,
}

impl WtFileBaseline {
    pub fn originals_map(&self) -> BTreeMap<WtKey, Option<Value>> {
        self.originals
            .iter()
            .map(|o| {
                let v = match &o.original {
                    Was::Absent => None,
                    Was::Present { value } => Some(value.clone()),
                };
                (o.key.clone(), v)
            })
            .collect()
    }

    pub fn written_map(&self) -> BTreeMap<WtKey, Value> {
        self.written.iter().map(|w| (w.key.clone(), w.value.clone())).collect()
    }
}

/// A font CMD Studio installed (spec §5.4: uninstall only what we installed).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontRecord {
    pub family: String,
    pub file: PathBuf,
    pub value_name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Baseline {
    /// AutoRun before CMD Studio first changed it; `None`: never changed.
    pub autorun: Option<AutoRunRecord>,
    pub autorun_written: Option<AutoRunWritten>,
    pub wt: Vec<WtFileBaseline>,
    pub fonts: Vec<FontRecord>,
    /// The user's ProgramData folder, when CMD Studio created it (spec §6.3 step 4).
    pub program_data_dir: Option<PathBuf>,
}

impl Baseline {
    /// Keeps the first-ever original; always records the latest write.
    pub fn note_autorun(&mut self, before: &AutoRunRecord, written: Option<&str>) {
        if self.autorun.is_none() {
            self.autorun = Some(before.clone());
        }
        self.autorun_written = Some(AutoRunWritten { value: written.map(str::to_string) });
    }

    pub fn note_wt(&mut self, settings: &Path, edited: &WtEdited) {
        let index = match self.wt.iter().position(|f| f.settings == settings) {
            Some(i) => i,
            None => {
                self.wt.push(WtFileBaseline {
                    settings: settings.to_path_buf(),
                    originals: Vec::new(),
                    written: Vec::new(),
                    created: Vec::new(),
                });
                self.wt.len().saturating_sub(1)
            }
        };
        let Some(file) = self.wt.get_mut(index) else {
            return;
        };
        for change in &edited.changes {
            if !file.originals.iter().any(|o| o.key == change.key) {
                let original = match &change.original {
                    None => Was::Absent,
                    Some(value) => Was::Present { value: value.clone() },
                };
                file.originals.push(WtOriginal { key: change.key.clone(), original });
            }
            match file.written.iter_mut().find(|w| w.key == change.key) {
                Some(w) => w.value = change.written.clone(),
                None => file.written.push(WtWritten { key: change.key.clone(), value: change.written.clone() }),
            }
        }
        for c in &edited.created {
            if !file.created.contains(c) {
                file.created.push(*c);
            }
        }
    }

    pub fn note_font(&mut self, font: FontRecord) {
        if !self.fonts.contains(&font) {
            self.fonts.push(font);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StateError {
    #[error("state.json could not be read: {0}")]
    Invalid(String),
    #[error("{0}")]
    Io(String),
}

impl StateError {
    pub fn code(&self) -> &'static str {
        match self {
            StateError::Invalid(_) => "E_STATE_INVALID",
            StateError::Io(_) => "E_DATA_WRITE",
        }
    }
}

impl State {
    /// The saved state; a fresh one when there is no file yet.
    pub fn load(fs: &dyn Fs, path: &Path) -> Result<State, StateError> {
        match fs.read(path).map_err(|e| StateError::Io(e.to_string()))? {
            None => Ok(State::default()),
            Some(bytes) => serde_json::from_slice(&bytes).map_err(|e| StateError::Invalid(e.to_string())),
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, StateError> {
        let mut bytes = serde_json::to_vec_pretty(self).map_err(|e| StateError::Invalid(e.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- state::tests`
Expected: `4 passed`.

- [ ] **Step 5: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 6: Commit**

Message:
```text
feat(core): add state.json and the baseline

state.json records the applied theme, the generated files' hashes,
<clink-launch>, clink.log's length at apply time (for doctor D3) and
the baseline: each setting's value before CMD Studio first changed it
and what it last wrote. AutoRun is kept as raw bytes (hex) with its
type; settings.json keys tell "absent" from null.
```
Run:
```bash
git add -- crates/core/src/lib.rs crates/core/src/state.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 9: Restore points and the on-disk journal store

**Files:**
- Create: `crates/core/src/snapshot.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod snapshot;`)

**Interfaces:**
- Consumes: `adapters::Fs` and `adapters::fake::FakeFs` (Task 1: `read`, `write_atomic`, `list_dir` → `DirEntry { path, is_dir }`, `exists`, `remove_file`, `remove_dir_all`; `FakeFs::file(path) -> Option<Vec<u8>>`); `model::Target`, `plan::Op`, `txn::{Failure, Journal, JournalStore, Snapshot, Stage}` (Phase 1a).
- Produces (in `cmdstudio_core::snapshot`):
  - `KEEP: usize = 20`; `new_id(now_ms: u64) -> String` (13-digit milliseconds, `-`, 8 hex: sorts by time).
  - `SnapshotKind::{Apply, Undo, RevertAll}`; `ThemeRef { id: Uuid, name }`; `Manifest { id, kind, created_ms: u64, theme: Option<ThemeRef>, target: Option<Target>, summary: Vec<String>, undoes: Option<String>, undone_by: Option<String> }`.
  - `SnapshotStore<'a>` (`Copy`): `new(&'a dyn Fs, &'a Path)`, `ids() -> Result<Vec<String>, Failure>` (oldest first), `manifest(&str) -> Result<Manifest, Failure>`, `write_manifest(&Manifest) -> Result<(), Failure>`, `journal(&str) -> Result<Journal, Failure>`, `latest_undoable() -> Result<Option<(Manifest, Journal)>, Failure>`, `mark_undone(&[String], by: &str) -> Result<(), Failure>`, `prune(keep: usize) -> Result<Vec<String>, Failure>`, `begin(Manifest) -> DiskJournal<'a>`, `recovery() -> DiskJournal<'a>`, `payload_path(id, op_index) -> PathBuf` (`<root>\<id>\payload\<op_index>.bin`), `write_incomplete(&Incomplete) -> Result<(), Failure>`, `latest_incomplete() -> Result<Option<Incomplete>, Failure>` (the newest restore point's record only).
  - `DiskJournal<'a>`: `impl txn::JournalStore` (`save`, `load_unfinished`).
  - `Unrestored { op_index: usize, target: String, code: String, message: String, fix: String }` and `Incomplete { id: String, code: String, reason: String, steps: Vec<Unrestored> }`, both camelCase JSON: what a rollback that ended `RollbackIncomplete` could not put back, and how to fix each by hand. Task 13 fills and writes them; Task 16 prints them.
  - Every failure here has code `E_SNAPSHOT_WRITE` (spec §5.9).

**How a restore point is stored** (spec §5.3, §5.5; brief): `<data>\snapshots\<id>\manifest.json` says what it is; `journal.json` is the txn `Journal` with each `Snapshot.prior` moved out to `payload\<op index>.bin`. `"payload": null` means the target did not exist before (an empty file is a zero-byte payload, not `null`). Payload files are written once each, before the journal that names them, so a crash between the two leaves a journal that never names a missing file. A missing payload file is an error (`E_SNAPSHOT_WRITE`), never read as "the target was absent": that would make an undo delete the user's file. `load_unfinished` looks only at the newest restore point, because a new transaction starts only after recovery has finished the previous one (Task 13).

**A rollback that could not finish** (spec §5.5: say which parts and how to fix them by hand, never report success): `txn::Stage::RollbackIncomplete` keeps only the positions of the ops that were not put back, because a `txn::Failure` (with its `&'static str` code) cannot be read back from JSON. `incomplete.json`, next to the journal, keeps the rest: each op's target, the error of its restore, and the fix by hand, so a restart does not lose them. It is not part of `state.json`, which may itself be a target the rollback could not put back. A record is news only while its restore point is the newest: once a newer transaction has run, `latest_incomplete` stops returning it (the file stays for a person to read).

- [ ] **Step 1: Write the failing tests** — add `pub mod snapshot;` to `crates/core/src/lib.rs` and create `crates/core/src/snapshot.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeFs;

    const ROOT: &str = r"C:\d\snapshots";

    fn manifest(id: &str, kind: SnapshotKind) -> Manifest {
        Manifest {
            id: id.into(),
            kind,
            created_ms: 1,
            theme: None,
            target: Some(Target::All),
            summary: vec![],
            undoes: None,
            undone_by: None,
        }
    }

    fn journal(id: &str, stage: Stage) -> Journal {
        Journal {
            id: id.into(),
            ops: vec![
                Op::WriteFile { path: PathBuf::from(r"C:\d\a.lua"), contents: "new".into() },
                Op::SetAutoRun { value: None },
                Op::WriteFile { path: PathBuf::from(r"C:\d\empty.txt"), contents: "x".into() },
            ],
            snapshots: vec![
                Snapshot { op_index: 0, prior: Some(b"old a".to_vec()) },
                Snapshot { op_index: 1, prior: None },
                Snapshot { op_index: 2, prior: Some(Vec::new()) },
            ],
            stage,
        }
    }

    #[test]
    fn journals_round_trip_through_payload_files() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        let mut disk = store.begin(manifest("0000000000001-aaaaaaaa", SnapshotKind::Apply));
        let j = journal("0000000000001-aaaaaaaa", Stage::Committed);
        disk.save(&j).unwrap();
        assert_eq!(fs.file(format!(r"{ROOT}\0000000000001-aaaaaaaa\payload\0.bin")), Some(b"old a".to_vec()));
        assert_eq!(
            fs.file(format!(r"{ROOT}\0000000000001-aaaaaaaa\payload\2.bin")),
            Some(Vec::new()),
            "an empty prior is a file"
        );
        assert_eq!(
            fs.file(format!(r"{ROOT}\0000000000001-aaaaaaaa\payload\1.bin")),
            None,
            "an absent prior has no file"
        );
        let text = String::from_utf8(fs.file(format!(r"{ROOT}\0000000000001-aaaaaaaa\journal.json")).unwrap()).unwrap();
        assert!(text.contains("\"opIndex\": 0") && text.contains("\"payload\": null") && !text.contains("old a"));
        assert_eq!(store.journal("0000000000001-aaaaaaaa").unwrap(), j);
        assert_eq!(store.manifest("0000000000001-aaaaaaaa").unwrap().kind, SnapshotKind::Apply);
    }

    #[test]
    fn only_the_newest_unfinished_journal_is_recovered() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        store
            .begin(manifest("0000000000001-a", SnapshotKind::Apply))
            .save(&journal("0000000000001-a", Stage::Applying { done: 1 }))
            .unwrap();
        store
            .begin(manifest("0000000000002-b", SnapshotKind::Apply))
            .save(&journal("0000000000002-b", Stage::Committed))
            .unwrap();
        assert_eq!(store.recovery().load_unfinished().unwrap(), None);
        store
            .begin(manifest("0000000000003-c", SnapshotKind::Apply))
            .save(&journal("0000000000003-c", Stage::Verifying))
            .unwrap();
        let mut rec = store.recovery();
        let j = rec.load_unfinished().unwrap().unwrap();
        assert_eq!(j.id, "0000000000003-c");
        let mut done = j.clone();
        done.stage = Stage::RolledBack;
        rec.save(&done).unwrap();
        assert_eq!(store.journal("0000000000003-c").unwrap().stage, Stage::RolledBack);
    }

    #[test]
    fn a_missing_payload_is_an_error_not_an_absent_target() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        store
            .begin(manifest("0000000000001-a", SnapshotKind::Apply))
            .save(&journal("0000000000001-a", Stage::Committed))
            .unwrap();
        fs.remove_file(Path::new(&format!(r"{ROOT}\0000000000001-a\payload\0.bin"))).unwrap();
        assert_eq!(store.journal("0000000000001-a").unwrap_err().code, "E_SNAPSHOT_WRITE");
    }

    #[test]
    fn prune_keeps_the_first_and_the_newest() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        for i in 1..=25u64 {
            let id = format!("{i:013}-x");
            let stage = if i == 3 { Stage::Applying { done: 0 } } else { Stage::Committed };
            store.begin(manifest(&id, SnapshotKind::Apply)).save(&journal(&id, stage)).unwrap();
        }
        let deleted = store.prune(KEEP).unwrap();
        // 25 ids: keep #1 and #6..#25; #2..#5 go, except unfinished #3.
        assert_eq!(deleted, vec![format!("{:013}-x", 2), format!("{:013}-x", 4), format!("{:013}-x", 5)]);
        assert_eq!(store.ids().unwrap().len(), 22);
    }

    #[test]
    fn latest_undoable_skips_reversed_and_unfinished_applies() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        store
            .begin(manifest("0000000000001-a", SnapshotKind::Apply))
            .save(&journal("0000000000001-a", Stage::Committed))
            .unwrap();
        store
            .begin(manifest("0000000000002-b", SnapshotKind::Apply))
            .save(&journal("0000000000002-b", Stage::RolledBack))
            .unwrap();
        store
            .begin(manifest("0000000000003-c", SnapshotKind::Apply))
            .save(&journal("0000000000003-c", Stage::Committed))
            .unwrap();
        assert_eq!(store.latest_undoable().unwrap().unwrap().0.id, "0000000000003-c");
        store.mark_undone(&["0000000000003-c".to_string()], "0000000000004-u").unwrap();
        assert_eq!(store.latest_undoable().unwrap().unwrap().0.id, "0000000000001-a");
        store.mark_undone(&["0000000000001-a".to_string()], "0000000000005-u").unwrap();
        assert_eq!(store.latest_undoable().unwrap(), None);
    }

    #[test]
    fn ids_sort_by_time() {
        let a = new_id(5);
        let b = new_id(1_791_694_709_123);
        assert_eq!(a.len(), 22);
        assert!(a < b);
    }

    #[test]
    fn an_incomplete_rollback_is_news_until_something_newer_runs() {
        let fs = FakeFs::new();
        let store = SnapshotStore::new(&fs, Path::new(ROOT));
        let failed = Stage::RollbackIncomplete { failed: vec![0] };
        store
            .begin(manifest("0000000000001-a", SnapshotKind::Apply))
            .save(&journal("0000000000001-a", failed))
            .unwrap();
        assert_eq!(store.latest_incomplete().unwrap(), None, "nothing recorded yet");
        let record = Incomplete {
            id: "0000000000001-a".into(),
            code: "E_DATA_WRITE".into(),
            reason: "disk full".into(),
            steps: vec![Unrestored {
                op_index: 0,
                target: r"file C:\d\a.lua".into(),
                code: "E_DATA_WRITE".into(),
                message: "denied".into(),
                fix: "Copy x over y.".into(),
            }],
        };
        store.write_incomplete(&record).unwrap();
        assert_eq!(store.latest_incomplete().unwrap(), Some(record));
        let text = String::from_utf8(fs.file(format!(r"{ROOT}\0000000000001-a\incomplete.json")).unwrap()).unwrap();
        assert!(text.contains("\"opIndex\": 0"), "{text}");
        // the payload a fix names is the one the journal wrote
        assert_eq!(fs.file(store.payload_path("0000000000001-a", 0)), Some(b"old a".to_vec()));
        // a newer transaction has run since: the record is no longer the news
        store
            .begin(manifest("0000000000002-b", SnapshotKind::Undo))
            .save(&journal("0000000000002-b", Stage::Committed))
            .unwrap();
        assert_eq!(store.latest_incomplete().unwrap(), None);
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- snapshot::tests`
Expected: compile errors (`cannot find type SnapshotStore`, …).

- [ ] **Step 3: Implement it above the tests**
```rust
//! Restore points on disk (spec §5.3, §5.5):
//! `<data>\snapshots\<id>\manifest.json`, `journal.json` and
//! `payload\<op index>.bin`, one file per prior state that existed. Keeps the
//! 20 newest plus the first ever.
use crate::adapters::Fs;
use crate::model::Target;
use crate::plan::Op;
use crate::txn::{Failure, Journal, JournalStore, Snapshot, Stage};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// How many recent restore points to keep, besides the first ever.
pub const KEEP: usize = 20;
const SNAPSHOT_WRITE: &str = "E_SNAPSHOT_WRITE";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotKind {
    Apply,
    Undo,
    RevertAll,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeRef {
    pub id: uuid::Uuid,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub kind: SnapshotKind,
    pub created_ms: u64,
    pub theme: Option<ThemeRef>,
    pub target: Option<Target>,
    /// The plan's English summary lines.
    pub summary: Vec<String>,
    /// For an undo: the apply it reverses.
    pub undoes: Option<String>,
    /// For an apply: the undo or revert-all that reversed it.
    pub undone_by: Option<String>,
}

/// `journal.json`: the txn journal, with each prior state moved out to a
/// payload file. `payload: None` means the target did not exist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JournalFile {
    id: String,
    ops: Vec<Op>,
    snapshots: Vec<PayloadRef>,
    stage: Stage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PayloadRef {
    op_index: usize,
    payload: Option<String>,
}

/// A sortable, unique id: 13-digit milliseconds, then 8 hex characters.
pub fn new_id(now_ms: u64) -> String {
    let tail: String = uuid::Uuid::new_v4().simple().to_string().chars().take(8).collect();
    format!("{now_ms:013}-{tail}")
}

fn fail(e: impl std::fmt::Display) -> Failure {
    Failure { code: SNAPSHOT_WRITE, message: e.to_string() }
}

#[derive(Clone, Copy)]
pub struct SnapshotStore<'a> {
    fs: &'a dyn Fs,
    root: &'a Path,
}

impl<'a> SnapshotStore<'a> {
    pub fn new(fs: &'a dyn Fs, root: &'a Path) -> Self {
        SnapshotStore { fs, root }
    }

    /// Ids of every restore point, oldest first.
    pub fn ids(&self) -> Result<Vec<String>, Failure> {
        let mut ids: Vec<String> = self
            .fs
            .list_dir(self.root)
            .map_err(fail)?
            .into_iter()
            .filter(|e| e.is_dir)
            .filter_map(|e| e.path.file_name().and_then(|n| n.to_str()).map(str::to_string))
            .collect();
        ids.sort();
        Ok(ids)
    }

    fn dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    pub fn manifest(&self, id: &str) -> Result<Manifest, Failure> {
        let path = self.dir(id).join("manifest.json");
        let bytes = self.fs.read(&path).map_err(fail)?.ok_or_else(|| fail(format!("{} is missing", path.display())))?;
        serde_json::from_slice(&bytes).map_err(fail)
    }

    pub fn write_manifest(&self, m: &Manifest) -> Result<(), Failure> {
        let mut bytes = serde_json::to_vec_pretty(m).map_err(fail)?;
        bytes.push(b'\n');
        self.fs.write_atomic(&self.dir(&m.id).join("manifest.json"), &bytes).map_err(fail)
    }

    /// The journal with every prior state read back from its payload file.
    pub fn journal(&self, id: &str) -> Result<Journal, Failure> {
        let dir = self.dir(id);
        let path = dir.join("journal.json");
        let bytes = self.fs.read(&path).map_err(fail)?.ok_or_else(|| fail(format!("{} is missing", path.display())))?;
        let file: JournalFile = serde_json::from_slice(&bytes).map_err(fail)?;
        let mut snapshots = Vec::with_capacity(file.snapshots.len());
        for s in file.snapshots {
            let prior = match s.payload {
                None => None,
                Some(rel) => {
                    let p = dir.join(&rel);
                    Some(self.fs.read(&p).map_err(fail)?.ok_or_else(|| fail(format!("{} is missing", p.display())))?)
                }
            };
            snapshots.push(Snapshot { op_index: s.op_index, prior });
        }
        Ok(Journal { id: file.id, ops: file.ops, snapshots, stage: file.stage })
    }

    /// The newest apply that finished and was not reversed yet.
    pub fn latest_undoable(&self) -> Result<Option<(Manifest, Journal)>, Failure> {
        for id in self.ids()?.into_iter().rev() {
            let m = self.manifest(&id)?;
            if m.kind != SnapshotKind::Apply || m.undone_by.is_some() {
                continue;
            }
            let j = self.journal(&id)?;
            if j.stage == Stage::Committed {
                return Ok(Some((m, j)));
            }
        }
        Ok(None)
    }

    /// Marks every committed apply that is not reversed yet as reversed by `by`.
    pub fn mark_undone(&self, ids: &[String], by: &str) -> Result<(), Failure> {
        for id in ids {
            let mut m = self.manifest(id)?;
            if m.undone_by.is_none() {
                m.undone_by = Some(by.to_string());
                self.write_manifest(&m)?;
            }
        }
        Ok(())
    }

    /// Deletes all but the first-ever and the `keep` newest restore points.
    /// Never deletes one whose journal is unfinished. Returns the deleted ids.
    pub fn prune(&self, keep: usize) -> Result<Vec<String>, Failure> {
        let ids = self.ids()?;
        let mut deleted = Vec::new();
        let cut = ids.len().saturating_sub(keep);
        for id in ids.iter().skip(1).take(cut.saturating_sub(1)) {
            let finished = self.journal(id).map(|j| j.is_finished()).unwrap_or(true);
            if finished {
                self.fs.remove_dir_all(&self.dir(id)).map_err(fail)?;
                deleted.push(id.clone());
            }
        }
        Ok(deleted)
    }

    /// A journal store for a new transaction `manifest.id`.
    pub fn begin(&self, manifest: Manifest) -> DiskJournal<'a> {
        DiskJournal {
            fs: self.fs,
            root: self.root.to_path_buf(),
            id: Some(manifest.id.clone()),
            manifest: Some(manifest),
            written: BTreeSet::new(),
        }
    }

    /// A journal store for crash recovery: `load_unfinished` finds the newest
    /// restore point, and returns its journal when it is unfinished.
    pub fn recovery(&self) -> DiskJournal<'a> {
        DiskJournal { fs: self.fs, root: self.root.to_path_buf(), id: None, manifest: None, written: BTreeSet::new() }
    }

    /// The file that holds the prior state of op `op_index` of restore point `id`.
    pub fn payload_path(&self, id: &str, op_index: usize) -> PathBuf {
        self.dir(id).join("payload").join(format!("{op_index}.bin"))
    }

    /// Writes `incomplete.json` into the restore point the record names.
    pub fn write_incomplete(&self, record: &Incomplete) -> Result<(), Failure> {
        let mut bytes = serde_json::to_vec_pretty(record).map_err(fail)?;
        bytes.push(b'\n');
        self.fs.write_atomic(&self.dir(&record.id).join("incomplete.json"), &bytes).map_err(fail)
    }

    /// The record of the newest restore point, when its transaction ended `RollbackIncomplete`:
    /// nothing has run since, so what it lists may still need a hand. `None` otherwise.
    pub fn latest_incomplete(&self) -> Result<Option<Incomplete>, Failure> {
        let Some(id) = self.ids()?.pop() else {
            return Ok(None);
        };
        match self.fs.read(&self.dir(&id).join("incomplete.json")).map_err(fail)? {
            None => Ok(None),
            Some(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(fail),
        }
    }
}

/// One op a rollback could not put back, as `incomplete.json` keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unrestored {
    pub op_index: usize,
    /// What the op touched, in plain English (`exec::op_label`).
    pub target: String,
    /// The code and the message of the restore that failed.
    pub code: String,
    pub message: String,
    /// How to put the target back by hand (`exec::fix_by_hand`).
    pub fix: String,
}

/// `incomplete.json`: what a transaction that ended `RollbackIncomplete` could not put back,
/// and how to do it by hand (spec §5.5). The journal keeps only the positions of those ops,
/// because a `txn::Failure` can't be read back from JSON; this record keeps the rest, next to
/// the journal, so it survives a restart. It is not in `state.json`, which is itself one of the
/// targets a rollback may have left as it was.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Incomplete {
    /// The restore point of the transaction.
    pub id: String,
    /// The code and the reason of the failure that started the rollback.
    pub code: String,
    pub reason: String,
    pub steps: Vec<Unrestored>,
}

/// `txn::JournalStore` on disk. Payload files are written once each, before
/// the journal that names them (write-ahead, spec §5.5).
pub struct DiskJournal<'a> {
    fs: &'a dyn Fs,
    root: PathBuf,
    id: Option<String>,
    manifest: Option<Manifest>,
    written: BTreeSet<usize>,
}

impl JournalStore for DiskJournal<'_> {
    fn save(&mut self, j: &Journal) -> Result<(), Failure> {
        let id = self.id.get_or_insert_with(|| j.id.clone()).clone();
        let store = SnapshotStore { fs: self.fs, root: &self.root };
        if let Some(m) = self.manifest.take() {
            store.write_manifest(&m)?;
        }
        let dir = self.root.join(&id);
        let mut refs = Vec::with_capacity(j.snapshots.len());
        for s in &j.snapshots {
            let payload = match &s.prior {
                None => None,
                Some(bytes) => {
                    let rel = format!("payload/{}.bin", s.op_index);
                    if !self.written.contains(&s.op_index) {
                        self.fs.write_atomic(&dir.join(&rel), bytes).map_err(fail)?;
                        self.written.insert(s.op_index);
                    }
                    Some(rel)
                }
            };
            refs.push(PayloadRef { op_index: s.op_index, payload });
        }
        let file = JournalFile { id: j.id.clone(), ops: j.ops.clone(), snapshots: refs, stage: j.stage.clone() };
        let mut bytes = serde_json::to_vec_pretty(&file).map_err(fail)?;
        bytes.push(b'\n');
        self.fs.write_atomic(&dir.join("journal.json"), &bytes).map_err(fail)
    }

    fn load_unfinished(&mut self) -> Result<Option<Journal>, Failure> {
        let store = SnapshotStore { fs: self.fs, root: &self.root };
        let Some(id) = store.ids()?.pop() else {
            return Ok(None);
        };
        if !self.fs.exists(&self.root.join(&id).join("journal.json")) {
            return Ok(None);
        }
        let j = store.journal(&id)?;
        if j.is_finished() {
            return Ok(None);
        }
        self.written = j.snapshots.iter().filter(|s| s.prior.is_some()).map(|s| s.op_index).collect();
        self.id = Some(id);
        Ok(Some(j))
    }
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- snapshot::tests`
Expected: `7 passed`.

- [ ] **Step 5: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 6: Commit**

Message:
```text
feat(core): store restore points and txn journals on disk

Each restore point is snapshots\<id>\manifest.json, journal.json and one
payload\<op>.bin per prior state, written before the journal names it.
A missing payload is an error, never "the target was absent". Prune keeps
the first and the 20 newest and never an unfinished journal.

incomplete.json, next to the journal, keeps what a rollback that ended
RollbackIncomplete could not put back and how to fix each by hand, so a
restart does not lose it (spec 5.5).
```
Run:
```bash
git add -- crates/core/src/lib.rs crates/core/src/snapshot.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 10: `detect`

**Files:**
- Create: `crates/core/src/detect.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod detect;`)

**Interfaces:**
- Consumes: `adapters::{System, SystemInfo, WtLocation, WtVariant, DirState, AdapterError, RegValue, program_data_dir, AutoRun, Fs, REG_SZ, REG_EXPAND_SZ}` and `adapters::fake::FakeMachine` (Task 1: fields `fs`, `autorun`, `fonts`, `wt.locations`, `host.short`; `FakeAutoRun::set_machine`, `FakeHost::set_dir_state`, `FakeMachine::system()`; the fake user SID is `S-1-5-21-1-2-3-1001`, the fake ProgramData is `C:\ProgramData`, the fake fonts are `Arial` and `Cascadia Mono`); `plan::{Machine, Paths}` (Phase 1a).
- Produces (in `cmdstudio_core::detect`):
  - `Detected { machine: plan::Machine, info: SystemInfo, wt: Vec<WtLocation>, program_data: (PathBuf, DirState), user_sid: String }`.
  - `DetectError(pub AdapterError)` with `code() -> "E_DATA_WRITE"`.
  - `detect(sys: System<'_>, paths: &Paths) -> Result<Detected, DetectError>`.

**What it does** (spec §5.2, §5.4 steps 2–3): creates the data folder (the only write: `GetShortPathNameW` needs a path that exists), reads its 8.3 name, the user's SID, the ProgramData folder's state, which Windows Terminal `settings.json` files exist, both AutoRun values as unexpanded text, and the GDI font families. `Machine.program_data_dir` is offered to the planner only when that folder is absent (it will be created private) or already private; a folder someone else controls is never offered, so the planner falls back or fails with `E_CLINK_PATH` (spec §5.4 step 3). An AutoRun value that is not text (for example a `REG_DWORD` left by another program) reads as `None` for the planner; its exact bytes are still captured for undo by the executor (Task 11). Any failure here is `E_DATA_WRITE`: in practice only creating the data folder can fail; the reads fail only when Windows itself is broken.

- [ ] **Step 1: Write the failing tests** — add `pub mod detect;` to `crates/core/src/lib.rs` and create `crates/core/src/detect.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeMachine;
    use crate::adapters::{AutoRun, Fs, RegValue, WtVariant, REG_EXPAND_SZ, REG_SZ};
    use std::path::Path;

    const DATA: &str = r"C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio";
    const SHORT: &str = r"C:\Users\3607~1\AppData\Local\AIRESI~1.CMD";

    fn machine() -> FakeMachine {
        let mut m = FakeMachine::new();
        m.host.short.insert(PathBuf::from(DATA), PathBuf::from(SHORT));
        let store = PathBuf::from(r"C:\L\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json");
        let preview =
            PathBuf::from(r"C:\L\Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json");
        m.fs.put(&store, b"{}");
        m.wt.locations = vec![
            WtLocation { variant: WtVariant::Store, installed: true, settings: store },
            WtLocation { variant: WtVariant::Preview, installed: true, settings: preview },
        ];
        m
    }

    #[test]
    fn fills_the_planner_facts() {
        let m = machine();
        m.autorun.write_user(&RegValue::from_text(REG_EXPAND_SZ, r"%USERPROFILE%\x.cmd")).unwrap();
        m.autorun.set_machine(Some(RegValue::from_text(REG_SZ, "clink inject")));
        let d = detect(m.system(), &Paths { data: PathBuf::from(DATA) }).unwrap();
        assert!(m.fs.is_dir(Path::new(DATA)));
        assert_eq!(d.machine.data_short, Some(PathBuf::from(SHORT)));
        assert_eq!(d.machine.wt_settings.len(), 1, "only settings files that exist");
        assert_eq!(d.machine.autorun_hkcu.as_deref(), Some(r"%USERPROFILE%\x.cmd"), "unexpanded text");
        assert_eq!(d.machine.autorun_hklm.as_deref(), Some("clink inject"));
        assert_eq!(d.machine.fonts, vec!["Arial".to_string(), "Cascadia Mono".to_string()]);
        let pd = PathBuf::from(r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001");
        assert_eq!(d.machine.program_data_dir, Some(pd.clone()));
        assert_eq!(d.program_data, (pd, DirState::Absent));
    }

    #[test]
    fn a_program_data_folder_someone_else_controls_is_not_offered() {
        let m = machine();
        let pd = PathBuf::from(r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001");
        m.host.set_dir_state(&pd, DirState::Unsafe("S-1-5-32-545 may change it".into()));
        let d = detect(m.system(), &Paths { data: PathBuf::from(DATA) }).unwrap();
        assert_eq!(d.machine.program_data_dir, None);
        m.host.set_dir_state(&pd, DirState::Private);
        let d = detect(m.system(), &Paths { data: PathBuf::from(DATA) }).unwrap();
        assert_eq!(d.machine.program_data_dir, Some(pd));
    }

    #[test]
    fn a_non_text_autorun_reads_as_none() {
        let m = machine();
        m.autorun.write_user(&RegValue { kind: 4, bytes: vec![1, 0, 0, 0] }).unwrap();
        let d = detect(m.system(), &Paths { data: PathBuf::from(DATA) }).unwrap();
        assert_eq!(d.machine.autorun_hkcu, None);
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- detect::tests`
Expected: compile errors (`cannot find function detect`, …).

- [ ] **Step 3: Implement it above the tests**
```rust
//! Reads the machine facts the planner needs (spec §5.2 `detect`). Read only,
//! except that it creates the data folder, because `GetShortPathNameW` works
//! only on a path that exists (spec §5.4 step 2).
use crate::adapters::{program_data_dir, AdapterError, DirState, System, SystemInfo, WtLocation};
use crate::plan::{Machine, Paths};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    pub machine: Machine,
    pub info: SystemInfo,
    /// Every Windows Terminal variant, installed or not, with where its settings live.
    pub wt: Vec<WtLocation>,
    /// `%ProgramData%\ai.resincore.cmdstudio-<SID>` and what detect found there.
    pub program_data: (PathBuf, DirState),
    pub user_sid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("could not read the machine: {0}")]
pub struct DetectError(pub AdapterError);

impl DetectError {
    pub fn code(&self) -> &'static str {
        "E_DATA_WRITE"
    }
}

pub fn detect(sys: System<'_>, paths: &Paths) -> Result<Detected, DetectError> {
    sys.fs.create_dir_all(&paths.data).map_err(DetectError)?;
    let data_short = sys.host.short_path(&paths.data).map_err(DetectError)?;
    let sid = sys.host.user_sid().map_err(DetectError)?;
    let pd = program_data_dir(&sys.host.program_data().map_err(DetectError)?, &sid);
    let pd_state = sys.host.private_dir_state(&pd).map_err(DetectError)?;
    let wt = sys.wt.locations();
    let wt_settings = wt.iter().filter(|l| sys.fs.exists(&l.settings)).map(|l| l.settings.clone()).collect();
    let text = |v: Option<crate::adapters::RegValue>| v.and_then(|v| v.text());
    let machine = Machine {
        wt_settings,
        autorun_hkcu: text(sys.autorun.read_user().map_err(DetectError)?),
        autorun_hklm: text(sys.autorun.read_machine().map_err(DetectError)?),
        fonts: sys.fonts.families().map_err(DetectError)?,
        data_short: Some(data_short),
        // Usable when absent (created at install) or private (spec §5.4).
        program_data_dir: matches!(pd_state, DirState::Absent | DirState::Private).then(|| pd.clone()),
    };
    Ok(Detected { machine, info: sys.host.system_info(), wt, program_data: (pd, pd_state), user_sid: sid })
}
```
These are all six fields of the shipped `plan::Machine`. `autorun_hkcu` and `autorun_hklm` are the raw text, never expanded, as its docs say since Task 0.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- detect::tests`
Expected: `3 passed`.

- [ ] **Step 5: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 6: Commit**

Message:
```text
feat(core): detect the machine facts the planner needs

detect creates the data folder (8.3 names need a path that exists), then
reads its short path, the user's SID, the ProgramData folder's state, the
settings.json files that exist, both AutoRun values as unexpanded text
and the GDI font families. A ProgramData folder someone else controls is
never offered to the planner.
```
Run:
```bash
git add -- crates/core/src/lib.rs crates/core/src/detect.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 11: The real executor, undo executor and undo verifier

**Files:**
- Create: `crates/core/src/exec.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod exec;`)

**Interfaces:**
- Consumes:
  - Task 1: `adapters::{System, RegValue, REG_SZ, REG_EXPAND_SZ, Created, DirState, AdapterError, program_data_dir, AutoRun, Fonts, Fs}`; fakes: `FakeMachine::new()/system()`, `FakeFs::{put, file, snapshot, fail_writes_to, fail_removes_of}`, `FakeAutoRun::{snapshot, set_key_exists, fail_writes}`, `FakeFonts::{snapshot, knows_file, set_key_exists}` (fake fonts folder `C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts`), `FakeHost::{set_dir_state, dir_state}`.
  - Task 2: `resources::{Resources, Manifest, check_bundle, sha256_hex, NERD_FONT_FILE}`, `Manifest::bundle(Component) -> &Bundle`, `Bundle { version, files: BTreeMap<String, String> }`, `fake_resources(&FakeFs, &Path) -> Resources` (files `omp\oh-my-posh.exe` = `omp-binary`, `clink\clink_x64.exe` = `clink-exe`, `clink\clink_dll_x64.dll` = `clink-dll`, `fonts\CaskaydiaCoveNerdFont-Regular.ttf` = `font-bytes`).
  - Task 7: `wt_edit::{apply_edit, WtEdit}`. Task 8: `state::{AutoRunRecord, RegValueRecord}`, `Paths::runtime_root()`.
  - Phase 1a: `plan::{Component, Op, Paths, NERD_FONT_FAMILY}`, `txn::{Executor, Verifier, Failure}`.
- Produces (in `cmdstudio_core::exec`):
  - `font_target(sys: System<'_>, resources: &Resources, family: &str) -> Option<(PathBuf, PathBuf, String)>` — (bundled file, installed file, registry value name `"<family> (TrueType)"`).
  - `SystemExecutor<'a>`: `new(System<'a>, &'a Resources)`; `impl txn::Executor`.
  - `UndoExecutor<'a>`: `new(SystemExecutor<'a>, priors: Vec<(Op, Option<Vec<u8>>)>)`; `impl txn::Executor` — its `apply(op)` restores `op`'s target to the prior captured by the apply being undone.
  - `UndoVerifier<'a>`: `new(SystemExecutor<'a>, expected: Vec<(Op, Option<Vec<u8>>)>)`; `impl txn::Verifier`.
  - `op_label(&Op) -> String`; `same_state(&Op, &Option<Vec<u8>>, &Option<Vec<u8>>) -> bool`; `sweep(System<'_>, &Paths)`.
  - `fix_by_hand(op: &Op, prior: &Option<Vec<u8>>, payload: &Path) -> String`: how a person puts `op`'s target back when a rollback could not (spec §5.5). A file: "Copy `<payload>` over `<path>`." (or "Delete `<path>`." when it did not exist); a settings.json: the same after closing Windows Terminal; AutoRun: open `HKEY_CURRENT_USER\Software\Microsoft\Command Processor` in Registry Editor and set the value back to the captured text (or delete it); a runtime folder or the font: close cmd and Windows Terminal, then run `cmdstudio --cleanup`. Task 13 stores it in `incomplete.json`.

**What each op captures, does and restores** (spec §5.4, §5.5). Every `apply` and every `restore` can run again with the same result (the txn engine relies on that after a crash).

| Op | prior (`capture`) | `apply` | `restore` | codes |
|---|---|---|---|---|
| `InstallRuntime { component, dir }` | JSON `{dirExisted, parentExisted}` | check the bundle's hashes; `dir` exists with the same files → nothing; with other files → fail (never overwrite); else copy into `.<name>.partial-<8 hex>` beside it and rename. Under ProgramData, first create the parent with the private ACL, or check an existing one is still private | if we created `dir`: rename it to `.<name>.removing-<8 hex>` (fails whole while a file is in use), then delete best-effort; remove a parent we created when empty. If `dir` existed before and is gone now (an undo removed it and is being rolled back), install it again from the bundle | `E_RUNTIME_MISSING`, `E_CLINK_PATH` |
| `WriteFile { path, contents }` | the file's bytes, `None` if absent | atomic write | write the old bytes back, or delete | `E_DATA_WRITE` |
| `WtApply { settings, … }` | the file's bytes | `wt_edit::apply_edit`; write only when the text changed; a file that does not parse is never written | write the old bytes when they differ; never create or delete a settings.json | `E_WT_PARSE`, `E_WT_WRITE` |
| `SetAutoRun { value }` | JSON `AutoRunRecord` (key existed; type + raw bytes) | write `REG_EXPAND_SZ` if the current value is, else `REG_SZ`; `None` deletes the value | write the exact old type and bytes, or delete; delete the key only if it did not exist and is now empty | `E_AUTORUN_WRITE` |
| `InstallFont { family }` | JSON `{fileExisted, dirExisted, keyExisted, registration}` | check the bundled file's hash; copy it to the user fonts folder unless identical; register; `AddFontResourceW` | if we added the file: unload, delete best-effort; remove the fonts folder if we created it and it is empty; put the old registration back or delete ours; delete the registration key if apply created it and it is empty | `E_FONT_INSTALL` (`E_RUNTIME_MISSING` for a bad bundled file) |

Failing to read a prior state is `E_SNAPSHOT_WRITE`. A font file that Windows Terminal still holds cannot be deleted, but once its registration is gone the font is uninstalled; `--cleanup` (Task 14) deletes the file later. `same_state` is how the undo verifier decides "back to what it was": files and settings byte for byte, AutoRun by type and bytes, fonts by registration only, runtimes by whether the folder exists. `sweep` deletes the `.partial-` / `.removing-` folders an interrupted run left in the runtime folders.

- [ ] **Step 1: Write the failing tests** — add `pub mod exec;` to `crates/core/src/lib.rs` and create `crates/core/src/exec.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeMachine;
    use crate::adapters::{AutoRun, Fonts, Fs};
    use crate::plan::NERD_FONT_FAMILY;
    use crate::resources::fake_resources;
    use crate::resources::NERD_FONT_FILE;
    use serde_json::{json, Map};

    const RES: &str = r"C:\app\resources";

    fn setup() -> (FakeMachine, Resources, Paths) {
        let m = FakeMachine::new();
        // `%LOCALAPPDATA%\Microsoft\Windows` exists on every account; `Fonts` under it may not.
        m.fs.create_dir_all(Path::new(r"C:\Users\tester\AppData\Local\Microsoft\Windows")).unwrap();
        let res = fake_resources(&m.fs, Path::new(RES));
        m.fonts.knows_file(NERD_FONT_FILE, NERD_FONT_FAMILY);
        (m, res, Paths { data: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio") })
    }

    /// capture → apply → restore leaves the target as it was, twice over.
    fn round_trip(m: &FakeMachine, res: &Resources, op: &Op) {
        let fs_before = m.fs.snapshot();
        let reg_before = m.autorun.snapshot();
        let fonts_before = m.fonts.snapshot();
        let mut ex = SystemExecutor::new(m.system(), res);
        let prior = ex.capture(op).unwrap();
        ex.apply(op).unwrap();
        ex.apply(op).unwrap();
        ex.restore(op, &prior).unwrap();
        ex.restore(op, &prior).unwrap();
        assert_eq!(m.fs.snapshot(), fs_before, "{op:?}");
        assert_eq!(m.autorun.snapshot(), reg_before, "{op:?}");
        assert_eq!(m.fonts.snapshot(), fonts_before, "{op:?}");
    }

    #[test]
    fn every_op_restores_byte_for_byte() {
        let (m, res, paths) = setup();
        m.fs.put(paths.generated().join("init.lua"), b"old");
        m.fs.put(r"C:\wt\settings.json", br#"{ "profiles": { "list": [] } } // c"#);
        m.autorun.write_user(&RegValue::from_text(REG_EXPAND_SZ, "%A% & echo hi")).unwrap();
        let mut profile = Map::new();
        profile.insert("colorScheme".into(), json!("CMD Studio"));
        for op in [
            Op::InstallRuntime { component: Component::Omp, dir: paths.omp_dir() },
            Op::InstallRuntime { component: Component::Clink, dir: paths.clink_dir() },
            Op::WriteFile { path: paths.generated().join("init.lua"), contents: "new".into() },
            Op::WriteFile { path: paths.generated().join("banner.lua"), contents: "new".into() },
            Op::WtApply {
                settings: PathBuf::from(r"C:\wt\settings.json"),
                scheme: json!({ "name": "CMD Studio" }),
                profile,
                set_default: true,
            },
            Op::SetAutoRun { value: Some("%A% & echo hi & ours".into()) },
            Op::SetAutoRun { value: None },
            Op::InstallFont { family: NERD_FONT_FAMILY.into() },
        ] {
            round_trip(&m, &res, &op);
        }
        // An account that never had a per-user font has no registration key: it goes again.
        m.fonts.set_key_exists(false);
        round_trip(&m, &res, &Op::InstallFont { family: NERD_FONT_FAMILY.into() });
    }

    #[test]
    fn autorun_keeps_expand_sz_and_a_created_key_goes_again() {
        let (m, res, _) = setup();
        m.autorun.write_user(&RegValue::from_text(REG_EXPAND_SZ, "%X%")).unwrap();
        let mut ex = SystemExecutor::new(m.system(), &res);
        ex.apply(&Op::SetAutoRun { value: Some("%X% & ours".into()) }).unwrap();
        assert_eq!(m.autorun.read_user().unwrap().unwrap().kind, REG_EXPAND_SZ);

        let (m, res, _) = setup();
        m.autorun.delete_user().unwrap();
        m.autorun.set_key_exists(false);
        round_trip(&m, &res, &Op::SetAutoRun { value: Some("ours".into()) });
        assert_eq!(m.autorun.snapshot(), (false, None));
    }

    #[test]
    fn runtime_install_is_a_no_op_when_identical_and_refuses_a_different_folder() {
        let (m, res, paths) = setup();
        let mut ex = SystemExecutor::new(m.system(), &res);
        let op = Op::InstallRuntime { component: Component::Clink, dir: paths.clink_dir() };
        ex.apply(&op).unwrap();
        assert_eq!(m.fs.file(paths.clink_exe()), Some(b"clink-exe".to_vec()));
        ex.apply(&op).unwrap();
        m.fs.put(paths.clink_dir().join("clink_dll_x64.dll"), b"other");
        assert_eq!(ex.apply(&op).unwrap_err().code, "E_RUNTIME_MISSING");
        assert_eq!(
            m.fs.file(paths.clink_dir().join("clink_dll_x64.dll")),
            Some(b"other".to_vec()),
            "never overwritten"
        );
    }

    #[test]
    fn a_runtime_in_use_is_not_half_deleted() {
        let (m, res, paths) = setup();
        let mut ex = SystemExecutor::new(m.system(), &res);
        let op = Op::InstallRuntime { component: Component::Clink, dir: paths.clink_dir() };
        let prior = ex.capture(&op).unwrap();
        ex.apply(&op).unwrap();
        m.fs.fail_removes_of(paths.clink_dir().join("clink_dll_x64.dll"));
        assert!(ex.restore(&op, &prior).is_err());
        assert_eq!(m.fs.file(paths.clink_exe()), Some(b"clink-exe".to_vec()));
    }

    #[test]
    fn program_data_runtime_gets_a_private_folder_and_refuses_a_foreign_one() {
        let (m, res, _) = setup();
        let pd = PathBuf::from(r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001");
        let op = Op::InstallRuntime { component: Component::Clink, dir: pd.join("clink-1.9.34") };
        let mut ex = SystemExecutor::new(m.system(), &res);
        ex.apply(&op).unwrap();
        assert_eq!(m.host.dir_state(&pd), DirState::Private);

        let (m, res, _) = setup();
        // Someone created the folder after detect looked (spec §5.4 TOCTOU).
        m.host.set_dir_state(&pd, DirState::Unsafe("S-1-5-32-545 may change it".into()));
        let mut ex = SystemExecutor::new(m.system(), &res);
        assert_eq!(ex.apply(&op).unwrap_err().code, "E_CLINK_PATH");
        assert!(!m.fs.exists(&pd.join("clink-1.9.34")));
    }

    #[test]
    fn wt_apply_refuses_a_broken_file_and_never_writes_it() {
        let (m, res, _) = setup();
        m.fs.put(r"C:\wt\settings.json", b"{ \"profiles\": ");
        let op = Op::WtApply {
            settings: PathBuf::from(r"C:\wt\settings.json"),
            scheme: json!({ "name": "CMD Studio" }),
            profile: Map::new(),
            set_default: false,
        };
        let mut ex = SystemExecutor::new(m.system(), &res);
        assert_eq!(ex.apply(&op).unwrap_err().code, "E_WT_PARSE");
        assert_eq!(m.fs.file(r"C:\wt\settings.json"), Some(b"{ \"profiles\": ".to_vec()));
    }

    #[test]
    fn a_font_file_in_use_still_ends_uninstalled() {
        let (m, res, _) = setup();
        let op = Op::InstallFont { family: NERD_FONT_FAMILY.into() };
        let mut ex = SystemExecutor::new(m.system(), &res);
        let prior = ex.capture(&op).unwrap();
        ex.apply(&op).unwrap();
        assert!(m.fonts.families().unwrap().contains(&NERD_FONT_FAMILY.to_string()));
        let dest = m.fonts.user_fonts_dir().join(NERD_FONT_FILE);
        m.fs.fail_removes_of(&dest);
        ex.restore(&op, &prior).unwrap();
        assert!(!m.fonts.families().unwrap().contains(&NERD_FONT_FAMILY.to_string()));
        assert_eq!(m.fonts.registration("CaskaydiaCove NF (TrueType)").unwrap(), None);
    }

    #[test]
    fn undo_executor_applies_the_captured_priors() {
        let (m, res, paths) = setup();
        let op = Op::WriteFile { path: paths.generated().join("a.lua"), contents: "new".into() };
        let mut undo =
            UndoExecutor::new(SystemExecutor::new(m.system(), &res), vec![(op.clone(), Some(b"old".to_vec()))]);
        m.fs.put(paths.generated().join("a.lua"), b"new");
        undo.apply(&op).unwrap();
        assert_eq!(m.fs.file(paths.generated().join("a.lua")), Some(b"old".to_vec()));
        let other = Op::SetAutoRun { value: None };
        assert_eq!(undo.apply(&other).unwrap_err().code, "E_SNAPSHOT_WRITE");
    }

    #[test]
    fn failures_carry_the_spec_codes() {
        let (m, res, paths) = setup();
        m.autorun.fail_writes();
        let mut ex = SystemExecutor::new(m.system(), &res);
        let err = ex.apply(&Op::SetAutoRun { value: Some("x".into()) }).unwrap_err();
        assert_eq!(err.code, "E_AUTORUN_WRITE");
        let file = paths.generated().join("init.lua");
        m.fs.fail_writes_to(&file);
        let err = ex.apply(&Op::WriteFile { path: file, contents: "x".into() }).unwrap_err();
        assert_eq!(err.code, "E_DATA_WRITE");
        let err = ex
            .apply(&Op::WtApply {
                settings: PathBuf::from(r"C:\gone\settings.json"),
                scheme: json!({}),
                profile: Map::new(),
                set_default: false,
            })
            .unwrap_err();
        assert_eq!(err.code, "E_WT_WRITE");
    }

    #[test]
    fn a_runtime_that_was_there_is_put_back() {
        // An undo removes the runtime folder. If a later step of the undo fails, its rollback
        // must bring the folder back: the theme would otherwise point at a runtime that is gone.
        let (m, res, paths) = setup();
        let mut ex = SystemExecutor::new(m.system(), &res);
        let op = Op::InstallRuntime { component: Component::Clink, dir: paths.clink_dir() };
        ex.apply(&op).unwrap();
        let there = ex.capture(&op).unwrap();
        m.fs.remove_dir_all(&paths.clink_dir()).unwrap();
        ex.restore(&op, &there).unwrap();
        ex.restore(&op, &there).unwrap();
        assert_eq!(m.fs.file(paths.clink_exe()), Some(b"clink-exe".to_vec()));
    }

    #[test]
    fn every_step_has_a_way_to_fix_it_by_hand() {
        let payload = Path::new(r"C:\d\snapshots\0000000000001-a\payload\3.bin");
        let file = PathBuf::from(r"C:\d\generated\init.lua");
        let write = Op::WriteFile { path: file.clone(), contents: "x".into() };
        assert_eq!(
            fix_by_hand(&write, &Some(b"old".to_vec()), payload),
            format!("Copy {} over {}.", payload.display(), file.display())
        );
        assert_eq!(fix_by_hand(&write, &None, payload), format!("Delete {}.", file.display()));
        let settings = PathBuf::from(r"C:\wt\settings.json");
        let wt = Op::WtApply {
            settings: settings.clone(),
            scheme: serde_json::json!({}),
            profile: serde_json::Map::new(),
            set_default: false,
        };
        assert_eq!(
            fix_by_hand(&wt, &Some(b"{}".to_vec()), payload),
            format!("Close Windows Terminal, then copy {} over {}.", payload.display(), settings.display())
        );
        assert_eq!(
            fix_by_hand(&wt, &None, payload),
            format!("Close Windows Terminal, then delete {}.", settings.display())
        );
        let autorun = Op::SetAutoRun { value: Some("x".into()) };
        let before = to_json(&AutoRunRecord {
            key_existed: true,
            value: Some(RegValueRecord::from_reg(&RegValue::from_text(REG_EXPAND_SZ, "%A% & echo hi"))),
        })
        .unwrap();
        assert!(fix_by_hand(&autorun, &Some(before), payload).ends_with("set AutoRun back to: %A% & echo hi"));
        let absent = to_json(&AutoRunRecord { key_existed: true, value: None }).unwrap();
        assert!(fix_by_hand(&autorun, &Some(absent), payload).ends_with("delete the AutoRun value."));
        for op in [
            Op::InstallRuntime { component: Component::Omp, dir: PathBuf::from(r"C:\d\runtime\omp-31.5.0") },
            Op::InstallFont { family: NERD_FONT_FAMILY.into() },
        ] {
            assert!(fix_by_hand(&op, &None, payload).contains("cmdstudio --cleanup"), "{op:?}");
        }
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- exec::tests`
Expected: compile errors (`cannot find struct SystemExecutor`, …).

- [ ] **Step 3: Implement it above the tests**
```rust
//! The real `txn::Executor` (spec §5.4, §5.5): runs each plan operation
//! through the adapters. `capture` records the prior state, `restore` puts it
//! back and may run any number of times.
use crate::adapters::{program_data_dir, Created, DirState, RegValue, System, REG_EXPAND_SZ, REG_SZ};
use crate::plan::{Component, Op, Paths};
use crate::resources::{check_bundle, sha256_hex, Manifest, Resources};
use crate::state::{AutoRunRecord, RegValueRecord};
use crate::txn::{Executor, Failure};
use crate::wt_edit::{apply_edit, WtEdit};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn failure(code: &'static str, message: impl std::fmt::Display) -> Failure {
    Failure { code, message: message.to_string() }
}

/// Prior state of an `InstallRuntime` target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimePrior {
    dir_existed: bool,
    parent_existed: bool,
}

/// Prior state of an `InstallFont` target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FontPrior {
    file_existed: bool,
    dir_existed: bool,
    /// A new account may not have the registration key yet; restore deletes the one apply made.
    key_existed: bool,
    registration: Option<String>,
}

/// Where a bundled font goes: (bundled file, installed file, registry value name).
pub fn font_target(sys: System<'_>, resources: &Resources, family: &str) -> Option<(PathBuf, PathBuf, String)> {
    let src = resources.font_file(family)?;
    let dest = sys.fonts.user_fonts_dir().join(src.file_name()?);
    Some((src, dest, format!("{family} (TrueType)")))
}

fn to_json<T: Serialize>(v: &T) -> Result<Vec<u8>, Failure> {
    serde_json::to_vec(v).map_err(|e| failure("E_SNAPSHOT_WRITE", e))
}

fn from_json<T: for<'de> Deserialize<'de>>(prior: &Option<Vec<u8>>, code: &'static str) -> Result<T, Failure> {
    let bytes = prior.as_deref().ok_or_else(|| failure(code, "the restore point has no prior state for this step"))?;
    serde_json::from_slice(bytes).map_err(|e| failure(code, e))
}

/// A sibling name in the same folder: `.<name>.<what>-<8 hex>`.
fn sibling(dir: &Path, what: &str) -> PathBuf {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tail: String = uuid::Uuid::new_v4().simple().to_string().chars().take(8).collect();
    dir.with_file_name(format!(".{name}.{what}-{tail}"))
}

pub struct SystemExecutor<'a> {
    sys: System<'a>,
    resources: &'a Resources,
    manifest: Option<Manifest>,
}

impl<'a> SystemExecutor<'a> {
    pub fn new(sys: System<'a>, resources: &'a Resources) -> Self {
        SystemExecutor { sys, resources, manifest: None }
    }

    fn manifest(&mut self) -> Result<&Manifest, Failure> {
        if self.manifest.is_none() {
            let m = self.resources.manifest(self.sys.fs).map_err(|e| failure(e.code(), e))?;
            self.manifest = Some(m);
        }
        self.manifest.as_ref().ok_or_else(|| failure("E_RUNTIME_MISSING", "no manifest"))
    }

    /// The user's own ProgramData folder: create it private, or check that
    /// an existing one still is (spec §5.4 step 3, TOCTOU).
    fn ensure_private(&self, dir: &Path) -> Result<(), Failure> {
        let created = self.sys.host.create_private_dir(dir).map_err(|e| failure("E_CLINK_PATH", e))?;
        if created == Created::AlreadyExisted {
            match self.sys.host.private_dir_state(dir).map_err(|e| failure("E_CLINK_PATH", e))? {
                DirState::Private => {}
                other => {
                    return Err(failure(
                        "E_CLINK_PATH",
                        format!("{} is not private to this user: {other:?}", dir.display()),
                    ))
                }
            }
        }
        Ok(())
    }

    fn is_program_data_parent(&self, dir: &Path) -> Result<bool, Failure> {
        let (pd, sid) = (self.sys.host.program_data(), self.sys.host.user_sid());
        let (pd, sid) = (pd.map_err(|e| failure("E_CLINK_PATH", e))?, sid.map_err(|e| failure("E_CLINK_PATH", e))?);
        Ok(dir.parent() == Some(program_data_dir(&pd, &sid).as_path()))
    }

    fn install_runtime(&mut self, component: Component, dir: &Path) -> Result<(), Failure> {
        let bundle = self.manifest()?.bundle(component).clone();
        let src = self.resources.dir(component);
        let fs = self.sys.fs;
        check_bundle(fs, &src, &bundle).map_err(|e| failure(e.code(), e))?;
        if fs.exists(dir) {
            // Never overwrite a runtime folder (spec §5.4 Fs): same files is a no-op.
            return check_bundle(fs, dir, &bundle).map_err(|e| {
                failure("E_RUNTIME_MISSING", format!("{} already exists with other files: {e}", dir.display()))
            });
        }
        if self.is_program_data_parent(dir)? {
            if let Some(parent) = dir.parent() {
                self.ensure_private(parent)?;
            }
        }
        let staging = sibling(dir, "partial");
        let copied = (|| {
            for name in bundle.files.keys() {
                let bytes = fs
                    .read(&src.join(name))?
                    .ok_or_else(|| crate::adapters::AdapterError::new(name.clone(), "missing"))?;
                fs.write_atomic(&staging.join(name), &bytes)?;
            }
            fs.rename(&staging, dir)
        })();
        if let Err(e) = copied {
            let _ = fs.remove_dir_all(&staging);
            return Err(failure("E_RUNTIME_MISSING", e));
        }
        Ok(())
    }

    fn install_font(&mut self, family: &str) -> Result<(), Failure> {
        let (src, dest, value_name) = font_target(self.sys, self.resources, family)
            .ok_or_else(|| failure("E_FONT_INSTALL", format!("no bundled file for {family}")))?;
        let expected =
            self.manifest()?.fonts.files.get(src.file_name().and_then(|n| n.to_str()).unwrap_or_default()).cloned();
        let fs = self.sys.fs;
        let bytes = fs
            .read(&src)
            .map_err(|e| failure("E_FONT_INSTALL", e))?
            .ok_or_else(|| failure("E_FONT_INSTALL", format!("{} is missing", src.display())))?;
        if expected.as_deref() != Some(sha256_hex(&bytes).as_str()) {
            return Err(failure("E_RUNTIME_MISSING", format!("{} does not match the pinned hash", src.display())));
        }
        if fs.read(&dest).map_err(|e| failure("E_FONT_INSTALL", e))?.as_deref() != Some(bytes.as_slice()) {
            fs.write_atomic(&dest, &bytes).map_err(|e| failure("E_FONT_INSTALL", e))?;
        }
        let file = dest.to_str().ok_or_else(|| failure("E_FONT_INSTALL", "font path is not Unicode"))?;
        self.sys.fonts.register(&value_name, file).map_err(|e| failure("E_FONT_INSTALL", e))?;
        self.sys.fonts.load(&dest).map_err(|e| failure("E_FONT_INSTALL", e))
    }
}

impl Executor for SystemExecutor<'_> {
    fn capture(&mut self, op: &Op) -> Result<Option<Vec<u8>>, Failure> {
        let fs = self.sys.fs;
        match op {
            Op::InstallRuntime { dir, .. } => {
                let parent_existed = dir.parent().is_none_or(|p| fs.exists(p));
                to_json(&RuntimePrior { dir_existed: fs.exists(dir), parent_existed }).map(Some)
            }
            Op::WriteFile { path, .. } => fs.read(path).map_err(|e| failure("E_SNAPSHOT_WRITE", e)),
            Op::WtApply { settings, .. } => fs.read(settings).map_err(|e| failure("E_SNAPSHOT_WRITE", e)),
            Op::SetAutoRun { .. } => {
                let a = self.sys.autorun;
                let key_existed = a.user_key_exists().map_err(|e| failure("E_SNAPSHOT_WRITE", e))?;
                let value =
                    a.read_user().map_err(|e| failure("E_SNAPSHOT_WRITE", e))?.as_ref().map(RegValueRecord::from_reg);
                to_json(&AutoRunRecord { key_existed, value }).map(Some)
            }
            Op::InstallFont { family } => {
                let (_, dest, value_name) = font_target(self.sys, self.resources, family)
                    .ok_or_else(|| failure("E_FONT_INSTALL", format!("no bundled file for {family}")))?;
                let registration =
                    self.sys.fonts.registration(&value_name).map_err(|e| failure("E_SNAPSHOT_WRITE", e))?;
                let key_existed =
                    self.sys.fonts.registration_key_exists().map_err(|e| failure("E_SNAPSHOT_WRITE", e))?;
                let dir_existed = fs.is_dir(&self.sys.fonts.user_fonts_dir());
                to_json(&FontPrior { file_existed: fs.exists(&dest), dir_existed, key_existed, registration }).map(Some)
            }
        }
    }

    fn apply(&mut self, op: &Op) -> Result<(), Failure> {
        let fs = self.sys.fs;
        match op {
            Op::InstallRuntime { component, dir } => self.install_runtime(*component, dir),
            Op::WriteFile { path, contents } => {
                fs.write_atomic(path, contents.as_bytes()).map_err(|e| failure("E_DATA_WRITE", e))
            }
            Op::WtApply { settings, scheme, profile, set_default } => {
                let bytes = fs
                    .read(settings)
                    .map_err(|e| failure("E_WT_WRITE", e))?
                    .ok_or_else(|| failure("E_WT_WRITE", format!("{} is gone", settings.display())))?;
                let text = String::from_utf8(bytes).map_err(|e| failure("E_WT_PARSE", e))?;
                let edited = apply_edit(&text, &WtEdit { scheme, profile, set_default: *set_default })
                    .map_err(|e| failure(e.code(), e))?;
                if edited.text == text {
                    return Ok(());
                }
                fs.write_atomic(settings, edited.text.as_bytes()).map_err(|e| failure("E_WT_WRITE", e))
            }
            Op::SetAutoRun { value } => {
                let a = self.sys.autorun;
                let result = match value {
                    Some(text) => {
                        // Keep REG_EXPAND_SZ when the value already has it (spec §5.4).
                        let current = a.read_user().map_err(|e| failure("E_AUTORUN_WRITE", e))?;
                        let kind =
                            if current.is_some_and(|v| v.kind == REG_EXPAND_SZ) { REG_EXPAND_SZ } else { REG_SZ };
                        a.write_user(&RegValue::from_text(kind, text))
                    }
                    None => a.delete_user(),
                };
                result.map_err(|e| failure("E_AUTORUN_WRITE", e))
            }
            Op::InstallFont { family } => self.install_font(family),
        }
    }

    fn restore(&mut self, op: &Op, prior: &Option<Vec<u8>>) -> Result<(), Failure> {
        let fs = self.sys.fs;
        match op {
            Op::InstallRuntime { component, dir } => {
                let p: RuntimePrior = from_json(prior, "E_RUNTIME_MISSING")?;
                if p.dir_existed {
                    // An undo that removed the folder and is rolled back puts it back, from
                    // the bundled files with their pinned hashes.
                    if !fs.exists(dir) {
                        self.install_runtime(*component, dir)?;
                    }
                    return Ok(());
                }
                if fs.exists(dir) {
                    // Rename first: it fails cleanly while a file inside is in use,
                    // so a half-deleted runtime is never left behind.
                    let tomb = sibling(dir, "removing");
                    fs.rename(dir, &tomb).map_err(|e| {
                        failure("E_RUNTIME_MISSING", format!("{e} (close every cmd window, then try again)"))
                    })?;
                    let _ = fs.remove_dir_all(&tomb);
                }
                if !p.parent_existed {
                    if let Some(parent) = dir.parent() {
                        let _ = fs.remove_dir(parent);
                    }
                }
                Ok(())
            }
            Op::WriteFile { path, .. } => match prior {
                Some(bytes) => fs.write_atomic(path, bytes),
                None => fs.remove_file(path),
            }
            .map_err(|e| failure("E_DATA_WRITE", e)),
            Op::WtApply { settings, .. } => match prior {
                Some(bytes) if fs.read(settings).ok().flatten().as_deref() != Some(bytes.as_slice()) => {
                    fs.write_atomic(settings, bytes).map_err(|e| failure("E_WT_WRITE", e))
                }
                // CMD Studio never creates or deletes a settings.json.
                _ => Ok(()),
            },
            Op::SetAutoRun { .. } => {
                let a = self.sys.autorun;
                let p: AutoRunRecord = from_json(prior, "E_AUTORUN_WRITE")?;
                match &p.value {
                    Some(rec) => {
                        a.write_user(&rec.to_reg().ok_or_else(|| failure("E_AUTORUN_WRITE", "bad saved value"))?)
                    }
                    None => a.delete_user(),
                }
                .map_err(|e| failure("E_AUTORUN_WRITE", e))?;
                if !p.key_existed {
                    a.delete_user_key_if_empty().map_err(|e| failure("E_AUTORUN_WRITE", e))?;
                }
                Ok(())
            }
            Op::InstallFont { family } => {
                let p: FontPrior = from_json(prior, "E_FONT_INSTALL")?;
                let (_, dest, value_name) = font_target(self.sys, self.resources, family)
                    .ok_or_else(|| failure("E_FONT_INSTALL", format!("no bundled file for {family}")))?;
                if !p.file_existed {
                    let _ = self.sys.fonts.unload(&dest);
                    // A running Windows Terminal may still hold the file; once the
                    // registration is gone the font is uninstalled either way, and
                    // a later cleanup deletes the file.
                    let _ = fs.remove_file(&dest);
                }
                if !p.dir_existed {
                    // Only when empty: another per-user font may live there by now.
                    let _ = fs.remove_dir(&self.sys.fonts.user_fonts_dir());
                }
                match &p.registration {
                    Some(file) => self.sys.fonts.register(&value_name, file),
                    None => self.sys.fonts.unregister(&value_name),
                }
                .map_err(|e| failure("E_FONT_INSTALL", e))?;
                if !p.key_existed {
                    self.sys.fonts.delete_registration_key_if_empty().map_err(|e| failure("E_FONT_INSTALL", e))?;
                }
                Ok(())
            }
        }
    }
}

/// Undo: "applying" an op of the reversed journal puts its target back to
/// the state that journal captured.
pub struct UndoExecutor<'a> {
    inner: SystemExecutor<'a>,
    priors: Vec<(Op, Option<Vec<u8>>)>,
}

impl<'a> UndoExecutor<'a> {
    pub fn new(inner: SystemExecutor<'a>, priors: Vec<(Op, Option<Vec<u8>>)>) -> Self {
        UndoExecutor { inner, priors }
    }
}

impl Executor for UndoExecutor<'_> {
    fn capture(&mut self, op: &Op) -> Result<Option<Vec<u8>>, Failure> {
        self.inner.capture(op)
    }

    fn apply(&mut self, op: &Op) -> Result<(), Failure> {
        let prior = self
            .priors
            .iter()
            .find(|(o, _)| o == op)
            .map(|(_, p)| p.clone())
            .ok_or_else(|| failure("E_SNAPSHOT_WRITE", "the restore point does not cover this step"))?;
        self.inner.restore(op, &prior)
    }

    fn restore(&mut self, op: &Op, prior: &Option<Vec<u8>>) -> Result<(), Failure> {
        self.inner.restore(op, prior)
    }
}

/// A short English name for an op's target, for messages.
pub fn op_label(op: &Op) -> String {
    match op {
        Op::InstallRuntime { dir, .. } => format!("runtime folder {}", dir.display()),
        Op::WriteFile { path, .. } => format!("file {}", path.display()),
        Op::WtApply { settings, .. } => format!("Windows Terminal settings {}", settings.display()),
        Op::SetAutoRun { .. } => "cmd AutoRun".to_string(),
        Op::InstallFont { family } => format!("font {family}"),
    }
}

/// The registry key that holds AutoRun, as Registry Editor shows it.
const COMMAND_PROCESSOR_KEY: &str = r"HKEY_CURRENT_USER\Software\Microsoft\Command Processor";

/// How to put `op`'s target back by hand when a rollback could not (spec §5.5: say which
/// parts, and how to fix them by hand). `prior` is what the transaction captured before the op,
/// and `payload` the restore point's file that holds it.
pub fn fix_by_hand(op: &Op, prior: &Option<Vec<u8>>, payload: &Path) -> String {
    let cleanup = "run `cmdstudio --cleanup`, which takes out what CMD Studio changed";
    match op {
        Op::InstallRuntime { dir, .. } => format!(
            "Close every cmd window and Windows Terminal, then {cleanup}. If {} is still there \
             afterwards, delete it.",
            dir.display()
        ),
        Op::WriteFile { path, .. } => match prior {
            Some(_) => format!("Copy {} over {}.", payload.display(), path.display()),
            None => format!("Delete {}.", path.display()),
        },
        Op::WtApply { settings, .. } => match prior {
            Some(_) => format!("Close Windows Terminal, then copy {} over {}.", payload.display(), settings.display()),
            None => format!("Close Windows Terminal, then delete {}.", settings.display()),
        },
        Op::SetAutoRun { .. } => match from_json::<AutoRunRecord>(prior, "") {
            Ok(AutoRunRecord { value: None, .. }) => {
                format!("In Registry Editor, open {COMMAND_PROCESSOR_KEY} and delete the AutoRun value.")
            }
            Ok(AutoRunRecord { value: Some(record), .. }) => match record.to_reg().and_then(|v| v.text()) {
                Some(text) => format!(
                    "In Registry Editor, open {COMMAND_PROCESSOR_KEY} and set AutoRun back to: \
                     {text}"
                ),
                None => format!("Close every cmd window, then {cleanup}."),
            },
            Err(_) => format!("Close every cmd window, then {cleanup}."),
        },
        Op::InstallFont { family } => format!(
            "Close Windows Terminal, then {cleanup}: it uninstalls the font {family} once nothing \
             uses it."
        ),
    }
}

/// Whether two captured states of `op`'s target mean the same thing. Files
/// and settings compare byte for byte; for a font only the registration
/// counts (a file Windows Terminal still holds is deleted later); for a
/// runtime only whether the folder exists.
pub fn same_state(op: &Op, a: &Option<Vec<u8>>, b: &Option<Vec<u8>>) -> bool {
    match op {
        Op::WriteFile { .. } | Op::WtApply { .. } => a == b,
        Op::SetAutoRun { .. } => from_json::<AutoRunRecord>(a, "").ok() == from_json::<AutoRunRecord>(b, "").ok(),
        Op::InstallFont { .. } => {
            from_json::<FontPrior>(a, "").ok().map(|p| p.registration)
                == from_json::<FontPrior>(b, "").ok().map(|p| p.registration)
        }
        Op::InstallRuntime { .. } => {
            from_json::<RuntimePrior>(a, "").ok().map(|p| p.dir_existed)
                == from_json::<RuntimePrior>(b, "").ok().map(|p| p.dir_existed)
        }
    }
}

/// Checks that undo put every target back to what the undone apply captured
/// (the spec §8 Phase 1 criterion: byte-identical after undo).
pub struct UndoVerifier<'a> {
    exec: SystemExecutor<'a>,
    expected: Vec<(Op, Option<Vec<u8>>)>,
}

impl<'a> UndoVerifier<'a> {
    pub fn new(exec: SystemExecutor<'a>, expected: Vec<(Op, Option<Vec<u8>>)>) -> Self {
        UndoVerifier { exec, expected }
    }
}

impl crate::txn::Verifier for UndoVerifier<'_> {
    fn verify(&mut self, _ops: &[Op]) -> Result<(), Failure> {
        for (op, want) in &self.expected {
            let now = self.exec.capture(op)?;
            if !same_state(op, &now, want) {
                return Err(failure("E_VERIFY_READBACK", format!("undo did not restore {}", op_label(op))));
            }
        }
        Ok(())
    }
}

/// Deletes runtime folders that earlier runs left half made or half removed
/// (`.<name>.partial-…`, `.<name>.removing-…`). Best effort.
pub fn sweep(sys: System<'_>, paths: &Paths) {
    let mut roots = vec![paths.runtime_root()];
    if let (Ok(pd), Ok(sid)) = (sys.host.program_data(), sys.host.user_sid()) {
        roots.push(program_data_dir(&pd, &sid));
    }
    for root in roots {
        for e in sys.fs.list_dir(&root).unwrap_or_default() {
            let name = e.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if e.is_dir && name.starts_with('.') && (name.contains(".partial-") || name.contains(".removing-")) {
                let _ = sys.fs.remove_dir_all(&e.path);
            }
        }
    }
}
```
`paths.omp_dir()` and `paths.clink_dir()` are Phase 1a's versioned runtime folders under `paths.runtime_root()` (`<data>\runtime\omp-31.5.0`, `<data>\runtime\clink-1.9.34`), so `sweep`'s first root is their parent.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- exec::tests`
Expected: `11 passed`.

- [ ] **Step 5: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 6: Commit**

Message:
```text
feat(core): add the real txn executor and the undo executor

SystemExecutor runs each plan op through the adapters and can capture,
apply and restore it any number of times. Runtimes are staged and renamed
in, and renamed out before deletion so an in-use folder is never half
deleted. AutoRun keeps its type and exact bytes; a font ends unregistered
even when its file is held. UndoExecutor and UndoVerifier put every
target back to what the undone apply captured; a runtime an undo
removed comes back if that undo is rolled back.

fix_by_hand says, for each kind of op, how a person puts its target
back when a rollback could not (spec 5.5).
```
Run:
```bash
git add -- crates/core/src/lib.rs crates/core/src/exec.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 12: The real verifier and `WinProcess`

**Files:**
- Create: `crates/core/src/verify.rs`, `crates/core/src/adapters/windows/process.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod verify;`), `crates/core/src/adapters/windows/mod.rs` (`pub mod process;`, between `host` and `registry`)

**Interfaces:**
- Consumes:
  - Task 1: `adapters::{System, Process, Output { status: i32, stdout: String, stderr: String }, AdapterError, AutoRun, RegValue, REG_SZ}`; `FakeProcess::answer(exe_name, first_arg, status, stdout)` (unanswered calls get status 0 and empty output) and `calls() -> Vec<Call { exe: PathBuf, args: Vec<String> }>`.
  - Task 2: `resources::{check_bundle, Resources, fake_resources}`. Task 7: `wt_edit::{check_parses, read_value, WtKey}`. Task 11: `exec::{font_target, SystemExecutor}`.
  - Phase 1a: `generate::wt::CMD_PROFILE_GUID`, `generate::lua::Lang`, `plan::{plan, ClinkLaunch, Component, Machine, Op, Paths, Request}`, `txn::{run, Executor, Failure, Journal, JournalStore, Outcome, Verifier}`, `model::Target`, and the test-only `presets::terracotta() -> Theme` (ruling P19; `#[cfg(test)] pub(crate)`, so only unit tests inside the core call it).
- Produces:
  - `cmdstudio_core::verify::{OMP_CONFIG_ERRORS, lua_error_line, omp_config_error, SystemVerifier}`: `lua_error_line(&str) -> Option<&str>`, `omp_config_error(&str) -> Option<&'static str>`, `SystemVerifier::new(sys: System<'a>, paths: &'a Paths, resources: &'a Resources, launch: Option<ClinkLaunch>) -> SystemVerifier<'a>`, `impl txn::Verifier`.
  - `cmdstudio_core::adapters::windows::process::WinProcess::new(allowed: Vec<PathBuf>, timeout: Duration)`, `impl Process`.

**The four checks** (spec §5.5, G4/G5): (1) every op's target is read back: files byte for byte, the CMD Studio scheme and each written cmd-profile key (an object value such as `font` must contain every sub-key written; the user's other sub-keys such as `font.weight` may stay), `defaultProfile` when it was set, AutoRun's text, the font registration and file, and each runtime folder's pinned hashes → `E_VERIFY_READBACK`; (2) when `theme.omp.json` was written: `oh-my-posh print primary --config <generated> --shell cmd --plain` must exit 0, print something, and print none of Oh My Posh 31.5.0's config-error markers (it prints them and still exits 0; the list is from `src/config/load.go` at tag v31.5.0) → `E_VERIFY_OMP`; (3) when a Clink script or `clink_settings` was written: `<clink-launch> --profile <p> set clink.logo` must print no line with `<name>.lua:<digits>:` (Clink always exits 0, G5) → `E_VERIFY_CLINK`; (4) each written settings.json still parses → `E_WT_PARSE`. `launch` is `None` only when nothing Clink loads was written (revert all, Task 14). `WinProcess` runs only executables under its allowed folders (canonical paths, so `..` or a junction cannot escape), with fixed arguments, no shell, `CREATE_NO_WINDOW` (no console flashes, §2.2 lesson 7), stdin closed, and kills the program after the time limit.

- [ ] **Step 1: Write the failing tests** — add `pub mod verify;` to `crates/core/src/lib.rs` and create `crates/core/src/verify.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeMachine;
    use crate::adapters::{AutoRun, RegValue, REG_SZ};
    use crate::exec::SystemExecutor;
    use crate::model::Target;
    use crate::plan::{plan, Component, Machine, Request};
    use crate::presets::terracotta;
    use crate::resources::fake_resources;
    use crate::txn::{run, Executor, Journal, JournalStore, Outcome};
    use std::path::{Path, PathBuf};

    #[test]
    fn lua_errors_are_found_by_their_shape() {
        assert!(lua_error_line(r"C:\x\bad.lua:1: deliberate error").is_some());
        assert!(lua_error_line("ok\nC:\\ผู้ใช้\\generated\\banner.lua:12: attempt to call nil\n").is_some());
        assert!(lua_error_line("Setting 'clink.logo' is 'none'.").is_none());
        assert!(lua_error_line("C:\\x\\a.lua: no line number").is_none());
        assert!(lua_error_line("C:\\x\\a.lua:12 no colon").is_none());
    }

    #[test]
    fn omp_config_errors_are_recognised() {
        assert_eq!(omp_config_error(" CONFIG PARSE ERROR  ~ "), Some("CONFIG PARSE ERROR"));
        assert_eq!(omp_config_error("╭─ ~ ─╮"), None);
    }

    #[derive(Default)]
    struct Mem(Vec<Journal>);
    impl JournalStore for Mem {
        fn save(&mut self, j: &Journal) -> Result<(), Failure> {
            self.0.push(j.clone());
            Ok(())
        }
        fn load_unfinished(&mut self) -> Result<Option<Journal>, Failure> {
            Ok(None)
        }
    }

    fn applied(m: &FakeMachine) -> (Resources, Paths, Vec<Op>, Option<ClinkLaunch>) {
        let res = fake_resources(&m.fs, Path::new(r"C:\app\resources"));
        let paths = Paths { data: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio") };
        let machine = Machine { data_short: Some(paths.data.clone()), ..Machine::default() };
        let req = Request {
            target: Target::All,
            set_wt_default_profile: false,
            lang: crate::generate::lua::Lang::En,
            replace_foreign_clink: false,
        };
        let p = plan(&terracotta(), &req, &machine, &paths).unwrap();
        (res, paths, p.ops, Some(p.launch))
    }

    #[test]
    fn a_good_apply_verifies_and_commits() {
        let m = FakeMachine::new();
        m.process.answer("oh-my-posh.exe", "print", 0, "╭─ ~ ─╮\n│ >");
        let (res, paths, ops, launch) = applied(&m);
        let out = run(
            "t",
            ops,
            &mut SystemExecutor::new(m.system(), &res),
            &mut SystemVerifier::new(m.system(), &paths, &res, launch),
            &mut Mem::default(),
        );
        assert_eq!(out, Outcome::Committed);
        let calls = m.process.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0].args,
            vec![
                "print",
                "primary",
                "--config",
                &paths.omp_config().display().to_string(),
                "--shell",
                "cmd",
                "--plain"
            ]
        );
        assert_eq!(calls[1].exe, paths.clink_exe());
    }

    #[test]
    fn omp_config_error_rolls_back_with_e_verify_omp() {
        let m = FakeMachine::new();
        m.process.answer("oh-my-posh.exe", "print", 0, " CONFIG PARSE ERROR ");
        let (res, paths, ops, launch) = applied(&m);
        let files_before = m.fs.snapshot().0;
        let out = run(
            "t",
            ops,
            &mut SystemExecutor::new(m.system(), &res),
            &mut SystemVerifier::new(m.system(), &paths, &res, launch),
            &mut Mem::default(),
        );
        assert!(matches!(out, Outcome::RolledBack { code: "E_VERIFY_OMP", .. }), "{out:?}");
        assert_eq!(m.fs.snapshot().0, files_before);
        assert_eq!(m.autorun.read_user().unwrap(), None);
    }

    #[test]
    fn a_lua_error_rolls_back_with_e_verify_clink() {
        let m = FakeMachine::new();
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        m.process.answer("clink_x64.exe", "--profile", 0, "C:\\d\\generated\\banner.lua:3: boom\n");
        let (res, paths, ops, launch) = applied(&m);
        let out = run(
            "t",
            ops,
            &mut SystemExecutor::new(m.system(), &res),
            &mut SystemVerifier::new(m.system(), &paths, &res, launch),
            &mut Mem::default(),
        );
        assert!(matches!(out, Outcome::RolledBack { code: "E_VERIFY_CLINK", .. }), "{out:?}");
    }

    #[test]
    fn readback_catches_a_value_changed_behind_our_back() {
        let m = FakeMachine::new();
        let (res, paths, _, launch) = applied(&m);
        let op = Op::SetAutoRun { value: Some("ours".into()) };
        SystemExecutor::new(m.system(), &res).apply(&op).unwrap();
        m.autorun.write_user(&RegValue::from_text(REG_SZ, "someone else")).unwrap();
        let err = SystemVerifier::new(m.system(), &paths, &res, launch.clone()).verify(&[op]).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_READBACK");
        let rt = Op::InstallRuntime { component: Component::Omp, dir: paths.omp_dir() };
        m.fs.put(paths.omp_exe(), b"not the pinned file");
        assert_eq!(
            SystemVerifier::new(m.system(), &paths, &res, launch).verify(&[rt]).unwrap_err().code,
            "E_VERIFY_READBACK"
        );
    }
}
```
Then add `pub mod process;` to `crates/core/src/adapters/windows/mod.rs` and create `crates/core/src/adapters/windows/process.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn system32() -> PathBuf {
        PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32")
    }

    #[test]
    fn runs_an_allowed_program_and_captures_output() {
        let p = WinProcess::new(vec![system32()], Duration::from_secs(30));
        let out = p
            .run(&system32().join("cmd.exe"), &[OsStr::new("/d"), OsStr::new("/c"), OsStr::new("echo cmdstudio")])
            .unwrap();
        assert_eq!(out.status, 0);
        assert_eq!(out.stdout.trim(), "cmdstudio");
    }

    #[test]
    fn refuses_programs_outside_the_allowed_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let p = WinProcess::new(vec![tmp.path().to_path_buf()], Duration::from_secs(30));
        assert!(p.run(&system32().join("cmd.exe"), &[]).is_err());
    }

    #[test]
    fn a_hung_program_is_stopped() {
        let p = WinProcess::new(vec![system32()], Duration::from_secs(1));
        let started = Instant::now();
        let err = p
            .run(&system32().join("PING.EXE"), &[OsStr::new("-n"), OsStr::new("30"), OsStr::new("127.0.0.1")])
            .unwrap_err();
        assert!(err.message.contains("no answer"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
```
These three tests only run `cmd /c echo` and `ping` on the loopback address; they change nothing.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- verify::tests adapters::windows::process::tests`
Expected: compile errors (`cannot find struct SystemVerifier`, `cannot find struct WinProcess`, …).

- [ ] **Step 3: Implement the verifier above its tests**
```rust
//! The real `txn::Verifier` (spec §5.5): after every op ran, (1) read every
//! written value back, (2) Oh My Posh renders the generated config, (3) Clink
//! loads the scripts without a Lua error, (4) every written settings.json
//! parses. Runs only CMD Studio's own bundled programs (spec §5.10).
use crate::adapters::System;
use crate::exec::font_target;
use crate::generate::wt::CMD_PROFILE_GUID;
use crate::plan::{ClinkLaunch, Op, Paths};
use crate::resources::{check_bundle, Resources};
use crate::txn::{Failure, Verifier};
use crate::wt_edit::{check_parses, read_value, WtKey};
use serde_json::Value;
use std::ffi::OsStr;

/// What Oh My Posh 31.5.0 prints instead of the prompt when it cannot use the
/// config; it still exits 0 (`src/config/load.go` at v31.5.0).
pub const OMP_CONFIG_ERRORS: [&str; 6] = [
    "CONFIG NOT FOUND",
    "INVALID CONFIG EXTENSION",
    "INVALID CONFIG THEME",
    "CONFIG URL FETCH FAILED",
    "CONFIG PARSE ERROR",
    "NO CONFIG",
];

/// The first line that reports a Lua error: `<script>.lua:<line>:` (G5).
pub fn lua_error_line(output: &str) -> Option<&str> {
    output.lines().find(|line| {
        line.match_indices(".lua:").any(|(i, m)| {
            let rest = line.get(i + m.len()..).unwrap_or("");
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            digits > 0 && rest.chars().nth(digits) == Some(':')
        })
    })
}

/// The Oh My Posh config error in `stdout`, if any.
pub fn omp_config_error(stdout: &str) -> Option<&'static str> {
    OMP_CONFIG_ERRORS.into_iter().find(|marker| stdout.contains(marker))
}

fn fail(code: &'static str, message: impl std::fmt::Display) -> Failure {
    Failure { code, message: message.to_string() }
}

pub struct SystemVerifier<'a> {
    sys: System<'a>,
    paths: &'a Paths,
    resources: &'a Resources,
    launch: Option<ClinkLaunch>,
}

impl<'a> SystemVerifier<'a> {
    /// `launch` is `<clink-launch>`; `None` when no Clink script is written (revert all).
    pub fn new(sys: System<'a>, paths: &'a Paths, resources: &'a Resources, launch: Option<ClinkLaunch>) -> Self {
        SystemVerifier { sys, paths, resources, launch }
    }

    fn read_back(&self, op: &Op) -> Result<(), Failure> {
        let fs = self.sys.fs;
        let mismatch = |what: String| fail("E_VERIFY_READBACK", format!("{what} does not hold what was written"));
        match op {
            Op::WriteFile { path, contents } => {
                let got = fs.read(path).map_err(|e| fail("E_VERIFY_READBACK", e))?;
                if got.as_deref() != Some(contents.as_bytes()) {
                    return Err(mismatch(path.display().to_string()));
                }
            }
            Op::WtApply { settings, scheme, profile, set_default } => {
                let bytes = fs.read(settings).map_err(|e| fail("E_VERIFY_READBACK", e))?.unwrap_or_default();
                let text = String::from_utf8(bytes).map_err(|e| fail("E_WT_PARSE", e))?;
                // Check 4: Windows Terminal can still read the file.
                check_parses(&text).map_err(|e| fail(e.code(), e))?;
                let read = |key: WtKey| read_value(&text, &key).map_err(|e| fail(e.code(), e));
                if read(WtKey::Scheme)?.as_ref() != Some(scheme) {
                    return Err(mismatch(format!("the CMD Studio scheme in {}", settings.display())));
                }
                for (key, value) in profile {
                    let got = read(WtKey::Profile { key: key.clone() })?;
                    let ok = match (value, &got) {
                        (Value::Object(want), Some(Value::Object(have))) => {
                            want.iter().all(|(k, v)| have.get(k) == Some(v))
                        }
                        _ => got.as_ref() == Some(value),
                    };
                    if !ok {
                        return Err(mismatch(format!("cmd profile key {key} in {}", settings.display())));
                    }
                }
                if *set_default {
                    let ok = read(WtKey::DefaultProfile)?
                        .as_ref()
                        .and_then(Value::as_str)
                        .is_some_and(|g| g.eq_ignore_ascii_case(CMD_PROFILE_GUID));
                    if !ok {
                        return Err(mismatch(format!("defaultProfile in {}", settings.display())));
                    }
                }
            }
            Op::SetAutoRun { value } => {
                let got =
                    self.sys.autorun.read_user().map_err(|e| fail("E_VERIFY_READBACK", e))?.and_then(|v| v.text());
                if got != *value {
                    return Err(mismatch("AutoRun".to_string()));
                }
            }
            Op::InstallFont { family } => {
                let (_, dest, value_name) =
                    font_target(self.sys, self.resources, family).ok_or_else(|| fail("E_FONT_INSTALL", family))?;
                let reg = self.sys.fonts.registration(&value_name).map_err(|e| fail("E_VERIFY_READBACK", e))?;
                if reg.as_deref() != dest.to_str() || !fs.exists(&dest) {
                    return Err(mismatch(format!("font {family}")));
                }
            }
            Op::InstallRuntime { component, dir } => {
                let m = self.resources.manifest(fs).map_err(|e| fail(e.code(), e))?;
                check_bundle(fs, dir, m.bundle(*component)).map_err(|e| fail("E_VERIFY_READBACK", e))?;
            }
        }
        Ok(())
    }

    /// Check 2: `oh-my-posh print primary --config <generated> --shell cmd --plain`.
    fn omp_renders(&self) -> Result<(), Failure> {
        let config = self.paths.omp_config();
        let args = [
            OsStr::new("print"),
            OsStr::new("primary"),
            OsStr::new("--config"),
            config.as_os_str(),
            OsStr::new("--shell"),
            OsStr::new("cmd"),
            OsStr::new("--plain"),
        ];
        let out = self.sys.process.run(&self.paths.omp_exe(), &args).map_err(|e| fail("E_VERIFY_OMP", e))?;
        if out.status != 0 {
            return Err(fail("E_VERIFY_OMP", format!("oh-my-posh exited {}: {}", out.status, out.stderr.trim())));
        }
        if let Some(marker) = omp_config_error(&out.stdout) {
            return Err(fail("E_VERIFY_OMP", format!("oh-my-posh says {marker}")));
        }
        if out.stdout.trim().is_empty() {
            return Err(fail("E_VERIFY_OMP", "oh-my-posh printed nothing"));
        }
        Ok(())
    }

    /// Check 3: `<clink-launch> --profile <p> set clink.logo`; only its output
    /// shows a Lua error, because it always exits 0 (G5).
    fn clink_loads(&self) -> Result<(), Failure> {
        let profile = self.paths.clink_profile();
        let args = [OsStr::new("--profile"), profile.as_os_str(), OsStr::new("set"), OsStr::new("clink.logo")];
        let launch = self.launch.as_ref().ok_or_else(|| fail("E_VERIFY_CLINK", "no <clink-launch> was given"))?;
        let out = self.sys.process.run(&launch.exe(), &args).map_err(|e| fail("E_VERIFY_CLINK", e))?;
        let all = format!("{}\n{}", out.stdout, out.stderr);
        match lua_error_line(&all) {
            Some(line) => Err(fail("E_VERIFY_CLINK", line.trim())),
            None => Ok(()),
        }
    }
}

impl Verifier for SystemVerifier<'_> {
    fn verify(&mut self, ops: &[Op]) -> Result<(), Failure> {
        for op in ops {
            self.read_back(op)?;
        }
        let writes =
            |path: &std::path::Path| ops.iter().any(|o| matches!(o, Op::WriteFile { path: p, .. } if p == path));
        if writes(&self.paths.omp_config()) {
            self.omp_renders()?;
        }
        let scripts = [
            self.paths.clink_profile().join("clink_settings"),
            self.paths.generated().join("init.lua"),
            self.paths.generated().join("banner.lua"),
        ];
        if scripts.iter().any(|p| writes(p)) {
            self.clink_loads()?;
        }
        Ok(())
    }
}
```
The three Clink file names are the ones Phase 1a's planner writes (`clink-profile\clink_settings`, `generated\init.lua`, `generated\banner.lua`), and it writes no other Lua file.

- [ ] **Step 4: Implement `WinProcess` above its tests**
```rust
//! Runs CMD Studio's own bundled programs (spec §5.10): only executables
//! under the given folders, fixed arguments, no shell, no console window, a
//! time limit.
use crate::adapters::{AdapterError, Output, Process};
use std::ffi::OsStr;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// `CREATE_NO_WINDOW`: no console flashes on the user's screen (spec §2.2 lesson 7).
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug)]
pub struct WinProcess {
    allowed: Vec<PathBuf>,
    timeout: Duration,
}

impl WinProcess {
    /// `allowed`: folders whose programs may run (the runtime folders).
    pub fn new(allowed: Vec<PathBuf>, timeout: Duration) -> Self {
        WinProcess { allowed, timeout }
    }

    fn permitted(&self, exe: &Path) -> bool {
        let Ok(exe) = std::fs::canonicalize(exe) else {
            return false;
        };
        self.allowed.iter().filter_map(|root| std::fs::canonicalize(root).ok()).any(|root| exe.starts_with(root))
    }
}

fn drain(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.read_to_end(&mut buf);
        buf
    })
}

impl Process for WinProcess {
    fn run(&self, exe: &Path, args: &[&OsStr]) -> Result<Output, AdapterError> {
        let context = || format!("run {}", exe.display());
        if !self.permitted(exe) {
            return Err(AdapterError::new(context(), "not one of CMD Studio's own programs"));
        }
        let mut child = Command::new(exe)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| AdapterError::new(context(), e.to_string()))?;
        let out = child.stdout.take().map(drain);
        let err = child.stderr.take().map(drain);
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() > self.timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(AdapterError::new(context(), format!("no answer after {} s", self.timeout.as_secs())));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => return Err(AdapterError::new(context(), e.to_string())),
            }
        };
        let text = |h: Option<std::thread::JoinHandle<Vec<u8>>>| {
            h.and_then(|h| h.join().ok()).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
        };
        Ok(Output { status: status.code().unwrap_or(-1), stdout: text(out), stderr: text(err) })
    }
}
```
Output is decoded with `from_utf8_lossy`: the verifier looks only for ASCII markers (`.lua:<digits>:` and Oh My Posh's error words), so a path printed in another code page cannot hide an error or fail the decode.

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- verify::tests adapters::windows::process::tests`
Expected: `9 passed` (6 verify, 3 process).

- [ ] **Step 6: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 7: Commit**

Message:
```text
feat(core): verify applies with the real runtimes

SystemVerifier reads every written value back, runs Oh My Posh on the
generated config (its config errors print a marker and exit 0, so the
output is checked), runs Clink's set command and looks for
"<script>.lua:<line>:" (it always exits 0), and checks every written
settings.json still parses. WinProcess runs only programs under the
runtime folders, with no shell, no console window and a time limit.
```
Run:
```bash
git add -- crates/core/src/lib.rs crates/core/src/verify.rs crates/core/src/adapters/windows/mod.rs crates/core/src/adapters/windows/process.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 13: Apply, undo and crash recovery

**Files:**
- Create: `crates/core/src/actions.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod actions;`)

**Interfaces:**
- Consumes:
  - Task 1: `adapters::{Arch, System, WtLocation, WtVariant, AutoRun, Fonts, Fs, RegValue, REG_EXPAND_SZ}`; `fake::{FakeMachine, FakeFonts::new(dir, families)}`; `FakeHost.info` (a `SystemInfo`).
  - Task 2: `resources::{sha256_hex, ResourceError, Resources, fake_resources, NERD_FONT_FILE}`. Task 7: `wt_edit::{apply_edit, WtEdit, WtError}` and `tests/fixtures/wt/default.json`. Task 8: `state::{Applied, AutoRunRecord, FontRecord, RegValueRecord, State, StateError}`, `Paths::{state_file, snapshots}`. Task 9: `snapshot::{new_id, Incomplete, Manifest, SnapshotKind, SnapshotStore, ThemeRef, Unrestored, KEEP}`, `SnapshotStore::{payload_path, write_incomplete, latest_incomplete}`. Task 10: `detect::{detect, DetectError, Detected}`. Task 11: `exec::{fix_by_hand, font_target, op_label, sweep, SystemExecutor, UndoExecutor, UndoVerifier}`. Task 12: `verify::SystemVerifier`.
  - Phase 1a: `generate::lua::Lang`, `model::{Theme, Target}`, `plan::{plan, Op, Paths, Plan, PlanError, Request, CLINK_VERSION, OMP_VERSION, NERD_FONT_FAMILY}`, `txn::{self, Executor, Failure, Journal, JournalStore, Outcome, Snapshot, Stage}`, the test-only `presets::terracotta()`.
- Produces (in `cmdstudio_core::actions`):
  - `Ctx { paths: Paths, resources: Resources, now_ms: u64 }` (`Clone + Debug`).
  - `ActionError::{Unsupported, Resources(ResourceError), Detect(DetectError), State(StateError), Plan(PlanError), Wt { path: PathBuf, error: WtError }, Snapshot(String)}` with `code() -> &'static str`; `impl From<Failure> for ActionError`.
  - `Prepared { plan: Plan, detected: Detected, ops: Vec<Op>, state_after: State, manifest: snapshot::Manifest }`.
  - `prepare_apply(System<'_>, &Ctx, &Theme, &Request) -> Result<Prepared, ActionError>` — changes nothing except creating the data folder (Task 10).
  - `run_apply(System<'_>, &Ctx, Prepared) -> Outcome`.
  - `UndoResult::{NothingToUndo, Done { undid: String, outcome: Outcome }}`; `undo(System<'_>, &Ctx) -> Result<UndoResult, ActionError>`.
  - `StartReport { recovered: Option<Outcome>, incomplete: Option<Incomplete> }` (`Clone + Debug + PartialEq + Eq`); `recover_at_start(System<'_>, &Ctx) -> StartReport`.
  - `pending_incomplete(System<'_>, &Ctx) -> Option<Incomplete>`: the newest restore point's `incomplete.json`, if nothing has run since its rollback.
  - `pub(crate) record_incomplete(&SnapshotStore<'_>, id: &str, &Outcome)`: writes `incomplete.json` when the outcome is `RollbackIncomplete`; Task 14's revert-all calls it too.
  - `#[cfg(test)] pub(crate) mod testing { WT: &str, SETTINGS: &str, machine() -> (FakeMachine, Ctx), request(Target) -> Request }` — reused by Tasks 14 and 15.

**How the pieces fit** (spec §5.5, §6.3, brief):
- `prepare_apply` checks the bundle manifest (`E_RUNTIME_MISSING`), runs `detect`, refuses ARM64 (`E_UNSUPPORTED_PLATFORM`: the bundle is x64 only, spec §5.4), plans, and dry-runs every `WtApply` through `apply_edit`, so a settings.json that does not parse stops everything before the first change (`E_WT_PARSE`). It works out the new `state.json` from what is on the machine *now*: the baseline notes (first-touch originals, last writes), the generated files' hashes, `<clink-launch>`, the WT font face. The new `state.json` is the **last op** of the same transaction (`WriteFile`), so it commits and rolls back with everything else and never claims an apply that did not happen.
- `run_apply` runs the transaction with its own restore point, then prunes old restore points (failures to prune are ignored: they cost disk space only).
- `undo` takes the newest committed apply that nothing has reversed yet, and runs a new transaction over the same ops in reverse order: `UndoExecutor` "applies" an op by restoring the prior that apply captured; `UndoVerifier` then checks every target equals that prior (byte-identical, spec §8). The undo has its own restore point, so a failed undo rolls back to the themed state. On commit the apply's manifest gets `undoneBy`. Undoing again reverses the apply before it.
- `recover_at_start` finishes a transaction a crash cut off (`E_TXN_INTERRUPTED`, Phase 1a) and sweeps half-made runtime folders. The CLI runs it before every command (Task 16). Its `StartReport` also carries the newest `incomplete.json`, so a rollback that could not finish is still reported after a restart.
- A rollback that cannot finish (spec §5.5): `run_apply`, `undo`, the revert-all of Task 14 and `recover_at_start` each call `record_incomplete` with their outcome. For each op in `failed` it writes the target (`op_label`), the restore's code and message, and `fix_by_hand` with the payload file that holds the op's prior state (`SnapshotStore::payload_path`). When the journal cannot be read, the target is `step <n>` and the fix is the generic "close every cmd window and Windows Terminal, then run `cmdstudio --cleanup`". Writing the record is best effort: the outcome itself is already `RollbackIncomplete` and is never reported as a success.

- [ ] **Step 1: Write the failing tests** — add `pub mod actions;` to `crates/core/src/lib.rs` and create `crates/core/src/actions.rs` with only:
```rust
#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::adapters::fake::FakeMachine;
    use crate::adapters::{Fs, WtLocation, WtVariant};
    use crate::model::Target;
    use crate::plan::NERD_FONT_FAMILY;
    use crate::resources::fake_resources;
    use crate::resources::NERD_FONT_FILE;
    use std::path::Path;

    pub(crate) const WT: &str =
        r"C:\Users\tester\AppData\Local\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json";
    pub(crate) const SETTINGS: &str = include_str!("../tests/fixtures/wt/default.json");

    /// A machine with Windows Terminal, an AutoRun from another program, and
    /// OMP/Clink answering like the real ones.
    pub(crate) fn machine() -> (FakeMachine, Ctx) {
        let mut m = FakeMachine::new();
        m.fs.create_dir_all(Path::new(r"C:\Users\tester\AppData\Local\Microsoft\Windows")).unwrap_or(());
        m.fs.put(WT, SETTINGS);
        m.wt.locations = vec![WtLocation { variant: WtVariant::Store, installed: true, settings: PathBuf::from(WT) }];
        m.fonts =
            crate::adapters::fake::FakeFonts::new(r"C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts", &["Arial"]);
        m.fonts.knows_file(NERD_FONT_FILE, NERD_FONT_FAMILY);
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        let resources = fake_resources(&m.fs, Path::new(r"C:\Program Files\CMD Studio\resources"));
        let ctx = Ctx {
            paths: Paths { data: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio") },
            resources,
            now_ms: 1_791_694_709_000,
        };
        (m, ctx)
    }

    pub(crate) fn request(target: Target) -> Request {
        Request { target, set_wt_default_profile: true, lang: Lang::En, replace_foreign_clink: false }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{machine, request, WT};
    use super::*;
    use crate::adapters::{AutoRun, Fonts, RegValue, REG_EXPAND_SZ};
    use crate::model::Target;
    use crate::plan::NERD_FONT_FAMILY;
    use crate::presets::terracotta;
    use crate::txn::{Journal, JournalStore, Stage};

    fn everything(m: &crate::adapters::fake::FakeMachine) -> impl PartialEq + std::fmt::Debug {
        (m.fs.file(WT), m.autorun.snapshot(), m.fonts.snapshot(), m.fonts.families().unwrap())
    }

    #[test]
    fn apply_then_undo_is_byte_identical() {
        // The spec §8 Phase 1 criterion, on the fake machine.
        let (m, ctx) = machine();
        m.autorun.write_user(&RegValue::from_text(REG_EXPAND_SZ, r#""%USERPROFILE%\conda_hook.bat""#)).unwrap();
        let before = everything(&m);
        let prepared = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert!(prepared.ops.iter().any(|o| matches!(o, Op::InstallFont { .. })));
        assert_eq!(run_apply(m.system(), &ctx, prepared), Outcome::Committed);
        assert_ne!(everything(&m), before);
        // Ours goes first, in parentheses, and the other program's command follows unchanged
        // (spec §5.4); the value keeps its REG_EXPAND_SZ type.
        let text = m.autorun.read_user().unwrap().unwrap();
        assert_eq!(text.kind, REG_EXPAND_SZ);
        let value = text.text().unwrap();
        assert!(value.starts_with("(if exist \""), "{value}");
        assert!(value.ends_with(r#") & "%USERPROFILE%\conda_hook.bat""#), "{value}");
        assert!(m.fonts.families().unwrap().contains(&NERD_FONT_FAMILY.to_string()));
        let state = State::load(m.system().fs, &ctx.paths.state_file()).unwrap();
        assert_eq!(state.applied.as_ref().unwrap().wt_font.as_deref(), Some(NERD_FONT_FAMILY));

        let ctx2 = Ctx { now_ms: ctx.now_ms + 1, ..ctx.clone() };
        let UndoResult::Done { outcome, .. } = undo(m.system(), &ctx2).unwrap() else { panic!("nothing undone") };
        assert_eq!(outcome, Outcome::Committed);
        assert_eq!(everything(&m), before);
        assert_eq!(m.system().fs.read(&ctx.paths.state_file()).unwrap(), None);
        assert_eq!(undo(m.system(), &ctx2).unwrap(), UndoResult::NothingToUndo);
    }

    #[test]
    fn two_applies_undo_one_at_a_time() {
        let (m, ctx) = machine();
        let before = everything(&m);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        let after_first = everything(&m);
        let ctx2 = Ctx { now_ms: ctx.now_ms + 10, ..ctx.clone() };
        let p = prepare_apply(m.system(), &ctx2, &terracotta(), &request(Target::Wt)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx2, p), Outcome::Committed);
        let ctx3 = Ctx { now_ms: ctx.now_ms + 20, ..ctx.clone() };
        undo(m.system(), &ctx3).unwrap();
        assert_eq!(everything(&m), after_first);
        let ctx4 = Ctx { now_ms: ctx.now_ms + 30, ..ctx.clone() };
        undo(m.system(), &ctx4).unwrap();
        assert_eq!(everything(&m), before);
    }

    #[test]
    fn a_broken_settings_file_stops_the_apply_before_anything_changes() {
        let (m, ctx) = machine();
        m.fs.put(WT, b"{ \"profiles\": [ ");
        let before = m.fs.snapshot().0;
        let err = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap_err();
        assert_eq!(err.code(), "E_WT_PARSE");
        assert_eq!(m.fs.snapshot().0, before);
        assert_eq!(m.autorun.read_user().unwrap(), None);
    }

    #[test]
    fn arm64_is_refused() {
        let (mut m, ctx) = machine();
        m.host.info.arch = Arch::Arm64;
        assert_eq!(
            prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap_err().code(),
            "E_UNSUPPORTED_PLATFORM"
        );
    }

    #[test]
    fn a_failed_verify_leaves_the_machine_as_it_was() {
        let (m, ctx) = machine();
        m.process.answer("oh-my-posh.exe", "print", 0, " CONFIG PARSE ERROR ");
        let before = everything(&m);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert!(matches!(run_apply(m.system(), &ctx, p), Outcome::RolledBack { code: "E_VERIFY_OMP", .. }));
        assert_eq!(everything(&m), before);
        assert_eq!(undo(m.system(), &ctx).unwrap(), UndoResult::NothingToUndo);
    }

    #[test]
    fn a_crash_mid_apply_is_rolled_back_at_the_next_start() {
        let (m, ctx) = machine();
        let before = everything(&m);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        // Simulate a crash: capture and run ops by hand, stop after three.
        let root = ctx.paths.snapshots();
        let store = SnapshotStore::new(m.system().fs, &root);
        let mut journal = store.begin(p.manifest.clone());
        let mut exec = SystemExecutor::new(m.system(), &ctx.resources);
        use crate::txn::Executor;
        let snapshots = p
            .ops
            .iter()
            .enumerate()
            .map(|(i, op)| crate::txn::Snapshot { op_index: i, prior: exec.capture(op).unwrap() })
            .collect();
        let mut j = Journal { id: p.manifest.id.clone(), ops: p.ops.clone(), snapshots, stage: Stage::Snapshotted };
        journal.save(&j).unwrap();
        for (i, op) in p.ops.iter().take(3).enumerate() {
            exec.apply(op).unwrap();
            j.stage = Stage::Applying { done: i + 1 };
            journal.save(&j).unwrap();
        }
        let start = recover_at_start(m.system(), &ctx);
        assert!(matches!(start.recovered, Some(Outcome::RolledBack { code: "E_TXN_INTERRUPTED", .. })));
        assert_eq!(start.incomplete, None);
        assert_eq!(everything(&m), before);
        assert!(!m.system().fs.exists(&ctx.paths.omp_dir()));
        assert_eq!(recover_at_start(m.system(), &ctx), StartReport { recovered: None, incomplete: None });
    }

    #[test]
    fn a_rollback_that_cannot_finish_says_how_to_fix_it_even_after_a_restart() {
        // Spec §5.5: say which parts could not be put back and how to fix them by hand.
        let (m, ctx) = machine();
        let init = ctx.paths.generated().join("init.lua");
        m.fs.put(&init, b"old");
        // init.lua can be neither written nor written back
        m.fs.fail_writes_to(&init);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        let out = run_apply(m.system(), &ctx, p);
        assert!(matches!(&out, Outcome::RollbackIncomplete { failed, .. } if failed.len() == 1), "{out:?}");
        // the next start finds the record
        let later = Ctx { now_ms: ctx.now_ms + 1, ..ctx.clone() };
        let start = recover_at_start(m.system(), &later);
        assert_eq!(start.recovered, None, "the journal itself is closed");
        let record = start.incomplete.unwrap();
        assert_eq!(record.code, "E_DATA_WRITE");
        let step = &record.steps[0];
        assert_eq!(step.target, format!("file {}", init.display()));
        assert_eq!(step.code, "E_DATA_WRITE");
        let payload =
            SnapshotStore::new(m.system().fs, &later.paths.snapshots()).payload_path(&record.id, step.op_index);
        assert_eq!(step.fix, format!("Copy {} over {}.", payload.display(), init.display()));
        // and the file the fix names holds what init.lua held before
        assert_eq!(m.fs.file(&payload), Some(b"old".to_vec()));
        assert_eq!(pending_incomplete(m.system(), &later), Some(record));
    }
}
```
`a_crash_mid_apply_is_rolled_back_at_the_next_start` stops after the first three ops, which in Phase 1a's planner are the two runtime installs and `theme.omp.json`. `apply_then_undo_is_byte_identical` checks that our entry goes first, in parentheses, and that the other program's command follows it unchanged, in its `REG_EXPAND_SZ` (ruling P59, spec §5.4). `a_rollback_that_cannot_finish_says_how_to_fix_it_even_after_a_restart` makes `init.lua` impossible to write or write back, so the apply fails and its rollback cannot put that file back: the outcome, the record on disk and `pending_incomplete` must all say which file and how to fix it.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- actions::tests`
Expected: compile errors (`cannot find struct Ctx`, …).

- [ ] **Step 3: Implement it above the `testing` module**
```rust
//! The commands behind the CLI (spec §5.7) and, later, the app's IPC:
//! apply, undo, and recovery when a run starts. Each one is a transaction
//! with its own restore point (spec §5.5).
use crate::adapters::{Arch, System};
use crate::detect::{detect, DetectError, Detected};
use crate::exec::{fix_by_hand, font_target, op_label, sweep, SystemExecutor, UndoExecutor, UndoVerifier};
use crate::generate::lua::Lang;
use crate::model::Theme;
use crate::plan::{plan, Op, Paths, Plan, PlanError, Request, CLINK_VERSION, OMP_VERSION};
use crate::resources::{sha256_hex, ResourceError, Resources};
use crate::snapshot::{new_id, Incomplete, Manifest, SnapshotKind, SnapshotStore, ThemeRef, Unrestored, KEEP};
use crate::state::{Applied, AutoRunRecord, FontRecord, RegValueRecord, State, StateError};
use crate::txn::{self, Failure, Outcome};
use crate::verify::SystemVerifier;
use crate::wt_edit::{apply_edit, WtEdit, WtError};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Where things are, and the clock.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub paths: Paths,
    pub resources: Resources,
    pub now_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActionError {
    #[error("CMD Studio supports x64 Windows only; this machine is ARM64")]
    Unsupported,
    #[error(transparent)]
    Resources(#[from] ResourceError),
    #[error(transparent)]
    Detect(#[from] DetectError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error("{path}: {error}")]
    Wt { path: PathBuf, error: WtError },
    #[error("{0}")]
    Snapshot(String),
}

impl ActionError {
    /// Stable code shown to users (spec §5.9).
    pub fn code(&self) -> &'static str {
        match self {
            ActionError::Unsupported => "E_UNSUPPORTED_PLATFORM",
            ActionError::Resources(e) => e.code(),
            ActionError::Detect(e) => e.code(),
            ActionError::State(e) => e.code(),
            ActionError::Plan(e) => e.code(),
            ActionError::Wt { error, .. } => error.code(),
            ActionError::Snapshot(_) => "E_SNAPSHOT_WRITE",
        }
    }
}

impl From<Failure> for ActionError {
    fn from(f: Failure) -> Self {
        ActionError::Snapshot(f.message)
    }
}

/// An apply that is planned but has not touched the machine yet.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub plan: Plan,
    pub detected: Detected,
    /// The plan's ops, then the write of the new state.json.
    pub ops: Vec<Op>,
    pub state_after: State,
    pub manifest: Manifest,
}

fn lang_code(lang: Lang) -> &'static str {
    match lang {
        Lang::Th => "th",
        Lang::En => "en",
    }
}

/// Detects, plans and works out the new state.json, without changing
/// anything. A settings.json that does not parse stops it here (spec §5.4:
/// never touch that file).
pub fn prepare_apply(sys: System<'_>, ctx: &Ctx, theme: &Theme, req: &Request) -> Result<Prepared, ActionError> {
    ctx.resources.manifest(sys.fs)?;
    let detected = detect(sys, &ctx.paths)?;
    if detected.info.arch == Arch::Arm64 {
        return Err(ActionError::Unsupported);
    }
    let mut state = State::load(sys.fs, &ctx.paths.state_file())?;
    let plan = plan(theme, req, &detected.machine, &ctx.paths)?;
    let id = new_id(ctx.now_ms);
    let mut generated = BTreeMap::new();
    let mut wt_font = None;

    for op in &plan.ops {
        match op {
            Op::WtApply { settings, scheme, profile, set_default } => {
                let wt = |error| ActionError::Wt { path: settings.clone(), error };
                let bytes = sys.fs.read(settings).map_err(|e| wt(WtError::Parse(e.to_string())))?.unwrap_or_default();
                let text = String::from_utf8(bytes).map_err(|e| wt(WtError::Parse(e.to_string())))?;
                let edited = apply_edit(&text, &WtEdit { scheme, profile, set_default: *set_default }).map_err(wt)?;
                state.baseline.note_wt(settings, &edited);
                wt_font = profile.get("font").and_then(|f| f.get("face")).and_then(|f| f.as_str()).map(str::to_string);
            }
            Op::SetAutoRun { value } => {
                let key_existed = sys.autorun.user_key_exists().map_err(DetectError)?;
                let current = sys.autorun.read_user().map_err(DetectError)?.as_ref().map(RegValueRecord::from_reg);
                state.baseline.note_autorun(&AutoRunRecord { key_existed, value: current }, value.as_deref());
            }
            Op::InstallFont { family } => {
                if let Some((_, file, value_name)) = font_target(sys, &ctx.resources, family) {
                    state.baseline.note_font(FontRecord { family: family.clone(), file, value_name });
                }
            }
            Op::InstallRuntime { dir, .. } => {
                let pd = &detected.program_data.0;
                if dir.parent() == Some(pd.as_path()) && !sys.fs.exists(pd) && state.baseline.program_data_dir.is_none()
                {
                    state.baseline.program_data_dir = Some(pd.clone());
                }
            }
            // Doctor D9 compares the files in generated (spec §5.8).
            Op::WriteFile { path, contents } if path.starts_with(ctx.paths.generated()) => {
                generated.insert(path.clone(), sha256_hex(contents.as_bytes()));
            }
            Op::WriteFile { .. } => {}
        }
    }

    // Doctor D3 reads what Clink logs after this point (a missing log is 0 bytes).
    let clink_log = ctx.paths.clink_profile().join("clink.log");
    let clink_log_len =
        sys.fs.read(&clink_log).ok().flatten().map_or(0, |bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX));
    state.applied = Some(Applied {
        theme_id: theme.id,
        theme_name: theme.name.clone(),
        target: req.target,
        set_default: req.set_wt_default_profile,
        lang: lang_code(req.lang).to_string(),
        launch: (&plan.launch).into(),
        omp_version: OMP_VERSION.to_string(),
        clink_version: CLINK_VERSION.to_string(),
        generated,
        wt_font,
        clink_log_len,
        snapshot_id: id.clone(),
        applied_at_ms: ctx.now_ms,
    });
    let state_text = String::from_utf8(state.to_bytes()?).map_err(|e| StateError::Invalid(e.to_string()))?;
    let mut ops = plan.ops.clone();
    ops.push(Op::WriteFile { path: ctx.paths.state_file(), contents: state_text });
    let manifest = Manifest {
        id,
        kind: SnapshotKind::Apply,
        created_ms: ctx.now_ms,
        theme: Some(ThemeRef { id: theme.id, name: theme.name.clone() }),
        target: Some(req.target),
        summary: plan.summary_en.iter().map(|s| s.to_string()).collect(),
        undoes: None,
        undone_by: None,
    };
    Ok(Prepared { plan, detected, ops, state_after: state, manifest })
}

/// Runs a prepared apply as one transaction. Prunes old restore points after a commit.
pub fn run_apply(sys: System<'_>, ctx: &Ctx, prepared: Prepared) -> Outcome {
    let root = ctx.paths.snapshots();
    let store = SnapshotStore::new(sys.fs, &root);
    let id = prepared.manifest.id.clone();
    let mut journal = store.begin(prepared.manifest);
    let mut exec = SystemExecutor::new(sys, &ctx.resources);
    let mut verifier = SystemVerifier::new(sys, &ctx.paths, &ctx.resources, Some(prepared.plan.launch.clone()));
    let outcome = txn::run(&id, prepared.ops, &mut exec, &mut verifier, &mut journal);
    record_incomplete(&store, &id, &outcome);
    if outcome == Outcome::Committed {
        let _ = store.prune(KEEP);
    }
    outcome
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UndoResult {
    NothingToUndo,
    Done { undid: String, outcome: Outcome },
}

/// Puts back what the newest apply that is not undone yet changed, byte for
/// byte (spec §4.4 "ย้อนกลับ", CLI `undo`).
pub fn undo(sys: System<'_>, ctx: &Ctx) -> Result<UndoResult, ActionError> {
    let root = ctx.paths.snapshots();
    let store = SnapshotStore::new(sys.fs, &root);
    let Some((applied, journal)) = store.latest_undoable()? else {
        return Ok(UndoResult::NothingToUndo);
    };
    let priors: Vec<(Op, Option<Vec<u8>>)> = journal
        .snapshots
        .iter()
        .filter_map(|s| journal.ops.get(s.op_index).map(|op| (op.clone(), s.prior.clone())))
        .collect();
    let ops: Vec<Op> = journal.ops.iter().rev().cloned().collect();
    let id = new_id(ctx.now_ms);
    let manifest = Manifest {
        id: id.clone(),
        kind: SnapshotKind::Undo,
        created_ms: ctx.now_ms,
        theme: applied.theme.clone(),
        target: applied.target,
        summary: vec![format!("Undo {}", applied.id)],
        undoes: Some(applied.id.clone()),
        undone_by: None,
    };
    let mut store_txn = store.begin(manifest);
    let mut exec = UndoExecutor::new(SystemExecutor::new(sys, &ctx.resources), priors.clone());
    let mut verifier = UndoVerifier::new(SystemExecutor::new(sys, &ctx.resources), priors);
    let outcome = txn::run(&id, ops, &mut exec, &mut verifier, &mut store_txn);
    record_incomplete(&store, &id, &outcome);
    if outcome == Outcome::Committed {
        store.mark_undone(std::slice::from_ref(&applied.id), &id)?;
        let _ = store.prune(KEEP);
    }
    Ok(UndoResult::Done { undid: applied.id, outcome })
}

/// What happened when a run started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartReport {
    /// How the rollback of a transaction a crash cut off ended; `None` when there was none.
    pub recovered: Option<Outcome>,
    /// What the newest transaction could not put back, when it ended `RollbackIncomplete`
    /// and nothing has run since: it may still need a hand (spec §5.5).
    pub incomplete: Option<Incomplete>,
}

/// Rolls back a transaction a crash cut off (spec §5.5), deletes half-made
/// runtime folders, and finds what a rollback left for a hand to fix.
pub fn recover_at_start(sys: System<'_>, ctx: &Ctx) -> StartReport {
    let root = ctx.paths.snapshots();
    let store = SnapshotStore::new(sys.fs, &root);
    let mut journal = store.recovery();
    let mut exec = SystemExecutor::new(sys, &ctx.resources);
    let recovered = txn::recover(&mut exec, &mut journal);
    if let Some(outcome @ Outcome::RollbackIncomplete { .. }) = &recovered {
        // Recovery works on the newest restore point.
        if let Some(id) = store.ids().ok().and_then(|mut ids| ids.pop()) {
            record_incomplete(&store, &id, outcome);
        }
    }
    sweep(sys, &ctx.paths);
    StartReport { recovered, incomplete: store.latest_incomplete().ok().flatten() }
}

/// What the newest transaction could not put back, if it ended `RollbackIncomplete`.
pub fn pending_incomplete(sys: System<'_>, ctx: &Ctx) -> Option<Incomplete> {
    let root = ctx.paths.snapshots();
    SnapshotStore::new(sys.fs, &root).latest_incomplete().ok().flatten()
}

/// Writes `incomplete.json` into restore point `id` when `outcome` is `RollbackIncomplete`:
/// each op that was not put back, its error, and how to fix it by hand, so that this survives
/// a restart (spec §5.5). Best effort: the outcome itself already says the rollback is
/// incomplete, and is never reported as a success.
pub(crate) fn record_incomplete(store: &SnapshotStore<'_>, id: &str, outcome: &Outcome) {
    let Outcome::RollbackIncomplete { code, reason, failed } = outcome else {
        return;
    };
    let journal = store.journal(id).ok();
    let steps = failed
        .iter()
        .map(|(index, failure)| {
            let op = journal.as_ref().and_then(|j| j.ops.get(*index));
            let snapshot = journal.as_ref().and_then(|j| j.snapshots.iter().find(|s| s.op_index == *index));
            let fix = match (op, snapshot) {
                (Some(op), Some(s)) => fix_by_hand(op, &s.prior, &store.payload_path(id, *index)),
                _ => "Close every cmd window and Windows Terminal, then run `cmdstudio \
                      --cleanup`, which takes out what CMD Studio changed."
                    .to_string(),
            };
            Unrestored {
                op_index: *index,
                target: op.map_or_else(|| format!("step {index}"), op_label),
                code: failure.code.to_string(),
                message: failure.message.clone(),
                fix,
            }
        })
        .collect();
    let _ = store.write_incomplete(&Incomplete {
        id: id.to_string(),
        code: code.to_string(),
        reason: reason.clone(),
        steps,
    });
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- actions::tests`
Expected: `7 passed`.

- [ ] **Step 5: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 6: Commit**

Message:
```text
feat(core): apply, undo and recover as transactions

prepare_apply detects, plans and dry-runs every settings.json edit, so a
file that does not parse stops the apply before anything changes; it
works out the new state.json, which is the last op of the same
transaction. undo replays the newest unreversed apply's ops in reverse,
restoring what that apply captured, and checks every target is
byte-identical. recover_at_start rolls back a transaction a crash cut
off. ARM64 is refused with E_UNSUPPORTED_PLATFORM.

A rollback that cannot finish leaves incomplete.json in its restore
point: each target that was not put back, the error, and how to fix it
by hand. recover_at_start and pending_incomplete give it back after a
restart, until a newer transaction runs (spec 5.5).
```
Run:
```bash
git add -- crates/core/src/lib.rs crates/core/src/actions.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 14: Revert everything and `--cleanup`

**Files:**
- Create: `crates/core/src/cleanup.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod cleanup;`)

**Interfaces:**
- Consumes:
  - Task 1: `adapters::{program_data_dir, DirState, System, AutoRun, Fonts, Fs, RegValue, REG_EXPAND_SZ, REG_SZ}`. Task 7: `wt_edit::revert`. Task 8: `state::{AutoRunRecord, AutoRunWritten, Baseline, RegValueRecord, State}`, `Paths::{state_file, snapshots, themes, runtime_root}`. Task 9: `snapshot::{new_id, Manifest, SnapshotKind, SnapshotStore}`. Task 10: `detect::DetectError`. Task 11: `exec::SystemExecutor`. Task 12: `verify::SystemVerifier`. Task 13: `actions::{record_incomplete, ActionError, Ctx, prepare_apply, run_apply, undo, UndoResult}` and `actions::testing::{machine, request, SETTINGS, WT}`.
  - Phase 1a: `plan::autorun::remove_ours` (`pub(crate)`: the planner's own AutoRun editor, which is why this module lives in the core), `plan::{autorun_entry, ClinkLaunch, Op, Paths, PlanError, NERD_FONT_FAMILY}`, `txn::{self, Outcome}`, `model::Target`, the test-only `presets::terracotta()`.
- Produces (in `cmdstudio_core::cleanup`): `Step { what: String, ok: bool, detail: String }`, `Report { outcome: Option<Outcome>, steps: Vec<Step> }` with `Report::ok() -> bool`; `autorun_after_revert(current: Option<&str>, original: Option<&AutoRunRecord>, written: Option<&AutoRunWritten>, paths: &Paths) -> Result<Option<Option<String>>, PlanError>` (`Ok(None)`: leave AutoRun as it is; `Ok(Some(None))`: delete the value; `Err`: `E_AUTORUN_WRITE`, a hand edit); `revert_all(System<'_>, &Ctx) -> Result<Report, ActionError>`; `cleanup(System<'_>, &Ctx, remove_themes: bool) -> Result<Report, ActionError>`.

**The rules** (spec §6.3, brief), which keep every change the user made after CMD Studio:
1. AutoRun: if nobody changed it since CMD Studio last wrote it, the original comes back (including another program's Clink that `--replace-other-clink` removed), minus any entry of ours; otherwise only our entries leave the current value. "Our entries" are exactly what the planner's editor takes out, `plan::autorun::remove_ours`: the parenthesized entry and the older forms, found by our profile path, each with one separator, every other byte left as it was. So the planner and the cleanup can never disagree about what is ours. A value that cannot be edited safely (cmd itself rejects it, or a command holds our profile path without being one of our entries) is not touched: the report gets a failed step with `E_AUTORUN_WRITE` and the Registry Editor hand edit, and the rest of the revert still runs. The value is deleted when nothing but blanks is left; the `Command Processor` key is deleted only if CMD Studio created it and it is empty.
2. Each settings.json: `wt_edit::revert` removes the `CMD Studio` scheme, gives each cmd-profile key its original back if it still holds what CMD Studio wrote (else the user's value stays), and removes containers CMD Studio added once they are empty. A file that is gone is skipped; a file that does not parse is reported (`E_WT_PARSE`) and not touched.
3. Fonts CMD Studio installed are unloaded and unregistered; a file Windows Terminal still holds stays until it is free (reported, not a failure). A font that cannot be unregistered stays in the baseline for the next attempt.
4. `--cleanup` only: the app data folder except `themes\` (unless `--remove-themes`), and the user's ProgramData folder when it is private to this user (a folder anyone else controls is left alone, and the report says why).

Steps 1–2 run as **one transaction** with a `revertAll` restore point (so a failure rolls both back), verified by read-back; like every transaction, a rollback of it that cannot finish leaves `incomplete.json` (`actions::record_incomplete`, Task 13). After it commits, every apply is marked undone, because an `undo` would otherwise bring back a theme the user removed. Then `state.json` is rewritten with no applied theme and only the fonts that are still installed. A damaged `state.json` is renamed `state.json.damaged-<ms>` and reported; our AutoRun entries still go (rule 1 with no baseline). Undo is byte-identical; revert is key by key, so a file CMD Studio grew from a one-line object may come back with different whitespace (Task 7: `revert_after_growing_a_one_line_object_restores_values_not_layout`).

- [ ] **Step 1: Write the failing tests** — add `pub mod cleanup;` to `crates/core/src/lib.rs` and create `crates/core/src/cleanup.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::testing::{machine, request, SETTINGS, WT};
    use crate::actions::{prepare_apply, run_apply, undo, UndoResult};
    use crate::adapters::{AutoRun, Fonts, Fs, RegValue, REG_EXPAND_SZ, REG_SZ};
    use crate::model::Target;
    use crate::plan::{autorun_entry, ClinkLaunch, NERD_FONT_FAMILY};
    use crate::presets::terracotta;
    use std::path::PathBuf;

    fn apply(m: &crate::adapters::fake::FakeMachine, ctx: &Ctx, target: Target) {
        let p = prepare_apply(m.system(), ctx, &terracotta(), &request(target)).unwrap();
        assert_eq!(run_apply(m.system(), ctx, p), Outcome::Committed);
    }

    fn later(ctx: &Ctx, ms: u64) -> Ctx {
        Ctx { now_ms: ctx.now_ms + ms, ..ctx.clone() }
    }

    #[test]
    fn revert_all_on_an_untouched_machine_gives_the_original_back() {
        let (m, ctx) = machine();
        let conda = RegValue::from_text(REG_EXPAND_SZ, r#""%USERPROFILE%\conda_hook.bat""#);
        m.autorun.write_user(&conda).unwrap();
        apply(&m, &ctx, Target::All);
        let report = revert_all(m.system(), &later(&ctx, 5)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert_eq!(m.autorun.read_user().unwrap(), Some(conda));
        assert_eq!(m.fs.file(WT).unwrap(), SETTINGS.as_bytes());
        assert!(!m.fonts.families().unwrap().contains(&NERD_FONT_FAMILY.to_string()));
        assert_eq!(undo(m.system(), &later(&ctx, 6)).unwrap(), UndoResult::NothingToUndo);
    }

    #[test]
    fn revert_all_keeps_what_the_user_changed_after_us() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let ours = m.autorun.read_user().unwrap().unwrap().text().unwrap();
        m.autorun.write_user(&RegValue::from_text(REG_SZ, &format!("{ours} & echo mine"))).unwrap();
        let text = String::from_utf8(m.fs.file(WT).unwrap()).unwrap().replace("\"opacity\": 96", "\"opacity\": 70");
        m.fs.put(WT, text);
        revert_all(m.system(), &later(&ctx, 5)).unwrap();
        assert_eq!(m.autorun.read_user().unwrap().unwrap().text().as_deref(), Some("echo mine"));
        let after = String::from_utf8(m.fs.file(WT).unwrap()).unwrap();
        assert!(after.contains("\"opacity\": 70") && !after.contains("CMD Studio"));
    }

    #[test]
    fn autorun_rule_table() {
        let paths = Paths { data: PathBuf::from(r"C:\d") };
        let ours = autorun_entry(&ClinkLaunch::Direct(paths.clink_exe()), &paths).unwrap();
        // our entry as builds before ruling P59 wrote it, without the parentheses
        let older = r#"if exist "C:\d\runtime\clink-1.9.0\clink_x64.exe" "C:\d\runtime\clink-1.9.0\clink_x64.exe" inject --autorun --quiet --profile "C:\d\clink-profile""#;
        let rec = |t: &str| AutoRunRecord {
            key_existed: true,
            value: Some(RegValueRecord::from_reg(&RegValue::from_text(REG_SZ, t))),
        };
        let absent = AutoRunRecord { key_existed: true, value: None };
        let wrote = |t: &str| AutoRunWritten { value: Some(t.to_string()) };
        let foreign = r#""C:\clink\clink.bat" inject --autorun"#;
        // Untouched since our write: the original comes back, a foreign Clink that the apply
        // took out included.
        assert_eq!(
            autorun_after_revert(Some(&ours), Some(&rec(foreign)), Some(&wrote(&ours)), &paths),
            Ok(Some(Some(foreign.to_string())))
        );
        assert_eq!(autorun_after_revert(Some(&ours), Some(&absent), Some(&wrote(&ours)), &paths), Ok(Some(None)));
        // Changed by someone after us: only our parts leave, the older form too, and every
        // other byte stays.
        let changed = format!("{ours} & echo a ^& b & {older} & echo z");
        assert_eq!(
            autorun_after_revert(Some(&changed), Some(&absent), Some(&wrote(&ours)), &paths),
            Ok(Some(Some("echo a ^& b & echo z".into())))
        );
        // No state at all: still only our part leaves; nothing of ours means no change.
        assert_eq!(autorun_after_revert(Some(&ours), None, None, &paths), Ok(Some(None)));
        assert_eq!(autorun_after_revert(Some("echo hi"), None, None, &paths), Ok(None));
        assert_eq!(autorun_after_revert(None, None, None, &paths), Ok(None));
        // A command that holds our profile path without being one of our entries is left for
        // a hand edit: cutting it out would take other text with it.
        let err = autorun_after_revert(Some(&format!("echo ^& {ours}")), None, None, &paths).unwrap_err();
        assert_eq!(err.code(), "E_AUTORUN_WRITE");
    }

    #[test]
    fn an_autorun_that_needs_a_hand_edit_is_reported_and_left_alone() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let ours = m.autorun.read_user().unwrap().unwrap().text().unwrap();
        let tangled = RegValue::from_text(REG_SZ, &format!("echo ^& {ours}"));
        m.autorun.write_user(&tangled).unwrap();
        let report = revert_all(m.system(), &later(&ctx, 5)).unwrap();
        assert!(!report.ok());
        assert!(report.steps.iter().any(|s| !s.ok && s.detail.starts_with("E_AUTORUN_WRITE")), "{report:?}");
        assert_eq!(m.autorun.read_user().unwrap(), Some(tangled));
        // the rest is still put back
        assert_eq!(m.fs.file(WT).unwrap(), SETTINGS.as_bytes());
    }

    #[test]
    fn cleanup_deletes_data_but_keeps_themes_unless_asked() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        m.fs.put(ctx.paths.themes().join("mine.cmdtheme"), b"{}");
        let report = cleanup(m.system(), &later(&ctx, 5), false).unwrap();
        assert!(report.ok(), "{report:?}");
        assert!(m.fs.exists(&ctx.paths.themes().join("mine.cmdtheme")));
        assert!(!m.fs.exists(&ctx.paths.runtime_root()));
        assert!(!m.fs.exists(&ctx.paths.state_file()));
        cleanup(m.system(), &later(&ctx, 6), true).unwrap();
        assert!(!m.fs.exists(&ctx.paths.data));
        assert_eq!(m.autorun.read_user().unwrap(), None);
    }

    #[test]
    fn cleanup_removes_a_private_program_data_folder_and_leaves_a_foreign_one() {
        let (m, ctx) = machine();
        let pd = PathBuf::from(r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001");
        m.fs.put(pd.join(r"clink-1.9.34\clink_x64.exe"), b"x");
        m.host.set_dir_state(&pd, crate::adapters::DirState::Private);
        cleanup(m.system(), &ctx, false).unwrap();
        assert!(!m.fs.exists(&pd));
        m.fs.put(pd.join("trap.exe"), b"x");
        m.host.set_dir_state(&pd, crate::adapters::DirState::Unsafe("owned by S-1-5-21-9".into()));
        let report = cleanup(m.system(), &later(&ctx, 1), false).unwrap();
        assert!(m.fs.exists(&pd.join("trap.exe")));
        assert!(report.steps.iter().any(|s| s.detail.starts_with("left alone")));
    }

    #[test]
    fn a_damaged_state_file_still_removes_our_autorun_part() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        m.fs.put(ctx.paths.state_file(), b"garbage");
        let report = revert_all(m.system(), &later(&ctx, 5)).unwrap();
        assert!(!report.ok());
        assert_eq!(m.autorun.read_user().unwrap(), None);
    }
}
```
`revert_all_keeps_what_the_user_changed_after_us` relies on Terracotta's cmd profile holding `"opacity": 96`, which the shipped preset and `generate::wt` write. `autorun_rule_table` builds our entry with Phase 1a's `autorun_entry`, so it follows the shipped form; the older bare `if exist …` entry stands for what a build before ruling P59 wrote.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- cleanup::tests`
Expected: compile errors (`cannot find function revert_all`, `cannot find type Report`, …).

- [ ] **Step 3: Implement `cleanup.rs` above its tests**
```rust
//! "Revert everything" and the uninstaller's `--cleanup` (spec §6.3), key by
//! key, so changes the user made after CMD Studio survive:
//!
//! 1. only our part of AutoRun goes;
//! 2. the cmd profile keys come back and the `CMD Studio` scheme goes;
//! 3. fonts CMD Studio installed go;
//! 4. (`--cleanup` only) app data except `themes\`, and the ProgramData folder.
use crate::actions::{record_incomplete, ActionError, Ctx};
use crate::adapters::{program_data_dir, DirState, System};
use crate::exec::SystemExecutor;
use crate::plan::autorun::remove_ours;
use crate::plan::{Op, Paths, PlanError};
use crate::snapshot::{new_id, Manifest, SnapshotKind, SnapshotStore};
use crate::state::{AutoRunRecord, AutoRunWritten, Baseline, RegValueRecord, State};
use crate::txn::{self, Outcome};
use crate::verify::SystemVerifier;
use crate::wt_edit::revert;

/// One step's result, for the report and the exit code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub what: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Report {
    /// The AutoRun and settings.json transaction; `None` when there was nothing to change.
    pub outcome: Option<Outcome>,
    pub steps: Vec<Step>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.steps.iter().all(|s| s.ok)
            && !matches!(self.outcome, Some(Outcome::RolledBack { .. } | Outcome::RollbackIncomplete { .. }))
    }
    fn step(&mut self, what: impl Into<String>, result: Result<String, String>) {
        let (ok, detail) = match result {
            Ok(d) => (true, d),
            Err(d) => (false, d),
        };
        self.steps.push(Step { what: what.into(), ok, detail });
    }
}

/// `value` without CMD Studio's entries, taken out by the planner's own editor
/// (`plan::autorun::remove_ours`), so the planner and the cleanup never disagree about what is
/// ours; `None` when nothing but blanks is left. Fails with `E_AUTORUN_WRITE` when the value
/// can't be edited safely and needs a hand edit.
fn without_ours(value: &str, profile: &str) -> Result<Option<String>, PlanError> {
    let edit = remove_ours(value, profile)?;
    Ok((!edit.value.trim().is_empty()).then_some(edit.value))
}

/// What AutoRun becomes (spec §6.3 step 1). `Ok(None)`: leave it as it is.
/// `Ok(Some(None))`: delete the value. When nobody changed AutoRun since CMD
/// Studio last wrote it, the original comes back (minus any entry of ours);
/// otherwise only our entries leave the current value. A value that can't be
/// edited safely is `E_AUTORUN_WRITE`, and is left for a hand edit.
pub fn autorun_after_revert(
    current: Option<&str>,
    original: Option<&AutoRunRecord>,
    written: Option<&AutoRunWritten>,
    paths: &Paths,
) -> Result<Option<Option<String>>, PlanError> {
    // The planner refuses a profile path that is not Unicode, so no entry of ours holds one.
    let profile_path = paths.clink_profile();
    let Some(profile) = profile_path.to_str() else {
        return Ok(None);
    };
    let untouched = written.is_some_and(|w| w.value.as_deref() == current);
    let target = match (original, untouched) {
        (Some(orig), true) => {
            let text = orig.value.as_ref().and_then(RegValueRecord::to_reg).and_then(|v| v.text());
            match text {
                Some(t) => without_ours(&t, profile)?,
                None => None,
            }
        }
        _ => match current {
            Some(c) => without_ours(c, profile)?,
            None => None,
        },
    };
    Ok((target.as_deref() != current).then_some(target))
}

/// Undoes everything CMD Studio changed, key by key (the "ย้อนทุกอย่าง" button, spec §4.3).
pub fn revert_all(sys: System<'_>, ctx: &Ctx) -> Result<Report, ActionError> {
    let mut report = Report::default();
    let state = match State::load(sys.fs, &ctx.paths.state_file()) {
        Ok(s) => s,
        Err(e) => {
            // Keep the damaged file for a person to look at; a fresh one replaces it below.
            let file = ctx.paths.state_file();
            let _ = sys.fs.rename(&file, &file.with_extension(format!("json.damaged-{}", ctx.now_ms)));
            report.step("Read state.json", Err(format!("{e}; only CMD Studio's own entries are removed")));
            State::default()
        }
    };
    let base = &state.baseline;
    let mut ops = Vec::new();

    // 1. AutoRun.
    let current = sys.autorun.read_user().map_err(crate::detect::DetectError)?.and_then(|v| v.text());
    match autorun_after_revert(current.as_deref(), base.autorun.as_ref(), base.autorun_written.as_ref(), &ctx.paths) {
        Ok(Some(next)) => ops.push(Op::SetAutoRun { value: next }),
        Ok(None) => {}
        Err(e) => report.step(
            "Take CMD Studio's part out of AutoRun",
            Err(format!(
                "{}: {e}. Open Registry Editor at HKEY_CURRENT_USER\\Software\\Microsoft\\Command \
                 Processor and take CMD Studio's part out of the AutoRun value by hand",
                e.code()
            )),
        ),
    }
    // 2. Windows Terminal, file by file.
    for file in &base.wt {
        let text = match sys.fs.read(&file.settings) {
            Ok(Some(bytes)) => String::from_utf8(bytes).map_err(|e| e.to_string()),
            Ok(None) => {
                report.step(
                    format!("Restore {}", file.settings.display()),
                    Ok("the file is gone; nothing to restore".into()),
                );
                continue;
            }
            Err(e) => Err(e.to_string()),
        };
        match text.and_then(|t| {
            revert(&t, &file.originals_map(), &file.written_map(), &file.created)
                .map(|n| (t, n))
                .map_err(|e| e.to_string())
        }) {
            Ok((old, new)) if old != new => ops.push(Op::WriteFile { path: file.settings.clone(), contents: new }),
            Ok(_) => {}
            Err(e) => report.step(format!("Restore {}", file.settings.display()), Err(format!("E_WT_PARSE: {e}"))),
        }
    }
    if !ops.is_empty() {
        let root = ctx.paths.snapshots();
        let store = SnapshotStore::new(sys.fs, &root);
        let id = new_id(ctx.now_ms);
        let manifest = Manifest {
            id: id.clone(),
            kind: SnapshotKind::RevertAll,
            created_ms: ctx.now_ms,
            theme: None,
            target: None,
            summary: vec!["Revert everything".into()],
            undoes: None,
            undone_by: None,
        };
        let mut journal = store.begin(manifest);
        let mut exec = SystemExecutor::new(sys, &ctx.resources);
        let mut verifier = SystemVerifier::new(sys, &ctx.paths, &ctx.resources, None);
        let outcome = txn::run(&id, ops, &mut exec, &mut verifier, &mut journal);
        record_incomplete(&store, &id, &outcome);
        let committed = outcome == Outcome::Committed;
        report.outcome = Some(outcome);
        if !committed {
            return Ok(report);
        }
        // An undo after this would bring back a theme the user removed.
        let applies: Vec<String> = store
            .ids()?
            .into_iter()
            .filter(|i| store.manifest(i).is_ok_and(|m| m.kind == SnapshotKind::Apply))
            .collect();
        store.mark_undone(&applies, &id)?;
    }
    // AutoRun's key, when CMD Studio created it and it is empty now.
    if base.autorun.as_ref().is_some_and(|a| !a.key_existed) {
        let r = sys
            .autorun
            .delete_user_key_if_empty()
            .map(|()| "done".to_string())
            .map_err(|e| format!("E_AUTORUN_WRITE: {e}"));
        report.step("Remove the empty Command Processor key", r);
    }
    // 3. Fonts CMD Studio installed. A file still in use stays, unregistered.
    let mut kept = Baseline::default();
    for font in &base.fonts {
        let _ = sys.fonts.unload(&font.file);
        match sys.fonts.unregister(&font.value_name) {
            Ok(()) => {
                let detail = match sys.fs.remove_file(&font.file) {
                    Ok(()) => "removed".to_string(),
                    Err(e) => format!("unregistered; the file stays until it is free ({e})"),
                };
                report.step(format!("Uninstall font {}", font.family), Ok(detail));
            }
            Err(e) => {
                kept.fonts.push(font.clone());
                report.step(format!("Uninstall font {}", font.family), Err(format!("E_FONT_INSTALL: {e}")));
            }
        }
    }
    let next = State { applied: None, baseline: kept, ..State::default() };
    let saved = next
        .to_bytes()
        .map_err(|e| e.to_string())
        .and_then(|b| sys.fs.write_atomic(&ctx.paths.state_file(), &b).map_err(|e| e.to_string()));
    if let Err(e) = saved {
        report.step("Save state.json", Err(format!("E_DATA_WRITE: {e}")));
    }
    Ok(report)
}

/// `cmdstudio --cleanup [--remove-themes]`: revert everything, then delete
/// the app's data (keeping `themes\` unless asked) and the user's
/// ProgramData folder (spec §6.3 step 4).
pub fn cleanup(sys: System<'_>, ctx: &Ctx, remove_themes: bool) -> Result<Report, ActionError> {
    let mut report = revert_all(sys, ctx)?;
    if matches!(report.outcome, Some(Outcome::RolledBack { .. } | Outcome::RollbackIncomplete { .. })) {
        return Ok(report);
    }
    if let (Ok(pd), Ok(sid)) = (sys.host.program_data(), sys.host.user_sid()) {
        let dir = program_data_dir(&pd, &sid);
        match sys.host.private_dir_state(&dir) {
            Ok(DirState::Absent) => {}
            Ok(DirState::Private) => {
                let r = sys.fs.remove_dir_all(&dir).map(|()| "removed".to_string()).map_err(|e| e.to_string());
                report.step(format!("Remove {}", dir.display()), r);
            }
            Ok(DirState::Unsafe(why)) => {
                report.step(format!("Remove {}", dir.display()), Ok(format!("left alone: {why}")))
            }
            Err(e) => report.step(format!("Remove {}", dir.display()), Err(e.to_string())),
        }
    }
    for entry in sys.fs.list_dir(&ctx.paths.data).unwrap_or_default() {
        if !remove_themes && entry.path == ctx.paths.themes() {
            continue;
        }
        let r = if entry.is_dir { sys.fs.remove_dir_all(&entry.path) } else { sys.fs.remove_file(&entry.path) };
        if let Err(e) = r {
            report.step(format!("Delete {}", entry.path.display()), Err(e.to_string()));
        }
    }
    if remove_themes {
        let _ = sys.fs.remove_dir(&ctx.paths.data);
    }
    Ok(report)
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- cleanup::tests`
Expected: `7 passed`. Phase 1a's planner is not changed by this task: `cargo test -p cmdstudio-core -- plan::` still passes (49 tests).

- [ ] **Step 5: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 6: Commit**

Message:
```text
feat(core): revert everything and clean up, key by key

revert_all puts AutoRun back (the original when nobody touched it since
our write, else only our entries leave), reverts each settings.json key
by key, uninstalls our fonts and rewrites state.json. AutoRun and the
settings files change in one transaction. --cleanup also removes the app
data (themes\ kept unless asked) and the user's ProgramData folder when
it is private.

Our entries are taken out by the planner's own editor
(plan::autorun::remove_ours), so the two never disagree about what is
ours; a value it cannot edit safely is left for a hand edit and reported
with E_AUTORUN_WRITE.
```
Run:
```bash
git add -- crates/core/src/lib.rs crates/core/src/cleanup.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 15: Doctor D1–D11, and the spec catches up

**Files:**
- Create: `crates/core/src/doctor.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod doctor;`), `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§5.3, §5.4, §5.5, §5.8, §5.9, appendix B)

**Interfaces:**
- Consumes:
  - Task 1: `adapters::{DefaultTerminal, System, AutoRun, Fs, RegValue, REG_SZ}`; `FakeHost.info` (`default_terminal`, `build`). Task 2: `resources::{check_bundle, sha256_hex}`. Task 7: `wt_edit::{check_parses, read_value, WtKey}`. Task 8: `state::{Applied, State, StateError}` (`Applied.clink_log_len`), `Launch::to_clink()`. Task 12: `verify::lua_error_line`. Task 13: `actions::{Ctx, prepare_apply, run_apply}`, `actions::testing::{machine, request, WT}`.
  - Phase 1a: `plan::autorun::{is_our_entry, runs_clink, scan}` (`pub(crate)`: the planner's own reading of AutoRun, which is why the doctor lives in the core), `generate::wt::{CMD_EXE, CMD_PROFILE_GUID, SCHEME_NAME}`, `model::Target`, `plan::{autorun_entry, ClinkLaunch, Component, Op, Warning, OMP_VERSION}`, `txn::Outcome`, the test-only `presets::terracotta()`.
- Produces (in `cmdstudio_core::doctor`):
  - `Status::{Ok, Warn, Fail, Skipped}` (serde `"ok" | "warn" | "fail" | "skipped"`).
  - `Check { id: &'static str, status: Status, code: Option<&'static str>, message_th: String, message_en: String, details: String }` (`Serialize`, camelCase: `messageTh`, `messageEn`).
  - `InjectSeen::{Never, Ok, Failed(String)}`; `last_inject(log: &str) -> InjectSeen`.
  - `doctor(sys: System<'_>, ctx: &Ctx) -> Vec<Check>` — always 11 checks, D1 to D11 in order. Read only.

**What each check looks at** (spec §5.8, brief; `ok` only after a real look, spec §2.1; a check that needs an applied theme is `skipped` without one):

| | `ok` when | otherwise |
|---|---|---|
| D1 | every runtime file of the applied theme has its pinned hash (the Clink folder is `<clink-launch>`'s) | fail `E_RUNTIME_MISSING` |
| D2 | `oh-my-posh version` exits 0 and prints exactly `31.5.0` (checked 2026-10-11 with the pinned binary) | fail `E_RUNTIME_MISSING` |
| D3 | `<clink-launch> --profile <p> set clink.logo` prints no Lua error, **and** what `clink.log` in the profile recorded since the apply (the bytes after `Applied.clink_log_len`; all of it when the log is shorter, because it was started over) last says `Inject completed` | Lua error: fail `E_VERIFY_CLINK`; the log says `Unable to get DLL version` / `Failed to copy DLL` (G19): fail `E_CLINK_PATH`; no inject since the apply: warn `E_THEME_NOT_LOADED` — open a new cmd window (in mode "WT only": a new cmd tab in Windows Terminal) and check again; for mode "cmd ทุกหน้าต่าง" it adds that if this stays, cmd stops before CMD Studio's part of AutoRun, and says how to fix the value by hand in Registry Editor (ruling P68: the planner's reading accepts a few values cmd rejects; only a real cmd window shows it) |
| D4 | mode "cmd ทุกหน้าต่าง": cmd accepts the AutoRun value (`scan(..).rejected` is false), its first command is our current entry (`autorun_entry`, any ASCII case), and `<clink-launch>` exists | a value cmd rejects: fail `E_AUTORUN_WRITE` with the Registry Editor hand edit; no entry of ours (`is_our_entry`): fail `E_AUTORUN_WRITE`, "apply again"; ours not first, or an older form: fail `E_AUTORUN_WRITE`, "apply again"; `<clink-launch>` gone: fail `E_RUNTIME_MISSING`; mode "WT only": `skipped` |
| D5 | no command in HKCU or HKLM AutoRun that injects another Clink (`runs_clink`, the planner's own reading; our entries do not count) | warn `E_AUTORUN_FOREIGN_CLINK`, with those commands in `details`, HKCU first |
| D6 | Windows Terminal's settings files exist and all parse | a file does not parse: fail `E_WT_PARSE`; WT installed but never opened (no settings.json yet): warn `E_WT_NOT_FOUND`, "open it once, then apply again" (Review Focus 1); WT not installed: warn `E_WT_NOT_FOUND` |
| D7 | each settings file's cmd profile uses the `CMD Studio` scheme, the scheme exists, `commandline` fits the mode (plain `%SystemRoot%\System32\cmd.exe` for "cmd ทุกหน้าต่าง", spec §2.2 lesson 2; the `/k … inject` line for "WT only"), and `defaultProfile` is ours when it was chosen | fail `E_WT_WRITE` |
| D8 | the font face written to WT is among the fonts GDI lists (brief: v1 counts GDI fonts only) | fail `E_FONT_INSTALL` |
| D9 | every file under `generated\` still has the SHA-256 recorded at apply | warn `E_GENERATED_EDITED`; unreadable state.json: fail `E_STATE_INVALID` |
| D10 | the app is not running inside an MSIX package (spec §2.2 lesson 1) | fail `E_APP_PACKAGED` |
| D11 | information only (spec §5.8): which app opens cmd windows started from Run or Start, and whether they get the colors (G7); always `ok`, no code | Windows 10: `skipped` |

- [ ] **Step 1: Write the failing tests** — add `pub mod doctor;` to `crates/core/src/lib.rs` and create `crates/core/src/doctor.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::testing::{machine, request, WT};
    use crate::actions::{prepare_apply, run_apply};
    use crate::adapters::{AutoRun, Fs, RegValue, REG_SZ};
    use crate::presets::terracotta;
    use crate::txn::Outcome;

    fn by_id(checks: &[Check], id: &str) -> Check {
        checks.iter().find(|c| c.id == id).cloned().unwrap()
    }

    #[test]
    fn a_fresh_machine_skips_what_needs_a_theme() {
        let (m, ctx) = machine();
        let checks = doctor(m.system(), &ctx);
        assert_eq!(
            checks.iter().map(|c| c.id).collect::<Vec<_>>(),
            ["D1", "D2", "D3", "D4", "D5", "D6", "D7", "D8", "D9", "D10", "D11"]
        );
        for id in ["D1", "D2", "D3", "D4", "D7", "D8", "D9"] {
            assert_eq!(by_id(&checks, id).status, Status::Skipped, "{id}");
        }
        assert_eq!(by_id(&checks, "D6").status, Status::Ok);
        assert_eq!(by_id(&checks, "D10").status, Status::Ok);
    }

    #[test]
    fn after_a_good_apply_everything_checked_is_ok() {
        let (m, ctx) = machine();
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0\n");
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        put_log(&m, &ctx, "5f50 initialise_clink 237 Inject completed at 2026/10/10 22:39:52.599\n");
        let checks = doctor(m.system(), &ctx);
        for c in &checks {
            assert!(matches!(c.status, Status::Ok | Status::Skipped), "{c:?}");
            if c.status == Status::Ok {
                assert_eq!(c.code, None);
            }
        }
        assert_eq!(by_id(&checks, "D3").status, Status::Ok);
    }

    #[test]
    fn only_a_cmd_window_started_since_the_apply_counts_for_d3() {
        // Ruling P68: AutoRun can stop before our entry in ways the planner can't see; a real
        // cmd window shows it. What Clink logged before the apply says nothing about this theme.
        let (m, ctx) = machine();
        let inject = "5f50 initialise_clink 237 Inject completed at 2026/10/10 22:39:52.599\n";
        put_log(&m, &ctx, inject);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        let d3 = by_id(&doctor(m.system(), &ctx), "D3");
        assert_eq!((d3.status, d3.code), (Status::Warn, Some("E_THEME_NOT_LOADED")));
        assert!(d3.message_en.contains("Open a new cmd window"), "{d3:?}");
        assert!(d3.message_en.contains("Registry Editor"), "{d3:?}");
        // a cmd window started now writes its own record after the old one
        put_log(&m, &ctx, &format!("{inject}{inject}"));
        assert_eq!(by_id(&doctor(m.system(), &ctx), "D3").status, Status::Ok);
        // a log that is shorter than it was at the apply was started over: all of it is new
        put_log(&m, &ctx, "1 x Inject completed\n");
        assert_eq!(by_id(&doctor(m.system(), &ctx), "D3").status, Status::Ok);
        put_log(&m, &ctx, "1 x nothing\n");
        assert_eq!(by_id(&doctor(m.system(), &ctx), "D3").status, Status::Warn);
    }

    #[test]
    fn our_entry_must_come_first_in_autorun() {
        let (m, ctx) = machine();
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0\n");
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        assert_eq!(by_id(&doctor(m.system(), &ctx), "D4").status, Status::Ok);
        let ours = m.autorun.read_user().unwrap().unwrap().text().unwrap();
        // another program put its command in front of ours
        let behind = RegValue::from_text(REG_SZ, &format!("if exist x.bat call x.bat & {ours}"));
        m.autorun.write_user(&behind).unwrap();
        let d4 = by_id(&doctor(m.system(), &ctx), "D4");
        assert_eq!((d4.status, d4.code), (Status::Fail, Some("E_AUTORUN_WRITE")));
        assert!(d4.message_en.contains("not first"), "{d4:?}");
        // a value cmd can't run at all
        let broken = RegValue::from_text(REG_SZ, &format!("{ours} & & echo x"));
        m.autorun.write_user(&broken).unwrap();
        let d4 = by_id(&doctor(m.system(), &ctx), "D4");
        assert_eq!(d4.code, Some("E_AUTORUN_WRITE"));
        assert!(d4.message_en.contains("Registry Editor"), "{d4:?}");
    }

    #[test]
    fn problems_are_reported_with_their_codes() {
        let (m, ctx) = machine();
        m.process.answer("oh-my-posh.exe", "version", 0, "31.4.0\n");
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        put_log(&m, &ctx, "copy_dll 122 Failed to copy DLL to 'C:\\x'\ncheck_dll_version 154 Unable to get DLL version for 'C:\\??\\clink_dll_x64.dll'\n");
        m.autorun.write_user(&RegValue::from_text(REG_SZ, r#""C:\clink\clink.bat" inject --autorun"#)).unwrap();
        m.fs.put(ctx.paths.generated().join("init.lua"), b"-- edited by hand");
        m.fs.put(WT, b"{ broken");
        let checks = doctor(m.system(), &ctx);
        let code = |id| (by_id(&checks, id).status, by_id(&checks, id).code);
        assert_eq!(code("D2"), (Status::Fail, Some("E_RUNTIME_MISSING")));
        assert_eq!(code("D3"), (Status::Fail, Some("E_CLINK_PATH")));
        assert_eq!(code("D4"), (Status::Fail, Some("E_AUTORUN_WRITE")));
        assert_eq!(code("D5"), (Status::Warn, Some("E_AUTORUN_FOREIGN_CLINK")));
        assert_eq!(code("D6"), (Status::Fail, Some("E_WT_PARSE")));
        assert_eq!(code("D9"), (Status::Warn, Some("E_GENERATED_EDITED")));
    }

    #[test]
    fn inject_log_reading_takes_the_last_word() {
        assert_eq!(last_inject(""), InjectSeen::Never);
        assert_eq!(last_inject("x Failed to copy DLL\ny Inject completed at t\n"), InjectSeen::Ok);
        assert!(matches!(
            last_inject("Inject completed\nUnable to get DLL version for 'C:\\??'"),
            InjectSeen::Failed(_)
        ));
    }

    #[test]
    fn json_is_camel_case() {
        let c = check("D1", Status::Ok, None, "ก", "a", "");
        let text = serde_json::to_string(&c).unwrap();
        assert_eq!(text, r#"{"id":"D1","status":"ok","code":null,"messageTh":"ก","messageEn":"a","details":""}"#);
    }

    #[test]
    fn windows_terminal_never_opened_is_a_warning_with_a_way_out() {
        // Review Focus 1: a fresh Windows 11 has WT installed but no settings.json yet.
        let (m, ctx) = machine();
        m.fs.remove_file(std::path::Path::new(WT)).unwrap();
        let d6 = by_id(&doctor(m.system(), &ctx), "D6");
        assert_eq!((d6.status, d6.code), (Status::Warn, Some("E_WT_NOT_FOUND")));
        assert!(d6.message_en.contains("Open it once"), "{d6:?}");
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert!(p.plan.warnings.contains(&crate::plan::Warning::NoWindowsTerminal));
        assert!(!p.ops.iter().any(|o| matches!(o, crate::plan::Op::WtApply { .. })));
        assert!(!m.fs.exists(std::path::Path::new(WT)), "never created");
    }

    #[test]
    fn d11_is_information_only() {
        let (mut m, ctx) = machine();
        m.host.info.default_terminal = DefaultTerminal::Conhost;
        let d11 = by_id(&doctor(m.system(), &ctx), "D11");
        assert_eq!((d11.status, d11.code), (Status::Ok, None));
        assert!(d11.message_en.contains("classic console"), "{d11:?}");
        m.host.info.build = Some(19045);
        assert_eq!(by_id(&doctor(m.system(), &ctx), "D11").status, Status::Skipped);
    }

    /// Writes `clink.log` in the Clink profile, as Clink does when it starts in cmd.
    fn put_log(m: &crate::adapters::fake::FakeMachine, ctx: &Ctx, text: &str) {
        m.fs.put(ctx.paths.clink_profile().join("clink.log"), text);
    }
}
```
`after_a_good_apply_everything_checked_is_ok` writes the line Clink 1.9.34 logged after a good inject (`spikes/clink/tmp/profile/clink.log`); `problems_are_reported_with_their_codes` uses the two G19 failure lines from `docs/spikes/G4-G5-G16-G17-clink.md`.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core -- doctor::tests`
Expected: compile errors (`cannot find function doctor`, …).

- [ ] **Step 3: Implement it above the tests**
```rust
//! Health checks D1–D11 (spec §5.8). Read only. A check says `ok` only after
//! it really looked (spec §2.1 COE); what it could not look at is `skipped`.
use crate::actions::Ctx;
use crate::adapters::{DefaultTerminal, System};
use crate::generate::wt::{CMD_EXE, CMD_PROFILE_GUID, SCHEME_NAME};
use crate::model::Target;
use crate::plan::autorun::{is_our_entry, runs_clink, scan};
use crate::plan::{autorun_entry, ClinkLaunch, Component, OMP_VERSION};
use crate::resources::{check_bundle, sha256_hex};
use crate::state::{Applied, State};
use crate::verify::lua_error_line;
use crate::wt_edit::{check_parses, read_value, WtKey};
use serde::Serialize;
use serde_json::Value;
use std::ffi::OsStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Ok,
    Warn,
    Fail,
    Skipped,
}

/// One check's result (spec §5.9: a stable code, Thai and English text, details).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: &'static str,
    pub status: Status,
    pub code: Option<&'static str>,
    pub message_th: String,
    pub message_en: String,
    pub details: String,
}

fn check(
    id: &'static str,
    status: Status,
    code: Option<&'static str>,
    th: &str,
    en: &str,
    details: impl Into<String>,
) -> Check {
    Check { id, status, code, message_th: th.to_string(), message_en: en.to_string(), details: details.into() }
}

fn no_theme(id: &'static str) -> Check {
    check(id, Status::Skipped, None, "ยังไม่ได้ใช้ธีม", "No theme has been applied yet", "")
}

/// What `clink.log` last says about starting Clink in a cmd window (G19).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InjectSeen {
    Never,
    Ok,
    Failed(String),
}

pub fn last_inject(log: &str) -> InjectSeen {
    let mut seen = InjectSeen::Never;
    for line in log.lines() {
        if line.contains("Inject completed") {
            seen = InjectSeen::Ok;
        } else if line.contains("Unable to get DLL version") || line.contains("Failed to copy DLL") {
            seen = InjectSeen::Failed(line.trim().to_string());
        }
    }
    seen
}

/// Runs D1–D11 in order.
pub fn doctor(sys: System<'_>, ctx: &Ctx) -> Vec<Check> {
    let state = State::load(sys.fs, &ctx.paths.state_file());
    let applied = state.as_ref().ok().and_then(|s| s.applied.clone());
    let a = applied.as_ref();
    vec![
        d1(sys, ctx, a),
        d2(sys, ctx, a),
        d3(sys, ctx, a),
        d4(sys, ctx, a),
        d5(sys, ctx),
        d6(sys),
        d7(sys, a),
        d8(sys, a),
        d9(sys, &state),
        d10(sys),
        d11(sys),
    ]
}

fn d1(sys: System<'_>, ctx: &Ctx, a: Option<&Applied>) -> Check {
    let Some(a) = a else { return no_theme("D1") };
    let bad = |details: String| {
        check(
            "D1",
            Status::Fail,
            Some("E_RUNTIME_MISSING"),
            "ไฟล์ส่วนเสริมหายหรือถูกแก้ ลองใช้ธีมซ้ำ",
            "Add-on files are missing or changed. Apply the theme again",
            details,
        )
    };
    let m = match ctx.resources.manifest(sys.fs) {
        Ok(m) => m,
        Err(e) => return bad(e.to_string()),
    };
    let launch = a.launch.to_clink();
    for (component, dir) in [(Component::Omp, ctx.paths.omp_dir()), (Component::Clink, launch.runtime_dir(&ctx.paths))]
    {
        if let Err(e) = check_bundle(sys.fs, &dir, m.bundle(component)) {
            return bad(e.to_string());
        }
    }
    check("D1", Status::Ok, None, "ไฟล์ส่วนเสริมอยู่ครบและตรงกับที่แนบมา", "The add-on files are all there and unchanged", "")
}

fn d2(sys: System<'_>, ctx: &Ctx, a: Option<&Applied>) -> Check {
    if a.is_none() {
        return no_theme("D2");
    }
    match sys.process.run(&ctx.paths.omp_exe(), &[OsStr::new("version")]) {
        Ok(out) if out.status == 0 && out.stdout.trim() == OMP_VERSION => check(
            "D2",
            Status::Ok,
            None,
            "ตัวสร้าง prompt ทำงานได้",
            "The prompt add-on runs",
            format!("oh-my-posh version: {OMP_VERSION}"),
        ),
        Ok(out) => check(
            "D2",
            Status::Fail,
            Some("E_RUNTIME_MISSING"),
            "ตัวสร้าง prompt ไม่ใช่รุ่นที่แนบมา ลองใช้ธีมซ้ำ",
            "The prompt add-on is not the bundled version. Apply the theme again",
            format!("oh-my-posh version printed {:?} (exit {})", out.stdout.trim(), out.status),
        ),
        Err(e) => check(
            "D2",
            Status::Fail,
            Some("E_RUNTIME_MISSING"),
            "ตัวสร้าง prompt ทำงานไม่ได้ ลองใช้ธีมซ้ำ",
            "The prompt add-on does not run. Apply the theme again",
            e.to_string(),
        ),
    }
}

fn d3(sys: System<'_>, ctx: &Ctx, a: Option<&Applied>) -> Check {
    let Some(a) = a else { return no_theme("D3") };
    let profile = ctx.paths.clink_profile();
    let launch: ClinkLaunch = a.launch.to_clink();
    let args = [OsStr::new("--profile"), profile.as_os_str(), OsStr::new("set"), OsStr::new("clink.logo")];
    let out = match sys.process.run(&launch.exe(), &args) {
        Ok(out) => out,
        Err(e) => {
            return check(
                "D3",
                Status::Fail,
                Some("E_RUNTIME_MISSING"),
                "ส่วนเสริมของ cmd ทำงานไม่ได้ ลองใช้ธีมซ้ำ",
                "The cmd add-on does not run. Apply the theme again",
                e.to_string(),
            )
        }
    };
    if let Some(line) = lua_error_line(&format!("{}\n{}", out.stdout, out.stderr)) {
        return check(
            "D3",
            Status::Fail,
            Some("E_VERIFY_CLINK"),
            "ส่วนเสริมของ cmd โหลดสคริปต์ของธีมไม่ได้ ลองใช้ธีมซ้ำ",
            "The cmd add-on can't load the theme's scripts. Apply the theme again",
            line.trim(),
        );
    }
    let log = sys.fs.read(&profile.join("clink.log")).ok().flatten().unwrap_or_default();
    match last_inject(&log_since(&log, a.clink_log_len)) {
        InjectSeen::Ok => check("D3", Status::Ok, None, "ส่วนเสริมของ cmd โหลดสคริปต์ได้และเปิดใน cmd ได้จริง", "The cmd add-on loads the theme's scripts and starts in cmd", ""),
        InjectSeen::Failed(line) => check("D3", Status::Fail, Some("E_CLINK_PATH"), "ส่วนเสริมของ cmd เปิดจากโฟลเดอร์ของมันไม่ได้ cmd จึงขึ้นข้อความผิดพลาด", "The cmd add-on can't start from its folder, so cmd shows an error", line),
        // Ruling P68: a value cmd refuses whole can pass the planner's reading, and then
        // nothing of AutoRun runs. Only a real cmd window shows that, so it is said here.
        InjectSeen::Never if a.target == Target::All => check("D3", Status::Warn, Some("E_THEME_NOT_LOADED"), "ยังไม่มี cmd หน้าต่างไหนโหลดธีมหลังใช้ธีม เปิด cmd หน้าต่างใหม่แล้วตรวจอีกครั้ง ถ้ายังขึ้นแบบนี้ แปลว่า cmd หยุดก่อนถึงส่วนของ CMD Studio ในการตั้งค่าเริ่มต้นของ cmd ให้แก้ด้วยมือ: เปิด Registry Editor ไปที่ HKEY_CURRENT_USER\\Software\\Microsoft\\Command Processor แก้ค่า AutoRun ให้รันได้ แล้วใช้ธีมซ้ำ", "No cmd window has loaded the theme since it was applied. Open a new cmd window, then check again. If this stays, cmd stops before it reaches CMD Studio's part of its start-up setting. Fix that by hand: in Registry Editor, open HKEY_CURRENT_USER\\Software\\Microsoft\\Command Processor and correct the AutoRun value, then apply the theme again", "clink.log has no inject record since the theme was applied"),
        InjectSeen::Never => check("D3", Status::Warn, Some("E_THEME_NOT_LOADED"), "ยังไม่มีแท็บ cmd ใน Windows Terminal แท็บไหนโหลดธีมหลังใช้ธีม เปิดแท็บ cmd ใหม่ใน Windows Terminal แล้วตรวจอีกครั้ง ถ้ายังขึ้นแบบนี้ ให้ใช้ธีมซ้ำ", "No cmd tab in Windows Terminal has loaded the theme since it was applied. Open a new cmd tab there, then check again. If this stays, apply the theme again", "clink.log has no inject record since the theme was applied"),
    }
}

/// What Clink wrote to `clink.log` after it held `len_at_apply` bytes: the record of every cmd
/// window started since the theme was applied. A log that is shorter now was started over, so
/// all of it is new.
fn log_since(log: &[u8], len_at_apply: u64) -> String {
    let start = usize::try_from(len_at_apply).unwrap_or(usize::MAX);
    String::from_utf8_lossy(log.get(start..).unwrap_or(log)).into_owned()
}

fn d4(sys: System<'_>, ctx: &Ctx, a: Option<&Applied>) -> Check {
    let Some(a) = a else { return no_theme("D4") };
    if a.target == Target::Wt {
        return check(
            "D4",
            Status::Skipped,
            None,
            "ใช้ธีมเฉพาะใน Windows Terminal",
            "The theme is set for Windows Terminal only",
            "",
        );
    }
    let launch = a.launch.to_clink();
    let entry = match autorun_entry(&launch, &ctx.paths) {
        Ok(e) => e,
        Err(e) => {
            return check(
                "D4",
                Status::Fail,
                Some(e.code()),
                "ที่อยู่ของส่วนเสริมใช้กับ cmd ไม่ได้",
                "The add-on's path can't be used by cmd",
                e.to_string(),
            )
        }
    };
    let current = sys.autorun.read_user().ok().flatten().and_then(|v| v.text()).unwrap_or_default();
    let read = scan(&current);
    if read.rejected {
        return check("D4", Status::Fail, Some("E_AUTORUN_WRITE"), "cmd หน้าต่างใหม่จะไม่ได้ธีม เพราะ cmd รันการตั้งค่าเริ่มต้นนี้ไม่ได้เลย ให้แก้ด้วยมือ: เปิด Registry Editor ไปที่ HKEY_CURRENT_USER\\Software\\Microsoft\\Command Processor แก้ค่า AutoRun แล้วใช้ธีมซ้ำ", "New cmd windows won't get the theme: cmd can't run this start-up setting at all. Fix it by hand: in Registry Editor, open HKEY_CURRENT_USER\\Software\\Microsoft\\Command Processor and correct the AutoRun value, then apply the theme again", current);
    }
    let profile = ctx.paths.clink_profile();
    let profile = profile.to_str().unwrap_or_default();
    if !read.parts.iter().any(|p| is_our_entry(&p.text, profile)) {
        return check(
            "D4",
            Status::Fail,
            Some("E_AUTORUN_WRITE"),
            "cmd หน้าต่างใหม่จะไม่ได้ธีม เพราะการตั้งค่าที่เปิดธีมหายไป ลองใช้ธีมซ้ำ",
            "New cmd windows won't get the theme: the setting that loads it is gone. Apply the theme again",
            current,
        );
    }
    // Ours goes first, in parentheses (spec §5.4): anywhere else another program's `if` can
    // keep it from running.
    if !read.parts.first().is_some_and(|p| p.text.eq_ignore_ascii_case(&entry)) {
        return check(
            "D4",
            Status::Fail,
            Some("E_AUTORUN_WRITE"),
            "การตั้งค่าที่เปิดธีมไม่ได้อยู่หน้าสุดหรือเป็นของรุ่นเก่า ลองใช้ธีมซ้ำ",
            "The setting that loads the theme is not first, or is an older one. Apply the theme again",
            current,
        );
    }
    if !sys.fs.exists(&launch.exe()) {
        return check(
            "D4",
            Status::Fail,
            Some("E_RUNTIME_MISSING"),
            "การตั้งค่าชี้ไปที่ส่วนเสริมที่ไม่มีอยู่แล้ว ลองใช้ธีมซ้ำ",
            "The setting points to an add-on that is gone. Apply the theme again",
            launch.exe().display().to_string(),
        );
    }
    check("D4", Status::Ok, None, "cmd ทุกหน้าต่างโหลดธีมตอนเปิด", "Every cmd window loads the theme", "")
}

/// The commands of an AutoRun value that inject a Clink other than CMD Studio's entries
/// (`plan::autorun::runs_clink`, the reading the planner asks the user about), one per line.
fn other_clinks(value: &str, profile: &str) -> String {
    scan(value)
        .parts
        .iter()
        .filter(|p| !is_our_entry(&p.text, profile) && runs_clink(&p.text))
        .map(|p| p.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn d5(sys: System<'_>, ctx: &Ctx) -> Check {
    let user = sys.autorun.read_user().ok().flatten().and_then(|v| v.text()).unwrap_or_default();
    let machine = sys.autorun.read_machine().ok().flatten().and_then(|v| v.text()).unwrap_or_default();
    let profile = ctx.paths.clink_profile();
    let profile = profile.to_str().unwrap_or_default();
    let in_user = other_clinks(&user, profile);
    if !in_user.is_empty() {
        return check(
            "D5",
            Status::Warn,
            Some("E_AUTORUN_FOREIGN_CLINK"),
            "มีส่วนเสริมของ cmd ตัวอื่นตั้งไว้ prompt ของธีมอาจไม่ขึ้น",
            "Another cmd add-on is set up; the theme's prompt may not show",
            in_user,
        );
    }
    let in_machine = other_clinks(&machine, profile);
    if !in_machine.is_empty() {
        return check("D5", Status::Warn, Some("E_AUTORUN_FOREIGN_CLINK"), "มีส่วนเสริมของ cmd ที่ติดตั้งไว้ทั้งเครื่อง prompt ของธีมอาจไม่ทำงาน แต่สีใน Windows Terminal ยังใช้ได้", "A cmd add-on is set up for the whole machine; the theme's prompt may not work, but the colors in Windows Terminal still do", in_machine);
    }
    check("D5", Status::Ok, None, "ไม่มีส่วนเสริมของ cmd ตัวอื่น", "No other cmd add-on is set up", "")
}

fn d6(sys: System<'_>) -> Check {
    let locations = sys.wt.locations();
    let present: Vec<_> = locations.iter().filter(|l| sys.fs.exists(&l.settings)).collect();
    for l in &present {
        let text = sys
            .fs
            .read(&l.settings)
            .ok()
            .flatten()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        if let Err(e) = check_parses(&text) {
            return check(
                "D6",
                Status::Fail,
                Some("E_WT_PARSE"),
                "อ่านการตั้งค่าของ Windows Terminal ไม่ได้",
                "Windows Terminal's settings can't be read",
                format!("{}: {e}", l.settings.display()),
            );
        }
    }
    if present.is_empty() {
        return if locations.iter().any(|l| l.installed) {
            check(
                "D6",
                Status::Warn,
                Some("E_WT_NOT_FOUND"),
                "Windows Terminal ยังไม่เคยเปิด เปิดสักครั้งแล้วใช้ธีมซ้ำ",
                "Windows Terminal has never been opened. Open it once, then apply the theme again",
                "settings.json does not exist yet",
            )
        } else {
            check(
                "D6",
                Status::Warn,
                Some("E_WT_NOT_FOUND"),
                "ไม่พบ Windows Terminal",
                "Windows Terminal is not installed",
                "",
            )
        };
    }
    check(
        "D6",
        Status::Ok,
        None,
        "พบ Windows Terminal และอ่านการตั้งค่าได้",
        "Windows Terminal is installed and its settings can be read",
        format!("{} settings file(s)", present.len()),
    )
}

fn d7(sys: System<'_>, a: Option<&Applied>) -> Check {
    let Some(a) = a else { return no_theme("D7") };
    let files: Vec<_> = sys.wt.locations().into_iter().filter(|l| sys.fs.exists(&l.settings)).collect();
    if files.is_empty() {
        return check("D7", Status::Skipped, None, "ไม่พบ Windows Terminal", "Windows Terminal is not installed", "");
    }
    let bad = |details: String| {
        check(
            "D7",
            Status::Fail,
            Some("E_WT_WRITE"),
            "การตั้งค่า cmd ใน Windows Terminal ไม่ตรงกับธีม ลองใช้ธีมซ้ำ",
            "cmd's settings in Windows Terminal don't match the theme. Apply the theme again",
            details,
        )
    };
    for l in files {
        let text = sys
            .fs
            .read(&l.settings)
            .ok()
            .flatten()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let read = |key: WtKey| read_value(&text, &key).ok().flatten();
        if read(WtKey::Profile { key: "colorScheme".into() }) != Some(Value::String(SCHEME_NAME.into()))
            || read(WtKey::Scheme).is_none()
        {
            return bad(format!("{}: the cmd profile does not use the {SCHEME_NAME} scheme", l.settings.display()));
        }
        let commandline = read(WtKey::Profile { key: "commandline".into() });
        let line_ok = match a.target {
            // Lesson 2: a plain commandline, or Win+R windows lose the colors.
            Target::All => commandline == Some(Value::String(CMD_EXE.into())),
            Target::Wt => {
                commandline.as_ref().and_then(Value::as_str).is_some_and(|c| c.contains(" inject --quiet --profile "))
            }
        };
        if !line_ok {
            return bad(format!("{}: commandline is {commandline:?}", l.settings.display()));
        }
        if a.set_default {
            let default = read(WtKey::DefaultProfile);
            if !default.as_ref().and_then(Value::as_str).is_some_and(|g| g.eq_ignore_ascii_case(CMD_PROFILE_GUID)) {
                return bad(format!("{}: defaultProfile is {default:?}", l.settings.display()));
            }
        }
    }
    check(
        "D7",
        Status::Ok,
        None,
        "หน้าต่าง cmd ใน Windows Terminal ใช้สีของธีม",
        "cmd in Windows Terminal uses the theme's colors",
        "",
    )
}

fn d8(sys: System<'_>, a: Option<&Applied>) -> Check {
    let Some(a) = a else { return no_theme("D8") };
    let Some(face) = &a.wt_font else {
        return check(
            "D8",
            Status::Skipped,
            None,
            "ธีมไม่ได้ตั้งฟอนต์ เพราะไม่มี Windows Terminal",
            "The theme set no font, because Windows Terminal is not installed",
            "",
        );
    };
    // v1 counts fonts GDI lists; fonts packaged inside Windows Terminal are not
    // counted (spec §5.4, decided in Phase 1b). The planner falls back to the
    // bundled font anyway.
    let known = "GDI fonts only; fonts inside Windows Terminal's package are not counted";
    match sys.fonts.families() {
        Ok(f) if f.iter().any(|n| n.eq_ignore_ascii_case(face)) => check(
            "D8",
            Status::Ok,
            None,
            "ฟอนต์ของธีมติดตั้งอยู่",
            "The theme's font is installed",
            format!("{face} ({known})"),
        ),
        Ok(_) => check(
            "D8",
            Status::Fail,
            Some("E_FONT_INSTALL"),
            "ไม่พบฟอนต์ของธีม Windows Terminal จะใช้ฟอนต์อื่นแทนโดยไม่เตือน ลองใช้ธีมซ้ำ",
            "The theme's font is missing, and Windows Terminal silently uses another. Apply the theme again",
            format!("{face} ({known})"),
        ),
        Err(e) => {
            check("D8", Status::Fail, Some("E_FONT_INSTALL"), "ตรวจฟอนต์ไม่ได้", "Could not list the fonts", e.to_string())
        }
    }
}

fn d9(sys: System<'_>, state: &Result<State, crate::state::StateError>) -> Check {
    let s = match state {
        Ok(s) => s,
        Err(e) => {
            return check(
                "D9",
                Status::Fail,
                Some(e.code()),
                "อ่านบันทึกของโปรแกรม (state.json) ไม่ได้",
                "CMD Studio's record (state.json) can't be read",
                e.to_string(),
            )
        }
    };
    let Some(a) = &s.applied else {
        return no_theme("D9");
    };
    for (path, hash) in &a.generated {
        let now = sys.fs.read(path).ok().flatten().map(|b| sha256_hex(&b));
        if now.as_deref() != Some(hash.as_str()) {
            return check(
                "D9",
                Status::Warn,
                Some("E_GENERATED_EDITED"),
                "มีการแก้ไฟล์ที่โปรแกรมสร้าง ใช้ธีมซ้ำเพื่อกลับเป็นค่าของธีม",
                "A file CMD Studio generated was edited. Apply the theme again to restore it",
                path.display().to_string(),
            );
        }
    }
    check("D9", Status::Ok, None, "ไฟล์ที่โปรแกรมสร้างยังเหมือนเดิม", "The files CMD Studio generated are unchanged", "")
}

fn d10(sys: System<'_>) -> Check {
    if sys.host.system_info().packaged {
        return check(
            "D10",
            Status::Fail,
            Some("E_APP_PACKAGED"),
            "โปรแกรมทำงานอยู่ในแพ็กเกจ สิ่งที่โปรแกรมเขียนจะไม่มีผลกับโปรแกรมอื่น",
            "The app runs inside a package, so what it writes is invisible to other programs",
            "spec §2.2 lesson 1",
        );
    }
    check(
        "D10",
        Status::Ok,
        None,
        "โปรแกรมทำงานแบบปกติ ไม่ได้อยู่ในแพ็กเกจ",
        "The app runs as a normal program, not inside a package",
        "",
    )
}

fn d11(sys: System<'_>) -> Check {
    let info = sys.host.system_info();
    if info.build.is_none_or(|b| b < 22000) {
        return check("D11", Status::Skipped, None, "มีเฉพาะใน Windows 11", "Windows 11 only", "");
    }
    // Information only (spec §5.8): which app opens cmd windows started from
    // Run or Start. Only Windows Terminal gives them the theme's colors (G7).
    let wt_installed = sys.wt.locations().iter().any(|l| l.installed);
    let (th, en) = match info.default_terminal {
        DefaultTerminal::WindowsTerminal => ("Windows Terminal", "Windows Terminal"),
        DefaultTerminal::WindowsTerminalPreview => ("Windows Terminal Preview", "Windows Terminal Preview"),
        DefaultTerminal::LetWindowsDecide if wt_installed => {
            ("Windows Terminal (Windows เลือกให้)", "Windows Terminal (chosen by Windows)")
        }
        DefaultTerminal::LetWindowsDecide | DefaultTerminal::Conhost => {
            ("หน้าต่าง cmd แบบเก่า", "the classic console window")
        }
        DefaultTerminal::Other => ("แอปอื่น", "another app"),
    };
    let colors = if th.starts_with("Windows Terminal") {
        "cmd windows opened from Run or Start open in Windows Terminal and get the theme's colors"
    } else {
        "cmd windows opened from Run or Start don't get the theme's colors; the prompt and welcome box still work"
    };
    check(
        "D11",
        Status::Ok,
        None,
        &format!("แอปเทอร์มินัลเริ่มต้นคือ {th}"),
        &format!("The default terminal app is {en}"),
        colors,
    )
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core -- doctor::tests`
Expected: `9 passed`. Then the whole crate: `cargo test -p cmdstudio-core` — every test passes: 323 unit tests (Phase 1a's 217 after Task 0, and 106 from Tasks 1–15), and the integration tests as after Task 0 (`omp_render` comes in Task 17).

- [ ] **Step 5: Bring the spec up to date** (spec §0: the spec must match the shipped code). In `docs/superpowers/specs/2026-10-10-cmd-studio-design.md`:

1. §5.9: append these rows at the end of the table (after `E_TXN_INTERRUPTED`, which Phase 1a added):
```text
| `E_UNSUPPORTED_PLATFORM` | เครื่องนี้ไม่ใช่ Windows แบบ x64 (เช่น ARM64) ส่วนเสริมที่แนบมากับ v1 มีแต่รุ่น x64 |
| `E_DATA_WRITE` | เขียนลงโฟลเดอร์ข้อมูลของโปรแกรมไม่ได้ เช่น ไฟล์ใน `generated\` หรือ `state.json` |
| `E_STATE_INVALID` | อ่าน `state.json` ไม่ได้ เพราะไฟล์เสียหรือถูกแก้ |
| `E_GENERATED_EDITED` | มีคนแก้ไฟล์ใน `generated\` ด้วยมือ (D9) |
| `E_APP_PACKAGED` | แอปรันอยู่ในแพ็กเกจ MSIX สิ่งที่แอปเขียนจึงไม่มีผลกับโปรแกรมอื่น (D10, §2.2 ข้อ 1) |
| `E_THEME_NOT_LOADED` | ใช้ธีมแล้วแต่ยังไม่มีหน้าต่าง cmd ไหนโหลดธีม ถ้าเปิดหน้าต่างใหม่แล้วยังเป็นแบบนี้ แปลว่า cmd หยุดก่อนถึงส่วนของเราใน AutoRun ต้องแก้ค่า AutoRun ด้วยมือ (D3) |
```
2. §5.4 "ฟอนต์ (`Fonts`)", replace the line
```text
  - ตัวสำรองตาม §3.3 คือ CaskaydiaCove NF ที่แนบมา (founder 2026-10-11) ส่วนวิธีที่ D8 นับฟอนต์ในแพ็กเกจของ WT ยังต้องตัดสินใน phase 1b ⬜
```
with
```text
  - ตัวสำรองตาม §3.3 คือ CaskaydiaCove NF ที่แนบมา (founder 2026-10-11) · D8 ใน v1 นับเฉพาะฟอนต์ที่ GDI เห็น ไม่นับฟอนต์ในแพ็กเกจของ WT (ตัดสินใน phase 1b) เมื่อ GDI ไม่เห็นฟอนต์ของธีม planner จึงใช้ตัวสำรองเสมอ
```
3. §5.8, replace the D3, D4, D6 and D8 rows with
```text
| D3 | Clink โหลดสคริปต์ของเราได้ (ตรวจจาก output ของ `clink set` ตาม G5) และ inject ได้จริงจาก path ของ runtime (หลังเปิด cmd ใน `clink.log` ต้องไม่มี `Unable to get DLL version` หรือ `Failed to copy DLL` ตาม G19) · อ่านเฉพาะส่วนของ `clink.log` ที่เขียนหลังใช้ธีม ถ้ายังไม่มีหน้าต่าง cmd ไหน inject ให้เตือน `E_THEME_NOT_LOADED` พร้อมบอกให้เปิดหน้าต่างใหม่ และถ้ายังเป็นอยู่ให้แก้ AutoRun ด้วยมือ (phase 1a: ตัวอ่าน AutoRun ของ planner ยอมรับค่าบางแบบที่ cmd ไม่รัน มีแต่หน้าต่าง cmd จริงที่บอกได้) |
| D4 | AutoRun มีส่วนของเราที่มี guard อยู่หน้าสุด และชี้ไปที่ runtime ที่มีอยู่จริง (ตรวจเมื่ออยู่ในโหมด "cmd ทุกหน้าต่าง") · ค่าที่ cmd รันไม่ได้ทั้งค่าให้แจ้ง `E_AUTORUN_WRITE` พร้อมวิธีแก้ด้วยมือ |
| D6 | มี WT ติดตั้งอยู่ และ `settings.json` ของทุกรุ่น parse ได้ · WT ที่ติดตั้งแล้วแต่ยังไม่เคยเปิดจะยังไม่มี `settings.json` ให้เตือนว่าเปิด WT หนึ่งครั้งแล้วใช้ธีมซ้ำ (CMD Studio ไม่สร้างไฟล์นี้เอง) |
| D8 | ฟอนต์ที่ธีมต้องใช้ติดตั้งอยู่ ข้อนี้สำคัญเพราะ WT ไม่เตือนเองเมื่อหาฟอนต์ไม่เจอ (G12) · นับเฉพาะฟอนต์ที่ GDI เห็น ไม่นับฟอนต์ในแพ็กเกจของ WT (§5.4) |
```
4. §5.3, in the folder tree, replace the line
```text
├─ snapshots\<id>\     manifest.json, journal.json, payload\
```
with
```text
├─ snapshots\<id>\     manifest.json, journal.json, payload\ และ incomplete.json เมื่อย้อนกลับไม่ครบ
```
and in §5.5 replace the line
```text
- **ถ้าย้อนกลับไม่ครบ (`rollbackIncomplete`):** ต้องแจ้งชัดว่าส่วนไหนคืนไม่ได้ พร้อมวิธีแก้ด้วยมือ ห้ามรายงานว่าสำเร็จ
```
with
```text
- **ถ้าย้อนกลับไม่ครบ (`rollbackIncomplete`):** ต้องแจ้งชัดว่าส่วนไหนคืนไม่ได้ พร้อมวิธีแก้ด้วยมือ ห้ามรายงานว่าสำเร็จ · รายการนี้และวิธีแก้เก็บใน `snapshots\<id>\incomplete.json` ข้าง journal (ไม่เก็บใน `state.json` เพราะ `state.json` เองก็อาจเป็นไฟล์ที่คืนไม่ได้) จึงยังแจ้งได้หลังเปิดโปรแกรมใหม่ จนกว่าจะมีทรานแซกชันใหม่ (phase 1b)
```
5. Appendix B: append the row
```text
| 2026-10-12 | ตัดสินระหว่างวางแผน phase 1b (architect เลือก founder เปลี่ยนได้):<br>• D8 นับเฉพาะฟอนต์ที่ GDI เห็น (§5.4, §5.8)<br>• WT ที่ยังไม่เคยเปิดไม่มี `settings.json`: CMD Studio ไม่สร้างไฟล์เอง แต่เตือนให้เปิด WT หนึ่งครั้งแล้วใช้ธีมซ้ำ (D6)<br>• เครื่อง ARM64 ใช้ธีมไม่ได้ (`E_UNSUPPORTED_PLATFORM`) เพราะส่วนเสริมที่แนบมามีแต่รุ่น x64<br>• D3 อ่าน `clink.log` เฉพาะส่วนที่เขียนหลังใช้ธีม และเตือน `E_THEME_NOT_LOADED` จนกว่าจะมีหน้าต่าง cmd ที่ inject ได้ D4 ตรวจว่าส่วนของเราอยู่หน้าสุดของ AutoRun (phase 1a ruling P68) และ D11 เป็นข้อมูลอย่างเดียว<br>• ย้อนกลับไม่ครบแล้วบันทึกรายการกับวิธีแก้ไว้ใน `snapshots\<id>\incomplete.json` (§5.3, §5.5)<br>• เพิ่มรหัส `E_DATA_WRITE`, `E_STATE_INVALID`, `E_GENERATED_EDITED`, `E_APP_PACKAGED`, `E_THEME_NOT_LOADED` ใน §5.9 |
```
Check that the tables still render: every new row has the same number of `|` as its neighbours, and nothing else in the file changed (`git diff --stat` shows only the spec, `doctor.rs` and `lib.rs`).

- [ ] **Step 6: Format and lint**

Run: `cargo fmt --all`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 7: Commit**

Message:
```text
feat(core): add doctor checks D1-D11

Each check reports ok only after a real look, with a stable code and
Thai and English text. D3 also reads clink.log for the G19 inject
failures, and only what was logged since the apply: no cmd window
injected since is E_THEME_NOT_LOADED, with the AutoRun hand edit
(ruling P68). D4 needs our entry first in an AutoRun cmd accepts; D5
reads AutoRun with the planner's runs_clink. D6 warns when Windows
Terminal was never opened; D8 counts GDI fonts only; D11 is
information only.

The spec now lists E_UNSUPPORTED_PLATFORM, E_DATA_WRITE,
E_STATE_INVALID, E_GENERATED_EDITED, E_APP_PACKAGED and
E_THEME_NOT_LOADED (5.9), records the D3, D4, D6 and D8 decisions
(5.4, 5.8) and incomplete.json (5.3, 5.5), and logs them in appendix B.
```
Run:
```bash
git add -- crates/core/src/lib.rs crates/core/src/doctor.rs docs/superpowers/specs/2026-10-10-cmd-studio-design.md
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 16: The `cmdstudio` command line

**Files:**
- Modify: `Cargo.toml` (workspace member `crates/cli`), `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§5.7, appendix B)
- Create: `crates/cli/Cargo.toml`, `crates/cli/src/args.rs`, `crates/cli/src/lib.rs`, `crates/cli/src/main.rs`

**Interfaces:**
- Consumes:
  - Task 1: `adapters::{System, Host, program_data_dir, AutoRun, Fs, RegValue, WtLocation, WtVariant, REG_SZ}`, `adapters::fake::FakeMachine` (through the core's `fake` feature). Tasks 3–7 and 12: `adapters::windows::{fs::WinFs, registry::WinAutoRun::real(), host::WinHost, fonts::WinFonts::real(&Path), wt::WinWt::new(PathBuf), process::WinProcess::new(Vec<PathBuf>, Duration)}`.
  - Task 2: `resources::{Resources, fake_resources}`. Task 9: `snapshot::Incomplete` (`steps: Vec<Unrestored { target, code, message, fix, .. }>`). Task 11: `exec::op_label`. Task 13: `actions::{pending_incomplete, prepare_apply, recover_at_start, run_apply, undo, ActionError, Ctx, StartReport, UndoResult}`. Task 14: `cleanup::cleanup` and `Report { outcome, steps }`, `Report::ok()`. Task 15: `doctor::{doctor, Status}`.
  - Phase 1a: `generate::lua::Lang`, `model::{parse_theme, Target}`, `ThemeError::code()`, `plan::{Op, Paths, PlanError, Request, Warning, NERD_FONT_FAMILY}` (`PlanError::ForeignClink { existing }`, `PlanError::AutoRunNeedsHandEdit { part }`), `txn::{Failure, Outcome}` (`RollbackIncomplete { code, reason, failed: Vec<(usize, Failure)> }`), and the file `presets/terracotta.cmdtheme`.
- Produces (crate `cmdstudio-cli`, library `cmdstudio_cli`, binary `cmdstudio`):
  - `EXIT_OK = 0`, `EXIT_FAILED = 1`, `EXIT_INVALID = 2`, `EXIT_INCOMPLETE = 3`.
  - `Io<'a> { out: &'a mut dyn Write, err: &'a mut dyn Write }`; `Defaults { data_dir: PathBuf, resources: PathBuf, now_ms: u64 }` (`Clone + Debug`).
  - `run_with(args: &[OsString], io: &mut Io<'_>, sys: System<'_>, defaults: &Defaults) -> i32` — everything except building the real adapters; `run(args: &[OsString], io: &mut Io<'_>) -> i32` (Windows only) — builds them; `now_ms() -> u64`.
  - `args::{parse(&[OsString]) -> Result<Args, UsageError>, Args { command, data_dir, resources }, Command::{Apply { theme, target, replace_other_clink, lang }, Doctor { json }, Undo, Cleanup { remove_themes }, Help}, UsageError, USAGE}`.
  - Phase 2's app links `cmdstudio_cli` and calls `run` when started with arguments (spec §5.7: one exe).

**Behaviour** (spec §5.7, brief):
- `apply <file> [--target all|wt] [--replace-other-clink] [--lang th|en]`: reads and validates the theme (`E_THEME_INVALID` → exit 2), prints the plan summary (English) and warnings, then applies. Another program's Clink in AutoRun stops the apply with exit 2: stderr lists every command that turning it off would take out, one per line, and the hint to re-run with `--replace-other-clink` (spec §5.4: the user sees all of them before agreeing). An AutoRun value CMD Studio cannot change safely (`E_AUTORUN_WRITE`) stops it with exit 1: stderr shows the command at fault and how to fix it in Registry Editor; nothing was changed. The banner language defaults to Thai when Windows' UI language is Thai (`0x041E`), else English. The cmd profile becomes WT's default, like the wizard's checkbox that is ticked by default (spec §4.2 page 4).
- `doctor [--json]`: the 11 checks as text, or `{"checks": [...]}` with camelCase fields; exit 1 when any check is `fail`.
- `undo`: "Nothing to undo." or the outcome. `--cleanup [--remove-themes]`: one line per step, then the outcome.
- Every command first finishes a transaction a crash cut off: a note `E_TXN_INTERRUPTED: <reason>; the machine is as it was before it.` on stderr; if that rollback is incomplete it stops with exit 3 and lists what to fix. A rollback that did not finish earlier (the newest restore point has `incomplete.json`) is printed as a note before every command, until a newer transaction runs; the command still runs, so `--cleanup` can fix it.
- Exit codes: 0 done; 1 failed and everything rolled back (or nothing was touched); 2 invalid command line, theme file, or another Clink without the flag; 3 a rollback could not restore everything — stderr says `E_ROLLBACK_INCOMPLETE: not everything could be put back. Fix these by hand:` and, for each target, `  could not put back: <target> (<code>: <message>)` and `    fix: <how>` from the core's record (Task 13); without a record, the targets come from the outcome and the fix is to close every cmd window and Windows Terminal, then run `cmdstudio --cleanup`.
- `--data-dir <dir>` and `--resources <dir>` go anywhere on the line; the defaults are `%LOCALAPPDATA%\ai.resincore.cmdstudio` and `<exe folder>\resources`. `run` allows only programs under `<data>\runtime` and the user's ProgramData folder, with a 60-second limit each.

- [ ] **Step 1: Create the crate and write the failing tests.** Add the member to the root `Cargo.toml`:
```toml
members = ["crates/core", "crates/cli"]
```
Create `crates/cli/Cargo.toml`:
```toml
[package]
name = "cmdstudio-cli"
version.workspace = true
edition.workspace = true
license.workspace = true

[lints]
workspace = true

[lib]
name = "cmdstudio_cli"

[[bin]]
name = "cmdstudio"
path = "src/main.rs"

[dependencies]
cmdstudio-core = { path = "../core" }
serde_json = "1"
thiserror = "2"

[dev-dependencies]
cmdstudio-core = { path = "../core", features = ["fake"] }
tempfile = "3"
```
Create `crates/cli/src/main.rs`:
```rust
//! `cmdstudio.exe`: see `cmdstudio_cli::run`.
#[cfg(windows)]
fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
    let code = cmdstudio_cli::run(&args, &mut cmdstudio_cli::Io { out: &mut out, err: &mut err });
    std::process::exit(code);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("CMD Studio runs on Windows only.");
    std::process::exit(cmdstudio_cli::EXIT_INVALID);
}
```
Create `crates/cli/src/args.rs` with only:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &[&str]) -> Result<Args, UsageError> {
        parse(&s.iter().map(OsString::from).collect::<Vec<_>>())
    }

    #[test]
    fn the_four_commands_parse() {
        assert_eq!(
            p(&["apply", r"C:\t\ธีม ของฉัน.cmdtheme"]).unwrap().command,
            Command::Apply {
                theme: PathBuf::from(r"C:\t\ธีม ของฉัน.cmdtheme"),
                target: Target::All,
                replace_other_clink: false,
                lang: None
            }
        );
        assert_eq!(
            p(&["apply", "x.cmdtheme", "--target", "wt", "--replace-other-clink", "--lang", "th"]).unwrap().command,
            Command::Apply {
                theme: PathBuf::from("x.cmdtheme"),
                target: Target::Wt,
                replace_other_clink: true,
                lang: Some(Lang::Th)
            }
        );
        assert_eq!(p(&["doctor", "--json"]).unwrap().command, Command::Doctor { json: true });
        assert_eq!(p(&["undo"]).unwrap().command, Command::Undo);
        assert_eq!(p(&["--cleanup", "--remove-themes"]).unwrap().command, Command::Cleanup { remove_themes: true });
        assert_eq!(p(&["--help"]).unwrap().command, Command::Help);
    }

    #[test]
    fn global_options_go_anywhere() {
        let a = p(&["--data-dir", r"C:\d", "doctor", "--resources", r"C:\r"]).unwrap();
        assert_eq!(a.data_dir, Some(PathBuf::from(r"C:\d")));
        assert_eq!(a.resources, Some(PathBuf::from(r"C:\r")));
    }

    #[test]
    fn mistakes_are_usage_errors() {
        for bad in [
            vec![],
            vec!["apply"],
            vec!["apply", "a", "b"],
            vec!["apply", "a", "--target", "everywhere"],
            vec!["apply", "a", "--target"],
            vec!["doctor", "--fast"],
            vec!["doctor", "--target", "wt"],
            vec!["undo", "now"],
            vec!["--cleanup", "undo"],
            vec!["remove"],
        ] {
            assert!(p(&bad).is_err(), "{bad:?}");
        }
    }
}
```
and `crates/cli/src/lib.rs` with only `pub mod args;` and:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::adapters::{AutoRun, Fs, RegValue, WtLocation, WtVariant, REG_SZ};
    use std::path::Path;

    const WT: &str =
        r"C:\Users\tester\AppData\Local\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json";
    const THEME: &str = r"C:\themes\terracotta.cmdtheme";

    fn machine() -> (FakeMachine, Defaults) {
        let mut m = FakeMachine::new();
        m.fs.create_dir_all(Path::new(r"C:\Users\tester\AppData\Local\Microsoft\Windows")).unwrap();
        m.fs.put(WT, "{\n    \"profiles\": { \"list\": [] }\n}\n");
        m.fs.put(THEME, include_bytes!("../../../presets/terracotta.cmdtheme"));
        m.wt.locations = vec![WtLocation { variant: WtVariant::Store, installed: true, settings: PathBuf::from(WT) }];
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0");
        let res = PathBuf::from(r"C:\Program Files\CMD Studio\resources");
        cmdstudio_core::resources::fake_resources(&m.fs, &res);
        let d = Defaults {
            data_dir: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio"),
            resources: res,
            now_ms: 1_791_694_709_000,
        };
        (m, d)
    }

    fn cli(m: &FakeMachine, d: &Defaults, args: &[&str]) -> (i32, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let code = run_with(&args, &mut Io { out: &mut out, err: &mut err }, m.system(), d);
        (code, String::from_utf8_lossy(&out).into_owned(), String::from_utf8_lossy(&err).into_owned())
    }

    fn later(d: &Defaults, ms: u64) -> Defaults {
        Defaults { now_ms: d.now_ms + ms, ..d.clone() }
    }

    #[test]
    fn apply_doctor_undo_round_trip() {
        let (m, d) = machine();
        let before = (m.fs.file(WT), m.autorun.snapshot());
        let (code, out, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(out.contains("Plan for \"Terracotta\":\n  - "), "{out}");
        let (code, out, _) = cli(&m, &later(&d, 1), &["doctor", "--json"]);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["checks"].as_array().unwrap().len(), 11);
        assert!(v["checks"][0].get("messageTh").is_some());
        assert_eq!(code, EXIT_OK, "{out}");
        let (code, out, err) = cli(&m, &later(&d, 2), &["undo"]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert_eq!((m.fs.file(WT), m.autorun.snapshot()), before);
        assert_eq!(cli(&m, &later(&d, 3), &["undo"]).1, "Nothing to undo.\n");
    }

    #[test]
    fn invalid_input_exits_2() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply"]).0, EXIT_INVALID);
        assert_eq!(cli(&m, &d, &["apply", r"C:\nope.cmdtheme"]).0, EXIT_INVALID);
        m.fs.put(THEME, b"not json");
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_INVALID);
        assert!(err.starts_with("E_THEME_INVALID"), "{err}");
    }

    #[test]
    fn a_foreign_clink_needs_the_flag() {
        let (m, d) = machine();
        let other = r#""C:\clink\clink.bat" inject --autorun"#;
        m.autorun.write_user(&RegValue::from_text(REG_SZ, other)).unwrap();
        let before = m.autorun.snapshot();
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_INVALID);
        assert!(err.starts_with("E_AUTORUN_FOREIGN_CLINK: "), "{err}");
        assert!(err.contains(&format!("\n  {other}\n")), "{err}");
        assert!(err.contains("--replace-other-clink"), "{err}");
        assert_eq!(m.autorun.snapshot(), before);
        assert_eq!(cli(&m, &d, &["apply", THEME, "--replace-other-clink"]).0, EXIT_OK);
    }

    #[test]
    fn an_autorun_cmd_rejects_needs_a_hand_edit_and_nothing_changes() {
        let (m, d) = machine();
        m.autorun.write_user(&RegValue::from_text(REG_SZ, "echo A |")).unwrap();
        let before = (m.fs.file(WT), m.autorun.snapshot());
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_FAILED, "{err}");
        assert!(err.starts_with("E_AUTORUN_WRITE: "), "{err}");
        assert!(err.contains("\n  echo A |\n"), "{err}");
        assert!(err.contains("Registry Editor"), "{err}");
        assert_eq!((m.fs.file(WT), m.autorun.snapshot()), before);
    }

    #[test]
    fn a_failed_verify_exits_1_and_a_failed_rollback_exits_3() {
        let (m, d) = machine();
        m.process.answer("oh-my-posh.exe", "print", 0, " CONFIG PARSE ERROR ");
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_FAILED);
        assert!(err.starts_with("E_VERIFY_OMP"), "{err}");

        let (m, d) = machine();
        let init = d.data_dir.join(r"generated\init.lua");
        m.fs.put(&init, b"old");
        m.fs.fail_writes_to(&init);
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_INCOMPLETE, "{err}");
        let target = format!("could not put back: file {} (E_DATA_WRITE: ", init.display());
        assert!(err.contains(&target), "{err}");
        let fix = format!(r"    fix: Copy {}\snapshots\", d.data_dir.display());
        assert!(err.contains(&fix), "{err}");
        assert!(err.contains(&format!(" over {}.\n", init.display())), "{err}");
        // The next run, even in a new process, still says what to fix.
        let (_, _, err) = cli(&m, &later(&d, 1), &["doctor"]);
        assert!(err.starts_with("Note: the rollback of restore point "), "{err}");
        assert!(err.contains(&target), "{err}");
    }

    #[test]
    fn every_command_first_rolls_back_a_run_a_crash_cut_off() {
        let (m, d) = machine();
        // What a crash leaves right after an apply started its journal.
        m.fs.put(
            d.data_dir.join(r"snapshots\0000000000001-aaaaaaaa\journal.json"),
            r#"{"id": "0000000000001-aaaaaaaa", "ops": [], "snapshots": [], "stage": {"state": "planned"}}"#,
        );
        let (code, _, err) = cli(&m, &d, &["doctor"]);
        assert_eq!(code, EXIT_OK, "{err}");
        assert_eq!(
            err,
            "E_TXN_INTERRUPTED: the previous apply was interrupted before it changed anything; \
             the machine is as it was before it.\n"
        );
        // Once only.
        assert_eq!(cli(&m, &later(&d, 1), &["doctor"]).2, "");
    }

    #[test]
    fn cleanup_and_usage() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        let (code, out, _) = cli(&m, &later(&d, 1), &["--cleanup"]);
        assert_eq!(code, EXIT_OK, "{out}");
        assert_eq!(m.autorun.read_user().unwrap(), None);
        let (code, out, _) = cli(&m, &d, &["--help"]);
        assert_eq!(code, EXIT_OK);
        assert!(out.starts_with("Usage:"));
        assert_eq!(cli(&m, &d, &["frobnicate"]).0, EXIT_INVALID);
    }
}
```
The CLI tests run on the fake machine (the core's `fake` feature is a dev-dependency only, so release builds never contain it) and read the real `presets/terracotta.cmdtheme`. `every_command_first_rolls_back_a_run_a_crash_cut_off` writes the journal a crash leaves right after an apply started (`{"state": "planned"}`, Phase 1a's camelCase journal), so the note it expects carries Task 0's wording for a run that had not changed anything yet.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-cli`
Expected: compile errors (`cannot find function parse`, `cannot find function run_with`, …).

- [ ] **Step 3: Implement `args.rs` above its tests**
```rust
//! Command-line parsing (spec §5.7). Hand-written so every mistake maps to
//! exit code 2 with the usage text.
use cmdstudio_core::generate::lua::Lang;
use cmdstudio_core::model::Target;
use std::ffi::OsString;
use std::path::PathBuf;

pub const USAGE: &str = "\
Usage:
  cmdstudio apply <file.cmdtheme> [--target all|wt] [--replace-other-clink] [--lang th|en]
  cmdstudio doctor [--json]
  cmdstudio undo
  cmdstudio --cleanup [--remove-themes]

Options for every command:
  --data-dir <dir>    use this data folder instead of %LOCALAPPDATA%\\ai.resincore.cmdstudio
  --resources <dir>   use these bundled files instead of <exe folder>\\resources

Exit codes: 0 done, 1 failed and rolled back, 2 invalid input, 3 rollback incomplete.
";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Apply { theme: PathBuf, target: Target, replace_other_clink: bool, lang: Option<Lang> },
    Doctor { json: bool },
    Undo,
    Cleanup { remove_themes: bool },
    Help,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Args {
    pub command: Command,
    pub data_dir: Option<PathBuf>,
    pub resources: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct UsageError(pub String);

fn bad(msg: impl Into<String>) -> UsageError {
    UsageError(msg.into())
}

/// Parses the arguments after the program name.
pub fn parse(args: &[OsString]) -> Result<Args, UsageError> {
    let mut data_dir = None;
    let mut resources = None;
    let mut words: Vec<OsString> = Vec::new();
    let mut flags: Vec<String> = Vec::new();
    let mut values: Vec<(String, OsString)> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.to_str() {
            Some(opt @ ("--data-dir" | "--resources" | "--target" | "--lang")) => {
                let value = it.next().ok_or_else(|| bad(format!("{opt} needs a value")))?.clone();
                match opt {
                    "--data-dir" => data_dir = Some(PathBuf::from(value)),
                    "--resources" => resources = Some(PathBuf::from(value)),
                    _ => values.push((opt.to_string(), value)),
                }
            }
            Some(flag) if flag.starts_with('-') => flags.push(flag.to_string()),
            _ => words.push(arg.clone()),
        }
    }
    let take_flag = |flags: &mut Vec<String>, name: &str| -> bool {
        let before = flags.len();
        flags.retain(|f| f != name);
        flags.len() != before
    };
    let command = if take_flag(&mut flags, "--help") | take_flag(&mut flags, "-h") {
        Command::Help
    } else if take_flag(&mut flags, "--cleanup") {
        if !words.is_empty() {
            return Err(bad("--cleanup takes no command"));
        }
        Command::Cleanup { remove_themes: take_flag(&mut flags, "--remove-themes") }
    } else {
        let (first, rest) = words.split_first().ok_or_else(|| bad("no command given"))?;
        match first.to_str() {
            Some("apply") => {
                let [theme] = rest else {
                    return Err(bad("apply needs exactly one theme file"));
                };
                let mut target = Target::All;
                let mut lang = None;
                for (name, value) in values.drain(..) {
                    match (name.as_str(), value.to_str()) {
                        ("--target", Some("all")) => target = Target::All,
                        ("--target", Some("wt")) => target = Target::Wt,
                        ("--lang", Some("th")) => lang = Some(Lang::Th),
                        ("--lang", Some("en")) => lang = Some(Lang::En),
                        (name, v) => return Err(bad(format!("{name} does not take {v:?}"))),
                    }
                }
                Command::Apply {
                    theme: PathBuf::from(theme),
                    target,
                    replace_other_clink: take_flag(&mut flags, "--replace-other-clink"),
                    lang,
                }
            }
            Some("doctor") if rest.is_empty() => Command::Doctor { json: take_flag(&mut flags, "--json") },
            Some("undo") if rest.is_empty() => Command::Undo,
            _ => return Err(bad(format!("unknown command {:?}", first))),
        }
    };
    if let Some(f) = flags.first() {
        return Err(bad(format!("unknown option {f}")));
    }
    if let Some((name, _)) = values.first() {
        return Err(bad(format!("{name} does not belong to this command")));
    }
    Ok(Args { command, data_dir, resources })
}
```

- [ ] **Step 4: Implement `lib.rs`:** replace everything above its tests with:
```rust
//! The `cmdstudio` command line (spec §5.7): `apply`, `doctor`, `undo` and
//! `--cleanup`. The Phase 2 app links this library and calls `run` when it is
//! started with arguments, so the app and the CLI are one exe.
pub mod args;

use args::{parse, Args, Command, USAGE};
use cmdstudio_core::actions::{
    pending_incomplete, prepare_apply, recover_at_start, run_apply, undo, ActionError, Ctx, UndoResult,
};
use cmdstudio_core::adapters::System;
use cmdstudio_core::cleanup::cleanup;
use cmdstudio_core::doctor::{doctor, Status};
use cmdstudio_core::exec::op_label;
use cmdstudio_core::generate::lua::Lang;
use cmdstudio_core::model::parse_theme;
use cmdstudio_core::plan::{Op, Paths, PlanError, Request, Warning, NERD_FONT_FAMILY};
use cmdstudio_core::resources::Resources;
use cmdstudio_core::snapshot::Incomplete;
use cmdstudio_core::txn::{Failure, Outcome};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

/// Done.
pub const EXIT_OK: i32 = 0;
/// Failed, and everything was rolled back (or nothing was touched).
pub const EXIT_FAILED: i32 = 1;
/// The command line or the theme file is not valid.
pub const EXIT_INVALID: i32 = 2;
/// A rollback could not restore everything.
pub const EXIT_INCOMPLETE: i32 = 3;

pub struct Io<'a> {
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
}

/// Where things are when no `--data-dir` / `--resources` is given, and the clock.
#[derive(Clone, Debug)]
pub struct Defaults {
    pub data_dir: PathBuf,
    pub resources: PathBuf,
    pub now_ms: u64,
}

/// Runs one command on the machine `sys` and returns the exit code.
pub fn run_with(args: &[OsString], io: &mut Io<'_>, sys: System<'_>, defaults: &Defaults) -> i32 {
    let parsed = match parse(args) {
        Ok(a) => a,
        Err(e) => {
            let _ = write!(io.err, "cmdstudio: {e}\n\n{USAGE}");
            return EXIT_INVALID;
        }
    };
    let Args { command, data_dir, resources } = parsed;
    if command == Command::Help {
        let _ = write!(io.out, "{USAGE}");
        return EXIT_OK;
    }
    let ctx = Ctx {
        paths: Paths { data: data_dir.unwrap_or_else(|| defaults.data_dir.clone()) },
        resources: Resources { root: resources.unwrap_or_else(|| defaults.resources.clone()) },
        now_ms: defaults.now_ms,
    };
    let start = recover_at_start(sys, &ctx);
    match &start.recovered {
        Some(Outcome::RolledBack { code, reason }) => {
            let _ = writeln!(io.err, "{code}: {reason}; the machine is as it was before it.");
        }
        Some(Outcome::RollbackIncomplete { code, reason, failed }) => {
            let _ = writeln!(io.err, "{code}: {reason}.");
            tell_unrestored(io, start.incomplete.as_ref(), failed, &[]);
            return EXIT_INCOMPLETE;
        }
        Some(Outcome::Committed) | None => {
            // A rollback that did not finish stays news until something newer runs.
            if let Some(record) = &start.incomplete {
                let _ = writeln!(io.err, "Note: the rollback of restore point {} did not finish.", record.id);
                tell_unrestored(io, Some(record), &[], &[]);
            }
        }
    }
    match command {
        Command::Apply { theme, target, replace_other_clink, lang } => {
            let lang = lang.unwrap_or(if sys.host.system_info().ui_language == 0x041E { Lang::Th } else { Lang::En });
            let req =
                Request { target, set_wt_default_profile: true, lang, replace_foreign_clink: replace_other_clink };
            apply(io, sys, &ctx, &theme, &req)
        }
        Command::Doctor { json } => run_doctor(io, sys, &ctx, json),
        Command::Undo => match undo(sys, &ctx) {
            Ok(UndoResult::NothingToUndo) => {
                let _ = writeln!(io.out, "Nothing to undo.");
                EXIT_OK
            }
            Ok(UndoResult::Done { undid, outcome }) => finish(io, sys, &ctx, &outcome, &[], &format!("Undid {undid}.")),
            Err(e) => error(io, &e),
        },
        Command::Cleanup { remove_themes } => match cleanup(sys, &ctx, remove_themes) {
            Ok(report) => {
                for s in &report.steps {
                    let _ = writeln!(io.out, "{} {}: {}", if s.ok { "ok  " } else { "FAIL" }, s.what, s.detail);
                }
                match &report.outcome {
                    Some(o @ (Outcome::RolledBack { .. } | Outcome::RollbackIncomplete { .. })) => {
                        finish(io, sys, &ctx, o, &[], "")
                    }
                    _ if !report.ok() => EXIT_INCOMPLETE,
                    _ => {
                        let _ = writeln!(io.out, "CMD Studio's changes are removed.");
                        EXIT_OK
                    }
                }
            }
            Err(e) => error(io, &e),
        },
        Command::Help => EXIT_OK,
    }
}

fn error(io: &mut Io<'_>, e: &ActionError) -> i32 {
    let code = e.code();
    match e {
        ActionError::Plan(PlanError::ForeignClink { existing }) => {
            let _ = writeln!(io.err, "{code}: another Clink starts with cmd, from AutoRun:");
            for command in existing {
                let _ = writeln!(io.err, "  {command}");
            }
            let _ = writeln!(
                io.err,
                "Nothing was changed. Run again with --replace-other-clink to turn it off (it can be undone)."
            );
            EXIT_INVALID
        }
        ActionError::Plan(PlanError::AutoRunNeedsHandEdit { part }) => {
            let _ = writeln!(io.err, "{code}: CMD Studio can't safely change AutoRun because of this command:");
            let _ = writeln!(io.err, "  {part}");
            let _ = writeln!(io.err, "Nothing was changed. Fix it by hand: in Registry Editor, open {COMMAND_PROCESSOR_KEY} and correct or take out that command in the AutoRun value. Then run this again.");
            EXIT_FAILED
        }
        _ => {
            let _ = writeln!(io.err, "{code}: {e}");
            EXIT_FAILED
        }
    }
}

/// The registry key that holds AutoRun, as Registry Editor shows it.
const COMMAND_PROCESSOR_KEY: &str = r"HKEY_CURRENT_USER\Software\Microsoft\Command Processor";

/// Said when a rollback could not put something back and nothing better is known.
const CLEANUP_FIX: &str = "Close every cmd window and Windows Terminal, then run `cmdstudio --cleanup`, which takes out what CMD Studio changed.";

/// Tells the user what a rollback could not put back, and how to fix each by hand (spec §5.5).
/// `record` is what the core kept of that rollback; without one, the targets come from `failed`
/// and `ops`, and the fix is a cleanup.
fn tell_unrestored(io: &mut Io<'_>, record: Option<&Incomplete>, failed: &[(usize, Failure)], ops: &[Op]) {
    let _ = writeln!(io.err, "E_ROLLBACK_INCOMPLETE: not everything could be put back. Fix these by hand:");
    match record {
        Some(r) if !r.steps.is_empty() => {
            for s in &r.steps {
                let _ = writeln!(io.err, "  could not put back: {} ({}: {})", s.target, s.code, s.message);
                let _ = writeln!(io.err, "    fix: {}", s.fix);
            }
        }
        _ => {
            for (i, f) in failed {
                let what = ops.get(*i).map_or_else(|| format!("step {i}"), op_label);
                let _ = writeln!(io.err, "  could not put back: {what} ({}: {})", f.code, f.message);
            }
            let _ = writeln!(io.err, "    fix: {CLEANUP_FIX}");
        }
    }
}

fn warning_text(w: &Warning) -> String {
    match w {
        Warning::NoWindowsTerminal => {
            "Windows Terminal is not installed: only the prompt and the welcome box change.".to_string()
        }
        Warning::MachineWideClink => {
            "Clink is also set up for the whole machine; the theme's prompt may not work.".to_string()
        }
        Warning::FontFallback { wanted } => {
            format!("The font {wanted:?} is not installed; Windows Terminal gets {NERD_FONT_FAMILY} instead.")
        }
    }
}

/// Maps a transaction outcome to an exit code and tells the user. `ops` are the transaction's
/// ops, when known, for a rollback the core kept no record of.
fn finish(io: &mut Io<'_>, sys: System<'_>, ctx: &Ctx, outcome: &Outcome, ops: &[Op], done: &str) -> i32 {
    match outcome {
        Outcome::Committed => {
            let _ = writeln!(io.out, "{done}");
            EXIT_OK
        }
        Outcome::RolledBack { code, reason } => {
            let _ = writeln!(io.err, "{code}: {reason}");
            let _ = writeln!(io.err, "Everything was rolled back; nothing changed.");
            EXIT_FAILED
        }
        Outcome::RollbackIncomplete { code, reason, failed } => {
            let _ = writeln!(io.err, "{code}: {reason}");
            // The record the core just wrote for this transaction: the newest one.
            let record = pending_incomplete(sys, ctx);
            tell_unrestored(io, record.as_ref(), failed, ops);
            EXIT_INCOMPLETE
        }
    }
}

fn apply(io: &mut Io<'_>, sys: System<'_>, ctx: &Ctx, theme_path: &std::path::Path, req: &Request) -> i32 {
    let bytes = match sys.fs.read(theme_path) {
        Ok(Some(b)) => b,
        Ok(None) => {
            let _ = writeln!(io.err, "E_THEME_INVALID: {} does not exist", theme_path.display());
            return EXIT_INVALID;
        }
        Err(e) => {
            let _ = writeln!(io.err, "E_THEME_INVALID: {e}");
            return EXIT_INVALID;
        }
    };
    let theme = match parse_theme(&bytes) {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(io.err, "{}: {e}", e.code());
            return EXIT_INVALID;
        }
    };
    let prepared = match prepare_apply(sys, ctx, &theme, req) {
        Ok(p) => p,
        Err(e) => return error(io, &e),
    };
    let _ = writeln!(io.out, "Plan for {:?}:", theme.name);
    for line in &prepared.plan.summary_en {
        let _ = writeln!(io.out, "  - {line}");
    }
    for w in &prepared.plan.warnings {
        let _ = writeln!(io.out, "Warning: {}", warning_text(w));
    }
    let ops = prepared.ops.clone();
    let outcome = run_apply(sys, ctx, prepared);
    finish(
        io,
        sys,
        ctx,
        &outcome,
        &ops,
        &format!("Applied {:?}. Open a new cmd window to see the prompt and the welcome box.", theme.name),
    )
}

fn run_doctor(io: &mut Io<'_>, sys: System<'_>, ctx: &Ctx, json: bool) -> i32 {
    let checks = doctor(sys, ctx);
    if json {
        let body = serde_json::json!({ "checks": checks });
        let _ = writeln!(io.out, "{}", serde_json::to_string_pretty(&body).unwrap_or_default());
    } else {
        for c in &checks {
            let status = match c.status {
                Status::Ok => "ok",
                Status::Warn => "warn",
                Status::Fail => "FAIL",
                Status::Skipped => "skip",
            };
            let code = c.code.map(|c| format!(" [{c}]")).unwrap_or_default();
            let _ = writeln!(io.out, "{:<4} {:<5} {}{}", c.id, status, c.message_en, code);
            if !c.details.is_empty() {
                let _ = writeln!(io.out, "           {}", c.details);
            }
        }
    }
    if checks.iter().any(|c| c.status == Status::Fail) {
        EXIT_FAILED
    } else {
        EXIT_OK
    }
}

/// Milliseconds since 1970, for restore point ids.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Runs one command on this machine (spec §5.7).
#[cfg(windows)]
pub fn run(args: &[OsString], io: &mut Io<'_>) -> i32 {
    use cmdstudio_core::adapters::program_data_dir;
    use cmdstudio_core::adapters::windows::{
        fonts::WinFonts, fs::WinFs, host::WinHost, process::WinProcess, registry::WinAutoRun, wt::WinWt,
    };
    use cmdstudio_core::adapters::Host;
    let host = WinHost;
    let local = match host.local_app_data() {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(io.err, "E_DATA_WRITE: {e}");
            return EXIT_FAILED;
        }
    };
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
    let defaults = Defaults {
        data_dir: local.join("ai.resincore.cmdstudio"),
        resources: exe_dir.join("resources"),
        now_ms: now_ms(),
    };
    // Only programs from the runtime folders may run (spec §5.10).
    let data = parse(args).ok().and_then(|a| a.data_dir).unwrap_or_else(|| defaults.data_dir.clone());
    let mut allowed = vec![data.join("runtime")];
    if let (Ok(pd), Ok(sid)) = (host.program_data(), host.user_sid()) {
        allowed.push(program_data_dir(&pd, &sid));
    }
    let (fs, autorun, fonts, wt) = (WinFs, WinAutoRun::real(), WinFonts::real(&local), WinWt::new(local.clone()));
    let process = WinProcess::new(allowed, std::time::Duration::from_secs(60));
    let sys = System { fs: &fs, autorun: &autorun, fonts: &fonts, wt: &wt, process: &process, host: &host };
    run_with(args, io, sys, &defaults)
}
```
`run_doctor` uses `unwrap_or_default()` on `to_string_pretty`, which cannot fail for this value; it never panics.

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-cli --lib`
Expected: `10 passed` (3 in `args::tests`, 7 in `tests`).

- [ ] **Step 6: Smoke-test the real binary, read-only.** Only `--help` and `doctor` are run on the development machine: `apply`, `undo` and `--cleanup` change the real AutoRun and fonts, and run only in Task 17's gated test, in CI and on the VMs.

Run:
```powershell
cargo build -p cmdstudio-cli
.\target\debug\cmdstudio.exe --help; "exit $LASTEXITCODE"
.\target\debug\cmdstudio.exe doctor --data-dir "$env:TEMP\cmdstudio-smoke"; "exit $LASTEXITCODE"
.\target\debug\cmdstudio.exe frobnicate; "exit $LASTEXITCODE"
Remove-Item -Recurse -Force "$env:TEMP\cmdstudio-smoke" -ErrorAction SilentlyContinue
```
Expected: the usage text and `exit 0`; eleven lines `D1` … `D11` with D1–D4, D7–D9 `skip` (no theme applied in that data folder), D6 `ok` or `warn` depending on Windows Terminal, D5 `ok`, or `warn [E_AUTORUN_FOREIGN_CLINK]` with the injecting command on the next line on a machine that already runs its own Clink, and `exit 0`; then `cmdstudio: unknown command "frobnicate"`, the usage text and `exit 2`. Nothing is printed before `D1`: the fresh data folder has no unfinished journal and no `incomplete.json`.

- [ ] **Step 7: Bring §5.7 of the spec up to date.** In `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` §5.7, replace the `apply` row of the CLI table with
```text
| `cmdstudio apply <file.cmdtheme> [--target all\|wt] [--replace-other-clink] [--lang th\|en]` | ใช้ธีม (ใช้ในเทสต์และงานอัตโนมัติ) · `--replace-other-clink` ปิด Clink ตัวอื่นใน AutoRun ของผู้ใช้ (ย้อนกลับได้) · `--lang` คือภาษาของกล่องต้อนรับ ค่าเริ่มต้นตามภาษาของ Windows · ตั้งโปรไฟล์ cmd เป็นค่าเริ่มต้นของ WT เหมือนช่องที่ติ๊กไว้ในหน้า 4 |
```
and after the line that starts with `exit code:` add
```text

ทุกคำสั่งรับ `--data-dir <dir>` และ `--resources <dir>` แทนโฟลเดอร์ข้อมูลและโฟลเดอร์ `resources\` ข้าง exe (ใช้ในเทสต์) · ก่อนทำคำสั่งใดก็ตาม CLI จะย้อนทรานแซกชันที่ค้างจากครั้งก่อนให้เสร็จก่อน (§5.5) ถ้าย้อนไม่ครบจะหยุดด้วย exit code `3` · ถ้าการย้อนกลับครั้งล่าสุดไม่ครบ (มี `incomplete.json`) CLI จะแจ้งรายการที่ต้องแก้ด้วยมือทุกครั้งที่รัน จนกว่าจะมีทรานแซกชันใหม่ · `apply` ที่เจอ Clink ตัวอื่นจะแสดงทุกคำสั่งที่จะถูกเอาออกแล้วจบด้วย `2` ส่วนค่า AutoRun ที่แก้อย่างปลอดภัยไม่ได้ (`E_AUTORUN_WRITE`) จะแสดงคำสั่งนั้นพร้อมวิธีแก้ด้วยมือแล้วจบด้วย `1` · `doctor` จบด้วย `1` เมื่อมีข้อที่ล้มเหลว
```
Append to appendix B:
```text
| 2026-10-12 | CLI ของ phase 1b (architect เลือก founder เปลี่ยนได้):<br>• `apply` รับ `--replace-other-clink` และ `--lang` และตั้งโปรไฟล์ cmd เป็นค่าเริ่มต้นของ WT ตามค่าเริ่มต้นของหน้า 4<br>• ทุกคำสั่งรับ `--data-dir` และ `--resources` สำหรับเทสต์ และย้อนทรานแซกชันที่ค้างก่อนเริ่มเสมอ รวมทั้ง `doctor`<br>• การย้อนกลับที่ไม่ครบถูกแจ้งพร้อมวิธีแก้ทุกครั้งที่รัน จนกว่าจะมีทรานแซกชันใหม่<br>• `doctor` จบด้วย exit code `1` เมื่อมีข้อที่ล้มเหลว |
```

- [ ] **Step 8: Format and lint**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`.
Expected: no warnings.

- [ ] **Step 9: Commit**

Message:
```text
feat(cli): add the cmdstudio command line

apply, doctor, undo and --cleanup over cmdstudio-core, with exit codes
0 done, 1 failed and rolled back, 2 invalid input, 3 rollback
incomplete. Every command first finishes a transaction a crash cut off,
and a rollback that did not finish is reported with what to fix by hand
on every run until a newer transaction runs. apply lists every command
of another Clink it would take out, and shows the command at fault
with the Registry Editor fix when AutoRun needs a hand edit.
--data-dir and --resources point tests elsewhere; only programs from
the runtime folders may run. The library's run() is what the Phase 2
app calls when started with arguments.

Spec 5.7 lists the extra options; appendix B records them.
```
Run:
```bash
git add -- Cargo.toml Cargo.lock crates/cli/Cargo.toml crates/cli/src/args.rs crates/cli/src/lib.rs crates/cli/src/main.rs docs/superpowers/specs/2026-10-10-cmd-studio-design.md
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 17: Gated system tests on the real machine

**Files:**
- Create: `crates/cli/tests/system.rs`, `crates/core/tests/omp_render.rs`

**Interfaces:**
- Consumes: Task 16 `cmdstudio_cli::{run_with, now_ms, Defaults, Io, EXIT_OK}`; Tasks 3–7 and 12 `adapters::windows::{fs::WinFs, registry::WinAutoRun, host::WinHost, fonts::WinFonts, wt::WinWt, process::WinProcess}` and the traits `AutoRun`, `Fonts`, `Host`; Task 12 `verify::omp_config_error`; Task 7's fixture `crates/core/tests/fixtures/wt/default.json`; Task 2's `resources\` (built by `scripts/fetch-resources.ps1`); Phase 1a `generate::omp::config_text`, `model::{Layout, Segment}` (`#[non_exhaustive]`, so only constructed here, never matched), the public `theme.prompt.{layout, segments, transient}` fields, `presets::builtin()` (Terracotta is picked by its key `"terracotta"`: the core's `presets::terracotta()` is test-only and invisible to an integration test), and `presets/terracotta.cmdtheme` containing `"Cascadia Mono"`.
- Produces: `system_apply_doctor_undo_is_byte_identical` (cmdstudio-cli) and `system_real_oh_my_posh_renders_every_layout` (cmdstudio-core). Both print `SKIPPED …` and pass unless `CMDSTUDIO_SYSTEM_TESTS=1`. CI (Task 18) and the VMs (Task 19) set it.

**What they prove:**
- `system_apply_doctor_undo_is_byte_identical` is spec §8's Phase 1 criterion on a real Windows: `cmdstudio apply` (AutoRun must then start with our parenthesized entry) → `cmdstudio doctor --json` (no check `fail`; D3 is a warning there, because no cmd window has started since the apply) → `cmdstudio undo`, using the real HKCU AutoRun, the real per-user fonts and the real bundled Oh My Posh and Clink, then the AutoRun key and value (type and bytes), `settings.json` (bytes), the GDI font list, the font registration and whether the per-user font key exists must equal what they were before, and the runtime folder must be gone. A CI runner has no Windows Terminal, so WT's folder is a temp copy of the `default.json` fixture. The theme file is named `ธีม ทดสอบ.cmdtheme` (Review Focus 4), and its font is swapped for one no machine has, so the bundled CaskaydiaCove NF really gets installed and uninstalled. The data folder is a temp folder; on VM-C its path holds the Thai user name, so `<clink-launch>` goes through the 8.3 path or the ProgramData folder (G19).
- `system_real_oh_my_posh_renders_every_layout` closes the gap the Phase 1a plan left to Phase 1b ("line/powerline checked by Phase 1b parity tests"; the emulator parity test itself is Phase 2, spec §8): every layout × transient on/off, with every segment on, is written by Phase 1a's generator and rendered by the pinned Oh My Posh with `print primary --config <file> --shell cmd --plain`; it must exit 0, print something, and print none of the config-error markers.

**Never set `CMDSTUDIO_SYSTEM_TESTS=1` for `system.rs` on the development machine:** it changes the real AutoRun and fonts while it runs. `omp_render.rs` only runs Oh My Posh on files in a temp folder, so Step 4 runs it for real here.

- [ ] **Step 1: Write the system test** — create `crates/cli/tests/system.rs`:
```rust
//! System test (spec §7 integration test): apply → doctor → undo with this
//! machine's real HKCU AutoRun, real per-user fonts and the real bundled
//! Oh My Posh and Clink, against a fake Windows Terminal folder; then every
//! one of those must be byte-identical to before. It changes the machine, so
//! it runs only with CMDSTUDIO_SYSTEM_TESTS=1 (set in CI and on test VMs).
#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use cmdstudio_cli::{now_ms, run_with, Defaults, Io, EXIT_OK};
use cmdstudio_core::adapters::windows::{
    fonts::WinFonts, fs::WinFs, host::WinHost, process::WinProcess, registry::WinAutoRun, wt::WinWt,
};
use cmdstudio_core::adapters::{AutoRun, Fonts, Host, System};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join(rel)
}

fn cli(sys: System<'_>, d: &Defaults, args: &[&std::ffi::OsStr]) -> (i32, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let args: Vec<OsString> = args.iter().map(|a| a.to_os_string()).collect();
    let code = run_with(&args, &mut Io { out: &mut out, err: &mut err }, sys, d);
    (code, format!("{}{}", String::from_utf8_lossy(&out), String::from_utf8_lossy(&err)))
}

#[test]
fn system_apply_doctor_undo_is_byte_identical() {
    if std::env::var_os("CMDSTUDIO_SYSTEM_TESTS").is_none() {
        println!("SKIPPED system_apply_doctor_undo_is_byte_identical: set CMDSTUDIO_SYSTEM_TESTS=1");
        return;
    }
    let resources = repo("resources");
    assert!(resources.join("manifest.json").is_file(), "run scripts/fetch-resources.ps1 first");
    let tmp = tempfile::tempdir().unwrap();
    // A fake %LOCALAPPDATA% for Windows Terminal only; a CI runner has no WT.
    let fake_local = tmp.path().join("Local");
    let wt = fake_local.join(r"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json");
    std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
    std::fs::write(&wt, std::fs::read(repo(r"crates\core\tests\fixtures\wt\default.json")).unwrap()).unwrap();
    // A font no machine has, so the bundled CaskaydiaCove NF really gets installed.
    let preset = std::fs::read_to_string(repo(r"presets\terracotta.cmdtheme")).unwrap();
    let theme = tmp.path().join("ธีม ทดสอบ.cmdtheme");
    std::fs::write(&theme, preset.replace("\"Cascadia Mono\"", "\"CMD Studio Missing Font\"")).unwrap();
    let data = tmp.path().join("ai.resincore.cmdstudio");

    let host = WinHost;
    let real_local = host.local_app_data().unwrap();
    let (fs, autorun, fonts, wt_loc) = (WinFs, WinAutoRun::real(), WinFonts::real(&real_local), WinWt::new(fake_local));
    let process = WinProcess::new(vec![data.join("runtime")], Duration::from_secs(60));
    let sys = System { fs: &fs, autorun: &autorun, fonts: &fonts, wt: &wt_loc, process: &process, host: &host };
    let snapshot = |a: &WinAutoRun, f: &WinFonts, w: &Path| {
        (
            a.user_key_exists().unwrap(),
            a.read_user().unwrap(),
            std::fs::read(w).unwrap(),
            f.families().unwrap(),
            f.registration("CaskaydiaCove NF (TrueType)").unwrap(),
            f.registration_key_exists().unwrap(),
        )
    };
    let before = snapshot(&autorun, &fonts, &wt);
    let d = Defaults { data_dir: data.clone(), resources, now_ms: now_ms() };

    let (code, log) = cli(sys, &d, &[std::ffi::OsStr::new("apply"), theme.as_os_str()]);
    assert_eq!(code, EXIT_OK, "{log}");
    assert!(fonts.families().unwrap().iter().any(|f| f == "CaskaydiaCove NF"), "font not installed");
    // Our entry comes first, in parentheses (spec §5.4).
    let value = autorun.read_user().unwrap().unwrap().text().unwrap();
    assert!(value.starts_with("(if exist \""), "{value}");
    assert!(value.contains("inject --autorun --quiet --profile"), "{value}");

    let d2 = Defaults { now_ms: d.now_ms + 1, ..d.clone() };
    let (code, log) = cli(sys, &d2, &[std::ffi::OsStr::new("doctor"), std::ffi::OsStr::new("--json")]);
    let report: serde_json::Value = serde_json::from_str(&log).unwrap_or_else(|_| panic!("{log}"));
    for c in report["checks"].as_array().unwrap() {
        assert_ne!(c["status"], "fail", "{c}");
    }
    assert_eq!(code, EXIT_OK, "{log}");

    let d3 = Defaults { now_ms: d.now_ms + 2, ..d.clone() };
    let (code, log) = cli(sys, &d3, &[std::ffi::OsStr::new("undo")]);
    assert_eq!(code, EXIT_OK, "{log}");
    assert_eq!(snapshot(&autorun, &fonts, &wt), before);
    assert!(!data.join("runtime").join("omp-31.5.0").exists());
}
```

- [ ] **Step 2: Check it compiles and skips**

Run: `cargo test -p cmdstudio-cli --test system -- --nocapture`
Expected: `SKIPPED system_apply_doctor_undo_is_byte_identical: set CMDSTUDIO_SYSTEM_TESTS=1` and `1 passed`.

- [ ] **Step 3: Write the render test** — create `crates/core/tests/omp_render.rs`:
```rust
//! The real Oh My Posh 31.5.0 renders every layout the generator writes, with
//! every segment on and transient on and off (spec §3.5; the Phase 1a plan
//! left this check to Phase 1b). It needs the bundled runtime, so it runs only
//! with CMDSTUDIO_SYSTEM_TESTS=1 after scripts/fetch-resources.ps1. It writes
//! only to a temp folder.
#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use cmdstudio_core::generate::omp::config_text;
use cmdstudio_core::model::{Layout, Segment};
use cmdstudio_core::presets::builtin;
use cmdstudio_core::verify::omp_config_error;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn system_real_oh_my_posh_renders_every_layout() {
    if std::env::var_os("CMDSTUDIO_SYSTEM_TESTS").is_none() {
        println!("SKIPPED system_real_oh_my_posh_renders_every_layout: set CMDSTUDIO_SYSTEM_TESTS=1");
        return;
    }
    let exe = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(r"..\..\resources\omp\oh-my-posh.exe");
    assert!(exe.is_file(), "run scripts/fetch-resources.ps1 first");
    let tmp = tempfile::tempdir().unwrap();
    // The Terracotta preset, picked by its key (the core's own helper is test-only).
    let mut theme = builtin().unwrap().into_iter().find(|(key, _)| *key == "terracotta").unwrap().1;
    theme.prompt.segments = vec![
        Segment::Path,
        Segment::Git,
        Segment::Time,
        Segment::User,
        Segment::Host,
        Segment::Status,
        Segment::Duration,
    ];
    for layout in [Layout::Box, Layout::Line, Layout::Powerline] {
        for transient in [false, true] {
            theme.prompt.layout = layout;
            theme.prompt.transient = transient;
            let config = tmp.path().join(format!("{layout:?}-{transient}.omp.json"));
            std::fs::write(&config, config_text(&theme).unwrap()).unwrap();
            let out = Command::new(&exe)
                .args(["print", "primary", "--config"])
                .arg(&config)
                .args(["--shell", "cmd", "--plain"])
                .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            let what = format!("{layout:?}, transient {transient}");
            assert!(out.status.success(), "{what}: {}", String::from_utf8_lossy(&out.stderr));
            assert_eq!(omp_config_error(&stdout), None, "{what}: {stdout}");
            assert!(!stdout.trim().is_empty(), "{what}: printed nothing");
        }
    }
}
```

- [ ] **Step 4: Run the render test for real**

Run (PowerShell; the variable is set for this one command and removed again):
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\fetch-resources.ps1
cargo test -p cmdstudio-core --test omp_render -- --nocapture
$env:CMDSTUDIO_SYSTEM_TESTS = '1'; cargo test -p cmdstudio-core --test omp_render -- --nocapture; Remove-Item Env:\CMDSTUDIO_SYSTEM_TESTS
```
Expected: the first `cargo test` prints `SKIPPED system_real_oh_my_posh_renders_every_layout …` and `1 passed`; the second prints no `SKIPPED` line and `1 passed` (it did when this plan was checked, 2026-10-12, with Phase 1a's shipped generator). If a layout fails, the message names the layout and Oh My Posh's output: that is a Phase 1a generator bug — stop and report it, do not change the generator in this task.

- [ ] **Step 5: Format and lint**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`.
Expected: no warnings.

- [ ] **Step 6: Commit**

Message:
```text
test: add gated system tests for apply-undo and Oh My Posh rendering

system_apply_doctor_undo_is_byte_identical runs cmdstudio apply, doctor
and undo against the real AutoRun, fonts and bundled runtimes, and
checks the registry, settings.json and the font list are byte-identical
afterwards (spec 8, Phase 1). system_real_oh_my_posh_renders_every_layout
renders every layout the generator writes with the pinned Oh My Posh.
Both run only with CMDSTUDIO_SYSTEM_TESTS=1.
```
Run:
```bash
git add -- crates/cli/tests/system.rs crates/core/tests/omp_render.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---
### Task 18: CI on `windows-latest`, and the Phase 1b quality gate

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: everything above; `scripts/fetch-resources.ps1` (Task 2); `rust-toolchain.toml` (Phase 1a, Rust `1.99.0` with rustfmt and clippy); the `system_*` tests and `writing_through_a_symlink_keeps_the_link` (Tasks 3–6, 17), which run for real only with `CMDSTUDIO_SYSTEM_TESTS=1`.
- Produces: the `ci` workflow: format, both clippy runs, the fetch, the normal test run, then the system-test run on the runner, which fails if any test printed `SKIPPED`.

The runner is a throwaway Windows machine, so the system tests may change its AutoRun and fonts there (brief: CI sets `CMDSTUDIO_SYSTEM_TESTS=1`). It has no Windows Terminal, which is why Task 17's system test points WT's folder at a temp copy. The existing `spike-windows.yml` stays as it is.

- [ ] **Step 1: Write the workflow** — create `.github/workflows/ci.yml`:
```yaml
name: ci
on:
  push:
    branches: [main]
  pull_request:
  workflow_dispatch:
permissions:
  contents: read
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true
jobs:
  windows:
    runs-on: windows-latest
    timeout-minutes: 60
    env:
      CARGO_TERM_COLOR: always
    steps:
      - uses: actions/checkout@v4
      - name: Install the toolchain pinned in rust-toolchain.toml
        run: |
          rustup --version
          rustup toolchain install
          cargo --version
      - name: Format
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: Clippy of the core alone, as a release build sees it (no fake adapters)
        run: cargo clippy -p cmdstudio-core --all-targets -- -D warnings
      - name: Fetch the pinned runtimes and font
        shell: powershell
        run: ./scripts/fetch-resources.ps1
      - name: Tests
        run: cargo test --workspace
      - name: System tests (this runner's real AutoRun, fonts and bundled runtimes)
        shell: bash
        env:
          CMDSTUDIO_SYSTEM_TESTS: '1'
        run: |
          cargo test --workspace -- --test-threads=1 --nocapture 2>&1 | tee system-tests.txt
          if grep -q SKIPPED system-tests.txt; then
            echo "a system test skipped although CMDSTUDIO_SYSTEM_TESTS=1"
            exit 1
          fi
      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: system-tests-log
          path: system-tests.txt
          if-no-files-found: ignore
```
GitHub runs `shell: bash` with `-eo pipefail`, so a failing `cargo test` fails the step even through `tee`. `--test-threads=1` keeps the tests that touch the runner's AutoRun and fonts from overlapping. The two clippy runs differ on purpose: in a workspace build the CLI's dev-dependency turns on the core's `fake` feature, so the first run lints the core with the fakes and the second without them.

- [ ] **Step 2: Run locally everything CI runs except the system-test step**

Run:
```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p cmdstudio-core --all-targets -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\fetch-resources.ps1
cargo test --workspace
```
Expected: no format diff; no clippy warnings; the fetch script prints `resources are complete: …\resources` (or `resources ready: …` after downloading on a fresh checkout); every test passes. Phase 1b's part of the count: `cmdstudio-core` gains 106 unit tests and 1 integration test (`omp_render`, printing `SKIPPED`), `cmdstudio-cli` has 10 unit tests and 1 integration test (`system`, printing `SKIPPED`). `cargo test --workspace -- --list` lists 429 tests: Phase 1a's 311 after Task 0 (its 2 doc tests included) plus these 118. **Do not run the system-test step on the development machine.**

- [ ] **Step 3: The quality gate** — three mechanical checks:
```bash
git grep -n -E "std::fs|winreg|windows_sys|std::process|Command::new" -- crates/core/src ':!crates/core/src/adapters/windows'
git log --format=%B main..HEAD | grep -i -E "co-authored-by|generated with|claude"
git ls-files resources
```
Expected: all three print nothing (exit code 1 for the two greps, 0 for `ls-files`). The first proves only `adapters::windows` touches the machine (spec §5.2); the second that no commit carries an attribution line (the founder's rule); the third that the downloaded binaries are not committed (Task 2 ignores `/resources/`).

- [ ] **Step 4: Commit**

Message:
```text
ci: add the Windows CI workflow with system tests

Format, clippy (the workspace, and the core alone without the fake
adapters), the pinned resource fetch, the tests, then the system tests with
CMDSTUDIO_SYSTEM_TESTS=1 on the throwaway runner: real AutoRun, fonts
and bundled runtimes. A SKIPPED line in that run fails the job.
```
Run:
```bash
git add -- .github/workflows/ci.yml
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

- [ ] **Step 5: The first CI run (controller).** CI runs only on GitHub. When the controller pushes the branch and opens the pull request (the `auroraz-pr-workflow` skill; implementers never push), the `ci` run must pass every step, and the `system-tests-log` artifact must show `test system_apply_doctor_undo_is_byte_identical ... ok`, `test system_real_oh_my_posh_renders_every_layout ... ok`, `system_real_autorun_round_trips`, `system_real_font_install_and_remove` and `writing_through_a_symlink_keeps_the_link` as `ok`, with no `SKIPPED` line. A failure there is a real-machine bug: fix it in the task that owns the code, with a test that pins it, before Task 19.

---
### Task 19: Apply → doctor → undo on the three VMs (run by the controller)

This task drives the VirtualBox VMs on the host, so the controller runs it, not an implementer subagent: it types into the VMs and needs their passwords in `%USERPROFILE%\VMs\accounts`. It is the spec §8 Phase 1 done-criterion on stock Windows: apply → undo leaves the registry, Windows Terminal's settings and the font list byte-identical on VM-A (`cmdstudio-win11`, user `tester`), VM-B (`cmdstudio-win10`, `tester`, no Windows Terminal) and VM-C (`cmdstudio-th`, user `ผู้ทดสอบ ไทย`). It also proves what CI cannot: Clink starting from `<clink-launch>` in a real cmd window (D3 reads `Inject completed` from `clink.log`, G19 on VM-C), the two-file Clink bundle (Task 2), the WT colors on cmd windows opened from Win+R (G7), the font install in a desktop session and undo while a cmd window still holds Clink's DLL (Review Focus 2). Ruling F3 adds four checks that Phase 1a changed and only a real machine can confirm:
- the parenthesized AutoRun entry, first, in front of another program's `if` command kept as `REG_EXPAND_SZ`, with a real interactive inject, and the other program's command still running after ours (ruling P59; spec §5.4 marks it ⬜);
- `init.lua`'s `2>nul` popen form: with `oh-my-posh.exe` gone, a new cmd window shows no error text and doctor D2 reports it (ruling P47; spec §5.4 marks it ⬜);
- `clink.path` with Thai letters (VM-C): Clink loads `banner.lua` from the folder `clink_settings` names;
- `cursorShape` `bar` and `underscore` in Windows Terminal (G6 ⬜, ruling P13).

**Files:**
- Create: `spikes/vm/guest/phase1b-check.ps1`
- Modify: `spikes/vm/guest/guard.test.ps1` (one more guarded script), `spikes/vm/README.md` (one line), `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (G6, §5.4, appendix B — only with the results below)

**Interfaces:**
- Consumes: the release `cmdstudio.exe` (Task 16) and `resources\` (Task 2); `presets/terracotta.cmdtheme` (Phase 1a); `spikes/vm/vm.ps1` (`Invoke-GuestDesktop`, `Copy-ToGuest`, `Wait-GuestReady`, `Get-VmState`, `Send-Run`, `Save-Shot`, `Measure-Color`, `$Vbm`) and `spikes/vm/guest/run-on-desktop.ps1`; the VMs' `clean` snapshots.
- Produces: `phase1b-check.ps1 -Step OpenWt|ForeignAutoRun|Snapshot|Apply|ShowAutoRun|ShowClinkPath|Doctor|HideOmp|RestoreOmp|CloseCmd|Undo|Compare [-Label <name>] [-Theme <file>] [-Against <name>]`; host logs `spikes\vm\tmp\<vm>-phase1b.txt` and screenshots `spikes\vm\tmp\<vm>-*.png` (git-ignored); the results table in the pull request.

- [ ] **Step 1: The guest script** — create `spikes/vm/guest/phase1b-check.ps1` (ASCII only, so Windows PowerShell 5.1 reads it without a BOM). Like every guest script that changes settings, it refuses to run outside a `CMDSTUDIO-*` machine:
```powershell
# Runs inside a test VM on the signed-in user's desktop (Invoke-GuestDesktop): the Phase 1b
# apply -> doctor -> undo check (plan Task 19). Each -Step does one part:
#   OpenWt          start Windows Terminal once, so it writes its settings.json, then close it
#   ForeignAutoRun  give AutoRun another program's command (REG_EXPAND_SZ) for our entry to go in front of
#   Snapshot        save AutoRun's key, the per-user font key, settings.json and the font lists to out\phase1b\<Label>
#   Apply           cmdstudio apply <Theme>       Doctor  cmdstudio doctor (and --json to a file)
#   ShowAutoRun     print AutoRun as the registry holds it, and how often the other program's command ran
#   ShowClinkPath   print the clink.path line CMD Studio wrote for Clink
#   HideOmp         rename oh-my-posh.exe away (a broken runtime); RestoreOmp puts it back
#   CloseCmd        close every cmd window and Windows Terminal
#   Undo            cmdstudio undo                Compare  are snapshots <Against> and <Label> byte-identical?
param(
  [Parameter(Mandatory)]
  [ValidateSet('OpenWt', 'ForeignAutoRun', 'Snapshot', 'Apply', 'ShowAutoRun', 'ShowClinkPath', 'Doctor', 'HideOmp', 'RestoreOmp', 'CloseCmd', 'Undo', 'Compare')]
  [string] $Step,
  [string] $Label = 'before',
  [string] $Theme = '',
  [string] $Against = 'before'
)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script applies and undoes CMD Studio themes" }
# cmdstudio.exe prints UTF-8; without this, Thai paths come back as '?'.
[Console]::OutputEncoding = [Text.Encoding]::UTF8
# C:\cmdstudio in the VM: this script lives in C:\cmdstudio\spikes\vm\guest.
$root = Split-Path (Split-Path (Split-Path $PSScriptRoot))
$app = Join-Path $root 'app\cmdstudio.exe'
$out = Join-Path $root 'out\phase1b'
$settings = Join-Path $env:LOCALAPPDATA 'Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json'
$data = Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio'
$omp = Join-Path $data 'runtime\omp-31.5.0'
# The other program's command: it adds a line to a file each time a cmd window starts.
$foreign = Join-Path $env:USERPROFILE 'cmdstudio-foreign.cmd'
$foreignLog = Join-Path $env:USERPROFILE 'cmdstudio-foreign.txt'

function Invoke-App([string[]] $Arguments) {
  # Windows PowerShell 5.1 turns a native program's stderr into error records; 'Stop' would end the script.
  $ErrorActionPreference = 'Continue'
  & $app @Arguments 2>&1 | ForEach-Object { "$_" }
  "exit $LASTEXITCODE"
}

# reg.exe keeps every value's type and bytes. An absent key is written as the word "absent".
function Export-Key([string] $Key, [string] $File) {
  $ErrorActionPreference = 'Continue'
  $null = & reg.exe query $Key 2>&1
  if ($LASTEXITCODE -eq 0) { $null = & reg.exe export $Key $File /y 2>&1 } else { Set-Content -LiteralPath $File -Value 'absent' }
}

switch ($Step) {
  'OpenWt' {
    # A fresh Windows has no settings.json until Windows Terminal first starts (plan Review Focus 1).
    # It starts twice, so anything it adds on a second start lands before the "before" snapshot.
    $wt = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\wt.exe'
    if (-not (Test-Path -LiteralPath $wt)) { 'Windows Terminal is not installed'; break }
    $had = Test-Path -LiteralPath $settings
    foreach ($run in 1, 2) {
      Start-Process -FilePath $wt
      $deadline = (Get-Date).AddSeconds(60)
      while (-not (Test-Path -LiteralPath $settings) -and (Get-Date) -lt $deadline) { Start-Sleep -Seconds 1 }
      Start-Sleep -Seconds 5
      Stop-Process -Name WindowsTerminal -Force -ErrorAction SilentlyContinue
      Start-Sleep -Seconds 2
    }
    if (-not (Test-Path -LiteralPath $settings)) { throw 'Windows Terminal wrote no settings.json within 60 s' }
    if ($had) { 'settings.json already existed' } else { 'settings.json created by Windows Terminal' }
  }
  'ForeignAutoRun' {
    # Ruling P59: our entry goes in front, in parentheses, so another program's `if` neither keeps
    # it from running nor is cut short by it. This command is such an `if`, kept unexpanded.
    Set-Content -LiteralPath $foreign -Value '@echo %DATE% %TIME%>>"%USERPROFILE%\cmdstudio-foreign.txt"' -Encoding ASCII
    $key = 'HKCU:\Software\Microsoft\Command Processor'
    if (-not (Test-Path -LiteralPath $key)) { New-Item -Path $key -Force | Out-Null }
    $value = 'if exist "%USERPROFILE%\cmdstudio-foreign.cmd" call "%USERPROFILE%\cmdstudio-foreign.cmd"'
    New-ItemProperty -Path $key -Name AutoRun -PropertyType ExpandString -Value $value -Force | Out-Null
    "AutoRun holds another program's command: $value"
  }
  'Snapshot' {
    $dir = Join-Path $out $Label
    New-Item -ItemType Directory -Force $dir | Out-Null
    Export-Key 'HKCU\Software\Microsoft\Command Processor' (Join-Path $dir 'command-processor.reg')
    Export-Key 'HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts' (Join-Path $dir 'user-fonts.reg')
    if (Test-Path -LiteralPath $settings) { Copy-Item -LiteralPath $settings (Join-Path $dir 'settings.json') -Force }
    else { Set-Content -LiteralPath (Join-Path $dir 'settings.json') -Value 'absent' }
    Add-Type -AssemblyName System.Drawing
    (New-Object System.Drawing.Text.InstalledFontCollection).Families | ForEach-Object { $_.Name } | Sort-Object |
      Set-Content -LiteralPath (Join-Path $dir 'font-families.txt') -Encoding UTF8
    $fonts = Join-Path $env:LOCALAPPDATA 'Microsoft\Windows\Fonts'
    $files = @(Get-ChildItem -LiteralPath $fonts -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Name) $($_.Length)" })
    # Written even when the folder is empty or missing, so Compare always has both files.
    [IO.File]::WriteAllText((Join-Path $dir 'user-font-files.txt'), ($files -join "`n"))
    Get-ChildItem -LiteralPath $dir | ForEach-Object { '{0} {1}' -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash, $_.Name }
  }
  'Apply' { Invoke-App @('apply', $Theme) }
  'ShowAutoRun' {
    $key = Get-Item -LiteralPath 'HKCU:\Software\Microsoft\Command Processor' -ErrorAction SilentlyContinue
    if ($key -and ($key.GetValueNames() -contains 'AutoRun')) {
      # The raw text and its type, as cmd reads them: never expanded.
      'AutoRun ({0}): {1}' -f $key.GetValueKind('AutoRun'), $key.GetValue('AutoRun', $null, 'DoNotExpandEnvironmentNames')
    } else { 'AutoRun: absent' }
    $runs = @(Get-Content -LiteralPath $foreignLog -ErrorAction SilentlyContinue).Count
    "the other program's command ran $runs time(s)"
  }
  'ShowClinkPath' {
    # clink.path is a ;-separated list; on VM-C it holds the Thai user name.
    $file = Join-Path $data 'clink-profile\clink_settings'
    Get-Content -LiteralPath $file -Encoding UTF8 | Where-Object { $_ -like 'clink.path*' }
  }
  'Doctor' {
    Invoke-App @('doctor')
    New-Item -ItemType Directory -Force $out | Out-Null
    $ErrorActionPreference = 'Continue'
    & $app doctor --json 2>$null | Set-Content -LiteralPath (Join-Path $out "doctor-$Label.json") -Encoding UTF8
  }
  'HideOmp' {
    # Ruling P47: with Oh My Posh gone, a new cmd window shows no error text (init.lua's popen ends
    # in 2>nul), and doctor D2 says what is wrong instead.
    Rename-Item -LiteralPath (Join-Path $omp 'oh-my-posh.exe') -NewName 'oh-my-posh.exe.hidden'
    'oh-my-posh.exe renamed away'
  }
  'RestoreOmp' {
    Rename-Item -LiteralPath (Join-Path $omp 'oh-my-posh.exe.hidden') -NewName 'oh-my-posh.exe'
    'oh-my-posh.exe is back'
  }
  'CloseCmd' {
    # Nothing may hold Clink's DLL or the font file while undo removes them.
    Stop-Process -Name cmd, WindowsTerminal -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 3
    'every cmd window and Windows Terminal closed'
  }
  'Undo' { Invoke-App @('undo') }
  'Compare' {
    $a = Join-Path $out $Against
    $b = Join-Path $out $Label
    $diff = 0
    foreach ($name in 'command-processor.reg', 'user-fonts.reg', 'settings.json', 'font-families.txt', 'user-font-files.txt') {
      $ha = (Get-FileHash -LiteralPath (Join-Path $a $name) -Algorithm SHA256).Hash
      $hb = (Get-FileHash -LiteralPath (Join-Path $b $name) -Algorithm SHA256).Hash
      if ($ha -eq $hb) { "same: $name" } else { "DIFFERENT: $name"; $diff++ }
    }
    if ($diff) { "PHASE1B-DIFFERENT: $diff file(s) differ between $Against and $Label" } else { "PHASE1B-IDENTICAL: $Against and $Label" }
  }
}
```
In `spikes/vm/guest/guard.test.ps1`, add it to `$changers` (the line with `add-thai-user.ps1` becomes):
```powershell
  'add-thai-user.ps1' = @(); 'phase1b-check.ps1' = @('-Step', 'Snapshot')
```
In `spikes/vm/README.md`, add to the list of guest scripts:
```text
- `phase1b-check.ps1`: Phase 1b's apply → doctor → undo check (plan Task 19). Steps `OpenWt`, `ForeignAutoRun`, `Snapshot`, `Apply`, `ShowAutoRun`, `ShowClinkPath`, `Doctor`, `HideOmp`, `RestoreOmp`, `CloseCmd`, `Undo`, `Compare`.
```

- [ ] **Step 2: Check the guard on the host** (it runs the guest scripts only under a fake machine name and a temp `LOCALAPPDATA`; nothing on the host changes)

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File spikes\vm\guest\guard.test.ps1`
Expected: every line `pass: …`, including `pass: phase1b-check.ps1 starts with the test-VM guard` and `pass: phase1b-check.ps1 refuses to run on a machine that is not a test VM`, then `guest guards: 19/19 checks pass`.

Run the host blocks below in an interactive PowerShell at the repo root, one after another (they share variables). Paste them rather than saving them as a `.ps1`: Windows PowerShell 5.1 misreads the Thai user name in a script file without a BOM.

- [ ] **Step 3: Build what goes into the VMs.** A fresh Windows may lack the Visual C++ runtime (`VCRUNTIME140.dll`), so this build links it statically (how the shipped app gets it is Phase 4's installer decision). The theme variants: the font no machine has makes the planner install the bundled CaskaydiaCove NF; `powerline` needs Nerd Font glyphs; `solid` turns off transparency so `Measure-Color` can count the exact background color.
```powershell
. .\spikes\vm\vm.ps1
$env:RUSTFLAGS = '-C target-feature=+crt-static'; cargo build --release -p cmdstudio-cli --target-dir target\vm; Remove-Item Env:\RUSTFLAGS
Select-String -LiteralPath target\vm\release\cmdstudio.exe -Pattern 'VCRUNTIME140' -SimpleMatch -Quiet
$stage = Join-Path $env:TEMP 'cmdstudio-vm\app'
Remove-Item -Recurse -Force (Split-Path $stage) -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item target\vm\release\cmdstudio.exe $stage
Copy-Item -Recurse resources (Join-Path $stage 'resources')
$utf8 = New-Object Text.UTF8Encoding $false
$preset = [IO.File]::ReadAllText((Resolve-Path presets\terracotta.cmdtheme))
function Edit-Theme([string] $Text, [string] $From, [string] $To) {
  if (-not $Text.Contains($From)) { throw "the preset has no $From" }
  $Text.Replace($From, $To)
}
$nerd = Edit-Theme (Edit-Theme $preset '"Cascadia Mono"' '"CMD Studio Missing Font"') '"layout": "box"' '"layout": "powerline"'
$solid = Edit-Theme (Edit-Theme $nerd '"opacity": 96' '"opacity": 100') '"acrylic": true' '"acrylic": false'
[IO.File]::WriteAllText("$stage\terracotta.cmdtheme", $preset, $utf8)
[IO.File]::WriteAllText("$stage\nerd-powerline.cmdtheme", $nerd, $utf8)
[IO.File]::WriteAllText("$stage\solid.cmdtheme", $solid, $utf8)
[IO.File]::WriteAllText("$stage\cursor-bar.cmdtheme", (Edit-Theme $solid '"filledBox"' '"bar"'), $utf8)
[IO.File]::WriteAllText("$stage\cursor-underscore.cmdtheme", (Edit-Theme $solid '"filledBox"' '"underscore"'), $utf8)
```
Expected: the build finishes; `Select-String` prints `False`; no `the preset has no …` error (the shipped preset holds every `From` text above).

The host helpers used in every VM below:
```powershell
function Start-Clean($Vm, $User) {
  if ((Get-VmState $Vm) -ne 'poweroff') { & $Vbm controlvm $Vm poweroff; Start-Sleep -Seconds 5 }
  & $Vbm snapshot $Vm restore clean
  & $Vbm startvm $Vm --type headless
  if (-not (Wait-GuestReady $Vm $User)) { throw "$Vm is not ready" }
  Copy-ToGuest $Vm @((Resolve-Path spikes).Path) 'C:\cmdstudio\' $User
  Copy-ToGuest $Vm @($stage) 'C:\cmdstudio\' $User
}
$g = 'C:\cmdstudio\spikes\vm\guest\phase1b-check.ps1'
function Step($Vm, $User, [string] $StepArgs) { "## $StepArgs"; Invoke-GuestDesktop $Vm $g $StepArgs $User }
function Open-Cmd($Vm, $Label) { Send-Run $Vm 'cmd' | Out-Null; Start-Sleep -Seconds 15; Save-Shot $Vm $Label }
```

- [ ] **Step 4: VM-A (Windows 11, English)** — the main check with another program's AutoRun command, then the broken Oh My Posh, then G6, then undo with a cmd window open:
```powershell
$vm = 'cmdstudio-win11'; $u = 'tester'
Start-Clean $vm $u
& {
  Step $vm $u '-Step OpenWt'
  Step $vm $u '-Step ForeignAutoRun'
  Step $vm $u '-Step Snapshot -Label before'
  Step $vm $u '-Step Apply -Theme C:\cmdstudio\app\solid.cmdtheme'
  Step $vm $u '-Step ShowAutoRun'
  $shot = Open-Cmd $vm 'phase1b-applied'; "background #1F1E1D pixels: $(Measure-Color $shot '#1F1E1D')"
  Step $vm $u '-Step ShowAutoRun'
  Step $vm $u '-Step Doctor -Label applied'
  # Ruling P47: a broken runtime prints nothing into a cmd window; the doctor says what is wrong.
  Step $vm $u '-Step HideOmp'
  $null = Open-Cmd $vm 'phase1b-no-omp'
  Step $vm $u '-Step Doctor -Label no-omp'
  Step $vm $u '-Step RestoreOmp'
  Step $vm $u '-Step CloseCmd'
  Step $vm $u '-Step Undo'
  Step $vm $u '-Step Snapshot -Label after'
  Step $vm $u '-Step Compare -Label after -Against before'
  # G6: the cursor shapes WT documents but Phase 0 did not try.
  foreach ($shape in 'bar', 'underscore') {
    Step $vm $u "-Step Apply -Theme C:\cmdstudio\app\cursor-$shape.cmdtheme"
    $null = Open-Cmd $vm "phase1b-cursor-$shape"
    Step $vm $u '-Step CloseCmd'
    Step $vm $u '-Step Undo'
  }
  Step $vm $u '-Step Snapshot -Label after-cursor'
  Step $vm $u '-Step Compare -Label after-cursor -Against before'
  # Review Focus 2: undo while a cmd window holds Clink's DLL.
  Step $vm $u '-Step Apply -Theme C:\cmdstudio\app\terracotta.cmdtheme'
  $null = Open-Cmd $vm 'phase1b-inuse'
  Step $vm $u '-Step Undo'
  Step $vm $u '-Step Doctor -Label inuse'
  Step $vm $u '-Step CloseCmd'
  Step $vm $u '-Step Undo'
  Step $vm $u '-Step Snapshot -Label after-inuse'
  Step $vm $u '-Step Compare -Label after-inuse -Against before'
} *>&1 | Tee-Object -FilePath "spikes\vm\tmp\$vm-phase1b.txt"
```
Expected, in order:
- `OpenWt`: `settings.json created by Windows Terminal` (or `settings.json already existed`). `ForeignAutoRun`: `AutoRun holds another program's command: if exist "%USERPROFILE%\cmdstudio-foreign.cmd" call "%USERPROFILE%\cmdstudio-foreign.cmd"`.
- the first `Apply`: the plan summary, including the line about installing 1 font for this account and `Use this theme in every cmd window`, then `Applied "Terracotta". …` and `exit 0`.
- the first `ShowAutoRun`: `AutoRun (ExpandString): (if exist "C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\runtime\clink-1.9.34\clink_x64.exe" "C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\runtime\clink-1.9.34\clink_x64.exe" inject --autorun --quiet --profile "C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\clink-profile") & if exist "%USERPROFILE%\cmdstudio-foreign.cmd" call "%USERPROFILE%\cmdstudio-foreign.cmd"` — ours first, in parentheses, the other program's command after it byte for byte, the type still `ExpandString` and `%USERPROFILE%` unexpanded — and `the other program's command ran N time(s)`. The second `ShowAutoRun`, after the cmd window opened, says a higher number: the other program's command still ran after our entry.
- `background #1F1E1D pixels:` more than 100,000 (G7 measured 556,097 on this VM with the prototype's scheme); `phase1b-applied.png` shows the powerline prompt with its arrows and the welcome box in Windows Terminal, and no error text.
- `Doctor -Label applied`: all eleven checks `ok` (D3 `ok` means `clink.log` recorded `Inject completed` after the apply: the parenthesized entry injected Clink in that interactive window, and the two-file Clink bundle works; D4 `ok` means our entry is first; D8 `ok` means GDI sees CaskaydiaCove NF in the desktop session), `exit 0`.
- `HideOmp`: `oh-my-posh.exe renamed away`. `phase1b-no-omp.png`: the welcome box and a plain prompt, and no error text from cmd or Oh My Posh anywhere in the window (no `is not recognized`, no `cannot find`). `Doctor -Label no-omp`: D1 and D2 `FAIL … [E_RUNTIME_MISSING]`, `exit 1`. `RestoreOmp`: `oh-my-posh.exe is back`.
- the first `Undo`: `Undid …` and `exit 0`; `Compare`: `PHASE1B-IDENTICAL: before and after` — the other program's AutoRun command is back alone, as `REG_EXPAND_SZ`. If only `settings.json` differs, compare the two copies under `C:\cmdstudio\out\phase1b\`: a change Windows Terminal made by itself is not CMD Studio's, but it must be explained in the results before the VM counts as passed.
- `phase1b-cursor-bar.png` and `phase1b-cursor-underscore.png`: zoom on the input line; the cursor is a thin vertical bar, then a line under the cell, and Windows Terminal shows no settings warning. Both undos `exit 0`; `PHASE1B-IDENTICAL: before and after-cursor`.
- the undo with cmd open: either `exit 1` with `E_RUNTIME_MISSING … (close every cmd window, then try again)` and `Everything was rolled back; nothing changed.`, and then `Doctor -Label inuse` shows D1 and D4 `ok` (the theme is fully there, the runtime folder whole); or `exit 0` (Windows let the folder be renamed away) and the doctor shows `skip` for D1. Either is correct; a half-deleted runtime is not (D1 `FAIL`), and neither is an `E_ROLLBACK_INCOMPLETE`. The last undo is `exit 0` (or `Nothing to undo.` after the second case), and `PHASE1B-IDENTICAL: before and after-inuse`.

- [ ] **Step 5: VM-B (Windows 10, no Windows Terminal)**
```powershell
$vm = 'cmdstudio-win10'; $u = 'tester'
Start-Clean $vm $u
& {
  Step $vm $u '-Step OpenWt'
  Step $vm $u '-Step Snapshot -Label before'
  Step $vm $u '-Step Apply -Theme C:\cmdstudio\app\terracotta.cmdtheme'
  Step $vm $u '-Step ShowAutoRun'
  $null = Open-Cmd $vm 'phase1b-applied'
  Step $vm $u '-Step Doctor -Label applied'
  Step $vm $u '-Step CloseCmd'
  Step $vm $u '-Step Undo'
  Step $vm $u '-Step Snapshot -Label after'
  Step $vm $u '-Step Compare -Label after -Against before'
} *>&1 | Tee-Object -FilePath "spikes\vm\tmp\$vm-phase1b.txt"
```
Expected: `OpenWt` says `Windows Terminal is not installed`; `Apply` prints `Warning: Windows Terminal is not installed: only the prompt and the welcome box change.` and `exit 0`; `ShowAutoRun` shows `AutoRun (String): (if exist "…" "…" inject --autorun --quiet --profile "…")` and nothing after it; `phase1b-applied.png` shows the classic console with the box prompt and the welcome box; `Doctor`: D6 `warn [E_WT_NOT_FOUND]`, D7 and D8 `skip`, D11 `skip` (Windows 10), every other check `ok` (D3 among them: the inject in the classic console), `exit 0`; `Undo` `exit 0`; `PHASE1B-IDENTICAL: before and after`.

- [ ] **Step 6: VM-C (Windows 11 in Thai, user `ผู้ทดสอบ ไทย`)** — the data folder's path has Thai letters and a space, so `<clink-launch>` must take the 8.3 path or the ProgramData folder (G19), while `clink.path` keeps the long Thai path. The Thai keyboard is active after sign-in, so switch to English before `Send-Run`:
```powershell
$vm = 'cmdstudio-th'; $u = 'ผู้ทดสอบ ไทย'
Start-Clean $vm $u
& {
  Step $vm $u '-Step OpenWt'
  Step $vm $u '-Step Snapshot -Label before'
  Step $vm $u '-Step Apply -Theme C:\cmdstudio\app\nerd-powerline.cmdtheme'
  Step $vm $u '-Step ShowAutoRun'
  Step $vm $u '-Step ShowClinkPath'
  & $Vbm controlvm $vm keyboardputscancode e0 5b 39 b9 e0 db; Start-Sleep -Seconds 2
  $null = Save-Shot $vm 'phase1b-keyboard'
  $null = Open-Cmd $vm 'phase1b-applied'
  Step $vm $u '-Step Doctor -Label applied'
  Step $vm $u '-Step CloseCmd'
  Step $vm $u '-Step Undo'
  Step $vm $u '-Step Snapshot -Label after'
  Step $vm $u '-Step Compare -Label after -Against before'
} *>&1 | Tee-Object -FilePath "spikes\vm\tmp\$vm-phase1b.txt"
```
Expected: `Apply` `exit 0` — its summary has no ProgramData line when the volume makes 8.3 names (the VM's C: does), so `<clink-launch>` is the short path; `ShowAutoRun` shows our entry alone, its two `<clink-launch>` paths all ASCII (`C:\Users\3607~1\…`) and the profile path with the Thai letters; `ShowClinkPath` prints `clink.path = C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio\generated`; `phase1b-keyboard.png` shows ENG in the tray (if not, press Win+Space again and repeat `Open-Cmd`); `phase1b-applied.png` shows the powerline prompt and the welcome box with no Clink error text — the welcome box is `banner.lua`, which Clink found only through that Thai `clink.path`; `Doctor`: all eleven checks `ok`, D3 among them — the G19 proof with the shipped code — `exit 0`; `Undo` `exit 0`; `PHASE1B-IDENTICAL: before and after`.

- [ ] **Step 7: Put every VM back to `clean`**
```powershell
foreach ($vm in 'cmdstudio-win11', 'cmdstudio-win10', 'cmdstudio-th') {
  if ((Get-VmState $vm) -ne 'poweroff') { & $Vbm controlvm $vm poweroff; Start-Sleep -Seconds 5 }
  & $Vbm snapshot $vm restore clean
}
```

- [ ] **Step 8: Record the results.** If any expectation above failed, stop: write down the VM, the step and the log lines, fix the owning task's code with a test that pins the failure, rebuild and rerun that VM. When everything passed:
1. In spec §0, the G6 row: replace `· ค่า \`bar\` และ \`underscore\` ⬜` with `· ค่า \`bar\` และ \`underscore\` ✅ บน VM-A ใน phase 1b`, but only if both cursor screenshots showed the shapes.
2. In spec §5.4 "AutoRun", replace `· แบบมีวงเล็บ ⬜ ต้องลองซ้ำบน VM ใน phase 1b)` with `· แบบมีวงเล็บ ✅ inject ได้จริงในหน้าต่าง cmd บน VM-A, VM-B และ VM-C ใน phase 1b และบน VM-A คำสั่ง \`if\` ของโปรแกรมอื่นที่ตามหลังยังรันอยู่)`; in §5.4 "Clink", at the end of the line about ` 2>nul`, replace `⬜ ต้องลองซ้ำบน VM ใน phase 1b` with `✅ ลองบน VM-A ใน phase 1b: เมื่อไม่มีไฟล์ OMP หน้าต่าง cmd ไม่มีข้อความ error และ D2 รายงาน`.
3. Append to appendix B, with the day of the run in place of `YYYY-MM-DD`:
```text
| YYYY-MM-DD | phase 1b ผ่านเกณฑ์ของ §8 บน VM-A, VM-B และ VM-C: ใช้ธีม → ตรวจสุขภาพ → ย้อนกลับ แล้ว AutoRun, คีย์ฟอนต์ของผู้ใช้, `settings.json` และรายชื่อฟอนต์ตรงกับก่อนใช้ธีมทุกไบต์ · D3 เห็น `Inject completed` หลังใช้ธีมทั้ง 3 เครื่อง รวมทั้งชื่อผู้ใช้ภาษาไทยบน VM-C (G19) ซึ่ง `clink.path` เป็น path ภาษาไทยและ Clink โหลดสคริปต์จากที่นั่นได้ · ส่วนของเราแบบมีวงเล็บอยู่หน้าสุด และคำสั่ง `if` ของโปรแกรมอื่นที่ตามหลังยังรัน · เมื่อไม่มีไฟล์ OMP หน้าต่าง cmd ไม่มีข้อความ error (` 2>nul`) |
```
4. Commit, with a message that states the outcome on each VM (no attribution lines; drop the sentence of any check that did not pass, and leave its spec marker ⬜):
```text
spike: check apply, doctor and undo on the three test VMs

phase1b-check.ps1 snapshots AutoRun's key, the per-user font key,
settings.json and the font lists, applies a theme with the release
cmdstudio.exe, runs doctor, undoes, and compares. It refuses to run
outside a CMDSTUDIO-* machine (guard.test.ps1 checks it).

Results: VM-A, VM-B and VM-C were byte-identical after undo; D3 saw
"Inject completed" after the apply on all three. Our parenthesized
AutoRun entry went first and another program's `if` command after it
still ran (VM-A); a missing oh-my-posh.exe put no error text in a cmd
window and D2 reported it (VM-A); Clink loaded the scripts through a
Thai clink.path (VM-C); cursorShape bar and underscore render in
Windows Terminal (G6).
```
```bash
git add -- spikes/vm/guest/phase1b-check.ps1 spikes/vm/guest/guard.test.ps1 spikes/vm/README.md docs/superpowers/specs/2026-10-10-cmd-studio-design.md
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```
5. Put the table of results (VM × step: pass, with the pixel count, the `ShowAutoRun` lines and the doctor lines) in the pull request description.

---
## Self-Review (2026-10-12)

**1. Spec coverage**

| Spec | Where |
|---|---|
| §5.2 `adapters` is the only layer that touches the machine | Tasks 1, 3–7, 12; the boundary grep in Task 18 Step 3 |
| §5.3 data folder: `state.json`, `snapshots\` (with `incomplete.json`), `generated\`, `runtime\`, `themes\` | Tasks 8, 9, 13, 14; the spec edit in Task 15 (`logs\` and `drafts\` are listed under "Not in this plan") |
| §5.4 WT: three locations, CST edit, never write a file that does not parse, `defaultProfile` only when chosen | Tasks 7, 11, 13; D6/D7 in Task 15 |
| §5.4 AutoRun: keep the type and raw text, never recreate the key, our parenthesized entry first, other programs' commands byte for byte, a hand edit when the value cannot be changed safely, HKLM read only, another Clink listed before it is turned off | Phase 1a's planner (`plan::autorun`, documented in Task 0); Tasks 4, 11, 13, 14 (`remove_ours`); D4/D5 in Task 15; Task 16 (the list, `E_AUTORUN_WRITE`); VM-A in Task 19 |
| §5.4 `<clink-launch>`, the private ProgramData folder, its ACL and the TOCTOU re-check | Tasks 5, 10, 11; D3 in Task 15; VM-C in Task 19 |
| §5.4 Clink: `clink.path` (`;`-separated) and `init.lua`'s ` 2>nul` popen | Phase 1a's generators; VM-C and VM-A in Task 19 |
| §5.4 fonts: per user, GDI, uninstall only ours | Tasks 6, 11, 14; D8 (GDI only) in Task 15 |
| §5.4 files: atomic writes, versioned runtime folders never overwritten | Tasks 3, 11 |
| §5.5 transactions: journal first, re-runnable ops, the four verify checks, crash recovery, incomplete rollbacks reported with the fix by hand and kept across a restart | Tasks 9, 11, 12, 13, 16 |
| §5.7 CLI, exit codes 0–3 | Task 16 (and the spec edit there) |
| §5.8 doctor D1–D11 | Task 15 |
| §5.9 codes | Tasks 8–16; the six new codes are added to the spec in Task 15 |
| §5.10 only our own programs, fixed arguments, no shell, no window | Task 12 (`WinProcess`), Task 16 (allowed folders) |
| §6.3 revert key by key, `--cleanup` | Task 14, Task 16 (the hook itself is Phase 4) |
| §7 tiers: unit, integration, VMs A/B/C | every task's fakes; Tasks 17–18; Task 19 (the Anaconda and Clink-preinstalled tiers stay ⬜) |
| §8 Phase 1 done-criterion: apply → undo byte-identical on CI and the Windows 10/11 VMs | Task 13 (fakes), Task 17 + 18 (CI runner), Task 19 (VMs) |
| §2.2 lessons 1, 2, 5, 6, 7, 9 | D10; D7 + planner; rename-first restore + VM-A in-use undo; revert before delete in `cleanup`; `CREATE_NO_WINDOW` and VM-only GUI checks; `RegCreateKeyExW` only for a missing key |
| G4/G5, G6, G7, G9, G12, G13, G19 | Task 12; Task 19; Task 19; Task 7; Task 6; Task 4 and Task 19 (the parenthesized entry); Tasks 5, 10, 11, 15, 19 |
| Phase 1a rulings F2, F3, F4, F6, P47, P59, P68, P69 | Task 0; Task 19; Task 0; Task 9; Task 19; Tasks 13, 15, 19; Task 15; Tasks 9, 13, 16 |

Gaps the spec leaves open are closed in the plan and written back into it: Windows Terminal installed but never opened (Review Focus 1; D6 warns, nothing is created; spec §5.8 in Task 15); a new account without the per-user font key (undo deletes the key apply created, so the registry is byte-identical; Tasks 1, 6, 11, checked on the VMs by Task 19's `user-fonts.reg`); where the "fix by hand" text of an incomplete rollback lives (`incomplete.json`, spec §5.3 and §5.5 in Task 15); and how the doctor sees an AutoRun that never reaches our entry (`E_THEME_NOT_LOADED`, spec §5.8 and §5.9 in Task 15).

**2. Placeholder scan.** No "TBD", "TODO", "implement later", "similar to Task N" or unspecified error handling. One thing is filled in at run time on purpose: the date of the VM run in Task 19's appendix row. The draft's "if the shipped Phase 1a code differs, adapt" notes are gone: every Phase 1a name and shape this plan uses was read from `05d4968`.

**3. Type consistency.** Every code block of Tasks 0–17 was compiled together in one scratch copy of `05d4968` with clippy, so the names below agree across tasks: `Paths::{state_file, snapshots, themes, runtime_root}` (Task 8) in Tasks 9, 11, 13, 14; `Applied.clink_log_len` (8) in 13, 15; `SnapshotStore::{new, begin, recovery, ids, manifest, journal, latest_undoable, mark_undone, prune, payload_path, write_incomplete, latest_incomplete}` (9) in 13, 14; `Incomplete`/`Unrestored` (9) in 13, 16; `SystemExecutor::new(System, &Resources)` (11) in 12–14; `fix_by_hand`/`op_label` (11) in 13, 16; `SystemVerifier::new(System, &Paths, &Resources, Option<ClinkLaunch>)` (12) in 13, 14; `Ctx`, `StartReport`, `record_incomplete`, `pending_incomplete` and `actions::testing` (13) in 14–16; `Report::ok()` (14) and `doctor`/`Status` (15) in 16; `run_with`/`Defaults`/`Io`/`EXIT_OK` (16) in 17; `Fonts::{registration_key_exists, delete_registration_key_if_empty}` (1) in 6, 11, 17; Phase 1a's `PlanError::{ForeignClink { existing }, AutoRunNeedsHandEdit { part }}` and `Outcome::RollbackIncomplete { code, reason, failed }` in 13, 14, 16.

**4. Review Focus.** Each of the seven has a named test in its owning task (see the list at the top); 2, 4 and 6 are repeated on real machines in Task 19.

## Execution handoff

Recommended: **subagent-driven**, one fresh implementer and one fresh reviewer per task, then a whole-branch review. The 20 tasks lean hard on each other's exact signatures and on Phase 1a's shipped ones, and a mistake that ships edits users' registry, fonts and Windows Terminal settings. Task 0 edits shipped Phase 1a files, so its reviewer checks each "replace" against the file as it is. Task 19, and the push and pull request after Task 18, are the controller's: they need the VMs and GitHub.
