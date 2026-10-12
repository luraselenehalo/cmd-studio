# Creates one headless test VM and starts an unattended Windows install (Phase 0, Task 8).
# Everything lands under %USERPROFILE%\VMs: machines\<Name> holds the VM, accounts\<Name>.password
# holds its test account's random password. Neither is in the repo.
# VirtualBox's own answer file (UnattendedTemplates\win_nt6_unattended.xml) accepts the Windows EULA,
# skips OOBE, makes the user a local administrator with auto-logon, and installs Guest Additions.
param(
  [Parameter(Mandatory)] [string] $Name,
  [Parameter(Mandatory)] [ValidateSet('Windows11_64', 'Windows10_64')] [string] $OsType,
  [Parameter(Mandatory)] [string] $Iso,
  [Parameter(Mandatory)] [int] $ImageIndex,
  [int] $MemoryMB = 4096,
  [int] $Cpus = 4,
  [int] $DiskGB = 64,
  [string] $User = 'tester',
  [string] $FullName = 'CMD Studio Tester',
  [string] $Locale = 'en_US',
  [string] $Country = 'US',
  # Runs elevated at first logon inside VBOXPOST.CMD. It is pasted into a .cmd file verbatim,
  # so pass PowerShell as -EncodedCommand to keep quotes and & out of it.
  [string] $PostInstallCommand,
  # 'separate' shows a window to watch the install; closing it leaves the VM running.
  [ValidateSet('headless', 'separate')] [string] $StartType = 'headless'
)
$ErrorActionPreference = 'Stop'
$vbm = 'C:\Program Files\Oracle\VirtualBox\VBoxManage.exe'
$machines = Join-Path $env:USERPROFILE 'VMs\machines'
$accounts = Join-Path $env:USERPROFILE 'VMs\accounts'
New-Item -ItemType Directory -Force $machines, $accounts | Out-Null
# VBoxManage prints progress on stderr. Windows PowerShell 5.1 turns redirected stderr lines into
# error records, so 'Stop' must not apply inside the call; the exit code decides.
function V {
  $ErrorActionPreference = 'Continue'
  & $vbm @args 2>&1 | ForEach-Object { "$_" }
  if ($LASTEXITCODE) { throw "VBoxManage $($args -join ' ') exited $LASTEXITCODE" }
}

if ((& $vbm list vms) -match "^`"$([regex]::Escape($Name))`" ") { throw "$Name already exists" }

# A random password for the throwaway test account, kept outside the repo.
$bytes = New-Object byte[] 18
[Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
$pwFile = Join-Path $accounts $(if ($User -eq 'tester') { "$Name.password" } else { "$Name.$User.password" })
[IO.File]::WriteAllText($pwFile, ([Convert]::ToBase64String($bytes) -replace '[+/=]', 'x'))
"account: $User, password in $pwFile"

V createvm --name $Name --ostype $OsType --basefolder $machines --default --register
V modifyvm $Name --memory=$MemoryMB --cpus=$Cpus --vram=128 --graphicscontroller=vboxsvga --firmware=efi `
  --audio-enabled=off --clipboard-mode=disabled --drag-and-drop=disabled --nic1=nat
# Real PCs ship with Secure Boot on, so the VM does too.
V modifynvram $Name inituefivarstore
V modifynvram $Name enrollmssignatures
V modifynvram $Name enrollorclpk
V modifynvram $Name secureboot --enable

$info = & $vbm showvminfo $Name --machinereadable
$ctl = ($info | Select-String '^storagecontrollername0="(.*)"').Matches[0].Groups[1].Value
$vdi = Join-Path (Join-Path $machines $Name) "$Name.vdi"
V createmedium disk --filename $vdi --size ($DiskGB * 1024) --format VDI
V storageattach $Name --storagectl $ctl --port 0 --device 0 --type hdd --medium $vdi

# @() around the if: a one-item array would come out of the if as a plain string, and splatting a
# string passes each character as its own argument.
$post = @(if ($PostInstallCommand) { "--post-install-command=$PostInstallCommand" })
V unattended install $Name --iso=$Iso --user=$User --user-password-file=$pwFile --full-user-name=$FullName `
  --install-additions --locale=$Locale --country=$Country --hostname="$Name.test" --image-index=$ImageIndex @post --start-vm=$StartType
& $vbm showvminfo $Name --machinereadable | Select-String '^(ostype|firmware|tpm_type|memory|cpus|VMState|storagecontrollername0)='
