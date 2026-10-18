# Phase 3 VM check — results

Runs of `protocol.md` on 2026-10-17 and 2026-10-18 (host clock, UTC+7) by the controller's VM runner. Step ids are the protocol's (`R2.7` is Run 2 step 7). Screenshots are in `spikes\vm\tmp\` (git-ignored) as `<vm>-<label>.png`, for example `cmdstudio-win11-r2-wt.png`; they were not copied into `docs\` (the run brief overrides the protocol's "Results" table on this). Logs of every step are in the controller's scratchpad, `vmrun\`.

## The build

- **CI run 39038662772**, artifact `cmdstudio-exe`, commit **`04e36f6089ab05a29ea938a8c9e16f8bd449d55b`** (head of `phase3-ui`, PR #9). `cmdstudio.exe` sha256 `C7A915A793450637359A992B4D149625AD8EA5B9736A7B1AC7EEB7177B647D89`, 9 files under `resources\`. Every `Install-Phase3App` printed `same exe: True` and 9 files on both sides.
- **The diff check.** `git diff --stat 04e36f6 phase3-ui -- app crates presets schema scripts Cargo.toml Cargo.lock` prints nothing. During the run the working tree's HEAD had moved to `phase3b-1` (`ff2749d`, six commits after `04e36f6`, 15 files under `crates/`); `git diff --stat 04e36f6 HEAD -- docs/spikes/phase3-vm-check spikes/vm` prints nothing, so the protocol, `helpers.ps1`, `vm.ps1` and the guest scripts that went into the VMs are those of `04e36f6`.

## How the VMs could be run (read this first)

Three things decided which runs were possible and how. None of them is about the app.

1. **The host was locked from about 23:48 to the end** (`LogonUI` running). VM-A's and VM-C's `clean` snapshots hold a saved state; restoring it while the host is locked fails: `Shared Clipboard: Failed to open clipboard, rc=VERR_ACCESS_DENIED` → `Failed to load unit 'HGCM' (VERR_ACCESS_DENIED)`, and the VM is left `aborted-saved` (VBox.log of 23:48). VM-B's `clean` has no saved state; it cold-boots, and guestcontrol works with the host locked.
2. **VM-A's normal path** (restore of the saved state) bugchecks within 2 s of the resume (0x0A at 22:33, 0xBE at 23:07: `GIMHv: BugCheck`, `Reset initiated through MSR`) and cold-boots from the snapshot's disk. Step 0 at 22:36 and Run 1 ran on that path.
3. **VM-A's cold-boot path**, used from 01:53 while the host was locked: `snapshot restore clean`, `discardstate`, `startvm`. The `clean` snapshot is unchanged. On this path Windows finishes its setup again at boot ("Installing 0…100 %", two resets; System log 7045 shows the Guest Additions services installed during the boot, so VBoxService is stopped after it and was started by hand through an elevated PowerShell typed on the VM: Win+R, Ctrl+Shift+Enter, UAC Yes, `Start-Service VBoxService`). The machine SID is new on each such boot, and **`tester`'s Windows Terminal is never-opened there** (`LocalState` empty, no `settings.json`), while the normal path shows it opened. OS 26H2 26300.9457 and Windows Terminal 1.24.11911.0 on both paths. Runs 2, 3, 4 and 5 and the fresh-user check ran on this path.

What follows from it: **Run 2 ran on a never-opened Terminal (`neverOpened`), not on the `present` one that Step 0 recorded for the normal path**, and the gate is judged on that state below; the `present` path of the gate was not run. VM-C's Run 6 could not be run (it needs its saved state, so an unlocked host).

## Step 0

| VM, path | Package folder | `LocalState` | `settings.json` | Preview / unpackaged | `wt.exe` alias | WT version | `page 2 state` | `FINDING:` |
|---|---|---|---|---|---|---|---|---|
| VM-A normal path (22:36, controller) | yes | yes: settings.json 1533 B, state.json 162 B | yes | no / no | yes | 1.24.11911.0 | `present` | none |
| VM-A cold-boot path (03:12, Run 2 step 1) | yes | yes, empty | no | no / no | yes | 1.24.11911.0 | `neverOpened` | none |
| VM-B `tester` (00:01) | no | no | no | no / no | no | none registered | not installed as the app reads it | none |
| VM-C `ผู้ทดสอบ ไทย` (23:06, after the fix below) | yes | yes: settings.json 1533 B, state.json 162 B | yes | no / no | yes | 1.24.11911.0 | `present` | none |
| VM-A new user `fresh` (02:19:42, 20 s after its first desktop) | yes | yes, empty | no | no / no | yes | 1.24.11911.0 | `neverOpened` | none |
| VM-A new user `fresh2` (02:30:03, first sign-in still provisioning) | no | no | no | no / no | no | registered for the machine, HKCU AppModel none | not installed as the app reads it | none |
| VM-A `fresh2` (02:30:40 to 02:33:43, every ~20 s) | yes | yes, empty | no | — | yes | — | (`neverOpened` by the same rule) | none |
| VM-A new user `fresh3` (02:38:44, 8 s after sign-in was reported) | yes | yes, empty | no | no / no | yes | 1.24.11911.0 | `neverOpened` | none |

The full Step 0 files are in the scratchpad logs (`step0-vma.log`, `step0-vmc.log`, `step0-vmb.log`, `step0-fresh-1.log`, `step0-fresh2.log`, `step0-fresh3.log`, `r2-step1.log`). The registered package is `Microsoft.WindowsTerminal_3001.24.11911.0_neutral_~_8wekyb3d8bbwe` on VM-A and VM-C; HKCU AppModel `Microsoft.WindowsTerminal_1.24.11911.0_x64__8wekyb3d8bbwe`.

**The never-opened question (finding I4).** For three new local users on VM-A the Store package folder `Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe` existed, with an empty `LocalState` and no `settings.json`, before Terminal's first start; the app read them as `neverOpened` and page 2 said so. For `fresh2`, one reading 40 s after the restart (its first sign-in still provisioning apps) had neither the folder nor the `wt.exe` alias nor the HKCU registration; 37 s later all three were there. In no reading was the alias there without the folder, so the protocol's `FINDING:` case did not occur and I4 is not reproduced: `installed: dir.is_dir()` in `crates/core/src/adapters/windows/wt.rs` holds for a fresh user once Windows has registered Terminal for them.

## The Tab counts that held

All from a page's heading unless said otherwise; read with `Get-GuestFocus` in Run 1 and used unchanged in Runs 2, 3, 5 and R2-NO.

| Where | Count | Reaches |
|---|---|---|
| Page 1, window just opened | Tab 1 / 2 / 3 / 4 | English / ไทย / Skip / Next |
| Page 1, Run 6's path | Tab 2, Space, Tab 2 | ไทย, then Next ("ถัดไป") |
| Page 2 (Windows 11) | Tab 3 | Next (after Back, Skip) |
| Page 2 (Windows 10, with the Install button) | Tab 4 | Next |
| Page 3 | Tab 10 | Next (6 cards, See all 12 themes, Back, Skip) |
| Page 4 | Tab 5 | Next (radios one stop, checkbox, Back, Skip) |
| Page 5 | Tab 2 | Apply (after "Steps (n)") |
| Page 6 | Tab 1 / Tab 2 | Try cmd / Go to the home page |
| Home | ShiftTab 1–5, 6, 7 | Advanced, Help, Settings, Themes, Home, Dismiss, Undo |
| Settings | Tab 4 (theme applied), Tab 3 (none) | the language radios |
| Help | Tab 1–6 | Health check, Restore points, Undo everything, Report a problem, Short guide, Credits and licenses |
| Help → Report a problem, from the tab | Tab 3 / Tab 4 | Copy details / Open GitHub Issues |
| Advanced | Tab 1 / Tab 2 | Open the data folder / Open the log folder |
| Themes | Tab 1–3, 4, 5 | All, Dark, Light, Create a theme, Import a theme file |
| Editor (Customize, advanced mode) | Tab 1 | Back to themes |
| Editor (Create a theme, simple mode) | Tab 8, then Down | the tone radio "Dark", then "Light" |

Where they do not hold: in the editor, the toolbar's Undo and Redo become Tab stops once something was edited, so a count back from a field changes after the first edit (R2.14). At Text size 150 % the preview on page 6 gets a horizontal scrollbar and Chromium makes it a Tab stop: Tab 2 from the heading then reaches "Try cmd", not "Go to the home page" (R5.3). After a Win+R in the guest (`Get-GuestAutoRun`, `Get-ExitCode`, `Send-Run`), the app window can be in front while the page gets no keys; a second start of the app (check 11) gives it the focus back, and the runs below did that before each key walk.

## Run 1 — VM-A, normal path (2026-10-17 23:07–23:47)

| Step | Result | What decided it | Screenshot |
|---|---|---|---|
| R1.2 | PASS | window in 10 s, page 1 English "Step 1 of 6"; `Get-GuestWindows`: cmdstudio "CMD Studio", 0 console hosts with a window, 0 WindowsTerminal; debug port `page \| CMD Studio \| http://tauri.localhost/` | r1-p1 |
| R1.3 page 1 (check 14) | PASS | ไทย, Space: "ขั้นที่ 1 จาก 6", ไทย pressed (accent border, bold); English again, English pressed | r1-p1-th, r1-p1-en |
| R1.3 page 2 | PASS | "Windows build 26300", "Windows Terminal is installed: your cmd windows get colors", "No other program sets up cmd", "No administrator rights are needed" | r1-p2 |
| R1.3 page 3 | PASS | Terracotta selected; See all → 12 theme buttons (UIA list) and "Show the recommended 6"; back to 6 | r1-p3, r1-p3-all |
| R1.3 page 4 | PASS | Down → "Only in Windows Terminal" selected (UIA IsSelected), the default-profile box On | r1-p4-wt |
| R1.3 page 5 | PASS | plan: Update the prompt and the welcome box / Colors and font of cmd in Windows Terminal / Open Windows Terminal with cmd; no "Use this theme in every cmd window"; progress "Applying (1 of 8)", "Checking the result" seen, "Saving a restore point" not caught | r1-p5, r1-apply-01 |
| R1.3 page 6 | PASS | "Your theme is applied. Open a new cmd tab in Windows Terminal to see the welcome box and the prompt."; Try cmd, Go to the home page | r1-p6 |
| R1.4 | PASS | `"target": "wt"`, `"wizardDone": true`; AutoRun `(absent)`; cmd profile `commandline` `%SystemRoot%\System32\cmd.exe /k ""…\runtime\clink-1.9.34\clink_x64.exe" inject --quiet --profile "…\clink-profile""` | — |
| R1.5 | PASS | `#1F1E1D` near: r1-p6 158,893 → r1-wt 490,409 (rise 331,516); one cmd.exe with `clink_dll_x64.dll` | r1-wt |
| R1.6 | done | every screen and dialog opened and closed (filters, Customize, Rename, Delete, Import cancelled → "No file was chosen.", Settings, six Help tabs with "Go back to this point" and the Undo-everything confirmation, Advanced); counts in the table above; Undo in the bar → "Undone: everything is as it was before.", Settings count 3 | r1-filter-*, r1-rename, r1-delete, r1-import-dialog, r1-revert-confirm, r1-restore-confirm, r1-undone |
| R1.7 (check 1) | PASS | `Runtime.enable and Log.enable answered: True`; first 10 s: 0 messages, 0 with "Content Security Policy" or "Refused to"; controls 1–4 True; `replay proven … True`; 0 non-JSON | r1-console |
| R1.8 | PASS | app's WebView2 processes gone; the msedgewebview2 left belong to SearchHost.exe (5) and Widgets.exe (5) | r1-closed |

## Run 2 — VM-A, the gate, cold-boot path, `tester`, Terminal never-opened (2026-10-18 02:57–03:55)

Step 0 of this boot said `neverOpened` (see "How the VMs could be run"), so steps 1–5 were console-free and the Undo was judged on 16a.

| Step | Result | What decided it | Screenshot |
|---|---|---|---|
| R2.1 | recorded | Step 0 again: `neverOpened` (the normal path's file says `present`: a property of the boot path, not of the tooling); `$before`: WT folder True, LocalState True, settings.json absent; `$autoBefore` `AutoRun: (absent)`; data folder False | — |
| R2.2 | PASS | cmdstudio + msedgewebview2; no WindowsTerminal, OpenConsole or cmd; no message box | r2-p1 |
| R2.3 (check 8) | PASS | data folder: `logs`, `webview`; `webview\EBWebView`; no EBWebView at the top | — |
| R2.4 (gate) | PASS | page 2 "Windows Terminal is installed and was never opened: CMD Studio sets it up when the theme is applied"; page 5: Update the prompt and the welcome box / Set up Windows Terminal's settings so cmd gets this theme / Colors and font of cmd in Windows Terminal / Open Windows Terminal with cmd / Use this theme in every cmd window, no warning; progress "Applying (1 of 9)", "Checking the result"; page 6 "Your theme is applied. Open a new cmd window to see the welcome box and the prompt.", Try cmd, Go to the home page | r2-p2, r2-p5, r2-apply-01, r2-apply-02, r2-p6 |
| R2.5 tally | PASS | **5 activations, 0 characters, 24 navigation keys** (0 other keys) | — |
| R2.5 AutoRun | PASS | `AutoRun: [REG_SZ] (if exist "…\runtime\clink-1.9.34\clink_x64.exe" "…\clink_x64.exe" inject --autorun --quiet --profile "…\clink-profile")` | — |
| R2.5 timing (check 3) | PASS | 41.8 s from the process start to `settings.json` (not flagged: under 60) | — |
| R2.5 settings, apply | PASS | `wizardDone` true, `language` en, `target` all, `setWtDefaultProfile` true; state.json Terracotta, `launch` direct; log `cmd=apply ms=1295 ok` | — |
| R2.5 Terminal's file | PASS | CMD Studio's 1515-byte file: `$schema`, scheme "CMD Studio" `#1f1e1d`, profile `{0caa0dad-35be-5f56-a8ff-afceeeaa6101}` with `"colorScheme": "CMD Studio"`, `defaultProfile` that guid; `"fromTemplate": true` | — |
| R2.6 (check 5) | PASS | `#0C0C0C` 0 in the base and in r2-try-1/2/3; one WindowsTerminal, 0 console hosts with a window | r2-before-try, r2-try-1..3 |
| R2.6 colors | PASS | `#1F1E1D` near 158,908 → 587,707 (rise 428,799), exact 586,063 | r2-wt |
| R2.6 first start | PASS | after Terminal's first start `settings.json` (2180 B) still holds the scheme and the cmd profile's `colorScheme`; Terminal added its generated profiles (Windows PowerShell, Azure Cloud Shell) and `$help`, `actions`, `keybindings`, `newTabMenu`, `themes`; `state.json` (206 B) appeared | — |
| R2.7 (gate) | PASS | one cmd.exe, `Clink loaded: clink_dll_x64.dll`; `#5F5D58` 1 → 1,851 (rise 1,850); `#D97757` 766 → 2,767 (rise 2,001); Terracotta box and prompt, `✻` in the box and on the tab | r2-wt |
| R2.8 (gate) | PASS | one cmd.exe with Clink; `#5F5D58` rise 1,739; `#D97757` rise 2,230; `#1F1E1D` near 158,907 → 490,323 (rise 331,416). The box shows `*`, not `✻` (finding F3) | r2-before-second, r2-second |
| R2.9 | PASS | Status "Everything looks fine" (doctor.txt: 0 warn, 0 FAIL); Terracotta and its preview; Customize / Change theme / Open cmd; "Switch quickly" "Themes you apply show up here."; "Latest restore point" "Terracotta · 18 Oct 2026, 03:14"; bar `Theme "Terracotta" applied · open a new cmd window to see the welcome box and the prompt` with Undo, Dismiss; D3 ok | r2-home, r2-home-2 |
| R2.10 (check 11) | PASS | after Win+D and a second start: one cmdstudio.exe, foreground cmdstudio, not minimized, one "CMD Studio" window | r2-step10 |
| R2.11 open question 1 | recorded | a typed `doctor` (no redirect): the prompt comes back first, and the report prints after it, on the new prompt's input line | r2-doctor-typed |
| R2.11 (check 4) | PASS | doctor.txt: D1–D11 all `ok`, complete when read 5 s after Enter; help.txt: Usage, the commands, Exit codes | — |
| R2.11 (check 2) | PASS | in cmd: `help=0`, `doctor=0`, `bad=2`; `exit-code.vbs`: help `exit=0`, bad `exit=2`, doctor `exit=0`, doctor-json `exit=0` | — |
| R2.12 (check 12) | PASS | 2,523 characters; `%USERNAME% present: True`; `user name present: False`; `8.3 name … present: False`; `any account SID present: False`; `<SID> present: False` (it names no SID); "Copied. Now open GitHub Issues and paste them."; the paste into Notepad works | r2-copied, r2-notepad |
| R2.13 (check 6) | PASS | `msedge.exe --single-argument https://github.com/luraselenehalo/cmd-studio/issues`, title "Issues · luraselenehalo/cmd-studio · GitHub"; Explorer at the data folder and its `logs` | r2-github, r2-folders |
| R2.14 (checks 9, 14) | PASS | filters pressed visibly; draft `"tone": "light"`, card under "Unsaved drafts" with "Draft" (and "Light"), its Edit opens it; "My Terracotta" under "My themes"; Export → Save As → "Saved to C:\cmdstudio\out\phase3\export-test.cmdtheme", file `"schemaVersion": 1`, `"name": "My Terracotta"`; Import → "Import this theme?" with preview and "You already have this theme; the import is added as a separate theme." → Add → `themes\` holds 2 files; Esc on a second Open → "No file was chosen." | r2-filter-light, r2-filter-all, r2-draft-edit, r2-export-dialog, r2-import-dialog, r2-import-cancel |
| R2.15 (check 10) | PASS, with finding F1 | `"language": "th"`; Thai home page (หน้าแรก, ธีมที่ใช้อยู่); "เปิด cmd" opens a Terminal tab with the English box and tips, Clink loaded; generated files: Thai letters False ×3; doctor.txt with the Thai UI: Thai False; back to English. The Thai home page's preview is not monospace (F1) | r2-home-th, r2-th-cmd |
| R2.16a (gate) | PASS | Terminal open (Open cmd from Home); Undo pressed by name: **committed**, "Undone: everything is as it was before."; no `E_VERIFY_READBACK`; `settings.json` at once, 10 s later and after Terminal closed: **Terminal's defaults** (1533 B, "CMD Studio" 0 times: default profile Windows PowerShell, a plain Command Prompt profile, `schemes: []`); state.json gone | r2-undo-wt, r2-undo-open |
| R2.16 | PASS | `$autoAfter -eq $autoBefore` True, both `(absent)`, and R2.5 read a value; `Compare-Object`: only the `WT settings.json` line (absent → Terminal's own 1533-byte file); fonts: none listed on page 5, no registration after; Restore points: the apply with "Undone", no "Go back to this point"; Home "No theme is applied yet."; a new cmd window: `Clink loaded: no`, `#5F5D58` rise 0, `#D97757` rise −229, `#1F1E1D` near rise −1,608 | r2-restore-points, r2-plain |
| R2.17 | PASS | Thai after a restart (r2-restart-th), English after the next (r2-restart-en) | r2-restart-th, r2-restart-en |
| R2.18 (checks 2, 8) | PASS | data before: clink-profile, drafts, generated, logs, snapshots, themes, webview, cmdstudio.lock, settings.json; `--cleanup` `exit=0`; only `themes\` left (the two themes); AutoRun `(absent)` | — |

## The never-opened runs (VM-A cold-boot path, new users)

### R2-NO — Run 2 steps 1–8, 16a and 16 for `fresh3` (02:40–02:55)

Run 2's never-opened path once more, on a user whose first sign-in was a minute old. Steps 9–15, 17 and 18 were not repeated (Run 2 above covers them).

| Step | Result | What decided it |
|---|---|---|
| 1–3 | PASS | Step 0 `neverOpened`; `AutoRun: (absent)`; only cmdstudio; `webview\EBWebView` |
| 4–5 | PASS | 5 activations, 0 characters, 24 navigation keys; 40.7 s; AutoRun `[REG_SZ]` with CMD Studio's entry; CMD Studio's 1515-byte file, `fromTemplate` true |
| 6 | PASS | `#0C0C0C` 1 in every early shot; one WindowsTerminal; near `#1F1E1D` 158,761 → 587,711 (rise 428,950); after the first start the file keeps the scheme and the profile |
| 7 | PASS | one cmd.exe with Clink; frame rise 1,850, accent rise 1,830 (r2no-wt) |
| 8 | PASS | one cmd.exe with Clink; frame rise 1,739, accent rise 2,230, near rise 331,404 (r2no-second) |
| 16a | PASS | committed, "Undone: everything is as it was before.", no `E_VERIFY_READBACK`; the file at all three readings: Terminal's defaults (1533 B, "CMD Studio" 0) |
| 16 | PASS | AutoRun equal; only the Terminal file line differs; Restore points "Undone"; plain cmd window (Clink no, rises 0, near −1,385) |

### Run 3 — `fresh`, undone before Terminal opens (02:19–02:26)

| Step | Result | What decided it | Screenshot |
|---|---|---|---|
| R3.1 | recorded | Step 0 `neverOpened` (LocalState existed, empty); `$before` settings.json absent; `AutoRun: (absent)` | — |
| R3.2 | PASS | five clicks: 5 activations, 0 characters, 24 navigation keys; page 2 never-opened text; page 5 lists "Set up Windows Terminal's settings so cmd gets this theme", no warning; Go to the home page | r3-p2, r3-p5, r3-home |
| R3.3 | PASS | CMD Studio's 1515-byte file; `"fromTemplate": true`; AutoRun `[REG_SZ]` with CMD Studio's entry; 31.9 s | — |
| R3.4 | done | the app was brought forward by a second start (console-free) after the AutoRun read; ShiftTab 7 showed the focus ring on Undo; Enter | r3-undo-focused |
| R3.5 | PASS | `Compare-Object` prints nothing (the file is gone; `LocalState`, which existed, stays); `$autoAfter -eq $autoBefore` True; no WindowsTerminal | r3-undone |
| R3.6 | PASS | `wt` starts (WindowsTerminal, OpenConsole); `#1F1E1D` near rise −1,087; `settings.json present, CMD Studio occurs 0 time(s)` (1533 B); Campbell `#0C0C0C` 456,601 | r3-wt-default |

### Run 4 — `fresh2`, Terminal's first start with a pre-written file (02:34)

`run.ps1` was not started: it is started with `powershell -ExecutionPolicy Bypass -File`, which the binding rules forbid, and it restores `clean`, which would remove the new user. Its steps were run inline with the same functions, for `fresh2`, without the restore.

| Step | Result | What decided it | Screenshot |
|---|---|---|---|
| state | PASS | package folder True, LocalState True, settings.json False, wt.exe True: the same as Step 0's later reading | — |
| first start | PASS | wrote 1027 B; after the start the file (1890 B) still holds scheme "CMD Studio" (`#1F1E1D`) and the cmd profile's `colorScheme`; Terminal added `$help`, `actions`, `keybindings`, `newTabMenu`, `defaults`, `themes` and the profiles Windows PowerShell and Azure Cloud Shell. Exact `#1F1E1D`: **194,883** — under G7's 556,097 because the Start menu, which Windows 11 opens by itself at a first sign-in, covered about two thirds of the Terminal window; the visible part is `#1F1E1D` | wt-precreate |

Log: `spikes\vm\tmp\wt-precreate.log`.

## Run 5 — VM-A, Text size 150 % (cold-boot path, 03:56–04:20) — check 7: FAIL

| Step | Result | What decided it | Screenshot |
|---|---|---|---|
| R5.1 | done | set the way a person does: Settings → Accessibility → Text size (Win+R `ms-settings:easeofaccess-display`), slider Right ×50 → 150 (UIA reads 150), Tab, Apply; `TextScaleFactor` = 150 | r5-slider-150 |
| R5.2 | PASS | WebView2 follows Text size: page 1's heading 604×24 px (r1-p1) → 907×36 px (r5-p1), 1.50× | r5-p1 |
| R5.3 five clicks | PASS | 5 activations, 0 characters, 24 navigation keys; page 6 reached | — |
| R5.3 fit | **FAIL** | page 3: Next (and See all, Back, Skip) below the window, reached only by scrolling; page 5: Back cut at the bottom; page 6: Try cmd / Go to the home page cut at the bottom, the preview scrolls sideways; Home: the cards run past the window's right edge; Help → Report a problem: the details box widens the column past the window. Pages 1, 2, 4, Themes, Settings, Advanced and the other Help tabs fit. The VM's screen is 1024×768 | r5-p3, r5-p5, r5-p6, r5-home, r5-help-4 |

## Run 6 — VM-C, Thai user: NOT RUN

VM-C's guestcontrol failure was found and fixed (see "VM-C" below), and its Step 0 was recorded, but Run 6 needs a restore of its saved state, which fails while the host is locked. The Thai-name checks (13, the Thai box and the preview with a Thai name) are therefore not run. The preview defect F1 is expected to show on VM-C's Thai walk as well.

**VM-C's guestcontrol.** VBox.log: the guest session process says `Failed to accept session N (client ID 1): VERR_NOT_FOUND` after every resume of the `clean` saved state; the same in VBox.log.1 and .3 of 2026-10-13, while .2 worked: an intermittent Guest Additions session bug after a resume, not the host. The screenshot `E_FAIL` was the guest's display turned off; one Shift press wakes it. Fix used: a restart inside the guest (Ctrl+Alt+Del → power → Restart; the ACPI power button did nothing); guestcontrol then worked for `tester` and the Thai user within 80 s.

## Run 7 — VM-B, Windows 10 (information only, 00:00–00:10)

| Step | Result | What decided it | Screenshot |
|---|---|---|---|
| R7.1 | as expected | Step 0: not installed, no `FINDING:`; `AutoRun: (absent)` | — |
| R7.2 | PASS | WebView2 present (no message box); page 1 English; page 2 "Windows build 19045", "Windows Terminal is not installed: the prompt and welcome box work, but the window keeps its colors", "Install Windows Terminal", "You can skip this and install it later."; Tab 4 reaches Next | r7-p1, r7-p2 |
| R7.3 (check 6) | PASS | Install → `WinStore.App` runs, window "Microsoft Store" in front. Its page says "We're sorry, this website needs a newer browser" / "Your browser is out of date": the Store app of this never-updated image is old; not the app's | r7-store |
| R7.4 | PASS | page 5 "Good to know: Windows Terminal is not installed: only the prompt and the welcome box change."; no font line; page 6 "Open a new cmd window …" | r7-p5, r7-p6 |
| R7.5 | PASS | a conhost window with the welcome box and the prompt; `#0C0C0C` 415,535 (a OneDrive notice covered part; the plain window later counts 457,432); one cmd.exe with Clink; the box shows the `*` fallback | r7-conhost |
| R7.6 | PASS | Undo: "Undone: everything is as it was before."; `Compare-Object` nothing; AutoRun `(absent)` = before; no font registration; plain cmd: Clink no, rises 0, `#0C0C0C` 457,432. Home's status "1 thing(s) to look at" is D6 "Windows Terminal is not installed" (`E_WT_NOT_FOUND`) | r7-undone, r7-checks, r7-plain |

## The pass rule (spec §1 criterion 1)

| Item | Where | Result |
|---|---|---|
| The five-click apply, with colors in Windows Terminal | R2.4–R2.6 (never-opened), R2-NO.4–6 | PASS: 5 activations, 0 characters, 24 navigation keys; scheme near-count rise 428,799 |
| The never-opened path | R2 (never-opened), R2-NO, Run 3, Run 4 | PASS |
| The themed cmd window | R2.7 | PASS |
| The second cmd window | R2.8 | PASS |
| Undo, on the open-window path of 16a | R2.16a, R2.16 | PASS |
| Check 3, under 2 minutes | R2.5 | PASS, 41.8 s |
| The `present` path that Step 0 recorded for the normal snapshot | — | NOT RUN (host locked) |

**Verdict: criterion 1 is met on VM-A on the never-opened path** — the path the founder asked to include, and the harder one. It is not shown on the `present` path, which the normal restore of `clean` gives; that part of the gate is NOT RUN and needs an unlocked host.

## Findings

- **F1 — Thai UI: every terminal preview loses its monospace font, so the welcome box's right edge is ragged.** On the Thai home page the box's right edge falls at x = 531, 570 and 589 (17 rows each) with the corners at about 757, against one column (x = 670, 78 rows) on the English home page of the same run. Cause: `app/ui/src/styles.css:55` `:lang(th) { font-family: "Leelawadee UI", … }` matches every element, the preview's `<span>`s included, and so beats the monospace `fontFamily` that `app/ui/src/components/TerminalPreview.tsx:72` sets on the `<pre>` (the spans would otherwise inherit it). Evidence: r2-home vs r2-home-th. Affects every preview while the UI is Thai (Home, page 6, the editor, the import dialog), and check 13's preview on VM-C.
- **F2 — Text size 150 %: the wizard's page 3 needs scrolling to reach Next, and Home, page 5, page 6 and Help → Report a problem are clipped** (R5.3). Points to `app/ui/src/styles.css`: the shell's grid is `grid-template-columns: 13rem 1fr` (line 79), and a `1fr` track does not shrink below its content's minimum, so a `white-space: pre` preview or details box wider than the window widens the column instead of scrolling inside it (the editor's grid, line 384, uses `minmax(0, 1fr)`); page 3's six cards with their buttons are simply taller than the window. Seen on a 1024×768 screen.
- **F3 — minor: a cmd window that Windows hands over to Terminal (Win+R `cmd`, the Start menu) shows `*` in the welcome box, not `✻`,** though the tab is Terminal's and draws `✻`. The banner chooses the icon by `WT_SESSION` (`crates/core/src/generate/lua.rs:141-143, 219`), and a handed-over cmd does not get it. Evidence: r2-second, r2-doctor-typed (the Try cmd window, started by `wt.exe`, shows `✻`: r2-wt).

## Notes on the protocol and the tooling (no app defect)

- `Invoke-GuestButton` (`helpers.ps1`) finds nothing after a second start: the process then has three top-level windows ("CMD Studio", `ai.resincore.cmdstudio-siw` of the single-instance plugin, "Tao Thread Event Target") and `FindFirst` on the process can return the wrong one. The runs used the same UIA call limited to the window named "CMD Studio", through `Invoke-GuestSnippet`. The Help tabs were found by neither and were pressed with keys.
- guestcontrol runs with a filtered token (Medium, Administrators deny-only), so `net user /add` through guestcontrol is refused; the new users were made by typing into an elevated PowerShell on the VM. Typing a `Set-ExecutionPolicy … Bypass` line there (to run a copied script) was refused by the permission system and was not retried.
- Run 4's `run.ps1` cannot be used under the binding rules (`-ExecutionPolicy Bypass -File`), and it restores `clean`.
- "Wait until no `msedgewebview2`" (R1.8, R2.17, R2.18) never holds on Windows 11: SearchHost.exe and Widgets.exe keep their own; the runs waited for the processes whose command line has `--webview-exe-name=cmdstudio.exe`.
- "Saving a restore point (n of N)" was never on a screenshot, even 0.05 s after Apply; the backup steps pass too fast to see. "Applying (n of N)" and "Checking the result" were seen.
- Windows 11 opens the Start menu by itself at a user's first sign-in and after the boot-time setup; keys must not be sent before Esc.
- Home's status read "1 thing(s) to look at" right after an Undo (Run 3, R2-NO, R2.16) and none in the Help check 3 minutes later (R2.16: every line ok or skipped); not followed further.
- The protocol's "Results" table asks for the screenshots in `docs/spikes/phase3-vm-check/shots/`; the run brief says to keep them in `spikes\vm\tmp\`, which was followed.

## Not run

- **Run 6 (VM-C, Thai user)**: needs a restore of VM-C's saved state, which fails while the host is locked. Step 0 of VM-C is recorded.
- **Run 2 on the `present` path** (the normal restore of VM-A's `clean`): same reason. Run 2 ran in full on the never-opened state instead.
- **Run 1 step 6's founder mouse walk** and the "Results" mouse-click count: no founder at the VM window.
- **Check 15** (a failed settings save): not a VM step (unit tests).
- `fresh`, `fresh2`, `fresh3` existed only on the cold boots; `clean` is restored and unchanged (`snapshot list`: `clean` only, same UUIDs).

## Second session (build d7fb389)

- **Build:** CI run 39078781267, artifact `cmdstudio-exe`, commit `d7fb389`. `cmdstudio.exe` sha256 `87502EB49969850FDEB8AE57AF5E922BB5D4D715043499893CAC0F0316BC8A73`, 9 files under `resources\`; every install printed `same exe: True`.
- **What changed in it:** the Thai font rule no longer reaches the preview (F1); the shell's grid is `13rem minmax(0, 1fr)` and wraps (F2); the window is made with `preventOverflow: true`; the editor stacks with the preview on top below 49.5rem.
- **Conditions:** the host was locked from 06:57 to 08:26, so VM-A ran on its cold-boot path again: `snapshot restore clean`, `discardstate`, `startvm`, the boot-time "Installing", and VBoxService started by hand. Each VM-A run was a separate boot from `clean`, with OS 26H2 26300.9457 and Terminal never-opened at the start of each boot. Screenshots are `cmdstudio-win11-s2-*.png`. At 12:43 the host was found unlocked. VM-C was restored from its saved state then, and Run 6 ran on that one boot. The host was locked again at 12:48, so no second restore was possible.

### Run 5 again — Text size 150 % at 1024×768: PASS

| Item | Result | What decided it | Screenshot |
|---|---|---|---|
| Text size | done | set in Settings (slider focused, Right ×50 → 150, Tab, Apply); `TextScaleFactor` = 150; the app was started after it | s2-r5-slider-150 |
| The window against the work area | PASS, with a 1-px border | read in the guest (user32, dwmapi): `GetWindowRect` L −8 T 0 R 1032 B 728; DWM visible frame **L −1 T 0 R 1025 B 721** (1026×721); work area **L 0 T 0 R 1024 B 720**; taskbar 0,720–1024,768; not maximized; DPI 96. The page and its scrollbar end above row 720. The scrollbar's down arrow sits at about y 708. The frame's 1-px border lies on the taskbar's first row and on one column each side; the taskbar is drawn over that row (pixel row 720 is #CACACA). At Text size 100 % the rect is the same | s2-r5-p1, s2-r5-p3-next |
| Page 3's Next | PASS | Tab 10 from the heading puts the focus on Next and scrolls it into view; the page's vertical scrollbar shows both arrows above the taskbar | s2-r5-p3-next |
| Page 5's Back | PASS | Tab 3 scrolls Back into view; ShiftTab 1 back to Apply, Enter | s2-r5-p5-back |
| Page 6's buttons | PASS | Tab 1 is the preview (a scrollable group), Tab 2 is Try cmd and Tab 3 is "Go to the home page"; both buttons scroll into view | s2-r5-p6-buttons |
| Home: no overflow to the right | PASS | the widest element's right edge is 1002 against the page's 1025 (−23 px); the cards end inside the window, the preview scrolls inside its own box, and the buttons wrap | s2-r5-home, s2-r5-home-2 |
| Help → Report a problem: no overflow | PASS | −23 px as above; the tabs wrap and the details box scrolls inside | s2-r5-help-4 |
| The editor stacks | PASS | Create a theme: the toolbar first; "Preview" at T 295 and the preview image from T 347 to T 734; "Theme name" at T 1076, below the preview | s2-r5-editor |
| The other screens | PASS | Themes, Settings, Advanced and Help tabs 1–6: −23 px each | s2-r5-themes, s2-r5-settings, s2-r5-advanced, s2-r5-help-1..6 |
| The five clicks | PASS | **5 activations, 0 characters**; 26 navigation keys, which is the walk's 24 plus Tab 3 and ShiftTab 1 on page 5 to show Back | — |

### Run 2 steps 1–8 again (smoke test, never-opened Terminal): PASS

| Step | Result | What decided it |
|---|---|---|
| R2.1 | recorded | Step 0: `neverOpened` (empty `LocalState`), alias present, no `FINDING:`; AutoRun `(absent)`; no data folder |
| R2.2–R2.3 | PASS | only cmdstudio (no WindowsTerminal, OpenConsole or cmd); `webview\EBWebView` |
| R2.4–R2.5 | PASS | **5 activations, 0 characters, 24 navigation keys**; 40.4 s; page 5 as in the first session (s2-r2-p5); AutoRun `[REG_SZ]` with CMD Studio's entry; `fromTemplate` true; CMD Studio's 1515-byte Terminal file; `cmd=apply ms=1344 ok` |
| R2.6 | PASS | `#0C0C0C` 0 in the base shot and in try-1/2/3; one WindowsTerminal and 0 console hosts with a window; near `#1F1E1D` 158,805 → 490,181 (rise 331,376); after Terminal's first start the file is 2180 B with "CMD Studio" twice |
| R2.7 | PASS | one cmd.exe with `clink_dll_x64.dll`; frame rise 1,739; accent rise 2,230 (s2-r2-wt) |
| R2.8 | PASS | one cmd.exe with Clink; frame rise 1,850, accent rise 2,001, near rise 428,760 (s2-r2-second). The box still shows `*`, not `✻` (F3, not changed in this build) |

### Run 6 — VM-C, the Thai user (12:43–13:08): PASS

**The boot.** VM-C was restored from `clean`'s saved state at 12:43, while the host was unlocked. guestcontrol failed at first (`Failed to accept session 54 (client ID 1): VERR_NOT_FOUND`, lesson 4). At 3 min the guest bugchecked (0x21) and rebooted, which did lesson 4's restart. guestcontrol answered for the Thai user at 12:48. Phase 3b-1's spike ran on the same boot after Run 6 (its own report).

**Step 0 on that boot** matches the first session:
- `present`, with `settings.json` 1533 B and `state.json` 162 B;
- the alias is present, with no `FINDING:`;
- OS 26H2 26300.9457, system locale th-TH, ANSI code page 874.

Screenshots are `cmdstudio-th-r6-*.png`. The keyboard was switched to ENG with Win+Space and checked in the tray. One AutoRun read typed its Run line in Thai, because Win+Space had been pressed once too often; it was dismissed and read again in ENG.

| Step | Result | What decided it | Screenshot |
|---|---|---|---|
| R6.1 | recorded | `$before`: Terminal file 1533 B, sha256 `3353F73E…3E26`; `AutoRun: (absent)` | — |
| R6.2: Run 2 steps 2–3 | PASS | plain start; only cmdstudio; `webview\EBWebView` | r6-p1 |
| R6.2: the walk in Thai | PASS | ไทย first (Tab 2, Space), then Tab 2, Enter, then the keys of Run 2 step 4: **6 activations, 0 characters, 24 navigation keys**. Page 2: "Windows บิลด์ 26300", "มี Windows Terminal แล้ว หน้าต่าง cmd ของคุณจะได้สีของธีม". Page 5: four Thai lines and the button "ใช้ธีม", with no "keep the add-on outside your user folder" line. Page 6: "เสร็จแล้ว", "ใช้ธีมแล้ว เปิด cmd หน้าต่างใหม่เพื่อดูกล่องต้อนรับกับพร้อมท์", "ลองเปิด cmd", "ไปหน้าแรก"; 43.0 s | r6-p1-th, r6-p2, r6-p5, r6-p6 |
| R6.2: the Thai name does not stop the apply | PASS | `cmd=apply ms=3091 ok`; `state.json` `launch` **`shortPath`**, exe `C:\Users\3607~1\AppData\Local\AIRESI~1.CMD\runtime\clink-1.9.34\clink_x64.exe`; AutoRun `[REG_SZ] (if exist "<8.3 exe>" "<8.3 exe>" inject --autorun --quiet --profile "C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio\clink-profile")`; Terminal file 2650 B with scheme "CMD Studio" and `defaultProfile` 0caa0dad | — |
| R6.2: Run 2 step 6 | PASS | `#0C0C0C` 1 in base and in try-1/2/3; one WindowsTerminal and 0 console hosts with a window; near `#1F1E1D` 158,337 → 489,934 (rise 331,597) | r6-wt |
| R6.2: Run 2 step 7 | PASS | one cmd.exe with `clink_dll_x64.dll`; frame rise 1,739, accent rise 2,122; "Welcome back, ผู้ทดสอบ ไทย!", English tips | r6-wt |
| R6.2: Run 2 step 8 | PASS | one cmd.exe with Clink; frame rise 1,850, accent rise 1,893, near rise 429,014 | r6-second |
| R6.3 (check 13) | PASS | new cmd in `%USERPROFILE%\น้ำ ทดสอบ`: Clink loaded; box "Welcome back, ผู้ทดสอบ ไทย!", `cwd: C:\Users\ผู้ทดสอบ ไทย\น้ำ ทดสอบ`, prompt `~\น้ำ ทดสอบ`, น้ำ whole. Box edges (`Get-ColorColumns` of the accent `#D97757`, the box's frame color): left 96/97, right **501/502**, runs 91 and 85. VM-A's r2-wt has 44/45 and 476/477, runs 91 and 85: one pair each, no second pair a cell away. The box shows `*` (F3) | r6-thai-box |
| R6.4 (preview, F1 fix) | PASS | the box's right edge per row: page 6 in Thai at x **585 for 78 rows** (VM-A English r2-p6: 571 ×78); Thai Home at **686 ×78**. In the first session on build 04e36f6 the Thai Home had x 531/570/589 | r6-p6, r6-home-th |
| R6.5 (check 12) | PASS | 2,573 characters; `%USERNAME% present: True`; `user name present: False`; `8.3 name … (3607~1) present: False`; `any account SID present: False` | r6-copied |
| R6.6 (check 10) | PASS | the "ลองเปิด cmd" tab shows the English box and tips; generated files: Thai letters False ×3; `doctor.txt` with the Thai UI: D1–D11 all ok, Thai letters False; Settings → English (Tab 4, Up): `"language": "en"` | r6-wt |
| R2.9 on VM-C | PASS | Thai Home: สถานะ "ทุกอย่างปกติดี", Terracotta and the preview; bar "ย้อนกลับ" / "ปิดข้อความ" | r6-home-th |
| R2.11 on VM-C (checks 2, 4) | PASS | `doctor.txt` complete (D1–D11 ok), `help.txt`; in cmd `help=0`, `doctor=0`, `bad=2`; `exit-code.vbs` 0 / 2 / 0 / 0 | r6-step11-cmd |
| R2.13 on VM-C | PASS | Explorer at `file:///C:/Users/ผู้ทดสอบ ไทย/AppData/Local/ai.resincore.cmdstudio` and `…/logs` | r6-folders |
| R2.16 on VM-C, **`present` path** | PASS | Undo by name: "Undone: everything is as it was before."; Home "No theme is applied yet."; Terminal's file 1533 B with **the same sha256** as before; `Compare-Object` nothing; AutoRun `(absent)` = before (and the applied read had a value); plain cmd: Clink no, frame rise 0, accent −229, near −899 | r6-undone, r6-plain |
| R2.18 on VM-C (checks 2, 8) | PASS | `--cleanup --data-dir "C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio"` (one argument, with the space): `exit=0`; data folder gone (no themes were made in this run); AutoRun `(absent)` | — |

### Not run in the second session

- **Run 2 on VM-A's `present` path:** VM-A needs a restore of its saved state, and the host was locked again by the time VM-C was done. VM-C covered the `present` path: walk, colors, both windows and Undo, on Thai Windows.
- **Run 2 steps 12–15 and 17 on VM-C:** not in Run 6's list. The clipboard and the Thai UI were checked in R6.5 and R6.6.
