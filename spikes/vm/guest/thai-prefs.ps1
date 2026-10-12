# Runs inside the Thai VM on the user's desktop (Invoke-GuestDesktop). guestcontrol processes get the
# user's environment but not necessarily the user's HKCU, so per-user settings are made from here.
# -Set: Thai display language, Thai regional format, home location Thailand (takes effect after sign-in).
param([switch] $Set)
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script changes the user's language settings" }
[Console]::OutputEncoding = [Text.Encoding]::UTF8
if ($Set) {
  Set-WinUILanguageOverride -Language th-TH
  Set-WinUserLanguageList -LanguageList th-TH, en-US -Force
  Set-Culture -CultureInfo th-TH
  Set-WinHomeLocation -GeoId 227
  'set: UI th-TH, culture th-TH, home 227 (UI applies after the next sign-in)'
}
"user: $([Security.Principal.WindowsIdentity]::GetCurrent().Name) profile: $env:USERPROFILE"
"UI culture: $((Get-UICulture).Name) | UI override: $((Get-WinUILanguageOverride).Name) | culture: $((Get-Culture).Name) | date: $(Get-Date -Format D)"
"HKCU LocaleName: $((Get-ItemProperty 'HKCU:\Control Panel\International').LocaleName) | PreferredUILanguages: $((Get-ItemProperty 'HKCU:\Control Panel\Desktop' -ErrorAction SilentlyContinue).PreferredUILanguages -join ',')"
"system locale: $((Get-WinSystemLocale).Name) | ANSI code page: $([Text.Encoding]::Default.CodePage)"
