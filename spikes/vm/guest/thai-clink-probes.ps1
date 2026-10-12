# Runs inside the Thai VM as the Thai user (G4 in /k mode, G5, G16, G17). Copies the Clink probes and
# Clink itself under the user's profile (C:\Users\ผู้ทดสอบ ไทย), so every path has Thai and a space,
# then runs spikes/clink/run-probes.ps1 from there.
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$root = Join-Path $env:USERPROFILE 'cmdstudio-probes'
New-Item -ItemType Directory -Force $root | Out-Null
if (-not (Test-Path (Join-Path $root 'clink'))) { Copy-Item 'C:\cmdstudio\spikes\clink' (Join-Path $root 'clink') -Recurse }
if (-not (Test-Path (Join-Path $root 'clink-bin'))) { Copy-Item 'C:\cmdstudio\clink-bin' (Join-Path $root 'clink-bin') -Recurse }
"probe root: $root"
"system locale: $((Get-WinSystemLocale).Name), ANSI code page: $([Text.Encoding]::Default.CodePage), UI culture: $((Get-UICulture).Name)"
& (Join-Path $root 'clink\run-probes.ps1') -Clink (Join-Path $root 'clink-bin\clink_x64.exe')
