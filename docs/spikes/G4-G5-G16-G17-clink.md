# G4, G5, G16, G17 — Clink probes

- **Date:** 2026-10-10
- **Where it ran:** dev machine (Windows 11 Home 26300, AtlasOS), using Clink 1.9.34 from the prototype folder.
- **Probe profile:** lives in `spikes/clink/tmp/` inside the repo, not under AppData, so the Claude app's MSIX virtualization does not affect it.
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
