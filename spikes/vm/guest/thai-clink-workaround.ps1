# Runs inside the Thai VM as the Thai user, after proto-setup.ps1 -WithOmp. Clink 1.9.34 cannot inject
# from a Thai path on a code page 874 system: clink.log shows its own DLL path garbled, then
# "Unable to get DLL version ... Error 2". Two ways round it, both keeping the profile under the Thai path:
#   -Mode short  run Clink through the 8.3 short path of the same folder
#   -Mode ascii  run a copy of Clink from C:\ProgramData\cmdstudio-spike (no user name in the path)
param([Parameter(Mandatory)] [ValidateSet('short', 'ascii')] [string] $Mode)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script rewrites the user's cmd AutoRun" }
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$data = Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike'
if (-not (Test-Path (Join-Path $data 'autorun-before.json'))) { throw 'run proto-setup.ps1 -WithOmp first: it saves the AutoRun that proto-setup.ps1 -Remove puts back' }
$profileDir = Join-Path $data 'clink-profile'
$clinkDir = Join-Path $data 'runtime\clink-1.9.34'
if ($Mode -eq 'short') {
  $dir = (New-Object -ComObject Scripting.FileSystemObject).GetFolder($clinkDir).ShortPath
} else {
  $dir = 'C:\ProgramData\cmdstudio-spike\clink-1.9.34'
  if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force (Split-Path $dir) | Out-Null; Copy-Item $clinkDir $dir -Recurse }
}
$exe = Join-Path $dir 'clink_x64.exe'
"clink exe: $exe"
Remove-Item (Join-Path $profileDir 'clink.log') -ErrorAction SilentlyContinue
Remove-Item (Join-Path $data 'first-edit.txt') -ErrorAction SilentlyContinue
$key = 'HKCU:\Software\Microsoft\Command Processor'
Set-ItemProperty $key AutoRun "if exist `"$exe`" `"$exe`" inject --autorun --quiet --profile `"$profileDir`""
"AutoRun: $((Get-ItemProperty $key).AutoRun)"
