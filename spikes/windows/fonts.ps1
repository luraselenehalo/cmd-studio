$ErrorActionPreference = 'Stop'
Add-Type -Namespace W -Name Font -MemberDefinition @'
[DllImport("gdi32.dll", CharSet = CharSet.Unicode)] public static extern int AddFontResourceW(string f);
[DllImport("gdi32.dll", CharSet = CharSet.Unicode)] public static extern bool RemoveFontResourceW(string f);
[DllImport("user32.dll")] public static extern IntPtr SendMessageTimeoutW(IntPtr h, uint m, UIntPtr w, IntPtr l, uint flags, uint timeout, out UIntPtr r);
'@
Add-Type -AssemblyName System.Drawing
function Broadcast { $r = [UIntPtr]::Zero; [void][W.Font]::SendMessageTimeoutW([IntPtr]0xffff, 0x001D, [UIntPtr]::Zero, [IntPtr]::Zero, 2, 1000, [ref]$r) }
# A fresh process answers, so this is what other programs see.
function Has($family) { "$(powershell -NoProfile -Command "Add-Type -AssemblyName System.Drawing; [bool]((New-Object System.Drawing.Text.InstalledFontCollection).Families | Where-Object Name -eq '$family')")".Trim() -eq 'True' }
# A probe that cannot fail proves nothing, so a wrong answer stops the run.
function Check($what, $actual, $want) { "${what}: $actual"; if ($actual -ne $want) { throw "G12: '$what' was $actual, expected $want" } }

"windows: $((Get-CimInstance Win32_OperatingSystem).Caption) $((Get-CimInstance Win32_OperatingSystem).Version)"
"context: Cascadia Code preinstalled=$(Has 'Cascadia Code') Cascadia Mono preinstalled=$(Has 'Cascadia Mono')"
# Run 38064746693 used Cascadia Code, which this image already has, so it proved nothing.
# CaskaydiaCove Nerd Font is the font the app ships, and stock Windows does not have it.
$zip = Join-Path $env:RUNNER_TEMP 'nerd-cascadia.zip'
Invoke-WebRequest 'https://github.com/ryanoasis/nerd-fonts/releases/download/v3.5.1/CascadiaCode.zip' -OutFile $zip
"zip sha256: $((Get-FileHash $zip -Algorithm SHA256).Hash)"
$x = Join-Path $env:RUNNER_TEMP 'nerd-cascadia'
Expand-Archive $zip $x -Force
$src = Get-ChildItem $x -Recurse -Filter 'CaskaydiaCoveNerdFont-Regular.ttf' | Select-Object -First 1
if (-not $src) { (Get-ChildItem $x -Recurse -Filter '*.ttf').Name; throw 'CaskaydiaCoveNerdFont-Regular.ttf is not in the zip' }
$pfc = New-Object System.Drawing.Text.PrivateFontCollection
$pfc.AddFontFile($src.FullName)
$family = $pfc.Families[0].Name
$pfc.Dispose()
"family in file: $family"

$fontsDir = Join-Path $env:LOCALAPPDATA 'Microsoft\Windows\Fonts'
New-Item -ItemType Directory -Force $fontsDir | Out-Null
$dst = Join-Path $fontsDir $src.Name
$key = 'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts'
# Never New-Item -Force on a registry key: it wipes the key's values and subkeys.
if (-not (Test-Path $key)) { New-Item -Path $key | Out-Null }
$name = "$family (TrueType)"
Check 'before' (Has $family) $false
Copy-Item $src.FullName $dst -Force
New-ItemProperty -Path $key -Name $name -Value $dst -PropertyType String -Force | Out-Null
"add=$([W.Font]::AddFontResourceW($dst))"; Broadcast
Check 'after install' (Has $family) $true
"remove=$([W.Font]::RemoveFontResourceW($dst))"; Broadcast
Remove-ItemProperty -Path $key -Name $name
$deleted = $true; try { Remove-Item $dst } catch { $deleted = $false; "delete error: $($_.Exception.Message)" }
Check 'after uninstall' (Has $family) $false
Check 'file deleted' $deleted $true
