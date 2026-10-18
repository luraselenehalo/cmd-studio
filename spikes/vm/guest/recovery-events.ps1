# Runs inside a test VM (plan 3b-1 Task 12, step 7b of docs\spikes\phase3b-recovery\vm-check.md):
# writes $Count "Application Error" events 1000 with a filler message into the Application log, so
# that the app's start check reads a log holding more than MOST_EVENTS (1000, crates\core\src\
# adapters\windows\eventlog.rs) events 1000 of the last seven days. They are no crash of cmd's:
# Write-EventLog puts the message into one EventData field without a name, and parse_crash
# (crates\core\src\eventlog.rs) takes the program and the module from the first and the fourth
# of unnamed fields, so these have no module and count for nothing; the real crashes of the steps
# before are older than the fillers, so the newest 1000 the reader keeps hold none of them. The
# source "Application Error" is Windows' own (WER) and is registered on every Windows; the
# Application log lets an interactive user write (if Write-EventLog is refused all the same,
# record it: the step is then not run). The events stay until the log rolls over or the VM goes
# back to clean. Prints how long the writes took and how many events 1000 of that source the log
# holds now.
param([int] $Count = 1100)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script writes $Count events into the Application log" }
$filter = @{ LogName = 'Application'; ProviderName = 'Application Error'; Id = 1000 }
"events 1000 from 'Application Error' in the log before: $(@(Get-WinEvent -FilterHashtable $filter -ErrorAction SilentlyContinue).Count)"
$sw = [Diagnostics.Stopwatch]::StartNew()
foreach ($i in 1..$Count) {
  Write-EventLog -LogName Application -Source 'Application Error' -EventId 1000 -EntryType Error -Message "CMD Studio phase 3b-1 VM check (vm-check.md step 7b): filler event $i of $Count, not a crash"
}
$sw.Stop()
"wrote $Count events 1000 from 'Application Error' in $([int]$sw.Elapsed.TotalSeconds) s"
"events 1000 from 'Application Error' in the log now: $(@(Get-WinEvent -FilterHashtable $filter -ErrorAction SilentlyContinue).Count)"
$newest = Get-WinEvent -FilterHashtable $filter -MaxEvents 1 -ErrorAction SilentlyContinue
if ($newest) { "newest: $($newest.TimeCreated.ToUniversalTime().ToString('o'))" }
