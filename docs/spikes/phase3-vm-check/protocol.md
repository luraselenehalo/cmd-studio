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
  - `helpers.ps1` in this folder adds what the steps need: `Start-CleanVm`, `Install-Phase3App`, `Send-Key`, `Send-Text` (with `Reset-InputTally` and `Get-InputTally`), `Invoke-GuestSnippet`, `Get-ExitCode`, `Get-GuestFacts`, `Get-GuestAutoRun`, `Get-GuestConsoleLog` and the other `Get-Guest*` functions, and two more pixel counts. Each says in its comment whether it is console-free or a desktop reading.
- **How results are judged:** by a pixel count (`Measure-Color`, `Measure-ColorNear`) or by a line the guest prints: a file's text, a hash, a registry value, a process list, an exit code, a time. Never by eye alone. Words on the screen and layout have no count; those steps name the number or line that goes with the screenshot, and "The steps that stay visual" lists them.

## Before you start

### Lessons from this phase

1. **A console program started in the guest opens Windows Terminal.** On Windows 11 the default terminal is Windows Terminal. A console program started on the desktop (`cmd`, `powershell`, a Run line that opens one) opens it, and that ends the never-opened state that Step 0 and Run 2 must see. So from the restore of `clean` until Terminal is meant to open (Run 2 step 6, "Try cmd"), start nothing that has a console. Use guestcontrol (`Invoke-Guest`, `Invoke-GuestPs`, `Copy-ToGuest`), `wscript.exe` with a `.vbs` file, and the app itself. The functions of `helpers.ps1` say which kind they are; the ones built on `Invoke-GuestSnippet` start a hidden PowerShell on the desktop and are not console-free. (`Wait-GuestReady` itself runs `cmd.exe /c echo` through guestcontrol. That is allowed: guestcontrol starts a program outside the user's desktop, where no terminal hand-off happens. If Step 0 nevertheless finds a Terminal that is opened on a snapshot that was never-opened when it was taken, suspect the tooling first.)
2. **guestcontrol needs an unlocked host session and an unsandboxed shell.** "Error starting guest session (current status is: terminated)" or an HGCM `VERR_ACCESS_DENIED` means: stop and fix the host (unlock Windows, run PowerShell outside the sandbox). It does not mean the VM failed, and it is not recorded as a failure of the app. In this phase both errors left VM-A aborted-saved; restore `clean` and start again.
3. **"No `msedgewebview2`" never holds on Windows 11.** `SearchHost.exe` and `Widgets.exe` keep WebView2 processes of their own, so a wait for none ends only by its timeout. Wait for the app's: the `msedgewebview2.exe` processes whose command line holds `--webview-exe-name=cmdstudio.exe` (on the guest, `Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'"`, through `Invoke-GuestPs`, then `CommandLine -like '*--webview-exe-name=cmdstudio.exe*'`). The `msedgewebview2` lines of `Get-GuestProcesses` are all of them, the app's and the others.
4. **VM-C's guestcontrol needs a restart inside the guest after a resume.** After a restore of VM-C's `clean` saved state the guest session can fail, and `Wait-GuestReady` times out although the VM runs; VBox.log says `Failed to accept session N (client ID 1): VERR_NOT_FOUND`. It is an intermittent Guest Additions fault after a resume (3 of 4 resumes in this phase), not the app and not the host's lock of lesson 2. Restart Windows inside the guest (Ctrl+Alt+Del on the VM's window, then Power, then Restart: the ACPI power button does nothing); guestcontrol then answers within about 80 seconds. A screenshot that fails with `E_FAIL` at the same time is the guest's display turned off: one Shift key wakes it. Record in `results.md` that the restart was made.

### Rules

- Nothing here runs on the dev PC, not even as a dry run. Every guest script refuses to run on a machine whose name does not start with `CMDSTUDIO-` (exit code 99 for the `.vbs` files). Step 0's `.vbs` and `read-autorun.vbs` start no program: each reads and writes one text file under `C:\cmdstudio\out\phase3\`.
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
| `step0-wt-state.vbs` | Step 0: Windows Terminal's state, written to a text file, no console |
| `read-autorun.vbs` | the AutoRun value of cmd (type and text) written to a text file, no console, in the user's own session; `AutoRun: (read failed)` when the registry could not be read, which is never printed as `(absent)` |
| `start-app.vbs` | starts the app with no console; `debug` also sets the CSP check's debug port, for the app's process only |
| `exit-code.vbs` | runs one command like the uninstaller's `ExecWait` (hidden, no console, waiting) and writes the exit code to a file; a token `@NAME@` in an argument becomes the value of the variable NAME in the user's environment, quoted as one argument |
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

VBoxManage can send keys but not the mouse, so the app is driven with the keyboard. **A click here is one activation** of a control: Enter or Space on a focused button. Moving focus with Tab is not a click. The host keeps a tally of what it sent: `Reset-InputTally` before the first key of a walk, `Get-InputTally` after it, which prints `N activations, N characters, N navigation keys, N other keys`. Enter and Space count as activations, Tab and the arrows as navigation, the characters of `Send-Text` as characters. `results.md` states the gate walk's tally in those words, for example "5 activations, 0 characters, N navigation keys". The tally says what the host sent; what confirms the word "click" is a walk with the mouse by the founder on the VM window, which the keyboard walk and the unit test in `App.test.tsx` stand in for.

- `Send-Key $vm Tab 4` presses Tab four times; `Send-Key $vm Enter` activates the focused control. Keys: `Tab`, `ShiftTab`, `Enter`, `Space`, `Esc`, `Up`, `Down`, `Left`, `Right`, `AltF4`, `WinD`, `WinSpace`, `CtrlA`, `CtrlV`.
- To type text use `Send-Text $vm 'text' -Enter`. It quotes the text itself, which `Send-Run` cannot do under Windows PowerShell 5.1. Use `Send-Run` only for text without double quotes.
- The pages put focus on their heading when they come, and Tab goes on from there. The counts below come from the source (`Wizard.tsx`, `App.tsx`, `Help.tsx`, `Settings.tsx`). They are hints: a note in a plan, or a preview that Chromium makes a focusable scroller, adds a stop. Run 1 confirms them, and `results.md` records the counts that held.

  | Where | Press | Reaches |
  |---|---|---|
  | Page 1, window just opened | Tab 4 | Next (after English, ไทย, Skip) |
  | Page 1 with ไทย first (Run 6) | Tab 2, Space, Tab 2 | ไทย, then Next (after Skip) |
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
- If the founder or the controller has the VM's window open (`Start-CleanVm $vm $u -Type separate`), do the walk once with the mouse as well and write the number of clicks in `results.md`. That is the criterion's own word.

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

A line starting `FINDING:` (the `wt.exe` alias is there but no package folder) means Windows Terminal is installed for the user and the app reads it as not installed. **The criterion 1 gate cannot pass on that VM**: page 2 offers the Store for a machine that has Terminal, the plan warns "Windows Terminal is not installed: only the prompt and the welcome box change", and the apply sets nothing up in Terminal. This is a defect of the app, not a VM problem: `WinWt::locations` in `crates/core/src/adapters/windows/wt.rs` calls a packaged Terminal installed when `dir.is_dir()` holds for its package folder. The finding goes to the controller before the PR, with the Step 0 file. Run 2 can still be run, to record what the app does on such a machine; its gate items are recorded FAIL, not skipped, and Runs 3 and 4 are not run.

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
   - **Page 1 (check 14).** Tab to "ไทย", Space: the whole page turns Thai at once ("ขั้นที่ 1 จาก 6") and the pressed button shows an accent border and bold text (`styles.css`, `.row-actions > button[aria-pressed="true"]`). Screenshot `r1-p1-th`. Shift+Tab to "English", Space: English again. Screenshot `r1-p1-en`. Read which button is pressed in each. Also count the Tab presses of the path Run 6 uses: from the window just opened, Tab 2 reaches "ไทย"; after Space on it, Tab 2 more reach Next (after Skip). Leave the language on English.
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
6. **Visit every screen, so that every kind of style and dialog has been drawn, and probe the counts that later runs use.** Home; Themes (filter "Dark", "Light", "All"; press "Customize" on the card named "Terracotta", then "Back to themes"; open Rename and Delete on the card named "My Terracotta" and "Import a theme file", and close each with Esc); Settings; Help with all six tabs (on "Restore points" open "Go back to this point" and close it; on "Undo everything" open the confirmation and close it); Advanced. Keep a list of what was opened. With `Get-GuestFocus`, record the Tab or ShiftTab counts that Run 2 steps 12, 13, 15 and 17 and Run 6 step 2 rely on:
   - from the heading of Home: ShiftTab to each of Advanced, Help, Settings, Themes and Home, and ShiftTab to Dismiss and to "Undo" in the bar;
   - Settings: Tab to the language radios while this theme is applied (4 expected);
   - Help: Tab to each of the six tabs; on "Report a problem" the Tab count from the tab to "Copy details" (3 expected) and to "Open GitHub Issues" (4);
   - Advanced: Tab to "Open the data folder" and "Open the log folder";
   - Themes: Tab from the heading to "Create a theme" and to "Import a theme file", and in the editor to "Back to themes" and to the "Light" tone radio;

   Last, press "Undo" in the bar and record the Settings count again (3 expected, because no theme is applied).
7. **Read the console as text (check 1).** The page's console is read over the Chrome DevTools Protocol, inside the guest, on the debug port of step 2. `Get-GuestConsoleLog` gets `http://127.0.0.1:9222/json`, takes the page's `webSocketDebuggerUrl`, connects with `System.Net.WebSockets.ClientWebSocket`, sends `Runtime.enable` and `Log.enable` (both replay what the page logged before they were sent), collects for 10 seconds, and writes every message to `C:\cmdstudio\out\phase3\console.txt`. A message is taken whole (its bytes are joined and decoded as UTF-8 once, at its end, so Thai is not split across frames). A message that is not JSON is written to the file raw and skipped, and the collection goes on; the 10 seconds last whatever happens.

   **Run `Get-GuestConsoleLog` once per app start.** Its second control makes the page refuse an inline `<style>`, and that refusal stays in the page's console. A second call in the same app start finds the refusal in its own first window and counts a violation that the app did not make. For another read, close the app and start it again with `start-app.vbs debug`.

   ```powershell
   Get-GuestConsoleLog $vm $u -Seconds 10
   Get-GuestText $vm "$($P3.Out)\console.txt" $u
   ```

   PASS: `Runtime.enable and Log.enable answered: True`; the count of messages in the first window that mention "Content Security Policy" or "Refused to" is 0 (no `style-src`, `connect-src` or `script-src` violation, and no error line in the file). It was read after step 6 had drawn every screen, so a blocked inline style would be in it. The function ends with four controls, so that "no message" cannot mean "nothing was read":

   - **Live delivery.** The page logs a probe line (`control 1 … True`) and creates an inline `<style>`, which the CSP must refuse and report (`control 2 … True`).
   - **Replay.** The first window's "nothing" rests on `Runtime.enable` and `Log.enable` replaying what the page logged before the connection, and the live controls do not test that. So the function then reconnects once, sends both again, and requires that the earlier probe line (`control 3 … True`) and the earlier refused `<style>` (`control 4 … True`) come back as replayed entries, and that both domains answered the second time. The line `replay proven … True` says all of it.

   If a control says `False`, or the line says `replay proven … False` (and `REPLAY NOT PROVEN` follows), the read proves nothing: **the PASS cannot be given**. Record "replay not proven" (or the control that failed), keep `console.txt`, and use the visible proxies instead. The same fallback applies if the debug port or the read cannot be had at all: the colors are painted (`Measure-Color` of the app's surface color in the screenshots of every screen is not zero) and no area of any screenshot is blank. The debug port exists only on this VM and only for this run; no devtools feature is added to the app. `Get-GuestConsoleLog` has been parsed, not run: this is its first run.
8. **Close.** `Send-Key $vm AltF4`, and wait until `Get-GuestProcesses $vm $u` lists no `cmdstudio` and no `msedgewebview2` of the app (lesson 3).

Record in `results.md`: the Tab counts that held, the console result with its four controls (or "replay not proven"), and the page-6 text.

## Run 2 — VM-A: the gate (criterion 1)

Restore `clean`. The Terminal is in the state Step 0 recorded. On a never-opened Terminal, steps 1 to 5 are **console-free**: only guestcontrol, the `.vbs` files and the app.

1. **Restore, install, record.** `Start-CleanVm $vm $u`; `Install-Phase3App $vm $u $artifact`; run Step 0's `.vbs` again and compare with the recorded file. Then:

   ```powershell
   $before = Get-GuestFacts $vm $u -NoConsole                 # Terminal, fonts and data folder before the apply
   $autoBefore = Get-GuestAutoRun $vm 'before' $u              # AutoRun, by read-autorun.vbs: no console
   $autoBefore
   Invoke-GuestPs $vm "Test-Path -LiteralPath '$data'" $u      # False: the app has not started yet
   ```

   `Get-GuestAutoRun` types a Win+R line, so on VM-C press `Send-Key $vm WinSpace` first (Run 6). Its line is `AutoRun: (absent)`, `AutoRun: [REG_SZ] …` or `AutoRun: [REG_EXPAND_SZ] …`. `AutoRun: (read failed)`, and a missing result file, are **no reading** (the function warns for either): two of them are equal text, so a comparison of them proves nothing. Read again before going on. `(absent)` is believed only once step 5 has shown that the same script reads a value.

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
   Reset-InputTally                                    # from here every key and character the host sends is counted
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
   Get-InputTally                                       # the walk's tally
   $autoApplied = Get-GuestAutoRun $vm 'applied' $u      # AutoRun right after the apply: the positive control
   $autoApplied
   Get-GuestTiming $vm $u -NoConsole
   Get-GuestText $vm "$data\settings.json" $u
   Get-GuestText $vm "$data\state.json" $u
   Get-GuestFacts $vm $u -NoConsole
   Get-GuestText $vm "$wtState\settings.json" $u
   Invoke-GuestPs $vm "Select-String -LiteralPath '$data\logs\cmdstudio.log' -Pattern 'cmd=apply'" $u
   ```

   - **The tally.** PASS: `Get-InputTally` prints `5 activations, 0 characters, N navigation keys, 0 other keys`. Write that line, with N, in `results.md` as "5 activations, 0 characters, N navigation keys".
   - **AutoRun, the positive control.** PASS: `$autoApplied` starts `AutoRun: [REG_SZ]` or `AutoRun: [REG_EXPAND_SZ]` and holds CMD Studio's entry: `if exist "<exe>" "<exe>" inject --autorun --quiet --profile "<dir>"` (the exe under the data folder's `runtime\`, or the short or ProgramData path that `state.json`'s `launch` names). This is what shows that `read-autorun.vbs` can read a value at all. If it says `(absent)` or `(read failed)` here, the apply set no AutoRun or the script cannot read one: either way the AutoRun comparisons of steps 16 and 18 (and Run 3 step 5) prove nothing, and that is recorded, not passed. The Terminal-only apply of Run 1 leaves AutoRun alone, so `(absent)` is its expected reading, not this one.
   - **Timing (check 3).** PASS: "seconds from the first window to Done" is under 120. The host pressed its keys faster than a person would, so a result above 60 is flagged in `results.md` even when it passes.
   - **The app's settings.** `settings.json` holds `"wizardDone": true`, `"language": "en"`, `"target": "all"` and `"setWtDefaultProfile": true`.
   - **The apply.** `state.json` names Terracotta as applied. The log has a `cmd=apply` line that ends `ok`.
   - **Terminal's file, by Step 0's state.**
     - `neverOpened`: `settings.json` now exists and is CMD Studio's file. It holds `$schema`, a `schemes` entry named "CMD Studio" with `"background": "#1f1e1d"`, the cmd profile `{0caa0dad-35be-5f56-a8ff-afceeeaa6101}` with `"colorScheme": "CMD Studio"`, and `defaultProfile` set to that guid; `state.json` holds `"fromTemplate": true` for it. If Step 0 said `LocalState` was absent, it exists now.
     - `present`: the file is the one `$before` hashed, with the same keys added and the rest untouched.
6. **Try cmd (the first Terminal start, on a never-opened one).** Press "Try cmd" (Tab 1 from the heading of page 6, then Enter; this is not a sixth click of the walk; if step 5's Win+R read of AutoRun moved the focus, which `Get-GuestFocus $vm $u` shows, press it with `Invoke-GuestButton $vm 'Try cmd' $u` instead). Screenshots at once, three times, then again after 10 seconds.

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
   foreach ($c in '#5F5D58', '#D97757') { '{0}: before {1}, now {2}, rise {3}' -f $c, (Measure-Color $base $c), (Measure-Color $wt $c), ((Measure-Color $wt $c) - (Measure-Color $base $c)) }
   ```

   PASS, on two conditions together:
   - `Get-GuestCmdWindows` lists exactly one `cmd.exe` (the tab of step 6 is the only one) and Clink's DLL is loaded in it (`Clink loaded: clink_dll_x64.dll`);
   - the frame color `#5F5D58` (the welcome box and the prompt's box) and the accent `#D97757` each **rise** over `$base`, the screenshot taken before "Try cmd", by at least 500 and 200 pixels. The rise matters, not the count: page 6's own preview in the app paints both colors, so a count alone could be met by the wrong window. The floors are well under what a 44-cell box draws (its top and bottom lines alone are about 800 pixels); record the real numbers, and raise a floor if they are far above it. The lines and letters are drawn in full-coverage pixels, which acrylic does not blend. The near count of step 6 holds as well.

   The screenshot shows the Terracotta welcome box with the box-layout prompt in English, the frame's right edge in one column, and `✻` on the tab. Keep `r2-wt` for `results.md`.

   **If a rise falls short of its floor,** the floor is not the verdict. Record the three numbers for each color (before, now, rise) and judge from the screenshot `r2-wt`: the Terracotta welcome box with its frame, and the prompt in the accent, in the window of step 6 and in no other window, is a PASS; `results.md` then says that the rise was short and why (a smaller window, another resolution or DPI than the floors were set for). A box that is missing or cut, colors that are not Terracotta's, or a window that is not the tab of step 6 is a FAIL, with the numbers. Write which way it went and why. A short rise alone is not an app FAIL.
8. **A second cmd window (gate).** Close the window of step 6 first (`Send-Key $vm AltF4`; it is in front), so that the only cmd that can be found is the new one, and take a baseline of what is on the screen now. Then type `cmd` in Win+R, as a person does.

   ```powershell
   Send-Key $vm AltF4
   Start-Sleep -Seconds 3
   $pre = Save-Shot $vm 'r2-before-second'
   Send-Run $vm 'cmd'
   Start-Sleep -Seconds 8
   $second = Save-Shot $vm 'r2-second'
   Get-GuestCmdWindows $vm $u
   foreach ($c in '#5F5D58', '#D97757') { '{0}: before {1}, now {2}, rise {3}' -f $c, (Measure-Color $pre $c), (Measure-Color $second $c), ((Measure-Color $second $c) - (Measure-Color $pre $c)) }
   '{0}: scheme near {1}, before {2}' -f (Split-Path $second -Leaf), (Measure-ColorNear $second '#1F1E1D'), (Measure-ColorNear $pre '#1F1E1D')
   ```

   Both guards are used: the old window is closed, so exactly one `cmd.exe` is expected, and the new window must also raise the counts over `$pre`, taken just before Win+R. PASS, on these together:
   - `Get-GuestCmdWindows` lists exactly one `cmd.exe`, and Clink's DLL is loaded in it (AutoRun injected it);
   - the frame and accent counts of `$second` rise over `$pre` by at least 500 and 200 pixels, as in step 7 (`$pre` shows the app with page 6's preview, which has both colors, so the rise is the new window's);
   - the near count of `#1F1E1D` rises by at least 100000 over `$pre`.

   **If a rise falls short of a floor,** it is judged as in step 7: record the numbers (before, now, rise) for each count, judge from the screenshot `r2-second` (the box and the prompt in Terracotta's colors, in the one new window), write which way it went and why. A short rise alone is not an app FAIL; the `cmd.exe` and Clink condition above is not a floor and stands.

   On Windows 11 this window opens in Windows Terminal (the default terminal), which maps it to the cmd profile (G7): its colors come from the profile and its prompt and box from AutoRun.
9. **Home.** Close the Terminal window of step 8 (`Send-Key $vm AltF4`; it is in front, and the app is behind it), then press "Go to the home page" (Tab 2 from the heading of page 6, or `Invoke-GuestButton $vm 'Go to the home page' $u`). Screenshot `r2-home`.

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
      Send-Text $vm ">>C:\cmdstudio\out\phase3\exit-in-cmd.txt echo $name=%ERRORLEVEL%" -Enter
    }
    Start-Sleep -Seconds 5
    Get-GuestText $vm "$($P3.Out)\exit-in-cmd.txt" $u
    ```

    - **Open question 1 (spec appendix B, phase 2b).** `cmdstudio.exe doctor` is a Windows-subsystem exe, so cmd does not wait for it. Record what `r2-doctor-typed` shows: the prompt back first and the report after it, or the report first.
    - **`doctor > out.txt` (check 4).** PASS: `doctor.txt` holds the whole report: a line for each of `D1` to `D11`, each `ok`, `warn`, `FAIL` or `skip`; `help.txt` holds the usage text ("Usage:", the commands, "Exit codes:"). If a file is empty right after the prompt came back, wait and read again, and record the delay.
    - **Exit codes in cmd (check 2).** The redirection comes first on purpose: in `echo help=0>>file` cmd reads the `0` right before `>>` as a handle number, writes `help=` to the screen and an empty file (checked with `cmd.exe /d /c` against a scratch file). With `>>file echo help=0` the file holds exactly `help=0`, `doctor=0`, `bad=2`, one per line. PASS: `help=0`, `bad=2`, and `doctor=0` (or `1` when a check failed, which must then be a `FAIL` line of `doctor.txt`).
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
    - Press "Customize" on the card named "Terracotta" (a card's buttons are a group labeled with the theme's name; `Invoke-GuestButton $vm 'Customize' $u` presses the first match, which is Terracotta's because the presets are listed in order). The editor opens with the name "My Terracotta". Go "Back to themes" (Tab 1 from the editor's heading, or `Invoke-GuestButton $vm 'Back to themes' $u`): "My Terracotta" is now a card under "My themes". On the card named "My Terracotta" press "Export" (the only Export button on the page): the Windows Save dialog opens. Type the path:

      ```powershell
      Send-Key $vm CtrlA
      Send-Text $vm 'C:\cmdstudio\out\phase3\export-test.cmdtheme' -Enter
      Get-GuestText $vm "$($P3.Out)\export-test.cmdtheme" $u
      ```

      PASS: the page says "Saved to C:\cmdstudio\out\phase3\export-test.cmdtheme"; the file holds `"schemaVersion": 1` and `"name": "My Terracotta"`.
    - "Import a theme file": the Windows Open dialog opens. `Send-Text $vm 'C:\cmdstudio\out\phase3\export-test.cmdtheme' -Enter`. PASS: the dialog "Import this theme?" appears with a preview and "You already have this theme; the import is added as a separate theme."; press "Add to my themes"; `Invoke-GuestPs $vm "Get-ChildItem -LiteralPath '$data\themes' | ForEach-Object { `$_.Name }" $u` lists two files. Opening the dialog again and pressing Esc says "No file was chosen."
15. **Thai UI, and cmd stays English (check 10).** In the app, Settings → ไทย: Tab 4 to the language radios (a theme is applied), Down. The whole UI turns Thai at once.
    - Go Home. Screenshot `r2-home-th` (the Thai home page: "หน้าแรก", "ธีมที่ใช้อยู่"). `Get-GuestText $vm "$data\settings.json" $u` holds `"language": "th"`.
    - Press "เปิด cmd" (Thai for "Open cmd"). A new Terminal tab opens. Screenshot `r2-th-cmd`. PASS: its welcome box and tips are English; `Get-GuestCmdWindows` shows Clink in the new cmd. Close that window (`Send-Key $vm AltF4`).
    - ```powershell
      Invoke-GuestSnippet $vm "Get-ChildItem '$data\generated' -Recurse -File | ForEach-Object { `$t = (Get-Content -LiteralPath `$_.FullName -Raw -Encoding UTF8) -replace [regex]::Escape(`$env:USERNAME), ''; 'thai letters in {0}: {1}' -f `$_.Name, (`$t -match '[\u0E00-\u0E7F]') }" $u
      ```
      PASS: `False` for every generated file. Run step 11's `doctor > doctor.txt` again with the UI in Thai and test it:
      ```powershell
      ("$(Get-GuestText $vm "$($P3.Out)\doctor.txt" $u)" -replace [regex]::Escape($u), '') -match '[\u0E00-\u0E7F]'
      ```
      PASS: `False`. The user name is taken out of both texts first, because on VM-C it is Thai and sits inside paths.
    - Settings → English (Tab 4, Up on the radio): the UI is English again. Undo is pressed in English in the next step.
16. **Undo (gate).** How the press is made depends on what Step 0 recorded.
    - `present`: in the app, from any page, ShiftTab 7 from the heading reaches "Undo" in the bar; Enter.
    - `neverOpened`: **the gate's "Undo" item is judged on the open-window path of 16a**, with a Terminal window still open, because that is how a person who pressed "Try cmd" meets it. Run 3 is the other path, with no Terminal ever started, and does not stand in for this one. Do 16a, then go on with the checks below.

    **16a. Undo with a Terminal window still open (`neverOpened` only; it comes before the Undo).** A running Terminal holds its settings in memory and may write them back after the undo has taken away the file CMD Studio made. `settings_terminal_made_again` in `crates/core/src/exec.rs` accepts a file that a running Terminal wrote again right after the undo deleted it, when that file holds nothing of CMD Studio's. Whether it also covers a Terminal that writes back the settings it holds in memory, the scheme "CMD Studio" among them, has not been seen on a real Terminal. This sub-step sees it.
    1. **After "Try cmd", leave the Terminal tab open.** The tab of step 6 was closed in step 8, so open it again with the button that does what "Try cmd" does, "Open cmd" on Home. The keys would go to whatever is in front, so press by name. Wait ten seconds, and leave the window open.

       ```powershell
       Invoke-GuestButton $vm 'Home' $u
       Invoke-GuestButton $vm 'Open cmd' $u
       Start-Sleep -Seconds 10
       Save-Shot $vm 'r2-undo-wt'
       Get-GuestProcesses $vm $u                        # must list WindowsTerminal
       ```
    2. **Go to the home page.** The app is behind the Terminal window, and Terminal stays open.

       ```powershell
       Invoke-GuestButton $vm 'Home' $u                 # a press on the page that is showing, harmless
       Save-Shot $vm 'r2-undo-home'                     # Home, and the bar above it with "Undo"
       ```

       If the names find nothing, bring the app forward with a second start (`Send-Run $vm 'C:\cmdstudio\phase3\app\cmdstudio.exe'`; step 10 shows that it focuses the first window) and press Undo with ShiftTab 7 from the heading, as in the `present` path.
    3. **Press the bar's Undo,** with the Terminal window still open, and read what is left:

       ```powershell
       Invoke-GuestButton $vm 'Undo' $u
       Start-Sleep -Seconds 20
       Save-Shot $vm 'r2-undo-open'
       $wtFile = "$wtState\settings.json"
       $probe = "if (Test-Path -LiteralPath '$wtFile') { 'settings.json present: ' + (Get-Item -LiteralPath '$wtFile').Length + ' bytes, CMD Studio occurs ' + (Select-String -SimpleMatch 'CMD Studio' -LiteralPath '$wtFile' | Measure-Object).Count + ' time(s)' } else { 'settings.json absent' }"
       Invoke-GuestPs $vm $probe $u                      # at once
       Get-GuestText $vm $wtFile $u                      # its text, whole
       Start-Sleep -Seconds 10
       Invoke-GuestPs $vm $probe $u                      # ten seconds later: Terminal may write after the undo has checked
       Get-GuestText $vm "$data\state.json" $u           # names no applied theme when the undo committed
       Get-GuestProcesses $vm $u                         # Terminal is still running
       ```

       Then close the Terminal window (`Send-Key $vm AltF4`; it is in front) and read `$probe` once more: Terminal may write on its way out. The checks below start from here, with Terminal closed.

    Record in `results.md` as R2.16a:
    - whether the Undo **committed or rolled back**, and the outcome text on the bar from `r2-undo-open`: "Undone: everything is as it was before." for a commit; "The undo did not go through, so nothing changed: the theme is still applied." for a roll-back;
    - what `settings.json` of Terminal held afterwards, at each of the three readings: **absent**, **Terminal's defaults** (present, "CMD Studio occurs 0 time(s)"), or **still holding "CMD Studio"** (a count above 0);
    - whether **`E_VERIFY_READBACK`** appeared (on a roll-back the bar's error shows "Code E_VERIFY_READBACK" under "Show details").

    PASS: the Undo committed, and the file is absent or holds no CMD Studio part at all three readings; `E_VERIFY_READBACK` did not appear.

    **If it rolled back,** or committed and Terminal then wrote CMD Studio's scheme back, record it as a **finding for the controller**, with the whole text of the file (`Get-GuestText` above, saved into `results.md`). It decides whether `settings_terminal_made_again` covers a running Terminal that writes back its in-memory settings. The gate's Undo item is FAIL on this path; nothing is repaired on the VM, and the other bullets below are recorded for the state it left (for example Home still showing the theme).

    ```powershell
    $autoAfter = Get-GuestAutoRun $vm 'after' $u
    $after = Get-GuestFacts $vm $u
    $skip = '^(data|AutoRun|user font registrations|WT LocalState entry)'    # the user fonts folder line is compared
    Compare-Object ($before -notmatch $skip) ($after -notmatch $skip)
    $autoAfter -eq $autoBefore
    $after | Where-Object { $_ -like 'user font registrations*' }
    ```

    PASS:
    - the bar reads "Undone: everything is as it was before." and Home reads "No theme is applied yet.";
    - `$autoAfter -eq $autoBefore` prints `True`: both read AutoRun with `read-autorun.vbs` (type and text), so this is a real comparison, and on a fresh VM both are `AutoRun: (absent)`. It counts only if neither line says `(read failed)` and step 5's `$autoApplied` showed a value, so that `(absent)` is a reading and not a script that cannot read;
    - the fonts: when page 5 listed "Install 1 font for this account only", the `user fonts folder:` line is in the comparison and `user font registrations` in `$after` names no CaskaydiaCove. The undo compares the registration; a font file that Windows Terminal still holds is deleted later (`exec.rs` `same_state`), so a lone file with no registration is recorded, not failed. A font the user had installed over CMD Studio's stays, and is not a failure;
    - `present`: Terminal's `settings.json` has the hash that `$before` has, and `Compare-Object` prints nothing;
    - `neverOpened`: judged by 16a, which was done with a Terminal window open: the undo committed, and the file CMD Studio made is gone, or, because that Terminal was running and wrote a file again, `Select-String -SimpleMatch 'CMD Studio'` finds nothing in it (the `$before` and `$after` hashes of the Terminal file then differ, and that difference is Terminal's own file, not a failure). `LocalState` is removed only if CMD Studio made it and it is empty: after a Terminal has run it holds Terminal's own files, so it stays. Run 3 checks the case where it goes;
    - Help → Restore points lists the apply with the badge "Undone" and without a "Go back to this point" button, and `state.json` in the data folder names no applied theme;
    - **a new cmd window is plain again.** Open one and compare it with a baseline taken just before:

      ```powershell
      $pre = Save-Shot $vm 'r2-before-plain'
      Send-Run $vm 'cmd'
      Start-Sleep -Seconds 8
      $plain = Save-Shot $vm 'r2-plain'
      Get-GuestCmdWindows $vm $u
      foreach ($c in '#5F5D58', '#D97757') { '{0}: before {1}, now {2}, rise {3}' -f $c, (Measure-Color $pre $c), (Measure-Color $plain $c), ((Measure-Color $plain $c) - (Measure-Color $pre $c)) }
      '{0}: scheme near {1}, before {2}' -f (Split-Path $plain -Leaf), (Measure-ColorNear $plain '#1F1E1D'), (Measure-ColorNear $pre '#1F1E1D')
      ```

      PASS: no welcome box and no themed prompt (`r2-plain` shows Windows' own header and a plain `C:\…>` prompt); `Get-GuestCmdWindows` lists exactly one `cmd.exe` with `Clink loaded: no`; the frame and accent counts rise by less than the floors of step 7 (500 and 200); the near count of `#1F1E1D` rises by less than 5000. Close the window (`Send-Key $vm AltF4`) before the next key for the app.
17. **The language is remembered.** Settings → ไทย (Tab 3 now that no theme is applied, Down). Close the app (Alt+F4), wait for no `msedgewebview2` of the app (lesson 3), start it with `Send-Run` as in step 2. PASS: it opens in Thai ("หน้าแรก"). Settings → English (Tab 3, Up), close, start: it opens in English.
18. **Close and clean up (checks 2 and 8).** Close the app (Alt+F4), wait for no `msedgewebview2.exe` of the app (lesson 3; `Get-GuestProcesses`), then run `--cleanup` as the uninstaller would:

    ```powershell
    Get-ExitCode $vm 'cleanup' '--cleanup --data-dir @LOCALAPPDATA@\ai.resincore.cmdstudio' $u
    Get-GuestFacts $vm $u
    Get-GuestAutoRun $vm 'cleanup' $u
    ```

    The token `@LOCALAPPDATA@` is replaced by `exit-code.vbs`, in the user's environment, and the whole path is quoted as one argument; the Run dialog never sees `%…%`, and the host never types a Thai path. PASS: `exit=0`. The data folder is gone except `themes\`, which holds the saved themes of step 14: `drafts\`, `logs\`, `webview\` and `settings.json` were accepted as CMD Studio's own. An exit 2 that names `EBWebView\` or another entry means the app put something in its data folder that `--cleanup` does not know. AutoRun is `(absent)` (a `(read failed)` line is no reading).
19. **After the run.** Copy the screenshots named in "Results" into the docs folder. Do not take a snapshot: the next run restores `clean`.

## Run 3 — VM-A: a Terminal that was never opened, undone before it opens

Only if Step 0 said `neverOpened`. The point is the undo of a file that CMD Studio made, with no Terminal running to write it again. **Everything is console-free until the last step.** The undo with a Terminal window still open, which a person who pressed "Try cmd" meets, is Run 2 step 16a, and the gate's "Undo" item is judged there; this run does not stand in for it.

1. Restore `clean`; `Install-Phase3App`; Step 0's `.vbs` again (the same facts as before); `$before = Get-GuestFacts $vm $u -NoConsole`; `$autoBefore = Get-GuestAutoRun $vm 'before' $u` (no console).
2. Start the app and walk the five clicks exactly as in Run 2 step 4. At page 6 do **not** press "Try cmd": press "Go to the home page" (Tab 2).
3. `$applied = Get-GuestFacts $vm $u -NoConsole`, `$autoApplied = Get-GuestAutoRun $vm 'applied' $u` (no console) and `Get-GuestText $vm "$wtState\settings.json" $u`. PASS: the file is CMD Studio's, as in Run 2 step 5, and `state.json` holds `"fromTemplate": true`. Record that `LocalState` was made, if Step 0 said it was absent. `$autoApplied` is the same positive control as in Run 2 step 5: `AutoRun: [REG_SZ]` or `[REG_EXPAND_SZ]` with CMD Studio's entry (`inject --autorun --quiet --profile`); `(absent)` or `(read failed)` here means the comparison of step 5 proves nothing.
4. Press "Undo" (ShiftTab 7 from the Home heading, Enter). The AutoRun read of step 3 used Win+R, which takes the focus for a moment: take `Save-Shot $vm 'r3-before-undo'` first and check that Home with the bar is showing. No desktop reading may be used here (Terminal must stay unopened), so there is no `Invoke-GuestButton`.
5. ```powershell
   $autoAfter = Get-GuestAutoRun $vm 'after' $u
   $after = Get-GuestFacts $vm $u -NoConsole
   $skip = '^(data|AutoRun)'                      # the user fonts folder line is compared
   Compare-Object ($before -notmatch $skip) ($after -notmatch $skip)
   $autoAfter -eq $autoBefore
   Get-GuestProcesses $vm $u
   ```
   PASS: nothing differs, so the file CMD Studio made is gone, and `$autoAfter -eq $autoBefore` prints `True`. If Step 0 said `LocalState` did not exist, it is gone too; if it existed, it is still there as it was. This is where "removed only if CMD Studio made it and it is empty" is seen. Nothing has started Terminal: `Get-GuestProcesses` lists no `WindowsTerminal`.
6. Now open Terminal as a person would, to see that the undo left it never-opened. The baseline is a screenshot taken just before, the Home page of the app, and the threshold is stated:

   ```powershell
   $pre = Save-Shot $vm 'r3-before-wt'
   Send-Run $vm 'wt'
   Start-Sleep -Seconds 12
   $d = Save-Shot $vm 'r3-wt-default'
   '#1F1E1D near: before {0}, now {1}, rise {2}' -f (Measure-ColorNear $pre '#1F1E1D'), (Measure-ColorNear $d '#1F1E1D'), ((Measure-ColorNear $d '#1F1E1D') - (Measure-ColorNear $pre '#1F1E1D'))
   Get-GuestProcesses $vm $u
   Invoke-GuestPs $vm "if (Test-Path -LiteralPath '$wtState\settings.json') { 'settings.json present, CMD Studio occurs ' + (Select-String -SimpleMatch 'CMD Studio' -LiteralPath '$wtState\settings.json' | Measure-Object).Count + ' time(s)' } else { 'settings.json is MISSING' }" $u
   ```

   PASS: a `WindowsTerminal` process is up; the near count (±12) of `#1F1E1D` rises by **less than 5000** over `$pre` (a Terminal with CMD Studio's scheme would raise it by 100000 or more, as in Run 2 step 6); Terminal wrote its own `settings.json` (the line says `present`), in which "CMD Studio" occurs 0 times. **A missing file is its own result,** never a 0 that passes: the line says `settings.json is MISSING`, which counts nothing. Then this part is neither PASS nor FAIL: record "settings.json missing after Terminal started" with the process list and the screenshot, and send it to the controller. Record the new background color Terminal paints (Campbell `#0C0C0C` or another default scheme), read from the screenshot.

## Run 4 — VM-A: Terminal's first start with a pre-written file

Only if Step 0 said `neverOpened`. This is the question that spec §5.4 still marks as unconfirmed on a VM, answered with a hand-written file so that it does not depend on CMD Studio. It tells a failure of Run 2 step 6 that is Terminal's from one that is CMD Studio's.

**Do not start `run.ps1`.** Its own header says to run it as `powershell -NoProfile -ExecutionPolicy Bypass -File`, and the run rules forbid bypassing the execution policy; it also restores `clean`, which removes any user made on the boot (the runner of this phase made new users for Runs 3 and 4). Run its steps inline, in the session of Setup, where `vm.ps1` and `helpers.ps1` are already dot-sourced. `wt-precreate.ps1` and `run-on-desktop.ps1` are in the VM already after `Install-Phase3App`. `Start-CleanVm` is the restore of `run.ps1`: leave it out when the VM is already on a never-opened Terminal that you want to keep (a new user, or the boot after a restore), and say so in `results.md`.

```powershell
$vm = 'cmdstudio-win11'; $u = 'tester'
Start-CleanVm $vm $u                       # leave out on a boot to keep; then Install-Phase3App $vm $u $artifact
$g = "$($P3.Check)\wt-precreate.ps1"
New-Item -ItemType Directory -Force $Shots | Out-Null
$log = Join-Path $Shots 'wt-precreate.log'
& {
  '## state'; Invoke-GuestDesktop $vm $g '-Step state' $u
  '## write'; Invoke-GuestDesktop $vm $g '-Step write' $u
  '## start'; Invoke-GuestDesktop $vm $g '-Step start' $u
  Start-Sleep -Seconds 12
  $shot = Save-Shot $vm 'wt-precreate'
  "screenshot: $shot"
  "background #1F1E1D pixels: $(Measure-Color $shot '#1F1E1D')"
  '## read'; Invoke-GuestDesktop $vm $g '-Step read' $u
  '## close'; Invoke-GuestDesktop $vm $g '-Step close' $u
} *>&1 | Tee-Object -FilePath $log
```

The steps write a `settings.json` with a "CMD Studio" scheme (background `#1F1E1D`, no opacity, no acrylic) on the cmd profile, start Terminal, take a screenshot, print the exact count of `#1F1E1D`, print the file as Terminal left it, and close Terminal. The log is `spikes\vm\tmp\wt-precreate.log`. Its first lines (`state`) are read on the desktop; they must show the same facts as Step 0 (the package folder and `LocalState` as Step 0 had them, no `settings.json`). If they show more, the desktop task itself opened Terminal, and the never-opened results of that boot are void.

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

1. **Setup.** Step 0 on VM-C (its Terminal may differ from VM-A's), `Install-Phase3App`, `$before = Get-GuestFacts $vm $u -NoConsole`, `$autoBefore = Get-GuestAutoRun $vm 'before' $u` (after `Send-Key $vm WinSpace`).
2. **Run 2 steps 2 to 8 and 11, 12 and 18**, with the same commands, recorded for VM-C. The deviations:
   - On page 1 the app is English although Windows is Thai ("Make your cmd window beautiful and easy to read"). Press "ไทย" first (one extra activation, not part of the gate: the gate is VM-A's English walk) and walk the pages in Thai: on page 1 `Send-Key $vm Tab 2`, `Send-Key $vm Space`, `Send-Key $vm Tab 2`, `Send-Key $vm Enter` (the count Run 1 step 3 confirmed), then the keys of Run 2 step 4 for pages 2 to 5. The tally of this walk is therefore 6 activations; write it as such. The Thai texts to read are th.ts's: page 2, "มี Windows Terminal แล้ว หน้าต่าง cmd ของคุณจะได้สีของธีม" or, when Step 0 said `neverOpened`, "มี Windows Terminal แต่ยังไม่เคยเปิดใช้ เมื่อใช้ธีม CMD Studio จะตั้งค่าให้เอง"; page 5, the button "ใช้ธีม"; page 6, "เสร็จแล้ว", "ใช้ธีมแล้ว เปิด cmd หน้าต่างใหม่เพื่อดูกล่องต้อนรับกับพร้อมท์" and the buttons "ลองเปิด cmd" and "ไปหน้าแรก". The after-apply bar's button is "ย้อนกลับ" and its result "ย้อนกลับแล้ว ทุกอย่างเป็นเหมือนเดิม".
   - From page 5 and `state.json` also record whether the plan has the line "Keep the add-on outside your user folder, because your user folder's name has letters it can't use", and which `launch` kind `state.json` holds (`direct`, `shortPath` or `programData`). The Thai name must not stop the apply. In step 18 the data folder is `C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio`; the host types the token `@LOCALAPPDATA@`, and `exit-code.vbs` expands it in the user's environment and quotes the whole path as one argument, so the host never types a Thai path or a `%…%` (check 2 with a path that has a space).
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

1. `Start-CleanVm`, `Install-Phase3App`, Step 0 (expected: not installed, and no `FINDING:`), `$before = Get-GuestFacts $vm $u -NoConsole`, `$autoBefore = Get-GuestAutoRun $vm 'before' $u`.
2. **Run 2 steps 2 to 5.** Page 2 reads "Windows build 19045" and "Windows Terminal is not installed: the prompt and welcome box work, but the window keeps its colors", with the button "Install Windows Terminal" and "You can skip this and install it later." Tab 4 (not 3) reaches Next on page 2, because the Install button comes first.
3. **The Store link (check 6).** On page 2 press "Install Windows Terminal" (Tab 1 from the heading).

   ```powershell
   Start-Sleep -Seconds 8
   Invoke-GuestSnippet $vm "Get-Process WinStore.App, msedge -ErrorAction SilentlyContinue | ForEach-Object { '{0} {1}' -f `$_.ProcessName, `$_.MainWindowTitle }; Get-CimInstance Win32_Process -Filter `"Name='msedge.exe'`" | ForEach-Object { `$_.CommandLine }" $u
   ```

   PASS: the Store app starts (`WinStore.App`), or, when it cannot open the link, Edge starts with `aka.ms/terminal` in its command line. If neither happens the app must show an error (screenshot).
4. **Pages 3 to 6.** Page 5 lists, under "Good to know", "Windows Terminal is not installed: only the prompt and the welcome box change."; the page-6 text is "Open a new cmd window …". Press "Try cmd".
5. **The conhost window.** A plain console window opens with the welcome box and the prompt. Screenshot `r7-conhost`. PASS: `Measure-Color $shot '#0C0C0C'` is of the order of G7's 457,432 (no theme color: spec §1); `Get-GuestCmdWindows` shows Clink in its `cmd.exe`. Record whether the box's `✻` shows or the `*` fallback appears (visual).
6. Undo from the bar, and the facts and AutoRun as in Run 2 step 16, including the plain cmd window afterwards. Windows 10 has no Cascadia, so page 5 probably lists "Install 1 font for this account only": the font checks of Run 2 step 16 matter here.

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
- Undo (Run 2 step 16; on a snapshot that was never-opened it is judged on the open-window path of step 16a, with a Terminal window still open).

Everything else is recorded PASS or FAIL and goes back to the task that owns it before the PR. Check 3, the two minutes, is criterion 1's own limit: a FAIL there is reported next to the pass rule, not among the rest. VM-B is information only. A run that is not run is recorded as not run, with the reason.

## Results

The controller writes the results of the runs into `results.md` in this folder, after the runs; this file holds the protocol only. For each step write its id (for example `R2.7`), PASS, FAIL or NOT RUN, and the number or printed line that decided it. Begin with the commit and the CI run the build came from, the date of each run, and the Step 0 file of each VM. State the gate walk's tally as "5 activations, 0 characters, N navigation keys", and, if a founder's mouse walk was done, its number of clicks. Put these screenshots into `docs/spikes/phase3-vm-check/shots/` (no user name of the dev PC appears in them or in the text):

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
