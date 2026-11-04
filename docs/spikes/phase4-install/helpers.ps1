# Host-side helpers for the phase 4a VM check (protocol.md). Dot-source after spikes\vm\vm.ps1
# and docs\spikes\phase3-vm-check\helpers.ps1, in Windows PowerShell 5.1 on the host, from the
# repo root. Only the CMD Studio test VMs get anything from them; this PC's installer is never
# run, and nothing here reads this PC's registry, fonts or Windows Terminal.
#
# Guest paths are built from the VM's user name ($User), never from this host's environment: a
# `$env:` in a double-quoted string would expand on the host PC. Code sent to the guest is in
# single quotes wherever it holds a `$`.
$P4 = @{
  Setup = 'C:\cmdstudio\phase4\setup'    # the installer and SHA256SUMS.txt
  Guest = 'C:\cmdstudio\spikes\vm\guest'  # every script of spikes\vm\guest (install-check.ps1, run-on-desktop.ps1, guard.test.ps1 ...)
  Out   = 'C:\cmdstudio\out\phase4'       # the snapshots and the CLI's outputs
}

# Puts the scripts and the one downloaded CI artifact into the VM and compares the installer's
# hash with the host's and with SHA256SUMS.txt. The whole of spikes\vm\guest goes in, because
# guard.test.ps1 parses every script it names; phase 3's read-autorun.vbs goes where
# Get-GuestAutoRun looks for it. -MarkOfTheWeb gives the copy the mark a browser download has
# (Zone.Identifier, ZoneId 3), so that SmartScreen judges it as a download (protocol step 2).
# The report lines go to the host's screen; the function returns the installer's file name.
function Install-Phase4Setup($Vm, [string] $User, [string] $Artifact, [switch] $MarkOfTheWeb) {
  $repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
  Invoke-GuestPs $Vm "New-Item -ItemType Directory -Force -Path '$($P4.Out)' | Out-Null" $User 60 | Out-Null
  $guest = @(Get-ChildItem -LiteralPath (Join-Path $repo 'spikes\vm\guest') -File | ForEach-Object FullName)
  Copy-ToGuest $Vm $guest ($P4.Guest + '\') $User | Out-Null
  Copy-ToGuest $Vm @((Join-Path $repo 'docs\spikes\phase3-vm-check\read-autorun.vbs')) ($P3.Check + '\') $User | Out-Null
  $exe = @(Get-ChildItem -LiteralPath $Artifact -Filter '*-setup.exe')
  if ($exe.Count -ne 1) { throw "expected one *-setup.exe in $Artifact, found $($exe.Count)" }
  $exe = $exe[0]
  Copy-ToGuest $Vm @($exe.FullName, (Join-Path $Artifact 'SHA256SUMS.txt')) ($P4.Setup + '\') $User | Out-Null
  $inGuest = "$($P4.Setup)\$($exe.Name)"
  if ($MarkOfTheWeb) {
    $mark = "Set-Content -LiteralPath '$inGuest' -Stream Zone.Identifier -Value @('[ZoneTransfer]', 'ZoneId=3', 'HostUrl=https://github.com/') -Encoding ASCII; 'mark of the web: ' + [bool](Get-Item -LiteralPath '$inGuest' -Stream Zone.Identifier -ErrorAction SilentlyContinue)"
    Write-Host "$(Invoke-GuestPs $Vm $mark $User 60)".Trim()
  }
  $hostHash = (Get-FileHash -LiteralPath $exe.FullName -Algorithm SHA256).Hash
  $inVm = "$(Invoke-GuestPs $Vm "(Get-FileHash -LiteralPath '$inGuest' -Algorithm SHA256).Hash" $User 180)".Trim()
  $sums = @((Get-Content -LiteralPath (Join-Path $Artifact 'SHA256SUMS.txt')) -match $hostHash.ToLowerInvariant()).Count
  Write-Host ('installer: ' + $exe.Name + ' ' + $exe.Length + ' bytes')
  Write-Host ('sha256 on the host: ' + $hostHash)
  Write-Host ('sha256 in the VM:   ' + $inVm)
  Write-Host ('same file: ' + ($inVm -eq $hostHash))
  Write-Host ('lines of SHA256SUMS.txt with that hash: ' + $sums)
  $exe.Name
}

# install-check.ps1 on the user's desktop (Invoke-GuestDesktop): HKCU and the environment are
# the user's there, and a program it starts runs where the user sees it.
function Get-Phase4Snapshot($Vm, [string] $Which, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" "-Snapshot $Which" $User 300 }
function Compare-Phase4($Vm, [ValidateSet('ThemesKept', 'ThemesGone')] [string] $Themes, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" "-Compare -$Themes" $User 300 }
function Get-Phase4Cli($Vm, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" '-Cli' $User 300 }
# The installer of $P4.Setup (install-check.ps1 picks the one *-setup.exe there, so the file name
# with its spaces never passes through a command line), started on the desktop: -Silent runs it
# with /S and waits for it; otherwise it returns at once and the pages are driven with keys.
function Start-Phase4Install($Vm, [string] $User = 'tester', [switch] $Silent) {
  if ($Silent) { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" "-InstallSilent -Setup $($P4.Setup)" $User 600 }
  else { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" "-Install -Setup $($P4.Setup)" $User 120 }
}
function Start-Phase4Uninstall($Vm, [string] $User = 'tester', [switch] $Silent) {
  if ($Silent) { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" '-UninstallSilent' $User 600 }
  else { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" '-Uninstall' $User 120 }
}
# The installed font file held open (-HoldFont) for the cleanup that cannot finish, and let go
# (-ReleaseFont); the installed cmdstudio.com --cleanup --remove-themes (-CleanupAll).
function Start-Phase4FontHold($Vm, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" '-HoldFont' $User 120 }
function Stop-Phase4FontHold($Vm, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" '-ReleaseFont' $User 120 }
# Whether the holder still runs once the desktop task that started it has ended (console-free).
function Get-Phase4FontHold($Vm, [string] $User = 'tester') {
  Invoke-GuestPs $Vm ("`$f = '$($P4.Out)\holdfont.pid'; " + 'if (Test-Path -LiteralPath $f) { $id = [int](Get-Content -LiteralPath $f); ''holder pid '' + $id + '' running: '' + [bool](Get-Process -Id $id -ErrorAction SilentlyContinue) } else { ''no holdfont.pid'' }') $User 60
}
function Invoke-Phase4CleanupAll($Vm, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" '-CleanupAll' $User 300 }

# The install folder's entries and the running app, read through guestcontrol (console-free).
function Get-Phase4Installed($Vm, [string] $User = 'tester') {
  $inst = "C:\Users\$User\AppData\Local\CMD Studio"
  Invoke-GuestPs $Vm ("if (Test-Path -LiteralPath '$inst') { Get-ChildItem -LiteralPath '$inst' -Force | ForEach-Object Name } else { 'install folder: ABSENT' }; " + 'Get-Process cmdstudio -ErrorAction SilentlyContinue | ForEach-Object { ''running: '' + $_.Id }') $User 120
}

# The app closed the way a person closes it (CloseMainWindow: Tauri's exit event, the run record
# removed), on the user's desktop, where its window is visible to Get-Process; ended only if it
# does not answer in 10 s, and the lines say which happened.
function Close-Phase4App($Vm, [string] $User = 'tester') {
  $code = @'
$asked = @(Get-Process cmdstudio -ErrorAction SilentlyContinue | Where-Object { $_.CloseMainWindow() }).Count
Start-Sleep -Seconds 10
$left = @(Get-Process cmdstudio -ErrorAction SilentlyContinue)
$left | Stop-Process -Force
'closed with CloseMainWindow: {0}; ended by force after 10 s: {1}' -f $asked, $left.Count
"cmdstudio processes now: $(@(Get-Process cmdstudio -ErrorAction SilentlyContinue).Count)"
'@
  Invoke-GuestSnippet $Vm $code $User
}
# Every cmd window and Windows Terminal ended outright, through guestcontrol.
function Close-Phase4Cmd($Vm, [string] $User = 'tester') {
  Invoke-GuestPs $Vm 'Get-Process cmd, WindowsTerminal, OpenConsole -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Seconds 2; "cmd windows now: $(@(Get-Process cmd -ErrorAction SilentlyContinue).Count)"' $User 60
}

# Presses, ticks or selects the control with this name in any top-level window on the desktop
# (the installer, the uninstaller, SmartScreen), through UI Automation: Invoke for a button or a
# link, Toggle for a check box, Select for a radio button. Best effort, like phase 3's
# Invoke-GuestButton (which looks in the app's window only); the keys and a screenshot are the
# fallback. A desktop reading. The lines say what was found and its state afterwards.
function Invoke-Phase4Control($Vm, [string] $Name, [string] $User = 'tester') {
  $code = @'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$ae = [System.Windows.Automation.AutomationElement]
$scope = [System.Windows.Automation.TreeScope]
$named = New-Object System.Windows.Automation.PropertyCondition($ae::NameProperty, '__NAME__')
$hit = $null
foreach ($w in $ae::RootElement.FindAll($scope::Children, [System.Windows.Automation.Condition]::TrueCondition)) {
  $hit = $w.FindFirst($scope::Descendants, $named)
  if ($hit) { 'found "__NAME__" in the window "{0}" (pid {1}), a {2}' -f $w.Current.Name, $w.Current.ProcessId, $hit.Current.ControlType.ProgrammaticName; break }
}
if (-not $hit) { 'no control named "__NAME__" in any top-level window'; return }
$p = $null
if ($hit.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$p)) { $p.Toggle(); 'toggled, now: ' + $p.Current.ToggleState }
elseif ($hit.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$p)) { $p.Select(); 'selected: ' + $p.Current.IsSelected }
elseif ($hit.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) { $p.Invoke(); 'invoked' }
else { 'it offers neither Invoke, Toggle nor Select' }
'@
  Invoke-GuestSnippet $Vm $code.Replace('__NAME__', $Name.Replace("'", "''")) $User
}
