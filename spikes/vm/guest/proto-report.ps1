# Runs inside a test VM after cmd was opened (Send-Run) on the proto-setup layout. Prints what Clink
# logged for G4 and G17, the time from cmd's start to Clink's first input line, and Oh My Posh's
# prompt cost on its own.
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$data = Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike'
$log = Join-Path $data 'clink-profile\clink.log'
# Log lines start with a thread id and the script path, which can hold spaces; keep from the tag on.
Get-Content $log -Encoding UTF8 | Select-String '\s(g4|g17|timing) ' | ForEach-Object { $_.Line -replace '^.*?\s(?=(g4|g17|timing) )', '' }

# The cmd that a person opened has Explorer as its parent; Oh My Posh's own cmd /c calls do not.
$explorer = (Get-Process explorer | Select-Object -First 1).Id
$cmd = Get-CimInstance Win32_Process -Filter "Name='cmd.exe'" | Where-Object ParentProcessId -eq $explorer |
  Sort-Object CreationDate | Select-Object -Last 1
$marker = Get-Item (Join-Path $data 'first-edit.txt') -ErrorAction SilentlyContinue
if ($cmd -and $marker) { "cmd start to first input line: $([int]($marker.LastWriteTime - $cmd.CreationDate).TotalMilliseconds) ms" }
else { "no timing: cmd from Explorer=$([bool]$cmd) first-edit marker=$([bool]$marker)" }

$omp = Join-Path $data 'runtime\omp-31.5.0\oh-my-posh.exe'
$config = Join-Path $data 'themes\claude.omp.json'
if (Test-Path $omp) {
  & $omp print primary --config $config --shell cmd | Out-Null
  $ms = 1..5 | ForEach-Object { (Measure-Command { & $omp print primary --config $config --shell cmd | Out-Null }).TotalMilliseconds }
  "oh-my-posh print primary: $([int](($ms | Measure-Object -Average).Average)) ms average of 5 (after one warm-up)"
}
