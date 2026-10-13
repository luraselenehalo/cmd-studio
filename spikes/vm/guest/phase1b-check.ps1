# Runs inside a test VM on the signed-in user's desktop (Invoke-GuestDesktop): the Phase 1b
# apply -> doctor -> undo check (plan Task 19). Each -Step does one part:
#   OpenWt          start Windows Terminal once, so it writes its settings.json, then close it
#   ForeignAutoRun  give AutoRun another program's command (REG_EXPAND_SZ) for our entry to go in front of
#   Snapshot        save AutoRun's key, the per-user font key, settings.json and the font lists to out\phase1b\<Label>
#   Apply           cmdstudio apply <Theme>       Doctor  cmdstudio doctor (and --json to a file)
#   ShowAutoRun     print AutoRun as the registry holds it, and how often the other program's command ran
#   ShowClinkPath   print the clink.path line CMD Studio wrote for Clink
#   HideOmp         rename oh-my-posh.exe away (a broken runtime); RestoreOmp puts it back
#   CloseCmd        close every cmd window and Windows Terminal
#   Undo            cmdstudio undo                Compare  are snapshots <Against> and <Label> byte-identical?
param(
  [Parameter(Mandatory)]
  [ValidateSet('OpenWt', 'ForeignAutoRun', 'Snapshot', 'Apply', 'ShowAutoRun', 'ShowClinkPath', 'Doctor', 'HideOmp', 'RestoreOmp', 'CloseCmd', 'Undo', 'Compare')]
  [string] $Step,
  [string] $Label = 'before',
  [string] $Theme = '',
  [string] $Against = 'before'
)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script applies and undoes CMD Studio themes" }
# cmdstudio.exe prints UTF-8; without this, Thai paths come back as '?'.
[Console]::OutputEncoding = [Text.Encoding]::UTF8
# C:\cmdstudio in the VM: this script lives in C:\cmdstudio\spikes\vm\guest.
$root = Split-Path (Split-Path (Split-Path $PSScriptRoot))
$app = Join-Path $root 'app\cmdstudio.exe'
$out = Join-Path $root 'out\phase1b'
$settings = Join-Path $env:LOCALAPPDATA 'Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json'
$data = Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio'
$omp = Join-Path $data 'runtime\omp-31.5.0'
# The other program's command: it adds a line to a file each time a cmd window starts.
$foreign = Join-Path $env:USERPROFILE 'cmdstudio-foreign.cmd'
$foreignLog = Join-Path $env:USERPROFILE 'cmdstudio-foreign.txt'

function Invoke-App([string[]] $Arguments) {
  # Windows PowerShell 5.1 turns a native program's stderr into error records; 'Stop' would end the script.
  $ErrorActionPreference = 'Continue'
  & $app @Arguments 2>&1 | ForEach-Object { "$_" }
  "exit $LASTEXITCODE"
}

# reg.exe keeps every value's type and bytes. An absent key is written as the word "absent".
function Export-Key([string] $Key, [string] $File) {
  $ErrorActionPreference = 'Continue'
  $null = & reg.exe query $Key 2>&1
  if ($LASTEXITCODE -eq 0) { $null = & reg.exe export $Key $File /y 2>&1 } else { Set-Content -LiteralPath $File -Value 'absent' }
}

switch ($Step) {
  'OpenWt' {
    # A fresh Windows has no settings.json until Windows Terminal first starts (plan Review Focus 1).
    # It starts twice, so anything it adds on a second start lands before the "before" snapshot.
    $wt = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\wt.exe'
    if (-not (Test-Path -LiteralPath $wt)) { 'Windows Terminal is not installed'; break }
    $had = Test-Path -LiteralPath $settings
    foreach ($run in 1, 2) {
      Start-Process -FilePath $wt
      $deadline = (Get-Date).AddSeconds(60)
      while (-not (Test-Path -LiteralPath $settings) -and (Get-Date) -lt $deadline) { Start-Sleep -Seconds 1 }
      Start-Sleep -Seconds 5
      Stop-Process -Name WindowsTerminal -Force -ErrorAction SilentlyContinue
      Start-Sleep -Seconds 2
    }
    if (-not (Test-Path -LiteralPath $settings)) { throw 'Windows Terminal wrote no settings.json within 60 s' }
    if ($had) { 'settings.json already existed' } else { 'settings.json created by Windows Terminal' }
  }
  'ForeignAutoRun' {
    # Ruling P59: our entry goes in front, in parentheses, so another program's `if` neither keeps
    # it from running nor is cut short by it. This command is such an `if`, kept unexpanded.
    Set-Content -LiteralPath $foreign -Value '@echo %DATE% %TIME%>>"%USERPROFILE%\cmdstudio-foreign.txt"' -Encoding ASCII
    $key = 'HKCU:\Software\Microsoft\Command Processor'
    if (-not (Test-Path -LiteralPath $key)) { New-Item -Path $key -Force | Out-Null }
    $value = 'if exist "%USERPROFILE%\cmdstudio-foreign.cmd" call "%USERPROFILE%\cmdstudio-foreign.cmd"'
    New-ItemProperty -Path $key -Name AutoRun -PropertyType ExpandString -Value $value -Force | Out-Null
    "AutoRun holds another program's command: $value"
  }
  'Snapshot' {
    $dir = Join-Path $out $Label
    New-Item -ItemType Directory -Force $dir | Out-Null
    Export-Key 'HKCU\Software\Microsoft\Command Processor' (Join-Path $dir 'command-processor.reg')
    Export-Key 'HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts' (Join-Path $dir 'user-fonts.reg')
    if (Test-Path -LiteralPath $settings) { Copy-Item -LiteralPath $settings (Join-Path $dir 'settings.json') -Force }
    else { Set-Content -LiteralPath (Join-Path $dir 'settings.json') -Value 'absent' }
    Add-Type -AssemblyName System.Drawing
    (New-Object System.Drawing.Text.InstalledFontCollection).Families | ForEach-Object { $_.Name } | Sort-Object |
      Set-Content -LiteralPath (Join-Path $dir 'font-families.txt') -Encoding UTF8
    $fonts = Join-Path $env:LOCALAPPDATA 'Microsoft\Windows\Fonts'
    $files = @(Get-ChildItem -LiteralPath $fonts -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Name) $($_.Length)" })
    # Written even when the folder is empty or missing, so Compare always has both files.
    [IO.File]::WriteAllText((Join-Path $dir 'user-font-files.txt'), ($files -join "`n"))
    Get-ChildItem -LiteralPath $dir | ForEach-Object { '{0} {1}' -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash, $_.Name }
  }
  'Apply' { Invoke-App @('apply', $Theme) }
  'ShowAutoRun' {
    $key = Get-Item -LiteralPath 'HKCU:\Software\Microsoft\Command Processor' -ErrorAction SilentlyContinue
    if ($key -and ($key.GetValueNames() -contains 'AutoRun')) {
      # The raw text and its type, as cmd reads them: never expanded.
      'AutoRun ({0}): {1}' -f $key.GetValueKind('AutoRun'), $key.GetValue('AutoRun', $null, 'DoNotExpandEnvironmentNames')
    } else { 'AutoRun: absent' }
    $runs = @(Get-Content -LiteralPath $foreignLog -ErrorAction SilentlyContinue).Count
    "the other program's command ran $runs time(s)"
  }
  'ShowClinkPath' {
    # clink.path is a ;-separated list; on VM-C it holds the Thai user name.
    $file = Join-Path $data 'clink-profile\clink_settings'
    Get-Content -LiteralPath $file -Encoding UTF8 | Where-Object { $_ -like 'clink.path*' }
  }
  'Doctor' {
    Invoke-App @('doctor')
    New-Item -ItemType Directory -Force $out | Out-Null
    $ErrorActionPreference = 'Continue'
    & $app doctor --json 2>$null | Set-Content -LiteralPath (Join-Path $out "doctor-$Label.json") -Encoding UTF8
  }
  'HideOmp' {
    # Ruling P47: with Oh My Posh gone, a new cmd window shows no error text (init.lua's popen ends
    # in 2>nul), and doctor D2 says what is wrong instead.
    Rename-Item -LiteralPath (Join-Path $omp 'oh-my-posh.exe') -NewName 'oh-my-posh.exe.hidden'
    'oh-my-posh.exe renamed away'
  }
  'RestoreOmp' {
    Rename-Item -LiteralPath (Join-Path $omp 'oh-my-posh.exe.hidden') -NewName 'oh-my-posh.exe'
    'oh-my-posh.exe is back'
  }
  'CloseCmd' {
    # Nothing may hold Clink's DLL or the font file while undo removes them.
    Stop-Process -Name cmd, WindowsTerminal -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 3
    'every cmd window and Windows Terminal closed'
  }
  'Undo' { Invoke-App @('undo') }
  'Compare' {
    $a = Join-Path $out $Against
    $b = Join-Path $out $Label
    $diff = 0
    foreach ($name in 'command-processor.reg', 'user-fonts.reg', 'settings.json', 'font-families.txt', 'user-font-files.txt') {
      $ha = (Get-FileHash -LiteralPath (Join-Path $a $name) -Algorithm SHA256).Hash
      $hb = (Get-FileHash -LiteralPath (Join-Path $b $name) -Algorithm SHA256).Hash
      if ($ha -eq $hb) { "same: $name" } else { "DIFFERENT: $name"; $diff++ }
    }
    if ($diff) { "PHASE1B-DIFFERENT: $diff file(s) differ between $Against and $Label" } else { "PHASE1B-IDENTICAL: $Against and $Label" }
  }
}
