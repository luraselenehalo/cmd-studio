# Runs inside a test VM. Starts $Script on the signed-in user's desktop and prints what it wrote.
# guestcontrol starts processes outside that desktop session, where fonts loaded at sign-in
# and console windows are not what the user sees. An interactive scheduled task runs on the desktop.
param([Parameter(Mandatory)] [string] $Script, [string] $ScriptArgs = '', [int] $TimeoutSec = 900)
$ErrorActionPreference = 'Stop'
# Our stdout goes back to the host through guestcontrol; in the default code page Thai becomes '?'.
[Console]::OutputEncoding = [Text.Encoding]::UTF8
# guestcontrol may not give this process the user's environment (its --profile is "not yet
# implemented"), so take the user from the token and write output to a fixed folder.
$me = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$outDir = 'C:\cmdstudio\out'
New-Item -ItemType Directory -Force $outDir | Out-Null
$id = [guid]::NewGuid().ToString('N').Substring(0, 8)
$out = Join-Path $outDir "desktop-$id.txt"
$done = "$out.done"
$cmd = "& '$Script' $ScriptArgs *>&1 | Out-File -Encoding utf8 -LiteralPath '$out'; Set-Content -LiteralPath '$done' -Value `$LASTEXITCODE"
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -ExecutionPolicy Bypass -Command `"$cmd`""
$principal = New-ScheduledTaskPrincipal -UserId $me -LogonType Interactive
$name = "cmdstudio-desktop-$id"
Register-ScheduledTask -TaskName $name -Action $action -Principal $principal | Out-Null
try {
  Start-ScheduledTask -TaskName $name
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while (-not (Test-Path -LiteralPath $done) -and (Get-Date) -lt $deadline) { Start-Sleep -Seconds 1 }
  if (Test-Path -LiteralPath $out) { Get-Content -LiteralPath $out -Encoding UTF8 }
  if (-not (Test-Path -LiteralPath $done)) { "run-on-desktop: no finish marker after $TimeoutSec s (task state $((Get-ScheduledTask -TaskName $name).State))" }
} finally {
  Unregister-ScheduledTask -TaskName $name -Confirm:$false
}
