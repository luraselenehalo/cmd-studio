# Runs inside a test VM on the signed-in user's desktop (Invoke-GuestDesktop), spike S8 and plan
# 3b-1 Task 12 step 7: starts cmd.exe $Runs times, each time waits for zz-timing.lua's marker of
# the window's first edit line (the first clink.onbeginedit: after every script has loaded, the
# start counter included, and before the prompt filters; a filter after Oh My Posh's never runs,
# spike S6), and prints the time from the process's start to the marker, then the average.
# -Data points at another data folder (Task 12: CMD Studio's own); -InstallMarker first writes
# zz-timing.lua into its generated\ folder from C:\cmdstudio\spikes\recovery\timing.lua.
# Each window is closed as soon as its marker is there, before its prompt, so the window's own
# start tag (health\start-<pid>, written by the counter at load) would stay and be counted as a
# start that failed. The script takes that tag away after closing the window, and the marker
# file with it, and says how many tags it found: a timing run leaves nothing for the next window
# to count. It does not touch zz-timing.lua: remove it when the measuring is over.
# The first $Warmup windows (1) are shown but not counted in the average: the first start after a
# setup pays for Defender's scan and Clink's DLL cache copy, and that would fall on one side of a
# without/with-the-counter comparison only. Every call warms up the same way, so the two averages
# are alike in that. In the spike the deciding number for the counter's cost was the `p3b counter
# ms=` line (recovery-setup.ps1 -Counter); the shipped init.lua logs no such line, so Task 12
# compares this average with the spike's without the counter (timing-vm-a.txt: 815 ms on VM-A).
param([int] $Runs = 10, [string] $Data = (Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike3b'), [switch] $InstallMarker, [int] $Warmup = 1)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script starts and closes cmd windows" }
if ($InstallMarker) {
  $text = "local p3b_data = [==[$Data]==]`n" + [IO.File]::ReadAllText('C:\cmdstudio\spikes\recovery\timing.lua')
  [IO.File]::WriteAllText((Join-Path $Data 'generated\zz-timing.lua'), $text, (New-Object Text.UTF8Encoding $false))
}
$tagsFound = 0
$times = foreach ($i in 1..($Warmup + $Runs)) {
  $p = Start-Process -FilePath "$env:SystemRoot\System32\cmd.exe" -PassThru
  $marker = Join-Path $Data "first-prompt-$($p.Id).txt"
  $deadline = (Get-Date).AddSeconds(20)
  while (-not (Test-Path -LiteralPath $marker) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 50 }
  if (Test-Path -LiteralPath $marker) { $one = [int]((Get-Item -LiteralPath $marker).LastWriteTime - $p.StartTime).TotalMilliseconds }
  else { $one = "no marker for pid $($p.Id)" }
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 2
  # The window never reached its prompt, so its tag is still there: not a start that failed.
  $tag = Join-Path $Data "health\start-$($p.Id)"
  if (Test-Path -LiteralPath $tag) { $tagsFound++; Remove-Item -LiteralPath $tag -Force }
  Remove-Item -LiteralPath $marker -Force -ErrorAction SilentlyContinue
  # A warm-up line is text, so the average below leaves it out.
  if ($i -le $Warmup) { "warm-up, not counted: $one" } else { $one }
}
$times
$ms = @($times | Where-Object { $_ -is [int] })
if ($ms.Count) { "average of $($ms.Count): $([int](($ms | Measure-Object -Average).Average)) ms" }
"start tags of the timed windows taken away: $tagsFound of $($Warmup + $Runs)"
