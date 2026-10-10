$ErrorActionPreference = 'Stop'
Add-Type -Namespace W -Name Font -MemberDefinition @'
[DllImport("gdi32.dll", CharSet = CharSet.Unicode)] public static extern int AddFontResourceW(string f);
[DllImport("gdi32.dll", CharSet = CharSet.Unicode)] public static extern bool RemoveFontResourceW(string f);
[DllImport("user32.dll")] public static extern IntPtr SendMessageTimeoutW(IntPtr h, uint m, UIntPtr w, IntPtr l, uint flags, uint timeout, out UIntPtr r);
'@
function Broadcast { $r = [UIntPtr]::Zero; [void][W.Font]::SendMessageTimeoutW([IntPtr]0xffff, 0x001D, [UIntPtr]::Zero, [IntPtr]::Zero, 2, 1000, [ref]$r) }
function Has($family) { powershell -NoProfile -Command "Add-Type -AssemblyName System.Drawing; [bool]((New-Object System.Drawing.Text.InstalledFontCollection).Families | Where-Object Name -eq '$family')" }

"windows: $((Get-CimInstance Win32_OperatingSystem).Caption) $((Get-CimInstance Win32_OperatingSystem).Version)"
$zip = Join-Path $env:RUNNER_TEMP 'cascadia.zip'
Invoke-WebRequest 'https://github.com/microsoft/cascadia-code/releases/download/v2407.24/CascadiaCode-2407.24.zip' -OutFile $zip
Expand-Archive $zip (Join-Path $env:RUNNER_TEMP 'cascadia') -Force
$src = Get-ChildItem (Join-Path $env:RUNNER_TEMP 'cascadia') -Recurse -Filter 'CascadiaCode.ttf' | Select-Object -First 1
$fontsDir = Join-Path $env:LOCALAPPDATA 'Microsoft\Windows\Fonts'
New-Item -ItemType Directory -Force $fontsDir | Out-Null
$dst = Join-Path $fontsDir 'CascadiaCode.ttf'
$key = 'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts'
"before: $(Has 'Cascadia Code')"
Copy-Item $src.FullName $dst -Force
New-ItemProperty -Path $key -Name 'Cascadia Code (TrueType)' -Value $dst -PropertyType String -Force | Out-Null
"add=$([W.Font]::AddFontResourceW($dst))"; Broadcast
"after install: $(Has 'Cascadia Code')"
"remove=$([W.Font]::RemoveFontResourceW($dst))"; Broadcast
Remove-ItemProperty -Path $key -Name 'Cascadia Code (TrueType)'
Remove-Item $dst
"after uninstall: $(Has 'Cascadia Code')"
