# Phase 3 — VM check of spec §1 criterion 1

- **Date:** written 2026-10-17 (phase 3, Task 4). The controller runs it. Dates, numbers and screenshots of the runs go into `results.md` beside this file (see "Results").
- **What it checks:** spec §1 criterion 1, which spec §8 makes the gate of phase 3: a person who has never used cmd applies a theme within 2 minutes without typing a command, on Windows 11. It also holds the checks that the phase's reviews could not make without a real Windows: the Content Security Policy, the exit codes, a Windows Terminal that was never opened, a Thai user name, and the dialogs that Windows draws itself.
- **The build:** only the CI artifact `cmdstudio-exe`, made by `tauri build --no-bundle`: `cmdstudio.exe` with `resources\` beside it. A plain `cargo build` exe is not the app and is never used here. The app has no installer yet (phase 4), so it runs from a folder. Record the commit it was built from.
- **VMs** (`spikes/vm/README.md`; every run starts from the snapshot `clean`):

  | VM | Name | What it stands for | User |
  |---|---|---|---|
  | VM-A | `cmdstudio-win11` | Windows 11 26H2, English, Windows Terminal 1.24: the gate | `tester` |
  | VM-B | `cmdstudio-win10` | Windows 10 22H2, English, no Windows Terminal: information only | `tester` |
  | VM-C | `cmdstudio-th` | Windows 11, system locale th-TH, a Thai user name with a space | `ผู้ทดสอบ ไทย` |

- **How the VMs are driven:** from the host with VBoxManage, through the functions of `spikes/vm/vm.ps1`:
  - `Wait-GuestReady` waits until guestcontrol can run a command.
  - `Copy-ToGuest`, `Invoke-Guest` and `Invoke-GuestPs` put files in and run a program or an encoded PowerShell command through guestcontrol. That runs outside the user's desktop; Thai comes back as UTF-8.
  - `Invoke-GuestDesktop` runs a script file on the signed-in user's desktop through an interactive scheduled task. The user's environment and HKCU are there.
  - `Send-Run` is Win+R, the text, Enter. Other keys go in with `controlvm keyboardputscancode`; VBoxManage cannot move the mouse.
  - `Save-Shot` takes a screenshot and `Measure-Color` counts the pixels of exactly one color in it.
  - `helpers.ps1` in this folder adds what the steps need: `Start-CleanVm`, `Install-Phase3App`, `Send-Key`, `Send-Text`, `Invoke-GuestSnippet`, `Get-ExitCode`, `Get-GuestFacts` and the other `Get-Guest*` functions, and two more pixel counts. Each says in its comment whether it is console-free or a desktop reading.
- **How results are judged:** by a pixel count (`Measure-Color`, `Measure-ColorNear`) or by a line the guest prints: a file's text, a hash, a registry value, a process list, an exit code, a time. Never by eye alone. Words on the screen and layout have no count; those steps name the number or line that goes with the screenshot, and "The steps that stay visual" lists them.

## Before you start

### Two lessons from this phase

1. **A console program started in the guest opens Windows Terminal.** On Windows 11 the default terminal is Windows Terminal. A console program started on the desktop (`cmd`, `powershell`, a Run line that opens one) opens it, and that ends the never-opened state that Step 0 and Run 2 must see. So from the restore of `clean` until Terminal is meant to open (Run 2 step 6, "Try cmd"), start nothing that has a console. Use guestcontrol (`Invoke-Guest`, `Invoke-GuestPs`, `Copy-ToGuest`), `wscript.exe` with a `.vbs` file, and the app itself. The functions of `helpers.ps1` say which kind they are; the ones built on `Invoke-GuestSnippet` start a hidden PowerShell on the desktop and are not console-free. (`Wait-GuestReady` itself runs `cmd.exe /c echo` through guestcontrol. That is allowed: guestcontrol starts a program outside the user's desktop, where no terminal hand-off happens. If Step 0 nevertheless finds a Terminal that is opened on a snapshot that was never-opened when it was taken, suspect the tooling first.)
2. **guestcontrol needs an unlocked host session and an unsandboxed shell.** "Error starting guest session (current status is: terminated)" or an HGCM `VERR_ACCESS_DENIED` means: stop and fix the host (unlock Windows, run PowerShell outside the sandbox). It does not mean the VM failed, and it is not recorded as a failure of the app. In this phase both errors left VM-A aborted-saved; restore `clean` and start again.

### Rules

- Nothing here runs on the dev PC. Every guest script that starts a program or changes something refuses to run on a machine whose name does not start with `CMDSTUDIO-` (Step 0's `.vbs` only reads).
- Inside the VMs cmd runs **without** `/d`. AutoRun is what is under test, and `/d` switches it off.
- Do not repair anything on a VM. A step that fails is recorded with its number and goes back to the task that owns it before the PR.
- The exe is unsigned. If Windows refuses to run it (Smart App Control, error 4551 or 0x800711C7), record that as a finding about the unsigned build; phase 4 signs it.

### Setup (once per host session)

Windows PowerShell 5.1 on the host, outside any sandbox, in an unlocked session, from the repo root:

```powershell
. .\spikes\vm\vm.ps1
. .\docs\spikes\phase3-vm-check\helpers.ps1

# The CI run of the commit under test, and its artifact.
gh run list --workflow ci.yml --branch phase3-ui --limit 5 --json databaseId,headSha,status,conclusion
$runId = 0                      # the databaseId of the green run for the commit under test
$artifact = Join-Path $Shots 'cmdstudio-exe'
gh run download $runId --name cmdstudio-exe --dir $artifact
Get-ChildItem $artifact         # cmdstudio.exe and resources\ side by side
$sha = 'the headSha of that run'
git diff --stat $sha HEAD -- app crates presets schema scripts Cargo.toml Cargo.lock    # must print nothing
```

The `git diff` must print nothing: the artifact then holds the code of the branch tip. Write the commit and the run id in `results.md`.

Then, per VM, set these (the first line of the VM at hand; VM-C's name is written as code points so that no shell has to carry Thai letters):

```powershell
$vm = 'cmdstudio-win11'; $u = 'tester'                         # VM-A
$vm = 'cmdstudio-win10'; $u = 'tester'                         # VM-B
$vm = 'cmdstudio-th'; $u = -join ([char[]](0x0E1C,0x0E39,0x0E49,0x0E17,0x0E14,0x0E2A,0x0E2D,0x0E1A,0x20,0x0E44,0x0E17,0x0E22))   # VM-C
$data = "C:\Users\$u\AppData\Local\ai.resincore.cmdstudio"     # the app's data folder
$wtState = "C:\Users\$u\AppData\Local\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState"
```

### What is where

| File in this folder | What it is |
|---|---|
| `protocol.md` | this file |
| `helpers.ps1` | host functions used by the steps (dot-source after `vm.ps1`) |
| `step0-wt-state.vbs` | Step 0: Windows Terminal's state, read only, no console |
| `start-app.vbs` | starts the app with no console; `debug` also sets the CSP check's debug port, for the app's process only |
| `exit-code.vbs` | runs one command like the uninstaller's `ExecWait` (hidden, no console, waiting) and writes the exit code to a file |
| `wt-precreate.ps1`, `run.ps1` | the pre-check of Run 4: does Terminal's first start use a settings.json written before it? |
| `results.md` | written by the controller after the runs; it does not exist yet |

Inside every VM, all ASCII and without spaces, so that Win+R can type them (`$P3` in `helpers.ps1`):

| Path | Holds |
|---|---|
| `C:\cmdstudio\phase3\app\` | `cmdstudio.exe` and `resources\` of the artifact |
| `C:\cmdstudio\phase3\check\` | the scripts of this folder |
| `C:\cmdstudio\out\phase3\` | what the scripts write: the Step 0 file, exit codes, the clipboard text |
| `C:\cmdstudio\spikes\vm\guest\` | `run-on-desktop.ps1`, which `Invoke-GuestDesktop` runs |

### Driving the app

VBoxManage can send keys but not the mouse, so the app is driven with the keyboard. **A click here is one activation** of a control: Enter or Space on a focused button. Moving focus with Tab is not a click. The walk counts the activations the host sent and the characters it typed (none).

- `Send-Key $vm Tab 4` presses Tab four times; `Send-Key $vm Enter` activates the focused control. Keys: `Tab`, `ShiftTab`, `Enter`, `Space`, `Esc`, `Up`, `Down`, `Left`, `Right`, `AltF4`, `WinD`, `WinSpace`, `CtrlA`, `CtrlV`.
- To type text use `Send-Text $vm 'text' -Enter`. It quotes the text itself, which `Send-Run` cannot do under Windows PowerShell 5.1. Use `Send-Run` only for text without double quotes.
- The pages put focus on their heading when they come, and Tab goes on from there. The counts below come from the source (`Wizard.tsx`, `App.tsx`, `Help.tsx`, `Settings.tsx`). They are hints: a note in a plan, or a preview that Chromium makes a focusable scroller, adds a stop. Run 1 confirms them, and `results.md` records the counts that held.

  | Where | Press | Reaches |
  |---|---|---|
  | Page 1, window just opened | Tab 4 | Next (after English, ไทย, Skip) |
  | Page 2 | Tab 3 (4 with the Install button) | Next (after Back, Skip) |
  | Page 3 | Tab 10 | Next (6 theme cards, See all 12 themes, Back, Skip) |
  | Page 4 | Tab 5 | Next (the target radios count as one stop, then the checkbox, Back, Skip) |
  | Page 5 | Tab 2 | Apply (the Steps summary comes first) |
  | Page 6 | Tab 1 / Tab 2 | Try cmd / Go to the home page |
  | Any page of the app, from its heading | ShiftTab 1 / 2 / 3 / 4 / 5 | Advanced / Help / Settings / Themes / Home |
  | Any page, with the after-apply bar showing | ShiftTab 6 / 7 | Dismiss / Undo |
  | Help, from its heading | Tab 1 to 6 | Health check, Restore points, Undo everything, Report a problem, Short guide, Credits and licenses |
  | Settings, from its heading | Tab 4 while a theme is applied (3 when none), then Down | the language radios, then ไทย |

- Two aids ask the guest instead of guessing, both through UI Automation, which is how a screen reader sees WebView2. **They have not been run against this app**; if they find nothing, the screenshot after each Tab group is the fallback. `Get-GuestFocus $vm $u` prints the name of the focused control. `Invoke-GuestButton $vm 'Next' $u` presses the control with that name. Both are desktop readings: not before Terminal has opened on a never-opened machine.
- Keys go to the window in front. When a Terminal, Explorer or Edge window has opened, close it with `Send-Key $vm AltF4` (it is in front) before the next key for the app, and look at `Get-GuestWindows $vm $u` if in doubt. The Tab counts above hold from a page's heading, which has focus right after a page change; after another window has had the focus, change the page by name with `Invoke-GuestButton` or press a navigation button first.
- The names of the controls are the dictionaries' (`app/ui/src/i18n/en.ts` and `th.ts`), quoted exactly below.
- If the founder or the controller has the VM's window open (`Start-CleanVm $vm $u -Type separate`), do the walk once with the mouse as well. That is the criterion's own word, "click".

### The steps that stay visual

No count answers these; read the screenshot and attach it: the words of a page (English, Thai), which language or filter button is pressed (check 14), the box's right edge in the preview, the `✻` on the tab, the `*` that the box falls back to in conhost, and whether every page fits at 150% text size (check 7). Each of them still gets a number or a printed line where one exists.

## Step 0 — Windows Terminal's state on the clean snapshot (before anything else)

Done on each VM right after its restore, before any other step. It records what Terminal looks like when nothing has opened it, and it decides what Run 2 must see on page 2. It uses guestcontrol and a `.vbs` only: no console.

```powershell
Start-CleanVm $vm $u
Install-Phase3App $vm $u $artifact                    # console-free; prints "same exe: True"
Invoke-Guest $vm 'C:\Windows\System32\wscript.exe' @('//B', '//Nologo', "$($P3.Check)\step0-wt-state.vbs", "C:\Users\$u\AppData\Local", "$($P3.Out)\step0-wt-state.txt") $u 120
Wait-GuestFile $vm "$($P3.Out)\step0-wt-state.txt" $u 60
Get-GuestText $vm "$($P3.Out)\step0-wt-state.txt" $u 'Unicode'
```

The file records, for the Store, Preview and unpackaged variants:

- whether the package folder `%LOCALAPPDATA%\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe` exists, whether `LocalState` and `settings.json` exist, and what each folder holds;
- the `wt.exe` alias, and the Windows Terminal version (it is in the package name that the script reads from the registry; if none was found, read it with `Get-AppxPackage Microsoft.WindowsTerminal` once Step 0 is recorded);
- the line `page 2 state`, which applies the app's own rule: the package folder alone makes a packaged Terminal "installed", and `detect::never_opened` picks the one an apply sets up.

Record the whole file in `results.md`. It decides what the steps below must see:

| `page 2 state` | Page 2 shows (en.ts) | Never-opened runs |
|---|---|---|
| `present` | "Windows Terminal is installed: your cmd windows get colors" | Runs 3 and 4 are not run on this snapshot (see below) |
| `neverOpened` | "Windows Terminal is installed and was never opened: CMD Studio sets it up when the theme is applied" | Runs 3 and 4 run |
| `not installed as the app reads it` | "Windows Terminal is not installed: the prompt and welcome box work, but the window keeps its colors", with the button "Install Windows Terminal" | not applicable |

A line starting `FINDING:` (the `wt.exe` alias is there but no package folder) means Windows Terminal is installed for the user and the app reads it as not installed. That is a defect in the app's detection, not a VM problem; record it and go on.

If Step 0 finds a Terminal that was already opened, a never-opened state needs a snapshot taken before Terminal's first start. Use one if it exists. Otherwise mark Runs 3 and 4 **not run** and say why in `results.md`. Do not fake the state by deleting `settings.json` from an opened Terminal: that tests the template and not Terminal's first start.

The facts belong to the snapshot, so they hold for every restore. Run the `.vbs` again at the start of Run 2 and Run 3 and compare; a difference means something in the tooling opened Terminal.

## Run 1 — VM-A: rehearsal, Terminal-only target, and the CSP read

Terminal's state does not matter here, so desktop readings are allowed. This run finds the Tab counts that hold, checks the Terminal-only wording, and reads the page's console for the Content Security Policy.

1. **Start and install.** `Start-CleanVm $vm $u`, `Install-Phase3App $vm $u $artifact`, and Step 0 if it is not recorded yet.
2. **Start the app with the debug port.** For this run only, the app's process gets `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`; no other process and no other run does. The `.vbs` sets it for its own process, which the app inherits.

   ```powershell
   Send-Run $vm 'wscript.exe //B //Nologo C:\cmdstudio\phase3\check\start-app.vbs debug'
   Start-Sleep -Seconds 10
   Save-Shot $vm 'r1-p1'
   Get-GuestWindows $vm $u
   Invoke-GuestSnippet $vm "Invoke-RestMethod http://127.0.0.1:9222/json/list | ForEach-Object { '{0} | {1} | {2}' -f `$_.type, `$_.title, `$_.url }" $u
   ```

   PASS: the window is up within 10 seconds; the screenshot shows the wizard's page 1 in English ("Make your cmd window beautiful and easy to read", "Step 1 of 6") with no console window and no message box; `Get-GuestWindows` lists `cmdstudio` with the title "CMD Studio" and counts no console host with a window; the debug port lists a `page` titled "CMD Studio".
3. **Walk the pages with the focus probe.** On each page press Tab by the table above, ask `Get-GuestFocus $vm $u`, take `Save-Shot $vm 'r1-p<n>'`, and write down the number of Tab presses that reached the control. Along the way:
   - **Page 1 (check 14).** Tab to "ไทย", Space: the whole page turns Thai at once ("ขั้นที่ 1 จาก 6") and the pressed button shows an accent border and bold text (`styles.css`, `.row-actions > button[aria-pressed="true"]`). Screenshot `r1-p1-th`. Shift+Tab to "English", Space: English again. Screenshot `r1-p1-en`. Read which button is pressed in each.
   - **Page 2.** The Windows line reads "Windows build 26300". The Terminal line is the one Step 0 predicted. "No other program sets up cmd" and "No administrator rights are needed" are there.
   - **Page 3.** Terracotta is pressed (selected) on arrival. "See all 12 themes" shows twelve cards and turns into "Show the recommended 6"; press it back.
   - **Page 4.** Tab to the target radios, Down: "Only in Windows Terminal" is selected. The box "Make Windows Terminal open cmd by default" stays ticked. Next.
   - **Page 5.** Screenshot `r1-p5`. The list of what will change is the real plan; for a target of Windows Terminal only it holds no line "Use this theme in every cmd window". Apply. The progress reads "Saving a restore point (n of N)", "Applying (n of N)" and "Checking the result".
   - **Page 6.** The text is "Your theme is applied. Open a new cmd tab in Windows Terminal to see the welcome box and the prompt." It must not say "Open a new cmd window". The buttons are "Try cmd" and "Go to the home page". Screenshot `r1-p6`.
4. **Facts of the Terminal-only apply.**

   ```powershell
   Get-GuestText $vm "$data\settings.json" $u
   Get-GuestFacts $vm $u
   Get-GuestText $vm "$wtState\settings.json" $u
   ```

   PASS: the app's `settings.json` holds `"target": "wt"` and `"wizardDone": true` (it is written when the apply commits, not only at "Go to the home page"); AutoRun is `(absent)`, because a Terminal-only apply leaves AutoRun alone; the cmd profile in Terminal's `settings.json` has a `commandline` that starts with `%SystemRoot%\System32\cmd.exe /k` and holds `inject --quiet --profile`.
5. **Try cmd, then Go to the home page.** A Windows Terminal tab opens with the cmd profile. Screenshot `r1-wt`. PASS: `Measure-ColorNear` of `#1F1E1D` in it is at least 100000 higher than in `r1-p6`. This run is a rehearsal; the formal check is Run 2 step 7.
6. **Visit every screen, so that every kind of style and dialog has been drawn.** Home; Themes (filter "Dark", "Light", "All"; press "Customize" on the first card, then "Back to themes"; open Rename, Delete and "Import a theme file" and close each with Esc); Settings; Help with all six tabs (on "Restore points" open "Go back to this point" and close it; on "Undo everything" open the confirmation and close it); Advanced. Keep a list of what was opened.
7. **Read the console (check 1).** Open Edge on the VM and look at the page's console.

   ```powershell
   Send-Run $vm 'msedge.exe --no-first-run edge://inspect/#devices'
   ```

   On the page "Discover network targets" must be ticked, with `localhost:9222` under "Configure…". The target "CMD Studio" appears under Remote Target; Tab to its "inspect" link and press Enter, then open the Console tab of the window that opens. Screenshot `r1-console`.

   PASS: no message that mentions "Content Security Policy" (`style-src`, `connect-src`, `script-src`), and no message in red. The page was drawn through every screen of step 6, so a blocked inline style would have shown. If DevTools cannot be reached, record the visible proxies instead: the colors are painted (`Measure-Color` of the app's surface color in the screenshots of every screen is not zero) and no area of any screenshot is blank. The debug port exists only on this VM and only for this run; no devtools feature is added to the app.
8. **Close.** `Send-Key $vm AltF4`, close Edge, and wait until `Get-GuestProcesses $vm $u` lists no `cmdstudio` and no `msedgewebview2`.

Record in `results.md`: the Tab counts that held, the console result, and the page-6 text.

## Run 2 — VM-A: the gate (criterion 1)

Restore `clean`. The Terminal is in the state Step 0 recorded. On a never-opened Terminal, steps 1 to 5 are **console-free**: only guestcontrol, the `.vbs` files and the app.

1. **Restore, install, record.** `Start-CleanVm $vm $u`; `Install-Phase3App $vm $u $artifact`; run Step 0's `.vbs` again and compare with the recorded file. Then:

   ```powershell
   $before = Get-GuestFacts $vm $u -NoConsole                 # Terminal, fonts and data folder before the apply
   Invoke-GuestPs $vm "Test-Path -LiteralPath '$data'" $u      # False: the app has not started yet
   ```

2. **Start the app (plain).** No debug port. This is the first window.

   ```powershell
   Send-Run $vm 'C:\cmdstudio\phase3\app\cmdstudio.exe'
   Start-Sleep -Seconds 8
   Save-Shot $vm 'r2-p1'
   Get-GuestProcesses $vm $u
   ```

   PASS: within a few seconds the window is up on page 1 in English; `Get-GuestProcesses` shows `cmdstudio` and `msedgewebview2` and none of `WindowsTerminal`, `OpenConsole` or `cmd` (nothing has a console); the screenshot shows no message box.
3. **WebView2's data (check 8).** Console-free:

   ```powershell
   Get-GuestFacts $vm $u -NoConsole | Where-Object { $_ -like 'data*' }
   Invoke-GuestPs $vm "Get-ChildItem -LiteralPath '$data\webview' -Force | ForEach-Object { `$_.Name }" $u
   ```

   PASS: `webview\` holds `EBWebView`; there is **no** `EBWebView` directly in the data folder; and every entry of the data folder is one that `--cleanup` knows as CMD Studio's own: `generated`, `clink-profile`, `runtime`, `snapshots`, `themes`, `drafts`, `logs`, `webview`, `state.json`, `settings.json`, `cmdstudio.lock`. The listing is repeated at step 18.
4. **The five-click apply (the gate).** The Tab counts are Run 1's. The host sends exactly five activations and types nothing. Wait three seconds after pages 2 and 4 for the machine check and the plan.

   ```powershell
   Send-Key $vm Tab 4;  Send-Key $vm Enter             # click 1: page 1 -> 2
   Start-Sleep -Seconds 3; Save-Shot $vm 'r2-p2'
   Send-Key $vm Tab 3;  Send-Key $vm Enter             # click 2: page 2 -> 3
   Send-Key $vm Tab 10; Send-Key $vm Enter             # click 3: page 3 -> 4
   Send-Key $vm Tab 5;  Send-Key $vm Enter             # click 4: page 4 -> 5
   Start-Sleep -Seconds 3; Save-Shot $vm 'r2-p5'
   Send-Key $vm Tab 2;  Send-Key $vm Enter             # click 5: Apply
   for ($i = 0; $i -lt 40; $i++) {                     # the apply commits when settings.json says wizardDone
     Start-Sleep -Seconds 3
     if ("$(Get-GuestText $vm "$data\settings.json" $u)" -match '"wizardDone":\s*true') { break }
   }
   Start-Sleep -Seconds 2; Save-Shot $vm 'r2-p6'
   ```

   PASS: the apply ends on page 6, "Done", with "Your theme is applied. Open a new cmd window to see the welcome box and the prompt." and the buttons "Try cmd" and "Go to the home page". On page 2 the Terminal line is the one Step 0 predicted. On page 5 the plan lists "Use this theme in every cmd window" and "Colors and font of cmd in Windows Terminal"; when Step 0 said `neverOpened` it also lists "Set up Windows Terminal's settings so cmd gets this theme" and shows no warning "Windows Terminal is not installed: only the prompt and the welcome box change." The progress on the way reads "Saving a restore point", "Applying", "Checking the result".
5. **After the apply, console-free.**

   ```powershell
   Get-GuestTiming $vm $u -NoConsole
   Get-GuestText $vm "$data\settings.json" $u
   Get-GuestText $vm "$data\state.json" $u
   Get-GuestFacts $vm $u -NoConsole
   Get-GuestText $vm "$wtState\settings.json" $u
   Invoke-GuestPs $vm "Select-String -LiteralPath '$data\logs\cmdstudio.log' -Pattern 'cmd=apply'" $u
   ```

   - **Timing (check 3).** PASS: "seconds from the first window to Done" is under 120. The host pressed its keys faster than a person would, so a result above 60 is flagged in `results.md` even when it passes.
   - **The app's settings.** `settings.json` holds `"wizardDone": true`, `"language": "en"`, `"target": "all"` and `"setWtDefaultProfile": true`.
   - **The apply.** `state.json` names Terracotta as applied. The log has a `cmd=apply` line that ends `ok`.
   - **Terminal's file, by Step 0's state.**
     - `neverOpened`: `settings.json` now exists and is CMD Studio's file. It holds `$schema`, a `schemes` entry named "CMD Studio" with `"background": "#1f1e1d"`, the cmd profile `{0caa0dad-35be-5f56-a8ff-afceeeaa6101}` with `"colorScheme": "CMD Studio"`, and `defaultProfile` set to that guid; `state.json` holds `"fromTemplate": true` for it. If Step 0 said `LocalState` was absent, it exists now.
     - `present`: the file is the one `$before` hashed, with the same keys added and the rest untouched.
6. **Try cmd (the first Terminal start, on a never-opened one).** Press "Try cmd" (Tab 1 from the heading of page 6, then Enter; this is not a sixth click of the walk). Screenshots at once, three times, then again after 10 seconds.

   ```powershell
   $base = Save-Shot $vm 'r2-before-try'
   Send-Key $vm Tab 1; Send-Key $vm Enter
   $s1 = Save-Shot $vm 'r2-try-1'; $s2 = Save-Shot $vm 'r2-try-2'; $s3 = Save-Shot $vm 'r2-try-3'
   Start-Sleep -Seconds 10
   $wt = Save-Shot $vm 'r2-wt'
   foreach ($s in $base, $s1, $s2, $s3, $wt) { '{0}: black console {1}, scheme near {2}, exact {3}' -f (Split-Path $s -Leaf), (Measure-Color $s '#0C0C0C'), (Measure-ColorNear $s '#1F1E1D'), (Measure-Color $s '#1F1E1D') }
   Get-GuestWindows $vm $u
   ```

   - **No console flash (check 5).** `wt.exe` is started with a new console of its own (`open.rs`). PASS: none of the three early screenshots has more than 5000 more pixels of `#0C0C0C` (the classic console's black) than the base screenshot, and `Get-GuestWindows` shows exactly one new `WindowsTerminal` process and no console host with a window.
   - **Colors in Windows Terminal.** Terracotta sets opacity 96 and acrylic (`presets/terracotta.cmdtheme`), so the window may not paint `#1F1E1D` exactly; record the exact and the near count. PASS: the near count (±12) of `#1F1E1D` in `r2-wt` is at least 100000 higher than in the base screenshot (G7 counted 556,097 exact pixels for a plain scheme).
   - **On a never-opened Terminal** this is Terminal's first start with the file CMD Studio made. Read `settings.json` again: `Get-GuestText $vm "$wtState\settings.json" $u`. Record whether Terminal added its own profiles to it (the expectation is Terminal's generated profiles plus CMD Studio's) or left the file as it was; either is recorded as it is. PASS needs the scheme "CMD Studio" and the cmd profile's `colorScheme` to still be in the file, and the colors above on the screen.
7. **The themed cmd window (gate).** The tab of step 6:

   ```powershell
   Get-GuestCmdWindows $vm $u                      # the cmd.exe in the tab, with Clink's DLL loaded in it
   'frame {0}, accent {1}' -f (Measure-Color $wt '#5F5D58'), (Measure-Color $wt '#D97757')
   ```

   PASS: one `cmd.exe` has Clink's DLL loaded (`Clink loaded: clink_dll_x64.dll`); the frame color `#5F5D58` (the welcome box and the prompt's box) and the accent `#D97757` each have pixels (record the counts; the lines are drawn in full-coverage pixels, which acrylic does not blend); the near count of step 6 holds. The screenshot shows the Terracotta welcome box with the box-layout prompt in English, the frame's right edge in one column, and `✻` on the tab. Keep `r2-wt` for `results.md`.
8. **A second cmd window (gate).** Type `cmd` in Win+R, as a person does.

   ```powershell
   Send-Run $vm 'cmd'
   Start-Sleep -Seconds 8
   $second = Save-Shot $vm 'r2-second'
   Get-GuestCmdWindows $vm $u
   '{0}: scheme near {1}, frame {2}, accent {3}' -f (Split-Path $second -Leaf), (Measure-ColorNear $second '#1F1E1D'), (Measure-Color $second '#5F5D58'), (Measure-Color $second '#D97757')
   ```

   PASS: `Get-GuestCmdWindows` lists two `cmd.exe` and Clink's DLL is loaded in both (AutoRun injected it into the second); `$second` has the near count, the frame pixels and the accent pixels of step 7. On Windows 11 this window opens in Windows Terminal (the default terminal), which maps it to the cmd profile (G7): its colors come from the profile and its prompt and box from AutoRun.
9. **Home.** Close the two Terminal windows (`Send-Key $vm AltF4` in each, as each is in front; the app is behind them), then press "Go to the home page" (Tab 2 from the heading of page 6, or `Invoke-GuestButton $vm 'Go to the home page' $u`). Screenshot `r2-home`.

   PASS: Home shows "Status", "Checked at …", "Theme in use" with **Terracotta** and its preview, the buttons "Customize", "Change theme" and "Open cmd", "Switch quickly" ("Themes you apply show up here."), and "Latest restore point" with Terracotta and its time. The status text is "Everything looks fine", or "{n} thing(s) to look at" with n equal to the number of `warn` lines in `doctor.txt` of step 11, or "{n} problem(s) found" with n equal to its `FAIL` lines. The bar above shows `Theme "Terracotta" applied · open a new cmd window to see the welcome box and the prompt` with "Undo" and "Dismiss". Doctor's D3 reads `ok`, because Clink logged in the cmd windows opened since the apply.
10. **A second start focuses the first window (check 11).**

    ```powershell
    Send-Key $vm WinD                                   # minimize everything
    Send-Run $vm 'C:\cmdstudio\phase3\app\cmdstudio.exe'
    Start-Sleep -Seconds 4
    Get-GuestWindows $vm $u
    ```

    PASS: exactly one `cmdstudio.exe` process; the foreground window belongs to `cmdstudio` and is not minimized; no second window titled "CMD Studio".
11. **doctor, `--help` and the exit codes (checks 2 and 4).** In a cmd window with AutoRun, which Win+R opens in Terminal. `Send-Text` quotes for you. Close the window when done (`Send-Key $vm AltF4`).

    ```powershell
    Send-Run $vm 'cmd'; Start-Sleep -Seconds 8
    $exe = '"C:\cmdstudio\phase3\app\cmdstudio.exe"'
    Send-Text $vm "$exe doctor" -Enter                  # no redirect: what happens to the output?
    Start-Sleep -Seconds 5; Save-Shot $vm 'r2-doctor-typed'
    Send-Text $vm "$exe doctor > C:\cmdstudio\out\phase3\doctor.txt 2>&1" -Enter
    Start-Sleep -Seconds 5
    Get-GuestText $vm "$($P3.Out)\doctor.txt" $u
    Send-Text $vm "$exe --help > C:\cmdstudio\out\phase3\help.txt" -Enter
    Start-Sleep -Seconds 3
    Get-GuestText $vm "$($P3.Out)\help.txt" $u
    foreach ($case in 'help:--help', 'doctor:doctor', 'bad:--nonsense') {
      $name, $arg = $case -split ':'
      Send-Text $vm "start `"`" /wait $exe $arg" -Enter
      Send-Text $vm "echo $name=%ERRORLEVEL%>>C:\cmdstudio\out\phase3\exit-in-cmd.txt" -Enter
    }
    Start-Sleep -Seconds 5
    Get-GuestText $vm "$($P3.Out)\exit-in-cmd.txt" $u
    ```

    - **Open question 1 (spec appendix B, phase 2b).** `cmdstudio.exe doctor` is a Windows-subsystem exe, so cmd does not wait for it. Record what `r2-doctor-typed` shows: the prompt back first and the report after it, or the report first.
    - **`doctor > out.txt` (check 4).** PASS: `doctor.txt` holds the whole report: a line for each of `D1` to `D11`, each `ok`, `warn`, `FAIL` or `skip`; `help.txt` holds the usage text ("Usage:", the commands, "Exit codes:"). If a file is empty right after the prompt came back, wait and read again, and record the delay.
    - **Exit codes in cmd (check 2).** PASS: `help=0`, `bad=2`, and `doctor=0` (or `1` when a check failed, which must then be a `FAIL` line of `doctor.txt`).
    - **Exit codes the way the uninstaller reads them (check 2).** `exit-code.vbs` under `wscript.exe` has no console, as NSIS `ExecWait` has none. It quotes every path, which matters on VM-C.

      ```powershell
      Get-ExitCode $vm 'help' '--help' $u
      Get-ExitCode $vm 'bad' '--nonsense' $u
      Get-ExitCode $vm 'doctor' 'doctor' $u
      Get-ExitCode $vm 'doctor-json' 'doctor --json' $u
      ```

      PASS: `exit=0`, `exit=2`, and for the last two the same code as in cmd (`0`, or `1` with a `FAIL`). `exit=error` with error -2147024894 means the exe was not found: the path is wrong.
12. **The clipboard (check 12).** Help → Report a problem → "Copy details". By keys: from a page heading ShiftTab 2 and Enter for Help, Tab 4 and Enter for the tab "Report a problem", Tab 3 and Enter for the button; or `Invoke-GuestButton` with each of the names. The text on the page is the text that goes to the clipboard, and "Copied. Now open GitHub Issues and paste them." appears.

    ```powershell
    Get-GuestClipboardReport $vm $u
    ```

    PASS: the clipboard holds text; `%USERNAME% present: True`; `user name present: False`, `8.3 name … present: False` and `any account SID present: False`; `<SID> present: True` when the report names a SID. Then paste it into Notepad (`Send-Run $vm 'notepad'`, `Send-Key $vm CtrlV`, screenshot `r2-notepad`): the paste works from WebView2.
13. **The ShellExecute targets (check 6).** On the same tab, close the Notepad window of step 12 first (`Send-Key $vm AltF4`; if it asks to save, choose "Don't save"), then "Open GitHub Issues" (Tab 4 from the tab). Then Advanced (ShiftTab 1 from a heading): "Open the data folder" and "Open the log folder". Close each window that opens.

    ```powershell
    Invoke-GuestSnippet $vm "Get-CimInstance Win32_Process -Filter `"Name='msedge.exe'`" | ForEach-Object { `$_.CommandLine }; (New-Object -ComObject Shell.Application).Windows() | ForEach-Object { [uri]::UnescapeDataString(`$_.LocationURL) }" $u
    ```

    PASS: an `msedge.exe` command line (or the title of the Edge window, which starts with "Issues") holds `github.com/luraselenehalo/cmd-studio/issues`; the open Explorer windows' locations include the data folder and its `logs` folder. Read the Edge line at once: a second start of an Edge that already runs hands over and exits. The Store target is checked on VM-B (Run 7).
14. **Themes: filter, draft, import and export (checks 9 and 14).**
    - Themes → "Dark", "Light", "All": the pressed filter shows its accent border and bold text. Screenshots `r2-filter-light` and `r2-filter-all` (check 14).
    - "Create a theme": the editor opens ("Edit theme"). Under "Tone" choose "Light" (Tab to the radios and Down; `Get-GuestFocus` says where the focus is). Wait for the status "Draft saved". "Back to themes". PASS: a card under "Unsaved drafts" with the badge "Draft", and
      ```powershell
      Invoke-GuestPs $vm "Get-ChildItem -LiteralPath '$data\drafts' | ForEach-Object { `$_.Name }; Get-Content -Raw -LiteralPath (Get-ChildItem '$data\drafts' | Select-Object -First 1).FullName" $u
      ```
      shows a file with `"tone": "light"`. The card's "Edit" opens that draft.
    - "Customize" on the first card (Terracotta) makes "My Terracotta" under "My themes". On that card press "Export": the Windows Save dialog opens. Type the path:

      ```powershell
      Send-Key $vm CtrlA
      Send-Text $vm 'C:\cmdstudio\out\phase3\export-test.cmdtheme' -Enter
      Get-GuestText $vm "$($P3.Out)\export-test.cmdtheme" $u
      ```

      PASS: the page says "Saved to C:\cmdstudio\out\phase3\export-test.cmdtheme"; the file holds `"schemaVersion": 1` and `"name": "My Terracotta"`.
    - "Import a theme file": the Windows Open dialog opens. `Send-Text $vm 'C:\cmdstudio\out\phase3\export-test.cmdtheme' -Enter`. PASS: the dialog "Import this theme?" appears with a preview and "You already have this theme; the import is added as a separate theme."; press "Add to my themes"; `Invoke-GuestPs $vm "Get-ChildItem -LiteralPath '$data\themes' | ForEach-Object { `$_.Name }" $u` lists two files. Opening the dialog again and pressing Esc says "No file was chosen."
15. **Thai UI, and cmd stays English (check 10).** In the app, Settings → ไทย: Tab 4 to the language radios (a theme is applied), Down. The whole UI turns Thai at once.
    - Go Home. Screenshot `r2-home-th` (the Thai home page: "หน้าแรก", "ธีมที่ใช้อยู่"). `Get-GuestText $vm "$data\settings.json" $u` holds `"language": "th"`.
    - Press "เปิด cmd" (Thai for "Open cmd"). A new Terminal tab opens. Screenshot `r2-th-cmd`. PASS: its welcome box and tips are English; `Get-GuestCmdWindows` shows Clink in the new cmd.
    - ```powershell
      Invoke-GuestSnippet $vm "Get-ChildItem '$data\generated' -Recurse -File | ForEach-Object { `$t = (Get-Content -LiteralPath `$_.FullName -Raw -Encoding UTF8) -replace [regex]::Escape(`$env:USERNAME), ''; 'thai letters in {0}: {1}' -f `$_.Name, (`$t -match '[\u0E00-\u0E7F]') }" $u
      ```
      PASS: `False` for every generated file. Run step 11's `doctor > doctor.txt` again with the UI in Thai and test it:
      ```powershell
      ("$(Get-GuestText $vm "$($P3.Out)\doctor.txt" $u)" -replace [regex]::Escape($u), '') -match '[\u0E00-\u0E7F]'
      ```
      PASS: `False`. The user name is taken out of both texts first, because on VM-C it is Thai and sits inside paths.
    - Settings → English (Tab 4, Up on the radio): the UI is English again. Undo is pressed in English in the next step.
16. **Undo (gate).** In the app, from any page: ShiftTab 7 from the heading reaches "Undo" in the bar; Enter.

    ```powershell
    $after = Get-GuestFacts $vm $u
    $skip = '^(data|AutoRun|user font|WT LocalState entry)'
    Compare-Object ($before -notmatch $skip) ($after -notmatch $skip)
    ```

    PASS:
    - the bar reads "Undone: everything is as it was before." and Home reads "No theme is applied yet.";
    - AutoRun in `$after` is `(absent)`, as it was before;
    - `present`: Terminal's `settings.json` has the hash that `$before` has, and `Compare-Object` prints nothing;
    - `neverOpened`: the file CMD Studio made is gone, or, because the Terminal of step 6 is still running and writes a file again, `Select-String -SimpleMatch 'CMD Studio'` finds nothing in it. `LocalState` is removed only if CMD Studio made it and it is empty: after a Terminal has run it holds Terminal's own files, so it stays. Run 3 checks the case where it goes;
    - Help → Restore points lists the apply with the badge "Undone" and without a "Go back to this point" button, and `state.json` in the data folder names no applied theme.
17. **The language is remembered.** Settings → ไทย (Tab 3 now that no theme is applied, Down). Close the app (Alt+F4), wait for no `msedgewebview2`, start it with `Send-Run` as in step 2. PASS: it opens in Thai ("หน้าแรก"). Settings → English (Tab 3, Up), close, start: it opens in English.
18. **Close and clean up (checks 2 and 8).** Close the app (Alt+F4), wait for no `msedgewebview2.exe` (`Get-GuestProcesses`), then run `--cleanup` as the uninstaller would:

    ```powershell
    Get-ExitCode $vm 'cleanup' '--cleanup --data-dir %LOCALAPPDATA%\ai.resincore.cmdstudio' $u
    Get-GuestFacts $vm $u
    ```

    PASS: `exit=0`. The data folder is gone except `themes\`, which holds the saved themes of step 14: `drafts\`, `logs\`, `webview\` and `settings.json` were accepted as CMD Studio's own. An exit 2 that names `EBWebView\` or another entry means the app put something in its data folder that `--cleanup` does not know. AutoRun is `(absent)`.
19. **After the run.** Copy the screenshots named in "Results" into the docs folder. Do not take a snapshot: the next run restores `clean`.

## Run 3 — VM-A: a Terminal that was never opened, undone before it opens

Only if Step 0 said `neverOpened`. The point is the undo of a file that CMD Studio made, with no Terminal running to write it again. **Everything is console-free until the last step.**

1. Restore `clean`; `Install-Phase3App`; Step 0's `.vbs` again (the same facts as before); `$before = Get-GuestFacts $vm $u -NoConsole`.
2. Start the app and walk the five clicks exactly as in Run 2 step 4. At page 6 do **not** press "Try cmd": press "Go to the home page" (Tab 2).
3. `$applied = Get-GuestFacts $vm $u -NoConsole` and `Get-GuestText $vm "$wtState\settings.json" $u`. PASS: the file is CMD Studio's, as in Run 2 step 5, and `state.json` holds `"fromTemplate": true`. Record that `LocalState` was made, if Step 0 said it was absent.
4. Press "Undo" (ShiftTab 7 from the Home heading, Enter).
5. ```powershell
   $after = Get-GuestFacts $vm $u -NoConsole
   $skip = '^(data|AutoRun|user font)'
   Compare-Object ($before -notmatch $skip) ($after -notmatch $skip)
   Get-GuestProcesses $vm $u
   ```
   PASS: nothing differs, so the file CMD Studio made is gone. If Step 0 said `LocalState` did not exist, it is gone too; if it existed, it is still there as it was. This is where "removed only if CMD Studio made it and it is empty" is seen. Nothing has started Terminal: `Get-GuestProcesses` lists no `WindowsTerminal`.
6. Now open Terminal as a person would, to see that the undo left it never-opened: `Send-Run $vm 'wt'`, then `$d = Save-Shot $vm 'r3-wt-default'`. PASS: it starts with its own defaults (`Measure-Color $d '#0C0C0C'`, Campbell, is large and the near count of `#1F1E1D` does not rise) and writes its own `settings.json`, which holds no "CMD Studio".

## Run 4 — VM-A: Terminal's first start with a pre-written file

Only if Step 0 said `neverOpened`. This is the question that spec §5.4 still marks as unconfirmed on a VM, answered with a hand-written file so that it does not depend on CMD Studio. It tells a failure of Run 2 step 6 that is Terminal's from one that is CMD Studio's.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\docs\spikes\phase3-vm-check\run.ps1
```

`run.ps1` restores `clean`, writes a `settings.json` with a "CMD Studio" scheme (background `#1F1E1D`, no opacity, no acrylic) on the cmd profile, starts Terminal, takes a screenshot, prints the exact count of `#1F1E1D`, prints the file as Terminal left it, and closes Terminal. The log is `spikes\vm\tmp\wt-precreate.log`. Its first lines (`state`) are read on the desktop; they must show the same facts as Step 0 (the package folder and `LocalState` as Step 0 had them, no `settings.json`). If they show more, the desktop task itself opened Terminal, and the never-opened results of that boot are void.

PASS: the exact count of `#1F1E1D` is of the order of G7's 556,097 (record it), and the file after the start still holds the scheme "CMD Studio". Record whether Terminal added profiles to it.

## Run 5 — VM-A: Windows Text size 150% (check 7)

A change of the user's setting, so this is the last run on VM-A. Restore `clean`; Terminal's state does not matter.

1. Set Text size to 150%. The way a person does: Settings → Accessibility → Text size, slider to 150%, Apply. If driving Settings by keyboard is too fiddly, set the value that the slider sets, and say so in `results.md`:

   ```powershell
   Invoke-GuestSnippet $vm "if (-not (Test-Path 'HKCU:\Software\Microsoft\Accessibility')) { New-Item -Path 'HKCU:\Software\Microsoft\Accessibility' | Out-Null }; Set-ItemProperty -Path 'HKCU:\Software\Microsoft\Accessibility' -Name TextScaleFactor -Type DWord -Value 150" $u
   ```

   Start the app after the change, not before: Chromium reads the setting as it starts.
2. Start the app; `Save-Shot $vm 'r5-p1'`. Compare it with `r1-p1`: the text must be about 1.5 times as large. If it is not, record "WebView2 does not follow Windows Text size here", and repeat the run with Display scale 150% (Settings → System → Display → Scale), which every WebView2 app follows, so that the layout is still tested.
3. Walk the five clicks with the keys of Run 2 step 4, taking `Save-Shot` of every page (`r5-p1` to `r5-p6`). Then Home, Themes, Settings, Help (all six tabs) and Advanced (`r5-home`, `r5-themes`, `r5-settings`, `r5-help-1` to `r5-help-6`, `r5-advanced`).

PASS: the wizard still takes five clicks (the same five activations end on page 6); every page fits, with no clipped or overlapping button, heading or text, and no page that needs scrolling to reach "Next" or "Apply". This is a visual check; attach the screenshots.

## Run 6 — VM-C: Windows 11 with the Thai user

Windows is Thai (locale th-TH) and the user is `ผู้ทดสอบ ไทย`, with a space. The keyboard is Thai after sign-in. Before any `Send-Run` or `Send-Text`, press `Send-Key $vm WinSpace` and check in a screenshot that the tray shows ENG; without it `cmd` is typed as `แทก`. The app's own keys (Tab, Enter) do not depend on the layout.

1. **Setup.** Step 0 on VM-C (its Terminal may differ from VM-A's), `Install-Phase3App`, `$before = Get-GuestFacts $vm $u -NoConsole`.
2. **Run 2 steps 2 to 8 and 11, 12 and 18**, with the same commands, recorded for VM-C. The deviations:
   - On page 1 the app is English although Windows is Thai ("Make your cmd window beautiful and easy to read"). Press "ไทย" first (one extra activation, not part of the gate: the gate is VM-A's English walk) and walk the pages in Thai. The Thai texts to read are th.ts's: page 2, "มี Windows Terminal แล้ว หน้าต่าง cmd ของคุณจะได้สีของธีม" or, when Step 0 said `neverOpened`, "มี Windows Terminal แต่ยังไม่เคยเปิดใช้ เมื่อใช้ธีม CMD Studio จะตั้งค่าให้เอง"; page 5, the button "ใช้ธีม"; page 6, "เสร็จแล้ว", "ใช้ธีมแล้ว เปิด cmd หน้าต่างใหม่เพื่อดูกล่องต้อนรับกับพร้อมท์" and the buttons "ลองเปิด cmd" and "ไปหน้าแรก". The after-apply bar's button is "ย้อนกลับ" and its result "ย้อนกลับแล้ว ทุกอย่างเป็นเหมือนเดิม".
   - From page 5 and `state.json` also record whether the plan has the line "Keep the add-on outside your user folder, because your user folder's name has letters it can't use", and which `launch` kind `state.json` holds (`direct`, `shortPath` or `programData`). The Thai name must not stop the apply. In step 18 the data folder is `C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio`; `exit-code.vbs` expands `%LOCALAPPDATA%` and quotes the path, so the host never types it (check 2 with a path that has a space).
3. **A cmd window in a Thai folder, with the welcome box (check 13).** `cd` in an open window cannot change a welcome box that is already printed, so open a **new** cmd window whose working folder is Thai. The folder's name is "น้ำ ทดสอบ" (a SARA AM cluster, a space, Thai letters), written below as code points. The desktop opens the window, and AutoRun loads Clink in it:

   ```powershell
   $code = '$name = -join ([char[]](0x0E19,0x0E49,0x0E33,0x20,0x0E17,0x0E14,0x0E2A,0x0E2D,0x0E1A)); $folder = Join-Path $env:USERPROFILE $name; New-Item -ItemType Directory -Force -Path $folder | Out-Null; Start-Process -FilePath "$env:SystemRoot\System32\cmd.exe" -WorkingDirectory $folder'
   Invoke-GuestSnippet $vm $code $u
   ```

   The form for a cmd window that is open already is `start "" /d "<folder>" cmd.exe`, with the folder in quotes. The `/d` after `start` sets the folder. Do **not** add a `/d` after `cmd.exe`: that second one would switch AutoRun off, and AutoRun is what draws the box. Explorer's address bar works too: open the folder, type `cmd` in the address bar, Enter. The window should open in Windows Terminal, as Win+R's does.

   Wait 8 seconds; `$thai = Save-Shot $vm 'r6-thai-box'`.
   - PASS: the window shows the welcome box with `Welcome back, ผู้ทดสอบ ไทย!` and the folder as `cwd:`, and the prompt's path shows Thai as typed; `Get-GuestCmdWindows` shows Clink loaded in it; the near count and the frame and accent counts hold as in Run 2 step 7.
   - **Lined up.** Compare the frame color's columns with VM-A's `r2-wt`: `Get-ColorColumns $thai '#5F5D58'` and `Get-ColorColumns (Join-Path $Shots 'cmdstudio-win11-r2-wt.png') '#5F5D58'`. PASS: the box's right edge is one pair of adjacent columns with a run as tall as the box, as it is on VM-A. A second pair a cell away with a shorter run means a row is off by a cell. The "น้ำ" cluster stays whole on the screen (its letters are not split across cells). Read the screenshot too, and attach both.
4. **The preview in the app.** On page 6 and on Home the preview is drawn with this computer's user name. `Save-Shot $vm 'r6-preview'`. PASS: its box's right edge sits in one column on every row, with the Thai name in the title and the "น้ำ" cluster whole. As a number: `Get-ColorColumns` of the frame color on the preview with `-MinRun 10`, compared with the same on VM-A. This is a visual check with that number.
5. **The clipboard (check 12).** As Run 2 step 12. PASS: `%USERNAME%` stands where the Thai name would be; `user name present: False`, `8.3 name … present: False`, `any account SID present: False`.
6. **Thai UI and English cmd (check 10).** The walk was in Thai, so: the Terminal tab that "ลองเปิด cmd" opens shows an English welcome box and English tips (screenshot `r6-th-cmd`), and the tests of Run 2 step 15 on `generated\` and `doctor.txt` give `False`, on a Windows whose own language is Thai. Then Settings → English, as in step 15.
7. **Run 2 steps 9, 13 and 16** on this VM, recorded the same way (Home, the data folder and log folder opening on a Thai path, Undo).

## Run 7 — VM-B: Windows 10 (information only)

Windows 10 22H2 has no Windows Terminal and its cmd is the old console host. It is not a gate for phase 3 (spec §8 names the Windows 11 VM); what is seen here is recorded. WebView2 may be missing on a fresh Windows 10: then the app shows a message box "CMD Studio could not open its window: …" (English) instead of the window. Record the text; the runtime comes with the installer in phase 4.

1. `Start-CleanVm`, `Install-Phase3App`, Step 0 (expected: not installed, and no `FINDING:`), `$before = Get-GuestFacts $vm $u -NoConsole`.
2. **Run 2 steps 2 to 5.** Page 2 reads "Windows build 19045" and "Windows Terminal is not installed: the prompt and welcome box work, but the window keeps its colors", with the button "Install Windows Terminal" and "You can skip this and install it later." Tab 4 (not 3) reaches Next on page 2, because the Install button comes first.
3. **The Store link (check 6).** On page 2 press "Install Windows Terminal" (Tab 1 from the heading).

   ```powershell
   Start-Sleep -Seconds 8
   Invoke-GuestSnippet $vm "Get-Process WinStore.App, msedge -ErrorAction SilentlyContinue | ForEach-Object { '{0} {1}' -f `$_.ProcessName, `$_.MainWindowTitle }; Get-CimInstance Win32_Process -Filter `"Name='msedge.exe'`" | ForEach-Object { `$_.CommandLine }" $u
   ```

   PASS: the Store app starts (`WinStore.App`), or, when it cannot open the link, Edge starts with `aka.ms/terminal` in its command line. If neither happens the app must show an error (screenshot).
4. **Pages 3 to 6.** Page 5 lists, under "Good to know", "Windows Terminal is not installed: only the prompt and the welcome box change."; the page-6 text is "Open a new cmd window …". Press "Try cmd".
5. **The conhost window.** A plain console window opens with the welcome box and the prompt. Screenshot `r7-conhost`. PASS: `Measure-Color $shot '#0C0C0C'` is of the order of G7's 457,432 (no theme color: spec §1); `Get-GuestCmdWindows` shows Clink in its `cmd.exe`. Record whether the box's `✻` shows or the `*` fallback appears (visual).
6. Undo from the bar, and the facts as in Run 2 step 16.

## Not run on a VM

- **A failed settings save (check 15).** Making the data folder unwritable would break the folder the other checks need, so it is not tested here. The unit tests cover it: the wizard still goes home when the save fails, keeps the settings in memory, and says that the wizard shows again at the next start (`Wizard.tsx`, `App.test.tsx`).

## The checks and where they run

| # | Check | Where |
|---|---|---|
| 1 | CSP: no violation, no error in the console | Run 1 step 7 |
| 2 | Exit codes, in cmd and as the uninstaller's `ExecWait`, paths quoted | Run 2 steps 11 and 18; Run 6 step 2 |
| 3 | From the first window to Done in under 2 minutes | Run 2 step 5 |
| 4 | `cmdstudio doctor > out.txt` gives the full report; what a typed `doctor` does | Run 2 step 11 |
| 5 | `wt.exe` opens with no console flash | Run 2 step 6 |
| 6 | Store, GitHub Issues, data folder, logs folder open | Run 7 step 3; Run 2 step 13 |
| 7 | Windows Text size 150% | Run 5 |
| 8 | WebView2 in `<data>\webview\EBWebView`, nothing else new in the data folder | Run 2 steps 3 and 18 |
| 9 | The native import and export dialogs | Run 2 step 14 |
| 10 | cmd stays English while the UI is Thai | Run 2 step 15; Run 6 step 6 |
| 11 | A second start focuses the window | Run 2 step 10 |
| 12 | The clipboard from WebView2 | Run 2 step 12; Run 6 step 5 |
| 13 | Thai user: Clink's cell counting, the box and the preview lined up | Run 6 steps 3 and 4 |
| 14 | The pressed language button and filter are visible | Run 1 step 3; Run 2 step 14 |
| 15 | A failed settings save | not run (unit tests) |

## The pass rule

Criterion 1 of spec §1 is met when all of these pass on VM-A:

- the five-click apply, with colors in Windows Terminal in whichever state Step 0 recorded, and the never-opened path (Runs 2, 3 and 4) when a snapshot allows it;
- the themed cmd window (Run 2 step 7);
- the second cmd window (Run 2 step 8);
- Undo (Run 2 step 16).

Everything else is recorded PASS or FAIL and goes back to the task that owns it before the PR. Check 3, the two minutes, is criterion 1's own limit: a FAIL there is reported next to the pass rule, not among the rest. VM-B is information only. A run that is not run is recorded as not run, with the reason.

## Results

The controller writes the results of the runs into `results.md` in this folder, after the runs; this file holds the protocol only. For each step write its id (for example `R2.7`), PASS, FAIL or NOT RUN, and the number or printed line that decided it. Begin with the commit and the CI run the build came from, the date of each run, and the Step 0 file of each VM. Put these screenshots into `docs/spikes/phase3-vm-check/shots/` (no user name of the dev PC appears in them or in the text):

| Screenshot | Step |
|---|---|
| page 1 | `r2-p1` |
| page 5, the plan | `r2-p5` |
| page 6, Done | `r2-p6` |
| the Terminal window with the theme | `r2-wt` |
| the second cmd window | `r2-second` |
| Home | `r2-home` |
| the Thai home page | `r2-home-th` |
| the Thai box (VM-C) | `r6-thai-box` |
| the console (CSP) | `r1-console` |

Anything that fails is listed with its step id and goes back to the plan's owning task before the PR.
