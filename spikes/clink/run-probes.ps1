# 'Continue', not 'Stop': in PowerShell 5.1, native stderr redirected with 2>&1
# becomes an error record, and 'Stop' would abort before G5 records the Lua error.
$ErrorActionPreference = 'Continue'
$clink ="$env:USERPROFILE\cmd-claude-theme\clink-bin\clink_x64.exe"
$root = Join-Path $PSScriptRoot 'tmp'
Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
$profileDir = Join-Path $root 'profile'
$scripts = Join-Path $root 'scripts'
$spaced = Join-Path $root 'โฟลเดอร์ ทดสอบ'
New-Item -ItemType Directory -Force $profileDir, $scripts, $spaced | Out-Null
Set-Content -Path (Join-Path $spaced 'echo args.cmd') -Value '@echo %1^|%2' -Encoding Ascii
Set-Content -Path (Join-Path $spaced 'check.cmd') -Value '@if exist "%~dp0%~1.txt" (echo FOUND) else (echo MISSING)' -Encoding Ascii
Set-Content -Path (Join-Path $spaced 'สาม.txt') -Value 'x' -Encoding Ascii
Copy-Item "$env:SystemRoot\System32\whoami.exe" (Join-Path $spaced 'who ami.exe')

"== G16: clink.autoupdate"
& $clink --profile $profileDir set clink.autoupdate 2>&1
& $clink --profile $profileDir set clink.autoupdate off 2>&1
"exit=$LASTEXITCODE"

"== G5: does a Lua error show up?"
Copy-Item (Join-Path $PSScriptRoot 'bad.lua') $scripts
& $clink --profile $profileDir set clink.path $scripts 2>&1
$out = & $clink --profile $profileDir set clink.logo 2>&1
"exit=$LASTEXITCODE"
"output:"; $out
"clink.log tail:"; Get-Content (Join-Path $profileDir 'clink.log') -Tail 15 -ErrorAction SilentlyContinue
Remove-Item (Join-Path $scripts 'bad.lua')

"== G4 + G17: hidden injected cmd"
Copy-Item (Join-Path $PSScriptRoot 'probe.lua') $scripts
$env:CMDSTUDIO_PROBE_DIR = $spaced
$p = Start-Process cmd.exe -ArgumentList "/d /k `"`"$clink`" inject --profile `"$profileDir`"`"" -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 5
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
"clink.log probe lines:"
Select-String -Path (Join-Path $profileDir 'clink.log') -Pattern 'probe ' | ForEach-Object { $_.Line }
