$ErrorActionPreference = 'Stop'
$key = 'HKCU:\Software\Microsoft\Command Processor'
# Never New-Item -Force here: on an existing key it wipes every value and subkey.
if (-not (Test-Path $key)) { New-Item -Path $key | Out-Null }
# Save and restore a value as stored. Get-ItemProperty expands REG_EXPAND_SZ, and writing that result
# back would change both the text and the type (autorun-restore.test.ps1 checks this).
function Save-Value($path, $name) {
  $k = Get-Item $path
  if ($k.GetValueNames() -notcontains $name) { return $null }
  @{ Data = $k.GetValue($name, $null, 'DoNotExpandEnvironmentNames'); Kind = $k.GetValueKind($name) }
}
function Restore-Value($path, $name, $saved) {
  if ($null -eq $saved) { Remove-ItemProperty $path $name -ErrorAction SilentlyContinue; return }
  New-ItemProperty -Path $path -Name $name -Value $saved.Data -PropertyType $saved.Kind -Force | Out-Null
}
$orig = Save-Value $key 'AutoRun'
"original AutoRun: [$($orig.Data)] type: $(if ($orig) { $orig.Kind } else { 'none' })"
$dir = Join-Path $env:LOCALAPPDATA 'สปายค์ ทดสอบ & R&D'
New-Item -ItemType Directory -Force $dir | Out-Null
$marker = Join-Path $dir 'marker.log'
$exe = Join-Path $dir 'probe.cmd'
# %~dp0 expands at run time, so the ASCII-only .cmd never has to hold the Thai path.
Set-Content $exe '@echo probe ran>>"%~dp0marker.log"' -Encoding Ascii
function RunCmd {
  $err = Join-Path $dir 'err.txt'
  $p = Start-Process cmd.exe -ArgumentList '/c exit' -NoNewWindow -Wait -PassThru -RedirectStandardError $err
  "exit=$($p.ExitCode) stderr=[$((Get-Content $err -Raw -ErrorAction SilentlyContinue))]"
}
try {
  $guard = "if exist `"$exe`" `"$exe`""
  Set-ItemProperty $key AutoRun $guard
  "(a) file present: $(RunCmd) marker=[$((Get-Content $marker -Raw -ErrorAction SilentlyContinue))]"
  Remove-Item $marker -ErrorAction SilentlyContinue
  Rename-Item $exe 'probe.cmd.gone'
  "(b) file missing: $(RunCmd)"
  Rename-Item (Join-Path $dir 'probe.cmd.gone') 'probe.cmd'
  Set-ItemProperty $key AutoRun "echo foreign>>`"$marker`" & $guard"
  "(c) chained: $(RunCmd) marker=[$((Get-Content $marker -Raw -ErrorAction SilentlyContinue))]"
  $clinkZip = Join-Path $env:RUNNER_TEMP 'clink.zip'
  Invoke-WebRequest 'https://github.com/chrisant996/clink/releases/download/v1.9.34/clink.1.9.34.4e8ae4.zip' -OutFile $clinkZip
  Expand-Archive $clinkZip (Join-Path $dir 'clink') -Force
  $cx = Join-Path $dir 'clink\clink_x64.exe'
  $prof = Join-Path $dir 'clink-profile'
  Remove-ItemProperty $key AutoRun -ErrorAction SilentlyContinue
  $base = (Measure-Command { 1..20 | ForEach-Object { cmd /c exit } }).TotalMilliseconds / 20
  Set-ItemProperty $key AutoRun "if exist `"$cx`" `"$cx`" inject --autorun --quiet --profile `"$prof`""
  "(e) AutoRun now: [$((Get-ItemProperty $key).AutoRun)]"
  "(e) one run with clink: $(RunCmd)"
  $with = (Measure-Command { 1..20 | ForEach-Object { cmd /c exit } }).TotalMilliseconds / 20
  "(e) cmd /c average ms: without=$([math]::Round($base,1)) with-clink=$([math]::Round($with,1))"
} finally {
  Restore-Value $key 'AutoRun' $orig
  $now = Save-Value $key 'AutoRun'
  "AutoRun restored to: [$($now.Data)] type: $(if ($now) { $now.Kind } else { 'none' })"
}
