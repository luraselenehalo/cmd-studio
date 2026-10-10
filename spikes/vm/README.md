# Test VMs for stock Windows

The dev machine runs AtlasOS, which few users have, so it does not show what most users would see. These VMs do. They run headless in VirtualBox, so no window appears on the host. Every input goes to the VM: keystrokes through `controlvm keyboardputscancode` and screenshots through `controlvm screenshotpng`.

## Host setup (done 2026-10-10)

| Item | Value |
|---|---|
| VirtualBox | 7.2.20 r175154, `VirtualBox-7.2.20-175154-Win.exe`, SHA256 `a81777d2…2c26` from the official `SHA256SUMS` |
| Install options | `--silent --no-silent-cert --msiparams ADDLOCAL=VBoxApplication` and no shortcuts, file associations or auto-start (see `install-virtualbox.ps1`) |
| Windows 11 | `Windows11_Client_x64_en-us_26300_9457.iso` (2026 Update, 26H2), SHA256 `BD4307DF…EA650`, the English 64-bit value Microsoft publishes |
| Windows 10 | `Win10_22H2_English_x64v1.iso`, SHA256 `A6F470CA…2852E`, the English 64-bit value Microsoft publishes |
| Where things live | `%USERPROFILE%\VMs`: downloads, `machines\` (the VMs) and `accounts\` (random test-account passwords, never in the repo) |

The install options matter:
- **`ADDLOCAL=VBoxApplication`** leaves out the bridged and host-only network drivers, so the host's network doesn't drop during install. NAT needs no host driver.
- **`--no-silent-cert`** stops silent mode from adding Oracle's certificate to the host's TrustedPublisher store.
- **Run the installer through Windows Terminal** (`wt -w new powershell -File …`), because the dev shell runs inside an MSIX-packaged app and is virtualized.

## Machines

| VM | What it stands for |
|---|---|
| `cmdstudio-win11` (VM-A) | Windows 11 26H2 in English, Windows Terminal as the default terminal, user `tester` |
| `cmdstudio-th` (VM-C) | The same ISO with locale `th_TH` (ANSI code page 874). Setup creates `tester`. A post-install command adds the Thai display language, creates `ผู้ทดสอบ ไทย` (a Thai name with a space) as an administrator, and switches auto-logon to it. |
| `cmdstudio-win10` (VM-B) | Windows 10 22H2 in English, which has no Windows Terminal |

```powershell
.\spikes\vm\new-vm.ps1 -Name cmdstudio-win11 -OsType Windows11_64 -Iso <win11 iso> -ImageIndex <n>
.\spikes\vm\new-vm.ps1 -Name cmdstudio-th -OsType Windows11_64 -Iso <win11 iso> -ImageIndex <n> `
  -Locale th_TH -Country TH -PostInstallCommand (Get-Content tmp\thai-post-install.txt -Raw).Trim()
.\spikes\vm\new-vm.ps1 -Name cmdstudio-win10 -OsType Windows10_64 -Iso <win10 iso> -ImageIndex <n> -DiskGB 50
```

`tmp\thai-post-install.txt` holds `powershell.exe -EncodedCommand …`. It runs elevated at the first logon of `tester`:
- `Install-Language th-TH -CopyToSettings`
- `Set-WinSystemLocale th-TH`
- `New-LocalUser 'ผู้ทดสอบ ไทย'`, added to `S-1-5-32-544` (Administrators)
- the Winlogon auto-logon values for that user
- a restart after 60 s

It holds that user's password, so it stays in `tmp\`. The command is encoded because VirtualBox pastes it into a `.cmd` file verbatim.

**Why not `-User 'ผู้ทดสอบ ไทย'`:** unattended setup cannot create a Thai account name. `C:\Windows\Panther\UnattendGC\setupact.log` logged `UserAccounts: failed to create user account '???????? ???' (0x8007089a)`, which is `NERR_BadUsername`. Setup passes the name through a code page without Thai, and `?` is not allowed in a user name. Setup then stops with "Windows could not complete the installation". `New-LocalUser` takes the name as Unicode and works.

`new-vm.ps1` makes an EFI VM with Secure Boot on. The OS type defaults give Windows 11 a TPM 2.0 (`<TrustedPlatformModule type="v2_0">` in the `.vbox`) and give Windows 10 none. It then runs VirtualBox's unattended install from `UnattendedTemplates\win_nt6_unattended.xml`. That answer file:
- accepts the Windows license terms
- skips the Windows 11 hardware checks (LabConfig keys)
- skips OOBE
- creates the local administrator `tester` with auto-logon
- installs Guest Additions at first logon

Windows is not activated.

Two things in the VM are not stock: Guest Additions, and the LabConfig keys. Neither touches cmd, Windows Terminal or fonts.

## Running probes

Dot-source `vm.ps1` on the host. It provides:

| Helper | Does |
|---|---|
| `Invoke-Guest`, `Invoke-GuestPs` | run a program in the VM. This runs outside the user's desktop session. |
| `Invoke-GuestDesktop` | run a script on the signed-in user's desktop, through an interactive scheduled task (`guest\run-on-desktop.ps1`). Fonts loaded at sign-in and console windows exist only there. |
| `Copy-ToGuest` | copy files into the VM |
| `Test-GuestReady` | true once setup and the first logon are done, going by guest properties |
| `Wait-GuestReady` | waits until guestcontrol can actually run a command. Guest properties can be stale for a while after a restart inside the guest. |
| `Save-Clean` | shuts down cleanly, takes the `clean` snapshot, then starts again |
| `Get-VmState` | `running`, `poweroff`, … |
| `Send-Run` | Win+R, the text, Enter, typed into the VM |
| `Save-Shot`, `Measure-Color` | VM screenshot, then how many pixels have exactly one color. This checks whether a color scheme is on screen. |

The guest scripts live in `guest\`:
- `wt-cmd-scheme.ps1`: G7, G8
- `font-persist.ps1`: G12 after a restart
- `run-ci-probe.ps1`: reruns `spikes\windows\*.ps1`
- `proto-setup.ps1`: G4 and G17, and the time to the first prompt. It lays out what CMD Studio will install under `%LOCALAPPDATA%`, with Clink and Oh My Posh called by their full paths, and sets the guarded AutoRun. Open cmd with `Send-Run` afterwards.
- `proto-report.ps1`: after that cmd is open, prints Clink's G4 and G17 log lines, the time from cmd's start to the first input line, and Oh My Posh's prompt cost.
- `thai-prefs.ps1`: on VM-C, reads or sets the Thai UI, regional format and home location. guestcontrol may not see the user's HKCU, so run it with `Invoke-GuestDesktop`.
- `thai-clink-probes.ps1`: on VM-C, reruns `spikes/clink/run-probes.ps1` from inside the Thai profile.
- `thai-clink-workaround.ps1`: on VM-C, points AutoRun at Clink through the 8.3 path (`short`) or a copy in `C:\ProgramData` (`ascii`). Clink 1.9.34 cannot inject from a Thai folder on Thai Windows; see `docs/spikes/G4-G5-G16-G17-clink.md`.

- `add-thai-user.ps1`: adds `ผู้ทดสอบ ไทย` to a VM and switches auto-logon to it. Start it elevated through UAC on the VM's own screen. On VM-A this gives the case of a Thai user name on English Windows.
- `guard.test.ps1`: checks on the host, without a VM, that every script that changes settings refuses to run outside a `CMDSTUDIO-*` machine. It also checks that `wt-cmd-scheme.ps1` keeps the first backup of `settings.json`.

Every script that changes the user's settings starts with that guard. They are meant for these VMs only.

On VM-C, the Thai keyboard is active after sign-in, and `Send-Run 'cmd'` would type `แทก`. Press Win+Space first (`keyboardputscancode e0 5b 39 b9 e0 db`), then check that the tray shows ENG.

Before running them, copy these into the VM's `C:\cmdstudio`:
- the repo's `spikes\`
- the prototype's `clink-bin\` as `clink-bin\`
- `claude.omp.json` and `clink\claude-banner.lua` under `proto\`
