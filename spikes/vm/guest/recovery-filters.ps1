# Runs inside a test VM (plan 3b-1 Task 12, steps 2a and 2f of docs\spikes\phase3b-recovery\
# vm-check.md). Puts one of two prompt-filter probes from C:\cmdstudio\spikes\recovery into CMD
# Studio's own generated\ under $Data, where Clink loads it beside init.lua, reports what the
# probe wrote, and takes it out again:
#   -Probe raise     generated\zz-raise.lua (raise.lua): a prompt filter at -Priority (1, Oh My
#                    Posh's own) whose first call raises, as an Oh My Posh that raises at run time
#                    would. Does CMD Studio's prompt coroutine still run and take the window's tag
#                    away, and does the window reach a prompt (recovery spec §13)?
#   -Probe refilter  generated\aa-refilter.lua (refilter.lua, loaded before init.lua): a prompt
#                    filter at -Priority (-1000, before CMD Studio's) that writes the time of each
#                    filter call of the window's first line, and wraps the filters registered
#                    after it with a start and an end line each: when the coroutine ends, Clink
#                    refilters the prompt once (§13); the lines say how often, and what it cost.
# -Install writes the probe with `local p3b_data = [==[<data>]==]` and `local p3b_priority = <n>`
# in front; -Report prints the lines of every <data>\<probe>-<pid>.txt the probe wrote; -Remove
# deletes the probe and those files. Close every cmd window before -Install and -Remove.
param([ValidateSet('raise', 'refilter')] [string] $Probe = 'raise', [switch] $Install, [switch] $Report, [switch] $Remove,
  [string] $Priority = '', [string] $Data = (Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio'))
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script puts a prompt filter into CMD Studio's generated scripts" }
if (-not ($Install -or $Report -or $Remove)) { throw 'give -Install, -Report or -Remove' }
try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch { }
$names = @{ raise = 'zz-raise.lua'; refilter = 'aa-refilter.lua' }
$defaults = @{ raise = '1'; refilter = '-1000' }
if ($Priority -eq '') { $Priority = $defaults[$Probe] }
if ($Priority -notmatch '^-?\d+(\.\d+)?$') { throw "-Priority must be a number, not '$Priority'" }
$lua = Join-Path $Data ('generated\' + $names[$Probe])
$marks = Join-Path $Data "$Probe-*.txt"
if ($Install) {
  $head = "local p3b_data = [==[$Data]==]`nlocal p3b_priority = $Priority`n"
  $body = [IO.File]::ReadAllText("C:\cmdstudio\spikes\recovery\$Probe.lua")
  [IO.File]::WriteAllText($lua, $head + $body, (New-Object Text.UTF8Encoding $false))
  "installed: $lua (priority $Priority)"
}
if ($Report) {
  $files = @(Get-ChildItem -Path $marks -ErrorAction SilentlyContinue)
  if (-not $files.Count) { "no $Probe-<pid>.txt under $Data" }
  foreach ($f in $files) {
    "--- $($f.Name)"
    Get-Content -LiteralPath $f.FullName -Encoding UTF8
  }
}
if ($Remove) {
  $gone = @()
  foreach ($path in @($lua) + @(Get-ChildItem -Path $marks -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })) {
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force; $gone += $path }
  }
  "removed: $($gone.Count) file(s)"
  $gone | ForEach-Object { "  $_" }
}
