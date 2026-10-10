# G12, G13 — Windows CI probes

- **Date:** 2026-10-10.
- **Where it ran:** GitHub Actions `windows-latest`. It reported `Microsoft Windows Server 2025 Datacenter 10.0.26100`, a server edition rather than the Windows 11 most users run. Its file system and registry are real, unlike the dev machine's shell inside the Claude app, which is MSIX-virtualized.
- **Runs:**
  - [38064746693](https://github.com/luraselenehalo/cmd-studio/actions/runs/38064746693), commit 00f966b: G12 inconclusive.
  - [38065370337](https://github.com/luraselenehalo/cmd-studio/actions/runs/38065370337), commit 9592f54: G12 rerun with a font the image does not have; G13 again.
- **Scripts:** `spikes/windows/fonts.ps1`, `spikes/windows/autorun.ps1`, `.github/workflows/spike-windows.yml`.

## G12 — per-user font install and removal: ✅ on Windows Server 2025

Run 38065370337, copied from the job log:
```
windows: Microsoft Windows Server 2025 Datacenter 10.0.26100
context: Cascadia Code preinstalled=True Cascadia Mono preinstalled=True
zip sha256: 1298BF92698AFA06185CF1D05E6AE05F2D8A1E8C3CB45DDF4C3035168AB342A1
family in file: CaskaydiaCove NF
before: False
add=1
after install: True
remove=True
after uninstall: False
file deleted: True
```

What this shows:
- **Install works per user, without admin rights.** The probe copied the file to `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, set the value in `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts`, called `AddFontResourceW` and broadcast `WM_FONTCHANGE`. A new process then saw the family.
- **Removal works in reverse:** `RemoveFontResourceW`, a broadcast, then deleting the value and the file. The file was not locked, and a new process no longer saw the family.
- **The checks are real.** Each one fails the step when its answer is wrong, so a green run means every line above came out as expected.

The first run proved nothing for G12:
```
before: True
add=6
after install: True
remove=True
after uninstall: True
```
Cascadia Code was already installed on the image, as the `context:` line of the second run shows, so the checks could not change. The rerun uses CaskaydiaCove Nerd Font v3.5.1 (`CascadiaCode.zip` from the Nerd Fonts release, sha256 above). This is the font the app ships.

Not yet proven:
- **Sign-in persistence:** the font is still there after signing out and back in, when Windows loads per-user fonts from the HKCU value. Needs VM-A.
- **Client Windows:** behavior on Windows 11 and Windows 10. Needs VM-A and VM-B.
- **A running Windows Terminal:** it sees a font installed after it started, without a restart.
- **The font's name (new, for Phase 1b).** GDI+ reports the family as `CaskaydiaCove NF`, not `CaskaydiaCove Nerd Font`. The fonts adapter's detection and the `font.face` value written into Windows Terminal must use a name that Windows Terminal (DirectWrite) accepts. Don't hard-code either name until that is checked.

## G13 — guarded AutoRun: ✅ for cases (ก)–(จ) on Windows Server 2025

Run 38065370337, copied from the job log:
```
original AutoRun: []
(a) file present: exit=0 stderr=[] marker=[probe ran
]
(b) file missing: exit=0 stderr=[]
(c) chained: exit=0 stderr=[] marker=[foreign 
probe ran
]
(e) AutoRun now: [if exist "C:\Users\runneradmin\AppData\Local\สปายค์ ทดสอบ & R&D\clink\clink_x64.exe" "C:\Users\runneradmin\AppData\Local\สปายค์ ทดสอบ & R&D\clink\clink_x64.exe" inject --autorun --quiet --profile "C:\Users\runneradmin\AppData\Local\สปายค์ ทดสอบ & R&D\clink-profile"]
(e) one run with clink: exit=0 stderr=[]
(e) cmd /c average ms: without=9.8 with-clink=23.6
AutoRun restored to: []
```
Run 38064746693 printed the same (a)–(c), and `(e) cmd /c average ms: without=10 with-clink=24.5`.

The spec's cases:
- **(ก) File present:** the guard ran the program (a).
- **(ข) File deleted:** `cmd` still exits 0 with nothing on stderr (b).
- **(ค) Chained after another program's value with ` & `:** both ran, the other program's first (c).
- **(ง) Thai or spaces in the path:** every probe path sits under `สปายค์ ทดสอบ & R&D`, which has Thai, a space and `&`. The quoting held in (a), (c) and (e).
- **(จ) Cost for `cmd /c`:** +14.5 ms and +13.8 ms per call (average of 20, in two runs). This is the cost of starting `clink_x64.exe` once.
  - Clink is not loaded into `cmd /c`. In v1.9.34, `clink/app/src/loader/inject.cpp` lines 407–428, `--autorun` reads cmd's command line and cancels the inject when it finds `/c`. The timing fits that, so scripts run without Clink.
  - A guard written in cmd syntax (substring tests on `%CMDCMDLINE%`) could skip the launch. It was not adopted: one quoting mistake there would break every cmd on the machine, and the saving is about 14 ms.

### Found while preparing the rerun

`autorun.ps1` ran `New-Item -Path $key -Force` on `HKCU\Software\Microsoft\Command Processor`. On an existing key, `New-Item -Force` deletes every value and subkey. I checked this on a scratch key in the dev shell's virtualized registry: both a value and a subkey were gone.

On a real machine the probe would have erased the user's cmd settings before saving them. The CI image is thrown away after each run, so nothing was lost. The fixed probe still read `original AutoRun: []`, so the first run's empty value was real.

The fix is in 9592f54. **Rule for Phase 1b:** the AutoRun adapter opens the existing key and changes only the `AutoRun` value. It never recreates the key.

Not yet proven:
- **Interactive `cmd` with Clink loaded:** start time, and the banner and screen clear in AutoRun mode (G4). Needs VM-A or the dev machine.
- **A Thai Windows with a Thai user name (VM-C).** On the runner, the Thai is only in a folder name, and the probe did not record the system locale.
