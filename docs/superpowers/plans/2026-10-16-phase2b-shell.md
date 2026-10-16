# Phase 2b (1 of 3) — The Rust Shell: core additions, the CLI as a library, the app crate, IPC and bindings

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This is the first of the three sub-plans indexed by `2026-10-16-phase2b-app.md`; read the index's Decisions and Spec conflicts once, then work here.

**Goal:** Give CMD Studio its Rust side: the core's `ts` feature and the three additions the app needs, the Phase 1b CLI as a library the app calls, and the app crate `cmdstudio-app` — one exe, a `Backend` with every IPC command of spec §5.7 tested over the core's in-memory machine, the 27 `#[tauri::command]`s, the capability and CSP of §5.10, the committed ts-rs bindings — with the Rust part of the CI app job and the spec's §5.3/§5.7/§5.9 edits.

**Architecture:** `cmdstudio.exe` opens the Tauri window when started bare and runs `cmdstudio_cli::run` when started with arguments. `Backend<M: Machine>` (`app/src-tauri/src/backend`, `library.rs`) holds one `pub` method per command over the core's `System` of adapters: `system_detect` reads what the planner reads; the library keeps my themes in `themes\` and drafts in `drafts\`; `plan_apply`/`apply` reuse the core's `actions::{prepare_apply, run_apply_with}` with a plan digest so the lock is never held while the user reads; `undo_last`, `snapshots_list`, `snapshot_restore`, `revert_all`, `doctor_run`, `doctor_fix`, `settings_get/set`, `diagnostics_copy`, `advanced_read` wrap the core likewise. `ipc.rs` runs each on a blocking thread through `Backend::logged`, which writes one log line per call, and maps every error to `AppError`. Nothing in the app crate computes a color or a prompt: `preview_render`, `palette_generate`, `contrast_check/fix` call the core.

**Tech Stack:** Rust 1.99.0; `tauri = "=2.12.4"`, `tauri-build = "=2.7.3"`, `tauri-plugin-single-instance = "=2.5.4"`, `ts-rs = "=12.0.1"`, `rfd = "=0.17.2"` (`default-features = false`), `windows-sys = "=0.61.2"`, `sha2 = "0.11"`, serde 1, serde_json 1, thiserror 2, uuid 1. All already in this machine's registry cache (`cargo fetch --offline` succeeds with the lock of appendix A).

**Spec:** `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` §5.1, §5.3, §5.5, §5.7, §5.8, §5.9, §5.10, §6.6 (versions in the credits), §7; the index plan's Decisions 1–10, 14, 15 and its Spec conflicts.

**Prerequisite:** main at `061668a`, branch `phase2b-app` checked out with the coordinator's uncommitted spec edits. `resources\` fetched (`scripts/fetch-resources.ps1`). The code of every task below compiled and passed in the scratch copy as the index plan's "Checked before" says: Tasks 1–4 with cargo on the crates themselves; Tasks 6–14 through the check crate of appendix B (`cargo test -p cmdstudio-app` itself was blocked on the dev PC by Application Control; CI is the gate); Task 15's Tauri-dependent files not compiled locally.

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

The five of the index plan that this plan's code owns; each line names the test that pins it here.

1. **A file in `themes\` that is not a theme, or does not validate.** The library lists the good themes and names the bad file with its reason (`Library.problems`), never fails. Task 10: `files_that_are_not_themes_are_problems_not_themes`, `a_theme_that_does_not_validate_is_refused_with_its_field`.
2. **The plan changed between showing and applying.** Not applied; the new plan comes back as `planChanged`. Task 12: `a_plan_that_changed_since_it_was_shown_is_not_applied`, `the_plan_names_each_step_and_lets_the_lock_go`.
3. **Another Clink in AutoRun.** Told apart from other AutoRun commands and never replaced without the user's yes. Task 9: `another_programs_autorun_and_another_clink_are_told_apart`; Task 12: `another_clink_is_asked_about_then_turned_off_on_agreement`.
4. **An apply whose check fails.** Rolled back, said with its code, each rollback step reported. Task 3: `run_apply_with_tells_the_rollback_after_a_failed_check`; Task 12: `a_failed_check_rolls_back_and_says_why`.
5. **A damaged or newer `settings.json`.** Set aside, defaults used, the UI told. Task 7: `a_damaged_or_newer_file_is_set_aside_and_said`, `a_file_that_cannot_be_read_is_a_read_error`.

## How the code is given

- A **new file** is given whole. Rust files with tests are given in two blocks: the test module first (Step "write the failing tests"), then the code that goes above it; the finished file is the second block, one blank line, the first block. `rustfmt` enforces the exact layout.
- A **changed file** is given as a unified diff against `061668a`. Save the block to a file under the scratchpad (keep every trailing space; a blank context line is a single space) and run `git apply --recount <file>` from `<repo>`; if your editor trimmed a space, `git apply --recount --ignore-whitespace <file>`. `git apply --check` first when in doubt.
- Test counts are for `cargo test -p <crate> --lib` unless said otherwise. The app crate's are cumulative: the count the whole crate shows after the task.

## File map

```
.gitignore                                   (Task 1)
crates/core/Cargo.toml                       + ts feature (Task 1)
crates/core/src/{model,palette,preview/mod,doctor,snapshot}.rs   TS derives (Task 1)
crates/core/src/plan.rs, plan/autorun.rs     Paths, AutoRunSummary, autorun_summary (Task 2)
crates/core/src/actions.rs                   Progress, run_apply_with (Task 3)
crates/cli/Cargo.toml, src/lib.rs, src/main.rs (deleted)   (Task 4)
Cargo.toml, Cargo.lock                       workspace member, the app's tree (Task 5)
app/src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json,icons/icon.ico}   (Task 5; [[bin]] in Task 15)
app/src-tauri/src/{command_names,console,lib}.rs   (Task 5; lib.rs grows through Task 15)
scripts/make-icon.ps1                        (Task 5)
app/src-tauri/src/error.rs                   (Task 6)   + spec §5.9 row, crates/core/tests/error_codes.rs
app/src-tauri/src/{settings,logs}.rs         (Task 7)
app/src-tauri/src/backend/mod.rs, machine.rs (Task 8)
app/src-tauri/src/backend/system.rs          (Task 9)
app/src-tauri/src/library.rs                 (Task 10)
app/src-tauri/src/backend/preview.rs         (Task 11)
app/src-tauri/src/backend/apply.rs           (Task 12)
app/src-tauri/src/backend/{doctor,diagnostics}.rs   (Task 13)
app/src-tauri/src/open.rs                    (Task 14)
app/src-tauri/src/{ipc,main}.rs, scripts/export-bindings.ps1, app/ui/src/bindings/*.ts, .github/workflows/ci.yml   (Task 15)
docs/superpowers/specs/2026-10-10-cmd-studio-design.md   §5.3, §5.7 (Task 16)
```

---

### Task 1: The core exports its types to TypeScript (feature `ts`)

The UI consumes the core's public types — `Theme` and what it holds, `Colors`, `Tone`, the preview's `Span`/`Line`/`PreviewContext`, the doctor's `Status`/`Check`, the snapshot's `Manifest`/`Unrestored`/`Incomplete` — as ts-rs bindings. The derives sit behind a feature so a release build of the core is unchanged. ts-rs exports through tests named `export_bindings_*`, which write to `TS_RS_EXPORT_DIR` (the script of Task 15 points it at `app/ui/src/bindings`) or, unset, to `crates/core/bindings/`, which `.gitignore` must skip.

**Files:**
- Modify: `.gitignore` (replace)
- Modify: `crates/core/Cargo.toml`, `crates/core/src/model.rs`, `crates/core/src/palette.rs`, `crates/core/src/preview/mod.rs`, `crates/core/src/doctor.rs`, `crates/core/src/snapshot.rs`
- Test: the generated `export_bindings_*` tests (`cargo test -p cmdstudio-core --lib --features ts -- export_bindings`)

**Interfaces:**
- Consumes: the shipped types as `061668a` has them; serde's `rename_all = "camelCase"` on each, which ts-rs follows (`serde-compat`).
- Produces: feature `ts = ["dep:ts-rs"]`; `#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]` on `Theme`, `SimpleRecipe`, `TerminalSettings`, `CursorShape`, `PromptSettings`, `Layout`, `Segment`, `BannerSettings`, `Target` (model), `Tone`, `Colors` (palette; its seven `Rgb` fields export as `string`, `ansi` as `string[]`), `Span` (`fg: string`, `bg: string | null`), `Line` (exported `as = "Vec<Span>"`: a plain array), `PreviewContext` (`duration_ms` as `number`), `Clock`, `GitState` (preview), `Status` (now also `Deserialize`), `Check` (`details_th` optional-nullable) (doctor), `SnapshotKind`, `ThemeRef`, `Manifest` (`created_ms` as `number`), `Unrestored`, `Incomplete` (snapshot). `Theme.schema` is `ts(skip)` (the `$schema` string is the file's, not the UI's). 23 exported types.

- [ ] **Step 1: Replace `.gitignore`**

```text
/target/
spikes/**/target/
spikes/**/tmp/
/resources/
# ts-rs writes here when TS_RS_EXPORT_DIR is unset; scripts/export-bindings.ps1 sets it to
# app/ui/src/bindings, which is committed.
/crates/core/bindings/
/app/src-tauri/bindings/
# The UI's packages, build output and caches.
/app/node_modules/
/app/ui/node_modules/
/app/ui/dist/
# Tauri's generated schemas and the Tauri CLI's own build output.
/app/src-tauri/gen/
/app/src-tauri/target/
# A local check crate for a machine where the Tauri build is blocked (appendix B); never committed.
/p2b-check/
```

- [ ] **Step 2: Add the feature and the optional dependency**

```diff
--- a/crates/core/Cargo.toml
+++ b/crates/core/Cargo.toml
@@ -13,6 +13,8 @@
 [features]
 # In-memory adapters for tests in other crates (crates/cli). Never enabled in a release build.
 fake = []
+# ts-rs exports of the public types the app's UI consumes (app/ui/src/bindings).
+ts = ["dep:ts-rs"]
 
 [dependencies]
 # wt_edit: Windows Terminal's settings.json through the CST (G9). `serde_json` adds `to_serde_value()`.
@@ -25,6 +27,7 @@
 uuid = { version = "1", features = ["v4", "serde"] }
 schemars = { version = "1", features = ["uuid1"] }
 unicode-width = "0.2"
+ts-rs = { version = "=12.0.1", optional = true, features = ["uuid-impl", "serde-json-impl", "no-serde-warnings"] }
 
 # The registry adapter (`adapters::windows::registry`); `adapters::windows` is cfg(windows) too.
 [target.'cfg(windows)'.dependencies]
```

- [ ] **Step 3: Run the export tests to see there is nothing to export yet**

Run: `cargo test -p cmdstudio-core --lib --features ts -- export_bindings`
Expected: compiles; `running 0 tests` … `test result: ok. 0 passed` (no type carries the derive yet).

- [ ] **Step 4: Put the derives on the types**

`crates/core/src/model.rs`:

```diff
--- a/crates/core/src/model.rs
+++ b/crates/core/src/model.rs
@@ -55,6 +55,7 @@
 
 #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
 #[serde(deny_unknown_fields, rename_all = "camelCase")]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct Theme {
     /// The version of the file format.
     #[schemars(range(min = SCHEMA_VERSION, max = SCHEMA_VERSION))]
@@ -89,6 +90,7 @@
     )]
     #[schemars(with = "String")]
     #[doc(hidden)]
+    #[cfg_attr(feature = "ts", ts(skip))]
     pub schema: (),
 }
 
@@ -127,7 +129,9 @@
 /// Inputs of simple mode, kept so the theme can be edited there again.
 #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
 #[serde(deny_unknown_fields)]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct SimpleRecipe {
+    #[cfg_attr(feature = "ts", ts(type = "string"))]
     pub accent: Rgb,
     #[schemars(range(min = limits::TINT_MIN, max = limits::TINT_MAX))]
     pub tint: f64,
@@ -135,6 +139,7 @@
 
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
 #[serde(deny_unknown_fields, rename_all = "camelCase")]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct TerminalSettings {
     /// Font family name. When it is not installed, Windows Terminal gets the bundled fallback font.
     #[schemars(length(min = limits::FONT_MIN_CHARS, max = limits::FONT_MAX_CHARS))]
@@ -153,6 +158,7 @@
 #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
 #[serde(rename_all = "camelCase")]
 #[non_exhaustive]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub enum CursorShape {
     FilledBox,
     Bar,
@@ -161,6 +167,7 @@
 
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
 #[serde(deny_unknown_fields)]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct PromptSettings {
     pub layout: Layout,
     /// Shown in list order; each segment may appear once.
@@ -172,6 +179,7 @@
 #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
 #[serde(rename_all = "lowercase")]
 #[non_exhaustive]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub enum Layout {
     Box,
     Line,
@@ -181,6 +189,7 @@
 #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
 #[serde(rename_all = "lowercase")]
 #[non_exhaustive]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub enum Segment {
     Path,
     Git,
@@ -195,6 +204,7 @@
 /// `{user}`, `{host}`, `{cwd}`, `{date}` and `{time}`; any other `{word}` stays as written.
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
 #[serde(deny_unknown_fields, rename_all = "camelCase")]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct BannerSettings {
     pub enabled: bool,
     #[schemars(length(max = limits::TITLE_MAX_CHARS))]
@@ -208,6 +218,7 @@
 /// Where a theme is applied (spec §4.2 page 4; CLI `--target`).
 #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
 #[serde(rename_all = "lowercase")]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub enum Target {
     All,
     Wt,
```

`crates/core/src/palette.rs`:

```diff
--- a/crates/core/src/palette.rs
+++ b/crates/core/src/palette.rs
@@ -7,6 +7,7 @@
 
 #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
 #[serde(rename_all = "lowercase")]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub enum Tone {
     Dark,
     Light,
@@ -16,14 +17,23 @@
 /// green, yellow, blue, magenta, cyan, white, then the 8 bright variants.
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
 #[serde(deny_unknown_fields)]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct Colors {
+    #[cfg_attr(feature = "ts", ts(type = "string"))]
     pub background: Rgb,
+    #[cfg_attr(feature = "ts", ts(type = "string"))]
     pub foreground: Rgb,
+    #[cfg_attr(feature = "ts", ts(type = "string"))]
     pub cursor: Rgb,
+    #[cfg_attr(feature = "ts", ts(type = "string"))]
     pub selection: Rgb,
+    #[cfg_attr(feature = "ts", ts(type = "string"))]
     pub accent: Rgb,
+    #[cfg_attr(feature = "ts", ts(type = "string"))]
     pub muted: Rgb,
+    #[cfg_attr(feature = "ts", ts(type = "string"))]
     pub frame: Rgb,
+    #[cfg_attr(feature = "ts", ts(type = "string[]"))]
     pub ansi: [Rgb; 16],
 }
 
```

`crates/core/src/preview/mod.rs`:

```diff
--- a/crates/core/src/preview/mod.rs
+++ b/crates/core/src/preview/mod.rs
@@ -44,9 +44,12 @@
 /// A run of text in one style.
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(rename_all = "camelCase", deny_unknown_fields)]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct Span {
     pub text: String,
+    #[cfg_attr(feature = "ts", ts(type = "string"))]
     pub fg: Rgb,
+    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
     pub bg: Option<Rgb>,
     pub bold: bool,
 }
@@ -66,6 +69,7 @@
 /// same are equal.
 #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(transparent)]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, as = "Vec<Span>"))]
 pub struct Line {
     pub spans: Vec<Span>,
 }
@@ -108,6 +112,7 @@
 /// The app chooses them; the welcome box and the prompt read the same ones.
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(rename_all = "camelCase", deny_unknown_fields)]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct PreviewContext {
     /// The Windows user name: `{user}` in the welcome box, and the prompt's `user`.
     pub user: String,
@@ -129,6 +134,7 @@
     /// The exit code of the command before; the prompt's `status` shows when it is not 0.
     pub exit_code: i32,
     /// How long the command before ran; the prompt's `duration` shows from 2 seconds on.
+    #[cfg_attr(feature = "ts", ts(type = "number"))]
     pub duration_ms: u64,
     /// The width of the window in cells. The box's top line is filled to it, and the right
     /// prompt sits at its right edge. 0 is taken as 1.
@@ -144,6 +150,7 @@
 /// A local date and time, as `os.date("*t")` gives it.
 #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(rename_all = "camelCase", deny_unknown_fields)]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct Clock {
     pub year: u16,
     /// 1 to 12.
@@ -159,6 +166,7 @@
 /// A git repository as the prompt shows it.
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(rename_all = "camelCase", deny_unknown_fields)]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct GitState {
     pub branch: String,
     /// Whether anything changed: an edited, staged or untracked file (`*`, else `✓`).
```

`crates/core/src/doctor.rs`:

```diff
--- a/crates/core/src/doctor.rs
+++ b/crates/core/src/doctor.rs
@@ -17,14 +17,15 @@
 use crate::txn::{Failure, JournalStore};
 use crate::verify::lua_error_line;
 use crate::wt_edit::{check_parses, read_value, WtError, WtKey};
-use serde::Serialize;
+use serde::{Deserialize, Serialize};
 use serde_json::Value;
 use std::ffi::OsStr;
 use std::fmt::Display;
 use std::path::Path;
 
-#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
+#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(rename_all = "camelCase")]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub enum Status {
     Ok,
     Warn,
@@ -42,6 +43,7 @@
 /// for an error (`result_large_err`, 128 bytes).
 #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
 #[serde(rename_all = "camelCase")]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct Check {
     pub id: &'static str,
     pub status: Status,
@@ -50,6 +52,7 @@
     pub message_en: String,
     pub details: String,
     #[serde(skip_serializing_if = "Option::is_none")]
+    #[cfg_attr(feature = "ts", ts(optional = nullable))]
     pub details_th: Option<Box<String>>,
 }
 
```

`crates/core/src/snapshot.rs`:

```diff
--- a/crates/core/src/snapshot.rs
+++ b/crates/core/src/snapshot.rs
@@ -37,6 +37,7 @@
 
 #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(rename_all = "camelCase")]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub enum SnapshotKind {
     Apply,
     Undo,
@@ -45,6 +46,7 @@
 
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(rename_all = "camelCase")]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct ThemeRef {
     pub id: uuid::Uuid,
     pub name: String,
@@ -52,9 +54,11 @@
 
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(rename_all = "camelCase")]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct Manifest {
     pub id: String,
     pub kind: SnapshotKind,
+    #[cfg_attr(feature = "ts", ts(type = "number"))]
     pub created_ms: u64,
     pub theme: Option<ThemeRef>,
     pub target: Option<Target>,
@@ -581,6 +585,7 @@
 /// One op a rollback could not put back, as `incomplete.json` keeps it.
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(rename_all = "camelCase")]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct Unrestored {
     pub op_index: usize,
     /// What the op touched, in plain English (`exec::op_label`).
@@ -608,6 +613,7 @@
 /// targets a rollback may have left as it was.
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(rename_all = "camelCase")]
+#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
 pub struct Incomplete {
     /// The restore point of the transaction.
     pub id: String,
```

- [ ] **Step 5: Run the export tests**

Run: `cargo test -p cmdstudio-core --lib --features ts -- export_bindings`
Expected: `test result: ok. 23 passed`; `crates/core/bindings/` now holds 23 `.ts` files (`Theme.ts`, `Colors.ts`, `Line.ts`, `PreviewContext.ts`, `Check.ts`, `Manifest.ts`, …) and `git status` does not list them.

- [ ] **Step 6: The rest of the core is unchanged**

Run: `cargo test -p cmdstudio-core --lib`
Expected: `test result: ok. 682 passed` (as `061668a`).
Run: `cargo test -p cmdstudio-core --lib --features ts`
Expected: `test result: ok. 705 passed` (682 + 23).
Run: `cargo fmt --all -- --check`, then `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features ts -- -D warnings`
Expected: each clean.

- [ ] **Step 7: Commit**

Stage `.gitignore crates/core/Cargo.toml Cargo.lock crates/core/src/model.rs crates/core/src/palette.rs crates/core/src/preview/mod.rs crates/core/src/doctor.rs crates/core/src/snapshot.rs` (`Cargo.lock` gains `ts-rs 12.0.1` and its dependencies). Message: `core: export the public types to TypeScript behind the ts feature (ts-rs 12.0.1)`.

---

### Task 2: `Paths` for the app's folders, and a summary of AutoRun

The app writes four more things under the data folder (spec §5.3; index Decisions 4, 5) and the planner is the one place that knows the folder's layout, so `Paths` learns them. `system_detect` (Task 9) and the doctor's fixes (Task 13) need to tell the user's other AutoRun commands from another Clink and from our own entry; the planner's `autorun.rs` already parses the value, so the summary lives next to it.

**Files:**
- Modify: `crates/core/src/plan.rs`, `crates/core/src/plan/autorun.rs`
- Test: `the_apps_paths_are_under_the_data_folder`, `the_autorun_summary_sorts_each_command_of_the_value` (in `plan.rs`)

**Interfaces:**
- Consumes: `Paths { data }` and its methods (`state_file`, `lock_file`, `themes`, …), `autorun::{parse, holds_ours, is_foreign_clink}` as shipped (`holds_ours` becomes `pub(crate)`), `Machine` (the planner's read of the machine).
- Produces:
  ```rust
  impl Paths {
      pub fn drafts(&self) -> PathBuf        // <data>\drafts
      pub fn logs(&self) -> PathBuf          // <data>\logs
      pub fn webview(&self) -> PathBuf       // <data>\webview
      pub fn settings_file(&self) -> PathBuf // <data>\settings.json
  }
  #[derive(Clone, Debug, Default, PartialEq, Eq)]
  pub struct AutoRunSummary {
      pub other_commands: Vec<String>,  // the user's own commands, as written, in order
      pub foreign_clink: Vec<String>,   // another Clink's commands (HKCU)
      pub ours_present: bool,           // CMD Studio's guarded entry is there
      pub machine_wide_clink: bool,     // HKLM holds a Clink (spec §5.4)
      pub not_text: bool,               // the value is not REG_SZ/REG_EXPAND_SZ: not read, not touched
  }
  pub fn autorun_summary(machine: &Machine, paths: &Paths) -> AutoRunSummary
  ```

- [ ] **Step 1: Apply the change (tests included)**

`crates/core/src/plan.rs`:

```diff
--- a/crates/core/src/plan.rs
+++ b/crates/core/src/plan.rs
@@ -479,6 +479,78 @@
     })
 }
 
+impl Paths {
+    /// `<data>\drafts`: the editor's autosaved drafts, one `<theme id>.cmdtheme` each (spec
+    /// §3.4, §5.3).
+    pub fn drafts(&self) -> PathBuf {
+        self.data.join("drafts")
+    }
+    /// `<data>\logs`: the app's log, `cmdstudio.log` and its rotated copies (spec §5.3).
+    pub fn logs(&self) -> PathBuf {
+        self.data.join("logs")
+    }
+    /// `<data>\webview`: WebView2's user data folder (its cache and storage). The app gives it
+    /// to its window on purpose, so that it sits in CMD Studio's data folder under a name of CMD
+    /// Studio's choosing, which `--cleanup --data-dir` knows (spec §5.3, §6.3).
+    pub fn webview(&self) -> PathBuf {
+        self.data.join("webview")
+    }
+    /// `<data>\settings.json`: the app's own settings (language, where themes go, updates).
+    /// Not in state.json, because an undo puts state.json back as it was before an apply, and
+    /// the settings must not go back with it.
+    pub fn settings_file(&self) -> PathBuf {
+        self.data.join("settings.json")
+    }
+}
+
+/// What cmd's AutoRun holds, for the wizard's machine check and the app's system report (spec
+/// §4.2 page 2): other programs' commands, which stay; every command that starts another Clink,
+/// which an apply asks about before turning off (spec §5.4); whether a Clink starts from the
+/// machine-wide value, which CMD Studio never changes; and whether the user's value is there
+/// but not plain text, which CMD Studio leaves alone. The same reading as [`plan`] and doctor
+/// D5 use (`autorun::scan`, `autorun::runs_clink`).
+#[derive(Clone, Debug, Default, PartialEq, Eq)]
+pub struct AutoRunSummary {
+    /// Other programs' commands in the user's value, as written, in order.
+    pub other_commands: Vec<String>,
+    /// Commands in the user's value that start another Clink, as written, in order.
+    pub foreign_clink: Vec<String>,
+    /// CMD Studio's own entry is in the user's value.
+    pub ours_present: bool,
+    /// A Clink starts from the machine-wide (HKLM) value.
+    pub machine_wide_clink: bool,
+    /// The user's value is there but is not plain text, so what it holds cannot be told.
+    pub not_text: bool,
+}
+
+pub fn autorun_summary(machine: &Machine, paths: &Paths) -> AutoRunSummary {
+    let mut summary = AutoRunSummary::default();
+    let profile = paths.clink_profile();
+    let profile = profile.to_str();
+    match &machine.autorun_hkcu {
+        RegText::Absent => {}
+        RegText::NotText => summary.not_text = true,
+        RegText::Text(value) => {
+            for part in autorun::scan(value).parts {
+                if profile.is_some_and(|p| autorun::holds_ours(&part.text, p)) {
+                    summary.ours_present = true;
+                } else if autorun::runs_clink(&part.text) {
+                    summary.foreign_clink.push(part.text);
+                } else {
+                    summary.other_commands.push(part.text);
+                }
+            }
+        }
+    }
+    if let RegText::Text(value) = &machine.autorun_hklm {
+        summary.machine_wide_clink = autorun::scan(value)
+            .parts
+            .iter()
+            .any(|p| autorun::runs_clink(&p.text));
+    }
+    summary
+}
+
 #[cfg(test)]
 mod tests {
     use super::autorun::scan;
@@ -1113,4 +1185,44 @@
         let back: Vec<Op> = serde_json::from_value(json).unwrap();
         assert_eq!(back, p.ops);
     }
+
+    #[test]
+    fn the_autorun_summary_sorts_each_command_of_the_value() {
+        let mut m = machine();
+        let anaconda = r#"if exist "C:\Users\me\anaconda3\condabin\conda_hook.bat" "C:\Users\me\anaconda3\condabin\conda_hook.bat""#;
+        let other_clink = r#""C:\clink\clink_x64.exe" inject --autorun"#;
+        m.autorun_hkcu = RegText::Text(format!("{} & {anaconda} & {other_clink}", entry()));
+        m.autorun_hklm = RegText::Text(
+            r#""C:\Program Files\clink\clink_x64.exe" inject --autorun --quiet"#.to_string(),
+        );
+        let s = autorun_summary(&m, &paths());
+        assert!(s.ours_present);
+        assert_eq!(s.other_commands, vec![anaconda.to_string()]);
+        assert_eq!(s.foreign_clink, vec![other_clink.to_string()]);
+        assert!(s.machine_wide_clink);
+        assert!(!s.not_text);
+        // A command that only mentions Clink is another program's own, not a Clink.
+        m.autorun_hkcu = RegText::Text(r"cd /d C:\clink-notes".to_string());
+        m.autorun_hklm = RegText::Absent;
+        let s = autorun_summary(&m, &paths());
+        assert_eq!(s.other_commands, vec![r"cd /d C:\clink-notes".to_string()]);
+        assert!(s.foreign_clink.is_empty() && !s.machine_wide_clink && !s.ours_present);
+        // Nothing, and a value that is not text.
+        let mut m = machine();
+        assert_eq!(autorun_summary(&m, &paths()), AutoRunSummary::default());
+        m.autorun_hkcu = RegText::NotText;
+        assert!(autorun_summary(&m, &paths()).not_text);
+    }
+
+    #[test]
+    fn the_apps_paths_are_under_the_data_folder() {
+        let p = paths();
+        assert_eq!(p.drafts(), PathBuf::from(format!(r"{DATA}\drafts")));
+        assert_eq!(p.logs(), PathBuf::from(format!(r"{DATA}\logs")));
+        assert_eq!(p.webview(), PathBuf::from(format!(r"{DATA}\webview")));
+        assert_eq!(
+            p.settings_file(),
+            PathBuf::from(format!(r"{DATA}\settings.json"))
+        );
+    }
 }
```

`crates/core/src/plan/autorun.rs`:

```diff
--- a/crates/core/src/plan/autorun.rs
+++ b/crates/core/src/plan/autorun.rs
@@ -334,7 +334,7 @@
 }
 
 /// Whether `text` holds our profile path, in any case: every form of our entry holds it.
-fn holds_ours(text: &str, profile: &str) -> bool {
+pub(crate) fn holds_ours(text: &str, profile: &str) -> bool {
     text.to_lowercase().contains(&profile.to_lowercase())
 }
 
```

- [ ] **Step 2: Run the two tests**

Run: `cargo test -p cmdstudio-core --lib -- the_apps_paths_are_under_the_data_folder the_autorun_summary_sorts_each_command_of_the_value`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 3: See that the path test bites**

In `plan.rs` change `self.data.join("drafts")` to `self.data.join("draft")`, run the same command.
Expected: `the_apps_paths_are_under_the_data_folder` FAILS (the asserted `\drafts` path). Put `"drafts"` back; run again; 2 passed.

- [ ] **Step 4: The whole core**

Run: `cargo test -p cmdstudio-core --lib` → `684 passed`; `cargo test -p cmdstudio-core --lib --features ts` → `707 passed`. The four clippy commands and `cargo fmt --all -- --check` of the Global Constraints: clean.

- [ ] **Step 5: Commit**

Stage `crates/core/src/plan.rs crates/core/src/plan/autorun.rs`. Message: `core: Paths for drafts, logs, webview and settings.json; autorun_summary for the app`.

---

### Task 3: The core reports each step of an apply (`run_apply_with`)

Spec §5.7 names the `apply-progress` event and §4.2 page 5 shows progress step by step. The transaction runner (`txn::run`) already calls an `Executor` per operation and a `Verifier` at the end; `run_apply_with` wraps both to count and tell, and `run_apply` becomes the no-op-listener case so Phase 1b's callers (the CLI) do not change.

**Files:**
- Modify: `crates/core/src/actions.rs`
- Test: `run_apply_with_tells_each_step_in_order`, `run_apply_with_tells_the_rollback_after_a_failed_check`

**Interfaces:**
- Consumes: `actions::{prepare_apply, run_apply, Prepared, Ctx, SystemExecutor, SystemVerifier}`, `txn::{run, Executor, Verifier, Failure, Outcome, Stage}`, `plan::Op` as shipped.
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq, Eq)]
  pub enum Progress {
      BackingUp { step: usize, total: usize },
      Applying { step: usize, total: usize, op: Op },
      Verifying,
      RollingBack { step: usize, total: usize },
  }
  pub fn run_apply(sys: System<'_>, ctx: &Ctx, prepared: Prepared) -> Outcome   // unchanged signature; = run_apply_with(.., &mut |_| {})
  pub fn run_apply_with(sys: System<'_>, ctx: &Ctx, prepared: Prepared, progress: &mut dyn FnMut(Progress)) -> Outcome
  ```
  `total` is the number of operations of the plan; `BackingUp` is told once per operation captured, `Applying` once per operation written, `Verifying` once before the checks, `RollingBack` once per operation restored (only after a failure).

- [ ] **Step 1: Apply the change (tests included)**

```diff
--- a/crates/core/src/actions.rs
+++ b/crates/core/src/actions.rs
@@ -17,7 +17,7 @@
     UnreadableKind, Unrestored, KEEP,
 };
 use crate::state::{Applied, AutoRunRecord, FontRecord, RegValueRecord, State, StateError};
-use crate::txn::{self, Failure, Outcome, Stage};
+use crate::txn::{self, Executor, Failure, Outcome, Stage, Verifier};
 use crate::verify::SystemVerifier;
 use crate::wt_edit::{apply_edit, WtEdit, WtError};
 use std::collections::BTreeMap;
@@ -353,6 +353,90 @@
     })
 }
 
+/// What an apply is doing, step by step, for the app's progress bar (spec §4.2 page 5). The
+/// steps are the transaction's ops in order: the plan's, then the write of state.json.
+#[derive(Clone, Debug, PartialEq, Eq)]
+pub enum Progress {
+    /// Reading what the target of op `step` of `total` holds, before anything changes.
+    BackingUp { step: usize, total: usize },
+    /// Applying op `step` of `total`.
+    Applying { step: usize, total: usize, op: Op },
+    /// Every op has ended; the result is being checked (spec §5.5).
+    Verifying,
+    /// Putting back the target of op `step` of `total`, newest first, after a failure.
+    RollingBack { step: usize, total: usize },
+}
+
+/// The one listener of an apply's steps, shared by its executor and its verifier.
+type Listener<'p> = std::cell::RefCell<&'p mut dyn FnMut(Progress)>;
+
+fn tell(on: &Listener<'_>, progress: Progress) {
+    let mut listener = on.borrow_mut();
+    (*listener)(progress);
+}
+
+/// The executor of an apply, telling each step before it runs it.
+struct Watched<'a, 'p, 'q> {
+    inner: SystemExecutor<'a>,
+    total: usize,
+    captured: usize,
+    applied: usize,
+    restored: usize,
+    on: &'q Listener<'p>,
+}
+
+impl Executor for Watched<'_, '_, '_> {
+    fn capture(&mut self, op: &Op) -> Result<Option<Vec<u8>>, Failure> {
+        self.captured += 1;
+        tell(
+            self.on,
+            Progress::BackingUp {
+                step: self.captured,
+                total: self.total,
+            },
+        );
+        self.inner.capture(op)
+    }
+
+    fn apply(&mut self, op: &Op) -> Result<(), Failure> {
+        self.applied += 1;
+        tell(
+            self.on,
+            Progress::Applying {
+                step: self.applied,
+                total: self.total,
+                op: op.clone(),
+            },
+        );
+        self.inner.apply(op)
+    }
+
+    fn restore(&mut self, op: &Op, prior: &Option<Vec<u8>>) -> Result<(), Failure> {
+        self.restored += 1;
+        tell(
+            self.on,
+            Progress::RollingBack {
+                step: self.restored,
+                total: self.total,
+            },
+        );
+        self.inner.restore(op, prior)
+    }
+}
+
+/// The verifier of an apply, telling that the check starts.
+struct WatchedVerifier<'a, 'p, 'q> {
+    inner: SystemVerifier<'a>,
+    on: &'q Listener<'p>,
+}
+
+impl Verifier for WatchedVerifier<'_, '_, '_> {
+    fn verify(&mut self, ops: &[Op]) -> Result<(), Failure> {
+        tell(self.on, Progress::Verifying);
+        self.inner.verify(ops)
+    }
+}
+
 /// Runs a prepared apply as one transaction, its new state.json the last op. After it ends, the
 /// font files its rollback uninstalled but could not delete are noted in state.json for a later
 /// delete; a rollback that could neither delete nor note one ends `RollbackIncomplete` with that
@@ -361,6 +445,18 @@
 /// `prepare_apply` took is let go when this returns. Given a data folder or bundled files other
 /// than those it was prepared with, it changes nothing and ends `RolledBack` (`E_DATA_WRITE`).
 pub fn run_apply(sys: System<'_>, ctx: &Ctx, prepared: Prepared) -> Outcome {
+    run_apply_with(sys, ctx, prepared, &mut |_| {})
+}
+
+/// [`run_apply`], telling `progress` about each step as it starts (see [`Progress`]): the app's
+/// progress bar reads it (spec §4.2 page 5). The listener runs on the thread of the apply,
+/// before each step, and never after this returns.
+pub fn run_apply_with(
+    sys: System<'_>,
+    ctx: &Ctx,
+    prepared: Prepared,
+    progress: &mut dyn FnMut(Progress),
+) -> Outcome {
     let Prepared {
         plan,
         ops,
@@ -391,12 +487,23 @@
     let store = SnapshotStore::new(sys.fs, &root);
     let id = manifest.id.clone();
     let mut journal = store.begin(manifest);
-    let mut exec = SystemExecutor::new(sys, &ctx.resources);
-    let mut verifier = SystemVerifier::new(sys, &ctx.paths, &ctx.resources, Some(plan.launch));
+    let on: Listener<'_> = std::cell::RefCell::new(progress);
+    let mut exec = Watched {
+        inner: SystemExecutor::new(sys, &ctx.resources),
+        total: ops.len(),
+        captured: 0,
+        applied: 0,
+        restored: 0,
+        on: &on,
+    };
+    let mut verifier = WatchedVerifier {
+        inner: SystemVerifier::new(sys, &ctx.paths, &ctx.resources, Some(plan.launch)),
+        on: &on,
+    };
     let outcome = txn::run(&id, ops.clone(), &mut exec, &mut verifier, &mut journal);
     // Only a rollback uninstalls a font (`SystemExecutor::apply` never does), so a committed
     // apply holds no file and `after_held` has nothing to give back for one.
-    let (outcome, _) = after_held(sys, ctx, &ops, &exec.held_files(), outcome);
+    let (outcome, _) = after_held(sys, ctx, &ops, &exec.inner.held_files(), outcome);
     // A list that could not be saved is said, and rebuilt from the journal at the next start.
     let outcome = match record_incomplete(&store, &id, &outcome) {
         Ok(()) => outcome,
@@ -2794,4 +2901,68 @@
             assert_eq!(m.autorun.read_user().unwrap(), Some(value));
         }
     }
+
+    #[test]
+    fn run_apply_with_tells_each_step_in_order() {
+        let (m, ctx) = machine();
+        let theme = crate::presets::terracotta();
+        let prepared = prepare_apply(m.system(), &ctx, &theme, &request(Target::All)).unwrap();
+        let ops = prepared.ops.clone();
+        let total = ops.len();
+        let mut seen = Vec::new();
+        let outcome = run_apply_with(m.system(), &ctx, prepared, &mut |p| seen.push(p));
+        assert_eq!(outcome, Outcome::Committed);
+        let mut want: Vec<Progress> = (1..=total)
+            .map(|step| Progress::BackingUp { step, total })
+            .collect();
+        want.extend(ops.iter().enumerate().map(|(i, op)| Progress::Applying {
+            step: i + 1,
+            total,
+            op: op.clone(),
+        }));
+        want.push(Progress::Verifying);
+        assert_eq!(seen, want);
+        // The last op is the write of state.json.
+        assert!(matches!(
+            ops.last(),
+            Some(Op::WriteFile { path, .. }) if *path == ctx.paths.state_file()
+        ));
+    }
+
+    #[test]
+    fn run_apply_with_tells_the_rollback_after_a_failed_check() {
+        let (m, ctx) = machine();
+        // Oh My Posh does not render the config: verify fails and everything is put back.
+        m.process.answer("oh-my-posh.exe", "print", 1, "");
+        let theme = crate::presets::terracotta();
+        let prepared = prepare_apply(m.system(), &ctx, &theme, &request(Target::All)).unwrap();
+        let total = prepared.ops.len();
+        let mut seen = Vec::new();
+        let outcome = run_apply_with(m.system(), &ctx, prepared, &mut |p| seen.push(p));
+        assert!(
+            matches!(
+                outcome,
+                Outcome::RolledBack {
+                    code: "E_VERIFY_OMP",
+                    ..
+                }
+            ),
+            "{outcome:?}"
+        );
+        let verifying = seen.iter().position(|p| *p == Progress::Verifying).unwrap();
+        let after: Vec<&Progress> = seen.iter().skip(verifying + 1).collect();
+        let want: Vec<Progress> = (1..=total)
+            .map(|step| Progress::RollingBack { step, total })
+            .collect();
+        assert_eq!(after, want.iter().collect::<Vec<_>>());
+        // The plain `run_apply` is the same apply without a listener.
+        let prepared = prepare_apply(m.system(), &ctx, &theme, &request(Target::All)).unwrap();
+        assert!(matches!(
+            run_apply(m.system(), &ctx, prepared),
+            Outcome::RolledBack {
+                code: "E_VERIFY_OMP",
+                ..
+            }
+        ));
+    }
 }
```

- [ ] **Step 2: Run the two tests**

Run: `cargo test -p cmdstudio-core --lib -- run_apply_with_tells`
Expected: `test result: ok. 2 passed`. The second test makes the fake Oh My Posh answer `print` with status 1, so the verifier fails with `E_VERIFY_OMP` and the listener sees `RollingBack {1..total}` after `Verifying`.

- [ ] **Step 3: The whole core, the CLI and the integration tests**

Run: `cargo test -p cmdstudio-core --lib` → `686 passed`; `--features ts` → `709 passed`; `cargo test -p cmdstudio-cli` → `46 passed` (unchanged: the CLI's `run_apply` call is the same); `cargo test --workspace` → every integration test as before (`english_in_cmd`, `golden`, `hostile_themes`, `schema_file`, `error_codes`, `banner_lua`, `banner_parity`, `omp_render`, `palette_props`; `parity` needs `resources\oh-my-posh.exe` and prints `SKIPPED` only with `CMDSTUDIO_ALLOW_PARITY_SKIP=1`, which you never set). Clippy and fmt clean.

- [ ] **Step 4: Commit**

Stage `crates/core/src/actions.rs`. Message: `core: run_apply_with reports each step of an apply for the app's progress event`.

---

### Task 4: The CLI becomes a library the app calls, and `--cleanup` learns the app's entries

Spec §5.7: the CLI is the same exe as the app, so `crates/cli` keeps `run`/`run_with` and loses its `[[bin]]`; `app/src-tauri/src/main.rs` (Task 15) calls it. `--cleanup --data-dir` goes on only when every entry of the folder is CMD Studio's (§5.7); after this phase the app also writes `drafts\`, `logs\`, `webview\` (WebView2's folder, index Decision 5), `settings.json`, its set-aside copy and its temp file, so the lists grow, and the folder name `ai.resincore.cmdstudio` becomes a constant the app shares.

**Files:**
- Modify: `crates/cli/Cargo.toml`, `crates/cli/src/lib.rs`
- Delete: `crates/cli/src/main.rs`
- Test: `cleanup_with_a_data_dir_takes_the_apps_own_entries_for_cmd_studios` (in `lib.rs`); the English-only test's line that embedded `main.rs` goes with the file.

**Interfaces:**
- Consumes: `cmdstudio_core::plan::Paths::{drafts, logs, webview, settings_file}` (Task 2).
- Produces: `pub const DATA_DIR_NAME: &str = "ai.resincore.cmdstudio";` (used by `run` for the default data folder and by the app's `machine.rs`); `put_there_by_cmd_studio` accepts `drafts\`, `logs\`, `webview\` (and everything under them), `settings.json`, `settings.json.damaged-<digits>` and `.settings.json.<hex32>.cmdstudio-tmp`; `cmd_studios_own_file` is true for `state.json`, `settings.json`, `cmdstudio.lock` and a `.damaged-<digits>` copy of the first two; the refusal text reads "(cmdstudio.lock, state.json, settings.json or a .damaged-<number> copy of them)". `run`, `run_with`, `Io`, `EXIT_*` unchanged.

- [ ] **Step 1: Apply the change (test included) and delete the binary's file**

`crates/cli/Cargo.toml`:

```diff
--- a/crates/cli/Cargo.toml
+++ b/crates/cli/Cargo.toml
@@ -11,10 +11,6 @@
 [lib]
 name = "cmdstudio_cli"
 
-[[bin]]
-name = "cmdstudio"
-path = "src/main.rs"
-
 [dependencies]
 cmdstudio-core = { path = "../core" }
 serde_json = "1"
```

`crates/cli/src/lib.rs`:

```diff
--- a/crates/cli/src/lib.rs
+++ b/crates/cli/src/lib.rs
@@ -47,6 +47,11 @@
 /// A rollback could not restore everything: the user must fix what it lists by hand.
 pub const EXIT_INCOMPLETE: i32 = 3;
 
+/// The data folder's name under `%LOCALAPPDATA%` (spec §5.3, G10): what Tauri's
+/// `app_local_data_dir()` gives for the identifier `ai.resincore.cmdstudio`, spelled out so the
+/// CLI and the app agree without Tauri.
+pub const DATA_DIR_NAME: &str = "ai.resincore.cmdstudio";
+
 pub struct Io<'a> {
     pub out: &'a mut dyn Write,
     pub err: &'a mut dyn Write,
@@ -671,7 +676,7 @@
             if !only_themes {
                 self.err(format!(
                     "cmdstudio: {} holds none of CMD Studio's own files (cmdstudio.lock, \
-                     state.json or state.json.damaged-<number>), so it may not be CMD \
+                     state.json, settings.json or a .damaged-<number> copy of them), so it may not be CMD \
                      Studio's data folder, and --cleanup would delete what is in it. Nothing \
                      was deleted; check the path after --data-dir.",
                     data.display()
@@ -931,14 +936,18 @@
 
 /// Whether an entry directly in the data folder is one CMD Studio itself puts there, of the
 /// kind it writes, as the core's `Paths` and its writers make them:
-/// - the folders `generated\`, `clink-profile\`, `runtime\`, `snapshots\` and `themes\`;
-/// - the files `state.json` and `cmdstudio.lock` (made by `Fs::try_lock`);
-/// - `state.json.damaged-<ms>`: a damaged state.json that Revert everything or `--cleanup` set
-///   aside (`cleanup::load_state`);
-/// - `.state.json.<32 hex digits>.cmdstudio-tmp`: the temp file `WinFs::write_atomic` writes
-///   beside state.json, left behind when a run is cut off in that write. state.json is the only
-///   file CMD Studio writes directly in the data folder: the generated files, Clink's profile,
-///   the runtimes and the restore points, with their own temp files, are in the folders above.
+/// - the folders `generated\`, `clink-profile\`, `runtime\`, `snapshots\`, `themes\`, and the
+///   app's `drafts\` (autosaved drafts), `logs\` (its log) and `webview\` (WebView2's own data,
+///   put there by the app on purpose; spec §5.3);
+/// - the files `state.json`, `settings.json` (the app's settings) and `cmdstudio.lock` (made by
+///   `Fs::try_lock`);
+/// - `state.json.damaged-<ms>` and `settings.json.damaged-<ms>`: a damaged file that Revert
+///   everything, `--cleanup` or the app set aside (`cleanup::load_state`, the app's settings);
+/// - `.state.json.<32 hex digits>.cmdstudio-tmp`, and the same for `settings.json`: the temp
+///   file `WinFs::write_atomic` writes beside the file, left behind when a run is cut off in
+///   that write. state.json and settings.json are the only files CMD Studio writes directly in
+///   the data folder: the generated files, Clink's profile, the runtimes, the restore points, the
+///   drafts and the log, with their own temp files, are in the folders above.
 ///
 /// Names compare without regard to ASCII letter case, as Windows does.
 fn put_there_by_cmd_studio(paths: &Paths, entry: &DirEntry) -> bool {
@@ -953,6 +962,9 @@
             paths.runtime_root(),
             paths.snapshots(),
             paths.themes(),
+            paths.drafts(),
+            paths.logs(),
+            paths.webview(),
         ]
         .into_iter()
         .any(named);
@@ -960,40 +972,39 @@
     if cmd_studios_own_file(paths, entry) {
         return true;
     }
-    let state_file = paths.state_file();
-    let Some(state) = state_file.file_name().and_then(|n| n.to_str()) else {
-        return false;
-    };
-    let (name, state) = (name.to_ascii_lowercase(), state.to_ascii_lowercase());
-    name.strip_prefix(&format!(".{state}."))
-        .and_then(|rest| rest.strip_suffix(".cmdstudio-tmp"))
-        .is_some_and(|hex| hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
+    let name = name.to_ascii_lowercase();
+    [paths.state_file(), paths.settings_file()]
+        .iter()
+        .filter_map(|file| file.file_name().and_then(|n| n.to_str()))
+        .any(|file| {
+            name.strip_prefix(&format!(".{}.", file.to_ascii_lowercase()))
+                .and_then(|rest| rest.strip_suffix(".cmdstudio-tmp"))
+                .is_some_and(|hex| hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
+        })
 }
 
 /// Whether an entry directly in the data folder is one of the files that only CMD Studio
 /// itself writes there, and that every data folder it used holds until a `--cleanup` empties
 /// it: `cmdstudio.lock` (made by the first command that takes the lock, before any other write,
-/// and left when the lock is let go), `state.json`, or `state.json.damaged-<ms>` (a damaged
-/// state.json that Revert everything or `--cleanup` set aside). Names compare without regard
-/// to ASCII letter case. The kind is the caller's to check.
+/// and left when the lock is let go), `state.json`, `settings.json` (the app writes it the first
+/// time a setting changes), or a damaged copy of the last two set aside as `<name>.damaged-<ms>`.
+/// Names compare without regard to ASCII letter case. The kind is the caller's to check.
 fn cmd_studios_own_file(paths: &Paths, entry: &DirEntry) -> bool {
-    if [paths.state_file(), paths.lock_file()]
-        .into_iter()
-        .any(|ours| same_name(&ours, &entry.path))
-    {
+    let own = [paths.state_file(), paths.settings_file(), paths.lock_file()];
+    if own.iter().any(|ours| same_name(ours, &entry.path)) {
         return true;
     }
-    let state_file = paths.state_file();
-    match (
-        entry.path.file_name().and_then(|n| n.to_str()),
-        state_file.file_name().and_then(|n| n.to_str()),
-    ) {
-        (Some(name), Some(state)) => name
-            .to_ascii_lowercase()
-            .strip_prefix(&format!("{}.damaged-", state.to_ascii_lowercase()))
-            .is_some_and(|ms| !ms.is_empty() && ms.bytes().all(|b| b.is_ascii_digit())),
-        _ => false,
-    }
+    let Some(name) = entry.path.file_name().and_then(|n| n.to_str()) else {
+        return false;
+    };
+    let name = name.to_ascii_lowercase();
+    [paths.state_file(), paths.settings_file()]
+        .iter()
+        .filter_map(|file| file.file_name().and_then(|n| n.to_str()))
+        .any(|file| {
+            name.strip_prefix(&format!("{}.damaged-", file.to_ascii_lowercase()))
+                .is_some_and(|ms| !ms.is_empty() && ms.bytes().all(|b| b.is_ascii_digit()))
+        })
 }
 
 /// The record of a restore point set aside by hand while its journal could not be read: one
@@ -1057,7 +1068,7 @@
         .and_then(|p| p.parent().map(PathBuf::from))
         .unwrap_or_default();
     let defaults = Defaults {
-        data_dir: local.join("ai.resincore.cmdstudio"),
+        data_dir: local.join(DATA_DIR_NAME),
         resources: exe_dir.join("resources"),
         now_ms: now_ms(),
     };
@@ -1741,7 +1752,7 @@
     #[test]
     fn a_folder_with_only_cmd_studio_named_folders_but_none_of_its_files_is_refused() {
         // Folders named like CMD Studio's are not enough: one of its own files must be there
-        // (cmdstudio.lock, state.json or a damaged state.json it set aside).
+        // (cmdstudio.lock, state.json, settings.json or a .damaged-<number> copy of them).
         for (m, d) in english_and_thai_windows() {
             let folder = r"D:\work";
             m.fs.put(format!(r"{folder}\runtime\tool.exe"), b"mine");
@@ -1753,7 +1764,12 @@
             ] {
                 let (code, out, err) = cli(&m, &d, &args);
                 assert_eq!(code, EXIT_INVALID, "{args:?}: {out}{err}");
-                for name in ["cmdstudio.lock", "state.json", "state.json.damaged-"] {
+                for name in [
+                    "cmdstudio.lock",
+                    "state.json",
+                    "settings.json",
+                    ".damaged-<number>",
+                ] {
                     assert!(err.contains(name), "{name}: {err}");
                 }
                 assert!(err.contains("none of CMD Studio's own files"), "{err}");
@@ -2332,7 +2348,6 @@
         for (file, source) in [
             ("lib.rs", include_str!("lib.rs")),
             ("args.rs", include_str!("args.rs")),
-            ("main.rs", include_str!("main.rs")),
         ] {
             let source = source.replace("\r\n", "\n");
             let code = source.split("\n#[cfg(test)]\nmod tests {").next().unwrap();
@@ -2352,4 +2367,53 @@
         assert!(out.starts_with("Usage:"));
         assert_eq!(cli(&m, &d, &["frobnicate"]).0, EXIT_INVALID);
     }
+
+    #[test]
+    fn cleanup_with_a_data_dir_takes_the_apps_own_entries_for_cmd_studios() {
+        // The app writes drafts\, logs\, webview\ (WebView2's data) and settings.json into the
+        // data folder (spec §5.3). --cleanup --data-dir must know them, or a real data folder of
+        // the app is refused as not CMD Studio's.
+        let (m, d) = machine();
+        let other = r"C:\Users\tester\other";
+        m.fs.put(format!(r"{other}\themes\mine.cmdtheme"), b"{}");
+        assert_eq!(
+            cli(&m, &d, &["apply", THEME, "--data-dir", other]).0,
+            EXIT_OK
+        );
+        for file in [
+            r"drafts\1e9c0c7e-0e0b-4c71-9a4b-6a0c5d7f1a22.cmdtheme",
+            r"logs\cmdstudio.log",
+            r"logs\cmdstudio.log.1",
+            r"webview\EBWebView\Default\Cache\data_0",
+            "settings.json",
+            "settings.json.damaged-1791694709000",
+            ".settings.json.0123456789abcdef0123456789abcdef.cmdstudio-tmp",
+            ".state.json.0123456789abcdef0123456789abcdef.cmdstudio-tmp",
+        ] {
+            m.fs.put(format!(r"{other}\{file}"), b"x");
+        }
+        let (code, out, err) = cli(&m, &later(&d, 1), &["--cleanup", "--data-dir", other]);
+        assert_eq!(code, EXIT_OK, "{out}{err}");
+        assert_eq!(
+            m.fs.list_dir(Path::new(other)).unwrap().len(),
+            1,
+            "only the themes folder stays"
+        );
+        // A folder with only settings.json and the app's folders is CMD Studio's too: the app
+        // writes them before any theme is applied.
+        let fresh = r"C:\Users\tester\fresh";
+        m.fs.put(format!(r"{fresh}\settings.json"), b"{}");
+        m.fs.put(format!(r"{fresh}\logs\cmdstudio.log"), b"");
+        let (code, out, err) = cli(&m, &later(&d, 2), &["--cleanup", "--data-dir", fresh]);
+        assert_eq!(code, EXIT_OK, "{out}{err}");
+        assert!(!m.fs.exists(Path::new(fresh)));
+        // A file CMD Studio never writes there is still refused, nothing deleted.
+        m.fs.put(format!(r"{fresh}\settings.json"), b"{}");
+        m.fs.put(format!(r"{fresh}\webview.txt"), b"x");
+        let before = m.fs.snapshot();
+        let (code, _, err) = cli(&m, &later(&d, 3), &["--cleanup", "--data-dir", fresh]);
+        assert_eq!(code, EXIT_INVALID, "{err}");
+        assert!(err.contains("\"webview.txt\""), "{err}");
+        assert_eq!(m.fs.snapshot(), before);
+    }
 }
```

Then: `git rm crates/cli/src/main.rs`.

- [ ] **Step 2: Run the new test**

Run: `cargo test -p cmdstudio-cli --lib -- cleanup_with_a_data_dir_takes_the_apps_own_entries_for_cmd_studios`
Expected: `test result: ok. 1 passed`. The test fills a data folder with `themes\mine.cmdtheme`, `drafts\…`, `logs\…`, `webview\EBWebView\…`, `settings.json`, a damaged copy and a temp file: `--cleanup --data-dir` answers `EXIT_OK` and leaves only `themes\`; a folder with only `settings.json` and `logs\` is cleaned and removed; a stray `webview.txt` is refused with `EXIT_INVALID`.

- [ ] **Step 3: The whole CLI and the workspace**

Run: `cargo test -p cmdstudio-cli --lib` → `47 passed`. `cargo test --workspace` → green; `cargo build --workspace` builds no `cmdstudio.exe` any more (there is no binary until Task 15). Clippy and fmt clean.

- [ ] **Step 4: Commit**

Stage `crates/cli/Cargo.toml crates/cli/src/lib.rs crates/cli/src/main.rs`. Message: `cli: a library only (the app is the exe); --cleanup knows drafts, logs, webview and settings.json`.

---

### Task 5: The app crate: manifest, Tauri config, capability, icon, and the two modules that need no backend

The crate `cmdstudio-app` joins the workspace. `build.rs` turns the command names into ACL permissions (`allow-<command>`), so `capabilities/default.json` is the one list of what the window may call, and writes a placeholder `ui/dist/index.html` when the UI is not built, so the Rust side builds and tests without Node. The icon is generated by a script (System.Drawing), because `tauri-build` needs `icons/icon.ico` for the exe's resource. `lib.rs` starts with the two modules that need nothing else and grows one `pub mod` line per task until Task 15 gives it `run_gui`. The `[[bin]]` section comes in Task 15 with `main.rs`.

**Files:**
- Modify: `Cargo.toml` (workspace members)
- Create: `app/src-tauri/Cargo.toml`, `app/src-tauri/build.rs`, `app/src-tauri/tauri.conf.json`, `app/src-tauri/capabilities/default.json`, `app/src-tauri/src/command_names.rs`, `app/src-tauri/src/console.rs`, `app/src-tauri/src/lib.rs`, `scripts/make-icon.ps1`, `app/src-tauri/icons/icon.ico` (generated)
- Test: `cargo build -p cmdstudio-app` (the Tauri build step accepts the config and the commands); the module tests come with later tasks.

**Interfaces:**
- Consumes: `tauri_build::{try_build, Attributes, AppManifest}` 2.7.3 (`AppManifest::new().commands(&[&str])` declares the app's commands; a command used by `generate_handler!` but missing here fails the build with an ACL error); `WindowConfig.create: false` (tauri-utils 2.10.2: the window is not created at startup; Task 15 builds it with a data directory); the CSP of the Global Constraints.
- Produces: `pub const COMMANDS: &[&str]` with the 27 names (`command_names.rs`, also `include!`d by `build.rs`); `console::attach_parent()`, `console::fatal(&str)`; the crate layout the later tasks fill.

- [ ] **Step 1: Add the member**

```diff
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1,6 +1,6 @@
 [workspace]
 resolver = "2"
-members = ["crates/core", "crates/cli"]
+members = ["crates/core", "crates/cli", "app/src-tauri"]
 
 [workspace.package]
 version = "0.1.0"
```

- [ ] **Step 2: Write the manifest** (`app/src-tauri/Cargo.toml`; the `[[bin]]` section is added in Task 15)

```toml
[package]
name = "cmdstudio-app"
version.workspace = true
edition.workspace = true
license.workspace = true
# The app and the CLI in one exe (spec §5.7): `cmdstudio.exe` opens the window when started
# without arguments and runs `cmdstudio_cli::run` when started with any.
publish = false

[lints]
workspace = true

[lib]
name = "cmdstudio_app"

[build-dependencies]
# Tauri's build step: the ACL permissions of the app's commands (`allow-<command>`), the icon
# resource, the config check. Pinned like every version here (spec §6.4).
tauri-build = { version = "=2.7.3", features = [] }

[dependencies]
# `ts`: the core's public types carry ts-rs derives, exported to app/ui/src/bindings.
cmdstudio-core = { path = "../../crates/core", features = ["ts"] }
cmdstudio-cli = { path = "../../crates/cli" }
# Tauri 2 (G10: 2.12.x). Default features: wry, compression, common-controls-v6, dynamic-acl.
tauri = { version = "=2.12.4", features = [] }
# One window: a second start focuses the first (spec §5.7 "single instance"). No JS commands.
tauri-plugin-single-instance = "=2.5.4"
# The native Open and Save dialogs for .cmdtheme files, shown from the main thread with the
# window as parent. Its default features are Linux portals; on Windows it needs none.
rfd = { version = "=0.17.2", default-features = false }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }
sha2 = "0.11"
# TypeScript bindings (G11). `no-serde-warnings`: ts-rs prints a note at compile time for each
# serde attribute it does not know (deny_unknown_fields, deserialize_with); none of them changes
# a generated type, and `Line` overrides its `transparent` with `ts(as)`.
ts-rs = { version = "=12.0.1", features = ["uuid-impl", "serde-json-impl", "no-serde-warnings"] }

[target.'cfg(windows)'.dependencies]
# console.rs: AttachConsole for the CLI mode of a windows-subsystem exe, MessageBoxW for a start
# that fails before the window exists. open.rs: CREATE_NEW_CONSOLE for the cmd window it opens,
# ShellExecuteW for the Store, GitHub and folders.
windows-sys = { version = "=0.61.2", features = [
    "Win32_Foundation",
    "Win32_System_Console",
    "Win32_System_Threading",
    "Win32_UI_Shell",
    "Win32_UI_WindowsAndMessaging",
] }

[dev-dependencies]
# The in-memory machine: the app's tests never touch this computer.
cmdstudio-core = { path = "../../crates/core", features = ["fake", "ts"] }
tauri = { version = "=2.12.4", features = ["test"] }
```

- [ ] **Step 3: Write `app/src-tauri/src/command_names.rs`**

```rust
//! The IPC commands the window may call: spec §5.7's list, plus `preview_render`,
//! `open_external` and `advanced_read` (Phase 2b; spec §5.7 records them). One list, read by
//! `build.rs`, which turns each into the permissions `allow-<command>` and `deny-<command>`
//! (underscores become hyphens), and by `ipc.rs`, whose test checks that every name here is
//! registered with Tauri and allowed by `capabilities/default.json`, and nothing else is.
pub const COMMANDS: &[&str] = &[
    "system_detect",
    "themes_list",
    "theme_get",
    "theme_save",
    "theme_duplicate",
    "theme_delete",
    "theme_import_preview",
    "theme_import_confirm",
    "theme_export",
    "palette_generate",
    "contrast_check",
    "contrast_fix",
    "preview_render",
    "plan_apply",
    "apply",
    "undo_last",
    "snapshots_list",
    "snapshot_restore",
    "revert_all",
    "doctor_run",
    "doctor_fix",
    "settings_get",
    "settings_set",
    "open_cmd",
    "open_external",
    "diagnostics_copy",
    "advanced_read",
];
```

- [ ] **Step 4: Write `app/src-tauri/build.rs`**

```rust
//! Tauri's build step, plus two things of CMD Studio's own. The app's commands become ACL
//! permissions (`allow-<command>`), so that `capabilities/default.json` is the one list of what
//! the window may call (spec §5.10): a command registered in `ipc.rs` but missing here, or the
//! other way round, fails the build or a test. And a placeholder page stands in for the UI when
//! it is not built yet, because `tauri::generate_context!` refuses a missing `frontendDist`:
//! `cargo test --workspace` on a fresh clone, and CI's Rust jobs, need no Node.
include!("src/command_names.rs");

fn main() {
    let dist = std::path::Path::new("../ui/dist");
    if !dist.join("index.html").exists() {
        let made = std::fs::create_dir_all(dist).and_then(|()| {
            std::fs::write(
                dist.join("index.html"),
                "<!doctype html><meta charset=\"utf-8\"><title>CMD Studio</title>\
                 <p>The UI is not built. Run <code>npm run build</code> in <code>app/</code>.",
            )
        });
        if let Err(e) = made {
            println!("cargo:warning=could not write the placeholder UI page: {e}");
        }
    }
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if let Err(e) = tauri_build::try_build(attributes) {
        println!("cargo:warning=tauri-build failed: {e}");
        std::process::exit(1);
    }
}
```

- [ ] **Step 5: Write `app/src-tauri/tauri.conf.json`**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "CMD Studio",
  "mainBinaryName": "cmdstudio",
  "version": "0.1.0",
  "identifier": "ai.resincore.cmdstudio",
  "build": {
    "devUrl": "http://localhost:1420",
    "frontendDist": "../ui/dist",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "withGlobalTauri": false,
    "windows": [
      {
        "label": "main",
        "create": false,
        "title": "CMD Studio",
        "width": 1100,
        "height": 760,
        "minWidth": 900,
        "minHeight": 620,
        "center": true
      }
    ],
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; font-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'self'; frame-ancestors 'none'"
    }
  },
  "bundle": {
    "active": false,
    "icon": ["icons/icon.ico"]
  }
}
```

- [ ] **Step 6: Write `app/src-tauri/capabilities/default.json`**

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "The main window may call exactly the IPC commands of spec §5.7 (with preview_render, open_external and advanced_read) and listen for the apply-progress event. Nothing else: no plugin command, no shell, no file system (spec §5.10).",
  "windows": ["main"],
  "permissions": [
    "core:event:allow-listen",
    "core:event:allow-unlisten",
    "allow-system-detect",
    "allow-themes-list",
    "allow-theme-get",
    "allow-theme-save",
    "allow-theme-duplicate",
    "allow-theme-delete",
    "allow-theme-import-preview",
    "allow-theme-import-confirm",
    "allow-theme-export",
    "allow-palette-generate",
    "allow-contrast-check",
    "allow-contrast-fix",
    "allow-preview-render",
    "allow-plan-apply",
    "allow-apply",
    "allow-undo-last",
    "allow-snapshots-list",
    "allow-snapshot-restore",
    "allow-revert-all",
    "allow-doctor-run",
    "allow-doctor-fix",
    "allow-settings-get",
    "allow-settings-set",
    "allow-open-cmd",
    "allow-open-external",
    "allow-diagnostics-copy",
    "allow-advanced-read"
  ]
}
```

- [ ] **Step 7: Write `app/src-tauri/src/console.rs`**

```rust
//! The console of the CLI mode, and the one message box the app shows when it cannot start.
//! Windows only; elsewhere these do nothing.

/// Attaches to the console of the parent process (the cmd or PowerShell window the exe was
/// started from), so that a Windows-subsystem build prints its CLI output there. Does nothing
/// when there is none (started by the uninstaller with its output redirected, or by a program
/// that gave no console): the standard handles then stay as they were inherited.
#[cfg(windows)]
pub fn attach_parent() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // SAFETY: `AttachConsole` takes a process id and touches no memory of ours; a failure (no
    // parent console, or a console already attached) is reported by its return value, which
    // is deliberately ignored: either way the standard handles are usable or inert.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
pub fn attach_parent() {}

/// Shows `message` in a message box titled "CMD Studio" and returns when it is closed. For a
/// start that fails before the window exists; English, like every text outside the UI.
#[cfg(windows)]
pub fn fatal(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let (text, title) = (wide(message), wide("CMD Studio"));
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call, the owner
    // window is null (no parent), and the flags are documented constants.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
pub fn fatal(message: &str) {
    eprintln!("CMD Studio: {message}");
}
```

- [ ] **Step 8: Write the first `app/src-tauri/src/lib.rs`** (Tasks 6–14 add one `pub mod` line each; Task 15 replaces the file)

```rust
//! CMD Studio's desktop app (Tauri 2): a window over `cmdstudio-core`. The IPC commands of
//! spec §5.7 are thin wrappers (`ipc`) over a [`backend::Backend`], which is tested against the
//! core's in-memory adapters; the theme library and drafts (`library`), the app's settings
//! (`settings`) and its log (`logs`) live in the data folder (spec §5.3); the CLI is the same
//! exe (`main.rs`). The UI (`app/ui`) paints what the core computes: no palette, contrast or
//! prompt logic lives in TypeScript (spec §5.6).
pub mod command_names;
pub mod console;
```

- [ ] **Step 9: Write `scripts/make-icon.ps1` and make the icon**

```powershell
<#
.SYNOPSIS
  Draws CMD Studio's placeholder icon (a terracotta square with ">_") as app\icon-source.png
  (1024x1024) and app\src-tauri\icons\icon.ico, with System.Drawing. Tauri's build embeds the
  .ico as the exe's icon (tauri-build reads bundle.icon); `npx tauri icon app\icon-source.png`
  makes every other size from the PNG once the npm packages are installed. The founder may
  replace both with a designed icon at any time; nothing else refers to their pixels.
#>
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = Split-Path $PSScriptRoot -Parent
$iconDir = Join-Path $root 'app\src-tauri\icons'
New-Item -ItemType Directory -Force $iconDir | Out-Null

function Draw([int] $size) {
  $bmp = New-Object System.Drawing.Bitmap $size, $size
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAlias
  $g.Clear([System.Drawing.ColorTranslator]::FromHtml('#d97757'))
  $font = New-Object System.Drawing.Font('Consolas', [float]($size * 0.47), [System.Drawing.FontStyle]::Bold)
  $brush = New-Object System.Drawing.SolidBrush([System.Drawing.ColorTranslator]::FromHtml('#faf9f5'))
  $g.DrawString('>_', $font, $brush, [float]($size * 0.04), [float]($size * 0.2))
  $g.Dispose()
  return $bmp
}

$png = Draw 1024
$png.Save((Join-Path $root 'app\icon-source.png'), [System.Drawing.Imaging.ImageFormat]::Png)
$png.Dispose()

$small = Draw 256
$icon = [System.Drawing.Icon]::FromHandle($small.GetHicon())
$stream = [System.IO.File]::Create((Join-Path $iconDir 'icon.ico'))
try { $icon.Save($stream) } finally { $stream.Close() }
$small.Dispose()
Write-Host "wrote app\icon-source.png and app\src-tauri\icons\icon.ico"
```

Run: `pwsh -NoProfile -File .\scripts\make-icon.ps1` (or `powershell -NoProfile -File …`)
Expected: `app/src-tauri/icons/icon.ico` exists (a few KB, a terracotta square with a white prompt mark; the script also writes `icon-1024.png` under `%TEMP%`, not in the repo). Open the `.ico` in the Photos app or Explorer's preview to see it; do not commit anything else the script wrote.

- [ ] **Step 10: Build the crate**

Run: `cargo build -p cmdstudio-app`
Expected: `Finished` after the Tauri tree compiles (minutes on the first run). `build.rs` wrote `app/ui/dist/index.html` (ignored) and `app/src-tauri/gen/schemas/*.json` (ignored); `Cargo.lock` grew by the Tauri tree. If a build script is blocked with `os error 4551`, rerun the identical command once; if blocked again, note "not compiled locally; CI is the gate" and continue (the Rust job of Task 15 builds it).
Run: `cargo clippy -p cmdstudio-app --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 11: Compare the lockfile with appendix A**

Run: `git diff --stat Cargo.lock`
Expected: additions only (about 380 `[[package]]` entries, appendix A). Spot-check against appendix A: `tauri 2.12.4`, `tauri-build 2.7.3`, `tauri-utils 2.10.2`, `tauri-plugin-single-instance 2.5.4`, `wry 0.57.0`, `rfd 0.17.2`, `ts-rs 12.0.1` (new), `windows-sys 0.61.2` and `sha2 0.11.0` (already there for the core; the Tauri tree adds `windows-sys 0.45.0` and `sha2 0.10.9` beside them). A transitive crate at a newer compatible version than appendix A lists is acceptable (index Decision 14); a different version of a pinned crate is not (the `=` pin would already have failed).

- [ ] **Step 12: Commit**

Stage `Cargo.toml Cargo.lock app/src-tauri/Cargo.toml app/src-tauri/build.rs app/src-tauri/tauri.conf.json app/src-tauri/capabilities/default.json app/src-tauri/src/command_names.rs app/src-tauri/src/console.rs app/src-tauri/src/lib.rs app/src-tauri/icons/icon.ico scripts/make-icon.ps1`. Message: `app: the cmdstudio-app crate — Tauri 2.12.4 config, capability for the 27 commands, icon`.

---

### Task 6: `AppError` and the meanings of spec §5.9, with the new `E_INTERNAL`

Every failure crosses the IPC as `{code, messageTh, messageEn, details}` (spec §5.9). The core's errors carry a code and English text; the Thai and English *meanings* are the rows of §5.9, kept in one table that a test compares with the spec file itself. The app's own faults — a poisoned mutex, a window that cannot be built, a bad argument — get the new code `E_INTERNAL`, which the spec gains here (the index's third spec conflict). `crates/core/tests/error_codes.rs`, which checks that every code in the source is a row of the spec, now scans the app crate too.

**Files:**
- Create: `app/src-tauri/src/error.rs`
- Modify: `app/src-tauri/src/lib.rs` (+ `pub mod error;`), `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§5.9, one row), `crates/core/tests/error_codes.rs`
- Test: `the_meanings_are_exactly_the_rows_of_spec_5_9`, `a_core_error_keeps_its_code_and_its_own_words`, `the_error_is_camel_case_json`, `export_bindings_apperror`

**Interfaces:**
- Consumes: `cmdstudio_core::actions::ActionError`, `model::ThemeError`, `detect::DetectError`, `state::StateError`, `plan::PlanError`, `resources::ResourceError`, `txn::Failure` as shipped (each has a code and a Display text).
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]  // camelCase, #[ts(export)]
  pub struct AppError { pub code: String, pub message_th: String, pub message_en: String, pub details: String }
  pub const INTERNAL: &str = "E_INTERNAL";
  pub fn meaning(code: &str) -> Option<(&'static str, &'static str)>   // (Thai, English) of a §5.9 row
  impl AppError {
      pub fn coded(code: &str, details: impl Into<String>) -> Self   // a known code: its meanings; unknown: E_INTERNAL with the code in the details
      pub fn internal(details: impl Into<String>) -> Self
  }
  impl From<ActionError | ThemeError | DetectError | StateError | PlanError | ResourceError | Failure> for AppError
  ```
  `Serialize` is what Tauri sends when a command returns `Err(AppError)`; the UI's `toIpcError` reads it.

- [ ] **Step 1: Write the failing tests** — create `app/src-tauri/src/error.rs` holding only this test module, and add `pub mod error;` to `lib.rs` after `pub mod console;`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    /// The spec, found from this crate upwards: `docs/superpowers/specs/…` in the nearest ancestor
    /// that has it (the repository root).
    fn spec_text() -> String {
        let rel = Path::new("docs/superpowers/specs/2026-10-10-cmd-studio-design.md");
        let found = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .map(|dir| dir.join(rel))
            .find(|path| path.exists())
            .unwrap();
        std::fs::read_to_string(found).unwrap()
    }

    /// The codes in the table of spec §5.9: its rows start with "| `E_".
    fn spec_codes() -> BTreeSet<String> {
        let spec = spec_text();
        spec.lines()
            .filter_map(|l| l.strip_prefix("| `E_"))
            .filter_map(|rest| rest.split('`').next())
            .map(|name| format!("E_{name}"))
            .collect()
    }

    #[test]
    fn the_meanings_are_exactly_the_rows_of_spec_5_9() {
        let here: BTreeSet<String> = MEANINGS.iter().map(|(c, _, _)| c.to_string()).collect();
        assert_eq!(here.len(), MEANINGS.len(), "a code twice");
        assert_eq!(here, spec_codes());
        for (code, th, en) in MEANINGS {
            assert!(
                th.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)),
                "{code}: Thai"
            );
            assert!(
                !en.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)),
                "{code}: English"
            );
        }
    }

    #[test]
    fn a_core_error_keeps_its_code_and_its_own_words() {
        let e = AppError::from(ActionError::Busy);
        assert_eq!(e.code, "E_BUSY");
        assert_eq!(e.message_th, cmdstudio_core::actions::BUSY_TH);
        assert_eq!(e.message_en, cmdstudio_core::actions::BUSY_EN);
        assert_eq!(e.details, cmdstudio_core::actions::BUSY_EN);
        // One passed on from another module: the row's texts, the core's English as detail.
        let e = AppError::from(ThemeError::NewerSchema(7));
        assert_eq!(e.code, "E_THEME_NEWER_SCHEMA");
        assert_eq!(e.message_th, "ไฟล์ธีมมาจากโปรแกรมรุ่นใหม่กว่า");
        assert_eq!(e.message_en, "The theme file comes from a newer version");
        assert!(e.details.contains("schemaVersion 7"), "{}", e.details);
        // An unknown code shows its details rather than nothing.
        let e = AppError::coded("E_NOT_A_ROW", "what happened");
        assert_eq!(
            (e.message_th.as_str(), e.message_en.as_str()),
            ("what happened", "what happened")
        );
        assert_eq!(AppError::internal("x").code, INTERNAL);
    }

    #[test]
    fn the_error_is_camel_case_json() {
        let e = AppError::coded("E_BUSY", "d");
        let v: serde_json::Value = serde_json::to_value(&e).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["code", "details", "messageEn", "messageTh"]);
    }
}
```

Run: `cargo test -p cmdstudio-app --lib error::`
Expected: FAILS to compile — `cannot find type AppError`, `cannot find function meaning`, `cannot find value INTERNAL` (the names Step 3 defines).

- [ ] **Step 2: Add the row to the spec** — in §5.9's table, after the `E_BUSY` row (the last one), add:

```markdown
| `E_INTERNAL` | CMD Studio เองมีปัญหา รายละเอียดบอกว่าตรงไหน ลองใหม่อีกครั้ง ถ้ายังเป็นอยู่ให้แจ้งปัญหาพร้อมข้อมูลวินิจฉัย (แอป, phase 2b) | CMD Studio itself ran into a problem; the details say where. Try again, and if it stays, report it with the diagnostics (the app, phase 2b) |
```

- [ ] **Step 3: Write the code above the test module**

```rust
//! What the window gets when a command fails (spec §5.9): a stable code, the problem in Thai
//! and in English, and the technical detail behind a "more" button. The Thai and English
//! meaning of each code is a row of spec §5.9, held here once ([`meaning`]); a core error that
//! words itself in both languages keeps its own words. The UI shows `messageTh` or `messageEn`
//! by its language and `details` on request; it never composes an error text of its own.
use cmdstudio_core::actions::ActionError;
use cmdstudio_core::detect::DetectError;
use cmdstudio_core::model::ThemeError;
use cmdstudio_core::plan::PlanError;
use cmdstudio_core::resources::ResourceError;
use cmdstudio_core::state::StateError;
use cmdstudio_core::txn::Failure;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppError {
    /// A code of spec §5.9, or [`INTERNAL`].
    pub code: String,
    pub message_th: String,
    pub message_en: String,
    /// The technical text behind the problem: the core's English words, a path, a system error.
    pub details: String,
}

/// CMD Studio's own fault: a worker thread that did not answer, a poisoned lock, a dialog that
/// could not be shown. Not one of the core's codes; spec §5.9 has its row.
pub const INTERNAL: &str = "E_INTERNAL";

/// The rows of spec §5.9: code, Thai meaning, English meaning. A test reads the spec and checks
/// that both have the same codes.
const MEANINGS: &[(&str, &str, &str)] = &[
    (
        "E_WT_NOT_FOUND",
        r#"ไม่พบ Windows Terminal หรือพบแต่ยังไม่เคยเปิด จึงยังไม่มี `settings.json` (D6)"#,
        r#"Windows Terminal was not found, or was never opened, so it has no settings.json yet (D6)"#,
    ),
    (
        "E_WT_PARSE",
        r#"parse `settings.json` ของ WT ไม่ได้ หรือไฟล์เขียนคีย์ที่ CMD Studio ดูแลซ้ำ หรือซ้อนลึกเกิน 64 ชั้น (§5.4) ต้องแก้ไฟล์ด้วยมือ"#,
        r#"Windows Terminal's settings.json can't be parsed, writes a key CMD Studio owns twice, or nests deeper than 64 levels (§5.4); fix it by hand"#,
    ),
    (
        "E_WT_WRITE",
        r#"เขียน `settings.json` ของ WT ไม่ได้ หรือ (D7) โปรไฟล์ cmd ใน WT ไม่ตรงกับธีม"#,
        r#"Windows Terminal's settings.json could not be written, or (D7) its cmd profile does not match the theme"#,
    ),
    (
        "E_AUTORUN_FOREIGN_CLINK",
        r#"เจอ Clink ตัวอื่นใน AutoRun"#,
        r#"Another Clink runs from AutoRun"#,
    ),
    (
        "E_AUTORUN_WRITE",
        r#"เขียน AutoRun ไม่ได้ หรือแก้อย่างปลอดภัยไม่ได้จึงต้องแก้ด้วยมือ หรือ (D4) ส่วนของเราหายไปหรือไม่อยู่หน้าสุด"#,
        r#"AutoRun could not be written, or can't be changed safely and needs a hand edit, or (D4) CMD Studio's part is gone or not first"#,
    ),
    (
        "E_CLINK_PATH",
        r#"ไม่มี path ที่ cmd ใช้เรียก Clink ได้อย่างปลอดภัย: ไม่มีทางไหนใน `<clink-launch>` ที่เป็น ASCII ล้วน (G19) หรือ path มี `%`, `"` หรือ `;` หรือไม่ใช่ Unicode ที่ถูกต้อง (§5.4) · หรือ (D3) Clink inject จากโฟลเดอร์ของมันไม่ได้ (G19)"#,
        r#"No path cmd can use to start Clink safely: no `<clink-launch>` choice is all ASCII (G19), or a path holds `%`, `"` or `;` or is not valid Unicode (§5.4); or (D3) Clink can't inject from its folder (G19)"#,
    ),
    (
        "E_FONT_INSTALL",
        r#"ติดตั้งฟอนต์ไม่สำเร็จ หรือ (D8) ไม่พบฟอนต์ของธีม"#,
        r#"The font could not be installed, or (D8) the theme's font is not installed"#,
    ),
    (
        "E_RUNTIME_MISSING",
        r#"ไฟล์ runtime หาย ถูกแก้ หรือรันไม่ได้ (D1–D4) หรือไฟล์ที่แนบมากับ CMD Studio หายหรือเสีย"#,
        r#"Runtime files are missing, changed or do not run (D1–D4), or the files that come with CMD Studio are missing or damaged"#,
    ),
    (
        "E_VERIFY_OMP",
        r#"ขั้นตรวจผล: OMP เรนเดอร์ config ไม่ได้"#,
        r#"Verify: Oh My Posh can't render the config"#,
    ),
    (
        "E_VERIFY_CLINK",
        r#"ขั้นตรวจผล: Clink โหลดสคริปต์ไม่ได้ (D3 ด้วย)"#,
        r#"Verify: Clink can't load the scripts (D3 too)"#,
    ),
    (
        "E_VERIFY_READBACK",
        r#"ขั้นตรวจผล: ค่าที่อ่านกลับมาไม่ตรงกับที่เขียน"#,
        r#"Verify: a value read back differs from what was written"#,
    ),
    (
        "E_SNAPSHOT_WRITE",
        r#"ถ่ายจุดย้อนกลับไม่สำเร็จ"#,
        r#"A restore point could not be written"#,
    ),
    (
        "E_THEME_INVALID",
        r#"ไฟล์ธีมผิดรูปแบบ"#,
        r#"The theme file is not valid"#,
    ),
    (
        "E_THEME_NEWER_SCHEMA",
        r#"ไฟล์ธีมมาจากโปรแกรมรุ่นใหม่กว่า"#,
        r#"The theme file comes from a newer version"#,
    ),
    (
        "E_ROLLBACK_INCOMPLETE",
        r#"ย้อนกลับได้ไม่ครบ"#,
        r#"The rollback did not finish; some parts must be fixed by hand"#,
    ),
    (
        "E_TXN_INTERRUPTED",
        r#"การเปลี่ยนแปลงครั้งก่อนถูกตัดกลางทาง (โปรแกรมดับ) และย้อนกลับให้ตอนเริ่มคำสั่งถัดไปที่เปลี่ยนเครื่อง · doctor แจ้งเมื่อยังไม่ได้ย้อน (§5.5)"#,
        r#"An earlier change was cut off (the app stopped); it is rolled back when the next command that changes the machine starts, and doctor reports one not rolled back yet (§5.5)"#,
    ),
    (
        "E_UNSUPPORTED_PLATFORM",
        r#"เครื่องนี้ไม่ใช่ Windows แบบ x64 (เช่น ARM64) ส่วนเสริมที่แนบมากับ v1 มีแต่รุ่น x64"#,
        r#"This machine is not x64 Windows (for example ARM64); the add-ons bundled with v1 are x64 only"#,
    ),
    (
        "E_DATA_WRITE",
        r#"เขียนลงโฟลเดอร์ข้อมูลของโปรแกรมไม่ได้ เช่น ไฟล์ใน `generated\` หรือ `state.json`"#,
        r#"Could not write to CMD Studio's data folder, such as a file in `generated\` or state.json"#,
    ),
    (
        "E_STATE_INVALID",
        r#"ใช้ `state.json` ไม่ได้ เพราะไฟล์เสีย ถูกแก้ เปิดไม่ได้ หรือเขียนโดย CMD Studio รุ่นที่ใหม่กว่า · ไฟล์ที่เปิดไม่ได้หรือมาจากรุ่นใหม่กว่าจะไม่ถูกเขียนทับ ส่วนไฟล์ที่เสีย "ย้อนกลับ" จะเอา `state.json` ก่อนใช้ธีมครั้งนั้นมาแทน และ "ย้อนทุกอย่าง" กับ `--cleanup` จะเปลี่ยนชื่อเป็น `state.json.damaged-<ms>` เก็บไว้ให้ดู"#,
        r#"state.json can't be used: it is damaged, was edited, could not be opened, or was written by a newer CMD Studio. One that could not be opened or is from a newer CMD Studio is never written over; a damaged one is replaced by undo with the state.json from before that apply, and renamed state.json.damaged-<ms> by Revert everything and --cleanup"#,
    ),
    (
        "E_GENERATED_EDITED",
        r#"มีคนแก้ไฟล์ใน `generated\` ด้วยมือ (D9)"#,
        r#"A file in `generated\` was edited by hand (D9)"#,
    ),
    (
        "E_APP_PACKAGED",
        r#"แอปรันอยู่ในแพ็กเกจ MSIX สิ่งที่แอปเขียนจึงไม่มีผลกับโปรแกรมอื่น (D10, §2.2 ข้อ 1)"#,
        r#"The app runs inside an MSIX package, so what it writes is invisible to other programs (D10, §2.2 lesson 1)"#,
    ),
    (
        "E_THEME_NOT_LOADED",
        r#"ใช้ธีมแล้วแต่ยังไม่มีหน้าต่าง cmd ไหนโหลดธีม ให้เปิดหน้าต่างใหม่ ในโหมด "cmd ทุกหน้าต่าง" ถ้าเปิดแล้วยังเป็นแบบนี้ แปลว่า cmd หยุดก่อนถึงส่วนของเราใน AutoRun ต้องแก้ค่า AutoRun ด้วยมือ ส่วนโหมด "เฉพาะใน Windows Terminal" ให้เปิดแท็บ cmd ใหม่ใน WT และถ้ายังเป็นแบบนี้ให้ใช้ธีมซ้ำ (D3)"#,
        r#"The theme is applied, but no cmd window has loaded it yet: open a new one. For every cmd window, if a new window does not load it either, cmd stops before CMD Studio's part of AutoRun, and AutoRun needs a hand edit; for Windows Terminal only, open a new cmd tab there, and if that does not load it either, apply the theme again (D3)"#,
    ),
    (
        "E_MACHINE_READ",
        r#"อ่านการตั้งค่าของเครื่องนี้ไม่ได้ (registry, ไฟล์ หรือรายชื่อฟอนต์) จึงยังไม่ได้เปลี่ยนการตั้งค่าของคุณ หรือ doctor ตรวจข้อนั้นไม่ได้"#,
        r#"Could not read this computer's settings (the registry, a file, the font list), so your settings were not changed, or doctor could not check that item"#,
    ),
    (
        "E_SNAPSHOT_READ",
        r#"อ่านจุดย้อนกลับไม่ได้ (ไฟล์หาย เสีย หรือเปิดไม่ได้ในตอนนี้) · ถ้าเป็นจุดของการเปลี่ยนแปลงที่ถูกตัดกลางทาง แปลว่ายังตรวจการเปลี่ยนแปลงครั้งก่อนไม่ได้ จึงยังไม่ได้เปลี่ยนการตั้งค่าของคุณ ให้ลองใหม่ (§5.5)"#,
        r#"A restore point can't be read (missing, damaged, or not readable right now). When it is the one of a change that was cut off, an earlier change could not be checked yet: your settings were not changed; try again (§5.5)"#,
    ),
    (
        "E_BUSY",
        r#"มี CMD Studio อีกตัว หรือโปรแกรมอื่น เปิดไฟล์ `cmdstudio.lock` อยู่ คือกำลังเปลี่ยนการตั้งค่าของเครื่องนี้ ให้ลองใหม่เมื่อทำเสร็จ (§5.5)"#,
        r#"Another CMD Studio, or another program, has cmdstudio.lock open: it is changing this computer right now. Try again when it is done (§5.5)"#,
    ),
    (
        "E_INTERNAL",
        r#"CMD Studio เองมีปัญหา รายละเอียดบอกว่าตรงไหน ลองใหม่อีกครั้ง ถ้ายังเป็นอยู่ให้แจ้งปัญหาพร้อมข้อมูลวินิจฉัย"#,
        r#"CMD Studio itself ran into a problem; the details say where. Try again, and if it stays, report it with the diagnostics"#,
    ),
];

/// The Thai and English meaning of a code of spec §5.9.
pub fn meaning(code: &str) -> Option<(&'static str, &'static str)> {
    MEANINGS
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, th, en)| (*th, *en))
}

impl AppError {
    /// An error whose texts are the §5.9 meaning of `code`, with `details` behind them. For a
    /// code the table does not know, the details stand in for both texts, so nothing is lost.
    pub fn coded(code: &str, details: impl Into<String>) -> Self {
        let details = details.into();
        let (th, en) = meaning(code)
            .map(|(th, en)| (th.to_string(), en.to_string()))
            .unwrap_or_else(|| (details.clone(), details.clone()));
        AppError {
            code: code.to_string(),
            message_th: th,
            message_en: en,
            details,
        }
    }

    /// [`INTERNAL`] with `details`.
    pub fn internal(details: impl Into<String>) -> Self {
        AppError::coded(INTERNAL, details)
    }

    /// The same error with the Thai text the core worded itself.
    fn with_th(mut self, th: Option<String>) -> Self {
        if let Some(th) = th {
            self.message_th = th;
        }
        self
    }
}

impl From<ActionError> for AppError {
    /// The core's English text is the detail; its own Thai, where it has one (`E_BUSY`,
    /// `E_UNSUPPORTED_PLATFORM`, a pending record, a pending recovery, the lock), is the Thai
    /// message, and the English text is then the English message too, because it says more than
    /// the code's row.
    fn from(e: ActionError) -> Self {
        let text = e.to_string();
        let th = e.message_th();
        let mut out = AppError::coded(e.code(), text.clone());
        if th.is_some() {
            out.message_en = text;
        }
        out.with_th(th)
    }
}

impl From<ThemeError> for AppError {
    fn from(e: ThemeError) -> Self {
        AppError::coded(e.code(), e.to_string())
    }
}

impl From<DetectError> for AppError {
    fn from(e: DetectError) -> Self {
        AppError::coded(e.code(), e.to_string())
    }
}

impl From<StateError> for AppError {
    fn from(e: StateError) -> Self {
        AppError::coded(e.code(), e.to_string())
    }
}

impl From<PlanError> for AppError {
    fn from(e: PlanError) -> Self {
        AppError::coded(e.code(), e.to_string())
    }
}

impl From<ResourceError> for AppError {
    fn from(e: ResourceError) -> Self {
        AppError::coded(e.code(), e.to_string())
    }
}

impl From<Failure> for AppError {
    fn from(f: Failure) -> Self {
        AppError::coded(f.code, f.message)
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cmdstudio-app --lib error::`
Expected: `test result: ok. 4 passed` (the three tests and `export_bindings_apperror`). `the_meanings_are_exactly_the_rows_of_spec_5_9` finds the spec by walking up from the crate's folder and compares the 27 codes of `MEANINGS` with the table's rows, Thai and English text included.

- [ ] **Step 5: The core's guard scans the app too**

```diff
--- a/crates/core/tests/error_codes.rs
+++ b/crates/core/tests/error_codes.rs
@@ -1,5 +1,5 @@
 //! The stable codes of spec §5.9 and the code agree: every `E_...` code that the production code
-//! of the core and the CLI says is a row of the table in §5.9, and every row is said somewhere.
+//! of the core, the CLI and the app says is a row of the table in §5.9, and every row is said somewhere.
 //! The table also holds each code's Thai meaning, for the app's own UI; the CLI prints the code
 //! and English text only.
 #![allow(
@@ -85,6 +85,11 @@
     for krate in ["core", "cli"] {
         rust_files(&root().join("crates").join(krate).join("src"), &mut files);
     }
+    // The app (Phase 2b) says codes too: its own E_INTERNAL, and the rows it quotes.
+    rust_files(
+        &root().join("app").join("src-tauri").join("src"),
+        &mut files,
+    );
     // The in-memory adapters are for tests only.
     files.retain(|f| !f.ends_with(Path::new("adapters").join("fake.rs")));
     assert!(files.len() > 20, "{files:?}");
```

Run: `cargo test -p cmdstudio-core --test error_codes`
Expected: `test result: ok. 2 passed` (every `E_…` in `crates/core/src`, `crates/cli/src` and `app/src-tauri/src` is a row of §5.9).

- [ ] **Step 6: Clean, commit**

`cargo fmt --all`, the clippy commands of the Global Constraints (the app crate: clean). Stage `app/src-tauri/src/error.rs app/src-tauri/src/lib.rs crates/core/tests/error_codes.rs docs/superpowers/specs/2026-10-10-cmd-studio-design.md`. Message: `app: AppError with the meanings of spec §5.9; E_INTERNAL for the app's own faults`.

---

### Task 7: The app's settings and its log

`settings.json` (index Decision 4) and `logs\cmdstudio.log` (Decision 10) are the two files the app alone owns. Both go through the core's `Fs` trait, so the tests use the fake file system and the real app writes atomically like everything else.

**Files:**
- Create: `app/src-tauri/src/settings.rs`, `app/src-tauri/src/logs.rs`
- Modify: `app/src-tauri/src/lib.rs` (+ `pub mod logs;` after `pub mod error;`, `pub mod settings;` at the end)
- Test: settings — `the_defaults_are_english_every_window_automatic_updates_and_the_wizard_not_done`, `settings_round_trip_as_camel_case_json_with_a_version`, `a_damaged_or_newer_file_is_set_aside_and_said`, `a_file_that_cannot_be_read_is_a_read_error` (+ 4 export tests); logs — `the_time_is_utc_iso_8601`, `lines_are_appended_with_their_time_and_without_line_breaks`, `the_log_rotates_at_one_megabyte_and_keeps_five_files`

**Interfaces:**
- Consumes: `cmdstudio_core::adapters::{Fs, AdapterError}` (`read`, `write_atomic`, `rename`, `exists`, `create_dir_all`, `list`), `cmdstudio_core::model::Target`, `error::AppError` (Task 6).
- Produces:
  ```rust
  // settings.rs
  pub const SETTINGS_VERSION: u32 = 1;
  pub enum Language { En, Th }                     // serde/ts: "en" | "th"
  pub enum UpdateMode { Auto, Notify, Off }        // "auto" | "notify" | "off"; read by Phase 4's updater
  pub struct AppSettings { pub language: Language, pub target: Target, pub set_wt_default_profile: bool, pub updates: UpdateMode, pub wizard_done: bool }
  impl Default for AppSettings  // En, All, true, Auto, false
  pub struct SettingsLoad { pub settings: AppSettings, pub reset_from: Option<String> }  // reset_from: the set-aside file's name, when the file was damaged or newer
  pub fn load(fs: &dyn Fs, path: &Path, now_ms: u64) -> Result<SettingsLoad, AppError>   // missing → defaults; damaged/newer → renamed `settings.json.damaged-<now_ms>`, defaults, reset_from; unreadable → E_MACHINE_READ
  pub fn save(fs: &dyn Fs, path: &Path, settings: &AppSettings) -> Result<(), AppError>  // {"version":1, …camelCase} written atomically
  // logs.rs
  pub const LOG_FILE: &str = "cmdstudio.log"; pub const KEEP: usize = 5; pub const MAX_BYTES: usize = 1024 * 1024;
  pub struct Logger { … }
  impl Logger {
      pub fn new(dir: PathBuf) -> Self
      pub fn file(&self) -> PathBuf
      pub fn write(&self, fs: &dyn Fs, now_ms: u64, line: &str) -> Result<(), AdapterError>  // "<iso utc> <line>\n", line breaks in `line` become spaces; rotates cmdstudio.log → .1 … .5 at 1 MB
      pub fn tail(&self, fs: &dyn Fs, lines: usize) -> String
  }
  pub fn iso_utc(ms: u64) -> String   // "2026-10-11T04:58:29Z"
  ```

- [ ] **Step 1: Write the failing tests** — `settings.rs` with only its test module, `logs.rs` with only its test module, the two `pub mod` lines in `lib.rs`:

`app/src-tauri/src/settings.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cmdstudio_core::adapters::fake::FakeFs;
    use std::path::PathBuf;

    fn file() -> PathBuf {
        PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\settings.json")
    }

    #[test]
    fn the_defaults_are_english_every_window_automatic_updates_and_the_wizard_not_done() {
        let fs = FakeFs::new();
        let loaded = load(&fs, &file(), 1).unwrap();
        assert_eq!(loaded.reset_from, None);
        assert_eq!(
            loaded.settings,
            AppSettings {
                language: Language::En,
                target: Target::All,
                set_wt_default_profile: true,
                updates: UpdateMode::Auto,
                wizard_done: false,
            }
        );
    }

    #[test]
    fn settings_round_trip_as_camel_case_json_with_a_version() {
        let fs = FakeFs::new();
        let mine = AppSettings {
            language: Language::Th,
            target: Target::Wt,
            set_wt_default_profile: false,
            updates: UpdateMode::Off,
            wizard_done: true,
        };
        save(&fs, &file(), &mine).unwrap();
        let text = String::from_utf8(fs.file(file()).unwrap()).unwrap();
        assert!(
            text.starts_with("{\n  \"version\": 1,\n  \"language\": \"th\",\n"),
            "{text}"
        );
        assert!(
            text.contains("\"setWtDefaultProfile\": false") && text.ends_with("}\n"),
            "{text}"
        );
        assert_eq!(load(&fs, &file(), 2).unwrap().settings, mine);
        // A field missing from an older file reads as its default.
        fs.put(file(), b"{\"version\": 1, \"language\": \"th\"}");
        let loaded = load(&fs, &file(), 3).unwrap();
        assert_eq!(loaded.settings.language, Language::Th);
        assert_eq!(loaded.settings.target, Target::All);
    }

    #[test]
    fn a_damaged_or_newer_file_is_set_aside_and_said() {
        let fs = FakeFs::new();
        fs.put(file(), b"{ not json");
        let loaded = load(&fs, &file(), 1_791_694_709_000).unwrap();
        assert_eq!(loaded.settings, AppSettings::default());
        assert_eq!(
            loaded.reset_from.as_deref(),
            Some("settings.json.damaged-1791694709000")
        );
        assert!(!fs.exists(&file()));
        assert!(fs.exists(&file().with_file_name("settings.json.damaged-1791694709000")));
        // The same millisecond again: the name moves on instead of failing.
        fs.put(file(), b"{\"version\": 9}");
        let loaded = load(&fs, &file(), 1_791_694_709_000).unwrap();
        assert_eq!(
            loaded.reset_from.as_deref(),
            Some("settings.json.damaged-1791694709001")
        );
        // A rename that fails leaves the file and is an error, not a silent reset.
        fs.put(file(), b"{ not json");
        fs.fail_removes_of(file());
        let e = load(&fs, &file(), 5).unwrap_err();
        assert_eq!(e.code, "E_DATA_WRITE");
        assert!(fs.exists(&file()));
    }

    #[test]
    fn a_file_that_cannot_be_read_is_a_read_error() {
        let fs = FakeFs::new();
        fs.put(file(), b"{}");
        fs.fail_reads_of(file());
        assert_eq!(load(&fs, &file(), 1).unwrap_err().code, "E_MACHINE_READ");
    }
}
```

`app/src-tauri/src/logs.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cmdstudio_core::adapters::fake::FakeFs;
    use std::path::Path;

    fn logger() -> Logger {
        Logger::new(PathBuf::from(
            r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\logs",
        ))
    }

    #[test]
    fn the_time_is_utc_iso_8601() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_791_694_709_000), "2026-10-11T04:58:29Z");
        assert_eq!(iso_utc(951_782_400_000), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn lines_are_appended_with_their_time_and_without_line_breaks() {
        let (fs, log) = (FakeFs::new(), logger());
        log.write(&fs, 0, "cmd=themes_list ok").unwrap();
        log.write(&fs, 1000, "cmd=apply\nerr=E_BUSY").unwrap();
        let text = String::from_utf8(fs.file(log.file()).unwrap()).unwrap();
        assert_eq!(
            text,
            "1970-01-01T00:00:00Z cmd=themes_list ok\n1970-01-01T00:00:01Z cmd=apply err=E_BUSY\n"
        );
        assert_eq!(
            log.tail(&fs, 1),
            "1970-01-01T00:00:01Z cmd=apply err=E_BUSY"
        );
        assert_eq!(log.tail(&fs, 9).lines().count(), 2);
        assert_eq!(Logger::new(PathBuf::from(r"C:\none")).tail(&fs, 3), "");
    }

    #[test]
    fn the_log_rotates_at_one_megabyte_and_keeps_five_files() {
        let (fs, log) = (FakeFs::new(), logger());
        let big = "x".repeat(MAX_BYTES - 100);
        for n in 0..6_u64 {
            log.write(&fs, n * 1000, &big).unwrap();
            log.write(&fs, n * 1000 + 1, &format!("round {n}")).unwrap();
        }
        // Round 5 is current; rounds 4, 3, 2, 1 are .1 to .4; round 0 is gone.
        let text = |p: PathBuf| String::from_utf8(fs.file(p).unwrap()).unwrap();
        assert!(text(log.file()).ends_with("round 5\n"));
        for (n, round) in [(1, 4), (2, 3), (3, 2), (4, 1)] {
            assert!(
                text(log.rotated(n)).ends_with(&format!("round {round}\n")),
                "{n}"
            );
        }
        assert!(!fs.exists(&log.rotated(5)));
        let entries = fs
            .list_dir(Path::new(
                r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\logs",
            ))
            .unwrap();
        assert_eq!(entries.len(), KEEP);
        for e in &entries {
            assert!(
                fs.file(&e.path).unwrap().len() <= MAX_BYTES,
                "{}",
                e.path.display()
            );
        }
    }
}
```

Run: `cargo test -p cmdstudio-app --lib settings::`
Expected: FAILS to compile (`AppSettings`, `load`, `save`, `Logger` not found).

- [ ] **Step 2: Write the code above each test module**

`app/src-tauri/src/settings.rs`:

```rust
//! The app's own settings (spec §4.3 "ตั้งค่า"): the UI's language, where themes go, whether
//! Windows Terminal opens with cmd, how updates are handled, and whether the first-run wizard
//! was finished. In `<data>\settings.json`, not in state.json: an undo puts state.json back as
//! it was before an apply, and the settings must not go back with it. The UI always opens in
//! English unless Thai was chosen here (founder, 2026-10-16); the language of Windows is not
//! looked at.
use crate::error::AppError;
use cmdstudio_core::adapters::Fs;
use cmdstudio_core::model::Target;
use serde::{Deserialize, Serialize};
use std::path::Path;
use ts_rs::TS;

pub const SETTINGS_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Language {
    En,
    Th,
}

/// Spec §6.2: automatic (the default, founder 2026-10-16), notify first, or off. Stored and shown
/// here; Phase 4's updater reads it, which is what makes it live.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum UpdateMode {
    Auto,
    Notify,
    Off,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct AppSettings {
    pub language: Language,
    /// Where an apply puts the theme (the wizard's page 4 and the Settings page).
    pub target: Target,
    /// Make the cmd profile Windows Terminal's default profile when applying.
    pub set_wt_default_profile: bool,
    pub updates: UpdateMode,
    /// The first-run wizard was finished or skipped: the app opens on the home page.
    pub wizard_done: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            language: Language::En,
            target: Target::All,
            set_wt_default_profile: true,
            updates: UpdateMode::Auto,
            wizard_done: false,
        }
    }
}

/// `settings.json` on disk: a version in front of the settings.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsFile {
    version: u32,
    #[serde(flatten)]
    settings: AppSettings,
}

/// What `settings_get` returns: the settings, and the name of the damaged file that was set
/// aside when the defaults had to stand in (spec §2.1: no silent change).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SettingsLoad {
    pub settings: AppSettings,
    pub reset_from: Option<String>,
}

/// Reads the settings. No file gives the defaults. A file that cannot be read is an error
/// (`E_MACHINE_READ`). A damaged file, or one from another version, is renamed
/// `settings.json.damaged-<ms>` (kept for a look) and the defaults stand in, which the result
/// says; when the rename fails, that is an error (`E_DATA_WRITE`) and the file stays.
pub fn load(fs: &dyn Fs, path: &Path, now_ms: u64) -> Result<SettingsLoad, AppError> {
    let bytes = fs.read(path).map_err(|e| {
        AppError::coded(
            "E_MACHINE_READ",
            format!("{} could not be read: {e}", path.display()),
        )
    })?;
    let Some(bytes) = bytes else {
        return Ok(SettingsLoad {
            settings: AppSettings::default(),
            reset_from: None,
        });
    };
    let parsed: Result<SettingsFile, _> = serde_json::from_slice(&bytes);
    let why = match parsed {
        Ok(file) if file.version == SETTINGS_VERSION => {
            return Ok(SettingsLoad {
                settings: file.settings,
                reset_from: None,
            })
        }
        Ok(file) => format!(
            "version {} (this CMD Studio writes {SETTINGS_VERSION})",
            file.version
        ),
        Err(e) => e.to_string(),
    };
    let aside = set_aside_name(fs, path, now_ms);
    fs.rename(path, &aside).map_err(|e| {
        AppError::coded(
            "E_DATA_WRITE",
            format!(
                "{} is damaged ({why}) and could not be set aside as {}: {e}",
                path.display(),
                aside.display()
            ),
        )
    })?;
    Ok(SettingsLoad {
        settings: AppSettings::default(),
        reset_from: aside.file_name().map(|n| n.to_string_lossy().into_owned()),
    })
}

/// `<file>.damaged-<ms>`, with the milliseconds moved on while that name is taken.
fn set_aside_name(fs: &dyn Fs, path: &Path, now_ms: u64) -> std::path::PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "settings.json".to_string());
    let parent = path.parent().unwrap_or(Path::new(""));
    let mut ms = now_ms;
    loop {
        let candidate = parent.join(format!("{name}.damaged-{ms}"));
        if !fs.exists(&candidate) {
            return candidate;
        }
        ms = ms.saturating_add(1);
    }
}

/// Writes the settings, whole, in one step (`write_atomic`).
pub fn save(fs: &dyn Fs, path: &Path, settings: &AppSettings) -> Result<(), AppError> {
    let file = SettingsFile {
        version: SETTINGS_VERSION,
        settings: settings.clone(),
    };
    let mut bytes = serde_json::to_vec_pretty(&file).map_err(|e| {
        AppError::coded(
            "E_DATA_WRITE",
            format!("settings could not be produced: {e}"),
        )
    })?;
    bytes.push(b'\n');
    fs.write_atomic(path, &bytes).map_err(|e| {
        AppError::coded(
            "E_DATA_WRITE",
            format!("{} could not be written: {e}", path.display()),
        )
    })
}
```

`app/src-tauri/src/logs.rs`:

```rust
//! `<data>\logs\cmdstudio.log` (spec §5.3): one line per IPC command, with the time, what was
//! asked and how it ended; five files of at most 1 MB, `cmdstudio.log`, `.1` to `.4`. Written
//! through `Fs` like every file of CMD Studio's, so the tests use the in-memory disk. No theme
//! contents and no user text go into it: the diagnostics (spec §5.10) quote it after redaction.
use cmdstudio_core::adapters::{AdapterError, Fs};
use std::path::PathBuf;

pub const LOG_FILE: &str = "cmdstudio.log";
/// The log and its rotated copies, together.
pub const KEEP: usize = 5;
pub const MAX_BYTES: usize = 1024 * 1024;

pub struct Logger {
    dir: PathBuf,
}

impl Logger {
    pub fn new(dir: PathBuf) -> Self {
        Logger { dir }
    }

    pub fn file(&self) -> PathBuf {
        self.dir.join(LOG_FILE)
    }

    fn rotated(&self, n: usize) -> PathBuf {
        self.dir.join(format!("{LOG_FILE}.{n}"))
    }

    /// Appends `<time> <line>`. When the file would pass 1 MB, the copies move up one (`.4`
    /// goes) and a new file starts. A line that cannot be written is the caller's to drop: a
    /// log never stops a command.
    pub fn write(&self, fs: &dyn Fs, now_ms: u64, line: &str) -> Result<(), AdapterError> {
        let entry = format!("{} {}\n", iso_utc(now_ms), line.replace(['\r', '\n'], " "));
        let file = self.file();
        let mut current = fs.read(&file)?.unwrap_or_default();
        if !current.is_empty() && current.len() + entry.len() > MAX_BYTES {
            self.rotate(fs)?;
            current.clear();
        }
        current.extend_from_slice(entry.as_bytes());
        fs.write_atomic(&file, &current)
    }

    fn rotate(&self, fs: &dyn Fs) -> Result<(), AdapterError> {
        let last = self.rotated(KEEP - 1);
        if fs.exists(&last) {
            fs.remove_file(&last)?;
        }
        for n in (1..KEEP - 1).rev() {
            let from = self.rotated(n);
            if fs.exists(&from) {
                fs.rename(&from, &self.rotated(n + 1))?;
            }
        }
        let file = self.file();
        if fs.exists(&file) {
            fs.rename(&file, &self.rotated(1))?;
        }
        Ok(())
    }

    /// The last `lines` lines of the current file; empty when there is none or it cannot be read.
    pub fn tail(&self, fs: &dyn Fs, lines: usize) -> String {
        let Ok(Some(bytes)) = fs.read(&self.file()) else {
            return String::new();
        };
        let text = String::from_utf8_lossy(&bytes);
        let all: Vec<&str> = text.lines().collect();
        let skip = all.len().saturating_sub(lines);
        all.iter()
            .skip(skip)
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` from milliseconds since 1970 (the civil-from-days algorithm).
pub fn iso_utc(ms: u64) -> String {
    let secs = ms / 1000;
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant's days-to-civil, for days since 1970-01-01.
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
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p cmdstudio-app --lib settings::` → `8 passed` (4 tests + `export_bindings_{appsettings,language,updatemode,settingsload}`).
Run: `cargo test -p cmdstudio-app --lib logs::` → `3 passed`.
Run: `cargo test -p cmdstudio-app --lib` → `15 passed`.

- [ ] **Step 4: See that the default is pinned**

In `settings.rs` change `updates: UpdateMode::Auto` in `Default` to `UpdateMode::Notify`; run `cargo test -p cmdstudio-app --lib settings::`.
Expected: `the_defaults_are_english_every_window_automatic_updates_and_the_wizard_not_done` FAILS. Put `Auto` back; 8 passed.

- [ ] **Step 5: Clean, commit**

Clippy of the app crate: clean. Stage `app/src-tauri/src/settings.rs app/src-tauri/src/logs.rs app/src-tauri/src/lib.rs`. Message: `app: settings.json (English, every window, automatic updates by default) and the rotating log`.

---

### Task 8: The `Backend` over a `Machine`, and the real machine

The heart of the app's Rust side: `Backend<M>` holds the machine, the paths, the bundled resources, a clock and the logger; `logged` runs a command and writes its line; the settings commands and `advanced_read` live here, the rest in the submodules of Tasks 9–13 (`impl<M: Machine> Backend<M>` blocks). The test-only `testing` module builds a backend over the core's `FakeMachine` with Windows Terminal, Oh My Posh and Clink answering like the real ones — every later task's tests start from it. `machine.rs` is the one `RealMachine`, Windows only.

**Files:**
- Create: `app/src-tauri/src/backend/mod.rs`, `app/src-tauri/src/machine.rs`
- Modify: `app/src-tauri/src/lib.rs` (+ `pub mod backend;` as the first module; `#[cfg(windows)] pub mod machine;` after `pub mod logs;`)
- Test: `a_logged_command_writes_one_line_with_its_name_and_outcome`, `settings_default_then_round_trip_through_the_data_folder` (+ 4 export tests: `Note`, `Versions`, `GeneratedFile`, `AdvancedInfo`)

**Interfaces:**
- Consumes: `cmdstudio_core::adapters::System` (the bundle of `fs`, `autorun`, `fonts`, `wt`, `process`, `host` adapters), `adapters::fake::FakeMachine` (tests), `adapters::windows::{WinFs, WinAutoRun, WinFonts, WinWt, WinProcess, WinHost}` (real), `actions::{Ctx, Handled, StartReport}`, `plan::{Paths, CLINK_VERSION, OMP_VERSION}`, `resources::{Resources, NERD_FONTS_VERSION}`, `snapshot::Unrestored`, `txn::Outcome`; `settings::{load, save, AppSettings, SettingsLoad}`, `logs::Logger`, `error::AppError`; `cmdstudio_cli::DATA_DIR_NAME` (Task 4).
- Produces:
  ```rust
  pub trait Machine { fn system(&self) -> System<'_>; }
  pub struct Backend<M> { /* machine, paths, resources, now, log, pending_import */ }
  impl<M: Machine> Backend<M> {
      pub fn new(machine: M, data: PathBuf, resources: PathBuf, now: Box<dyn Fn() -> u64 + Send + Sync>) -> Self
      pub fn paths(&self) -> &Paths
      pub(crate) fn sys(&self) -> System<'_>
      pub(crate) fn now_ms(&self) -> u64
      pub(crate) fn ctx(&self) -> Ctx                       // the core's context: paths, resources, clock
      pub fn logged<T>(&self, name: &str, f: impl FnOnce(&Self) -> Result<T, AppError>) -> Result<T, AppError>  // one log line: "cmd=<name> ms=<n> ok" or "… err=<code>"
      pub(crate) fn log_tail(&self, lines: usize) -> String
      pub fn settings_get(&self) -> Result<SettingsLoad, AppError>
      pub fn settings_set(&self, settings: AppSettings) -> Result<AppSettings, AppError>
      pub(crate) fn settings_or_default(&self) -> AppSettings
      pub fn advanced_read(&self) -> Result<AdvancedInfo, AppError>
  }
  pub struct Note { pub code: String, pub message_th: String, pub message_en: String }   // a start-of-command note (spec §5.5: a rollback finished, a point set aside)
  pub fn note(code: &str, en: impl Into<String>) -> Note
  pub fn start_notes(start: &StartReport) -> Vec<Note>
  pub fn start_unrestored(start: &StartReport) -> Vec<Unrestored>
  pub struct Versions { pub app: String, pub oh_my_posh: String, pub clink: String, pub nerd_fonts: String }
  pub fn versions() -> Versions
  pub struct GeneratedFile { pub name: String, pub text: String }
  pub struct AdvancedInfo { pub data_dir: String, pub log_tail: String, pub generated: Vec<GeneratedFile>, pub versions: Versions }
  #[cfg(test)] pub(crate) mod testing { pub(crate) const WT: &str; pub(crate) const DATA: &str; pub(crate) fn clock() -> (Arc<AtomicU64>, Box<dyn Fn() -> u64 + Send + Sync>); pub(crate) fn backend() -> Backend<FakeMachine>; pub(crate) fn terracotta_id() -> Uuid; impl Machine for FakeMachine }
  // machine.rs (Windows)
  pub struct RealMachine { … }   impl Machine for RealMachine
  pub fn real_backend() -> Result<Backend<RealMachine>, String>   // data = %LOCALAPPDATA%\ai.resincore.cmdstudio, resources = <exe folder>\resources
  ```
  The DTOs are `camelCase` and `#[ts(export)]`.

- [ ] **Step 1: Write the failing tests** — `backend/mod.rs` holding only the two test modules below (`testing`, then `tests`), and `pub mod backend;` at the top of `lib.rs`'s module list:

```rust
#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use cmdstudio_core::adapters::fake::{FakeFonts, FakeMachine};
    use cmdstudio_core::adapters::{Fs, WtLocation, WtVariant};
    use cmdstudio_core::plan::NERD_FONT_FAMILY;
    use cmdstudio_core::resources::{fake_resources, NERD_FONT_FILE};
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    impl Machine for FakeMachine {
        fn system(&self) -> System<'_> {
            FakeMachine::system(self)
        }
    }

    pub(crate) const WT: &str = r"C:\Users\tester\AppData\Local\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json";
    pub(crate) const DATA: &str = r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio";

    /// A clock the test moves: every read is one millisecond later than the one before, so
    /// restore point ids never collide.
    pub(crate) fn clock() -> (Arc<AtomicU64>, Box<dyn Fn() -> u64 + Send + Sync>) {
        let now = Arc::new(AtomicU64::new(1_791_694_709_000));
        let shared = now.clone();
        (now, Box::new(move || shared.fetch_add(1, Ordering::SeqCst)))
    }

    /// A machine with Windows Terminal, Oh My Posh and Clink answering like the real ones, and
    /// the bundled files; the backend over it.
    pub(crate) fn backend() -> Backend<FakeMachine> {
        let mut m = FakeMachine::new();
        m.fs.create_dir_all(Path::new(
            r"C:\Users\tester\AppData\Local\Microsoft\Windows",
        ))
        .unwrap();
        m.fs.put(
            WT,
            include_str!("../../../../crates/core/tests/fixtures/wt/default.json"),
        );
        m.wt.locations = vec![WtLocation {
            variant: WtVariant::Store,
            installed: true,
            settings: PathBuf::from(WT),
        }];
        m.fonts = FakeFonts::new(
            r"C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts",
            &["Arial", "Cascadia Mono", "Consolas"],
        );
        m.fonts.knows_file(NERD_FONT_FILE, NERD_FONT_FAMILY);
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0");
        let resources = fake_resources(&m.fs, Path::new(r"C:\Program Files\CMD Studio\resources"));
        let (_, now) = clock();
        Backend::new(m, PathBuf::from(DATA), resources.root, now)
    }

    /// The id of the Terracotta preset.
    pub(crate) fn terracotta_id() -> uuid::Uuid {
        cmdstudio_core::presets::builtin()
            .unwrap()
            .into_iter()
            .find(|(key, _)| *key == "terracotta")
            .map(|(_, t)| t.id)
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::backend;
    use super::*;

    #[test]
    fn a_logged_command_writes_one_line_with_its_name_and_outcome() {
        let b = backend();
        let ok: Result<u8, AppError> = b.logged("themes_list", |_| Ok(7));
        assert_eq!(ok, Ok(7));
        let err: Result<u8, AppError> = b.logged("apply", |_| Err(AppError::coded("E_BUSY", "x")));
        assert_eq!(err.unwrap_err().code, "E_BUSY");
        let log = b.log_tail(10);
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 2, "{log}");
        assert!(
            lines[0].contains(" cmd=themes_list ms=") && lines[0].ends_with(" ok"),
            "{log}"
        );
        assert!(
            lines[1].contains(" cmd=apply ms=") && lines[1].ends_with(" err=E_BUSY"),
            "{log}"
        );
        assert!(b.sys().fs.exists(&b.paths().logs().join("cmdstudio.log")));
    }

    #[test]
    fn settings_default_then_round_trip_through_the_data_folder() {
        let b = backend();
        let loaded = b.settings_get().unwrap();
        assert_eq!(loaded.settings, AppSettings::default());
        assert_eq!(loaded.reset_from, None);
        let mut mine = loaded.settings;
        mine.language = crate::settings::Language::Th;
        mine.wizard_done = true;
        assert_eq!(b.settings_set(mine.clone()).unwrap(), mine);
        assert_eq!(b.settings_get().unwrap().settings, mine);
        assert!(b.sys().fs.exists(&b.paths().settings_file()));
    }
}
```

Run: `cargo test -p cmdstudio-app --lib backend::`
Expected: FAILS to compile (`Backend`, `Machine`, `AppError` paths not found).

- [ ] **Step 2: Write the code above the test modules** — this is the file without its five `pub mod` lines; Tasks 9–13 add `pub mod system;`, `pub mod preview;`, `pub mod apply;`, `pub mod doctor;`, `pub mod diagnostics;` back under the doc comment (alphabetical in the end: apply, diagnostics, doctor, preview, system):

```rust
//! The app's commands behind the IPC (spec §5.7), as a [`Backend`] over a machine: the IPC layer
//! (`ipc.rs`) only moves arguments and results across, so every command is tested here against
//! the core's in-memory adapters, never against this computer. One `pub` method per command,
//! spread over the modules by subject: `system`, the theme library (`crate::library`), `preview`,
//! `apply` (plan, apply, undo, restore points, revert), `doctor`, `diagnostics`, and the settings
//! and the Advanced page here.

use crate::error::AppError;
use crate::logs::Logger;
use crate::settings::{self, AppSettings, SettingsLoad};
use cmdstudio_core::actions::{Ctx, Handled, StartReport};
use cmdstudio_core::adapters::System;
use cmdstudio_core::model::Theme;
use cmdstudio_core::plan::{Paths, CLINK_VERSION, OMP_VERSION};
use cmdstudio_core::resources::{Resources, NERD_FONTS_VERSION};
use cmdstudio_core::snapshot::Unrestored;
use cmdstudio_core::txn::Outcome;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;
use ts_rs::TS;

/// What the backend runs on: the real adapters (`crate::machine::RealMachine`) or the core's
/// `FakeMachine` in tests.
pub trait Machine {
    fn system(&self) -> System<'_>;
}

/// A theme read from a file the user picked, waiting for `theme_import_confirm` (spec §3.1):
/// the file is read once, at the preview, and the token stands for what was read.
pub(crate) struct PendingImport {
    pub(crate) token: String,
    pub(crate) theme: Theme,
}

pub struct Backend<M> {
    machine: M,
    paths: Paths,
    resources: Resources,
    now: Box<dyn Fn() -> u64 + Send + Sync>,
    log: Logger,
    pub(crate) pending_import: Mutex<Option<PendingImport>>,
}

impl<M: Machine> Backend<M> {
    /// `data` is the data folder (spec §5.3), `resources` the bundled files, `now` the clock in
    /// milliseconds since 1970 (restore point ids and the log read it).
    pub fn new(
        machine: M,
        data: PathBuf,
        resources: PathBuf,
        now: Box<dyn Fn() -> u64 + Send + Sync>,
    ) -> Self {
        let paths = Paths { data };
        let log = Logger::new(paths.logs());
        Backend {
            machine,
            paths,
            resources: Resources { root: resources },
            now,
            log,
            pending_import: Mutex::new(None),
        }
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub(crate) fn sys(&self) -> System<'_> {
        self.machine.system()
    }

    pub(crate) fn now_ms(&self) -> u64 {
        (self.now)()
    }

    /// The core's context for one command: the paths, the bundled files and the clock now.
    pub(crate) fn ctx(&self) -> Ctx {
        Ctx {
            paths: self.paths.clone(),
            resources: self.resources.clone(),
            now_ms: self.now_ms(),
        }
    }

    /// Runs one command and writes one log line: its name, how long it took, and `ok` or the
    /// code it failed with. A log that cannot be written never stops the command.
    pub fn logged<T>(
        &self,
        name: &str,
        f: impl FnOnce(&Self) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let started = Instant::now();
        let result = f(self);
        let ms = started.elapsed().as_millis();
        let ended = match &result {
            Ok(_) => "ok".to_string(),
            Err(e) => format!("err={}", e.code),
        };
        let _ = self.log.write(
            self.sys().fs,
            self.now_ms(),
            &format!("cmd={name} ms={ms} {ended}"),
        );
        result
    }

    /// The last lines of the log, for the Advanced page and the diagnostics.
    pub(crate) fn log_tail(&self, lines: usize) -> String {
        self.log.tail(self.sys().fs, lines)
    }

    pub fn settings_get(&self) -> Result<SettingsLoad, AppError> {
        settings::load(self.sys().fs, &self.paths.settings_file(), self.now_ms())
    }

    /// Saves the settings and returns them as saved.
    pub fn settings_set(&self, settings: AppSettings) -> Result<AppSettings, AppError> {
        settings::save(self.sys().fs, &self.paths.settings_file(), &settings)?;
        Ok(settings)
    }

    /// The settings a command works from when it is not told otherwise; the defaults when the
    /// file cannot be used (the Settings page shows that problem itself).
    pub(crate) fn settings_or_default(&self) -> AppSettings {
        self.settings_get()
            .map(|loaded| loaded.settings)
            .unwrap_or_default()
    }

    /// The Advanced page (spec §4.3): the components' versions, the data folder, the log's
    /// last lines and the generated files, read only.
    pub fn advanced_read(&self) -> Result<AdvancedInfo, AppError> {
        let fs = self.sys().fs;
        let mut generated = Vec::new();
        let files = [
            self.paths.omp_config(),
            self.paths.generated().join("init.lua"),
            self.paths.generated().join("banner.lua"),
            self.paths.clink_profile().join("clink_settings"),
        ];
        for path in files {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let text = match fs.read(&path) {
                Ok(Some(bytes)) => String::from_utf8_lossy(&bytes).into_owned(),
                Ok(None) => continue,
                Err(e) => format!("(could not be read: {e})"),
            };
            generated.push(GeneratedFile { name, text });
        }
        Ok(AdvancedInfo {
            data_dir: self.paths.data.display().to_string(),
            log_tail: self.log_tail(200),
            generated,
            versions: versions(),
        })
    }
}

/// A line the UI shows about what a command found when it started (spec §5.5): a crash rolled
/// back, a restore point that cannot be read, a record of a rollback that did not finish, a
/// problem the start met. Thai from the code's row of spec §5.9, English from the core.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Note {
    pub code: String,
    pub message_th: String,
    pub message_en: String,
}

/// A note for `code` with the core's English text.
pub fn note(code: &str, en: impl Into<String>) -> Note {
    let th = crate::error::meaning(code)
        .map(|(th, _)| th.to_string())
        .unwrap_or_default();
    let en = en.into();
    Note {
        code: code.to_string(),
        message_th: if th.is_empty() { en.clone() } else { th },
        message_en: en,
    }
}

/// What a start found and did, as notes: the CLI's `Say::start` and `Say::unreadable`, in the
/// same words.
pub fn start_notes(start: &StartReport) -> Vec<Note> {
    let mut notes = Vec::new();
    match &start.recovered {
        Some(Outcome::RolledBack { code, reason }) => {
            notes.push(note(
                code,
                format!("{reason}; the machine is as it was before it."),
            ));
        }
        Some(Outcome::RollbackIncomplete { code, reason, .. }) => {
            notes.push(note(code, format!("{reason}.")));
        }
        Some(Outcome::Committed) | None => {}
    }
    for u in &start.unreadable {
        let (id, code, why) = (&u.point.id, u.point.failure.code, &u.point.failure.message);
        let en = match &u.handled {
            Handled::SetAside => format!(
                "restore point {id} could not be used ({why}). It was set aside, its files kept \
                 for a look, and an undo no longer goes back to it."
            ),
            Handled::LeftForLater => format!(
                "restore point {id} can't be read right now ({why}). It was left as it is, and \
                 an undo does not go past it until it can be read."
            ),
            Handled::SetAsideFailed(f) => format!(
                "restore point {id} could not be used ({why}), and setting it aside failed ({}: \
                 {}). An undo does not go past it.",
                f.code, f.message
            ),
        };
        notes.push(note(code, en));
    }
    for problem in &start.problems {
        notes.push(note(problem.code, problem.message.clone()));
    }
    if let Some(record) = &start.incomplete {
        notes.push(note(
            "E_ROLLBACK_INCOMPLETE",
            format!(
                "the rollback of restore point {} did not finish: {} ({})",
                record.id, record.reason, record.code
            ),
        ));
    }
    notes
}

/// The steps a rollback could not put back, from the record a start found (spec §5.5), each
/// with how to fix it by hand in both languages.
pub fn start_unrestored(start: &StartReport) -> Vec<Unrestored> {
    start
        .incomplete
        .as_ref()
        .map(|record| record.steps.clone())
        .unwrap_or_default()
}

/// The components' versions, for the Advanced page and the diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Versions {
    pub app: String,
    pub oh_my_posh: String,
    pub clink: String,
    pub nerd_fonts: String,
}

pub fn versions() -> Versions {
    Versions {
        app: env!("CARGO_PKG_VERSION").to_string(),
        oh_my_posh: OMP_VERSION.to_string(),
        clink: CLINK_VERSION.to_string(),
        nerd_fonts: NERD_FONTS_VERSION.to_string(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GeneratedFile {
    pub name: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdvancedInfo {
    pub data_dir: String,
    pub log_tail: String,
    pub generated: Vec<GeneratedFile>,
    pub versions: Versions,
}
```

- [ ] **Step 3: Write `app/src-tauri/src/machine.rs`** and add `#[cfg(windows)]` + `pub mod machine;` to `lib.rs` after `pub mod logs;`:

```rust
//! The real machine behind the app: the core's Windows adapters, wired as the CLI's `run` wires
//! them (`crates/cli/src/lib.rs`), so the app and the CLI see the same data folder, the same
//! bundled files beside the exe, and may run programs only from the runtime folders (spec §5.10).
use crate::backend::{Backend, Machine};
use cmdstudio_core::adapters::windows::{
    fonts::WinFonts, fs::WinFs, host::WinHost, process::WinProcess, registry::WinAutoRun, wt::WinWt,
};
use cmdstudio_core::adapters::{program_data_dir, Host, System};
use std::path::PathBuf;
use std::time::Duration;

pub struct RealMachine {
    fs: WinFs,
    autorun: WinAutoRun,
    fonts: WinFonts,
    wt: WinWt,
    process: WinProcess,
    host: WinHost,
}

impl Machine for RealMachine {
    fn system(&self) -> System<'_> {
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

/// The backend over this computer: `%LOCALAPPDATA%\ai.resincore.cmdstudio` as the data folder,
/// `<exe folder>\resources` as the bundled files, the clock. Fails only when Windows cannot say
/// where the user's local app data or the exe is.
pub fn real_backend() -> Result<Backend<RealMachine>, String> {
    let host = WinHost;
    let local = host
        .local_app_data()
        .map_err(|e| format!("the user's local app data folder could not be found: {e}"))?;
    let data = local.join(cmdstudio_cli::DATA_DIR_NAME);
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .ok_or_else(|| "the folder of cmdstudio.exe could not be found".to_string())?;
    // Only programs from the runtime folders may run, each for 60 seconds at most.
    let mut allowed = vec![data.join("runtime")];
    if let (Ok(pd), Ok(sid)) = (host.program_data(), host.user_sid()) {
        allowed.push(program_data_dir(&pd, &sid));
    }
    let machine = RealMachine {
        fs: WinFs,
        autorun: WinAutoRun::real(),
        fonts: WinFonts::real(&local),
        wt: WinWt::new(local.clone()),
        process: WinProcess::new(allowed, Duration::from_secs(60)),
        host,
    };
    Ok(Backend::new(
        machine,
        data,
        exe_dir.join("resources"),
        Box::new(cmdstudio_cli::now_ms),
    ))
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cmdstudio-app --lib backend::` → `6 passed` (2 tests + 4 export tests).
Run: `cargo test -p cmdstudio-app --lib` → `21 passed`.
Run: `cargo clippy -p cmdstudio-app --all-targets -- -D warnings`
Expected: only dead-code errors for items whose users come later in this plan — fields `resources` and `pending_import` of `Backend`, fields `token` and `theme` of `PendingImport` (Task 10), methods `ctx` (Task 12) and `settings_or_default` (Task 13), function `terracotta_id` (Task 10's tests) — and nothing else. `cargo fmt --all -- --check` clean.

- [ ] **Step 5: Commit**

Stage `app/src-tauri/src/backend/mod.rs app/src-tauri/src/machine.rs app/src-tauri/src/lib.rs`. Message: `app: Backend over a Machine (fake in tests, Windows adapters in the app); settings and Advanced commands`.

---

### Task 9: `system_detect`

What the wizard's second page and the editor need to know about the machine (spec §4.2 page 2, §3.4 fonts): Windows build and architecture, whether the app runs packaged, every Windows Terminal found and whether its `settings.json` exists, the Store's availability (build ≥ 19041), the AutoRun summary, the fonts (Nerd Font installed? which monospace families), the data folder and the versions. Read from the same `detect` the planner reads; what cannot be read is listed in `unreadable`, never guessed.

**Files:**
- Create: `app/src-tauri/src/backend/system.rs`
- Modify: `app/src-tauri/src/backend/mod.rs` (+ `pub mod system;`)
- Test: `the_report_reads_the_machine_the_planner_reads`, `another_programs_autorun_and_another_clink_are_told_apart`, `an_old_windows_without_terminal_gets_no_store_link` (+ 5 export tests)

**Interfaces:**
- Consumes: `cmdstudio_core::detect` (Windows build, packaged, WT locations, fonts), `plan::{autorun_summary, AutoRunSummary, NERD_FONT_FAMILY}` (Task 2), `adapters::{WtLocation, WtVariant}`, `backend::{versions, Versions}`.
- Produces:
  ```rust
  pub const WT_MIN_BUILD: u32 = 19041;
  pub const KNOWN_MONOSPACE: &[&str];   // the families the editor offers when the machine's list cannot be read
  pub enum WtKind { Store, Preview, Unpackaged }
  pub struct WtInfo { pub kind: WtKind, pub installed: bool, pub has_settings: bool }
  pub struct AutoRunInfo { pub other_commands: Vec<String>, pub foreign_clink: Vec<String>, pub ours_present: bool, pub machine_wide_clink: bool, pub not_text: bool }
  pub struct FontInfo { pub nerd_font_installed: bool, pub monospace: Vec<String> }
  pub struct SystemReport { pub build: Option<u32>, pub arch: String, pub packaged: bool, pub unreadable: Vec<String>, pub windows_terminal: Vec<WtInfo>, pub wt_installed: bool, pub store_available: bool, pub autorun: AutoRunInfo, pub fonts: FontInfo, pub data_dir: String, pub versions: Versions }
  impl<M: Machine> Backend<M> { pub fn system_detect(&self) -> Result<SystemReport, AppError> }
  ```

- [ ] **Step 1: Write the failing tests** — `backend/system.rs` with only its test module, and `pub mod system;` in `backend/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{backend, DATA};
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::adapters::{AutoRun, RegValue, REG_SZ};
    use std::path::PathBuf;

    #[test]
    fn the_report_reads_the_machine_the_planner_reads() {
        let b = backend();
        let r = b.system_detect().unwrap();
        assert_eq!(
            (r.build, r.arch.as_str(), r.packaged),
            (Some(26100), "x64", false)
        );
        assert!(r.unreadable.is_empty());
        assert_eq!(r.windows_terminal.len(), 1);
        assert_eq!(
            r.windows_terminal[0],
            WtInfo {
                kind: WtKind::Store,
                installed: true,
                has_settings: true
            }
        );
        assert!(r.wt_installed && r.store_available);
        assert_eq!(r.autorun, AutoRunInfo::default_for_test());
        assert_eq!(r.fonts.monospace, ["Cascadia Mono", "Consolas"]);
        assert!(!r.fonts.nerd_font_installed);
        assert_eq!(r.data_dir, DATA);
        assert_eq!(r.versions.oh_my_posh, "31.5.0");
    }

    impl AutoRunInfo {
        fn default_for_test() -> Self {
            AutoRunInfo {
                other_commands: Vec::new(),
                foreign_clink: Vec::new(),
                ours_present: false,
                machine_wide_clink: false,
                not_text: false,
            }
        }
    }

    #[test]
    fn another_programs_autorun_and_another_clink_are_told_apart() {
        let b = backend();
        let value = r#"if exist "C:\conda\conda_hook.bat" "C:\conda\conda_hook.bat" & "C:\clink\clink_x64.exe" inject --autorun"#;
        let m: &FakeMachine = &b.machine;
        m.autorun.set_machine(None);
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, value))
            .unwrap();
        let r = b.system_detect().unwrap();
        assert_eq!(
            r.autorun.other_commands,
            [r#"if exist "C:\conda\conda_hook.bat" "C:\conda\conda_hook.bat""#]
        );
        assert_eq!(
            r.autorun.foreign_clink,
            [r#""C:\clink\clink_x64.exe" inject --autorun"#]
        );
        assert!(!r.autorun.ours_present && !r.autorun.machine_wide_clink);
    }

    #[test]
    fn an_old_windows_without_terminal_gets_no_store_link() {
        let mut b = backend();
        b.machine.host.info.build = Some(18363);
        b.machine.wt.locations = vec![];
        let r = b.system_detect().unwrap();
        assert!(!r.wt_installed && !r.store_available);
        b.machine.host.info.build = None;
        b.machine.host.info.unreadable = vec!["build: access denied".to_string()];
        let r = b.system_detect().unwrap();
        assert!(r.build.is_none() && !r.store_available);
        assert_eq!(r.unreadable, ["build: access denied"]);
        let _ = PathBuf::new();
    }
}
```

Run: `cargo test -p cmdstudio-app --lib system::`
Expected: FAILS to compile (`SystemReport`, `system_detect` not found).

- [ ] **Step 2: Write the code above the test module**

```rust
//! `system_detect`: what the wizard's machine check (spec §4.2 page 2), the home page's status
//! bar and the editor's font list need to know about this computer, from the core's `detect`
//! and `plan::autorun_summary`. Read only.
use super::{versions, Backend, Machine, Versions};
use crate::error::AppError;
use cmdstudio_core::adapters::{Arch, WtVariant};
use cmdstudio_core::detect::detect;
use cmdstudio_core::plan::{autorun_summary, NERD_FONT_FAMILY};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Windows 10 2004, the oldest Windows that Windows Terminal supports (G18).
pub const WT_MIN_BUILD: u32 = 19041;

/// Font families the editor offers when they are installed (spec §3.4: monospace fonts on the
/// machine, and the bundled Nerd Font). GDI does not tell the app which families are
/// monospace, so this is the list of common ones; the editor also takes any family name typed.
pub const KNOWN_MONOSPACE: &[&str] = &[
    "CaskaydiaCove NF",
    "Cascadia Mono",
    "Cascadia Code",
    "Consolas",
    "Courier New",
    "Lucida Console",
    "Fira Code",
    "FiraCode Nerd Font",
    "JetBrains Mono",
    "JetBrainsMono Nerd Font",
    "Source Code Pro",
    "SauceCodePro Nerd Font",
    "Hack",
    "Hack Nerd Font",
    "Iosevka",
    "IBM Plex Mono",
    "Roboto Mono",
    "Ubuntu Mono",
    "DejaVu Sans Mono",
    "Noto Sans Mono",
    "Inconsolata",
    "Victor Mono",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum WtKind {
    Store,
    Preview,
    Unpackaged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WtInfo {
    pub kind: WtKind,
    /// Its folder exists.
    pub installed: bool,
    /// Its settings.json exists (one that was never opened has none, D6).
    pub has_settings: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AutoRunInfo {
    pub other_commands: Vec<String>,
    pub foreign_clink: Vec<String>,
    pub ours_present: bool,
    pub machine_wide_clink: bool,
    pub not_text: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FontInfo {
    /// The bundled Nerd Font (`CaskaydiaCove NF`) is installed for this account.
    pub nerd_font_installed: bool,
    /// Installed families from [`KNOWN_MONOSPACE`], in that order.
    pub monospace: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SystemReport {
    /// `CurrentBuildNumber`; absent when it could not be read (see `unreadable`).
    pub build: Option<u32>,
    pub arch: String,
    pub packaged: bool,
    pub unreadable: Vec<String>,
    pub windows_terminal: Vec<WtInfo>,
    /// Any Windows Terminal is installed.
    pub wt_installed: bool,
    /// The Store page of Windows Terminal applies: the build is known and at least 19041.
    pub store_available: bool,
    pub autorun: AutoRunInfo,
    pub fonts: FontInfo,
    pub data_dir: String,
    pub versions: Versions,
}

impl<M: Machine> Backend<M> {
    pub fn system_detect(&self) -> Result<SystemReport, AppError> {
        let sys = self.sys();
        let detected = detect(sys, self.paths())?;
        let autorun = autorun_summary(&detected.machine, self.paths());
        let windows_terminal: Vec<WtInfo> = detected
            .wt
            .iter()
            .map(|l| WtInfo {
                kind: match l.variant {
                    WtVariant::Store => WtKind::Store,
                    WtVariant::Preview => WtKind::Preview,
                    WtVariant::Unpackaged => WtKind::Unpackaged,
                },
                installed: l.installed,
                has_settings: sys.fs.exists(&l.settings),
            })
            .collect();
        let families = &detected.machine.fonts;
        let has = |family: &str| families.iter().any(|f| f.eq_ignore_ascii_case(family));
        let monospace = KNOWN_MONOSPACE
            .iter()
            .filter(|f| has(f))
            .map(|f| f.to_string())
            .collect();
        Ok(SystemReport {
            build: detected.info.build,
            arch: match detected.info.arch {
                Arch::X64 => "x64",
                Arch::Arm64 => "arm64",
                Arch::Other => "other",
            }
            .to_string(),
            packaged: detected.info.packaged,
            unreadable: detected.info.unreadable.clone(),
            wt_installed: windows_terminal.iter().any(|w| w.installed),
            store_available: detected.info.build.is_some_and(|b| b >= WT_MIN_BUILD),
            windows_terminal,
            autorun: AutoRunInfo {
                other_commands: autorun.other_commands,
                foreign_clink: autorun.foreign_clink,
                ours_present: autorun.ours_present,
                machine_wide_clink: autorun.machine_wide_clink,
                not_text: autorun.not_text,
            },
            fonts: FontInfo {
                nerd_font_installed: has(NERD_FONT_FAMILY),
                monospace,
            },
            data_dir: self.paths().data.display().to_string(),
            versions: versions(),
        })
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p cmdstudio-app --lib system::` → `8 passed`. `cargo test -p cmdstudio-app --lib` → `29 passed`. Clippy: the same dead-code list as Task 8, nothing else. fmt clean.

- [ ] **Step 4: Commit**

Stage `app/src-tauri/src/backend/system.rs app/src-tauri/src/backend/mod.rs`. Message: `app: system_detect — what the wizard and the editor need to know about the machine`.

---

### Task 10: The theme library: list, get, save, duplicate, delete, import, export

Spec §3.1 and §3.3; index Decision 7. Presets come from `presets::builtin()` and are never written; my themes are files in `themes\`; drafts in `drafts\<id>.cmdtheme`. `theme_save(theme, draft)` validates through the core's `Theme` deserialization and refuses a preset's id; `theme_import_preview_from(path)` reads and validates the file once and keeps it under a token until `theme_import_confirm(token)`; `theme_export_to(id, path)` writes the theme as its file. The IPC layer (Task 15) adds the native dialogs around import and export.

**Files:**
- Create: `app/src-tauri/src/library.rs`
- Modify: `app/src-tauri/src/lib.rs` (+ `pub mod library;` after `pub mod error;` — before `pub mod logs;`)
- Test: `the_library_lists_the_twelve_presets_first_in_their_order`, `customize_copies_a_preset_into_my_themes_with_a_new_id_and_a_safe_file`, `a_preset_is_not_saved_or_deleted_in_place`, `a_rename_keeps_the_file_and_a_draft_comes_and_goes`, `a_draft_of_a_theme_never_saved_is_listed_as_a_draft`, `a_theme_that_does_not_validate_is_refused_with_its_field`, `files_that_are_not_themes_are_problems_not_themes`, `import_previews_first_and_gives_a_taken_id_a_new_one`, `export_writes_the_theme_as_a_file_the_import_takes_back` (+ 7 export tests). `the_applied_theme_is_marked_in_use` is in Task 12 (it applies first).

**Interfaces:**
- Consumes: `cmdstudio_core::presets::builtin()`, `model::{Theme, ThemeError, Target}`, `state::State` (the applied theme, for `in_use`), `adapters::Fs`, `backend::{Backend, PendingImport}`, `error::AppError`.
- Produces:
  ```rust
  pub const THEME_EXT: &str = "cmdtheme";
  pub enum ThemeSource { Preset, Mine, Draft }
  pub struct ThemeSummary { pub id: Uuid, pub name: String, pub tone: Tone, pub layout: Layout, pub source: ThemeSource, pub in_use: bool, pub has_draft: bool }
  pub struct LibraryProblem { pub file: String, pub message: String }
  pub struct AppliedInfo { pub theme_id: Uuid, pub theme_name: String, pub target: Target, pub applied_at_ms: u64, pub snapshot_id: String }
  pub struct Library { pub themes: Vec<ThemeSummary>, pub applied: Option<AppliedInfo>, pub problems: Vec<LibraryProblem>, pub state_problem: Option<AppError> }
  pub struct ThemeDetail { pub theme: Theme, pub source: ThemeSource, pub draft: Option<Theme> }
  pub struct ImportPreview { pub token: String, pub theme: Theme, pub id_taken: bool, pub name_taken: bool }
  impl<M: Machine> Backend<M> {
      pub fn themes_list(&self) -> Result<Library, AppError>                 // presets in their order, then mine by name, then orphan drafts
      pub fn theme_get(&self, id: Uuid) -> Result<ThemeDetail, AppError>
      pub fn theme_save(&self, theme: Theme, draft: bool) -> Result<ThemeSummary, AppError>   // draft=false also deletes the draft
      pub fn theme_duplicate(&self, id: Uuid, name: String) -> Result<ThemeSummary, AppError>
      pub fn theme_delete(&self, id: Uuid, draft_only: bool) -> Result<(), AppError>
      pub fn theme_import_preview_from(&self, path: &Path) -> Result<ImportPreview, AppError>
      pub fn theme_import_confirm(&self, token: &str) -> Result<ThemeSummary, AppError>
      pub fn export_file_name(&self, id: Uuid) -> Result<String, AppError>  // "<name>.cmdtheme", made safe
      pub fn theme_export_to(&self, id: Uuid, path: &Path) -> Result<(), AppError>
      pub(crate) fn find_theme(&self, id: Uuid) -> Result<(Theme, ThemeSource, Option<PathBuf>), AppError>
      pub(crate) fn applied_info(&self) -> (Option<AppliedInfo>, Option<AppError>)
      pub(crate) fn draft_path(&self, id: Uuid) -> PathBuf
  }
  ```

- [ ] **Step 1: Write the failing tests** — `library.rs` with only its test module, `pub mod library;` in `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{backend, terracotta_id, DATA};

    fn names(lib: &Library) -> Vec<String> {
        lib.themes.iter().map(|t| t.name.clone()).collect()
    }

    #[test]
    fn the_library_lists_the_twelve_presets_first_in_their_order() {
        let b = backend();
        let lib = b.themes_list().unwrap();
        assert_eq!(lib.themes.len(), 12);
        assert!(lib
            .themes
            .iter()
            .all(|t| t.source == ThemeSource::Preset && !t.in_use && !t.has_draft));
        assert_eq!(lib.themes[0].name, "Terracotta");
        assert_eq!(lib.themes[11].name, "Paper");
        assert!(lib.applied.is_none() && lib.problems.is_empty() && lib.state_problem.is_none());
    }

    #[test]
    fn customize_copies_a_preset_into_my_themes_with_a_new_id_and_a_safe_file() {
        let b = backend();
        let copy = b
            .theme_duplicate(terracotta_id(), "My Terracotta".to_string())
            .unwrap();
        assert_ne!(copy.id, terracotta_id());
        assert_eq!(
            (copy.source, copy.name.as_str()),
            (ThemeSource::Mine, "My Terracotta")
        );
        let file = format!(r"{DATA}\themes\My Terracotta.cmdtheme");
        let saved = parse_theme(&b.sys().fs.read(Path::new(&file)).unwrap().unwrap()).unwrap();
        assert_eq!((saved.id, saved.name.as_str()), (copy.id, "My Terracotta"));
        // The same name again gets its own file; a name with characters a file cannot hold is
        // made safe; the themes come after the presets, by name.
        let again = b
            .theme_duplicate(terracotta_id(), "My Terracotta".to_string())
            .unwrap();
        assert!(b.sys().fs.exists(Path::new(&format!(
            r"{DATA}\themes\My Terracotta (2).cmdtheme"
        ))));
        let odd = b
            .theme_duplicate(terracotta_id(), r#"a/b:"c"?"#.to_string())
            .unwrap();
        assert!(b
            .sys()
            .fs
            .exists(Path::new(&format!(r"{DATA}\themes\a_b__c__.cmdtheme"))));
        let lib = b.themes_list().unwrap();
        assert_eq!(lib.themes.len(), 15);
        assert_eq!(
            &names(&lib)[12..],
            ["a/b:\"c\"?", "My Terracotta", "My Terracotta"]
        );
        assert_eq!(b.find_theme(again.id).unwrap().1, ThemeSource::Mine);
        assert_eq!(b.find_theme(odd.id).unwrap().1, ThemeSource::Mine);
    }

    #[test]
    fn a_preset_is_not_saved_or_deleted_in_place() {
        let b = backend();
        let (preset, _, _) = b.find_theme(terracotta_id()).unwrap();
        let e = b.theme_save(preset, false).unwrap_err();
        assert_eq!(e.code, "E_THEME_INVALID");
        assert!(
            e.details.contains("Customize makes a copy"),
            "{}",
            e.details
        );
        assert_eq!(
            b.theme_delete(terracotta_id(), false).unwrap_err().code,
            "E_THEME_INVALID"
        );
        assert_eq!(b.themes_list().unwrap().themes.len(), 12);
    }

    #[test]
    fn a_rename_keeps_the_file_and_a_draft_comes_and_goes() {
        let b = backend();
        let copy = b
            .theme_duplicate(terracotta_id(), "Mine".to_string())
            .unwrap();
        let (mut theme, _, path) = b.find_theme(copy.id).unwrap();
        let path = path.unwrap();
        // A draft with a new name: the file in themes\ is untouched, the draft is listed.
        theme.name = "Mine, renamed".to_string();
        let drafted = b.theme_save(theme.clone(), true).unwrap();
        assert!(drafted.has_draft && drafted.source == ThemeSource::Mine);
        let detail = b.theme_get(copy.id).unwrap();
        assert_eq!(detail.theme.name, "Mine");
        assert_eq!(detail.draft.unwrap().name, "Mine, renamed");
        assert!(b.sys().fs.exists(&b.draft_path(copy.id)));
        // Saving for real keeps the file name and drops the draft.
        let saved = b.theme_save(theme, false).unwrap();
        assert_eq!(
            (saved.name.as_str(), saved.has_draft),
            ("Mine, renamed", false)
        );
        assert!(b.sys().fs.exists(&path));
        assert!(!b.sys().fs.exists(&b.draft_path(copy.id)));
        assert_eq!(b.theme_get(copy.id).unwrap().theme.name, "Mine, renamed");
        // Deleting takes the file and any draft.
        b.theme_save(b.find_theme(copy.id).unwrap().0, true)
            .unwrap();
        b.theme_delete(copy.id, false).unwrap();
        assert!(!b.sys().fs.exists(&path) && !b.sys().fs.exists(&b.draft_path(copy.id)));
        assert_eq!(b.find_theme(copy.id).unwrap_err().code, "E_THEME_INVALID");
    }

    #[test]
    fn a_draft_of_a_theme_never_saved_is_listed_as_a_draft() {
        let b = backend();
        let (mut theme, _, _) = b.find_theme(terracotta_id()).unwrap();
        theme.id = Uuid::new_v4();
        theme.name = "Brand new".to_string();
        let summary = b.theme_save(theme.clone(), true).unwrap();
        assert_eq!(
            (summary.source, summary.has_draft),
            (ThemeSource::Draft, true)
        );
        let lib = b.themes_list().unwrap();
        assert_eq!(
            lib.themes.last().map(|t| (t.source, t.name.as_str())),
            Some((ThemeSource::Draft, "Brand new"))
        );
        let detail = b.theme_get(theme.id).unwrap();
        assert_eq!((detail.source, detail.draft), (ThemeSource::Draft, None));
        assert_eq!(detail.theme.name, "Brand new");
        // Discarding the draft is deleting it.
        b.theme_delete(theme.id, true).unwrap();
        assert_eq!(b.themes_list().unwrap().themes.len(), 12);
        // Saving it for real makes it one of mine.
        let saved = b.theme_save(theme, false).unwrap();
        assert_eq!(saved.source, ThemeSource::Mine);
    }

    #[test]
    fn a_theme_that_does_not_validate_is_refused_with_its_field() {
        let b = backend();
        let (mut theme, _, _) = b.find_theme(terracotta_id()).unwrap();
        theme.id = Uuid::new_v4();
        theme.name = "   ".to_string();
        let e = b.theme_save(theme, true).unwrap_err();
        assert_eq!(e.code, "E_THEME_INVALID");
        assert!(e.details.contains("name"), "{}", e.details);
        assert!(b.themes_list().unwrap().themes.len() == 12);
    }

    #[test]
    fn files_that_are_not_themes_are_problems_not_themes() {
        let b = backend();
        let themes = b.paths().themes();
        b.sys()
            .fs
            .write_atomic(&themes.join("broken.cmdtheme"), b"{ not json")
            .unwrap();
        let (preset, _, _) = b.find_theme(terracotta_id()).unwrap();
        b.sys()
            .fs
            .write_atomic(
                &themes.join("copy-of-preset.cmdtheme"),
                &preset.to_file_bytes().unwrap(),
            )
            .unwrap();
        let copy = b
            .theme_duplicate(terracotta_id(), "Mine".to_string())
            .unwrap();
        let (mine, _, path) = b.find_theme(copy.id).unwrap();
        b.sys()
            .fs
            .write_atomic(
                &themes.join("zz-same-id.cmdtheme"),
                &mine.to_file_bytes().unwrap(),
            )
            .unwrap();
        b.sys()
            .fs
            .write_atomic(&themes.join("notes.txt"), b"not a theme")
            .unwrap();
        let lib = b.themes_list().unwrap();
        assert_eq!(lib.themes.len(), 13, "{:?}", names(&lib));
        let files: Vec<&str> = lib.problems.iter().map(|p| p.file.as_str()).collect();
        assert_eq!(
            files,
            [
                "broken.cmdtheme",
                "copy-of-preset.cmdtheme",
                "zz-same-id.cmdtheme"
            ]
        );
        assert!(
            lib.problems[0].message.starts_with("E_THEME_INVALID: "),
            "{}",
            lib.problems[0].message
        );
        assert!(
            lib.problems[2].message.contains(&name_of(&path.unwrap())),
            "{}",
            lib.problems[2].message
        );
    }

    #[test]
    fn import_previews_first_and_gives_a_taken_id_a_new_one() {
        let b = backend();
        let (preset, _, _) = b.find_theme(terracotta_id()).unwrap();
        let file = Path::new(r"C:\Users\tester\Downloads\terracotta.cmdtheme");
        b.sys()
            .fs
            .write_atomic(file, &preset.to_file_bytes().unwrap())
            .unwrap();
        let preview = b.theme_import_preview_from(file).unwrap();
        assert!(preview.id_taken && preview.name_taken);
        assert_eq!(preview.theme.name, "Terracotta");
        // Nothing is in the library until the confirm.
        assert_eq!(b.themes_list().unwrap().themes.len(), 12);
        let added = b.theme_import_confirm(&preview.token).unwrap();
        assert_ne!(added.id, terracotta_id());
        assert_eq!(
            (added.source, added.name.as_str()),
            (ThemeSource::Mine, "Terracotta")
        );
        assert_eq!(b.themes_list().unwrap().themes.len(), 13);
        // A token is good once; a wrong token changes nothing.
        assert_eq!(
            b.theme_import_confirm(&preview.token).unwrap_err().code,
            "E_THEME_INVALID"
        );
        // A file that is not a theme, and one that is not there.
        b.sys()
            .fs
            .write_atomic(file, b"{\"schemaVersion\": 99}")
            .unwrap();
        assert_eq!(
            b.theme_import_preview_from(file).unwrap_err().code,
            "E_THEME_NEWER_SCHEMA"
        );
        assert_eq!(
            b.theme_import_preview_from(Path::new(r"C:\nowhere.cmdtheme"))
                .unwrap_err()
                .code,
            "E_THEME_INVALID"
        );
    }

    #[test]
    fn export_writes_the_theme_as_a_file_the_import_takes_back() {
        let b = backend();
        let copy = b
            .theme_duplicate(terracotta_id(), "ธีม ของฉัน".to_string())
            .unwrap();
        assert_eq!(b.export_file_name(copy.id).unwrap(), "ธีม ของฉัน.cmdtheme");
        let out = Path::new(r"C:\Users\tester\Desktop\out.cmdtheme");
        b.theme_export_to(copy.id, out).unwrap();
        let (theme, _, _) = b.find_theme(copy.id).unwrap();
        assert_eq!(
            b.sys().fs.read(out).unwrap().unwrap(),
            theme.to_file_bytes().unwrap()
        );
        let back = b.theme_import_preview_from(out).unwrap();
        assert!(back.id_taken);
        assert_eq!(back.theme, theme);
    }
}
```

Run: `cargo test -p cmdstudio-app --lib library::`
Expected: FAILS to compile (`themes_list`, `Library`, `ThemeSource` not found).

- [ ] **Step 2: Write the code above the test module**

```rust
//! The theme library (spec §3.1): the twelve presets, embedded in the core; "my themes" as
//! `*.cmdtheme` files in `<data>\themes`, the id inside each file being its identity; the
//! editor's autosaved drafts in `<data>\drafts\<id>.cmdtheme` (spec §3.4); import, preview
//! first, with a new id when the file's id is taken (spec §3.3); export. A preset is never
//! changed in place: "Customize" copies it into my themes.
use crate::backend::{Backend, Machine, PendingImport};
use crate::error::AppError;
use cmdstudio_core::adapters::AdapterError;
use cmdstudio_core::model::{parse_theme, Layout, Target, Theme};
use cmdstudio_core::palette::Tone;
use cmdstudio_core::presets;
use cmdstudio_core::state::State;
use cmdstudio_core::text::safe_file_stem;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;
use uuid::Uuid;

pub const THEME_EXT: &str = "cmdtheme";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ThemeSource {
    /// Built in; customize to change.
    Preset,
    /// A file in `themes\`.
    Mine,
    /// Only a draft in `drafts\`: a new theme that was never saved.
    Draft,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThemeSummary {
    pub id: Uuid,
    pub name: String,
    pub tone: Tone,
    pub layout: Layout,
    pub source: ThemeSource,
    /// The theme state.json says is applied (spec §3.1 "ใช้อยู่").
    pub in_use: bool,
    /// A draft of it waits in `drafts\`.
    pub has_draft: bool,
}

/// A file in `themes\` that is not a theme of the library, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LibraryProblem {
    pub file: String,
    pub message: String,
}

/// The applied theme as state.json records it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppliedInfo {
    pub theme_id: Uuid,
    pub theme_name: String,
    pub target: Target,
    #[ts(type = "number")]
    pub applied_at_ms: u64,
    pub snapshot_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Library {
    /// Presets in their fixed order, then my themes by name, then drafts of unsaved themes.
    pub themes: Vec<ThemeSummary>,
    pub applied: Option<AppliedInfo>,
    pub problems: Vec<LibraryProblem>,
    /// state.json could not be read, so `applied` and `in_use` are unknown (doctor D9 says more).
    pub state_problem: Option<AppError>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThemeDetail {
    pub theme: Theme,
    pub source: ThemeSource,
    /// The autosaved draft, when there is one and it reads.
    pub draft: Option<Theme>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportPreview {
    /// Stands for the theme read from the file, for `theme_import_confirm`.
    pub token: String,
    pub theme: Theme,
    /// Its id is already in the library: the copy gets a new one (spec §3.3).
    pub id_taken: bool,
    /// A theme of that name is already in the library.
    pub name_taken: bool,
}

fn has_theme_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(THEME_EXT))
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn same_file_name(a: &Path, b: &Path) -> bool {
    match (
        a.file_name().and_then(|n| n.to_str()),
        b.file_name().and_then(|n| n.to_str()),
    ) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    }
}

/// My themes as read from `themes`: each file with its theme, and the files that are not themes.
type Mine = (Vec<(PathBuf, Theme)>, Vec<LibraryProblem>);

fn data_write(e: AdapterError) -> AppError {
    AppError::coded("E_DATA_WRITE", e.to_string())
}

fn no_such_theme(id: Uuid) -> AppError {
    AppError::coded(
        "E_THEME_INVALID",
        format!("there is no theme with the id {id}; it may have been deleted"),
    )
}

impl<M: Machine> Backend<M> {
    fn presets(&self) -> Result<Vec<Theme>, AppError> {
        Ok(presets::builtin()?.into_iter().map(|(_, t)| t).collect())
    }

    /// My themes: every `.cmdtheme` in `themes\`, in file-name order. A file that does not
    /// parse, one whose id another file (or a preset) already has, is a problem, not a theme.
    fn mine(&self) -> Result<Mine, AppError> {
        let fs = self.sys().fs;
        let dir = self.paths().themes();
        let entries = fs.list_dir(&dir).map_err(|e| {
            AppError::coded(
                "E_MACHINE_READ",
                format!("{} could not be listed: {e}", dir.display()),
            )
        })?;
        let preset_ids: Vec<Uuid> = self.presets()?.iter().map(|t| t.id).collect();
        let mut themes: Vec<(PathBuf, Theme)> = Vec::new();
        let mut problems = Vec::new();
        for entry in entries
            .into_iter()
            .filter(|e| !e.is_dir && has_theme_ext(&e.path))
        {
            let file = name_of(&entry.path);
            let bytes = match fs.read(&entry.path) {
                Ok(Some(bytes)) => bytes,
                Ok(None) => continue,
                Err(e) => {
                    problems.push(LibraryProblem {
                        file,
                        message: format!("could not be read: {e}"),
                    });
                    continue;
                }
            };
            let theme = match parse_theme(&bytes) {
                Ok(theme) => theme,
                Err(e) => {
                    problems.push(LibraryProblem {
                        file,
                        message: format!("{}: {e}", e.code()),
                    });
                    continue;
                }
            };
            if preset_ids.contains(&theme.id) {
                problems.push(LibraryProblem {
                    file,
                    message: "has the id of a built-in preset; give it a new id or delete it"
                        .to_string(),
                });
            } else if let Some((first, _)) = themes.iter().find(|(_, t)| t.id == theme.id) {
                problems.push(LibraryProblem {
                    file,
                    message: format!(
                        "has the same id as {}, which CMD Studio shows; give this file a new id \
                         or delete it",
                        name_of(first)
                    ),
                });
            } else {
                themes.push((entry.path, theme));
            }
        }
        Ok((themes, problems))
    }

    /// Drafts in `drafts\` whose id is no theme of the library: new themes never saved.
    fn orphan_drafts(&self, known: &[Uuid]) -> Vec<(PathBuf, Theme)> {
        let fs = self.sys().fs;
        let Ok(entries) = fs.list_dir(&self.paths().drafts()) else {
            return Vec::new();
        };
        entries
            .into_iter()
            .filter(|e| !e.is_dir && has_theme_ext(&e.path))
            .filter_map(|e| {
                let bytes = fs.read(&e.path).ok().flatten()?;
                let theme = parse_theme(&bytes).ok()?;
                (!known.contains(&theme.id)).then_some((e.path, theme))
            })
            .collect()
    }

    pub(crate) fn applied_info(&self) -> (Option<AppliedInfo>, Option<AppError>) {
        match State::load(self.sys().fs, &self.paths().state_file()) {
            Ok(state) => (
                state.applied.map(|a| AppliedInfo {
                    theme_id: a.theme_id,
                    theme_name: a.theme_name,
                    target: a.target,
                    applied_at_ms: a.applied_at_ms,
                    snapshot_id: a.snapshot_id,
                }),
                None,
            ),
            Err(e) => (None, Some(AppError::from(e))),
        }
    }

    fn summary(&self, theme: &Theme, source: ThemeSource, in_use: Option<Uuid>) -> ThemeSummary {
        ThemeSummary {
            id: theme.id,
            name: theme.name.clone(),
            tone: theme.tone,
            layout: theme.prompt.layout,
            source,
            in_use: in_use == Some(theme.id),
            has_draft: self.sys().fs.exists(&self.draft_path(theme.id)),
        }
    }

    pub fn themes_list(&self) -> Result<Library, AppError> {
        let (applied, state_problem) = self.applied_info();
        let in_use = applied.as_ref().map(|a| a.theme_id);
        let mut themes: Vec<ThemeSummary> = self
            .presets()?
            .iter()
            .map(|t| self.summary(t, ThemeSource::Preset, in_use))
            .collect();
        let (mine, problems) = self.mine()?;
        let mut mine: Vec<ThemeSummary> = mine
            .iter()
            .map(|(_, t)| self.summary(t, ThemeSource::Mine, in_use))
            .collect();
        mine.sort_by_key(|t| t.name.to_lowercase());
        themes.extend(mine);
        let known: Vec<Uuid> = themes.iter().map(|t| t.id).collect();
        let mut drafts: Vec<ThemeSummary> = self
            .orphan_drafts(&known)
            .iter()
            .map(|(_, t)| self.summary(t, ThemeSource::Draft, in_use))
            .collect();
        drafts.sort_by_key(|t| t.name.to_lowercase());
        themes.extend(drafts);
        Ok(Library {
            themes,
            applied,
            problems,
            state_problem,
        })
    }

    /// A theme by id: a preset, one of mine (with its file), or an unsaved draft (with its
    /// file). `E_THEME_INVALID` when there is none.
    pub(crate) fn find_theme(
        &self,
        id: Uuid,
    ) -> Result<(Theme, ThemeSource, Option<PathBuf>), AppError> {
        if let Some(t) = self.presets()?.into_iter().find(|t| t.id == id) {
            return Ok((t, ThemeSource::Preset, None));
        }
        let (mine, _) = self.mine()?;
        if let Some((path, theme)) = mine.into_iter().find(|(_, t)| t.id == id) {
            return Ok((theme, ThemeSource::Mine, Some(path)));
        }
        self.orphan_drafts(&[])
            .into_iter()
            .find(|(_, t)| t.id == id)
            .map(|(path, theme)| (theme, ThemeSource::Draft, Some(path)))
            .ok_or_else(|| no_such_theme(id))
    }

    pub fn theme_get(&self, id: Uuid) -> Result<ThemeDetail, AppError> {
        let (theme, source, _) = self.find_theme(id)?;
        let draft = match source {
            // The draft is the theme itself.
            ThemeSource::Draft => None,
            _ => match self.sys().fs.read(&self.draft_path(id)) {
                Ok(Some(bytes)) => parse_theme(&bytes).ok(),
                _ => None,
            },
        };
        Ok(ThemeDetail {
            theme,
            source,
            draft,
        })
    }

    pub(crate) fn draft_path(&self, id: Uuid) -> PathBuf {
        self.paths().drafts().join(format!("{id}.{THEME_EXT}"))
    }

    /// Saves a theme of mine (`draft: false`: into `themes\`, and its draft goes) or its draft
    /// (`draft: true`: into `drafts\`). A preset is refused: "Customize" copies it first (spec
    /// §3.1). The file of a theme keeps the name it got when the theme was first saved; a new
    /// one is named after the theme (`text::safe_file_stem`), with ` (2)`, ` (3)`… when that name
    /// is taken. What is written is `Theme::to_file_bytes`: clean, within the limits, so a
    /// theme that does not validate is refused here with the field at fault.
    pub fn theme_save(&self, theme: Theme, draft: bool) -> Result<ThemeSummary, AppError> {
        let bytes = theme.to_file_bytes()?;
        let fs = self.sys().fs;
        if self.presets()?.iter().any(|p| p.id == theme.id) {
            return Err(AppError::coded(
                "E_THEME_INVALID",
                format!(
                    "\"{}\" is a built-in preset, which is not changed in place; Customize makes \
                     a copy of it in My themes",
                    theme.name
                ),
            ));
        }
        let in_use = self.applied_info().0.map(|a| a.theme_id);
        if draft {
            fs.write_atomic(&self.draft_path(theme.id), &bytes)
                .map_err(data_write)?;
            let source = match self.mine()?.0.iter().any(|(_, t)| t.id == theme.id) {
                true => ThemeSource::Mine,
                false => ThemeSource::Draft,
            };
            return Ok(self.summary(&theme, source, in_use));
        }
        let (mine, _) = self.mine()?;
        let path = match mine.iter().find(|(_, t)| t.id == theme.id) {
            Some((path, _)) => path.clone(),
            None => self.new_theme_file(&theme.name, &mine),
        };
        fs.write_atomic(&path, &bytes).map_err(data_write)?;
        let draft_file = self.draft_path(theme.id);
        if fs.exists(&draft_file) {
            fs.remove_file(&draft_file).map_err(data_write)?;
        }
        Ok(self.summary(&theme, ThemeSource::Mine, in_use))
    }

    fn new_theme_file(&self, name: &str, mine: &[(PathBuf, Theme)]) -> PathBuf {
        let dir = self.paths().themes();
        let stem = safe_file_stem(name);
        let taken = |candidate: &Path| {
            mine.iter().any(|(p, _)| same_file_name(p, candidate))
                || self.sys().fs.exists(candidate)
        };
        let mut n: u32 = 1;
        loop {
            let file = if n == 1 {
                format!("{stem}.{THEME_EXT}")
            } else {
                format!("{stem} ({n}).{THEME_EXT}")
            };
            let candidate = dir.join(file);
            if !taken(&candidate) {
                return candidate;
            }
            n = n.saturating_add(1);
        }
    }

    /// A copy of a theme with a new id and the given name, saved into my themes (spec §3.1
    /// "Customize": the UI gives "My <name>" or "<ชื่อ> ของฉัน").
    pub fn theme_duplicate(&self, id: Uuid, name: String) -> Result<ThemeSummary, AppError> {
        let (mut theme, _, _) = self.find_theme(id)?;
        theme.id = Uuid::new_v4();
        theme.name = name;
        self.theme_save(theme, false)
    }

    /// Deletes a theme of mine and its draft, or (`draft_only`) just the draft. A preset is
    /// refused. Deleting the applied theme is allowed: the machine keeps its generated files
    /// until the next apply, and the UI says so first.
    pub fn theme_delete(&self, id: Uuid, draft_only: bool) -> Result<(), AppError> {
        let fs = self.sys().fs;
        let draft = self.draft_path(id);
        if fs.exists(&draft) {
            fs.remove_file(&draft).map_err(data_write)?;
        }
        if draft_only {
            return Ok(());
        }
        let (theme, source, path) = self.find_theme(id)?;
        match (source, path) {
            (ThemeSource::Mine, Some(path)) => fs.remove_file(&path).map_err(data_write),
            // Its only file was the draft, which is gone.
            (ThemeSource::Draft, _) => Ok(()),
            _ => Err(AppError::coded(
                "E_THEME_INVALID",
                format!("\"{}\" is a built-in preset and is not deleted", theme.name),
            )),
        }
    }

    /// Import, step one (spec §3.1): reads and checks the file, and keeps the theme under a
    /// token until `theme_import_confirm`, so the file is read once. Says whether its id or
    /// its name is already in the library.
    pub fn theme_import_preview_from(&self, path: &Path) -> Result<ImportPreview, AppError> {
        let fs = self.sys().fs;
        let bytes = fs
            .read(path)
            .map_err(|e| {
                AppError::coded(
                    "E_THEME_INVALID",
                    format!("{} could not be read: {e}", path.display()),
                )
            })?
            .ok_or_else(|| {
                AppError::coded(
                    "E_THEME_INVALID",
                    format!("{} does not exist", path.display()),
                )
            })?;
        let theme = parse_theme(&bytes)?;
        let known = self.themes_list()?;
        let id_taken = known.themes.iter().any(|t| t.id == theme.id);
        let name_taken = known.themes.iter().any(|t| t.name == theme.name);
        let token = Uuid::new_v4().to_string();
        let mut pending = self
            .pending_import
            .lock()
            .map_err(|_| AppError::internal("the import lock is poisoned"))?;
        *pending = Some(PendingImport {
            token: token.clone(),
            theme: theme.clone(),
        });
        Ok(ImportPreview {
            token,
            theme,
            id_taken,
            name_taken,
        })
    }

    /// Import, step two: saves the theme the token stands for into my themes, with a new id
    /// when its id is taken (spec §3.3). The token is good once.
    pub fn theme_import_confirm(&self, token: &str) -> Result<ThemeSummary, AppError> {
        let mut theme = {
            let mut pending = self
                .pending_import
                .lock()
                .map_err(|_| AppError::internal("the import lock is poisoned"))?;
            match pending.take() {
                Some(p) if p.token == token => p.theme,
                other => {
                    *pending = other;
                    return Err(AppError::coded(
                        "E_THEME_INVALID",
                        "the import preview is no longer there; pick the file again",
                    ));
                }
            }
        };
        if self.themes_list()?.themes.iter().any(|t| t.id == theme.id) {
            theme.id = Uuid::new_v4();
        }
        self.theme_save(theme, false)
    }

    /// The file name the Save dialog offers for an export: the theme's name as a safe stem.
    pub fn export_file_name(&self, id: Uuid) -> Result<String, AppError> {
        let (theme, _, _) = self.find_theme(id)?;
        Ok(format!("{}.{THEME_EXT}", safe_file_stem(&theme.name)))
    }

    /// Writes the theme to the file the user chose, as `Theme::to_file_bytes` writes it.
    pub fn theme_export_to(&self, id: Uuid, path: &Path) -> Result<(), AppError> {
        let (theme, _, _) = self.find_theme(id)?;
        let bytes = theme.to_file_bytes()?;
        self.sys().fs.write_atomic(path, &bytes).map_err(|e| {
            AppError::coded(
                "E_DATA_WRITE",
                format!("{} could not be written: {e}", path.display()),
            )
        })
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p cmdstudio-app --lib library::` → `16 passed` (9 + 7 export). `cargo test -p cmdstudio-app --lib` → `45 passed`. Clippy: dead code only for `Backend.resources`, `ctx`, `settings_or_default` now (`pending_import` and `terracotta_id` are used). fmt clean.

- [ ] **Step 4: Commit**

Stage `app/src-tauri/src/library.rs app/src-tauri/src/lib.rs`. Message: `app: the theme library — presets, my themes, drafts, import with preview, export`.

---

### Task 11: `preview_render`, `palette_generate`, `contrast_check`, `contrast_fix`

Four free functions over the core (spec §5.6: the emulator and the palette live in Rust). `preview_render` cleans the sample texts the way the core's banner cleans them, then returns the banner's and the prompt's rows; `palette_generate` parses the accent (`#rrggbb`, else `E_THEME_INVALID`) and calls `palette::generate`; `contrast_check` names each slot under its WCAG floor; `contrast_fix` is `palette::fix`.

**Files:**
- Create: `app/src-tauri/src/backend/preview.rs`
- Modify: `app/src-tauri/src/backend/mod.rs` (+ `pub mod preview;`)
- Test: `the_preview_is_the_cores_rows_and_the_sample_is_cleaned_first`, `simple_mode_generates_and_the_check_and_fix_are_the_cores` (+ 2 export tests)

**Interfaces:**
- Consumes: `cmdstudio_core::preview::{render_banner, render_prompt, PreviewContext, Line}`, `palette::{generate, check, fix, Tone, Colors, Rgb, Slot}`.
- Produces:
  ```rust
  pub struct PreviewResult { pub banner: Vec<Line>, pub prompt: Vec<Line> }
  pub struct ContrastIssue { pub slot: String, pub ratio: f64, pub required: f64 }
  pub fn preview_render(theme: &Theme, ctx: &PreviewContext) -> PreviewResult
  pub fn palette_generate(tone: Tone, accent: &str, tint: f64) -> Result<Colors, AppError>
  pub fn contrast_check(colors: &Colors) -> Vec<ContrastIssue>
  pub fn contrast_fix(colors: Colors) -> Colors
  ```

- [ ] **Step 1: Write the failing tests** — `backend/preview.rs` with only its test module, `pub mod preview;` in `backend/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cmdstudio_core::presets;

    fn terracotta() -> Theme {
        presets::builtin()
            .unwrap()
            .into_iter()
            .find(|(k, _)| *k == "terracotta")
            .unwrap()
            .1
    }

    #[test]
    fn the_preview_is_the_cores_rows_and_the_sample_is_cleaned_first() {
        let theme = terracotta();
        let sample = PreviewContext::sample();
        let drawn = preview_render(&theme, &sample);
        assert_eq!(drawn.banner, render_banner(&theme, &sample));
        assert_eq!(drawn.prompt, render_prompt(&theme, &sample));
        assert!(drawn.banner.len() >= 4 && drawn.prompt.len() >= 3);
        // An escape sequence typed into the sample command never reaches a span.
        let mut hostile = sample.clone();
        hostile.command = "dir \u{1b}[31mred\u{1b}[0m\nnext".to_string();
        hostile.user = "you\u{202e}".to_string();
        let drawn = preview_render(&theme, &hostile);
        let text: String = drawn
            .prompt
            .iter()
            .chain(&drawn.banner)
            .map(Line::text)
            .collect();
        assert!(!text.contains('\u{1b}') && !text.contains('\n') && !text.contains('\u{202e}'));
        assert!(text.contains("dir red"), "{text}");
    }

    #[test]
    fn simple_mode_generates_and_the_check_and_fix_are_the_cores() {
        let colors = palette_generate(Tone::Dark, "#d97757", 0.10).unwrap();
        assert_eq!(
            colors,
            palette::generate(Tone::Dark, "#d97757".parse().unwrap(), 0.10)
        );
        assert!(contrast_check(&colors).is_empty());
        let e = palette_generate(Tone::Light, "d97757", 0.1).unwrap_err();
        assert_eq!(e.code, "E_THEME_INVALID");
        assert!(e.details.contains("\"d97757\""), "{}", e.details);
        assert_eq!(
            palette_generate(Tone::Light, "#d97757", f64::NAN)
                .unwrap_err()
                .code,
            "E_THEME_INVALID"
        );
        // A grey text on a grey background is unreadable; the fix makes it readable.
        let mut bad = colors.clone();
        bad.foreground = bad.background;
        bad.ansi[3] = bad.background;
        let issues = contrast_check(&bad);
        let slots: Vec<&str> = issues.iter().map(|i| i.slot.as_str()).collect();
        assert_eq!(slots, ["foreground", "ansi3"]);
        assert_eq!(issues[0].required, 4.5);
        assert_eq!(issues[1].required, 3.0);
        let fixed = contrast_fix(bad);
        assert!(contrast_check(&fixed).is_empty());
    }
}
```

Run: `cargo test -p cmdstudio-app --lib preview::`
Expected: FAILS to compile.

- [ ] **Step 2: Write the code above the test module**

```rust
//! The commands that compute without touching the machine (spec §5.6: palette, contrast and
//! prompt logic live in Rust; the UI paints spans): `preview_render`, `palette_generate`,
//! `contrast_check`, `contrast_fix`.
use crate::error::AppError;
use cmdstudio_core::color::Rgb;
use cmdstudio_core::model::Theme;
use cmdstudio_core::palette::{self, Colors, Slot, Tone};
use cmdstudio_core::preview::{render_banner, render_prompt, Line, PreviewContext};
use cmdstudio_core::text::sanitize_text;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The rows of the mock terminal: the welcome box, then the prompt rows (spec §5.6).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PreviewResult {
    pub banner: Vec<Line>,
    pub prompt: Vec<Line>,
}

/// One color below its minimum contrast against the background (spec §3.4). `slot` is
/// `foreground`, `muted`, `accent` or `ansi<n>` (n = 0–15), the names the editor's panel uses.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContrastIssue {
    pub slot: String,
    pub ratio: f64,
    pub required: f64,
}

/// Draws the welcome box and the prompt for a theme and the sample the UI chose. The sample's
/// texts are cleaned like a theme's (escape sequences, control characters, line breaks and
/// bidi controls go), so a user-typed sample cannot reach the spans raw.
pub fn preview_render(theme: &Theme, ctx: &PreviewContext) -> PreviewResult {
    let ctx = PreviewContext {
        user: sanitize_text(&ctx.user),
        host: sanitize_text(&ctx.host),
        home: sanitize_text(&ctx.home),
        cwd: sanitize_text(&ctx.cwd),
        command: sanitize_text(&ctx.command),
        git: ctx.git.as_ref().map(|g| cmdstudio_core::preview::GitState {
            branch: sanitize_text(&g.branch),
            dirty: g.dirty,
        }),
        ..ctx.clone()
    };
    PreviewResult {
        banner: render_banner(theme, &ctx),
        prompt: render_prompt(theme, &ctx),
    }
}

/// Simple mode (spec §3.4): all 23 colors from a tone, an accent and a tint. The accent is
/// `#rrggbb`; anything else is `E_THEME_INVALID` with the text as typed.
pub fn palette_generate(tone: Tone, accent: &str, tint: f64) -> Result<Colors, AppError> {
    let accent: Rgb = accent
        .parse()
        .map_err(|e: cmdstudio_core::color::ParseColorError| {
            AppError::coded("E_THEME_INVALID", format!("accent: {e}"))
        })?;
    if !tint.is_finite() {
        return Err(AppError::coded("E_THEME_INVALID", "tint: must be a number"));
    }
    Ok(palette::generate(tone, accent, tint))
}

fn slot_name(slot: Slot) -> String {
    match slot {
        Slot::Foreground => "foreground".to_string(),
        Slot::Muted => "muted".to_string(),
        Slot::Accent => "accent".to_string(),
        Slot::Ansi(i) => format!("ansi{i}"),
    }
}

/// Every checked color below its minimum (spec §3.4). Empty means readable.
pub fn contrast_check(colors: &Colors) -> Vec<ContrastIssue> {
    palette::contrast_issues(colors)
        .into_iter()
        .map(|i| ContrastIssue {
            slot: slot_name(i.slot),
            ratio: i.ratio,
            required: i.required,
        })
        .collect()
}

/// "Fix automatically": every checked color raised to its minimum by lightness alone.
pub fn contrast_fix(mut colors: Colors) -> Colors {
    palette::fix_all(&mut colors);
    colors
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p cmdstudio-app --lib preview::` → `4 passed`. `cargo test -p cmdstudio-app --lib` → `49 passed`. Clippy: the same three dead-code items as Task 10. fmt clean.

- [ ] **Step 4: Commit**

Stage `app/src-tauri/src/backend/preview.rs app/src-tauri/src/backend/mod.rs`. Message: `app: preview_render, palette_generate, contrast_check and contrast_fix over the core`.

---

### Task 12: Plan, apply with progress, undo, restore points, revert all

Index Decisions 3 and 6. `plan_apply` prepares with the core (`actions::prepare_apply`), answers `otherClink` when another Clink is in AutoRun and `replace_other_clink` is false, otherwise `ready` with the plan — its digest, bilingual summary lines (from the core's plan texts), warnings, steps, the notes and unrestored list of the start report — and lets the lock go. `apply(request, digest, on_progress)` prepares again under the lock, compares the digest (`planChanged` if different), runs `run_apply_with` and tells each step. `undo_last`, `snapshots_list`, `snapshot_restore` (undo back through the points until the chosen one), `revert_all` (`cleanup::revert_all`) wrap the core's actions and report their outcome as DTOs the UI shows.

**Files:**
- Create: `app/src-tauri/src/backend/apply.rs`
- Modify: `app/src-tauri/src/backend/mod.rs` (+ `pub mod apply;`)
- Test: `the_plan_names_each_step_and_lets_the_lock_go`, `apply_commits_and_tells_each_step`, `a_plan_that_changed_since_it_was_shown_is_not_applied`, `another_clink_is_asked_about_then_turned_off_on_agreement`, `a_failed_check_rolls_back_and_says_why`, `undo_undoes_then_has_nothing_to_undo`, `restore_points_list_newest_first_and_restore_goes_back_through_them`, `revert_all_takes_every_change_out`, `a_missing_theme_is_refused_before_anything_is_planned`, `the_advanced_page_reads_the_generated_files_after_an_apply`, `the_applied_theme_is_marked_in_use` (+ 20 export tests)

**Interfaces:**
- Consumes: `cmdstudio_core::actions::{prepare_apply, run_apply_with, undo, pending_incomplete, Prepared, Progress, Request, StartReport}`, `cleanup::revert_all`, `snapshot::{SnapshotStore, SnapshotKind, Manifest, ThemeRef, Unrestored}`, `txn::{Outcome, Failure, Stage}`, `plan::{Op, Warning, Paths}`, `model::Target`; `library::{find_theme, applied_info}` (Task 10); `backend::{start_notes, start_unrestored, Note}`; `sha2::Sha256` for the digest.
- Produces:
  ```rust
  pub struct ApplyRequest { pub theme_id: Uuid, pub target: Target, pub set_wt_default_profile: bool, pub replace_other_clink: bool }
  pub enum StepKind { InstallOhMyPosh, InstallClink, WriteGenerated, WindowsTerminal, AutoRun, InstallFont, SaveState }
  pub struct PlanStep { pub kind: StepKind, pub detail: String }
  pub enum PlanWarning { NoWindowsTerminal, MachineWideClink, FontFallback { wanted: String } }   // serde tag "kind"
  pub struct ApplyPlan { pub digest: String, pub theme_name: String, pub summary_en: Vec<String>, pub summary_th: Vec<String>, pub warnings: Vec<PlanWarning>, pub steps: Vec<PlanStep>, pub notes: Vec<Note>, pub unrestored: Vec<Unrestored> }
  pub enum PlanOutcome { Ready { plan: ApplyPlan }, OtherClink { commands: Vec<String> } }   // tag "kind": "ready" | "otherClink"
  pub enum ApplyStatus { Committed, RolledBack, RollbackIncomplete, PlanChanged }
  pub struct ApplyOutcome { pub status: ApplyStatus, pub theme_name: String, pub snapshot_id: Option<String>, pub error: Option<AppError>, pub unrestored: Vec<Unrestored>, pub notes: Vec<Note>, pub plan: Option<ApplyPlan> /* the new plan when PlanChanged */ }
  pub enum Phase { BackingUp, Applying, Verifying, RollingBack }
  pub struct ApplyProgress { pub phase: Phase, pub step: u32, pub total: u32, pub kind: Option<StepKind> }
  pub enum UndoStatus { Committed, RolledBack, RollbackIncomplete, NothingToUndo }
  pub struct FontLeft { pub file: String, pub why: String }
  pub struct UndoOutcome { pub status: UndoStatus, pub undid: Option<String>, pub error: Option<AppError>, pub unrestored: Vec<Unrestored>, pub notes: Vec<Note>, pub fonts_left: Vec<FontLeft> }
  pub struct RestorePoint { pub id: String, pub kind: SnapshotKind, pub created_ms: u64, pub theme: Option<ThemeRef>, pub target: Option<Target>, pub summary: Vec<String>, pub undoes: Option<String>, pub undone_by: Option<String>, pub restorable: bool }
  pub struct RestorePoints { pub points: Vec<RestorePoint>, pub problems: Vec<Note> }
  pub enum RestoreStatus { Done, NotRestorable, Stopped }
  pub struct RestoreOutcome { pub status: RestoreStatus, pub undone: Vec<String>, pub last: Option<UndoOutcome> }
  pub enum TxnStatus { Committed, RolledBack, RollbackIncomplete }
  pub struct StepReport { pub what: String, pub ok: bool, pub detail: String, pub code: Option<String>, pub fix_en: Option<String>, pub fix_th: Option<String> }
  pub struct RevertOutcome { pub ok: bool, pub steps: Vec<StepReport>, pub notes: Vec<Note>, pub status: TxnStatus, pub error: Option<AppError>, pub unrestored: Vec<Unrestored> }
  impl<M: Machine> Backend<M> {
      pub fn plan_apply(&self, req: ApplyRequest) -> Result<PlanOutcome, AppError>
      pub fn apply(&self, req: ApplyRequest, digest: &str, on_progress: &mut dyn FnMut(ApplyProgress)) -> Result<ApplyOutcome, AppError>
      pub fn undo_last(&self) -> Result<UndoOutcome, AppError>
      pub fn snapshots_list(&self) -> Result<RestorePoints, AppError>
      pub fn snapshot_restore(&self, id: &str) -> Result<RestoreOutcome, AppError>
      pub fn revert_all(&self) -> Result<RevertOutcome, AppError>
  }
  ```
  Every enum serializes `camelCase` (`"committed"`, `"planChanged"`, `"installOhMyPosh"`, …).

- [ ] **Step 1: Write the failing tests** — `backend/apply.rs` with only its test module, `pub mod apply;` in `backend/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{backend, terracotta_id};
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::adapters::{AutoRun, RegValue, REG_SZ};
    use cmdstudio_core::state::State;

    fn request(id: Uuid) -> ApplyRequest {
        ApplyRequest {
            theme_id: id,
            target: Target::All,
            set_wt_default_profile: true,
            replace_other_clink: false,
        }
    }

    fn ready(outcome: PlanOutcome) -> ApplyPlan {
        match outcome {
            PlanOutcome::Ready { plan } => plan,
            other => panic!("{other:?}"),
        }
    }

    fn apply_terracotta(b: &Backend<FakeMachine>) -> (ApplyOutcome, Vec<ApplyProgress>) {
        let req = request(terracotta_id());
        let plan = ready(b.plan_apply(req.clone()).unwrap());
        let mut seen = Vec::new();
        let done = b.apply(req, &plan.digest, &mut |p| seen.push(p)).unwrap();
        (done, seen)
    }

    #[test]
    fn the_plan_names_each_step_and_lets_the_lock_go() {
        let b = backend();
        let plan = ready(b.plan_apply(request(terracotta_id())).unwrap());
        assert_eq!(plan.theme_name, "Terracotta");
        let kinds: Vec<StepKind> = plan.steps.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            [
                StepKind::InstallOhMyPosh,
                StepKind::InstallClink,
                StepKind::WriteGenerated,
                StepKind::WriteGenerated,
                StepKind::WriteGenerated,
                StepKind::WriteGenerated,
                StepKind::WindowsTerminal,
                StepKind::AutoRun,
                StepKind::SaveState,
            ]
        );
        assert_eq!(plan.summary_en[0], "Update the prompt and the welcome box");
        assert_eq!(plan.summary_th[0], "ปรับ prompt และกล่องต้อนรับตามธีม");
        assert!(plan.warnings.is_empty() && plan.notes.is_empty() && plan.unrestored.is_empty());
        assert_eq!(plan.digest.len(), 64);
        // The same plan twice: the same digest, and the lock is free in between (a second
        // plan under a held lock would be E_BUSY).
        let again = ready(b.plan_apply(request(terracotta_id())).unwrap());
        assert_eq!(again.digest, plan.digest);
        assert!(b
            .sys()
            .fs
            .try_lock(&b.paths().lock_file())
            .unwrap()
            .is_some());
    }

    #[test]
    fn apply_commits_and_tells_each_step() {
        let b = backend();
        let (done, seen) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::Committed);
        assert_eq!(done.theme_name, "Terracotta");
        assert!(done.snapshot_id.is_some() && done.error.is_none() && done.plan.is_none());
        let state = State::load(b.sys().fs, &b.paths().state_file()).unwrap();
        assert_eq!(
            state.applied.as_ref().map(|a| &a.snapshot_id),
            done.snapshot_id.as_ref()
        );
        // Back up 9, apply 9 with the step kinds in order, then verify.
        let backing: Vec<u32> = seen
            .iter()
            .filter(|p| p.phase == Phase::BackingUp)
            .map(|p| p.step)
            .collect();
        assert_eq!(backing, (1..=9).collect::<Vec<_>>());
        let applying: Vec<(u32, Option<StepKind>)> = seen
            .iter()
            .filter(|p| p.phase == Phase::Applying)
            .map(|p| (p.step, p.kind))
            .collect();
        assert_eq!(applying[0], (1, Some(StepKind::InstallOhMyPosh)));
        assert_eq!(applying[8], (9, Some(StepKind::SaveState)));
        assert!(seen
            .iter()
            .all(|p| p.total == 9 || p.phase == Phase::Verifying));
        assert_eq!(seen.last().unwrap().phase, Phase::Verifying);
        assert!(!seen.iter().any(|p| p.phase == Phase::RollingBack));
    }

    #[test]
    fn a_plan_that_changed_since_it_was_shown_is_not_applied() {
        let mut b = backend();
        let req = request(terracotta_id());
        let plan = ready(b.plan_apply(req.clone()).unwrap());
        // Windows Terminal is uninstalled in between: the plan has fewer steps now.
        b.machine.wt.locations.clone_from(&Vec::new());
        let done = b.apply(req.clone(), &plan.digest, &mut |_| {}).unwrap();
        assert_eq!(done.status, ApplyStatus::PlanChanged);
        let new_plan = done.plan.unwrap();
        assert_ne!(new_plan.digest, plan.digest);
        assert_eq!(new_plan.warnings, [PlanWarning::NoWindowsTerminal]);
        assert!(
            !b.sys().fs.exists(&b.paths().state_file()),
            "nothing was applied"
        );
        // A wrong digest never applies.
        let done = b.apply(req, "not-a-digest", &mut |_| {}).unwrap();
        assert_eq!(done.status, ApplyStatus::PlanChanged);
    }

    #[test]
    fn another_clink_is_asked_about_then_turned_off_on_agreement() {
        let b = backend();
        let other = r#""C:\clink\clink_x64.exe" inject --autorun"#;
        b.machine
            .autorun
            .write_user(&RegValue::from_text(REG_SZ, other))
            .unwrap();
        let req = request(terracotta_id());
        match b.plan_apply(req.clone()).unwrap() {
            PlanOutcome::OtherClink { commands } => assert_eq!(commands, [other]),
            other => panic!("{other:?}"),
        }
        let agreed = ApplyRequest {
            replace_other_clink: true,
            ..req
        };
        let plan = ready(b.plan_apply(agreed.clone()).unwrap());
        assert!(
            plan.summary_en
                .iter()
                .any(|s| s.contains("Turn off the cmd add-on")),
            "{:?}",
            plan.summary_en
        );
        let done = b.apply(agreed, &plan.digest, &mut |_| {}).unwrap();
        assert_eq!(done.status, ApplyStatus::Committed);
        let value = b
            .machine
            .autorun
            .read_user()
            .unwrap()
            .unwrap()
            .text()
            .unwrap();
        assert!(
            !value.contains(other) && value.contains("clink-profile"),
            "{value}"
        );
        // Undo brings the other Clink back.
        let undone = b.undo_last().unwrap();
        assert_eq!(undone.status, UndoStatus::Committed);
        assert_eq!(
            b.machine
                .autorun
                .read_user()
                .unwrap()
                .unwrap()
                .text()
                .unwrap(),
            other
        );
    }

    #[test]
    fn a_failed_check_rolls_back_and_says_why() {
        let b = backend();
        b.machine.process.answer("oh-my-posh.exe", "print", 1, "");
        let (done, seen) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::RolledBack);
        assert_eq!(done.error.as_ref().unwrap().code, "E_VERIFY_OMP");
        assert_eq!(
            done.error.unwrap().message_th,
            "ขั้นตรวจผล: OMP เรนเดอร์ config ไม่ได้"
        );
        assert!(seen.iter().any(|p| p.phase == Phase::RollingBack));
        assert!(!b.sys().fs.exists(&b.paths().state_file()));
        assert_eq!(b.machine.autorun.read_user().unwrap(), None);
    }

    #[test]
    fn undo_undoes_then_has_nothing_to_undo() {
        let b = backend();
        let wt_before = b
            .sys()
            .fs
            .read(std::path::Path::new(crate::backend::testing::WT))
            .unwrap();
        let (done, _) = apply_terracotta(&b);
        let undone = b.undo_last().unwrap();
        assert_eq!(undone.status, UndoStatus::Committed);
        assert_eq!(undone.undid, done.snapshot_id);
        assert!(undone.error.is_none() && undone.fonts_left.is_empty());
        assert_eq!(
            b.sys()
                .fs
                .read(std::path::Path::new(crate::backend::testing::WT))
                .unwrap(),
            wt_before
        );
        let nothing = b.undo_last().unwrap();
        assert_eq!(
            (nothing.status, nothing.undid),
            (UndoStatus::NothingToUndo, None)
        );
    }

    #[test]
    fn restore_points_list_newest_first_and_restore_goes_back_through_them() {
        let b = backend();
        let (first, _) = apply_terracotta(&b);
        let paper = cmdstudio_core::presets::builtin()
            .unwrap()
            .into_iter()
            .find(|(k, _)| *k == "paper")
            .unwrap()
            .1
            .id;
        let req = request(paper);
        let plan = ready(b.plan_apply(req.clone()).unwrap());
        let second = b.apply(req, &plan.digest, &mut |_| {}).unwrap();
        let list = b.snapshots_list().unwrap();
        assert!(list.problems.is_empty());
        let ids: Vec<&str> = list.points.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                second.snapshot_id.as_deref().unwrap(),
                first.snapshot_id.as_deref().unwrap()
            ]
        );
        assert!(list
            .points
            .iter()
            .all(|p| p.kind == SnapshotKind::Apply && p.restorable));
        assert_eq!(list.points[0].theme.as_ref().unwrap().name, "Paper");
        // Restoring the first point undoes Paper, then Terracotta.
        let restored = b
            .snapshot_restore(first.snapshot_id.as_deref().unwrap())
            .unwrap();
        assert_eq!(restored.status, RestoreStatus::Done);
        assert_eq!(
            restored.undone,
            [
                second.snapshot_id.clone().unwrap(),
                first.snapshot_id.clone().unwrap()
            ]
        );
        assert_eq!(restored.last.unwrap().status, UndoStatus::Committed);
        let list = b.snapshots_list().unwrap();
        assert_eq!(list.points.len(), 4);
        assert!(list.points.iter().all(|p| !p.restorable));
        assert_eq!(list.points[0].kind, SnapshotKind::Undo);
        // An undone point, or an undo, is not restorable; a missing id is an error.
        assert_eq!(
            b.snapshot_restore(first.snapshot_id.as_deref().unwrap())
                .unwrap()
                .status,
            RestoreStatus::NotRestorable
        );
        assert_eq!(
            b.snapshot_restore(&list.points[0].id).unwrap().status,
            RestoreStatus::NotRestorable
        );
        assert_eq!(
            b.snapshot_restore("0000000000000-nowhere")
                .unwrap_err()
                .code,
            "E_SNAPSHOT_READ"
        );
    }

    #[test]
    fn revert_all_takes_every_change_out() {
        let b = backend();
        let wt_before = b
            .sys()
            .fs
            .read(std::path::Path::new(crate::backend::testing::WT))
            .unwrap();
        apply_terracotta(&b);
        assert!(b.machine.autorun.read_user().unwrap().is_some());
        let reverted = b.revert_all().unwrap();
        assert!(reverted.ok, "{:?}", reverted.steps);
        assert_eq!(reverted.status, Some(TxnStatus::Committed));
        assert!(reverted.steps.iter().all(|s| s.ok));
        assert_eq!(b.machine.autorun.read_user().unwrap(), None);
        assert_eq!(
            b.sys()
                .fs
                .read(std::path::Path::new(crate::backend::testing::WT))
                .unwrap(),
            wt_before
        );
        // Nothing applied any more.
        let state = State::load(b.sys().fs, &b.paths().state_file()).unwrap();
        assert!(state.applied.is_none());
    }

    #[test]
    fn a_missing_theme_is_refused_before_anything_is_planned() {
        let b = backend();
        let e = b.plan_apply(request(Uuid::new_v4())).unwrap_err();
        assert_eq!(e.code, "E_THEME_INVALID");
        assert!(b.sys().fs.read(&b.paths().lock_file()).unwrap().is_none());
    }

    // `advanced_read` is a method of backend/mod.rs; it needs an apply first, so its test is here.
    #[test]
    fn the_advanced_page_reads_the_generated_files_after_an_apply() {
        let b = backend();
        let before = b.advanced_read().unwrap();
        assert!(before.generated.is_empty());
        assert_eq!(before.versions.oh_my_posh, "31.5.0");
        assert_eq!(before.versions.clink, "1.9.34");
        assert_eq!(before.data_dir, crate::backend::testing::DATA);
        let req = ApplyRequest {
            theme_id: terracotta_id(),
            target: Target::All,
            set_wt_default_profile: true,
            replace_other_clink: false,
        };
        let plan = match b.plan_apply(req.clone()).unwrap() {
            PlanOutcome::Ready { plan } => plan,
            other => panic!("{other:?}"),
        };
        let done = b.apply(req, &plan.digest, &mut |_| {}).unwrap();
        assert_eq!(done.status, ApplyStatus::Committed);
        let after = b.advanced_read().unwrap();
        let names: Vec<&str> = after.generated.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            ["theme.omp.json", "init.lua", "banner.lua", "clink_settings"]
        );
        assert!(after.log_tail.contains("cmd=") || after.log_tail.is_empty());
    }

    // The library's "in use" badge comes from the applied state, so its test lives with apply.
    #[test]
    fn the_applied_theme_is_marked_in_use() {
        let b = backend();
        let req = ApplyRequest {
            theme_id: terracotta_id(),
            target: Target::All,
            set_wt_default_profile: true,
            replace_other_clink: false,
        };
        let plan = match b.plan_apply(req.clone()).unwrap() {
            PlanOutcome::Ready { plan } => plan,
            other => panic!("{other:?}"),
        };
        b.apply(req, &plan.digest, &mut |_| {}).unwrap();
        let lib = b.themes_list().unwrap();
        assert!(lib.themes[0].in_use && !lib.themes[1].in_use);
        let applied = lib.applied.unwrap();
        assert_eq!(
            (
                applied.theme_id,
                applied.theme_name.as_str(),
                applied.target
            ),
            (terracotta_id(), "Terracotta", Target::All)
        );
        // A damaged state.json is a problem the library says, not a crash.
        b.sys()
            .fs
            .write_atomic(&b.paths().state_file(), b"{ damaged")
            .unwrap();
        let lib = b.themes_list().unwrap();
        assert!(lib.applied.is_none());
        assert_eq!(lib.state_problem.unwrap().code, "E_STATE_INVALID");
        assert!(lib.themes.iter().all(|t| !t.in_use));
    }
}
```

Run: `cargo test -p cmdstudio-app --lib apply::`
Expected: FAILS to compile (`ApplyRequest`, `plan_apply`, `apply` not found).

- [ ] **Step 2: Write the code above the test module**

```rust
//! Plan, apply, undo, restore points and revert everything (spec §4.2 page 5, §4.4, §4.3
//! "ช่วยเหลือ"), over the core's transactions. The plan is shown before anything changes (spec
//! §2.1): `plan_apply` plans and lets the lock go; `apply` plans again and, when the plan is no
//! longer the one the user saw, changes nothing and shows the new one.
use super::{note, start_notes, start_unrestored, Backend, Machine, Note};
use crate::error::AppError;
use cmdstudio_core::actions::{
    pending_incomplete, prepare_apply, run_apply_with, undo, ActionError, Ctx, Prepared, Progress,
    UndoResult,
};
use cmdstudio_core::cleanup;
use cmdstudio_core::exec::{CLEANUP_FIX_EN, CLEANUP_FIX_TH};
use cmdstudio_core::model::{Target, Theme};
use cmdstudio_core::plan::{Component, Op, Paths, PlanError, Request, Warning};
use cmdstudio_core::resources::sha256_hex;
use cmdstudio_core::snapshot::{SnapshotKind, SnapshotStore, ThemeRef, Unrestored};
use cmdstudio_core::txn::{Outcome, Stage};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// What the user asked for (the wizard's pages 3 and 4, the editor's "Save and apply", the
/// home page's quick switch).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApplyRequest {
    pub theme_id: Uuid,
    pub target: Target,
    pub set_wt_default_profile: bool,
    /// The user agreed to turn off another Clink found in AutoRun (spec §5.4).
    pub replace_other_clink: bool,
}

impl ApplyRequest {
    fn request(&self) -> Request {
        Request {
            target: self.target,
            set_wt_default_profile: self.set_wt_default_profile,
            replace_foreign_clink: self.replace_other_clink,
        }
    }
}

/// What one step of the plan does, for the UI's words (spec §4.5: no technical terms).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum StepKind {
    InstallOhMyPosh,
    InstallClink,
    WriteGenerated,
    WindowsTerminal,
    AutoRun,
    InstallFont,
    SaveState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlanStep {
    pub kind: StepKind,
    /// The folder, file or font the step touches, as written.
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum PlanWarning {
    NoWindowsTerminal,
    MachineWideClink,
    /// The theme's font is not installed; Windows Terminal gets the bundled Nerd Font.
    FontFallback {
        wanted: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApplyPlan {
    /// Stands for what the user saw; `apply` takes it back and compares.
    pub digest: String,
    pub theme_name: String,
    pub summary_en: Vec<String>,
    pub summary_th: Vec<String>,
    pub warnings: Vec<PlanWarning>,
    pub steps: Vec<PlanStep>,
    /// What the start found (spec §5.5), to show with the plan.
    pub notes: Vec<Note>,
    /// The steps of a rollback that did not finish, which still need a hand.
    pub unrestored: Vec<Unrestored>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum PlanOutcome {
    Ready {
        plan: ApplyPlan,
    },
    /// Another Clink starts from AutoRun: the user chooses between turning it off (these are
    /// the commands that go) and cancelling (spec §5.4).
    OtherClink {
        commands: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ApplyStatus {
    Committed,
    RolledBack,
    RollbackIncomplete,
    /// The machine changed since the plan was shown: nothing was done; `plan` is the new plan.
    PlanChanged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApplyOutcome {
    pub status: ApplyStatus,
    pub theme_name: String,
    /// The restore point of this apply, which "Undo" in the after-apply bar goes back to.
    pub snapshot_id: Option<String>,
    /// Why it rolled back, when it did.
    pub error: Option<AppError>,
    pub unrestored: Vec<Unrestored>,
    pub notes: Vec<Note>,
    pub plan: Option<ApplyPlan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Phase {
    BackingUp,
    Applying,
    Verifying,
    RollingBack,
}

/// The `apply-progress` event (spec §4.2 page 5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApplyProgress {
    pub phase: Phase,
    pub step: u32,
    pub total: u32,
    /// The step being applied; absent for the other phases.
    pub kind: Option<StepKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UndoStatus {
    NothingToUndo,
    Committed,
    RolledBack,
    RollbackIncomplete,
}

/// A font an undo left installed because the user installed one of that name since.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FontLeft {
    pub family: String,
    pub message_en: String,
    pub message_th: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UndoOutcome {
    pub status: UndoStatus,
    /// The restore point of the apply that was undone.
    pub undid: Option<String>,
    pub error: Option<AppError>,
    pub unrestored: Vec<Unrestored>,
    pub notes: Vec<Note>,
    pub fonts_left: Vec<FontLeft>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestorePoint {
    pub id: String,
    pub kind: SnapshotKind,
    #[ts(type = "number")]
    pub created_ms: u64,
    pub theme: Option<ThemeRef>,
    pub target: Option<Target>,
    pub summary: Vec<String>,
    pub undoes: Option<String>,
    pub undone_by: Option<String>,
    /// An apply that is not undone and not set aside: "Restore" may go back to it.
    pub restorable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestorePoints {
    /// Newest first.
    pub points: Vec<RestorePoint>,
    /// Restore points whose manifest could not be read.
    pub problems: Vec<Note>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RestoreStatus {
    /// Every apply from the newest down to that one is undone.
    Done,
    /// Not an apply, already undone, set aside, or it did not commit: nothing was done.
    NotRestorable,
    /// An undo on the way did not commit; `undone` says how far it got, `last` why it stopped.
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreOutcome {
    pub status: RestoreStatus,
    pub undone: Vec<String>,
    pub last: Option<UndoOutcome>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TxnStatus {
    Committed,
    RolledBack,
    RollbackIncomplete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StepReport {
    pub what: String,
    pub ok: bool,
    pub detail: String,
    pub code: Option<String>,
    pub fix_en: Option<String>,
    pub fix_th: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RevertOutcome {
    /// Every step went well and the transaction, if it ran, committed.
    pub ok: bool,
    pub steps: Vec<StepReport>,
    pub notes: Vec<Note>,
    pub status: Option<TxnStatus>,
    pub error: Option<AppError>,
    pub unrestored: Vec<Unrestored>,
}

/// How a transaction ended, for the UI.
struct TxnResult {
    status: TxnStatus,
    error: Option<AppError>,
    unrestored: Vec<Unrestored>,
    notes: Vec<Note>,
}

fn step_of(op: &Op, paths: &Paths) -> PlanStep {
    let (kind, detail) = match op {
        Op::InstallRuntime {
            component: Component::Omp,
            dir,
        } => (StepKind::InstallOhMyPosh, dir.display().to_string()),
        Op::InstallRuntime {
            component: Component::Clink,
            dir,
        } => (StepKind::InstallClink, dir.display().to_string()),
        Op::WriteFile { path, .. } if *path == paths.state_file() => {
            (StepKind::SaveState, path.display().to_string())
        }
        Op::WriteFile { path, .. } => (
            StepKind::WriteGenerated,
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
        Op::WtApply { settings, .. } => (StepKind::WindowsTerminal, settings.display().to_string()),
        Op::SetAutoRun { .. } => (StepKind::AutoRun, String::new()),
        Op::InstallFont { family } => (StepKind::InstallFont, family.clone()),
    };
    PlanStep { kind, detail }
}

fn warning_of(w: &Warning) -> PlanWarning {
    match w {
        Warning::NoWindowsTerminal => PlanWarning::NoWindowsTerminal,
        Warning::MachineWideClink => PlanWarning::MachineWideClink,
        Warning::FontFallback { wanted } => PlanWarning::FontFallback {
            wanted: wanted.clone(),
        },
    }
}

/// What the user is shown, reduced to a hash: the same plan gives the same digest.
fn digest(summary_en: &[String], warnings: &[PlanWarning], steps: &[PlanStep]) -> String {
    #[derive(Serialize)]
    struct Fingerprint<'a> {
        summary_en: &'a [String],
        warnings: &'a [PlanWarning],
        steps: &'a [PlanStep],
    }
    let json = serde_json::to_vec(&Fingerprint {
        summary_en,
        warnings,
        steps,
    })
    .unwrap_or_default();
    sha256_hex(&json)
}

fn plan_of(theme: &Theme, prepared: &Prepared, paths: &Paths) -> ApplyPlan {
    let steps: Vec<PlanStep> = prepared.ops.iter().map(|op| step_of(op, paths)).collect();
    let warnings: Vec<PlanWarning> = prepared.plan.warnings.iter().map(warning_of).collect();
    let summary_en: Vec<String> = prepared
        .plan
        .summary_en
        .iter()
        .map(|s| s.to_string())
        .collect();
    let summary_th: Vec<String> = prepared
        .plan
        .summary_th
        .iter()
        .map(|s| s.to_string())
        .collect();
    ApplyPlan {
        digest: digest(&summary_en, &warnings, &steps),
        theme_name: theme.name.clone(),
        summary_en,
        summary_th,
        warnings,
        steps,
        notes: start_notes(&prepared.start),
        unrestored: start_unrestored(&prepared.start),
    }
}

fn progress_of(p: Progress, paths: &Paths) -> ApplyProgress {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    match p {
        Progress::BackingUp { step, total } => ApplyProgress {
            phase: Phase::BackingUp,
            step: count(step),
            total: count(total),
            kind: None,
        },
        Progress::Applying { step, total, op } => ApplyProgress {
            phase: Phase::Applying,
            step: count(step),
            total: count(total),
            kind: Some(step_of(&op, paths).kind),
        },
        Progress::Verifying => ApplyProgress {
            phase: Phase::Verifying,
            step: 0,
            total: 0,
            kind: None,
        },
        Progress::RollingBack { step, total } => ApplyProgress {
            phase: Phase::RollingBack,
            step: count(step),
            total: count(total),
            kind: None,
        },
    }
}

impl<M: Machine> Backend<M> {
    /// The plan for applying `req`, shown before anything changes (spec §2.1, §4.2 page 5).
    /// Takes the lock while it plans and lets it go: nothing waits on the user's reading.
    /// Another Clink in AutoRun is an answer, not an error: the UI asks, and plans again with
    /// `replace_other_clink`.
    pub fn plan_apply(&self, req: ApplyRequest) -> Result<PlanOutcome, AppError> {
        let (theme, _, _) = self.find_theme(req.theme_id)?;
        let ctx = self.ctx();
        match prepare_apply(self.sys(), &ctx, &theme, &req.request()) {
            Ok(prepared) => Ok(PlanOutcome::Ready {
                plan: plan_of(&theme, &prepared, &ctx.paths),
            }),
            Err(ActionError::Plan(PlanError::ForeignClink { existing })) => {
                Ok(PlanOutcome::OtherClink { commands: existing })
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Applies `req` as one transaction, telling `on_progress` each step (the `apply-progress`
    /// event). `digest` is the plan the user saw: when the plan now differs, nothing is done
    /// and the new plan comes back as `PlanChanged`.
    pub fn apply(
        &self,
        req: ApplyRequest,
        digest: &str,
        on_progress: &mut dyn FnMut(ApplyProgress),
    ) -> Result<ApplyOutcome, AppError> {
        let (theme, _, _) = self.find_theme(req.theme_id)?;
        let ctx = self.ctx();
        let prepared = prepare_apply(self.sys(), &ctx, &theme, &req.request())?;
        let plan = plan_of(&theme, &prepared, &ctx.paths);
        if plan.digest != digest {
            return Ok(ApplyOutcome {
                status: ApplyStatus::PlanChanged,
                theme_name: theme.name.clone(),
                snapshot_id: None,
                error: None,
                unrestored: Vec::new(),
                notes: plan.notes.clone(),
                plan: Some(plan),
            });
        }
        let snapshot_id = prepared.manifest.id.clone();
        let mut notes = plan.notes.clone();
        let mut unrestored = plan.unrestored.clone();
        let paths = ctx.paths.clone();
        let outcome = run_apply_with(self.sys(), &ctx, prepared, &mut |p| {
            on_progress(progress_of(p, &paths))
        });
        let result = self.transaction_result(&outcome, &ctx);
        notes.extend(result.notes);
        unrestored.extend(result.unrestored);
        Ok(ApplyOutcome {
            status: match result.status {
                TxnStatus::Committed => ApplyStatus::Committed,
                TxnStatus::RolledBack => ApplyStatus::RolledBack,
                TxnStatus::RollbackIncomplete => ApplyStatus::RollbackIncomplete,
            },
            theme_name: theme.name.clone(),
            snapshot_id: Some(snapshot_id),
            error: result.error,
            unrestored,
            notes,
            plan: None,
        })
    }

    /// How a transaction ended: its error, and for a rollback that did not finish the steps
    /// to fix by hand, from the record the core wrote (or, when it could not be written, from
    /// the failed ops with the clean-up advice).
    fn transaction_result(&self, outcome: &Outcome, ctx: &Ctx) -> TxnResult {
        match outcome {
            Outcome::Committed => TxnResult {
                status: TxnStatus::Committed,
                error: None,
                unrestored: Vec::new(),
                notes: Vec::new(),
            },
            Outcome::RolledBack { code, reason } => TxnResult {
                status: TxnStatus::RolledBack,
                error: Some(AppError::coded(code, reason.clone())),
                unrestored: Vec::new(),
                notes: Vec::new(),
            },
            Outcome::RollbackIncomplete {
                code,
                reason,
                failed,
            } => {
                let mut notes = Vec::new();
                let unrestored = match pending_incomplete(self.sys(), ctx) {
                    Ok(Some(record)) => record.steps,
                    Ok(None) => failed
                        .iter()
                        .map(|(i, f)| Unrestored {
                            op_index: *i,
                            target: format!("step {}", i + 1),
                            code: f.code.to_string(),
                            message: f.message.clone(),
                            fix: CLEANUP_FIX_EN.to_string(),
                            fix_th: CLEANUP_FIX_TH.to_string(),
                            restore_point: None,
                        })
                        .collect(),
                    Err(e) => {
                        notes.push(note(e.code(), e.to_string()));
                        Vec::new()
                    }
                };
                TxnResult {
                    status: TxnStatus::RollbackIncomplete,
                    error: Some(AppError::coded(code, reason.clone())),
                    unrestored,
                    notes,
                }
            }
        }
    }

    /// Undoes the newest apply that is not undone (spec §4.4 "ย้อนกลับ").
    pub fn undo_last(&self) -> Result<UndoOutcome, AppError> {
        let ctx = self.ctx();
        let report = undo(self.sys(), &ctx)?;
        let mut notes = start_notes(&report.start);
        let mut unrestored = start_unrestored(&report.start);
        for p in &report.problems {
            notes.push(note(p.code, p.message.clone()));
        }
        let fonts_left = report
            .fonts_left
            .iter()
            .map(|f| FontLeft {
                family: f.family.clone(),
                message_en: f.en(),
                message_th: f.th(),
            })
            .collect();
        match report.result {
            UndoResult::NothingToUndo => Ok(UndoOutcome {
                status: UndoStatus::NothingToUndo,
                undid: None,
                error: None,
                unrestored,
                notes,
                fonts_left,
            }),
            UndoResult::Done { undid, outcome } => {
                let result = self.transaction_result(&outcome, &ctx);
                notes.extend(result.notes);
                unrestored.extend(result.unrestored);
                Ok(UndoOutcome {
                    status: match result.status {
                        TxnStatus::Committed => UndoStatus::Committed,
                        TxnStatus::RolledBack => UndoStatus::RolledBack,
                        TxnStatus::RollbackIncomplete => UndoStatus::RollbackIncomplete,
                    },
                    undid: Some(undid),
                    error: result.error,
                    unrestored,
                    notes,
                    fonts_left,
                })
            }
        }
    }

    /// Every restore point, newest first (spec §4.3 "จุดย้อนกลับทั้งหมด"). Read only.
    pub fn snapshots_list(&self) -> Result<RestorePoints, AppError> {
        let root = self.paths().snapshots();
        let store = SnapshotStore::new(self.sys().fs, &root);
        let ids = store.ids()?;
        let mut points = Vec::new();
        let mut problems = Vec::new();
        for id in ids.iter().rev() {
            match store.manifest(id) {
                Ok(m) => points.push(RestorePoint {
                    restorable: m.kind == SnapshotKind::Apply
                        && m.undone_by.is_none()
                        && !store.is_set_aside(id),
                    id: m.id,
                    kind: m.kind,
                    created_ms: m.created_ms,
                    theme: m.theme,
                    target: m.target,
                    summary: m.summary,
                    undoes: m.undoes,
                    undone_by: m.undone_by,
                }),
                Err(f) => problems.push(note(f.code, format!("restore point {id}: {}", f.message))),
            }
        }
        Ok(RestorePoints { points, problems })
    }

    /// Goes back to restore point `id` (an apply): undoes every apply from the newest down to
    /// it, each as its own transaction, and stops at the first that does not commit. The
    /// restore points are layered, so an older one cannot be put back alone.
    pub fn snapshot_restore(&self, id: &str) -> Result<RestoreOutcome, AppError> {
        let root = self.paths().snapshots();
        let store = SnapshotStore::new(self.sys().fs, &root);
        let ids = store.ids()?;
        if !ids.iter().any(|known| known == id) {
            return Err(AppError::coded(
                "E_SNAPSHOT_READ",
                format!("there is no restore point {id}"),
            ));
        }
        let manifest = store.manifest(id)?;
        let not_restorable = manifest.kind != SnapshotKind::Apply
            || manifest.undone_by.is_some()
            || store.is_set_aside(id)
            || store.journal(id)?.stage != Stage::Committed;
        if not_restorable {
            return Ok(RestoreOutcome {
                status: RestoreStatus::NotRestorable,
                undone: Vec::new(),
                last: None,
            });
        }
        let mut undone = Vec::new();
        for _ in 0..ids.len() {
            let last = self.undo_last()?;
            if let Some(u) = &last.undid {
                undone.push(u.clone());
            }
            if last.status != UndoStatus::Committed {
                return Ok(RestoreOutcome {
                    status: RestoreStatus::Stopped,
                    undone,
                    last: Some(last),
                });
            }
            if store.manifest(id)?.undone_by.is_some() {
                return Ok(RestoreOutcome {
                    status: RestoreStatus::Done,
                    undone,
                    last: Some(last),
                });
            }
        }
        Ok(RestoreOutcome {
            status: RestoreStatus::Stopped,
            undone,
            last: None,
        })
    }

    /// "Undo everything" (spec §4.3, §6.3 steps 1–3): every change of CMD Studio's taken out,
    /// key by key, without uninstalling.
    pub fn revert_all(&self) -> Result<RevertOutcome, AppError> {
        let ctx = self.ctx();
        let report = cleanup::revert_all(self.sys(), &ctx)?;
        let mut notes = start_notes(&report.start);
        let mut unrestored = start_unrestored(&report.start);
        let steps = report
            .steps
            .iter()
            .map(|s| StepReport {
                what: s.what.clone(),
                ok: s.ok,
                detail: s.detail.clone(),
                code: s.code.map(str::to_string),
                fix_en: s.fix.as_ref().map(|f| f.en.clone()),
                fix_th: s.fix.as_ref().map(|f| f.th.clone()),
            })
            .collect();
        let (status, error) = match &report.outcome {
            None => (None, None),
            Some(outcome) => {
                let result = self.transaction_result(outcome, &ctx);
                notes.extend(result.notes);
                unrestored.extend(result.unrestored);
                (Some(result.status), result.error)
            }
        };
        Ok(RevertOutcome {
            ok: report.ok(),
            steps,
            notes,
            status,
            error,
            unrestored,
        })
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p cmdstudio-app --lib apply::` → `31 passed` (11 + 20 export). `cargo test -p cmdstudio-app --lib` → `80 passed`. Clippy: dead code only for `settings_or_default` (Task 13). fmt clean.

- [ ] **Step 4: See that the digest bites**

In `apply` change the comparison `if plan.digest != digest` so that it never fires (e.g. `if false`); run `cargo test -p cmdstudio-app --lib -- a_plan_that_changed_since_it_was_shown_is_not_applied`.
Expected: FAILS (the stale digest was applied). Restore the line; passes.

- [ ] **Step 5: Commit**

Stage `app/src-tauri/src/backend/apply.rs app/src-tauri/src/backend/mod.rs`. Message: `app: plan_apply with a digest, apply with progress, undo, restore points, revert all`.

---

### Task 13: The doctor's report with fixes, and the diagnostics

Spec §5.8 and §4.3; index Decisions 8 and 9. `doctor_run` runs the core's doctor and gives each check a fix when there is one to offer (`fix_for`: a hand-edited file → `reapply`; another Clink after an apply → `replaceOtherClink`; no Windows Terminal → `installWindowsTerminal`; a terminal never opened → `openWindowsTerminalOnce`; no cmd window loaded the theme yet → `openNewCmdWindow`; a damaged runtime before any apply → `reinstall`; a check that could not read → nothing). `doctor_fix(check_id)` turns a fix into a proposal the UI can act on: the apply kinds carry the request and its plan (through `plan_apply`). `diagnostics_copy` writes the report for a GitHub issue with the user name redacted.

**Files:**
- Create: `app/src-tauri/src/backend/doctor.rs`, `app/src-tauri/src/backend/diagnostics.rs`
- Modify: `app/src-tauri/src/backend/mod.rs` (+ `pub mod doctor;`, `pub mod diagnostics;`)
- Test: doctor — `a_clean_machine_after_an_apply_needs_no_fix`, `a_hand_edited_file_offers_to_apply_again_with_a_plan`, `another_clink_offers_to_replace_it_and_a_missing_terminal_the_store`, `a_check_that_could_not_read_offers_nothing_and_an_unknown_id_is_refused`, `a_damaged_runtime_before_any_apply_says_reinstall` (+ 4 export tests); diagnostics — `the_user_name_is_the_folder_before_appdata`, `paths_with_the_user_name_are_redacted_whatever_their_case`, `the_diagnostics_hold_the_versions_the_doctor_and_no_user_name`

**Interfaces:**
- Consumes: `cmdstudio_core::doctor::{run, Check, Status}`, `detect` (build, WT locations), `plan::{autorun_summary, AutoRunSummary}`, `adapters::WtLocation`; `backend::apply::{ApplyRequest, PlanOutcome, plan_apply}`, `library::applied_info`, `backend::{settings_or_default, versions, log_tail}`, `snapshots_list`.
- Produces:
  ```rust
  pub enum FixKind { Reapply, ReplaceOtherClink, InstallWindowsTerminal, OpenWindowsTerminalOnce, OpenNewCmdWindow, Reinstall }
  pub struct CheckReport { pub id: String, pub status: Status, pub code: Option<String>, pub message_th: String, pub message_en: String, pub details: String, pub details_th: Option<String>, pub fix: Option<FixKind> }
  pub struct DoctorReport { pub checks: Vec<CheckReport>, pub checked_at_ms: u64 }
  pub struct FixProposal { pub kind: FixKind, pub request: Option<ApplyRequest>, pub plan: Option<PlanOutcome> }
  pub fn fix_for(check: &Check, applied: bool, autorun: Option<&AutoRunSummary>, wt: &[WtLocation], build: Option<u32>) -> Option<FixKind>
  impl<M: Machine> Backend<M> {
      pub fn doctor_run(&self) -> Result<DoctorReport, AppError>
      pub fn doctor_fix(&self, check_id: &str) -> Result<FixProposal, AppError>   // unknown id → E_INTERNAL
      pub fn diagnostics_copy(&self) -> Result<String, AppError>
  }
  pub fn user_name_of(local_app_data: &Path) -> String   // the folder before "AppData"
  pub fn redact(text: &str, user: &str) -> String         // every occurrence, any case → "<you>"
  ```

- [ ] **Step 1: Write the failing tests** — the two files with only their test modules, the two `pub mod` lines in `backend/mod.rs`:

`app/src-tauri/src/backend/doctor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::apply::ApplyStatus;
    use crate::backend::testing::{backend, terracotta_id};
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::adapters::{AutoRun, RegValue, WtVariant, REG_SZ};
    use std::path::PathBuf;

    fn applied(b: &Backend<FakeMachine>) {
        let req = ApplyRequest {
            theme_id: terracotta_id(),
            target: cmdstudio_core::model::Target::All,
            set_wt_default_profile: true,
            replace_other_clink: false,
        };
        let plan = match b.plan_apply(req.clone()).unwrap() {
            PlanOutcome::Ready { plan } => plan,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            b.apply(req, &plan.digest, &mut |_| {}).unwrap().status,
            ApplyStatus::Committed
        );
    }

    fn by_id<'a>(report: &'a DoctorReport, id: &str) -> &'a CheckReport {
        report.checks.iter().find(|c| c.id == id).unwrap()
    }

    #[test]
    fn a_clean_machine_after_an_apply_needs_no_fix() {
        let b = backend();
        applied(&b);
        let report = b.doctor_run().unwrap();
        assert_eq!(report.checks.len(), 11);
        assert!(report.checked_at_ms >= 1_791_694_709_000);
        for c in &report.checks {
            assert!(!c.message_th.is_empty() && !c.message_en.is_empty());
            if c.id != "D3" {
                assert!(c.fix.is_none(), "{}: {:?} {:?}", c.id, c.status, c.fix);
            }
        }
        // D3 warns that no window has loaded the theme yet: the fix is to open one.
        let d3 = by_id(&report, "D3");
        assert_eq!(
            (d3.status, d3.code.as_deref()),
            (Status::Warn, Some("E_THEME_NOT_LOADED"))
        );
        assert_eq!(d3.fix, Some(FixKind::OpenNewCmdWindow));
    }

    #[test]
    fn a_hand_edited_file_offers_to_apply_again_with_a_plan() {
        let b = backend();
        applied(&b);
        let banner = b.paths().generated().join("banner.lua");
        b.sys()
            .fs
            .write_atomic(&banner, b"-- edited by hand")
            .unwrap();
        let report = b.doctor_run().unwrap();
        let d9 = by_id(&report, "D9");
        assert_eq!(
            (d9.code.as_deref(), d9.fix),
            (Some("E_GENERATED_EDITED"), Some(FixKind::Reapply))
        );
        let proposal = b.doctor_fix("D9").unwrap();
        assert_eq!(proposal.kind, Some(FixKind::Reapply));
        let request = proposal.request.unwrap();
        assert_eq!(
            (request.theme_id, request.replace_other_clink),
            (terracotta_id(), false)
        );
        assert!(matches!(proposal.plan, Some(PlanOutcome::Ready { .. })));
    }

    #[test]
    fn another_clink_offers_to_replace_it_and_a_missing_terminal_the_store() {
        let b = backend();
        applied(&b);
        let other = r#""C:\clink\clink_x64.exe" inject --autorun"#;
        let ours = b
            .machine
            .autorun
            .read_user()
            .unwrap()
            .unwrap()
            .text()
            .unwrap();
        b.machine
            .autorun
            .write_user(&RegValue::from_text(REG_SZ, &format!("{ours} & {other}")))
            .unwrap();
        let report = b.doctor_run().unwrap();
        assert_eq!(by_id(&report, "D5").fix, Some(FixKind::ReplaceOtherClink));
        let proposal = b.doctor_fix("D5").unwrap();
        assert!(proposal.request.unwrap().replace_other_clink);
        assert!(matches!(proposal.plan, Some(PlanOutcome::Ready { .. })));
        // No Windows Terminal at all, on a build that can have it.
        let mut b = backend();
        b.machine.wt.locations = vec![];
        let report = b.doctor_run().unwrap();
        assert_eq!(
            by_id(&report, "D6").fix,
            Some(FixKind::InstallWindowsTerminal)
        );
        let proposal = b.doctor_fix("D6").unwrap();
        assert_eq!(
            (proposal.kind, proposal.request, proposal.plan),
            (Some(FixKind::InstallWindowsTerminal), None, None)
        );
        // Too old for Windows Terminal: nothing to offer.
        b.machine.host.info.build = Some(18363);
        assert_eq!(by_id(&b.doctor_run().unwrap(), "D6").fix, None);
        // Installed but never opened: open it once.
        b.machine.wt.locations = vec![WtLocation {
            variant: WtVariant::Store,
            installed: true,
            settings: PathBuf::from(r"C:\nowhere\settings.json"),
        }];
        assert_eq!(
            by_id(&b.doctor_run().unwrap(), "D6").fix,
            Some(FixKind::OpenWindowsTerminalOnce)
        );
    }

    #[test]
    fn a_check_that_could_not_read_offers_nothing_and_an_unknown_id_is_refused() {
        let b = backend();
        applied(&b);
        b.machine.autorun.fail_reads();
        let report = b.doctor_run().unwrap();
        let d4 = by_id(&report, "D4");
        assert_eq!(
            (d4.status, d4.code.as_deref(), d4.fix),
            (Status::Fail, Some("E_MACHINE_READ"), None)
        );
        assert_eq!(b.doctor_fix("D99").unwrap_err().code, "E_INTERNAL");
    }

    #[test]
    fn a_damaged_runtime_before_any_apply_says_reinstall() {
        let b = backend();
        // Nothing applied: D1 is skipped, D2 fails on the bundled file check only when the
        // files are missing. Take the Oh My Posh file away.
        applied(&b);
        let omp = b.paths().omp_exe();
        b.sys().fs.remove_file(&omp).unwrap();
        let report = b.doctor_run().unwrap();
        let d1 = by_id(&report, "D1");
        assert_eq!(
            (d1.code.as_deref(), d1.fix),
            (Some("E_RUNTIME_MISSING"), Some(FixKind::Reapply))
        );
    }
}
```

`app/src-tauri/src/backend/diagnostics.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{backend, terracotta_id};
    use std::path::PathBuf;

    #[test]
    fn the_user_name_is_the_folder_before_appdata() {
        assert_eq!(
            user_name_of(Path::new(r"C:\Users\tester\AppData\Local")),
            "tester"
        );
        assert_eq!(
            user_name_of(Path::new(r"C:\Users\ผู้ทดสอบ ไทย\AppData\Local")),
            "ผู้ทดสอบ ไทย"
        );
        assert_eq!(user_name_of(Path::new(r"D:\Local")), "");
    }

    #[test]
    fn paths_with_the_user_name_are_redacted_whatever_their_case() {
        let text = r"C:\Users\Tester\AppData\Local\x; c:\users\tester\Desktop; C:\Users\testers\x; \tester";
        assert_eq!(
            redact(text, "tester"),
            r"C:\Users\%USERNAME%\AppData\Local\x; c:\users\%USERNAME%\Desktop; C:\Users\testers\x; \%USERNAME%"
        );
        assert_eq!(
            redact(r"C:\Users\ผู้ทดสอบ ไทย\AppData", "ผู้ทดสอบ ไทย"),
            r"C:\Users\%USERNAME%\AppData"
        );
        assert_eq!(redact("nothing here", "tester"), "nothing here");
        assert_eq!(redact(r"C:\Users\tester", ""), r"C:\Users\tester");
        // A name whose lower case is longer falls back to exact-case replacement.
        assert_eq!(
            redact(r"C:\Users\İsa\x \isa", "İsa"),
            r"C:\Users\%USERNAME%\x \isa"
        );
    }

    #[test]
    fn the_diagnostics_hold_the_versions_the_doctor_and_no_user_name() {
        let b = backend();
        let req = crate::backend::apply::ApplyRequest {
            theme_id: terracotta_id(),
            target: cmdstudio_core::model::Target::All,
            set_wt_default_profile: true,
            replace_other_clink: false,
        };
        let plan = match b.plan_apply(req.clone()).unwrap() {
            crate::backend::apply::PlanOutcome::Ready { plan } => plan,
            other => panic!("{other:?}"),
        };
        b.apply(req, &plan.digest, &mut |_| {}).unwrap();
        let text = b.diagnostics_copy().unwrap();
        assert!(
            text.starts_with(
                "CMD Studio diagnostics\napp 0.1.0 · Oh My Posh 31.5.0 · Clink 1.9.34"
            ),
            "{text}"
        );
        assert!(text.contains("applied: \"Terracotta\""), "{text}");
        assert!(text.contains("== doctor ==\nD1   ok"), "{text}");
        assert!(text.contains("== restore points ==\n1791694709"), "{text}");
        assert!(text.contains(r"\%USERNAME%\"), "{text}");
        assert!(!text.contains(r"\tester\"), "{text}");
        let _ = PathBuf::new();
    }
}
```

Run: `cargo test -p cmdstudio-app --lib doctor::`
Expected: FAILS to compile.

- [ ] **Step 2: Write the code above each test module**

`app/src-tauri/src/backend/doctor.rs`:

```rust
//! `doctor_run` and `doctor_fix` (spec §4.3 "ตรวจสุขภาพพร้อมปุ่มแก้", §5.8): the core's checks
//! with, for each, what the app can offer to do about it. A fix that changes the machine is an
//! apply, shown as a plan first (spec §2.1); the others open something or say what to do.
use super::apply::{ApplyRequest, PlanOutcome};
use super::{Backend, Machine};
use crate::error::AppError;
use cmdstudio_core::adapters::WtLocation;
use cmdstudio_core::detect::detect;
use cmdstudio_core::doctor::{doctor, Check, Status};
use cmdstudio_core::plan::{autorun_summary, AutoRunSummary};
use cmdstudio_core::state::State;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::system::WT_MIN_BUILD;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FixKind {
    /// Apply the current theme again (D1, D3, D4, D7, D8, D9).
    Reapply,
    /// Apply again with the other Clink turned off (D5, with the user's agreement).
    ReplaceOtherClink,
    /// Open Windows Terminal's Store page (D6, no Windows Terminal, build 19041 or later).
    InstallWindowsTerminal,
    /// Windows Terminal is installed but was never opened (D6): open it once, then apply again.
    OpenWindowsTerminalOnce,
    /// No cmd window has loaded the theme yet (D3): open a new one.
    OpenNewCmdWindow,
    /// The bundled files are missing or damaged (D2): reinstall CMD Studio.
    Reinstall,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckReport {
    pub id: String,
    pub status: Status,
    pub code: Option<String>,
    pub message_th: String,
    pub message_en: String,
    pub details: String,
    pub details_th: Option<String>,
    pub fix: Option<FixKind>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DoctorReport {
    pub checks: Vec<CheckReport>,
    #[ts(type = "number")]
    pub checked_at_ms: u64,
}

/// What `doctor_fix` offers: the fix, and for an apply the request and its plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FixProposal {
    pub kind: Option<FixKind>,
    pub request: Option<ApplyRequest>,
    pub plan: Option<PlanOutcome>,
}

/// The fix for a check, from what it found. A check that could not read (`E_MACHINE_READ`)
/// has none: check again instead. A fix that applies needs an applied theme.
pub fn fix_for(
    check: &Check,
    applied: bool,
    autorun: Option<&AutoRunSummary>,
    wt: &[WtLocation],
    build: Option<u32>,
) -> Option<FixKind> {
    if matches!(check.status, Status::Ok | Status::Skipped) {
        return None;
    }
    let code = check.code?;
    if code == "E_MACHINE_READ" {
        return None;
    }
    match check.id {
        "D1" | "D4" | "D7" | "D8" | "D9" => applied.then_some(FixKind::Reapply),
        "D2" => (code == "E_RUNTIME_MISSING").then_some(FixKind::Reinstall),
        "D3" if code == "E_THEME_NOT_LOADED" => Some(FixKind::OpenNewCmdWindow),
        "D3" => applied.then_some(FixKind::Reapply),
        "D5" => (applied && autorun.is_some_and(|a| !a.foreign_clink.is_empty()))
            .then_some(FixKind::ReplaceOtherClink),
        "D6" if code == "E_WT_NOT_FOUND" => {
            if wt.iter().any(|l| l.installed) {
                Some(FixKind::OpenWindowsTerminalOnce)
            } else {
                build
                    .is_some_and(|b| b >= WT_MIN_BUILD)
                    .then_some(FixKind::InstallWindowsTerminal)
            }
        }
        _ => None,
    }
}

impl<M: Machine> Backend<M> {
    /// Runs the core's checks (read only) and offers a fix for each where there is one.
    pub fn doctor_run(&self) -> Result<DoctorReport, AppError> {
        let sys = self.sys();
        let ctx = self.ctx();
        let checks = doctor(sys, &ctx);
        let applied = State::load(sys.fs, &self.paths().state_file())
            .ok()
            .is_some_and(|s| s.applied.is_some());
        let detected = detect(sys, self.paths()).ok();
        let autorun = detected
            .as_ref()
            .map(|d| autorun_summary(&d.machine, self.paths()));
        let wt: Vec<WtLocation> = detected.as_ref().map(|d| d.wt.clone()).unwrap_or_default();
        let build = detected.as_ref().and_then(|d| d.info.build);
        let checks = checks
            .iter()
            .map(|c| CheckReport {
                id: c.id.to_string(),
                status: c.status,
                code: c.code.map(str::to_string),
                message_th: c.message_th.clone(),
                message_en: c.message_en.clone(),
                details: c.details.clone(),
                details_th: c.details_th.as_ref().map(|d| d.as_ref().clone()),
                fix: fix_for(c, applied, autorun.as_ref(), &wt, build),
            })
            .collect();
        Ok(DoctorReport {
            checks,
            checked_at_ms: ctx.now_ms,
        })
    }

    /// What "Fix" does for `check_id`: for a fix that applies, the request from state.json
    /// (the applied theme, its target, its default-profile choice) and its plan, which the UI
    /// shows before calling `apply`.
    pub fn doctor_fix(&self, check_id: &str) -> Result<FixProposal, AppError> {
        let report = self.doctor_run()?;
        let check = report
            .checks
            .iter()
            .find(|c| c.id == check_id)
            .ok_or_else(|| AppError::internal(format!("there is no check {check_id}")))?;
        let kind = check.fix;
        let applies = matches!(kind, Some(FixKind::Reapply | FixKind::ReplaceOtherClink));
        if !applies {
            return Ok(FixProposal {
                kind,
                request: None,
                plan: None,
            });
        }
        let state = State::load(self.sys().fs, &self.paths().state_file())?;
        let Some(applied) = state.applied else {
            return Ok(FixProposal {
                kind: None,
                request: None,
                plan: None,
            });
        };
        let request = ApplyRequest {
            theme_id: applied.theme_id,
            target: applied.target,
            set_wt_default_profile: applied.set_default,
            replace_other_clink: kind == Some(FixKind::ReplaceOtherClink),
        };
        let plan = self.plan_apply(request.clone())?;
        Ok(FixProposal {
            kind,
            request: Some(request),
            plan: Some(plan),
        })
    }
}
```

`app/src-tauri/src/backend/diagnostics.rs`:

```rust
//! `diagnostics_copy` (spec §4.3 "แจ้งปัญหา", §5.10): the text the user copies into a GitHub
//! issue. English only, with every path that holds the Windows user name redacted to
//! `%USERNAME%`. Nothing leaves the machine unless the user pastes it.
use super::{versions, Backend, Machine};
use crate::error::AppError;
use cmdstudio_core::doctor::{doctor, Status};
use cmdstudio_core::snapshot::SnapshotStore;
use std::path::Path;

impl<M: Machine> Backend<M> {
    pub fn diagnostics_copy(&self) -> Result<String, AppError> {
        let sys = self.sys();
        let ctx = self.ctx();
        let v = versions();
        let info = sys.host.system_info();
        let mut out = String::new();
        out.push_str("CMD Studio diagnostics\n");
        out.push_str(&format!(
            "app {} · Oh My Posh {} · Clink {} · Nerd Fonts {}\n",
            v.app, v.oh_my_posh, v.clink, v.nerd_fonts
        ));
        out.push_str(&format!(
            "Windows build {} · {:?} · packaged {} · default terminal {:?}\n",
            info.build.map_or("unknown".to_string(), |b| b.to_string()),
            info.arch,
            info.packaged,
            info.default_terminal
        ));
        for u in &info.unreadable {
            out.push_str(&format!("unreadable: {u}\n"));
        }
        out.push_str(&format!("data folder: {}\n", self.paths().data.display()));
        let s = self.settings_or_default();
        out.push_str(&format!(
            "settings: language {:?} · target {:?} · wt default {} · updates {:?} · wizard done {}\n",
            s.language, s.target, s.set_wt_default_profile, s.updates, s.wizard_done
        ));
        let (applied, state_problem) = self.applied_info();
        match (applied, state_problem) {
            (Some(a), _) => out.push_str(&format!(
                "applied: \"{}\" ({}) target {:?} at {} restore point {}\n",
                a.theme_name,
                a.theme_id,
                a.target,
                crate::logs::iso_utc(a.applied_at_ms),
                a.snapshot_id
            )),
            (None, Some(e)) => out.push_str(&format!("state.json: {}: {}\n", e.code, e.details)),
            (None, None) => out.push_str("applied: nothing\n"),
        }
        out.push_str("\n== doctor ==\n");
        for c in doctor(sys, &ctx) {
            let status = match c.status {
                Status::Ok => "ok",
                Status::Warn => "warn",
                Status::Fail => "FAIL",
                Status::Skipped => "skip",
            };
            let code = c.code.map(|c| format!(" [{c}]")).unwrap_or_default();
            out.push_str(&format!("{:<4} {status:<5} {}{code}\n", c.id, c.message_en));
            for line in c.details.lines() {
                out.push_str(&format!("           {line}\n"));
            }
        }
        out.push_str("\n== restore points ==\n");
        let root = self.paths().snapshots();
        let store = SnapshotStore::new(sys.fs, &root);
        match store.ids() {
            Ok(ids) => {
                for id in ids.iter().rev() {
                    match store.manifest(id) {
                        Ok(m) => out.push_str(&format!(
                            "{id} {:?} {} {}{}\n",
                            m.kind,
                            m.theme.as_ref().map(|t| t.name.as_str()).unwrap_or("-"),
                            m.undoes
                                .as_deref()
                                .map(|u| format!("undoes {u} "))
                                .unwrap_or_default(),
                            m.undone_by
                                .as_deref()
                                .map(|u| format!("undone by {u}"))
                                .unwrap_or_default()
                        )),
                        Err(f) => {
                            out.push_str(&format!("{id} unreadable: {}: {}\n", f.code, f.message))
                        }
                    }
                }
            }
            Err(f) => out.push_str(&format!("could not list: {}: {}\n", f.code, f.message)),
        }
        out.push_str("\n== log (last 100 lines) ==\n");
        out.push_str(&self.log_tail(100));
        out.push('\n');
        Ok(redact(&out, &self.user_name()))
    }

    /// The Windows user name, as the folder before `AppData` in the local app data path.
    fn user_name(&self) -> String {
        let Ok(local) = self.sys().host.local_app_data() else {
            return String::new();
        };
        user_name_of(&local)
    }
}

/// `C:\Users\<name>\AppData\Local` → `<name>`; empty when the path has no `AppData` folder.
pub fn user_name_of(local_app_data: &Path) -> String {
    let parts: Vec<String> = local_app_data
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts
        .iter()
        .position(|p| p.eq_ignore_ascii_case("AppData"))
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| parts.get(i))
        .cloned()
        .unwrap_or_default()
}

/// Replaces `\<user>` by `\%USERNAME%` wherever it ends a folder name (followed by `\`, `/`, a
/// quote, whitespace or the end), without regard to letter case. A name that changes length
/// when lower-cased falls back to an exact-case replacement.
pub fn redact(text: &str, user: &str) -> String {
    if user.is_empty() {
        return text.to_string();
    }
    let needle = format!("\\{user}");
    let (lower, lower_needle) = (text.to_lowercase(), needle.to_lowercase());
    if lower.len() != text.len() || lower_needle.len() != needle.len() {
        return text.replace(&needle, "\\%USERNAME%");
    }
    let ends_name = |rest: &str| {
        rest.chars().next().is_none_or(|c| {
            matches!(c, '\\' | '/' | '"' | '\'' | ')' | ']' | ';') || c.is_whitespace()
        })
    };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while let Some(pos) = lower.get(i..).and_then(|rest| rest.find(&lower_needle)) {
        let at = i + pos;
        let end = at + lower_needle.len();
        let (Some(before), Some(found), Some(after)) =
            (text.get(i..at), text.get(at..end), text.get(end..))
        else {
            return text.replace(&needle, "\\%USERNAME%");
        };
        out.push_str(before);
        out.push_str(if ends_name(after) {
            "\\%USERNAME%"
        } else {
            found
        });
        i = end;
    }
    out.push_str(text.get(i..).unwrap_or(""));
    out
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p cmdstudio-app --lib doctor::` → `9 passed`; `… diagnostics::` → `3 passed`; `cargo test -p cmdstudio-app --lib` → `92 passed`. `cargo clippy -p cmdstudio-app --all-targets -- -D warnings` → clean (every item has its user now). fmt clean.

- [ ] **Step 4: Commit**

Stage `app/src-tauri/src/backend/doctor.rs app/src-tauri/src/backend/diagnostics.rs app/src-tauri/src/backend/mod.rs`. Message: `app: doctor_run with a fix per finding, doctor_fix as a plan, diagnostics with the user name redacted`.

---

### Task 14: Opening cmd and the external links

`open_cmd` opens a new cmd window (a Windows Terminal tab with the `CMD Studio` profile when the theme is Terminal-only and Terminal is installed, else a plain `cmd.exe` in a new console, starting in `%USERPROFILE%`) — spec §4.2 page 6 "ลองเปิด cmd", §4.3 "เปิด cmd". `open_external` opens the Store page of Windows Terminal (or `aka.ms/terminal` when the Store is not there), GitHub Issues, the data folder or the logs folder through `ShellExecuteW`. The IPC command is `#[cfg(windows)]`; the decision of what to launch is a pure function with tests.

**Files:**
- Create: `app/src-tauri/src/open.rs`
- Modify: `app/src-tauri/src/lib.rs` (+ `pub mod open;` after the `machine` lines)
- Test: `open_cmd_uses_the_terminal_profile_only_for_a_terminal_only_theme`, `the_links_are_the_specs` (+ 1 export test: `ExternalTarget`)

**Interfaces:**
- Consumes: `cmdstudio_core::model::Target`, `cmdstudio_core::generate::wt::CMD_PROFILE_GUID` (the profile the planner writes), `windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE`, `Win32::UI::Shell::ShellExecuteW`.
- Produces:
  ```rust
  pub enum ExternalTarget { WindowsTerminalStore, GitHubIssues, DataFolder, LogsFolder }
  pub const STORE_URL: &str = "ms-windows-store://pdp/?ProductId=9N0DX20HK701";
  pub const TERMINAL_URL: &str = "https://aka.ms/terminal";
  pub const ISSUES_URL: &str = "https://github.com/luraselenehalo/cmd-studio/issues";
  pub enum CmdLaunch { WindowsTerminalProfile, PlainCmd }
  pub fn cmd_launch(target: Option<Target>, wt_installed: bool) -> CmdLaunch
  pub fn command_line(launch: &CmdLaunch) -> (&'static str, Vec<String>)   // ("wt.exe", ["-p", <guid>]) or ("cmd.exe", [])
  pub fn open_cmd(launch: &CmdLaunch) -> Result<(), String>                 // Windows: spawns with CREATE_NEW_CONSOLE; falls back to cmd.exe when wt.exe is missing
  pub fn open_external(target: ExternalTarget, data: &Path, logs: &Path) -> Result<(), String>
  ```

- [ ] **Step 1: Write the failing tests** — `open.rs` with only its test module, `pub mod open;` in `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_cmd_uses_the_terminal_profile_only_for_a_terminal_only_theme() {
        assert_eq!(
            cmd_launch(Some(Target::Wt), true),
            CmdLaunch::WindowsTerminalProfile
        );
        assert_eq!(cmd_launch(Some(Target::Wt), false), CmdLaunch::PlainCmd);
        assert_eq!(cmd_launch(Some(Target::All), true), CmdLaunch::PlainCmd);
        assert_eq!(cmd_launch(None, true), CmdLaunch::PlainCmd);
        assert_eq!(
            command_line(&CmdLaunch::WindowsTerminalProfile),
            (
                "wt.exe",
                vec![
                    "-p".to_string(),
                    "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}".to_string()
                ]
            )
        );
        assert_eq!(command_line(&CmdLaunch::PlainCmd), ("cmd.exe", Vec::new()));
    }

    #[test]
    fn the_links_are_the_specs() {
        assert_eq!(STORE_URL, "ms-windows-store://pdp/?ProductId=9N0DX20HK701");
        assert_eq!(TERMINAL_URL, "https://aka.ms/terminal");
        assert!(ISSUES_URL.starts_with("https://github.com/") && ISSUES_URL.ends_with("/issues"));
    }
}
```

Run: `cargo test -p cmdstudio-app --lib open::`
Expected: FAILS to compile.

- [ ] **Step 2: Write the code above the test module**

```rust
//! Opening things outside the window: a cmd window (spec §4.3 "เปิด cmd"), Windows Terminal's
//! Store page (spec §4.2 page 2), GitHub Issues (spec §4.3), the data and log folders (spec
//! §2.1). Every target is fixed here; nothing from the UI or a theme is ever run (spec §5.10).
use cmdstudio_core::generate::wt::CMD_PROFILE_GUID;
use cmdstudio_core::model::Target;
use serde::{Deserialize, Serialize};
use std::path::Path;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ExternalTarget {
    /// Windows Terminal in the Store; the web page when the Store cannot open (G18).
    WindowsTerminalStore,
    GitHubIssues,
    DataFolder,
    LogsFolder,
}

pub const STORE_URL: &str = "ms-windows-store://pdp/?ProductId=9N0DX20HK701";
pub const TERMINAL_URL: &str = "https://aka.ms/terminal";
pub const ISSUES_URL: &str = "https://github.com/luraselenehalo/cmd-studio/issues";

/// How "Open cmd" opens a window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CmdLaunch {
    /// `wt.exe -p <cmd profile guid>`: the theme is applied to Windows Terminal only, which is
    /// installed, so a plain cmd window would show no theme.
    WindowsTerminalProfile,
    /// `cmd.exe` in a new console window; Windows 11 shows it in Windows Terminal itself when
    /// that is the default terminal.
    PlainCmd,
}

pub fn cmd_launch(target: Option<Target>, wt_installed: bool) -> CmdLaunch {
    match (target, wt_installed) {
        (Some(Target::Wt), true) => CmdLaunch::WindowsTerminalProfile,
        _ => CmdLaunch::PlainCmd,
    }
}

/// The program and arguments of a launch.
pub fn command_line(launch: &CmdLaunch) -> (&'static str, Vec<String>) {
    match launch {
        CmdLaunch::WindowsTerminalProfile => (
            "wt.exe",
            vec!["-p".to_string(), CMD_PROFILE_GUID.to_string()],
        ),
        CmdLaunch::PlainCmd => ("cmd.exe", Vec::new()),
    }
}

#[cfg(windows)]
pub fn open_cmd(launch: &CmdLaunch) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;
    let (program, args) = command_line(launch);
    let mut command = std::process::Command::new(program);
    command.args(&args).creation_flags(CREATE_NEW_CONSOLE);
    if let Some(home) = std::env::var_os("USERPROFILE") {
        command.current_dir(home);
    }
    match command.spawn() {
        Ok(_) => Ok(()),
        // wt.exe missing after all: a plain window is better than nothing.
        Err(_) if *launch == CmdLaunch::WindowsTerminalProfile => open_cmd(&CmdLaunch::PlainCmd),
        Err(e) => Err(format!("{program} could not be started: {e}")),
    }
}

#[cfg(windows)]
pub fn open_external(target: ExternalTarget, data: &Path, logs: &Path) -> Result<(), String> {
    match target {
        ExternalTarget::WindowsTerminalStore => {
            shell_open(STORE_URL).or_else(|_| shell_open(TERMINAL_URL))
        }
        ExternalTarget::GitHubIssues => shell_open(ISSUES_URL),
        ExternalTarget::DataFolder => shell_open(&data.display().to_string()),
        ExternalTarget::LogsFolder => shell_open(&logs.display().to_string()),
    }
}

/// `ShellExecuteW("open", target)`: a URL in the default browser or the Store, a folder in
/// Explorer. Windows reports success as a value above 32.
#[cfg(windows)]
fn shell_open(target: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let (verb, file) = (wide("open"), wide(target));
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call; the other
    // pointers are null as the documentation allows; the result is read as a number only.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        Ok(())
    } else {
        Err(format!(
            "{target} could not be opened (ShellExecuteW returned {})",
            result as isize
        ))
    }
}

#[cfg(not(windows))]
pub fn open_cmd(_launch: &CmdLaunch) -> Result<(), String> {
    Err("CMD Studio runs on Windows only".to_string())
}

#[cfg(not(windows))]
pub fn open_external(_target: ExternalTarget, _data: &Path, _logs: &Path) -> Result<(), String> {
    Err("CMD Studio runs on Windows only".to_string())
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p cmdstudio-app --lib open::` → `3 passed`; `cargo test -p cmdstudio-app --lib` → `95 passed`. Clippy and fmt clean. (Nothing here runs `cmd.exe` or `wt.exe`: the tests check the pure functions only.)

- [ ] **Step 4: Commit**

Stage `app/src-tauri/src/open.rs app/src-tauri/src/lib.rs`. Message: `app: open a cmd window and the external links (Store, GitHub Issues, data and logs folders)`.

---

### Task 15: The IPC commands, the window, the exe, the bindings, the Rust CI job

The 27 `#[tauri::command]`s: each takes its arguments, runs the backend on `tauri::async_runtime::spawn_blocking` inside `Backend::logged`, and returns `Result<T, AppError>`. `apply` emits `apply-progress` (`Emitter::emit`) for each `ApplyProgress`. `theme_import_preview` and `theme_export` show rfd's Open/Save dialog on the main thread (`Window::run_on_main_thread`) with the window as parent, and answer `None` when the user cancels. `handler()` is `tauri::generate_handler!` over exactly `COMMANDS`; two tests hold the three lists together (the names, the handler's source, the capability file). `run_gui` builds the app: the single-instance plugin focusing the window, the managed `Arc<Backend<RealMachine>>`, the window from the `main` config with `data_directory(<data>\webview)`. `main.rs` chooses GUI or CLI. The bindings are exported to `app/ui/src/bindings` and committed; CI's new `app` job (Rust part) builds, tests and checks them.

**Files:**
- Create: `app/src-tauri/src/ipc.rs`, `app/src-tauri/src/main.rs`, `scripts/export-bindings.ps1`, `app/ui/src/bindings/*.ts` (71 generated files)
- Modify: `app/src-tauri/src/lib.rs` (replace), `app/src-tauri/Cargo.toml` (+ `[[bin]]`), `.github/workflows/ci.yml` (+ job `app`)
- Test: `every_named_command_is_a_tauri_command_here_and_in_the_handler_and_no_other`, `the_capability_allows_exactly_the_named_commands_and_the_event` (in `ipc.rs`); CI.

**Interfaces:**
- Consumes: everything of Tasks 5–14; `tauri 2.12.4`: `tauri::{command, generate_handler, generate_context, Builder, Manager, Emitter, State, Window, WebviewWindowBuilder, async_runtime::spawn_blocking, ipc::Invoke}`, `tauri_plugin_single_instance::init`, `rfd::FileDialog::{new, set_parent, set_file_name, add_filter, pick_file, save_file}` 0.17.2 (`set_parent` takes a `&W: HasWindowHandle + HasDisplayHandle`; Tauri's `Window` implements both); `cmdstudio_cli::{run, Io, EXIT_INVALID}`.
- Produces: `pub const APPLY_PROGRESS_EVENT: &str = "apply-progress"`; `pub fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static`; `pub fn run_gui()`; the exe `cmdstudio`. Argument names of the commands, as the UI sends them (camelCase): `theme_get(id)`, `theme_save(theme, draft)`, `theme_duplicate(id, name)`, `theme_delete(id, draftOnly)`, `theme_import_confirm(token)`, `theme_export(id)`, `palette_generate(tone, accent, tint)`, `contrast_check(colors)`, `contrast_fix(colors)`, `preview_render(theme, context)`, `plan_apply(request)`, `apply(request, digest)`, `snapshot_restore(id)`, `doctor_fix(checkId)`, `settings_set(settings)`, `open_external(target)`; the rest take none. Returns: as the `Backend` methods; `theme_import_preview` → `Option<ImportPreview>`, `theme_export` → `Option<String>` (the path written).

- [ ] **Step 1: Write the tests** — `ipc.rs` with only its test module (the tests read `ipc.rs` itself and `capabilities/default.json` with `include_str!`, so they run once the file is complete; Step 2 completes it):

```rust
#[cfg(test)]
mod tests {
    use crate::command_names::COMMANDS;
    use std::collections::BTreeSet;

    const SOURCE: &str = include_str!("ipc.rs");
    const CAPABILITY: &str = include_str!("../capabilities/default.json");

    #[test]
    fn every_named_command_is_a_tauri_command_here_and_in_the_handler_and_no_other() {
        // The handler list is the text between `generate_handler![` and `]`.
        let start = SOURCE.find("tauri::generate_handler![").unwrap();
        let rest = SOURCE.get(start..).unwrap();
        let end = rest.find(']').unwrap();
        let listed: BTreeSet<&str> = rest
            .get(..end)
            .unwrap()
            .split([',', '\n', ' ', '!', '['])
            .filter(|w| !w.is_empty() && *w != "tauri::generate_handler")
            .collect();
        let named: BTreeSet<&str> = COMMANDS.iter().copied().collect();
        assert_eq!(listed, named);
        for name in COMMANDS {
            let decl_async = format!("pub async fn {name}(");
            let decl = format!("pub fn {name}(");
            assert!(
                SOURCE.contains(&decl_async) || SOURCE.contains(&decl),
                "{name} has no #[tauri::command] function"
            );
        }
        assert_eq!(COMMANDS.len(), 27);
    }

    #[test]
    fn the_capability_allows_exactly_the_named_commands_and_the_event() {
        let capability: serde_json::Value = serde_json::from_str(CAPABILITY).unwrap();
        let permissions: BTreeSet<String> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_string())
            .collect();
        let mut expected: BTreeSet<String> = COMMANDS
            .iter()
            .map(|c| format!("allow-{}", c.replace('_', "-")))
            .collect();
        expected.insert("core:event:allow-listen".to_string());
        expected.insert("core:event:allow-unlisten".to_string());
        assert_eq!(permissions, expected);
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
    }
}
```

- [ ] **Step 2: Write the commands above the test module**

```rust
//! The IPC commands (spec §5.7), one `#[tauri::command]` each, as thin wrappers over the
//! [`Backend`]: they move arguments and results across, run the work off the main thread, and
//! log it. Nothing is decided here, so the tests of what a command does are the backend's; the
//! tests here hold the list of commands to `command_names::COMMANDS`, to what Tauri registers
//! and to what `capabilities/default.json` allows (spec §5.10).
use crate::backend::apply::{
    ApplyOutcome, ApplyProgress, ApplyRequest, PlanOutcome, RestoreOutcome, RestorePoints,
    RevertOutcome, UndoOutcome,
};
use crate::backend::doctor::{DoctorReport, FixProposal};
use crate::backend::preview::{self, ContrastIssue, PreviewResult};
use crate::backend::system::SystemReport;
use crate::backend::{AdvancedInfo, Backend};
use crate::error::AppError;
use crate::library::{ImportPreview, Library, ThemeDetail, ThemeSummary};
use crate::machine::RealMachine;
use crate::open::{self, ExternalTarget};
use crate::settings::{AppSettings, SettingsLoad};
use cmdstudio_core::model::{Target, Theme};
use cmdstudio_core::palette::{Colors, Tone};
use cmdstudio_core::preview::PreviewContext;
use cmdstudio_core::state::State;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State as TauriState, WebviewWindow};
use uuid::Uuid;

/// The event `apply` emits for each step (spec §4.2 page 5).
pub const APPLY_PROGRESS_EVENT: &str = "apply-progress";

type Shared = Arc<Backend<RealMachine>>;

/// Runs `f` on a worker thread, so a command that runs Oh My Posh or Clink never blocks the
/// window's thread (sync commands run there).
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    match tauri::async_runtime::spawn_blocking(f).await {
        Ok(result) => result,
        Err(e) => Err(AppError::internal(format!(
            "the worker that ran the command did not answer: {e}"
        ))),
    }
}

#[tauri::command]
pub async fn system_detect(state: TauriState<'_, Shared>) -> Result<SystemReport, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("system_detect", |b| b.system_detect())).await
}

#[tauri::command]
pub async fn themes_list(state: TauriState<'_, Shared>) -> Result<Library, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("themes_list", |b| b.themes_list())).await
}

#[tauri::command]
pub async fn theme_get(state: TauriState<'_, Shared>, id: Uuid) -> Result<ThemeDetail, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("theme_get", |b| b.theme_get(id))).await
}

#[tauri::command]
pub async fn theme_save(
    state: TauriState<'_, Shared>,
    theme: Theme,
    draft: bool,
) -> Result<ThemeSummary, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("theme_save", |b| b.theme_save(theme, draft))).await
}

#[tauri::command]
pub async fn theme_duplicate(
    state: TauriState<'_, Shared>,
    id: Uuid,
    name: String,
) -> Result<ThemeSummary, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("theme_duplicate", |b| b.theme_duplicate(id, name))).await
}

#[tauri::command]
pub async fn theme_delete(
    state: TauriState<'_, Shared>,
    id: Uuid,
    draft_only: bool,
) -> Result<(), AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("theme_delete", |b| b.theme_delete(id, draft_only))).await
}

/// Shows the native Open dialog for a `.cmdtheme` file on the window's thread (a modal dialog
/// needs it) and waits for the answer on a worker. `None` when the user cancelled.
async fn pick_theme_file(window: &WebviewWindow) -> Result<Option<PathBuf>, AppError> {
    let (tx, rx) = std::sync::mpsc::channel();
    let parent = window.clone();
    window
        .run_on_main_thread(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("CMD Studio theme", &["cmdtheme"])
                .set_parent(&parent)
                .pick_file();
            let _ = tx.send(picked);
        })
        .map_err(|e| AppError::internal(format!("the file dialog could not be shown: {e}")))?;
    blocking(move || {
        rx.recv()
            .map_err(|e| AppError::internal(format!("the file dialog gave no answer: {e}")))
    })
    .await
}

/// The native Save dialog, as [`pick_theme_file`], offering `file_name`.
async fn save_theme_file(
    window: &WebviewWindow,
    file_name: String,
) -> Result<Option<PathBuf>, AppError> {
    let (tx, rx) = std::sync::mpsc::channel();
    let parent = window.clone();
    window
        .run_on_main_thread(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("CMD Studio theme", &["cmdtheme"])
                .set_file_name(file_name)
                .set_parent(&parent)
                .save_file();
            let _ = tx.send(picked);
        })
        .map_err(|e| AppError::internal(format!("the file dialog could not be shown: {e}")))?;
    blocking(move || {
        rx.recv()
            .map_err(|e| AppError::internal(format!("the file dialog gave no answer: {e}")))
    })
    .await
}

#[tauri::command]
pub async fn theme_import_preview(
    window: WebviewWindow,
    state: TauriState<'_, Shared>,
) -> Result<Option<ImportPreview>, AppError> {
    let Some(path) = pick_theme_file(&window).await? else {
        return Ok(None);
    };
    let b = state.inner().clone();
    blocking(move || {
        b.logged("theme_import_preview", |b| {
            b.theme_import_preview_from(&path)
        })
    })
    .await
    .map(Some)
}

#[tauri::command]
pub async fn theme_import_confirm(
    state: TauriState<'_, Shared>,
    token: String,
) -> Result<ThemeSummary, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("theme_import_confirm", |b| b.theme_import_confirm(&token))).await
}

/// Exports a theme to a file the user picks; the path written, or `None` when cancelled.
#[tauri::command]
pub async fn theme_export(
    window: WebviewWindow,
    state: TauriState<'_, Shared>,
    id: Uuid,
) -> Result<Option<String>, AppError> {
    let b = state.inner().clone();
    let file_name = {
        let b = b.clone();
        blocking(move || b.export_file_name(id)).await?
    };
    let Some(path) = save_theme_file(&window, file_name).await? else {
        return Ok(None);
    };
    blocking(move || {
        b.logged("theme_export", |b| {
            b.theme_export_to(id, &path)
                .map(|()| Some(path.display().to_string()))
        })
    })
    .await
}

#[tauri::command]
pub fn palette_generate(tone: Tone, accent: String, tint: f64) -> Result<Colors, AppError> {
    preview::palette_generate(tone, &accent, tint)
}

#[tauri::command]
pub fn contrast_check(colors: Colors) -> Vec<ContrastIssue> {
    preview::contrast_check(&colors)
}

#[tauri::command]
pub fn contrast_fix(colors: Colors) -> Colors {
    preview::contrast_fix(colors)
}

#[tauri::command]
pub fn preview_render(theme: Theme, context: PreviewContext) -> PreviewResult {
    preview::preview_render(&theme, &context)
}

#[tauri::command]
pub async fn plan_apply(
    state: TauriState<'_, Shared>,
    request: ApplyRequest,
) -> Result<PlanOutcome, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("plan_apply", |b| b.plan_apply(request))).await
}

#[tauri::command]
pub async fn apply(
    app: AppHandle,
    state: TauriState<'_, Shared>,
    request: ApplyRequest,
    digest: String,
) -> Result<ApplyOutcome, AppError> {
    let b = state.inner().clone();
    blocking(move || {
        let mut tell = |p: ApplyProgress| {
            let _ = app.emit(APPLY_PROGRESS_EVENT, &p);
        };
        b.logged("apply", |b| b.apply(request, &digest, &mut tell))
    })
    .await
}

#[tauri::command]
pub async fn undo_last(state: TauriState<'_, Shared>) -> Result<UndoOutcome, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("undo_last", |b| b.undo_last())).await
}

#[tauri::command]
pub async fn snapshots_list(state: TauriState<'_, Shared>) -> Result<RestorePoints, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("snapshots_list", |b| b.snapshots_list())).await
}

#[tauri::command]
pub async fn snapshot_restore(
    state: TauriState<'_, Shared>,
    id: String,
) -> Result<RestoreOutcome, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("snapshot_restore", |b| b.snapshot_restore(&id))).await
}

#[tauri::command]
pub async fn revert_all(state: TauriState<'_, Shared>) -> Result<RevertOutcome, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("revert_all", |b| b.revert_all())).await
}

#[tauri::command]
pub async fn doctor_run(state: TauriState<'_, Shared>) -> Result<DoctorReport, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("doctor_run", |b| b.doctor_run())).await
}

#[tauri::command]
pub async fn doctor_fix(
    state: TauriState<'_, Shared>,
    check_id: String,
) -> Result<FixProposal, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("doctor_fix", |b| b.doctor_fix(&check_id))).await
}

#[tauri::command]
pub async fn settings_get(state: TauriState<'_, Shared>) -> Result<SettingsLoad, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("settings_get", |b| b.settings_get())).await
}

#[tauri::command]
pub async fn settings_set(
    state: TauriState<'_, Shared>,
    settings: AppSettings,
) -> Result<AppSettings, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("settings_set", |b| b.settings_set(settings))).await
}

/// Opens a cmd window: Windows Terminal's cmd profile when the theme is applied to Windows
/// Terminal only, else a plain cmd window (spec §4.3).
#[tauri::command]
pub async fn open_cmd(state: TauriState<'_, Shared>) -> Result<(), AppError> {
    let b = state.inner().clone();
    blocking(move || {
        b.logged("open_cmd", |b| {
            let target: Option<Target> = State::load(b.sys().fs, &b.paths().state_file())
                .ok()
                .and_then(|s| s.applied.map(|a| a.target));
            let wt_installed = b.sys().wt.locations().iter().any(|l| l.installed);
            open::open_cmd(&open::cmd_launch(target, wt_installed)).map_err(AppError::internal)
        })
    })
    .await
}

#[tauri::command]
pub async fn open_external(
    state: TauriState<'_, Shared>,
    target: ExternalTarget,
) -> Result<(), AppError> {
    let b = state.inner().clone();
    blocking(move || {
        b.logged("open_external", |b| {
            open::open_external(target, &b.paths().data, &b.paths().logs())
                .map_err(AppError::internal)
        })
    })
    .await
}

#[tauri::command]
pub async fn diagnostics_copy(state: TauriState<'_, Shared>) -> Result<String, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("diagnostics_copy", |b| b.diagnostics_copy())).await
}

#[tauri::command]
pub async fn advanced_read(state: TauriState<'_, Shared>) -> Result<AdvancedInfo, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged("advanced_read", |b| b.advanced_read())).await
}

/// Every command, for `tauri::Builder::invoke_handler`.
pub fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        system_detect,
        themes_list,
        theme_get,
        theme_save,
        theme_duplicate,
        theme_delete,
        theme_import_preview,
        theme_import_confirm,
        theme_export,
        palette_generate,
        contrast_check,
        contrast_fix,
        preview_render,
        plan_apply,
        apply,
        undo_last,
        snapshots_list,
        snapshot_restore,
        revert_all,
        doctor_run,
        doctor_fix,
        settings_get,
        settings_set,
        open_cmd,
        open_external,
        diagnostics_copy,
        advanced_read,
    ]
}
```

- [ ] **Step 3: Replace `app/src-tauri/src/lib.rs`**

```rust
//! CMD Studio's desktop app (Tauri 2): a window over `cmdstudio-core`. The IPC commands of
//! spec §5.7 are thin wrappers (`ipc`) over a [`backend::Backend`], which is tested against the
//! core's in-memory adapters; the theme library and drafts (`library`), the app's settings
//! (`settings`) and its log (`logs`) live in the data folder (spec §5.3); the CLI is the same
//! exe (`main.rs`). The UI (`app/ui`) paints what the core computes: no palette, contrast or
//! prompt logic lives in TypeScript (spec §5.6).
pub mod backend;
pub mod command_names;
pub mod console;
pub mod error;
#[cfg(windows)]
pub mod ipc;
pub mod library;
pub mod logs;
#[cfg(windows)]
pub mod machine;
pub mod open;
pub mod settings;

/// Opens the window. Everything that can fail before it exists (the data folder, the window
/// itself) ends in a message box and exit code 1; after that, Tauri owns the thread.
#[cfg(windows)]
pub fn run_gui() {
    use tauri::Manager;
    let backend = match machine::real_backend() {
        Ok(backend) => std::sync::Arc::new(backend),
        Err(why) => {
            console::fatal(&format!("CMD Studio could not start: {why}"));
            std::process::exit(1);
        }
    };
    let for_window = backend.clone();
    let built = tauri::Builder::default()
        // A second start focuses this window instead of opening another (spec §5.7). The plugin
        // has no JS commands, so the capability file does not change.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .manage(backend)
        .invoke_handler(ipc::handler())
        .setup(move |app| {
            // The window is in tauri.conf.json with `create: false`, so that WebView2's data
            // folder can be set here: `<data>\webview`, a folder of CMD Studio's choosing in its
            // data folder, which `--cleanup` knows (spec §5.3). Tauri's own default would be the
            // data folder itself, where WebView2 makes an `EBWebView` folder of its own.
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .cloned()
                .ok_or("tauri.conf.json has no window labelled main")?;
            tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?
                .data_directory(for_window.paths().webview())
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!());
    match built {
        Ok(app) => app.run(|_, _| {}),
        Err(e) => {
            console::fatal(&format!("CMD Studio could not open its window: {e}"));
            std::process::exit(1);
        }
    }
}

#[cfg(not(windows))]
pub fn run_gui() {
    console::fatal("CMD Studio runs on Windows only.");
}
```

- [ ] **Step 4: Write `app/src-tauri/src/main.rs`** and add the binary to `app/src-tauri/Cargo.toml` after the `[lib]` section:

```toml
[[bin]]
name = "cmdstudio"
path = "src/main.rs"
```

```rust
//! `cmdstudio.exe`: the app and the CLI in one exe (spec §5.7). Started with no arguments it
//! opens the window; with any, it runs `cmdstudio_cli::run` and exits with its code, so the
//! uninstaller's `--cleanup`, `cmdstudio doctor` and the rest are the same program.
//!
//! A release build is a Windows-subsystem exe (no console window flashes when the app starts),
//! so in CLI mode it attaches to the console it was started from first (`console::attach_parent`).
//! cmd does not wait for a Windows-subsystem program, so `cmdstudio doctor` typed in cmd prints
//! after the prompt came back; `cmdstudio doctor | more`, or `start /wait cmdstudio doctor`,
//! waits. The uninstaller runs it with `ExecWait`, and tests call the library: both wait.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.is_empty() {
        cmdstudio_app::run_gui();
        return;
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

#[cfg(not(windows))]
fn main() {
    eprintln!("CMD Studio runs on Windows only.");
    std::process::exit(cmdstudio_cli::EXIT_INVALID);
}
```

- [ ] **Step 5: Build and test**

Run: `cargo test -p cmdstudio-app --lib`
Expected: `test result: ok. 97 passed` (95 + the two IPC tests). If Application Control blocks a fresh test exe (`os error 4551`), rerun once; if blocked again, CI runs it (Step 8).
Run: `cargo build -p cmdstudio-app --release`
Expected: `Finished`; `target\release\cmdstudio.exe` exists. **Do not run it** on the dev PC (Global Constraints); the VMs run it (phase 3 plan, Task 4).
Run: `cargo clippy --workspace --all-targets -- -D warnings` and the three core clippy commands; `cargo fmt --all -- --check`.
Expected: clean.

- [ ] **Step 6: The bindings script, and the bindings**

```powershell
<#
.SYNOPSIS
  Writes the TypeScript bindings of the Rust types the UI consumes (ts-rs 12, G11) into
  app\ui\src\bindings, from the core (feature `ts`) and the app crate. CI runs this and fails
  when `git diff` shows a change: the committed bindings must match the Rust types.
.DESCRIPTION
  ts-rs exports through tests named export_bindings_*; TS_RS_EXPORT_DIR tells them where. The
  app crate's export runs through the check crate when given -NoTauri, for a machine where the
  Tauri build is blocked (Windows Application Control, os error 4551); CI runs the app crate.
#>
param(
  [switch] $NoTauri
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$out = Join-Path $root 'app\ui\src\bindings'
New-Item -ItemType Directory -Force $out | Out-Null
$env:TS_RS_EXPORT_DIR = $out
Push-Location $root
try {
  cargo test -p cmdstudio-core --features ts --lib -- export_bindings
  if ($LASTEXITCODE -ne 0) { throw "core bindings export failed ($LASTEXITCODE)" }
  if ($NoTauri) {
    Write-Host 'skipping the app crate (-NoTauri): run this on CI for the app bindings'
  } else {
    cargo test -p cmdstudio-app --lib -- export_bindings
    if ($LASTEXITCODE -ne 0) { throw "app bindings export failed ($LASTEXITCODE)" }
  }
} finally {
  Pop-Location
  Remove-Item Env:TS_RS_EXPORT_DIR -ErrorAction SilentlyContinue
}
Write-Host "bindings written to $out"
```

Run: `pwsh -NoProfile -File .\scripts\export-bindings.ps1` (if the app crate's test exe is blocked on this machine: `… -NoTauri`, then take the app crate's 48 files from appendix B's check crate run with `TS_RS_EXPORT_DIR` set to `app\ui\src\bindings`)
Expected: `bindings written to <repo>\app\ui\src\bindings`; the folder holds **71** `.ts` files: the core's 23 (`Theme`, `SimpleRecipe`, `TerminalSettings`, `CursorShape`, `PromptSettings`, `Layout`, `Segment`, `BannerSettings`, `Target`, `Tone`, `Colors`, `Span`, `Line`, `PreviewContext`, `Clock`, `GitState`, `Status`, `Check`, `SnapshotKind`, `ThemeRef`, `Manifest`, `Unrestored`, `Incomplete`) and the app's 48 (`AppError`, `AppSettings`, `Language`, `UpdateMode`, `SettingsLoad`, `Note`, `Versions`, `GeneratedFile`, `AdvancedInfo`, `WtKind`, `WtInfo`, `AutoRunInfo`, `FontInfo`, `SystemReport`, `ThemeSource`, `ThemeSummary`, `LibraryProblem`, `AppliedInfo`, `Library`, `ThemeDetail`, `ImportPreview`, `PreviewResult`, `ContrastIssue`, `ApplyRequest`, `StepKind`, `PlanStep`, `PlanWarning`, `ApplyPlan`, `PlanOutcome`, `ApplyStatus`, `ApplyOutcome`, `Phase`, `ApplyProgress`, `UndoStatus`, `FontLeft`, `UndoOutcome`, `RestorePoint`, `RestorePoints`, `RestoreStatus`, `RestoreOutcome`, `TxnStatus`, `StepReport`, `RevertOutcome`, `FixKind`, `CheckReport`, `DoctorReport`, `FixProposal`, `ExternalTarget`). Each is `// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.` followed by the type; `Line.ts` is `export type Line = Array<{ text: string, fg: string, bg: string | null, bold: boolean, }>;` (the `ts(as = "Vec<Span>")` override, inlined by ts-rs 12).

- [ ] **Step 7: The Rust part of CI's `app` job** — append to `.github/workflows/ci.yml` after the `windows` job (same indentation as `windows:`; the 2c plan's Task 7 adds the Node steps and the Tauri build):

```yaml
  # The app (Phase 2b): the app crate, its tests over the in-memory machine, and the bindings'
  # freshness. Separate from the Rust job so a UI failure and a Rust failure read apart.
  app:
    runs-on: windows-latest
    timeout-minutes: 60
    env:
      CARGO_TERM_COLOR: always
    steps:
      - uses: actions/checkout@v4
      - name: Install the toolchain pinned in rust-toolchain.toml
        shell: bash
        run: |
          rustup --version
          rustup toolchain install
          cargo --version
      - name: Cache the pinned runtimes and font
        uses: actions/cache@v4
        with:
          path: resources
          key: resources-${{ runner.os }}-${{ hashFiles('scripts/fetch-resources.ps1') }}
      - name: Fetch the pinned runtimes and font
        shell: powershell
        run: ./scripts/fetch-resources.ps1
      # The committed bindings must be what the Rust types export now (G11). The script runs the
      # core's and the app's export tests, then git shows any difference.
      - name: Bindings are fresh
        shell: bash
        run: |
          pwsh -File ./scripts/export-bindings.ps1
          if ! git diff --exit-code -- app/ui/src/bindings; then
            echo "::error::app/ui/src/bindings is stale: run scripts/export-bindings.ps1 and commit"
            exit 1
          fi
      - name: Clippy of the app crate
        run: cargo clippy -p cmdstudio-app --all-targets -- -D warnings
      - name: Tests of the app crate (backend over the in-memory machine)
        run: cargo test -p cmdstudio-app
      - name: Release build of the exe (not run)
        run: cargo build -p cmdstudio-app --release
```

Check: `git diff -- .github/workflows/ci.yml` shows only the added job; the `windows` job is unchanged (it already runs `cargo test --workspace`, which now includes the app crate's tests, and `cargo clippy --workspace`).

- [ ] **Step 8: Commit**

Stage `app/src-tauri/src/ipc.rs app/src-tauri/src/lib.rs app/src-tauri/src/main.rs app/src-tauri/Cargo.toml scripts/export-bindings.ps1 app/ui/src/bindings .github/workflows/ci.yml`. Message: `app: the 27 IPC commands, the window with its own WebView2 folder, one exe for the app and the CLI; bindings and the CI app job`. If the app crate was not compiled locally, say so in the message ("not compiled locally; CI is the gate").

---

### Task 16: Spec §5.3 and §5.7 say what the app now does

The data folder's tree gains `settings.json` and `webview\` and stops saying `state.json` holds the settings; the IPC list gains the three commands; the `--cleanup` sentence names the app's entries (the code of Task 4 already does). Edit the working copy as it is (it holds the coordinator's edits).

**Files:**
- Modify: `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§5.3, §5.7)

- [ ] **Step 1: §5.3's tree** — replace the two lines

```
├─ state.json          ธีมที่ใช้อยู่ + hash ของไฟล์ที่สร้าง + เวอร์ชัน runtime + settings
├─ themes\             ธีมของฉัน (*.cmdtheme) ไม่ลบตอนถอนโปรแกรม เว้นแต่ผู้ใช้เลือกให้ลบ
```

with

```
├─ state.json          ธีมที่ใช้อยู่ + hash ของไฟล์ที่สร้าง + เวอร์ชัน runtime (ไม่มีการตั้งค่าของแอป เพราะ undo คืนไฟล์นี้ทั้งไฟล์)
├─ settings.json       การตั้งค่าของแอป: ภาษา (ค่าเริ่มต้น en), ให้ธีมไปอยู่ตรงไหน, โหมดอัปเดต (ค่าเริ่มต้น auto), วิซาร์ดทำเสร็จหรือยัง · ไฟล์ที่เสียหรือเป็นรุ่นใหม่กว่าจะถูกกันไว้เป็น settings.json.damaged-<ms> แล้วใช้ค่าเริ่มต้นและบอกผู้ใช้ (phase 2b)
├─ themes\             ธีมของฉัน (*.cmdtheme ชื่อไฟล์ตามชื่อธีม) ไม่ลบตอนถอนโปรแกรม เว้นแต่ผู้ใช้เลือกให้ลบ
```

and the line `├─ drafts\             ร่างที่บันทึกอัตโนมัติ` with `├─ drafts\             ร่างที่บันทึกอัตโนมัติ (<id>.cmdtheme หนึ่งวินาทีหลังแก้ครั้งล่าสุด)`, and insert after the `snapshots\<id>\` line:

```
├─ webview\            ข้อมูลของ WebView2 · แอปกำหนดโฟลเดอร์นี้เองแทนค่าเริ่มต้นของ Tauri (ซึ่งจะสร้าง EBWebView\ ในโฟลเดอร์ข้อมูลตรงๆ) เพื่อให้ --cleanup รู้จักทุกอย่างในโฟลเดอร์ (phase 2b)
```

- [ ] **Step 2: §5.7's IPC list** — replace

```
- `palette_generate`, `contrast_check`, `contrast_fix`
```

with

```
- `palette_generate`, `contrast_check`, `contrast_fix`
- `preview_render` (วาดพรีวิวจากธีมและค่าตัวอย่างด้วยตัวจำลองใน core §5.6 คืนบรรทัดของ span ให้ UI ระบายสี) (phase 2b)
```

replace `- \`open_cmd\`` with

```
- `open_cmd`, `open_external` (เปิด Store ของ Windows Terminal, GitHub Issues, โฟลเดอร์ข้อมูล หรือโฟลเดอร์ log ผ่าน ShellExecute ไม่มี URL อื่น) (phase 2b)
```

and replace `- \`diagnostics_copy\`` with

```
- `diagnostics_copy`, `advanced_read` (ข้อมูลของหน้าขั้นสูง: เวอร์ชัน, ท้าย log, โฟลเดอร์ข้อมูล, ไฟล์ที่สร้าง) (phase 2b)
```

Then, after the list's last line, add the paragraph:

```
ทุกคำสั่งเป็นตัวห่อบางๆ ของ `Backend` ใน `app/src-tauri/src/backend` ซึ่งเทสต์กับ adapter ปลอมของ core ทั้งหมด · ทุก error ข้าม IPC เป็น `{code, messageTh, messageEn, details}` ตาม §5.9 · ไฟล์ capability อนุญาตเฉพาะ 27 คำสั่งนี้กับ event `apply-progress` และ CSP ไม่มี origin ภายนอก (§5.10) · `plan_apply` ปล่อยล็อกทันทีที่วางแผนเสร็จและคืน digest ของแผน ส่วน `apply` วางแผนใหม่ใต้ล็อกแล้วทำต่อเมื่อ digest ตรงกัน ไม่ตรงจะคืน `planChanged` พร้อมแผนใหม่ให้แสดงอีกครั้ง (phase 2b)
```

- [ ] **Step 3: §5.7's `--cleanup` sentence** — in the long paragraph after the exit-code line, replace

```
(โฟลเดอร์ `generated\`, `clink-profile\`, `runtime\`, `snapshots\`, `themes\` และไฟล์ `state.json`, `cmdstudio.lock`, `state.json.damaged-<ms>` และไฟล์ชั่วคราว `.state.json.<hex 32 ตัว>.cmdstudio-tmp` ที่การเขียนซึ่งถูกตัดกลางทางทิ้งไว้ เทียบชื่อโดยไม่สนตัวพิมพ์เล็กใหญ่)
```

with

```
(โฟลเดอร์ `generated\`, `clink-profile\`, `runtime\`, `snapshots\`, `themes\`, `drafts\`, `logs\`, `webview\` และไฟล์ `state.json`, `settings.json`, `cmdstudio.lock`, `state.json.damaged-<ms>`, `settings.json.damaged-<ms>` และไฟล์ชั่วคราว `.state.json.<hex 32 ตัว>.cmdstudio-tmp` กับ `.settings.json.<hex 32 ตัว>.cmdstudio-tmp` ที่การเขียนซึ่งถูกตัดกลางทางทิ้งไว้ เทียบชื่อโดยไม่สนตัวพิมพ์เล็กใหญ่ · `drafts\`, `logs\`, `webview\` และ `settings.json` เพิ่มใน phase 2b)
```

and replace

```
(`cmdstudio.lock`, `state.json` หรือ `state.json.damaged-<ms>`)
```

with

```
(`cmdstudio.lock`, `state.json`, `settings.json` หรือสำเนา `.damaged-<ms>` ของสองไฟล์หลัง)
```

- [ ] **Step 4: Check and commit**

Run: `cargo test -p cmdstudio-app --lib error::` (the §5.9 table is untouched by this task; the test still passes) and `cargo test -p cmdstudio-core --test error_codes`.
Expected: `4 passed`, `2 passed`.
Stage `docs/superpowers/specs/2026-10-10-cmd-studio-design.md`. Message: `spec: §5.3 settings.json and webview\, §5.7 the app's IPC commands and --cleanup entries (phase 2b)`.

---

## Appendix A: the `Cargo.lock` entries the plan was verified with

The 381 `[[package]]` entries (name and version) that the scratch copy's `Cargo.lock` gained when the app crate joined the workspace, 2026-10-16 (`cmdstudio-app 0.1.0` among them; nothing was removed). Compare with `git diff Cargo.lock` after Task 5; a transitive crate at a newer compatible version is acceptable (index Decision 14).

```text
adler2 2.0.1
aho-corasick 1.1.5
alloc-no-stdlib 3.0.0
alloc-stdlib 0.3.0
android_system_properties 0.1.6
anyhow 1.0.104
async-broadcast 0.7.2
async-channel 2.5.0
async-executor 1.14.0
async-io 2.6.0
async-lock 3.4.2
async-process 2.5.0
async-recursion 1.2.0
async-signal 0.2.14
async-task 4.7.1
async-trait 0.1.92
atk 0.18.2
atk-sys 0.18.2
atomic-waker 1.1.2
base64 0.21.7
base64 0.22.1
base64 0.23.1
bit-set 0.8.0
bit-vec 0.8.0
bitflags 1.3.2
block-buffer 0.10.4
block2 0.6.2
blocking 1.7.0
brotli 9.0.0
brotli-decompressor 6.0.1
bs58 0.5.1
bytemuck 1.25.2
byteorder 1.5.0
cairo-rs 0.18.5
cairo-sys-rs 0.18.2
camino 1.2.6
cargo-platform 0.1.9
cargo_metadata 0.19.2
cargo_toml 1.0.1
cesu8 1.1.0
cfb 0.14.0
cfg-expr 0.15.8
chrono 0.4.45
cmdstudio-app 0.1.0
combine 4.6.9
concurrent-queue 2.5.0
cookie 0.18.2
core-foundation 0.10.1
core-foundation-sys 0.8.7
core-graphics 0.25.0
core-graphics-types 0.2.0
cpufeatures 0.2.17
crc32fast 1.5.2
crossbeam-channel 0.5.17
crossbeam-utils 0.8.23
crypto-common 0.1.7
cssparser 0.37.0
cssparser-macros 0.7.1
ctor 1.0.13
darling 0.24.1
darling_core 0.24.1
darling_macro 0.24.1
dbus 0.9.12
defmt 1.2.0
defmt-macros 1.2.0
defmt-parser 1.1.0
deranged 0.5.9
derive_more 2.1.1
derive_more-impl 2.1.1
digest 0.10.7
dirs 7.0.0
dirs-sys 0.5.0
dispatch2 0.3.1
displaydoc 0.2.7
dlopen2 0.8.2
dlopen2_derive 0.4.3
dom_query 0.28.0
dpi 0.1.2
dtoa 1.0.11
dtoa-short 0.3.5
dunce 1.0.5
embed-resource 3.0.12
embed_plist 1.2.2
endi 1.1.1
enumflags2 0.7.12
enumflags2_derive 0.7.12
erased-serde 0.4.10
event-listener 5.4.2
event-listener-strategy 0.5.4
fdeflate 0.3.7
field-offset 0.3.6
flate2 1.1.10
foldhash 0.2.0
foreign-types 0.5.0
foreign-types-macros 0.2.4
foreign-types-shared 0.3.1
form_urlencoded 1.2.2
futures-channel 0.3.34
futures-executor 0.3.34
futures-io 0.3.34
futures-lite 2.6.1
futures-macro 0.3.34
futures-sink 0.3.34
gdk 0.18.2
gdk-pixbuf 0.18.5
gdk-pixbuf-sys 0.18.0
gdk-sys 0.18.2
gdkwayland-sys 0.18.2
gdkx11 0.18.2
gdkx11-sys 0.18.2
generic-array 0.14.7
getrandom 0.3.4
gio 0.18.4
gio-sys 0.18.1
glib 0.18.5
glib-macros 0.18.5
glib-sys 0.18.1
glob 0.3.4
gobject-sys 0.18.0
gtk 0.18.2
gtk-sys 0.18.2
gtk3-macros 0.18.2
hashbrown 0.12.3
heck 0.4.1
heck 0.5.0
hermit-abi 0.5.3
hex 0.4.3
html5ever 0.39.0
http 1.5.0
http-body 1.1.0
http-body-util 0.1.5
httparse 1.10.1
hyper 1.12.0
hyper-util 0.1.21
iana-time-zone 0.1.65
iana-time-zone-haiku 0.1.2
ico 0.5.0
icu_collections 2.3.0
icu_locale_core 2.3.0
icu_normalizer 2.3.0
icu_normalizer_data 2.3.0
icu_properties 2.3.0
icu_properties_data 2.3.0
icu_provider 2.3.1
ident_case 1.0.1
idna 1.1.0
idna_adapter 1.2.2
indexmap 1.9.3
infer 0.22.0
ipnet 2.12.2
javascriptcore-rs 1.1.2
javascriptcore-rs-sys 1.1.1
jiff 0.2.38
jiff-core 0.1.1
jiff-static 0.2.38
jiff-tzdb 0.1.9
jiff-tzdb-platform 0.1.3
jni 0.21.1
jni-sys 0.3.1
jni-sys 0.4.1
jni-sys-macros 0.4.1
json-patch 4.2.0
jsonptr 0.7.1
keyboard-types 0.8.3
libappindicator 0.9.0
libappindicator-sys 0.9.0
libdbus-sys 0.2.7
libloading 0.7.4
libredox 0.1.25
litemap 0.8.3
log 0.4.34
markup5ever 0.39.0
memoffset 0.9.1
mime 0.3.17
miniz_oxide 0.8.9
miniz_oxide 0.9.1
mio 1.2.4
muda 0.20.0
ndk 0.9.0
ndk-context 0.1.1
ndk-sys 0.6.0+11769913
new_debug_unreachable 1.0.6
num-conv 0.2.2
num_enum 0.7.6
num_enum_derive 0.7.6
objc2 0.6.5
objc2-app-kit 0.3.2
objc2-cloud-kit 0.3.2
objc2-core-data 0.3.2
objc2-core-foundation 0.3.2
objc2-core-graphics 0.3.2
objc2-core-image 0.3.2
objc2-core-location 0.3.2
objc2-core-text 0.3.2
objc2-encode 4.1.0
objc2-exception-helper 0.1.1
objc2-foundation 0.3.2
objc2-io-surface 0.3.2
objc2-quartz-core 0.3.2
objc2-ui-kit 0.3.2
objc2-user-notifications 0.3.2
objc2-web-kit 0.3.2
option-ext 0.2.0
ordered-stream 0.2.0
pango 0.18.3
pango-sys 0.18.0
parking 2.2.1
percent-encoding 2.3.2
phf 0.13.1
phf_codegen 0.13.1
phf_generator 0.13.1
phf_macros 0.13.1
phf_shared 0.13.1
piper 0.2.5
plist 1.10.1
png 0.17.16
png 0.18.1
polling 3.11.0
portable-atomic 1.15.0
portable-atomic-util 0.2.8
potential_utf 0.1.6
powerfmt 0.2.1
precomputed-hash 0.1.1
proc-macro-crate 1.3.1
proc-macro-crate 2.0.2
proc-macro-error 1.0.4
proc-macro-error-attr 1.0.4
quick-xml 0.42.0
r-efi 5.3.0
raw-window-handle 0.6.2
redox_users 0.5.3
regex 1.13.1
regex-automata 0.4.18
reqwest 0.13.5
rfd 0.17.2
rustc_version 0.4.1
same-file 1.0.6
schemars 0.8.22
schemars 0.9.0
schemars_derive 0.8.22
selectors 0.38.0
semver 1.0.28
serde-untagged 0.1.9
serde_derive_internals 0.29.1
serde_repr 0.1.21
serde_spanned 0.6.9
serde_spanned 1.1.2
serde_with 3.24.0
serde_with_macros 3.24.0
serialize-to-javascript 0.1.2
serialize-to-javascript-impl 0.1.2
servo_arc 0.4.3
sha2 0.10.9
signal-hook-registry 1.4.8
simd-adler32 0.3.10
siphasher 1.0.4
socket2 0.6.5
softbuffer 0.4.8
soup3 0.5.0
soup3-sys 0.5.0
stable_deref_trait 1.2.1
static_vcruntime 3.0.0
string_cache 0.9.0
string_cache_codegen 0.6.1
strsim 0.11.1
swift-rs 1.0.9
syn 1.0.109
syn 2.0.119
sync_wrapper 1.0.2
synstructure 0.14.0
system-deps 6.2.2
tao 0.37.1
tao-macros 0.1.4
target-lexicon 0.12.16
tauri 2.12.4
tauri-build 2.7.3
tauri-codegen 2.7.3
tauri-macros 2.7.3
tauri-plugin-single-instance 2.5.4
tauri-runtime 2.12.2
tauri-runtime-wry 2.12.2
tauri-utils 2.10.2
tauri-winres 0.3.6
tendril 0.5.1
termcolor 1.4.1
thiserror 1.0.69
thiserror-impl 1.0.69
time 0.3.55
time-core 0.1.9
time-macros 0.2.32
tinystr 0.8.4
tinyvec 1.13.3
tokio 1.53.3
tokio-util 0.7.20
toml 0.8.2
toml 1.1.8+spec-1.1.0
toml_datetime 0.6.3
toml_edit 0.19.15
toml_edit 0.20.2
toml_writer 1.1.3+spec-1.1.0
tower 0.5.3
tower-http 0.6.11
tower-layer 0.3.3
tower-service 0.3.3
tracing 0.1.44
tracing-attributes 0.1.31
tracing-core 0.1.36
tray-icon 0.25.1
try-lock 0.2.5
ts-rs 12.0.1
ts-rs-macros 12.0.1
typeid 1.0.3
uds_windows 1.2.1
unicode-segmentation 1.13.3
url 2.5.8
urlpattern 0.6.0
utf8_iter 1.0.4
version-compare 0.2.1
version_check 0.9.5
vswhom 0.1.0
vswhom-sys 0.1.3
walkdir 2.5.0
want 0.3.2
wasi 0.11.1+wasi-snapshot-preview1
wasip2 1.0.4+wasi-0.2.12
wasm-bindgen-futures 0.4.79
wasm-streams 0.5.0
web-sys 0.3.106
web-time 1.1.0
web_atoms 0.2.6
webkit2gtk 2.0.2
webkit2gtk-sys 2.0.2
webview2-com 0.39.1
webview2-com-macros 0.8.1
webview2-com-sys 0.39.1
winapi 0.3.9
winapi-i686-pc-windows-gnu 0.4.0
winapi-util 0.1.11
winapi-x86_64-pc-windows-gnu 0.4.0
window-vibrancy 0.8.1
windows 0.62.2
windows-collections 0.3.2
windows-core 0.62.2
windows-future 0.3.2
windows-implement 0.60.2
windows-interface 0.59.3
windows-numerics 0.3.1
windows-result 0.4.1
windows-strings 0.5.1
windows-sys 0.45.0
windows-targets 0.42.2
windows-threading 0.2.1
windows-version 0.1.7
windows_aarch64_gnullvm 0.42.2
windows_aarch64_msvc 0.42.2
windows_i686_gnu 0.42.2
windows_i686_msvc 0.42.2
windows_x86_64_gnu 0.42.2
windows_x86_64_gnullvm 0.42.2
windows_x86_64_msvc 0.42.2
winnow 0.5.40
wit-bindgen 0.57.1
writeable 0.6.4
wry 0.57.0
x11 2.21.0
x11-dl 2.21.0
yoke 0.8.3
yoke-derive 0.8.4
zbus 5.19.0
zbus_macros 5.19.0
zbus_names 4.3.4
zcheapstr 1.1.0
zerofrom 0.1.8
zerofrom-derive 0.1.8
zerotrie 0.2.5
zerovec 0.11.8
zerovec-derive 0.11.6
zlib-rs 0.6.8
zvariant 5.15.0
zvariant_derive 5.15.0
zvariant_utils 4.2.0
```

## Appendix B: a check crate for a machine where the Tauri build is blocked

When Application Control blocks the app crate's build on the dev PC twice, the non-Tauri modules can still be compiled and tested through this crate, which includes the same source files by `#[path]` with the same dependency pins. Create it at `<repo>\p2b-check` (ignored by `.gitignore`, never committed), run `cargo test` and `cargo clippy --all-targets -- -D warnings` inside it with your own `CARGO_TARGET_DIR`. The expected counts are the ones in Tasks 6–14 (the whole crate: 95). `ipc.rs`, `lib.rs`, `machine.rs` and `main.rs` are not in it: CI compiles them.

`p2b-check/Cargo.toml`:

```toml
[package]
name = "p2b-check"
version = "0.1.0"
edition = "2021"
publish = false

[workspace]

[lib]
path = "src/lib.rs"

[dependencies]
cmdstudio-core = { path = "../crates/core", features = ["fake", "ts"] }
cmdstudio-cli = { path = "../crates/cli" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }
sha2 = "0.11"
ts-rs = { version = "=12.0.1", features = ["uuid-impl", "serde-json-impl", "no-serde-warnings"] }

[target.'cfg(windows)'.dependencies]
windows-sys = { version = "=0.61.2", features = ["Win32_Foundation", "Win32_System_Console", "Win32_System_Threading", "Win32_UI_Shell", "Win32_UI_WindowsAndMessaging"] }

[lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
indexing_slicing = "deny"
string_slice = "deny"
undocumented_unsafe_blocks = "deny"
```

`p2b-check/src/lib.rs`:

```rust
//! The app crate's modules that do not need Tauri, compiled and tested here because Windows
//! Application Control blocked serde's build script for the Tauri feature set (os error 4551).
#![allow(dead_code)]
#[path = "../../app/src-tauri/src/backend/mod.rs"]
pub mod backend;
#[path = "../../app/src-tauri/src/command_names.rs"]
pub mod command_names;
#[path = "../../app/src-tauri/src/console.rs"]
pub mod console;
#[path = "../../app/src-tauri/src/error.rs"]
pub mod error;
#[path = "../../app/src-tauri/src/library.rs"]
pub mod library;
#[path = "../../app/src-tauri/src/logs.rs"]
pub mod logs;
#[path = "../../app/src-tauri/src/open.rs"]
pub mod open;
#[path = "../../app/src-tauri/src/settings.rs"]
pub mod settings;
```

To export the app's bindings from here: `$env:TS_RS_EXPORT_DIR = "<repo>\app\ui\src\bindings"; cargo test -- export_bindings` inside `p2b-check`, then `Remove-Item Env:TS_RS_EXPORT_DIR`.
