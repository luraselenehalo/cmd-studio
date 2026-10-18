# Runs inside a test VM on the signed-in user's desktop (plan 3b-1 Task 12, step 2b of
# docs\spikes\phase3b-recovery\vm-check.md). -Open starts $Count cmd windows as
# `cmd /k "ping -t localhost"` (no /d: AutoRun injects Clink, whose start counter writes the
# window's tag; the prompt never comes while ping runs), writes their pids to
# C:\cmdstudio\out\ping-pids.txt and prints them. -Close stops those cmd.exe processes and every
# ping.exe, and prints what it stopped. A window still running keeps its tag from being counted at
# the pause decision (tasklist, HEALTH_BODY in crates\core\src\generate\lua.rs); the first prompt
# of any later window deletes every tag older than 60 s all the same.
param([switch] $Open, [switch] $Close, [int] $Count = 3)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script opens cmd windows with the user's AutoRun" }
if (-not ($Open -or $Close)) { throw 'give -Open or -Close' }
$list = 'C:\cmdstudio\out\ping-pids.txt'
if ($Open) {
  New-Item -ItemType Directory -Force (Split-Path $list) | Out-Null
  $pids = New-Object System.Collections.Generic.List[string]
  foreach ($i in 1..$Count) {
    $p = Start-Process -FilePath "$env:SystemRoot\System32\cmd.exe" -ArgumentList '/k', 'ping -t localhost' -PassThru
    "ping window ${i}: cmd pid $($p.Id) opened at $((Get-Date).ToString('HH:mm:ss'))"
    $pids.Add("$($p.Id)")
    Start-Sleep -Seconds 3
  }
  Set-Content -LiteralPath $list -Value $pids -Encoding ASCII
  "pids written to $list"
}
if ($Close) {
  if (-not (Test-Path -LiteralPath $list)) { throw "no pid list at ${list}: was -Open run?" }
  foreach ($id in Get-Content -LiteralPath $list) {
    $p = Get-Process -Id ([int]$id) -ErrorAction SilentlyContinue
    if ($p -and $p.ProcessName -eq 'cmd') { Stop-Process -Id $p.Id -Force; "stopped cmd pid $id" } else { "cmd pid $id was not running" }
  }
  $pings = @(Get-Process ping -ErrorAction SilentlyContinue)
  $pings | Stop-Process -Force -ErrorAction SilentlyContinue
  "stopped ping.exe processes: $($pings.Count)"
  Remove-Item -LiteralPath $list
}
