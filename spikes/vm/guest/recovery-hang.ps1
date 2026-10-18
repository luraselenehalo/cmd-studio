# Runs inside a test VM (plan 3b-1 Task 12). -Swap puts the hang stand-in (spikes\recovery\hang,
# built on the host and copied to C:\cmdstudio\hang\oh-my-posh.exe) in the place of
# oh-my-posh.exe in CMD Studio's runtime under $Data, keeping the real one beside it as
# oh-my-posh.exe.real: init.lua then waits on `oh-my-posh init cmd`, which never answers, and the
# cmd window never reaches its prompt, as one that fails to start. -Restore stops every stand-in
# still running and puts the real one back. Close CMD Studio and every cmd window before either.
# The exe swapped is the one init.lua runs: its `local omp = "..."` line (written by lua_str:
# \\ for a backslash, \" for a quote, \ddd for a control), so a launch through an 8.3 path (VM-C,
# state.json launch shortPath) swaps the file the window loads. Without a readable init.lua the
# default <data>\runtime\omp-31.5.0\oh-my-posh.exe is taken.
param([switch] $Swap, [switch] $Restore,
  [string] $Data = (Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio'))
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script replaces Oh My Posh with a program that never answers" }
if (-not ($Swap -or $Restore)) { throw 'give -Swap or -Restore' }
try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch { }
# The path init.lua runs, unescaped as lua_str wrote it; the default when there is no such line.
function Get-OmpPath {
  $init = Join-Path $Data 'generated\init.lua'
  $default = Join-Path $Data 'runtime\omp-31.5.0\oh-my-posh.exe'
  if (-not (Test-Path -LiteralPath $init)) { return $default }
  $line = @(Get-Content -LiteralPath $init -Encoding UTF8 | Where-Object { $_ -match '^local omp = "(.*)"$' })
  if (-not $line.Count) { return $default }
  $raw = [regex]::Match($line[0], '^local omp = "(.*)"$').Groups[1].Value
  $unescape = [System.Text.RegularExpressions.MatchEvaluator] {
    param($m)
    $x = $m.Groups[1].Value
    if ($x -eq '\') { '\' } elseif ($x -eq '"') { '"' } else { [string][char][int]$x }
  }
  [regex]::Replace($raw, '\\(\\|"|\d{3})', $unescape)
}
$omp = Get-OmpPath
$real = "$omp.real"
"oh-my-posh: $omp"
if ($Swap) {
  if (-not (Test-Path -LiteralPath $omp)) { throw "no Oh My Posh at ${omp}: is the theme applied?" }
  if (-not (Test-Path -LiteralPath $real)) { Copy-Item -LiteralPath $omp -Destination $real }
  Copy-Item -LiteralPath 'C:\cmdstudio\hang\oh-my-posh.exe' -Destination $omp -Force
  "swapped: $((Get-FileHash -LiteralPath $omp).Hash)"
}
if ($Restore) {
  if (-not (Test-Path -LiteralPath $real)) { throw "nothing to restore: $real is not there" }
  Get-Process oh-my-posh -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Seconds 1
  Copy-Item -LiteralPath $real -Destination $omp -Force
  Remove-Item -LiteralPath $real
  "restored: $((Get-FileHash -LiteralPath $omp).Hash)"
}
