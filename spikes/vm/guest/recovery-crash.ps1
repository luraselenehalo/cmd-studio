# Runs inside a test VM (spike S9-S11, plan 3b-1 Task 12). -Swap puts the crash stand-in in the
# place of Clink's DLL in the runtime copy of $Data (never the dev PC's, never the installer's)
# and empties Clink's DLL cache, so the next cmd window crashes inside a module named
# clink_dll_x64.dll; -Restore puts the real DLL back. -Events saves the newest $Last
# "Application Error" events 1000 as XML into C:\cmdstudio\out, prints their EventData fields,
# and runs the app's XPath query (crates/core/src/eventlog.rs, crash_query, seven days). Close
# every cmd window before -Swap and -Restore.
param([switch] $Swap, [switch] $Restore, [switch] $Events, [int] $Last = 3,
  [string] $Data = (Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike3b'))
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script replaces Clink's DLL with one that crashes cmd" }
[Console]::OutputEncoding = [Text.Encoding]::UTF8
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
  $found = @(Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'Application Error'; Id = 1000 } -MaxEvents $Last -ErrorAction SilentlyContinue)
  "events 1000: $($found.Count)"
  $i = 0
  foreach ($e in $found) {
    $i++
    $xml = $e.ToXml()
    [IO.File]::WriteAllText((Join-Path $out "event1000-$i.xml"), $xml, (New-Object Text.UTF8Encoding $false))
    "--- event $i at $($e.TimeCreated.ToUniversalTime().ToString('o'))"
    ([xml]$xml).Event.EventData.Data | ForEach-Object { "  [$($_.Name)] = [$($_.'#text')]" }
  }
  $query = "*[System[Provider[@Name='Application Error'] and (EventID=1000) and TimeCreated[timediff(@SystemTime) <= 604800000]]]"
  $hits = @(Get-WinEvent -LogName Application -FilterXPath $query -ErrorAction SilentlyContinue)
  "the app's query finds: $($hits.Count)"
  '--- clink.log, last lines'
  $log = Join-Path $Data 'clink-profile\clink.log'
  if (Test-Path -LiteralPath $log) { Get-Content -LiteralPath $log -Encoding UTF8 -Tail 15 }
}
