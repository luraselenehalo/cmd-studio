# G4, G5, G16, G17 — Clink probes

- **Date:** 2026-10-10
- **Where it ran:** dev machine (Windows 11 Home 26300, AtlasOS), using Clink 1.9.34 from the prototype folder.
- **Probe profile:** lives in `spikes/clink/tmp/` inside the repo, not under AppData. The dev shell runs inside an MSIX-packaged app whose AppData writes are virtualized, so this keeps the probe out of that.
- **Window:** the injected cmd ran as a hidden window, so it took no focus and sent no keystrokes.
- **Raw output:** `docs/spikes/clink-probe-output.txt`. The user and computer names are redacted as `%USERPROFILE%` and `<computer>\<user>`.

## G16 — turning off Clink's self-update: ✅

```
        Name: clink.autoupdate
     Options: off,check,prompt,auto
       Value: check
     Default: check
...
Setting 'clink.autoupdate' set to 'off'.
exit=0
```
The default `check` only prints a message. `auto` would try to install updates by itself and could replace our bundled runtime. The spec's `clink.autoupdate = off` is correct.

## G5 — how the verify step can see a Lua error: ✅ (by output, not by exit code)

With `bad.lua` (`error("cmdstudio spike: deliberate error")`) on `clink.path`:
```
%USERPROFILE%\cmd-studio\spikes\clink\tmp\scripts\bad.lua:1: cmdstudio spike: deliberate error
        Name: clink.logo
...
exit=0
```
- `clink set` **exits 0** even when a script fails.
- The error is printed to the command's output as `<script path>:<line>: <message>`.
- `clink.log` had no entry from this run.

**For Phase 1b:** the verifier runs `clink_x64.exe --profile <p> set clink.logo` and reports `E_VERIFY_CLINK` when any output line matches `\.lua:\d+:`. It must not rely on the exit code.

## G4 — `console.getlinetext` and `clink.print(s, NONL)`: ✅ (header detection in AutoRun mode still ⬜)

```
probe getlinetext: 6=CMDSTUDIO-PROBE-A | 7=CMDSTUDIO-PROBE-B
probe screen line 1: [Clink v1.9.34.4e8ae4]
probe screen line 2: [Copyright (c) 2012-2018 Martin Ridgers]
probe screen line 3: [Portions Copyright (c) 2020-2026 Christopher Antos]
probe screen line 4: [https://github.com/chrisant996/clink]
probe screen line 5: []
```
- `getlinetext(n)` returns line `n` of the screen buffer, counting from 1.
- `clink.print("CMDSTUDIO-", NONL)` followed by `clink.print("PROBE-B")` landed on one line (line 7).
- With `cmd /d /k …`, cmd printed **no** "Microsoft Windows" header. Lines 1–4 are Clink's own logo, because the probe profile did not set `clink.logo = none`. This matches what the founder's Windows Terminal tab showed.
- **Still ⬜:** in AutoRun mode (a plain `cmd.exe`, as from Win+R), the header is printed before AutoRun runs. Whether the banner's `clear_cmd_logo()` sees it on lines 1–2 must be checked on a VM, including Thai Windows (Task 8).

## G17 — `io.popen` with the doubled outer quotes: ✅ for arguments and exe paths, with one caveat

```
probe popen: ["one two"|"???"
probe popen-input (Thai file name reaches cmd?): [FOUND
probe popen-exe (exe under a Thai path with spaces): [<computer>\<user>
```
- **Quoting works.** The pattern `'"' .. '"' .. <exe> .. '" args"' .. '"'` survives cmd's quote stripping. `"one two"` arrived as one argument.
- **Input is intact.** A Thai argument reached cmd (the `.cmd` found `สาม.txt` and printed `FOUND`).
- **Thai paths launch.** An exe inside a folder whose name has Thai letters and a space (`โฟลเดอร์ ทดสอบ\who ami.exe`) ran.
- **Caveat (output only):** Thai text written back by a `.cmd` through the pipe arrived as `???`, because cmd writes in the OEM code page. Oh My Posh writes its init script as UTF-8 bytes, so this should not affect `init.lua`. Phase 1b must still prove `init.lua` end to end with a data folder under a Thai username (VM-C).

## Stock Windows (Task 8, 2026-10-11)

### The VMs

The VMs are built by `spikes/vm`; its README covers the setup.

| VM | Windows | Console | User and paths |
|---|---|---|---|
| VM-B | 10 Home 22H2 (19045), en-US | conhost, no WT | `tester` |
| VM-A | 11 Home 26H2 (26300.9457), en-US | Windows Terminal 1.24 | `tester` |
| VM-C | 11 Home 26H2 with the Thai language pack | Windows Terminal 1.24 | `ผู้ทดสอบ ไทย`, profile `C:\Users\ผู้ทดสอบ ไทย` |

VM-C details:
- System locale `th-TH`, with ANSI and OEM code page 874.
- The screen shows Thai: the Recycle Bin is "ถังรีไซเคิล" and the date is 11/10/2569.

### How the probes ran

- **AutoRun mode:** `spikes/vm/guest/proto-setup.ps1 -WithOmp` lays out what CMD Studio will install under `%LOCALAPPDATA%`:
  - `runtime\clink-1.9.34`, `runtime\omp-31.5.0` and a Clink profile
  - an `init.lua` that runs Oh My Posh from its full quoted path, as spec §5.4 asks
  - `00-g4-header.lua`, which logs the screen at inject time
  - the prototype's `claude-banner.lua`
  - the guarded AutoRun value
- **The cmd under test** is then opened the way a person does: Win+R, `cmd`, Enter, typed into the VM.
- **The `/k` probe:** VM-C also reran `run-probes.ps1` from inside the Thai profile (`guest/thai-clink-probes.ps1`).

### G4 — the cmd header in AutoRun mode: ✅ on all three

Every VM logged this at inject time, with the prototype's version of `getlinetext` lines 1–2, and VM-B logged build 19045:
```
g4 screen line 1: [Microsoft Windows [Version 10.0.26300.9457]]
g4 screen line 2: [(c) Microsoft Corporation. All rights reserved.]
g4 prototype header match: true
```
- **Header detection works:** in AutoRun mode, cmd prints its header before AutoRun runs, and `claude-banner.lua` cleared it. On screen, the welcome box replaced the header on VM-A, VM-B and VM-C.
- **Thai Windows keeps the English header.** With Thai installed, `cmd.exe.mui` exists only in `System32\en-US`; there is no `th-TH` copy. The listing is saved in `spikes/vm/tmp/thai-cmd-mui.txt`. So the pattern from spec §3.6 also works for Thai users.
- **VM-C's lines, as saved:** `spikes/vm/tmp/thai-proto-report-short.txt`, rerun on 2026-10-11 for the final review with Clink through its 8.3 path. The first VM-C run's output was printed but not saved.

### G17 — `init.lua` end to end: ✅ on all three, but only once Clink itself runs (see below)

| VM | Oh My Posh path | Result |
|---|---|---|
| VM-B, VM-A | `C:\Users\tester\AppData\Local\…\runtime\omp-31.5.0\oh-my-posh.exe` | `g17 init bytes: 272`, `g17 prompt loaded` |
| VM-C | `C:\Users\ผู้ทดสอบ ไทย\AppData\Local\…\runtime\omp-31.5.0\oh-my-posh.exe` | `g17 init bytes: 328`, `g17 prompt loaded` |

The 56 extra bytes on VM-C are the Thai path, which Oh My Posh writes into its init script as UTF-8. On a code page 874 system, `io.popen` with the doubled outer quotes runs an exe from a Thai path that has a space, and the prompt works.

### New blocker: Clink 1.9.34 cannot inject from a folder whose path has a Thai user name ❌

On VM-C, with Clink's own files under the Thai profile, neither the hidden `/k` probe nor the AutoRun layout managed to inject.

In AutoRun mode, **every cmd window then prints Clink's error** under the header (screenshot `spikes/vm/tmp/cmdstudio-th-proto-thai.png`):
```
DLL version mismatch.
Unable to inject Clink.
See log file for details (C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio-spike\clink-profile\clink.log).
```
The `if exist` guard does not prevent this: the exe exists, starts, and only then fails.

`clink.log`, copied as it was:
```
copy_dll                  122 *** ERROR 123 ***
copy_dll                  122 Failed to copy DLL to 'C:\Users\3607~1\AppData\Local\Temp\clink\dll_cache\1.9.34.4e8ae4_97d16ab3\clink_dll_x64.dll'
copy_dll                  122 (Error 123.  The filename, directory name, or volume label syntax is incorrect.)
inject_dll                318 DLL: C:\Users\��鷴�ͺ ��\cmdstudio-probes\clink-bin\clink_dll_x64.dll
check_dll_version         154 *** ERROR 2 ***
check_dll_version         154 Unable to get DLL version for 'C:\Users\��鷴�ͺ ��\cmdstudio-probes\clink-bin\clink_dll_x64.dll'
check_dll_version         154 (Error 2.  The system cannot find the file specified.)
```
- **What goes wrong:** Clink converts the path of its own DLL through the wrong code page, so the Thai name comes out garbled, and the file can't be found.
- **What still works:** the profile path is passed on the command line and stays correct. `clink info` showed the Thai profile paths correctly, and G5 and G16 still worked.
- **Upstream:** [chrisant996/clink#203](https://github.com/chrisant996/clink/issues/203) was the same symptom for a Chinese folder. It was fixed in 1.2.47 (2021). The maintainer said then that other places might still rely on the system code page being UTF-8. 1.9.34, released 2026-09-21, is the latest release.
- **Why it blocks the design:** spec §5.3–5.4 puts the runtime under `%LOCALAPPDATA%`. A user whose Windows name is Thai would get no theme, and the error above in every cmd window.
- **English Windows too:** VM-A (code page 1252) with an added user `ผู้ทดสอบ ไทย` failed the same way. There the Thai letters turn into `?`: `Unable to get DLL version for 'C:\Users\???????? ???\AppData\Local\…\clink_dll_x64.dll'`. Raw output: `spikes/vm/tmp/win11-thai-user-cp1252.txt`. The user was added with `guest/add-thai-user.ps1`, elevated through UAC on the VM's own screen.

Two workarounds were tried on VM-C (Thai Windows), then again on VM-A (English Windows with the Thai user). Every run left 0 `ERROR` lines in `clink.log` and kept the profile under the Thai path:

| Mode (`guest/thai-clink-workaround.ps1`) | Clink run from | VM-C (code page 874) | VM-A (code page 1252) |
|---|---|---|---|
| `short` | the 8.3 path of the same folder: `C:\Users\3607~1\AppData\Local\AIRESI~1.CMD\runtime\CLINK-~1.34\clink_x64.exe` | injects; G4 and G17 as above | injects; `g17 init bytes: 328`, header match `true` |
| `ascii` | a copy in `C:\ProgramData\cmdstudio-spike\clink-1.9.34` | injects; G4 and G17 as above | injects; `g17 init bytes: 328`, header match `true` |

A third form was tried on VM-C after the founder chose the 8.3 route. It joins the data folder's 8.3 path to the long ASCII rest: `C:\Users\3607~1\AppData\Local\AIRESI~1.CMD\runtime\clink-1.9.34\clink_x64.exe`. Result: `g17 init bytes: 328`, header match `true`, and 0 `ERROR` lines in `clink.log`. Raw output: `spikes/vm/tmp/thai-mixed-path.txt`.

This form matters because `GetShortPathNameW` works only on paths that exist. The data folder exists before the first apply, but the runtime folder does not.

Each has a cost:
- **`short`:** depends on 8.3 names, which can be turned off per volume.
- **`ascii`:** moves part of a per-user app out of the user's own folders.
- **Report upstream:** the real fix belongs in Clink, but filing the issue publishes something, so it is the founder's call.

### Also found on stock Windows

- **The banner's ✻ needs a fallback on Windows 10:** conhost's default font has no ✻ (U+273B) and drew an empty box. On VM-A and VM-C, with CaskaydiaCove NF in WT, it rendered.
- **`{date}` on Thai Windows:** the prototype's banner date uses Lua's `os.date` and came out as `อา. 11 ต.ค. 2026`, with Thai day and month names but the Gregorian year. Spec §3.6 wants `ศ. 9 ต.ค. 2569`, so the generator must format the date itself and must not rely on the locale.
- **A Thai keyboard by default:** typing `cmd` into Win+R produced `แทก` ("Windows ไม่พบ 'แทก'"). Tests must switch the input to ENG first (Win+Space). This matters only for automation.
- **Time to the first prompt:** see the "Interactive cmd with Clink" table in `docs/spikes/G12-G13-windows.md`.
