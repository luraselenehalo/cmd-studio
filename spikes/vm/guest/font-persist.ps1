# Runs inside a test VM, on the user's desktop. Open G12 items in docs/spikes/G12-G13-windows.md:
#   -Install  per-user install of CaskaydiaCove NF, left installed
#   -Check    is the family visible to a new process? Run it again after a restart.
#   -Remove   per-user removal
param([switch] $Install, [switch] $Check, [switch] $Remove)
$ErrorActionPreference = 'Stop'
Add-Type -Namespace W -Name Font -MemberDefinition @'
[DllImport("gdi32.dll", CharSet = CharSet.Unicode)] public static extern int AddFontResourceW(string f);
[DllImport("gdi32.dll", CharSet = CharSet.Unicode)] public static extern bool RemoveFontResourceW(string f);
[DllImport("user32.dll")] public static extern IntPtr SendMessageTimeoutW(IntPtr h, uint m, UIntPtr w, IntPtr l, uint flags, uint timeout, out UIntPtr r);
'@
function Broadcast { $r = [UIntPtr]::Zero; [void][W.Font]::SendMessageTimeoutW([IntPtr]0xffff, 0x001D, [UIntPtr]::Zero, [IntPtr]::Zero, 2, 1000, [ref]$r) }
function Has($family) { "$(powershell -NoProfile -Command "Add-Type -AssemblyName System.Drawing; [bool]((New-Object System.Drawing.Text.InstalledFontCollection).Families | Where-Object Name -eq '$family')")".Trim() -eq 'True' }

$family = 'CaskaydiaCove NF'
$file = 'CaskaydiaCoveNerdFont-Regular.ttf'
$dst = Join-Path $env:LOCALAPPDATA "Microsoft\Windows\Fonts\$file"
$key = 'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts'
$name = "$family (TrueType)"
"user=$env:USERNAME session=$((Get-Process -Id $PID).SessionId)"
if ($Install) {
  $zip = Join-Path $env:TEMP 'nerd-cascadia.zip'
  Invoke-WebRequest 'https://github.com/ryanoasis/nerd-fonts/releases/download/v3.5.1/CascadiaCode.zip' -OutFile $zip -UseBasicParsing
  $h = (Get-FileHash $zip -Algorithm SHA256).Hash
  if ($h -ne '1298BF92698AFA06185CF1D05E6AE05F2D8A1E8C3CB45DDF4C3035168AB342A1') { throw "zip sha256 $h differs from CI run 38065370337" }
  $x = Join-Path $env:TEMP 'nerd-cascadia'
  Expand-Archive $zip $x -Force
  $src = Get-ChildItem $x -Recurse -Filter $file | Select-Object -First 1
  New-Item -ItemType Directory -Force (Split-Path $dst) | Out-Null
  Copy-Item $src.FullName $dst -Force
  if (-not (Test-Path $key)) { New-Item -Path $key | Out-Null }
  New-ItemProperty -Path $key -Name $name -Value $dst -PropertyType String -Force | Out-Null
  "add=$([W.Font]::AddFontResourceW($dst))"; Broadcast
  "installed: visible=$(Has $family)"
}
if ($Check) { "check: visible=$(Has $family) file=$(Test-Path $dst) value=$($null -ne (Get-ItemProperty $key -Name $name -ErrorAction SilentlyContinue))" }
if ($Remove) {
  "remove=$([W.Font]::RemoveFontResourceW($dst))"; Broadcast
  Remove-ItemProperty -Path $key -Name $name -ErrorAction SilentlyContinue
  Remove-Item $dst -ErrorAction SilentlyContinue
  "removed: visible=$(Has $family) file=$(Test-Path $dst)"
}
