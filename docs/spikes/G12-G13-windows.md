# G12, G13 — Windows CI probes

- **Date:** 2026-10-10.
- **Where it ran:** GitHub Actions `windows-latest`. It reported `Microsoft Windows Server 2025 Datacenter 10.0.26100`, a server edition rather than the Windows 11 most users run. Its file system and registry are real. The dev machine's shell is not: it runs inside an MSIX-packaged app, which virtualizes its AppData and HKCU writes.
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

## Stock Windows (Task 8, 2026-10-11)

- **Where:** two VirtualBox VMs on the dev machine, set up by `spikes/vm` (see its README).
  - VM-A: Windows 11 Home 26H2, build 26300.9457, with Windows Terminal 1.24.11911.0 as the default terminal.
  - VM-B: Windows 10 Home 22H2, build 19045, with no Windows Terminal.
- **How:** both run the same probes as CI (`spikes/vm/guest/run-ci-probe.ps1`). They run on the signed-in user's desktop session, through an interactive scheduled task, not in a service session.
- **Raw output:** in `spikes/vm/tmp/` (not in git).

### G12 on stock Windows: ✅

| | VM-A Windows 11 | VM-B Windows 10 |
|---|---|---|
| `context:` line | `Cascadia Code preinstalled=True Cascadia Mono preinstalled=True` | `Cascadia Code preinstalled=False Cascadia Mono preinstalled=False` |
| `family in file:` | `CaskaydiaCove NF` | `CaskaydiaCove NF` |
| `before` / `after install` / `after uninstall` / `file deleted` | `False` / `True` / `False` / `True` | `False` / `True` / `False` / `True` |
| Still there after a restart (`font-persist.ps1 -Install`, restart, `-Check`) | `check: visible=True file=True value=True` | `check: visible=True file=True value=True` |
| Removal afterwards (`-Remove`) | `removed: visible=False file=False` | `removed: visible=False file=False` |

The same test on VM-C (Thai Windows 11, user `ผู้ทดสอบ ไทย`) installed the font file under `C:\Users\ผู้ทดสอบ ไทย\AppData\Local\Microsoft\Windows\Fonts`, a Thai path with a space. After a restart: `check: visible=True file=True value=True`. Removal: `removed: visible=False file=False`.

Settled by this:
- **Sign-in persistence:** Windows reloads a per-user font from the HKCU value at sign-in. Both VMs showed this after a full restart with auto sign-in.
- **Fonts preinstalled per version:** Windows 11 has Cascadia Code and Cascadia Mono, and Windows 10 has neither. A theme that needs Cascadia must install it on Windows 10.
- **The font name in Windows Terminal (VM-A).** WT 1.24 accepts both `CaskaydiaCove NF` and `CaskaydiaCove Nerd Font` as `font.face`. The test typed `-> != <= >= www`, and both names drew Cascadia Code's ligatures (`→ ≠ ≤ ≥`).
  - A name that does not exist (`NoSuchFont Control`) fell back to Cascadia Mono with no ligatures, and WT showed **no warning**.
  - So the app must check the font itself; WT will not tell the user.
  - Use `CaskaydiaCove NF`, the one name that both GDI detection and WT accept.

### G13 on stock Windows: ✅

The (a) to (c) results match CI on both VMs. VM-A, verbatim:
```
(a) file present: exit=0 stderr=[] marker=[probe ran
]
(b) file missing: exit=0 stderr=[]
(c) chained: exit=0 stderr=[] marker=[foreign 
probe ran
]
(e) AutoRun now: [if exist "C:\Users\tester\AppData\Local\สปายค์ ทดสอบ & R&D\clink\clink_x64.exe" "C:\Users\tester\AppData\Local\สปายค์ ทดสอบ & R&D\clink\clink_x64.exe" inject --autorun --quiet --profile "C:\Users\tester\AppData\Local\สปายค์ ทดสอบ & R&D\clink-profile"]
(e) one run with clink: exit=0 stderr=[]
(e) cmd /c average ms: without=56.7 with-clink=97.9
```
- **VM-B's timing:** `(e) cmd /c average ms: without=26.3 with-clink=50.9`.
- **Cost:** in these VMs, AutoRun adds 25–41 ms to each `cmd /c`, against 14 ms on CI. The VMs are slower overall: the baseline is 26–57 ms against CI's 10 ms. During VM-A's run, a third VM was installing Windows at the same time.
- **The share stays alike:** with AutoRun, a `cmd /c` takes 1.7–1.9× as long on the VMs and 2.4× on CI.

### Interactive cmd with Clink (opened with Win+R)

`spikes/vm/guest/proto-setup.ps1` lays out what the app will install under `%LOCALAPPDATA%`: Clink and Oh My Posh run from their full paths, plus the prototype's banner and the guarded AutoRun. `proto-report.ps1` then reads Clink's log and the timing marker.

The final numbers were taken with the other VMs paused. Every number comes from these VMs, which are 2.5–5 times slower than CI, going by `cmd /c` with no AutoRun: 26–57 ms here against 10 ms on CI.

| | VM-B Windows 10 (conhost) | VM-A Windows 11 (WT) | VM-C Thai Windows 11 (WT) |
|---|---|---|---|
| cmd's start to Clink's first input line | 564 ms | 1,575 / 1,725 / 2,022 ms | 2,511 (first) / 1,117 / 1,402 ms |
| `oh-my-posh print primary` (average of 5) | 55 ms | 53–57 ms | 94–149 ms |

To see where the time goes on VM-A, the runs were split at the inject time Clink writes to `clink.log`:

| VM-A run | cmd's start to Clink's inject | Clink's inject to the first input line |
|---|---|---|
| 1 | 1,674 ms | 388 ms |
| 2 | 663 ms | 393 ms |

- **What CMD Studio adds:** Clink's inject, the Lua scripts and Oh My Posh's `init` cost a steady ~390 ms on this VM. Starting `clink_x64.exe` from AutoRun adds about 40 ms on top (G13 (e)).
- **What varies (0.7–1.7 s):** the time before Clink starts. That is cmd starting and Windows handing the console to Windows Terminal. Every cmd on Windows 11 pays it, with or without CMD Studio. Windows 10's conhost does not have it.
- **On real hardware:** the added time should be well under half a second. This is still not proven on real hardware; the founder's dev machine can measure it.
- **VM-C:** ran with Clink through its 8.3 path. See the Clink blocker in `docs/spikes/G4-G5-G16-G17-clink.md`.
- **G4 (the header and the banner) and G17:** see the "Stock Windows" section of `docs/spikes/G4-G5-G16-G17-clink.md`.
