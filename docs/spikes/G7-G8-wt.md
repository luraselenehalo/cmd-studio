# G7, G8 — Windows Terminal behaviors

- **Date:** 2026-10-11.
- **Dev machine (Task 6):** ⬜ not run yet. The founder has `spikes/wt/g8-live-reload.ps1` and the Win+R screenshot step to do.
- **Stock Windows (Task 8):**
  - VM-A: Windows 11 Home 26H2, build 26300.9457, with Windows Terminal 1.24.11911.0.
  - VM-B: Windows 10 Home 22H2, build 19045, with no Windows Terminal.
  - VM-C: the same Windows 11 with the Thai language pack, system locale th-TH, and the user `ผู้ทดสอบ ไทย`.
- **How the VMs ran it:** `spikes/vm`. Keystrokes went into the VM with `VBoxManage controlvm keyboardputscancode` (Win+R, `cmd`, Enter). Screens came from `controlvm screenshotpng`.
- **How results are judged:** by counting the pixels that have exactly one color (`Measure-Color` in `spikes/vm/vm.ps1`), not by looking at the screenshot.

## G7 — WT picks the cmd profile for a cmd started outside WT: ✅ on stock Windows 11, in English and in Thai

Setup: `spikes/vm/guest/wt-cmd-scheme.ps1 -Setup` writes a minimal `settings.json`. The cmd profile `{0caa0dad-35be-5f56-a8ff-afceeeaa6101}` gets the prototype's "Claude" scheme, with background `#1F1E1D`, and keeps WT's own commandline with no arguments. Then Win+R, `cmd`, Enter.

| Screen | `#1F1E1D` (Claude) | `#0C0C0C` (Campbell) |
|---|---|---|
| VM-A, the window Win+R opened | 556,097 px | 1 px |
| VM-C (Thai Windows 11, user `ผู้ทดสอบ ไทย`), the window Win+R opened | 586,372 px | 1 px |

On VM-C the window showed cmd's header in English and the prompt `C:\Users\ผู้ทดสอบ ไทย>` in Thai. The first try there failed, because the Thai keyboard was active and `cmd` came out as `แทก` ("Windows ไม่พบ 'แทก'"); switching the input to ENG with Win+Space fixed it.

- **What opened:** a Windows Terminal window (tabs, `cmd.exe` in the tab) with the Claude scheme. So WT matched a cmd that Explorer started to the cmd profile.
- **Starting cmd with arguments:** done by mistake while preparing (`cmd /k sc query …`), it also opened in WT. Colors were not measured then, so this proves only the handoff.
- **VM-B (Windows 10, no WT):** Win+R opens the old console host. Only `cmd` and `conhost` were running, and the screen had 457,432 px of `#0C0C0C`. WT settings cannot reach it, which matches spec §1: "Win10 gets the prompt but no legacy-console colors".

## G8 — WT reloads settings.json and recolors an open window: ✅ on stock Windows 11

With the G7 window still open, `wt-cmd-scheme.ps1 -Background '#3A1E1D'` changed only the scheme's background in `settings.json`:

| Screen | `#3A1E1D` (new) | `#1F1E1D` (old) |
|---|---|---|
| Before the edit | 0 px | 556,116 px |
| First screenshot after the edit | 556,114 px | 0 px |
| 3 s later | 556,114 px | 0 px |

The window changed color without being reopened, by the first screenshot after the edit. Most of the 17 s between starting the edit and that screenshot was the scheduled task starting inside the VM.

**For §4.4:** after applying a theme, the message can say that open WT windows change by themselves.

## Mistake made while testing

The first G8 attempt passed `-Background #3A1E1D` without quotes. In the task's PowerShell command line, `#` starts a comment, so everything after it was dropped, including the line that writes the finish marker. The run reported `no finish marker`, and the color stayed `#1F1E1D` (556,097 px). With the argument quoted, it worked as shown above.
