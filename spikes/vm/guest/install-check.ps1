# Runs inside a test VM (plan 4a Task 9): the state a CMD Studio install and uninstall must leave
# byte-equal, written to files before and compared after. -Snapshot <label> (any lower-case label;
# -Compare reads "before" and "after") records, under -Out, the user's AutoRun value (type and
# bytes), the user's Path value (type and bytes), the user's font registrations (HKCU\...\Fonts,
# every value), the user fonts folder's files with their hashes, Windows Terminal's settings.json
# (its hash, or ABSENT), the Start-menu folder "CMD Studio" (the .lnk files, each one's target and
# arguments), the desktop shortcut "CMD Studio.lnk" (Tauri's finish page and silent install make
# it), the uninstall registry key of CMD Studio, Tauri's own key HKCU\Software\<publisher>\CMD Studio,
# the install folder's file list, the data folder's top-level entries and its themes\ files.
# -Compare says SAME or DIFFERENT per item against the two snapshots, requires the install folder,
# both kinds of shortcut and the uninstall key to be gone, and, with -ThemesKept, the data folder
# to hold themes\ and nothing of CMD Studio's own files (state.json, snapshots\ ...), with
# -ThemesGone the data folder and Tauri's own key to be gone (both go only with the box ticked).
# -Cli runs `cmdstudio doctor` in cmd (through a batch file, so that no quote has to pass through
# PowerShell) and in PowerShell and prints, for each, the lines in the order they came and the
# exit code. -HoldFont opens the installed font file and keeps it open for -HoldSeconds in a
# process this script starts, which ends by itself; -ReleaseFont ends that process at once.
# -CleanupAll runs the installed cmdstudio.com --cleanup --remove-themes, as the uninstaller's
# failure box tells a person to do after a new install. -Install / -InstallSilent -Setup <file or
# folder> start the installer on this desktop (silent: /S, waited for); -Uninstall /
# -UninstallSilent the same for the installed uninstall.exe. Run on the user's desktop
# (Invoke-GuestDesktop): HKCU and the environment are the signed-in user's only there.
param(
  [ValidatePattern('^[a-z0-9]*$')] [string] $Snapshot = '',
  [switch] $Compare, [switch] $ThemesKept, [switch] $ThemesGone, [switch] $Cli,
  [switch] $HoldFont, [int] $HoldSeconds = 600, [switch] $ReleaseFont, [switch] $CleanupAll,
  [switch] $Install, [switch] $InstallSilent, [string] $Setup = '',
  [switch] $Uninstall, [switch] $UninstallSilent,
  [string] $Out = 'C:\cmdstudio\out\phase4'
)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script reads the user's registry and starts programs" }
# What the programs started here print comes back as UTF-8, or a Thai folder name becomes '?'.
[Console]::OutputEncoding = [Text.Encoding]::UTF8
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$local = $env:LOCALAPPDATA
$data = Join-Path $local 'ai.resincore.cmdstudio'
$inst = Join-Path $local 'CMD Studio'
$com = Join-Path $inst 'cmdstudio.com'
$sm = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\CMD Studio'
$desk = Join-Path ([Environment]::GetFolderPath('Desktop')) 'CMD Studio.lnk'
$wt = Join-Path $local 'Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json'
$fontFile = Join-Path $local 'Microsoft\Windows\Fonts\CaskaydiaCoveNerdFont-Regular.ttf'
$holdPid = Join-Path $Out 'holdfont.pid'
$utf8 = New-Object Text.UTF8Encoding $false

function RegRaw([string] $key, [string] $name) {
  # Type and bytes, so that REG_SZ and REG_EXPAND_SZ, and every byte, are compared.
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
# The same value as text, for the lines a person reads (the bytes above are what is compared).
function RegText([string] $key, [string] $name) {
  $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($key)
  if ($null -eq $k) { return 'KEY ABSENT' }
  try {
    if ($k.GetValueNames() -notcontains $name) { return 'ABSENT' }
    "$($k.GetValueKind($name)) " + $k.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
  } finally { $k.Close() }
}
function FontValues {
  $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Microsoft\Windows NT\CurrentVersion\Fonts')
  if ($null -eq $k) { return 'KEY ABSENT' }
  try { ($k.GetValueNames() | Sort-Object | ForEach-Object { "$_ = $($k.GetValue($_))" }) -join "`n" } finally { $k.Close() }
}
function Files([string] $dir) {
  if (-not (Test-Path -LiteralPath $dir)) { return 'ABSENT' }
  (Get-ChildItem -LiteralPath $dir -Recurse -File -Force | Sort-Object FullName | ForEach-Object { "$($_.FullName.Substring($dir.Length)) $((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash)" }) -join "`n"
}
function Link($sh, [string] $file) { $l = $sh.CreateShortcut($file); "$(Split-Path -Leaf $file) -> $($l.TargetPath) [$($l.Arguments)]" }
function Shortcuts {
  if (-not (Test-Path -LiteralPath $sm)) { return 'ABSENT' }
  $sh = New-Object -ComObject WScript.Shell
  (Get-ChildItem -LiteralPath $sm -Filter *.lnk | Sort-Object Name | ForEach-Object { Link $sh $_.FullName }) -join "`n"
}
function Desktop {
  if (-not (Test-Path -LiteralPath $desk)) { return 'ABSENT' }
  Link (New-Object -ComObject WScript.Shell) $desk
}
function AppKey {
  # Tauri's installer keeps the install folder under HKCU\Software\<publisher>\CMD Studio and
  # deletes it only when "Delete the application data" is ticked. Every such key is listed.
  $found = @(Get-ChildItem -LiteralPath 'HKCU:\Software' -ErrorAction SilentlyContinue | Where-Object { Test-Path -LiteralPath (Join-Path $_.PSPath 'CMD Studio') } | ForEach-Object { "$($_.PSChildName)\CMD Studio" })
  if ($found.Count -eq 0) { 'ABSENT' } else { $found -join "`n" }
}
function State {
  [ordered]@{
    autorun   = RegRaw 'Software\Microsoft\Command Processor' 'AutoRun'
    path      = RegRaw 'Environment' 'Path'
    fontsReg  = FontValues
    fontsDir  = Files (Join-Path $local 'Microsoft\Windows\Fonts')
    wt        = if (Test-Path -LiteralPath $wt) { (Get-FileHash -LiteralPath $wt -Algorithm SHA256).Hash } else { 'ABSENT' }
    shortcuts = Shortcuts
    desktop   = Desktop
    uninstall = if (Test-Path -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CMD Studio') { 'PRESENT' } else { 'ABSENT' }
    appKey    = AppKey
    install   = Files $inst
    dataTop   = if (Test-Path -LiteralPath $data) { (Get-ChildItem -LiteralPath $data -Force | Sort-Object Name | ForEach-Object Name) -join "`n" } else { 'ABSENT' }
    themes    = Files (Join-Path $data 'themes')
  }
}
function Line([string] $text) { $text -replace "`n", ', ' }

if ($Snapshot) {
  $s = State
  foreach ($k in $s.Keys) { [IO.File]::WriteAllText((Join-Path $Out "$Snapshot-$k.txt"), "$($s[$k])", $utf8) }
  "snapshot $Snapshot written: $($s.Keys -join ', ')"
  "autorun: $(RegText 'Software\Microsoft\Command Processor' 'AutoRun')"
  "path: $(RegText 'Environment' 'Path')"
  "shortcuts:`n$($s.shortcuts)"
  "desktop shortcut: $($s.desktop)"
  "uninstall key: $($s.uninstall)"
  "app key: $(Line $s.appKey)"
  "install folder: $(if ($s.install -eq 'ABSENT') { 'ABSENT' } else { Line ((Get-ChildItem -LiteralPath $inst -Force | Sort-Object Name | ForEach-Object Name) -join "`n") })"
  "data folder: $(Line $s.dataTop)"
  "themes: $(Line $s.themes)"
}
if ($Compare) {
  $fail = 0
  $read = { param($label, $k) $f = Join-Path $Out "$label-$k.txt"; if (Test-Path -LiteralPath $f) { [IO.File]::ReadAllText($f) } else { throw "no snapshot file ${f}: take the '$label' snapshot first" } }
  foreach ($k in 'autorun', 'path', 'fontsReg', 'fontsDir', 'wt') {
    $b = & $read 'before' $k; $a = & $read 'after' $k
    if ($b -eq $a) { "SAME $k" } else { "DIFFERENT $k`n  before: $b`n  after:  $a"; $fail++ }
  }
  $after = State
  foreach ($k in 'shortcuts', 'desktop', 'uninstall', 'install') { if ($after[$k] -eq 'ABSENT') { "GONE $k" } else { "STILL THERE $k`n  $($after[$k])"; $fail++ } }
  $appBefore = & $read 'before' 'appKey'
  if ($ThemesKept) {
    $top = $after.dataTop -split "`n"
    if ($top -contains 'themes' -and -not ($top -contains 'state.json') -and -not ($top -contains 'snapshots')) { "THEMES KEPT, the rest gone (data folder: $(Line $after.dataTop); themes: $(Line $after.themes))" } else { "DATA FOLDER WRONG: $(Line $after.dataTop)"; $fail++ }
    # Not counted: Tauri keeps its own key unless the box is ticked (its template, not CMD Studio).
    "INFO appKey before: $(Line $appBefore); after: $(Line $after.appKey)"
  }
  if ($ThemesGone) {
    if ($after.dataTop -eq 'ABSENT') { 'DATA FOLDER GONE' } else { "DATA FOLDER STILL THERE: $(Line $after.dataTop)"; $fail++ }
    if ($after.appKey -eq $appBefore) { 'SAME appKey' } else { "DIFFERENT appKey`n  before: $(Line $appBefore)`n  after:  $(Line $after.appKey)"; $fail++ }
  }
  if ($fail) { "VERDICT: FAIL ($fail)" } else { 'VERDICT: PASS' }
}
if ($Cli) {
  # cmd: %errorlevel% must be doctor's code, and `where` shows which file cmd takes for a bare
  # `cmdstudio` (PATHEXT puts .COM before .EXE). The lines are a batch file, so no quote passes
  # through PowerShell's native-argument quoting, and %errorlevel% expands per line. A batch file
  # waits for every program, so the order here proves the exit code and the file, not the wait of
  # a typed command: protocol step 4 types it into a cmd window for that. cmd runs without /d
  # here on purpose: AutoRun is under test in these VMs.
  $cmdOut = Join-Path $Out 'cli-cmd.txt'
  $bat = Join-Path $Out 'cli.cmd'
  @('@echo off', "where cmdstudio > `"$cmdOut`" 2>&1", "cmdstudio doctor >> `"$cmdOut`" 2>&1", "echo EXIT=%errorlevel%>> `"$cmdOut`"", "echo PROMPT-RETURNED>> `"$cmdOut`"") | Set-Content -LiteralPath $bat -Encoding ASCII
  & "$env:SystemRoot\System32\cmd.exe" /c $bat
  '--- cmd:'; Get-Content -LiteralPath $cmdOut -Encoding UTF8
  '--- powershell:'
  # Encoded, so that nothing of the command is re-quoted on its way to the child.
  $ps = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; cmdstudio doctor; 'EXIT=' + `$LASTEXITCODE; (Get-Command cmdstudio).Source"
  & "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -EncodedCommand ([Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($ps)))
}
if ($Install -or $InstallSilent) {
  # A folder is taken to hold one installer (its name has spaces, which this way never pass
  # through the host's command line).
  if (Test-Path -LiteralPath $Setup -PathType Container) { $Setup = @(Get-ChildItem -LiteralPath $Setup -Filter '*-setup.exe' | ForEach-Object FullName) | Select-Object -First 1 }
  if (-not $Setup -or -not (Test-Path -LiteralPath $Setup -PathType Leaf)) { throw "no installer at $Setup" }
  "installer: $Setup"
  if ($InstallSilent) {
    # A marked file (protocol step 2) would stop a silent run at Windows' "Open File" warning.
    if (Get-Item -LiteralPath $Setup -Stream Zone.Identifier -ErrorAction SilentlyContinue) { Unblock-File -LiteralPath $Setup; 'mark of the web removed for the silent run' }
    $p = Start-Process -Wait -PassThru -FilePath $Setup -ArgumentList '/S'
    "silent install ended, exit code $($p.ExitCode)"
  } else { Start-Process -FilePath $Setup | Out-Null; 'installer started on the desktop' }
}
if ($Uninstall -or $UninstallSilent) {
  $un = Join-Path $inst 'uninstall.exe'
  if (-not (Test-Path -LiteralPath $un)) { throw "no uninstaller at $un" }
  if ($UninstallSilent) {
    # Start-Process -Wait waits for the uninstaller's copy in %TEMP% too (it waits for the
    # descendants); the uninstall key going is the second sign that it is over.
    $p = Start-Process -Wait -PassThru -FilePath $un -ArgumentList '/S'
    $deadline = (Get-Date).AddSeconds(120)
    while ((Test-Path -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CMD Studio') -and (Get-Date) -lt $deadline) { Start-Sleep -Seconds 2 }
    "silent uninstall ended, exit code $($p.ExitCode), uninstall key gone: $(-not (Test-Path -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CMD Studio'))"
  } else { Start-Process -FilePath $un | Out-Null; 'uninstaller started on the desktop' }
}
if ($CleanupAll) {
  if (-not (Test-Path -LiteralPath $com)) { throw "no console program at $com" }
  # Windows PowerShell 5.1 turns a native program's stderr into error records; 'Stop' would end here.
  $ErrorActionPreference = 'Continue'
  & $com --cleanup --remove-themes 2>&1 | ForEach-Object { "$_" }
  "EXIT=$LASTEXITCODE"
  $ErrorActionPreference = 'Stop'
}
if ($HoldFont) {
  if (-not (Test-Path -LiteralPath $fontFile)) { throw "no installed font at $fontFile" }
  $code = "`$f = [IO.File]::Open('$($fontFile.Replace("'", "''"))', 'Open', 'Read', 'None'); Start-Sleep -Seconds $HoldSeconds; `$f.Close()"
  $holder = Start-Process -PassThru -WindowStyle Hidden "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -ArgumentList @('-NoProfile', '-EncodedCommand', [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($code)))
  [IO.File]::WriteAllText($holdPid, "$($holder.Id)", $utf8)
  Start-Sleep -Seconds 3
  $held = try { [IO.File]::Open($fontFile, 'Open', 'Read', 'ReadWrite').Close(); $false } catch { $true }
  "holding $fontFile for $HoldSeconds s in pid $($holder.Id); a second open is refused: $held"
}
if ($ReleaseFont) {
  if (-not (Test-Path -LiteralPath $holdPid)) { 'no font holder was started (no holdfont.pid)' }
  else {
    $id = [int]([IO.File]::ReadAllText($holdPid).Trim())
    $p = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($p -and $p.ProcessName -eq 'powershell') { Stop-Process -Id $id -Force; "font holder pid $id ended" } else { "font holder pid $id was no longer running" }
    Remove-Item -LiteralPath $holdPid
  }
}
