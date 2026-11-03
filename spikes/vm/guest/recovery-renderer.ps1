# Runs inside a test VM on the signed-in user's desktop (plan 3b-2 Task 1 S2 and Task 12 step 6):
# kills the renderer process of CMD Studio's window, so that WebView2 raises ProcessFailed
# (RENDER_PROCESS_EXITED), and reports whether the window's page came back by itself. -Times
# kills it that many times, -Gap seconds apart (3 within a minute make the app give up and show
# the Yes/No box; answer it by hand or with Send-Key). The renderer is the msedgewebview2.exe
# with --type=renderer whose parent chain reaches cmdstudio.exe; there is one per window.
param([int] $Times = 1, [int] $Gap = 8)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script kills processes" }
function Get-Renderer {
  $app = @(Get-Process cmdstudio -ErrorAction SilentlyContinue)
  if (-not $app) { throw 'cmdstudio.exe is not running' }
  $byId = @{}
  Get-CimInstance Win32_Process | ForEach-Object { $byId[$_.ProcessId] = $_ }
  foreach ($p in Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'") {
    if ($p.CommandLine -notmatch '--type=renderer') { continue }
    $q = $p
    for ($i = 0; $i -lt 6 -and $q; $i++) {
      if ($q.Name -eq 'cmdstudio.exe') { return $p }
      $q = $byId[$q.ParentProcessId]
    }
  }
  throw 'no renderer of cmdstudio.exe found'
}
for ($n = 1; $n -le $Times; $n++) {
  $r = Get-Renderer
  "kill $n`: renderer pid $($r.ProcessId)"
  Stop-Process -Id $r.ProcessId -Force
  Start-Sleep -Seconds 5
  try { $again = Get-Renderer; "after $n`: renderer pid $($again.ProcessId) (page came back: $($again.ProcessId -ne $r.ProcessId))" }
  catch { "after $n`: no renderer ($($_.Exception.Message))" }
  if ($n -lt $Times) { Start-Sleep -Seconds $Gap }
}
