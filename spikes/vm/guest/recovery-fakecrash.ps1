# Runs inside a test VM on the signed-in user's desktop (plan 3b-1 Task 12, the spike's fallback
# F10: Clink refuses the crash stand-in DLL, S11, so no real cmd window crashes in it). Starts
# C:\cmdstudio\fake\cmd.exe (fake-cmd.exe, spikes\recovery\fake-cmd, built on the host) $Runs
# times, each with the stand-in C:\cmdstudio\crash-dll\clink_dll_x64.dll as its argument: the
# program loads the DLL and crashes in it, and Windows writes one "Application Error" event 1000
# with AppName cmd.exe and ModuleName clink_dll_x64.dll, which the app's start check and doctor
# D12 count (docs\spikes\phase3b-recovery\f10-vm-a.txt). Each run is waited for, the next comes
# $GapSeconds later, and the WerFault processes still running are counted at the end. No window
# of cmd's own is started here. Run it with the theme applied: the app counts the events since
# applied_at_ms.
param([int] $Runs = 3, [int] $GapSeconds = 12,
  [string] $Exe = 'C:\cmdstudio\fake\cmd.exe', [string] $Dll = 'C:\cmdstudio\crash-dll\clink_dll_x64.dll')
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script crashes a program named cmd.exe in a module named clink_dll_x64.dll on purpose" }
foreach ($path in $Exe, $Dll) { if (-not (Test-Path -LiteralPath $path)) { throw "not in the VM: $path" } }
"fake cmd.exe: $((Get-FileHash -LiteralPath $Exe).Hash)"
"stand-in dll: $((Get-FileHash -LiteralPath $Dll).Hash)"
foreach ($i in 1..$Runs) {
  $p = Start-Process -FilePath $Exe -ArgumentList "`"$Dll`"" -PassThru
  $exited = $p.WaitForExit(60000)
  "fake cmd.exe run ${i}: pid $($p.Id) exited=$exited at $((Get-Date).ToString('HH:mm:ss'))"
  if (-not $exited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
  if ($i -lt $Runs) { Start-Sleep -Seconds $GapSeconds }
}
Start-Sleep -Seconds 5
"WerFault processes now: $(@(Get-Process WerFault -ErrorAction SilentlyContinue).Count)"
