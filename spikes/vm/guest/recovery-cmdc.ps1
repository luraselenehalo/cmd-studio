# Runs inside a test VM (phase 3b-1 spike S7): three `cmd /c` runs with AutoRun on, as io.popen
# and other programs start cmd. Prints their process ids: recovery-report.ps1 must show no probe
# line from any of them (Clink's AutoRun mode does not inject into cmd /c).
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script starts cmd with the user's AutoRun" }
foreach ($i in 1..3) {
  $p = Start-Process -FilePath "$env:SystemRoot\System32\cmd.exe" -ArgumentList '/c', 'echo', "p3b-cmd-c-$i" -PassThru -Wait -WindowStyle Hidden
  "cmd /c pid=$($p.Id) exit=$($p.ExitCode)"
}
