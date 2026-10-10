# Runs inside a test VM as the test user (G4, G17, start time). Lays out what CMD Studio will
# install, under the user's own LOCALAPPDATA (a Thai path with a space on the Thai VM), per spec §5.4:
#   runtime\clink-1.9.34, runtime\omp-31.5.0\oh-my-posh.exe, clink-profile, scripts with an init.lua
#   that calls Oh My Posh by its full quoted path, and the guarded AutoRun value.
# Scripts load in name order: 00-g4-header.lua logs the screen before claude-banner.lua clears it.
param([switch] $WithOmp, [switch] $Remove)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script rewrites the user's cmd AutoRun" }
$key = 'HKCU:\Software\Microsoft\Command Processor'
# Never New-Item -Force on this key: it wipes every value and subkey.
if (-not (Test-Path $key)) { New-Item -Path $key | Out-Null }
$data = Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike'
# The user's AutoRun as it was before the first setup: text unexpanded, and its type.
$before = Join-Path $data 'autorun-before.json'
if ($Remove) {
  if (-not (Test-Path $before)) { 'no saved AutoRun, nothing restored'; return }
  $saved = Get-Content -LiteralPath $before -Raw | ConvertFrom-Json
  if ($saved.Absent) { Remove-ItemProperty $key AutoRun -ErrorAction SilentlyContinue }
  else { New-ItemProperty -Path $key -Name AutoRun -Value $saved.Data -PropertyType $saved.Kind -Force | Out-Null }
  Remove-Item -LiteralPath $before
  "AutoRun restored ($(if ($saved.Absent) { 'there was none' } else { $saved.Kind }))"
  return
}

$utf8 = New-Object Text.UTF8Encoding $false
$clinkDir = Join-Path $data 'runtime\clink-1.9.34'
$profileDir = Join-Path $data 'clink-profile'
$scripts = Join-Path $data 'scripts'
New-Item -ItemType Directory -Force $profileDir, $scripts | Out-Null
if (-not (Test-Path $before)) {
  $k = Get-Item $key
  $saved = if ($k.GetValueNames() -contains 'AutoRun') { @{ Data = $k.GetValue('AutoRun', $null, 'DoNotExpandEnvironmentNames'); Kind = "$($k.GetValueKind('AutoRun'))" } } else { @{ Absent = $true } }
  $saved | ConvertTo-Json | Set-Content -LiteralPath $before -Encoding UTF8
}
if (-not (Test-Path $clinkDir)) { Copy-Item 'C:\cmdstudio\clink-bin' $clinkDir -Recurse }
Copy-Item (Join-Path $PSScriptRoot 'g4-header.lua') (Join-Path $scripts '00-g4-header.lua') -Force
Copy-Item 'C:\cmdstudio\proto\clink\claude-banner.lua' $scripts -Force

if ($WithOmp) {
  $ompDir = Join-Path $data 'runtime\omp-31.5.0'
  $omp = Join-Path $ompDir 'oh-my-posh.exe'
  New-Item -ItemType Directory -Force $ompDir, (Join-Path $data 'themes') | Out-Null
  if (-not (Test-Path $omp)) {
    Invoke-WebRequest 'https://github.com/JanDeDobbeleer/oh-my-posh/releases/download/v31.5.0/posh-windows-amd64.exe' -OutFile $omp -UseBasicParsing
  }
  $h = (Get-FileHash $omp -Algorithm SHA256).Hash
  if ($h -ne '9DD2C1297D5AE89F23F19B02F74A25510FCB56D84DA5139FA3C3CA6E6D008A73') { throw "oh-my-posh.exe sha256 $h is not the v31.5.0 release value" }
  $config = Join-Path $data 'themes\claude.omp.json'
  Copy-Item 'C:\cmdstudio\proto\claude.omp.json' $config -Force
  # What the generator will write: absolute paths baked in, quoted for cmd /c (G17).
  $init = @"
-- Mirrors spec §5.4 generated\init.lua: Oh My Posh by its full path, no PATH, quoted for cmd /c (G17).
local exe = [==[$omp]==]
local config = [==[$config]==]
local h = io.popen('""' .. exe .. '" init cmd --config "' .. config .. '""')
local init = h and h:read("*a") or ""
if h then h:close() end
log.info("g17 init bytes: " .. #init)
if init == "" then return end
local chunk, err = load(init)
if not chunk then log.info("g17 load error: " .. tostring(err)) return end
chunk()
log.info("g17 prompt loaded")
"@
  [IO.File]::WriteAllText((Join-Path $scripts 'init.lua'), $init, $utf8)
}

# Marks when Clink starts the first input line (onbeginedit runs before the prompt filters; a prompt
# filter can't do this, because Oh My Posh's filter ends the chain). The host compares the marker's
# time with cmd's start time; Oh My Posh's own prompt cost is measured apart with 'print primary'.
$marker = Join-Path $data 'first-edit.txt'
Remove-Item $marker -ErrorAction SilentlyContinue
$timing = @"
local marked = false
clink.onbeginedit(function()
  if marked then return end
  marked = true
  local h = io.open([==[$marker]==], "w")
  log.info("timing marker written: " .. tostring(h ~= nil))
  if h then h:write("ok") h:close() end
end)
"@
[IO.File]::WriteAllText((Join-Path $scripts 'zz-timing.lua'), $timing, $utf8)

$exe = Join-Path $clinkDir 'clink_x64.exe'
& $exe --profile $profileDir set clink.path $scripts | Out-Null
& $exe --profile $profileDir set clink.logo none | Out-Null
& $exe --profile $profileDir set clink.autoupdate off | Out-Null
& $exe --profile $profileDir set prompt.transient always | Out-Null
Remove-Item (Join-Path $profileDir 'clink.log') -ErrorAction SilentlyContinue
Set-ItemProperty $key AutoRun "if exist `"$exe`" `"$exe`" inject --autorun --quiet --profile `"$profileDir`""
"AutoRun: $((Get-ItemProperty $key).AutoRun)"
"data: $data"
