# Runs inside a test VM (plan 3b-2 Task 12, docs\spikes\phase3b-recovery\vm-check-3b2.md): the
# app's run record (recovery spec §5.1) without a test build.
#   -FailedStarts n  writes <data>\run.json as a run of a pid that is gone would have left it
#                    (uiReadyAt null, failedStarts n): the next start counts one more, and from 2
#                    opens in recovery mode. The pid written is 4000000 - 4: Windows pids are
#                    multiples of 4 and far below it, so it is never a live process, and the app's
#                    OpenProcess of it answers "gone" (no `run record=` line).
#   -Kill            ends cmdstudio.exe the way a crash would (Stop-Process, no CloseMainWindow, so
#                    no Tauri exit event and no end_run): run.json stays with its uiReadyAt, and the
#                    next start shows the crash notice. This is the one place the protocol does not
#                    close the app with CloseMainWindow.
#   -Show            prints run.json, the first lines of the crash notes in <data>\logs\ and the
#                    log's lines of the run record, the recovery entry, the renderer watch, a pause
#                    and the app_* / crash_* / settings_reset commands (the last 20).
# The record's field names are the app's (serde camelCase: createdAt, startedAt, uiReadyAt,
# failedStarts, safeMode); version 1 is RUN_VERSION in app\src-tauri\src\backend\run.rs.
param([int] $FailedStarts = -1, [switch] $Kill, [switch] $Show, [string] $Data = (Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio'))
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script writes the app's data folder and kills processes" }
try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch { }
if (-not ($FailedStarts -ge 0 -or $Kill -or $Show)) { throw 'give -FailedStarts <n>, -Kill or -Show' }
$run = Join-Path $Data 'run.json'
if ($FailedStarts -ge 0) {
  if (-not (Test-Path -LiteralPath $Data)) { throw "no data folder at $Data (the app has not run yet)" }
  if (Get-Process cmdstudio -ErrorAction SilentlyContinue) { throw 'cmdstudio.exe is running: close it first, or its own end_run deletes the record' }
  $ms = [int64]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
  $json = '{"version":1,"pid":3999996,"createdAt":' + ($ms - 600000) + ',"startedAt":' + ($ms - 500000) + ',"uiReadyAt":null,"failedStarts":' + $FailedStarts + ',"safeMode":false}'
  [IO.File]::WriteAllText($run, $json + "`n", (New-Object Text.UTF8Encoding $false))
  "wrote run.json: failedStarts=$FailedStarts pid=3999996 (gone)"
}
if ($Kill) {
  $p = @(Get-Process cmdstudio -ErrorAction SilentlyContinue)
  if (-not $p) { throw 'cmdstudio.exe is not running' }
  $p | ForEach-Object { "killing cmdstudio pid $($_.Id) (started $($_.StartTime.ToUniversalTime().ToString('o')))"; Stop-Process -Id $_.Id -Force }
  Start-Sleep -Seconds 2
  "cmdstudio processes now: $(@(Get-Process cmdstudio -ErrorAction SilentlyContinue).Count)"
  'run.json after the kill: ' + $(if (Test-Path -LiteralPath $run) { 'still there' } else { 'gone (not a crash)' })
}
if ($Show) {
  if (Test-Path -LiteralPath $run) { 'run.json = ' + (Get-Content -Raw -LiteralPath $run -Encoding UTF8).Trim() } else { '(no run.json)' }
  $logs = Join-Path $Data 'logs'
  $notes = @(Get-ChildItem -LiteralPath $logs -Filter 'crash-*.txt' -ErrorAction SilentlyContinue | Sort-Object Name)
  "crash notes: $($notes.Count)"
  foreach ($n in $notes) { '--- ' + $n.Name; Get-Content -LiteralPath $n.FullName -TotalCount 6 -Encoding UTF8 }
  $log = Join-Path $logs 'cmdstudio.log'
  if (Test-Path -LiteralPath $log) {
    '--- cmdstudio.log (the last 20 matching lines)'
    Select-String -LiteralPath $log -Pattern ' run |recovery_entry|renderer |pause reason|resume by|crash_check|cmd=app_|cmd=crash_|cmd=settings_reset|cmd=log_ui_error' | ForEach-Object Line | Select-Object -Last 20
  } else { "(no log at $log)" }
}
