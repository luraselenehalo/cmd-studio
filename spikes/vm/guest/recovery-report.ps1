# Runs inside a test VM after the cmd windows of a step were opened on the recovery-setup layout.
# Prints every probe line, from clink.log and from <data>\probe.txt (one of the two may be
# missing: probe.txt needs io.open on the data folder's path, which is spike S2), then what is
# left in health\. Changes nothing.
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$data = Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike3b'
$log = Join-Path $data 'clink-profile\clink.log'
'--- clink.log'
if (Test-Path -LiteralPath $log) {
  Get-Content -LiteralPath $log -Encoding UTF8 | Select-String '\sp3b ' | ForEach-Object { $_.Line -replace '^.*?\s(?=p3b )', '' }
}
'--- probe.txt'
$probe = Join-Path $data 'probe.txt'
if (Test-Path -LiteralPath $probe) { Get-Content -LiteralPath $probe -Encoding UTF8 } else { '(none)' }
'--- health\'
Get-ChildItem -LiteralPath (Join-Path $data 'health') -Force | ForEach-Object { "$($_.Name): $([IO.File]::ReadAllText($_.FullName).Trim())" }
'--- cmd.exe processes now'
Get-CimInstance Win32_Process -Filter "Name='cmd.exe'" | ForEach-Object { "pid=$($_.ProcessId) parent=$($_.ParentProcessId) $($_.CommandLine)" }
