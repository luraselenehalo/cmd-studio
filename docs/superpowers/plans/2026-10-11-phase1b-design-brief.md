# Phase 1b design brief (controller decisions for the plan writer)

Status: draft by the controller on 2026-10-11, to be expanded into
`docs/superpowers/plans/2026-10-11-phase1b-system.md` with superpowers:writing-plans.
Spec: `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (Thai; binding).
Builds on Phase 1a (`crates/core`: color, oklch, palette, text, model, schema, presets,
generate::{omp,lua,clink,wt}, plan, txn). Read `crates/core/src/plan.rs` and `txn.rs`
as shipped (source is ground truth over the 1a plan text).

## Goal (spec §8 Phase 1 done-criterion)
CI integration test passes, and on VM Windows 11 + Windows 10: `cmdstudio apply` →
`cmdstudio undo` leaves registry, WT settings.json and fonts byte-identical.
Deliver: Windows adapters, detect, doctor, on-disk snapshot store, real txn
Executor/Verifier/JournalStore, runtime resources pipeline, CLI.

## Crates
- `crates/core` (cmdstudio-core) keeps the pure modules pure. New:
  - `adapters/mod.rs`: traits `WtSettings`, `AutoRun`, `Fonts`, `Fs` (+ `Process` for
    running our own bundled exes with fixed args, spec §5.10 last bullet), pure.
  - `adapters/fake.rs`: in-memory impls used by unit tests (cfg(test) or a `fake`
    feature so crates/cli tests can use them too).
  - `adapters/windows/*.rs` behind `#[cfg(windows)]`: registry (winreg: keep REG_SZ vs
    REG_EXPAND_SZ via raw values; open the existing key, never recreate it — lesson 9),
    fonts (copy to `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, HKCU Fonts value,
    AddFontResourceW/RemoveFontResourceW, broadcast WM_FONTCHANGE with
    SendMessageTimeoutW to HWND_BROADCAST; G12), short path (GetShortPathNameW),
    ProgramData folder (see below), atomic file writes (temp in same dir → flush →
    rename/ReplaceFileW), font family enumeration (EnumFontFamiliesExW, GDI names).
  - `state.rs` (state.json model, pure serde), `snapshot.rs` (store over `Fs`),
    `detect.rs`, `doctor.rs`, `exec.rs` (real txn Executor/Verifier/JournalStore over
    the adapter traits; pure except through traits).
- `crates/cli` (cmdstudio-cli): lib `run(args, io) -> i32` + bin `cmdstudio`. Phase 2's
  Tauri app links the lib and calls `run` when started with CLI args (spec §5.7: one exe).

## The `plan::Machine` facts detect must fill (from shipped plan.rs)
wt_settings (Store `%LOCALAPPDATA%\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json`,
Preview `...WindowsTerminalPreview_8wekyb3d8bbwe...`, unpackaged
`%LOCALAPPDATA%\Microsoft\Windows Terminal\settings.json`; only files that exist),
autorun_hkcu, autorun_hklm, fonts (GDI family names), data_short (GetShortPathNameW of
the data dir, which detect creates first if missing), program_data_dir:
`%ProgramData%\ai.resincore.cmdstudio-<SID>`; Some only if absent or owner ∈ {user,
SYSTEM, Administrators} and no other SID has write/append/delete rights (spec §5.4 step 3).
Creating it: CreateDirectoryW with an SDDL security descriptor `D:P(A;OICI;FA;;;<SID>)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)`
(protected, no inheritance); if it already exists at create time, re-check ownership and
ACL before writing (TOCTOU), else E_CLINK_PATH.
Also a `SystemInfo` for the wizard: Windows build (≥19041 for WT), arch (x64 only;
ARM64 → unsupported), default terminal (Win11 delegation keys, info only), WebView2 presence (Phase 4 can use it).

## Executor semantics per Op (spec §5.4, §5.5, doctrine guards §7)
Capture = prior state bytes recorded in the journal before apply; restore must be idempotent.
- InstallRuntime{component, dir}: copy the bundled runtime folder for that component
  into `dir` (new versioned folder; never overwrite an existing different folder — if it
  exists with identical files, no-op). Capture = "dir existed?"; restore deletes the dir
  only if we created it.
- WriteFile{path, contents}: atomic write; capture prior bytes or absent; restore writes
  them back or deletes.
- WtApply{settings, scheme, profile, set_default}: edit settings.json with jsonc-parser
  0.34.0 CST (G9): upsert scheme named "CMD Studio" in `schemes`; ensure the cmd profile
  guid `{0caa0dad-35be-5f56-a8ff-afceeeaa6101}` exists in `profiles.list` (create it with
  name "Command Prompt" and the given keys if absent); set each profile key; set
  `defaultProfile` when set_default. Write `serde_json::Value → CstInputValue` converter
  (G9). Unparseable file → E_WT_PARSE and don't touch it. Capture = whole-file bytes
  (txn rollback is byte-identical; doctrine guard ง).
- SetAutoRun{value}: HKCU `Software\Microsoft\Command Processor`, value `AutoRun`; keep
  the existing value kind (REG_SZ/REG_EXPAND_SZ), new value REG_SZ; None deletes the
  value. Capture = {kind, data} or absent (serialize as JSON bytes).
- InstallFont{family}: per-user install of the bundled font file (G12). Capture =
  installed-before?; restore uninstalls only if we installed it.
Baseline (for "revert all" and uninstall cleanup, spec §6.3 steps 1–3): the first time we
touch something, record its original in state.json: AutoRun original (kind+data or
absent), per WT file the original value of every key we touch (absent → delete on
revert), the fonts we installed. revert_all = remove only our AutoRun part (others
verbatim), restore WT keys from the baseline and delete our scheme, uninstall our fonts —
key-level, so later user edits survive.

## Verifier (spec §5.5, 4 checks)
1 read back every written value; 2 `oh-my-posh print primary --config <generated> --shell
cmd` succeeds (run the bundled exe only); 3 `<clink-launch> --profile <p> set clink.logo`
output has no line matching `\.lua:\d+:` (G5; exit code is always 0); 4 every written
settings.json parses. Failures map to E_VERIFY_READBACK / E_VERIFY_OMP / E_VERIFY_CLINK / E_WT_PARSE.

## Snapshot store
`<data>\snapshots\<id>\manifest.json` (id, created, theme, target, ops summary),
`journal.json` (txn::Journal, camelCase), `payload\<op_index>.bin` for prior bytes
(journal references them). Keep the 20 newest plus the first-ever forever (spec §5.5).
Recovery at startup: unfinished journal → txn::recover → report (spec §5.5).

## Doctor D1–D11 (spec §5.8)
Each check returns {id, status: ok|warn|fail|skipped, code, message_th, message_en,
details}; "ok" only from a real check (COE). D3 includes the clink.log scan for
`Unable to get DLL version` / `Failed to copy DLL` (G19). D8 counts GDI fonts only for
v1 (WT-packaged fonts: record as a known limitation; the planner falls back anyway).

## CLI (spec §5.7)
`cmdstudio apply <file.cmdtheme> [--target all|wt]`, `cmdstudio doctor [--json]`,
`cmdstudio undo`, `cmdstudio --cleanup [--remove-themes]`; exit 0 ok / 1 failed and
rolled back / 2 invalid input / 3 rollback incomplete. `--data-dir <dir>` and
`--resources <dir>` overrides for tests (default data dir `%LOCALAPPDATA%\ai.resincore.cmdstudio`,
default resources `<exe dir>\resources`). Print the plan summary (en) before applying.

## Resources pipeline
`scripts/fetch-resources.ps1` downloads pinned files, checks SHA256, fills `resources/`
(gitignored): oh-my-posh 31.5.0 `posh-windows-amd64.exe` → `resources/omp/oh-my-posh.exe`;
Clink 1.9.34 release zip → x64 files into `resources/clink/`; Nerd Fonts v3.5.1
`CascadiaCode.zip` (SHA256 1298BF92698AFA06185CF1D05E6AE05F2D8A1E8C3CB45DDF4C3035168AB342A1)
→ the TTF whose GDI family is `CaskaydiaCove NF` into `resources/fonts/`; plus
`resources/LICENSES/` (OMP MIT, Clink GPL-3 + source tag link, OFL 1.1). Record every
URL+hash in the script; read `spikes/windows/fonts.ps1` for the font file name.

## Test safety (non-negotiable)
Tests must never change the developer's real AutoRun, WT settings or fonts. Adapters take
their locations as parameters (registry subkey path, settings paths, fonts dir/registry
key) so tests use a throwaway HKCU subkey (`Software\CmdStudioTest\<uuid>`, deleted after)
and temp dirs. Tests that must touch the real per-user locations (real font install,
real `Command Processor` key) run only when `CMDSTUDIO_SYSTEM_TESTS=1` (set in CI and VMs).

## CI
`.github/workflows/ci.yml` on push/PR, windows-latest: fetch resources (cached), `cargo fmt
--check`, `cargo clippy --all-targets -D warnings`, `cargo test --workspace`, then the
system tests with CMDSTUDIO_SYSTEM_TESTS=1 (apply → doctor → undo → byte-identical,
against a fake WT dir and the runner's real HKCU).

## VM check (controller, after the plan's tasks)
Copy the built `cmdstudio.exe` + resources into VM-A, VM-B, VM-C; run apply → doctor →
undo; compare registry export, settings.json bytes and font list before/after; save the
evidence under spikes/vm/tmp; restore each VM to `clean`.
