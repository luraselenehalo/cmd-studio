# Runs inside a test VM (plan 4b Tasks 1 and 9): a small web server on the guest's own loopback,
# http://127.0.0.1:8765/, over one folder, for the updater's latest.json and the spike's installers,
# so that nothing listens on the host and no firewall changes (the ledger's ruling). -Start runs
# the listener as a hidden process of its own (as 4a's -HoldFont does, so it outlives the desktop
# task that starts it), writes its pid to <Root>\server.pid, waits 3 s and says whether the port
# answers; -Stop ends that process; -Serve is the listener itself (what -Start runs). GET only:
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
if ($Start) {
  $p = Start-Process -PassThru -WindowStyle Hidden "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $PSCommandPath, '-Serve', '-Root', $Root, '-Port', "$Port")
  [IO.File]::WriteAllText($pidFile, "$($p.Id)", $utf8)
  Start-Sleep -Seconds 3
  $answers = try { $c = New-Object Net.Sockets.TcpClient -ArgumentList '127.0.0.1', $Port; $c.Close(); $true } catch { $false }
  "server pid $($p.Id) on 127.0.0.1:$Port over $Root; answers: $answers"
}
if ($Stop) {
  if (-not (Test-Path -LiteralPath $pidFile)) { 'no server.pid: nothing to stop'; return }
  $id = [int](Get-Content -LiteralPath $pidFile)
  Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $pidFile
  Start-Sleep -Seconds 1
  $answers = try { $c = New-Object Net.Sockets.TcpClient -ArgumentList '127.0.0.1', $Port; $c.Close(); $true } catch { $false }
  "server $id stopped; answers: $answers"
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
