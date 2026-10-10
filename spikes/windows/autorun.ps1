$ErrorActionPreference = 'Stop'
$key = 'HKCU:\Software\Microsoft\Command Processor'
New-Item -Path $key -Force | Out-Null
$orig = (Get-ItemProperty $key -ErrorAction SilentlyContinue).AutoRun
"original AutoRun: [$orig]"
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
  if ($null -eq $orig) { Remove-ItemProperty $key AutoRun -ErrorAction SilentlyContinue } else { Set-ItemProperty $key AutoRun $orig }
  "AutoRun restored to: [$((Get-ItemProperty $key -ErrorAction SilentlyContinue).AutoRun)]"
}
