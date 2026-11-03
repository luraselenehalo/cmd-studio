# Runs inside a test VM (plan 3b-1 Task 12, step 2d of docs\spikes\phase3b-recovery\vm-check.md):
# times the command the start counter runs once at its pause decision (running_cmds in
# HEALTH_BODY, crates\core\src\generate\lua.rs):
#   "<SystemRoot>\System32\tasklist.exe" /FO CSV /NH 2>nul
# through cmd, as io.popen hands it over: cmd /c "<the command>", the outer quotes stripped by
# cmd. $Runs times with `cmd /d /c` (the command's own cost, AutoRun off) and $Runs times with
# `cmd /c` (as the C runtime's popen starts it: AutoRun runs in that cmd too, and Clink's autorun
# declines a cmd /c, spike S7), each from the process's creation to the end of its output, with
# the rows read as the counter reads them, then the averages.
param([int] $Runs = 5)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script starts cmd and tasklist" }
$exe = Join-Path $env:SystemRoot 'System32\tasklist.exe'
$inner = '"' + $exe + '" /FO CSV /NH 2>nul'
function Measure-TasklistRun([string] $Switches) {
  $psi = New-Object Diagnostics.ProcessStartInfo
  $psi.FileName = Join-Path $env:SystemRoot 'System32\cmd.exe'
  $psi.Arguments = $Switches + ' "' + $inner + '"'
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.CreateNoWindow = $true
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $p = [Diagnostics.Process]::Start($psi)
  $text = $p.StandardOutput.ReadToEnd()
  $p.WaitForExit()
  $sw.Stop()
  $rows = @($text -split "`r?`n" | Where-Object { $_ -match '^"[^"]*","\d+",' })
  [pscustomobject]@{
    Ms = $sw.Elapsed.TotalMilliseconds
    Rows = $rows.Count
    Cmd = @($rows | Where-Object { $_ -match '^"cmd\.exe","\d+",' }).Count
    Exit = $p.ExitCode
  }
}
foreach ($case in @(@{ Label = 'cmd /d /c (the command alone, AutoRun off)'; Switches = '/d /c' }, @{ Label = 'cmd /c (as io.popen starts it, AutoRun on)'; Switches = '/c' })) {
  "--- $($case.Label)"
  "command line: cmd.exe $($case.Switches) `"$inner`""
  $times = New-Object System.Collections.Generic.List[double]
  foreach ($i in 1..$Runs) {
    $r = Measure-TasklistRun $case.Switches
    "run ${i}: $([int]$r.Ms) ms, rows=$($r.Rows), cmd.exe rows=$($r.Cmd), exit=$($r.Exit)"
    $times.Add($r.Ms)
  }
  "average of ${Runs}: $([int](($times | Measure-Object -Average).Average)) ms"
}
