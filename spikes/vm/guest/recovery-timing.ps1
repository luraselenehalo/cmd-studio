# Runs inside a test VM on the signed-in user's desktop (Invoke-GuestDesktop), spike S8 and plan
# 3b-1 Task 12: starts cmd.exe $Runs times, each time waits for zz-timing.lua's marker of its
# first prompt, and prints the time from the process's start to the marker, then the average.
# Each window is closed after its prompt is up, so it leaves no tag. -Data points at another data
# folder (Task 12: CMD Studio's own); -InstallMarker first writes zz-timing.lua into its
# generated\ folder.
param([int] $Runs = 10, [string] $Data = (Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike3b'), [switch] $InstallMarker)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script starts and closes cmd windows" }
if ($InstallMarker) {
  $text = "local p3b_data = [==[$Data]==]`n" + [IO.File]::ReadAllText('C:\cmdstudio\spikes\recovery\timing.lua')
  [IO.File]::WriteAllText((Join-Path $Data 'generated\zz-timing.lua'), $text, (New-Object Text.UTF8Encoding $false))
}
$times = foreach ($i in 1..$Runs) {
  $p = Start-Process -FilePath "$env:SystemRoot\System32\cmd.exe" -PassThru
  $marker = Join-Path $Data "first-prompt-$($p.Id).txt"
  $deadline = (Get-Date).AddSeconds(20)
  while (-not (Test-Path -LiteralPath $marker) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 50 }
  if (Test-Path -LiteralPath $marker) { [int]((Get-Item -LiteralPath $marker).LastWriteTime - $p.StartTime).TotalMilliseconds }
  else { "no marker for pid $($p.Id)" }
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 2
}
$times
$ms = @($times | Where-Object { $_ -is [int] })
if ($ms.Count) { "average of $($ms.Count): $([int](($ms | Measure-Object -Average).Average)) ms" }
