# Phase 3b-1 spike — Clink, the start counter and Windows' crash reports (recovery spec §0)

Written by the controller's VM runner from the raw files beside it; checked by the controller on 2026-10-18. Round 1 ran at `ca20eae`, round 2 at `14bcaeb`.

- **Date:** 2026-10-18.
- **Where:**
  - **VM-A** (`cmdstudio-win11`, Windows 11 26H2 26300.9457 en-US, user `tester`), 07:39–08:06. The host was locked, so VM-A was cold-booted from `clean`'s disk (`snapshot restore clean`, `discardstate`, `startvm`); see `docs/spikes/phase3-vm-check/results.md`, "How the VMs could be run". On that boot Windows finishes its setup again, so VBoxService was started by hand.
  - **VM-C** (`cmdstudio-th`, Thai, system locale th-TH, code page 874, user `ผู้ทดสอบ ไทย`; the data folder path is Thai with a space), 13:09–13:18. VM-C was restored from `clean`'s saved state in a short unlocked spell of the host, and the guest bugchecked and rebooted 3 minutes later. Phase 3's Run 6 ran first on the same boot and ended with the app's `--cleanup` (AutoRun absent, no CMD Studio data).
  - Clink 1.9.34 and Oh My Posh 31.5.0 from the repo's `resources\`; scripts `spikes/vm/guest/recovery-*.ps1` and `spikes/recovery/` at `ca20eae`.
- **Two VM-only changes,** never in the repo:
  - **`probe.lua` line 25.** As committed, `tostring(os.getenv("_ZL_CLINK_PROMPT_PRIORITY"))` stops the probe at its first line: `aa-probe.lua:26: bad argument #1 to 'tostring' (value expected)`. Clink's `os.getenv` returns no value at all for an unset variable. The VMs' copy had `tostring((os.getenv(...)))`.
  - **`timing.lua`.** For the second half of S8 the VMs' copy marks the first `clink.onbeginedit` instead of a prompt filter at 100 (S6 says why).
- **Raw output** beside this file:
  - `probe-vm-a.txt`, `probe-vm-c.txt`, `timing-vm-a.txt`, `timing-vm-c.txt`, `crash-vm-a.txt`, `crash-vm-c.txt`.
  - `event1000-th-svchost.xml`: an older, unrelated event 1000 on VM-C, kept for its field names. It is not a fixture.
  - **Round 1 left no `event1000-en.xml` or `event1000-th.xml`:** no cmd.exe crash happened (S11). Round 2 added both from F10's program (below).

| # | What | Plan 3b-1 assumes | VM-A | VM-C | Holds? | Fallback taken |
|---|---|---|---|---|---|---|
| S1 | `os.getpid()` in a cmd window | a number, cmd.exe's pid | `p3b 5080 load host=C:\WINDOWS\system32\cmd.exe`; `Get-GuestCmdWindows`: `cmd pid 5080 … Clink loaded: clink_dll_x64.dll` (also 1208) | `p3b 2020 load host=C:\WINDOWS\system32\cmd.exe`; `cmd pid 2020` (also 8800) | ✅ | — |
| S2 | `io.open` write/read and `os.remove` in `health\` | work on the (Thai) path | `p3b 1208 io.open write ok=true`, `io.open read text=v1 1792284108\n`, `os.remove ok=true`, `gone after remove=true` | data `C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio-spike3b`: `p3b 2020 io.open write ok=true`, `io.open read text=v1 1792303801\n`, `os.remove ok=true`, `gone after remove=true`; `probe.txt` (written by `io.open` in that folder) exists | ✅ | — (F2 not needed) |
| S3 | `os.globfiles(<health>\start-*)` | the matching files; names alone or whole paths | `globfiles count=1 first=start-1208` | `globfiles count=1 first=start-2020` | ✅ (names alone) | — |
| S4 | `os.gethost()` | `…\cmd.exe` in cmd, Clink's own exe in `clink set` | cmd: `host=C:\WINDOWS\system32\cmd.exe`; `clink set`: `host=C:\Users\tester\AppData\Local\ai.resincore.cmdstudio-spike3b\runtime\clink-1.9.34\clink_x64.exe` | cmd: `host=C:\WINDOWS\system32\cmd.exe`; `clink set`: `host=C:\Users\3607~1\AppData\Local\AIRESI~1.CMD\runtime\clink-1.9.34\clink_x64.exe` | ✅ | — |
| S5 | `clink set` loads the scripts | yes | `clink set said:` `p3b 5972 load host=…clink_x64.exe …`; each of setup's five `clink set` calls left probe lines | `p3b 11160 load host=…clink_x64.exe …` | ✅ | — |
| S6 | order of oninject, onbeginedit, filter 0, Oh My Posh's (1), filter 2 | onbeginedit before the filters; filter 2 sees Oh My Posh's prompt (`esc=true`) | `order 1 oninject`, `order 2 onbeginedit`, `order 3 filter-0 bytes=16 esc=false`; **no `filter-2` line** in either window | the same, `filter-0 bytes=44 esc=false`, **no `filter-2`** | ❌ | **F5** |
| S7 | `cmd /c` with AutoRun | no probe line from those pids | `cmd /c pid=1448/7900/11040 exit=0`; no `p3b` line from them | `cmd /c pid=8080/6592/6096 exit=0`; none | ✅ | — |
| S8 | cost per start: without / with the counter, and `p3b counter ms=` | ≤ 5 ms more | as written: `no marker for pid …` every time (S6). With the onbeginedit marker: average of 10 **815 ms → 831 ms (+16)**; `p3b counter ms=2.635` and `2.666` | as written: no marker. Onbeginedit marker: **861 ms → 860 ms**; `p3b counter ms=6.088` | ❌ (VM-A +16 ms; VM-C's own 6.1 ms, both with 4–11 leftover tags, see Notes) | **F7** |
| S9 | event 1000: EventData names and values | `AppName` = `cmd.exe`, `ModuleName` = `clink_dll_x64.dll`, same names in Thai | no event 1000 at all (`events 1000: 0`, `NoMatchingEventsFound`) | no cmd.exe event; the only event 1000 (2026-10-10, `svchost.exe_CDPSvc` in `ntdll.dll`) has named fields `AppName`, `AppVersion`, `AppTimeStamp`, `ModuleName`, … on Thai Windows | not shown | waits for F10's program; F8 not indicated |
| S10 | `crash_query`'s XPath through `Get-WinEvent -FilterXPath` | accepted, finds the three crashes | `the app's query finds: 0` with `NoMatchingEventsFound` (accepted, not "The specified query is invalid") | accepted the same way; the FilterHashtable read returns 1 event 1000 (8 days old) and the app's seven-day query returns 0: the `timediff` clause is applied | accepted ✅; "finds the crashes" not shown | — (F9 not needed); the finding part waits for F10 |
| S11 | Clink injects the stand-in and cmd crashes in it | yes | `clink_x64.exe - Entry Point Not Found: The procedure entry point ?loader_main_thunk@@YAHXZ could not be located in the dynamic link library …\clink_x64.exe` | the same box | ❌ | **F10** |

## Notes

**S6 → F5.** No prompt filter after Oh My Posh's runs.
- On both VMs, the probe's filter at 2 never logged.
- `zz-timing.lua`'s filter at 100 never wrote its marker with Oh My Posh loaded (`no marker for pid …`, 11 of 11 and 3 of 3). With `init.lua` renamed away, a Win+R window wrote it at once (`markers without Oh My Posh: 1`, VM-A).
- The likely cause is Oh My Posh's own script (`%LOCALAPPDATA%\oh-my-posh\init.*.lua`, lines 480–483). Its `p:filter` returns one value, but its `p:rightfilter` returns `(… ), false` under the comment "Returning false as the second value halts further prompt filtering". Clink 1.9.34 then evidently stops the chain for every later filter, left ones included.
- The consequence for the counter as planned was seen on VM-A. With `-Counter`, every window left its tag in `health\` (4, then 11 tags). A fifth Win+R window, opened once three tags were over 60 s old, paused the theme although every window had reached its prompt: `paused.json: {"version":1,"reason":"cmdStarts","at":1792284673000,"count":3,"by":"clink"}`. The window printed "CMD Studio paused its theme because cmd failed to start 3 times. Open CMD Studio to turn it back on." (screenshot `cmdstudio-win11-p3b-false-pause.png`).
- F5 replaces the filter with `clink.onendedit`, a hook, which Oh My Posh's filter cannot stop. `onbeginedit` ran in every window (`order 2 onbeginedit`) and is another candidate.

**S8 → F7.** As written, S8 cannot be measured: `timing.lua`'s marker is a prompt filter after Oh My Posh's (S6). With the VMs' onbeginedit marker (cmd's start to the first edit line, after every script has loaded):

| | Without the counter | With the counter | Counter's own `os.clock` |
|---|---|---|---|
| VM-A | 815 ms | 831 ms (+16; runs from 787 to 920) | 2.6 ms |
| VM-C | 861 ms | 860 ms | 6.1 ms |

- The VM-A difference is within the spread of 10 runs.
- Both `p3b counter ms=` figures were taken with 10 leftover tags in `health\` to list and read, because the tags were never removed (S6). With F5 in place, a window finds at most a few tags.
- F7 says record and go on; re-measure after Task 3 with the F5 counter and an onbeginedit or onendedit marker. Task 12's `recovery-timing.ps1 -InstallMarker` has the same problem as S8.

**S11 → F10 (and S9, S10 wait for it).**
- `clink_x64.exe` imports `?loader_main_thunk@@YAHXZ` from `clink_dll_x64.dll`. With the stand-in in its place (which exports `DllMain` only), Windows refuses to start the loader ("Entry Point Not Found", one box per window). cmd.exe never loads the stand-in, and no event 1000 is written.
- Even with that export, the stand-in would crash `clink_x64.exe` (the loader loads the DLL first), not cmd.exe.
- The three cmd windows waited behind the boxes, and the boxes were dismissed before `-Restore`. After `-Restore` a new window loaded the real DLL again (`Clink loaded: clink_dll_x64.dll`), and `-Remove` put AutoRun back to absent on both VMs.
- F10's `fake-cmd` program was not run in round 1; round 2 ran it (below).
- S10's query was accepted on both VMs. VM-C shows that its `timediff` part filters: an event 1000 from 2026-10-10 is returned by the plain read and left out by the app's seven-day query.

**S9's field names on Thai Windows** are named, as on English: see `event1000-th-svchost.xml` (`<Data Name='AppName'>…`). That event is not a cmd.exe or Clink crash. The values the plan assumes (`cmd.exe`, `clink_dll_x64.dll`) need F10's event.

**probe.lua.** In Clink 1.9.34, `os.getenv` of an unset variable returns nothing, not `nil`.
- `tostring(os.getenv(x))` therefore raises; `os.getenv(x) or ""` and `os.getenv(x) == nil` are safe.
- `counter.lua` uses only the safe forms (`tonumber(os.getenv(...) or "")`, `if os.getenv(...) then`), and so does F4's text.
- The committed `probe.lua` needs the extra parentheses before a re-run.

## Round 2 (commit 14bcaeb): S6b, the coroutine after Oh My Posh's prompt, and F10

- **Where and when:**
  - **VM-C**, 14:04–14:20: `clean` restored from its saved state while the host was unlocked; guestcontrol answered at once.
  - **VM-A**, 14:32–14:48: its normal path (saved state, then a bugcheck 0x0A 23 s in and a reboot), so Terminal is `present`.
  - The host was locked again from 14:20 to 14:29 and from about 14:48.
- **Scripts:** `recovery-s6b.ps1` and `probe-s6b.lua` as committed, and `probe.lua` with its getenv fix. Nothing was changed on the VMs this time.
- **F10's program:** `fake-cmd.exe` from the scratchpad (sha256 `4E8B65AD…B672`), copied to the VMs as `C:\cmdstudio\fake\cmd.exe`, with the stand-in `clink_dll_x64.dll` (`D52A20C7…928A2E`).
- **Raw output:**
  - `s6b-vm-c.txt`, `s6b-vm-a.txt`: setup, runs and cumulative reports.
  - `f10-vm-c.txt`, `f10-vm-a.txt`: the runs and `recovery-crash.ps1 -Events`.
  - `event1000-th.xml` and `event1000-en.xml`: the newest event of each, with `UserID` set to `S-1-5-21-1-2-3-1001` and nothing else changed.
- **Read by the coordinator's rules:**
  - the n=1 lines only;
  - Oh My Posh's filter is the `prio=1` one;
  - "after Oh My Posh" means after that window's last `omp … end prio=1 n=1`, here `omp rightfilter end … n=1`.
  - No key was sent to any of these windows: the script opens and kills them.
- **Settings:** `prompt.async` is `True` in the spike profile on both VMs.
- **Setup:** `-Setup` printed the `load` and `api` lines (`addcoroutine=function promptcoroutine=function … version_encoded=10090034`) and no `raised` line, on both VMs.
- **The wrapper:** every window logged `wrapped 3 methods of 1 filters: filter@1,rightfilter@1,transientfilter@1`.

| # | What | Must show | VM-A | VM-C | Holds? |
|---|---|---|---|---|---|
| S6b-a | the `clink.addcoroutine` route, no hang | runs after Oh My Posh's filter, within seconds, no key | `addcoroutine ok=true ret=nil status=suspended` (not run inside the call). `coroutine` 0.17 s and 0.15 s after `omp rightfilter end … n=1` (10036: 6.522 → 6.691; 5736: 3.669 → 3.822). The third window (2076), whose Oh My Posh filter took 13.9 s, was killed about 1 s after it ended and has no coroutine line (not reached, so it counts for neither side) | `coroutine` 0.10, 0.06 and 0.06 s after it (8196: 1.385 → 1.481; 7780: 0.931 → 0.994; 4532: 0.636 → 0.696) | ✅ |
| S6b-p | the `clink.promptcoroutine` route, no hang | the same | `promptcoroutine call ok=true ret=nil`, called from `filter0` n=1; `promptcoroutine t=` 0.17 and 0.12 s after Oh My Posh's end (6.693, 3.791) | 0.09, 0.07 and 0.06 s after it (1.475, 0.997, 0.694) | ✅ |
| S6b-h | a hang in a filter (loop, and `io.popen` of a 20 s `ping`) | no coroutine line between `filter0 hanging` and `filter0 … hung=`; both routes run after the hang and after Oh My Posh's | loop 8572: `hanging t=20.370`, `filter0 t=40.372 hung=20s`, `omp rightfilter end n=1 t=40.820`, then `promptcoroutine t=40.917`, `coroutine t=41.072`; loop 5364: 0.556 / 20.557 / 20.980 / 21.173 / 21.175; popen 4448: 4.802 / 26.883 (`hung=22s`) / 32.214 / 32.894 / 32.892 | loop 7124: 0.602 / 20.604 / 20.807 / 20.878 / 20.902; loop 4624: 0.381 / 20.381 / 20.566 / 20.594 / 20.612; popen 11184: 0.965 / 21.894 (`hung=21s`) / 22.382 / 22.587 / 22.602 | ✅ |
| S6b-c | a window closed during the hang | no coroutine line for that pid | closed at 20 s (5716, 5656): `filter0 hanging kind=popen`, then nothing: no `filter0 t=`, no `omp`, no `coroutine`. Closed at 10 s (5700, 4100): killed before the hang began (only `load` lines, or none) | closed at 10 s (9416, 820): `filter0 hanging kind=popen`, then nothing | ✅ |
| S6 cause | why round 1's filter at 2 never ran | — | every `omp rightfilter end … results=2 second=false`: Oh My Posh's right filter returns `false` as its second value, which halts the later filters | the same | confirmed |
| wrapper | did the wrapper change Oh My Posh's prompt? | no | `cmdstudio-win11-s6b-a-prompt.png` has the same box prompt (`╭─ ~ ───`, then `>`) and welcome box as round 1's `cmdstudio-win11-p3b-1.png` | `cmdstudio-th-s6b-c-prompt.png` matches round 1's `cmdstudio-th-p3b-c-1.png` | no change |
| S9 (F10) | event 1000 EventData | `AppName` = `cmd.exe`, `ModuleName` = `clink_dll_x64.dll`, the same names on both | `events 1000: 3`, each `[AppName] = [cmd.exe]`, `[ModuleName] = [clink_dll_x64.dll]`, `[ModuleVersion] = [1.9.34.0]`, `[ExceptionCode] = [c000001d]`, `[AppPath] = [C:\cmdstudio\fake\cmd.exe]`, `[ModulePath] = [C:\cmdstudio\crash-dll\clink_dll_x64.dll]` | the same three, the same field names, on Thai Windows | ✅ (F8 not needed) |
| S10 (F10) | the app's query and the `cmd.exe` variant | find the 3 crashes | `the app's query finds: 3`; `with EventData[Data='cmd.exe']: 3` | `3`; `3` | ✅ (F9 not needed) |
| S11 | Clink injects the stand-in | — | still ❌ (round 1): F10's program stands in for it, and its events are the fixtures | | **F10** holds |

**What the round 2 rows change.**
- **The coroutine route works on both routes:** it runs once Oh My Posh's filters have returned, never during a hung filter, and not for a window closed during the hang. So the S6 fallback F5 (`onendedit`) is not the only way: the counter could clear its tag from a coroutine added at `onbeginedit`, with no key needed. Which one to use is the plan's call.
- One VM-A window shows the limit: when Oh My Posh's first prompt takes 14 s and the window is closed 1 s later, nothing marks it. That is the same "closed before the prompt finished" case the counter counts as a failed start.
- **`event1000-en.xml` and `event1000-th.xml` are real events** of a program named `cmd.exe` crashing in `clink_dll_x64.dll` (`AppPath` is `C:\cmdstudio\fake\cmd.exe`, as F10 says).
