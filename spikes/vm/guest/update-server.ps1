# Runs inside a test VM (plan 4b Tasks 1 and 9): a small web server on the guest's own loopback,
# http://127.0.0.1:8765/, over one folder, for the updater's latest.json and the spike's installers,
# so that nothing listens on the host and no firewall changes (the ledger's ruling). -Start runs
# the listener as a hidden process of its own, created through WMI so that it is not a child of
# the desktop task that starts it, writes its pid to <Root>\server.pid, waits 3 s and says whether
# the port answers; it refuses when a server of server.pid still runs or the port already answers,
# and removes a stale server.pid. -Stop ends that process (only if server.pid names this script's
# -Serve) and removes server.pid, stale or not; -Serve is the listener itself (what -Start runs). GET only:
# the path is decoded (%20), a file directly under -Root is answered with its bytes and its
# Content-Length, anything else with 404; one request per connection; one line per request is
# appended to <Root>\server.log (U7 reads it: no line in Off mode). A TcpListener, not
# HttpListener: no http.sys URL ACL, so the signed-in user may listen without admin rights.
param(
  [switch] $Start, [switch] $Stop, [switch] $Serve,
  [string] $Root = 'C:\cmdstudio\phase4b\setup',
  [int] $Port = 8765
)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script listens on a port and starts programs" }
$pidFile = Join-Path $Root 'server.pid'
$logFile = Join-Path $Root 'server.log'
$utf8 = New-Object Text.UTF8Encoding $false
function Answers { try { $c = New-Object Net.Sockets.TcpClient -ArgumentList '127.0.0.1', $Port; $c.Close(); $true } catch { $false } }
# The pid of server.pid if that process is this script's -Serve (its command line says so), else
# $null: a pid Windows has given to another process since is never ended.
function OurServer {
  if (-not (Test-Path -LiteralPath $pidFile)) { return $null }
  $id = 0
  if (-not [int]::TryParse("$(Get-Content -LiteralPath $pidFile -Raw)".Trim(), [ref]$id)) { return $null }
  $p = Get-CimInstance -ClassName Win32_Process -Filter "ProcessId = $id" -ErrorAction SilentlyContinue
  if ($p -and "$($p.CommandLine)" -like '*update-server.ps1*-Serve*') { $id } else { $null }
}
if ($Start) {
  $running = OurServer
  if ($running) { throw "a server already runs (pid $running from $pidFile): -Stop first" }
  if (Test-Path -LiteralPath $pidFile) { Remove-Item -LiteralPath $pidFile; 'stale server.pid removed' }
  if (Answers) { throw "127.0.0.1:$Port already answers, and not from a server.pid of $Root" }
  # Through WMI (Win32_Process Create), not Start-Process: the listener's parent is then the WMI
  # provider host, not this script, so it is outside the desktop task's process tree and job and
  # outlives the task (run-on-desktop.ps1 unregisters the task when this script returns). The
  # host's Test-Phase4bServer checks that from a separate guestcontrol call.
  $ps = "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe"
  $line = "`"$ps`" -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$PSCommandPath`" -Serve -Root `"$Root`" -Port $Port"
  $r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $line }
  if ($r.ReturnValue -ne 0) { throw "Win32_Process Create answered $($r.ReturnValue)" }
  [IO.File]::WriteAllText($pidFile, "$($r.ProcessId)", $utf8)
  Start-Sleep -Seconds 3
  "server pid $($r.ProcessId) on 127.0.0.1:$Port over $Root; answers: $(Answers)"
}
if ($Stop) {
  if (-not (Test-Path -LiteralPath $pidFile)) { "no server.pid: nothing to stop; answers: $(Answers)"; return }
  $id = OurServer
  if ($id) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
  Remove-Item -LiteralPath $pidFile
  Start-Sleep -Seconds 1
  $what = if ($id) { "server $id stopped" } else { 'server.pid named no running server: stale server.pid removed' }
  "${what}; answers: $(Answers)"
}
if ($Serve) {
  $listener = New-Object Net.Sockets.TcpListener -ArgumentList ([Net.IPAddress]::Loopback), $Port
  $listener.Start()
  while ($true) {
    $client = $listener.AcceptTcpClient()
    try {
      $stream = $client.GetStream()
      $reader = New-Object IO.StreamReader -ArgumentList $stream, ([Text.Encoding]::ASCII)
      $request = $reader.ReadLine()
      while ($true) { $line = $reader.ReadLine(); if ($null -eq $line -or $line -eq '') { break } }
      $parts = "$request" -split ' '
      $name = if ($parts.Count -ge 2) { [Uri]::UnescapeDataString($parts[1].TrimStart('/')) } else { '' }
      $file = Join-Path $Root $name
      $ok = ($parts.Count -ge 2) -and ($parts[0] -eq 'GET') -and ($name -ne '') -and ($name -notmatch '[\\/]|\.\.') -and (Test-Path -LiteralPath $file -PathType Leaf)
      $body = if ($ok) { [IO.File]::ReadAllBytes($file) } else { [Text.Encoding]::ASCII.GetBytes('not found') }
      $status = if ($ok) { '200 OK' } else { '404 Not Found' }
      $type = if ($name -like '*.json') { 'application/json' } else { 'application/octet-stream' }
      $head = [Text.Encoding]::ASCII.GetBytes("HTTP/1.1 $status`r`nContent-Type: $type`r`nContent-Length: $($body.Length)`r`nConnection: close`r`n`r`n")
      $stream.Write($head, 0, $head.Length)
      $stream.Write($body, 0, $body.Length)
      $stream.Flush()
      [IO.File]::AppendAllText($logFile, "$(Get-Date -Format o) $request -> $status`n", $utf8)
    } catch {
      [IO.File]::AppendAllText($logFile, "$(Get-Date -Format o) error: $($_.Exception.Message)`n", $utf8)
    } finally { $client.Close() }
  }
}
