# Host-side helpers for the phase 4b spike and update check (protocol.md). Dot-source after
# spikes\vm\vm.ps1, docs\spikes\phase3-vm-check\helpers.ps1 and docs\spikes\phase4-install\helpers.ps1,
# in Windows PowerShell 5.1 on the host, from the repo root. Only the CMD Studio test VMs get
# anything from them; this PC's installer is never run, nothing listens on this PC, and nothing
# here reads this PC's registry, fonts or Windows Terminal. Guest paths are built from the VM's
# user name, never from this host's environment (a `$env:` in a double-quoted string would expand
# on the host PC).
$P4B = @{
  Setup = 'C:\cmdstudio\phase4b\setup'     # the installers and the manifests; update-server.ps1 serves it
  Guest = 'C:\cmdstudio\spikes\vm\guest'   # every script of spikes\vm\guest
  Out   = 'C:\cmdstudio\out\phase4b'       # the snapshots
}
# The scripts and the downloaded CI artifact into the VM: every script of spikes\vm\guest,
# phase 3's read-autorun.vbs where Get-GuestAutoRun looks for it (as 4a's setup does), and every
# *-setup.exe and *.json of $Artifact into $P4B.Setup, each installer's hash compared with the
# host's; the artifact's latest.json is kept as latest.real.json for Set-Phase4bLatest. 4a's
# $P4.Setup is pointed at $P4B.Setup afterwards, so 4a's helpers keep working for one installer.
function Install-Phase4bSetup($Vm, [string] $User, [string] $Artifact) {
  $repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
  Invoke-GuestPs $Vm "New-Item -ItemType Directory -Force -Path '$($P4B.Out)', '$($P4B.Setup)' | Out-Null" $User 60 | Out-Null
  $guest = @(Get-ChildItem -LiteralPath (Join-Path $repo 'spikes\vm\guest') -File | ForEach-Object FullName)
  Copy-ToGuest $Vm $guest ($P4B.Guest + '\') $User | Out-Null
  Copy-ToGuest $Vm @((Join-Path $repo 'docs\spikes\phase3-vm-check\read-autorun.vbs')) ($P3.Check + '\') $User | Out-Null
  $files = @(Get-ChildItem -LiteralPath $Artifact -File | Where-Object { $_.Name -like '*-setup.exe' -or $_.Name -like '*.json' })
  if (@($files | Where-Object { $_.Name -like '*-setup.exe' }).Count -eq 0) { throw "no *-setup.exe in $Artifact" }
  Copy-ToGuest $Vm @($files | ForEach-Object FullName) ($P4B.Setup + '\') $User | Out-Null
  foreach ($exe in @($files | Where-Object { $_.Name -like '*-setup.exe' })) {
    $hostHash = (Get-FileHash -LiteralPath $exe.FullName -Algorithm SHA256).Hash
    $inVm = "$(Invoke-GuestPs $Vm "(Get-FileHash -LiteralPath '$($P4B.Setup)\$($exe.Name)' -Algorithm SHA256).Hash" $User 180)".Trim()
    Write-Host ('installer: ' + $exe.Name + ' ' + $exe.Length + ' bytes; same file in the VM: ' + ($inVm -eq $hostHash))
  }
  if (Test-Path -LiteralPath (Join-Path $Artifact 'latest.json')) {
    Invoke-GuestPs $Vm "Copy-Item -LiteralPath '$($P4B.Setup)\latest.json' -Destination '$($P4B.Setup)\latest.real.json' -Force" $User 60 | Out-Null
  }
  $script:P4.Setup = $P4B.Setup
  $files | ForEach-Object Name
}
# One of the spike's installers (-Which 1 or 2), on the desktop; -Silent runs it with /S and waits
# (no app starts then). 4a's install-check.ps1 -Setup takes a file, so the name with its spaces
# never passes through a command line unquoted.
function Start-Phase4bInstall($Vm, [int] $Which, [string] $User = 'tester', [switch] $Silent) {
  $file = "$($P4B.Setup)\CMD Studio_0.1.0-spike.$Which" + '_x64-setup.exe'
  if ($Silent) { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" "-InstallSilent -Setup '$file'" $User 600 }
  else { Invoke-GuestDesktop $Vm "$($P4.Guest)\install-check.ps1" "-Install -Setup '$file'" $User 120 }
}
function Get-Phase4bSnapshot($Vm, [string] $Which, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-check.ps1" "-Snapshot $Which" $User 300 }
function Set-Phase4bMode($Vm, [ValidateSet('auto', 'notify', 'off')] [string] $Mode, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-check.ps1" "-SetMode $Mode" $User 60 }
function Wait-Phase4bVersion($Vm, [string] $Version, [string] $User = 'tester', [int] $Seconds = 180) { Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-check.ps1" "-WaitForVersion $Version -WaitSeconds $Seconds" $User ($Seconds + 60) }
function Get-Phase4bLog($Vm, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-check.ps1" '-Log' $User 60 }
# Every log line matching $Pattern (a regex of letters, digits, `_ |=.-` only), with its time, in
# order: the lines -Log leaves out (A5b: `recovery_entry`, `pause `).
function Get-Phase4bLogLines($Vm, [string] $Pattern, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-check.ps1" "-LogLines '$Pattern'" $User 60 }
# (Not `$args`: that name is PowerShell's automatic variable inside a function.)
function Start-Phase4bApp($Vm, [string] $User = 'tester', [switch] $Recovery) {
  $how = if ($Recovery) { '-StartApp -Recovery' } else { '-StartApp' }
  Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-check.ps1" $how $User 120
}
# The guest's web server over $P4B.Setup (update-server.ps1), started from the desktop as a
# hidden process of its own, created through WMI so that it is not the desktop task's child; its request log is read for U7's "no request in Off mode".
function Start-Phase4bServer($Vm, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-server.ps1" "-Start -Root '$($P4B.Setup)'" $User 60 }
# Whether the server outlived the desktop task that started it: a separate guestcontrol call
# (Invoke-GuestPs, not on the desktop), made after Start-Phase4bServer has returned, so the task
# is gone, waits 5 s, connects to the port and reads server.pid. Prints `server up: True` and
# `server pid <n> running: True` when it did.
function Test-Phase4bServer($Vm, [string] $User = 'tester') {
  $code = 'Start-Sleep -Seconds 5; $up = try { $c = New-Object Net.Sockets.TcpClient -ArgumentList ''127.0.0.1'', 8765; $c.Close(); $true } catch { $false }; ''server up: '' + $up; $f = ''__PID__''; if (Test-Path -LiteralPath $f) { $id = [int](Get-Content -LiteralPath $f); ''server pid '' + $id + '' running: '' + [bool](Get-Process -Id $id -ErrorAction SilentlyContinue) } else { ''no server.pid'' }'
  Invoke-GuestPs $Vm $code.Replace('__PID__', "$($P4B.Setup)\server.pid") $User 60
}
function Stop-Phase4bServer($Vm, [string] $User = 'tester') { Invoke-GuestDesktop $Vm "$($P4B.Guest)\update-server.ps1" "-Stop -Root '$($P4B.Setup)'" $User 60 }
function Get-Phase4bServerLog($Vm, [string] $User = 'tester') {
  Invoke-GuestPs $Vm "if (Test-Path -LiteralPath '$($P4B.Setup)\server.log') { Get-Content -LiteralPath '$($P4B.Setup)\server.log' } else { 'no server.log' }" $User 60
}
# Which manifest the VM's server answers as latest.json: 'latest' (the artifact's, offering
# spike.2), 'same' (spike.1 again, U5) or 'bad' (not JSON, U5); a copy inside the guest.
function Set-Phase4bLatest($Vm, [ValidateSet('latest', 'same', 'bad')] [string] $Which, [string] $User = 'tester') {
  $from = switch ($Which) { 'latest' { 'latest.real.json' } 'same' { 'latest.same.json' } 'bad' { 'latest.bad.json' } }
  Invoke-GuestPs $Vm "Copy-Item -LiteralPath '$($P4B.Setup)\$from' -Destination '$($P4B.Setup)\latest.json' -Force; 'latest.json is now $Which'" $User 60
}
# The VM's network cable, for Task 9's offline case against GitHub (the controller's host command).
function Set-Phase4bNic([string] $VmName, [ValidateSet('on', 'off')] [string] $State) {
  & (Join-Path $env:ProgramFiles 'Oracle\VirtualBox\VBoxManage.exe') controlvm $VmName setlinkstate1 $State
  "nic of $VmName $State"
}
