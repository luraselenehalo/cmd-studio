# Runs inside a test VM (spike S9-S11, plan 3b-1 Task 12). -Swap puts the crash stand-in in the
# place of Clink's DLL in the runtime copy of $Data (never the dev PC's, never the installer's)
# and empties Clink's DLL cache, so the next cmd window crashes inside a module named
# clink_dll_x64.dll; -Restore puts the real DLL back. -Events saves the newest $Last
# "Application Error" events 1000 as XML into C:\cmdstudio\out, prints their EventData fields,
# and runs the app's XPath query (crates/core/src/eventlog.rs, crash_query, seven days), then the
# same query with `and EventData[Data='cmd.exe']` (plan 3b-1 Task 4b), each with its own count
# and, when Windows answered with an error, its error id and message: a refused query and a query
# that matches nothing are told apart by those. Close every cmd window before -Swap and -Restore.
#
# About the stand-in itself (built on the host with cargo only, never started there): run cargo
# from spikes\recovery\crash-dll, because .cargo\config.toml (the C runtime linked in) applies
# only when cargo runs from that folder. Before the DLL is copied to a VM,
# `dumpbin /dependents clink_dll_x64.dll` must list KERNEL32.dll and nothing else: a
# VCRUNTIME140.dll there means the flag was not applied, and a VM without the Visual C++ runtime
# would refuse to load the DLL instead of crashing in it.
param([switch] $Swap, [switch] $Restore, [switch] $Events, [int] $Last = 3,
  [string] $Data = (Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike3b'))
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script replaces Clink's DLL with one that crashes cmd" }
[Console]::OutputEncoding = [Text.Encoding]::UTF8
# Windows' answer to a query as text. Both a terminating and a non-terminating error end up here.
function Say-Error($e) { if ($e) { "  error: $($e[0].FullyQualifiedErrorId): $($e[0].Exception.Message)" } }
function Find-Events([string] $Label, [string] $XPath) {
  $err = $null
  $hits = @()
  try { $hits = @(Get-WinEvent -LogName Application -FilterXPath $XPath -ErrorAction SilentlyContinue -ErrorVariable err) }
  catch { $err = @($_) }
  "${Label}: $($hits.Count)"
  Say-Error $err
}
$dll = Join-Path $Data 'runtime\clink-1.9.34\clink_dll_x64.dll'
$cache = Join-Path $env:TEMP 'clink\dll_cache'
if ($Swap) {
  Copy-Item 'C:\cmdstudio\crash-dll\clink_dll_x64.dll' $dll -Force
  Remove-Item -LiteralPath $cache -Recurse -Force -ErrorAction SilentlyContinue
  "swapped: $((Get-Item $dll).VersionInfo.FileVersion) $((Get-FileHash $dll).Hash)"
}
if ($Restore) {
  Copy-Item 'C:\cmdstudio\resources\clink\clink_dll_x64.dll' $dll -Force
  Remove-Item -LiteralPath $cache -Recurse -Force -ErrorAction SilentlyContinue
  "restored: $((Get-FileHash $dll).Hash)"
}
if ($Events) {
  $out = 'C:\cmdstudio\out'
  New-Item -ItemType Directory -Force $out | Out-Null
  $found = @()
  $ferr = $null
  try { $found = @(Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'Application Error'; Id = 1000 } -MaxEvents $Last -ErrorAction SilentlyContinue -ErrorVariable ferr) }
  catch { $ferr = @($_) }
  "events 1000: $($found.Count)"
  Say-Error $ferr
  $i = 0
  foreach ($e in $found) {
    $i++
    $xml = $e.ToXml()
    [IO.File]::WriteAllText((Join-Path $out "event1000-$i.xml"), $xml, (New-Object Text.UTF8Encoding $false))
    "--- event $i at $($e.TimeCreated.ToUniversalTime().ToString('o'))"
    ([xml]$xml).Event.EventData.Data | ForEach-Object { "  [$($_.Name)] = [$($_.'#text')]" }
  }
  $query = "*[System[Provider[@Name='Application Error'] and (EventID=1000) and TimeCreated[timediff(@SystemTime) <= 604800000]]]"
  Find-Events "the app's query finds" $query
  $query2 = "*[System[Provider[@Name='Application Error'] and (EventID=1000) and TimeCreated[timediff(@SystemTime) <= 604800000]] and EventData[Data='cmd.exe']]"
  Find-Events "with EventData[Data='cmd.exe']" $query2
  '--- clink.log, last lines'
  $log = Join-Path $Data 'clink-profile\clink.log'
  if (Test-Path -LiteralPath $log) { Get-Content -LiteralPath $log -Encoding UTF8 -Tail 15 }
}
