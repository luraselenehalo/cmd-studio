# Host driver of the Windows Terminal pre-check (run 4 of protocol.md). It restores VM-A to its
# "clean" snapshot, whose Terminal was never opened, writes a settings.json by hand before
# Terminal's first start, starts Terminal, and reports whether the first start used the file
# (the pixels of the scheme's background, #1F1E1D) and what the file holds afterwards.
# The scheme is written without opacity or acrylic on purpose, so the exact count is meaningful.
# Run it in Windows PowerShell 5.1 on the host, in an unlocked session and outside any sandbox,
# because it powers a VM off and on with VBoxManage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File .\docs\spikes\phase3-vm-check\run.ps1
# It works from any folder: the repo root is found from this file's place.
# Do step 0 of protocol.md first. Its facts belong to the snapshot, so they hold for every
# restore, and this run starts Terminal, which ends the never-opened state of this boot.
$ErrorActionPreference = 'Continue'
$here = $PSScriptRoot
$repo = (Resolve-Path (Join-Path $here '..\..\..')).Path
# vm.ps1 defines a type, which a second dot-source in one session cannot add again.
if (-not (Get-Command Wait-GuestReady -ErrorAction SilentlyContinue)) { . (Join-Path $repo 'spikes\vm\vm.ps1') }
$vm = 'cmdstudio-win11'; $u = 'tester'
if ((Get-VmState $vm) -ne 'poweroff') { & $Vbm controlvm $vm poweroff; Start-Sleep -Seconds 5 }
& $Vbm snapshot $vm restore clean
& $Vbm startvm $vm --type headless
if (-not (Wait-GuestReady $vm $u)) { throw "$vm is not ready" }
# Invoke-GuestDesktop runs C:\cmdstudio\spikes\vm\guest\run-on-desktop.ps1, which a snapshot
# taken before any probe may not have; copying it again does no harm.
Copy-ToGuest $vm @((Join-Path $repo 'spikes\vm\guest\run-on-desktop.ps1')) 'C:\cmdstudio\spikes\vm\guest\' $u
Copy-ToGuest $vm @((Join-Path $here 'wt-precreate.ps1')) 'C:\cmdstudio\phase3\check\' $u
$g = 'C:\cmdstudio\phase3\check\wt-precreate.ps1'
# The log goes to spikes\vm\tmp, which git ignores, not into this folder.
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
"log: $log"
