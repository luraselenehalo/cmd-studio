# Runs inside a test VM (plan 4b Tasks 1 and 9): what an update of CMD Studio must leave, read and
# printed. -Snapshot <label> writes under -Out and prints: the AutoRun value (type and bytes), the
# installed version (DisplayVersion of the uninstall key, which the installer writes from its own
# version string, and cmdstudio.exe's ProductVersion and FileVersion), the install folder's
# entries, the Start-menu folder's shortcuts, the user Path's entries that name CMD Studio, the
# data folder's update.json (its text or ABSENT) and updates\ (its entries or ABSENT), the `update `
# lines of logs\cmdstudio.log, and the cmdstudio processes. -SetMode auto|notify|off edits
# settings.json's "updates" field (the app must be closed; a missing file is made with that field
# alone, the rest being the app's defaults). -WaitForVersion <v> polls DisplayVersion up to
# -WaitSeconds and prints the seconds it took, or TIMEOUT. -Log prints the log's `update ` lines.
# -StartApp opens the Start-menu shortcut "CMD Studio.lnk" as a click does; with -Recovery the
# "CMD Studio Recovery.lnk" one. Run on the user's desktop (Invoke-GuestDesktop): HKCU and the
# environment are the signed-in user's only there.
param(
  [ValidatePattern('^[a-z0-9]*$')] [string] $Snapshot = '',
  [ValidateSet('', 'auto', 'notify', 'off')] [string] $SetMode = '',
  [string] $WaitForVersion = '', [int] $WaitSeconds = 180,
  [switch] $Log, [switch] $StartApp, [switch] $Recovery,
  [string] $Out = 'C:\cmdstudio\out\phase4b'
)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script reads the user's registry and starts programs" }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$local = $env:LOCALAPPDATA
$data = Join-Path $local 'ai.resincore.cmdstudio'
$inst = Join-Path $local 'CMD Studio'
$sm = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\CMD Studio'
$uninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\CMD Studio'

function RegRaw([string] $key, [string] $name) {
  $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($key)
  if ($null -eq $k) { return 'KEY ABSENT' }
  try {
    if ($k.GetValueNames() -notcontains $name) { return 'ABSENT' }
    $kind = $k.GetValueKind($name)
    $v = $k.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $bytes = if ($v -is [byte[]]) { $v } elseif ($v -is [string]) { [Text.Encoding]::Unicode.GetBytes($v) } else { [Text.Encoding]::Unicode.GetBytes("$v") }
    "$kind " + [BitConverter]::ToString($bytes)
  } finally { $k.Close() }
}
function RegText([string] $key, [string] $name) {
  $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($key)
  if ($null -eq $k) { return 'KEY ABSENT' }
  try { $v = $k.GetValue($name, $null); if ($null -eq $v) { 'ABSENT' } else { "$v" } } finally { $k.Close() }
}
function Installed {
  $exe = Join-Path $inst 'cmdstudio.exe'
  $display = RegText $uninstallKey 'DisplayVersion'
  if (-not (Test-Path -LiteralPath $exe)) { return "DisplayVersion=$display exe=ABSENT" }
  $vi = (Get-Item -LiteralPath $exe).VersionInfo
  "DisplayVersion=$display ProductVersion=$($vi.ProductVersion) FileVersion=$($vi.FileVersion)"
}
function Entries([string] $dir) {
  if (-not (Test-Path -LiteralPath $dir)) { return 'ABSENT' }
  (Get-ChildItem -LiteralPath $dir -Force | Sort-Object Name | ForEach-Object { if ($_.PSIsContainer) { "$($_.Name)\" } else { "$($_.Name) $($_.Length)" } }) -join "`n"
}
function Shortcuts {
  if (-not (Test-Path -LiteralPath $sm)) { return 'ABSENT' }
  $sh = New-Object -ComObject WScript.Shell
  (Get-ChildItem -LiteralPath $sm -Filter *.lnk | Sort-Object Name | ForEach-Object { $l = $sh.CreateShortcut($_.FullName); "$($_.Name) -> $($l.TargetPath) [$($l.Arguments)]" }) -join "`n"
}
function PathEntries {
  $v = RegText 'Environment' 'Path'
  if ($v -eq 'ABSENT' -or $v -eq 'KEY ABSENT') { return $v }
  $ours = @($v -split ';' | Where-Object { $_ -like '*\CMD Studio' -or $_ -like '*\CMD Studio\' })
  "$($ours.Count) entry(ies): " + ($ours -join ' | ')
}
function UpdateLines {
  $log = Join-Path $data 'logs\cmdstudio.log'
  if (-not (Test-Path -LiteralPath $log)) { return 'LOG ABSENT' }
  $lines = @(Get-Content -LiteralPath $log | Where-Object { $_ -match ' update ' })
  if ($lines.Count -eq 0) { 'no update lines' } else { $lines -join "`n" }
}
function TextOrAbsent([string] $file) {
  if (Test-Path -LiteralPath $file) { [IO.File]::ReadAllText($file) } else { 'ABSENT' }
}
function State {
  [ordered]@{
    autorun    = RegRaw 'Software\Microsoft\Command Processor' 'AutoRun'
    installed  = Installed
    install    = Entries $inst
    shortcuts  = Shortcuts
    path       = PathEntries
    updateJson = TextOrAbsent (Join-Path $data 'update.json')
    updates    = Entries (Join-Path $data 'updates')
    runJson    = TextOrAbsent (Join-Path $data 'run.json')
    log        = UpdateLines
    processes  = (@(Get-Process cmdstudio -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Id)" }) -join ',')
  }
}
if ($Snapshot) {
  $s = State
  foreach ($k in $s.Keys) { [IO.File]::WriteAllText((Join-Path $Out "$Snapshot-$k.txt"), "$($s[$k])", (New-Object Text.UTF8Encoding $false)) }
  "snapshot $Snapshot written"
  foreach ($k in $s.Keys) { "${k}: $($s[$k] -replace "`n", ' | ')" }
}
if ($SetMode) {
  $file = Join-Path $data 'settings.json'
  if (@(Get-Process cmdstudio -ErrorAction SilentlyContinue).Count -gt 0) { throw 'close the app before -SetMode: it holds its settings in memory' }
  if (Test-Path -LiteralPath $file) {
    $json = [IO.File]::ReadAllText($file) | ConvertFrom-Json
    # Add-Member -Force sets the field whether the file has it or not (an assignment to a field
    # a PSCustomObject lacks throws).
    $json | Add-Member -NotePropertyName updates -NotePropertyValue $SetMode -Force
  } else {
    New-Item -ItemType Directory -Force -Path $data | Out-Null
    $json = [pscustomobject]@{ version = 1; updates = $SetMode }
  }
  [IO.File]::WriteAllText($file, ($json | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))
  "settings.json updates=$SetMode"
}
if ($WaitForVersion) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.Elapsed.TotalSeconds -lt $WaitSeconds) {
    if ((RegText $uninstallKey 'DisplayVersion') -eq $WaitForVersion) { "version $WaitForVersion after $([int]$sw.Elapsed.TotalSeconds) s; $(Installed)"; return }
    Start-Sleep -Seconds 2
  }
  "TIMEOUT after $WaitSeconds s; $(Installed)"
}
if ($Log) { UpdateLines }
if ($StartApp) {
  $lnk = Join-Path $sm $(if ($Recovery) { 'CMD Studio Recovery.lnk' } else { 'CMD Studio.lnk' })
  if (-not (Test-Path -LiteralPath $lnk)) { throw "no shortcut at $lnk" }
  Start-Process -FilePath $lnk | Out-Null
  Start-Sleep -Seconds 10
  "cmdstudio processes 10 s after the shortcut: $(@(Get-Process cmdstudio -ErrorAction SilentlyContinue).Count)"
}
