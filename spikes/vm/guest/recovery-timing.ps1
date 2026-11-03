# Runs inside a test VM on the signed-in user's desktop (Invoke-GuestDesktop), spike S8 and plan
# 3b-1 Task 12 step 7: starts cmd.exe $Runs times, each time waits for zz-timing.lua's marker of
# the window's first edit line (the first clink.onbeginedit: after every script has loaded, the
# start counter included, and before the prompt filters; a filter after Oh My Posh's never runs,
# spike S6), and prints the time from the process's start to the marker, then on one line the
# average, the median, the minimum and the maximum of the counted runs
# (`average of N: <n> ms; median <n> ms; min <n> ms; max <n> ms`).
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
# ms=` line (recovery-setup.ps1 -Counter); the shipped init.lua logs no such line. 3b-1's Task 12
# compared this average with the spike's without the counter (timing-vm-a.txt: 815 ms on VM-A),
# measured on another boot from another init.lua, which settled nothing (its finding F4). Plan
# 3b-2 Task 12 (vm-check-3b2.md step 10) compares two runs of this script in one boot instead:
# `-InstallMarker` once, then the script with the shipped init.lua, then `-WithoutCounter`, which
# takes the counter's block (HEALTH_BODY) out of init.lua for its run and puts the file back.
# The counter's cost per start is the difference of the two averages. A run without the counter
# writes no start tags, so its last line says `0 of N` taken away: expected, not a fault.
param([int] $Runs = 10, [string] $Data = (Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike3b'), [switch] $InstallMarker, [int] $Warmup = 1, [switch] $WithoutCounter)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script starts and closes cmd windows" }
if ($InstallMarker) {
  $text = "local p3b_data = [==[$Data]==]`n" + [IO.File]::ReadAllText('C:\cmdstudio\spikes\recovery\timing.lua')
  [IO.File]::WriteAllText((Join-Path $Data 'generated\zz-timing.lua'), $text, (New-Object Text.UTF8Encoding $false))
}
# Carry F4 of plan 3b-1: the same boot, the same data folder, the same marker, once with the
# shipped init.lua and once with the start counter (HEALTH_BODY) cut out of it. The cut is from
# the line that starts the counter to the line before Oh My Posh's part (the first line of
# INIT_BODY in crates\core\src\generate\lua.rs); the original is put back afterwards, whatever
# happens. The counter's cost per start is the difference of the two averages.
$initLua = Join-Path $Data 'generated\init.lua'
$backup = "$initLua.with-counter"
if ($WithoutCounter) {
  if (Test-Path -LiteralPath $backup) { throw "$backup is still there: an earlier run did not put init.lua back; move it back by hand first" }
  $lines = [IO.File]::ReadAllLines($initLua)
  $from = [Array]::FindIndex($lines, [Predicate[string]]{ param($l) $l.StartsWith('-- The start counter (recovery spec') })
  $to = [Array]::FindIndex($lines, [Predicate[string]]{ param($l) $l.StartsWith('-- cmd /c drops the outermost quotes') })
  if ($from -lt 0 -or $to -le $from) { throw "init.lua has no counter block to cut (from=$from to=$to)" }
  Copy-Item -LiteralPath $initLua -Destination $backup -Force
  $cut = $lines[0..($from - 1)] + $lines[$to..($lines.Length - 1)]
  [IO.File]::WriteAllLines($initLua, $cut, (New-Object Text.UTF8Encoding $false))
  "init.lua without the counter: $($lines.Length - $cut.Length) lines cut (from line $($from + 1) to $to)"
}
try {
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
if ($ms.Count) {
  $sorted = @($ms | Sort-Object)
  $mid = [int][Math]::Floor($sorted.Count / 2)
  $median = if ($sorted.Count % 2) { $sorted[$mid] } else { [int](($sorted[$mid - 1] + $sorted[$mid]) / 2) }
  "average of $($ms.Count): $([int](($ms | Measure-Object -Average).Average)) ms; median $median ms; min $($sorted[0]) ms; max $($sorted[-1]) ms"
}
"start tags of the timed windows taken away: $tagsFound of $($Warmup + $Runs)"
} finally {
  if ($WithoutCounter -and (Test-Path -LiteralPath $backup)) { Move-Item -LiteralPath $backup -Destination $initLua -Force; 'init.lua restored' }
}
