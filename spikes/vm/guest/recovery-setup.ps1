# Runs inside a test VM as the test user (phase 3b-1 spike, recovery spec §0). Lays out Clink
# 1.9.34 and Oh My Posh 31.5.0 from the repo's resources\ under the user's own LOCALAPPDATA (a
# Thai path with a space on VM-C), as CMD Studio does (spec §5.4), with these scripts on
# clink.path:
#   aa-probe.lua   the Clink API probes (spikes\recovery\probe.lua), unless -NoProbe
#   banner.lua     the Terracotta welcome box as the generator writes it (golden file)
#   init.lua       Oh My Posh loaded as the generator loads it, with the start counter
#                  (spikes\recovery\counter.lua) in front when -Counter is given
#   zz-timing.lua  marks each window's first prompt for recovery-timing.ps1
# and CMD Studio's guarded AutoRun entry. Clink starts through the 8.3 path of the data folder
# when the long one is not ASCII (G19), as the app does. -Remove puts AutoRun back.
param([switch] $Counter, [switch] $NoProbe, [switch] $Remove)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script rewrites the user's cmd AutoRun" }
$key = 'HKCU:\Software\Microsoft\Command Processor'
# Never New-Item -Force on this key: it wipes every value and subkey.
if (-not (Test-Path $key)) { New-Item -Path $key | Out-Null }
$data = Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike3b'
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
$gen = Join-Path $data 'generated'
$profileDir = Join-Path $data 'clink-profile'
$health = Join-Path $data 'health'
$clinkDir = Join-Path $data 'runtime\clink-1.9.34'
$ompDir = Join-Path $data 'runtime\omp-31.5.0'
New-Item -ItemType Directory -Force $gen, $profileDir, $health, $clinkDir, $ompDir | Out-Null
if (-not (Test-Path $before)) {
  $k = Get-Item $key
  $saved = if ($k.GetValueNames() -contains 'AutoRun') { @{ Data = $k.GetValue('AutoRun', $null, 'DoNotExpandEnvironmentNames'); Kind = "$($k.GetValueKind('AutoRun'))" } } else { @{ Absent = $true } }
  $saved | ConvertTo-Json | Set-Content -LiteralPath $before -Encoding UTF8
}
# The real DLL every time: a setup after recovery-crash.ps1 -Swap puts it back.
Copy-Item 'C:\cmdstudio\resources\clink\*' $clinkDir -Force
Copy-Item 'C:\cmdstudio\resources\omp\oh-my-posh.exe' $ompDir -Force
$omp = Join-Path $ompDir 'oh-my-posh.exe'
$config = Join-Path $gen 'theme.omp.json'
Copy-Item 'C:\cmdstudio\golden\terracotta.box.omp.json' $config -Force
Copy-Item 'C:\cmdstudio\golden\terracotta.banner.en.lua' (Join-Path $gen 'banner.lua') -Force
function Lua-Long([string] $s) { "[==[$s]==]" }
$read = { param($name) [IO.File]::ReadAllText("C:\cmdstudio\spikes\recovery\$name") }
$probe = Join-Path $gen 'aa-probe.lua'
Remove-Item -LiteralPath $probe -ErrorAction SilentlyContinue
if (-not $NoProbe) { [IO.File]::WriteAllText($probe, "local p3b_data = $(Lua-Long $data)`n" + (& $read 'probe.lua'), $utf8) }
# The five locals init_lua writes in front of its counter (generate::lua::init_lua).
$head = "local omp = $(Lua-Long $omp)`nlocal config = $(Lua-Long $config)`nlocal health = $(Lua-Long $health)`n" +
  "local STALE_AFTER = 60`nlocal PAUSE_AFTER = 3`n" +
  "local PAUSED_NOW = `"CMD Studio paused its theme because cmd failed to start %d times. Open CMD Studio to turn it back on.`"`n" +
  "local PAUSED_STILL = `"CMD Studio's theme is paused. Open CMD Studio to turn it back on.`"`n"
$counterText = ''
if ($Counter) { $counterText = "local p3b_t0 = os.clock()`n" + (& $read 'counter.lua') + "log.info(string.format('p3b counter ms=%.3f', (os.clock() - p3b_t0) * 1000))`n" }
[IO.File]::WriteAllText((Join-Path $gen 'init.lua'), $head + $counterText + (& $read 'omp-init.lua'), $utf8)
[IO.File]::WriteAllText((Join-Path $gen 'zz-timing.lua'), "local p3b_data = $(Lua-Long $data)`n" + (& $read 'timing.lua'), $utf8)
Remove-Item (Join-Path $data 'probe.txt'), (Join-Path $profileDir 'clink.log') -ErrorAction SilentlyContinue
Get-ChildItem -LiteralPath $data -Filter 'first-prompt-*.txt' | Remove-Item
Get-ChildItem -LiteralPath $health -Force | Remove-Item -Force
$exe = Join-Path $clinkDir 'clink_x64.exe'
$launch = $exe
if ($exe -notmatch '^[\x00-\x7F]*$') {
  $short = (New-Object -ComObject Scripting.FileSystemObject).GetFolder($data).ShortPath
  $launch = Join-Path $short 'runtime\clink-1.9.34\clink_x64.exe'
}
# Clink prints a Lua run-time error with puts, on stdout, and a native program's stderr line would
# end this script under 'Stop': so every line comes back as a string, with 'Continue' inside.
function Clink-Set([string[]] $Words) {
  $ErrorActionPreference = 'Continue'
  & $launch --profile $profileDir set @Words 2>&1 | ForEach-Object { "$_" }
}
# The four settings: their own chatter is hidden, a Lua error (...lua:<n>: ...) never is.
foreach ($setting in @('clink.path', $gen), @('clink.logo', 'none'), @('clink.autoupdate', 'off'), @('prompt.transient', 'off')) {
  Clink-Set $setting | Where-Object { $_ -match '\.lua:\d+:' }
}
# S4, S5: Clink's own program loads the scripts too; everything it printed there, as it printed it.
"clink set said:"
Clink-Set @('clink.logo')
Set-ItemProperty $key AutoRun "(if exist `"$launch`" `"$launch`" inject --autorun --quiet --profile `"$profileDir`")"
"AutoRun: $((Get-ItemProperty $key).AutoRun)"
"data: $data"
"health: $health"
