# Host-side helpers for the Phase 0 test VMs (Task 8). Dot-source it: . .\spikes\vm\vm.ps1
# Each VM's test-account password sits in %USERPROFILE%\VMs\accounts, outside the repo.
$Vbm = 'C:\Program Files\Oracle\VirtualBox\VBoxManage.exe'
$Accounts = Join-Path $env:USERPROFILE 'VMs\accounts'
$Shots = Join-Path $PSScriptRoot 'tmp'
$GuestPs = 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'

function Get-PasswordFile($Vm, $User = 'tester') {
  if ($User -eq 'tester') { Join-Path $Accounts "$Vm.password" } else { Join-Path $Accounts "$Vm.$User.password" }
}

# Runs a program in the VM as $User. It does not run on the user's desktop; use Invoke-GuestDesktop for that.
function Invoke-Guest($Vm, [string] $Exe, [string[]] $Arguments = @(), [string] $User = 'tester', [int] $TimeoutSec = 900) {
  & $Vbm guestcontrol $Vm run "--exe=$Exe" "--username=$User" "--passwordfile=$(Get-PasswordFile $Vm $User)" `
    --profile --wait-stdout --wait-stderr "--timeout=$($TimeoutSec * 1000)" -- @Arguments
}

# guestcontrol re-quotes arguments and the inner double quotes get lost, so send the command encoded.
function Invoke-GuestPs($Vm, [string] $Command, [string] $User = 'tester', [int] $TimeoutSec = 900) {
  # UTF-8 stdout, or Thai comes back to the host as '?'.
  $b64 = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes("[Console]::OutputEncoding = [Text.Encoding]::UTF8; $Command"))
  Invoke-Guest $Vm $GuestPs @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', $b64) $User $TimeoutSec
}

# Runs a guest PowerShell script on the signed-in user's desktop and prints its output.
function Invoke-GuestDesktop($Vm, [string] $Script, [string] $ScriptArgs = '', [string] $User = 'tester', [int] $TimeoutSec = 900) {
  $argv = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'C:\cmdstudio\spikes\vm\guest\run-on-desktop.ps1',
    '-Script', $Script, '-TimeoutSec', "$TimeoutSec")
  if ($ScriptArgs) { $argv += @('-ScriptArgs', $ScriptArgs) }
  Invoke-Guest $Vm $GuestPs $argv $User ($TimeoutSec + 60)
}

# The target needs a trailing backslash: without it copyto fails once the folder exists
# ("already exists and is a directory"), and a later update silently never arrives.
function Copy-ToGuest($Vm, [string[]] $Sources, [string] $TargetDir, [string] $User = 'tester') {
  if (-not $TargetDir.EndsWith('\')) { $TargetDir += '\' }
  & $Vbm guestcontrol $Vm mkdir --parents "--username=$User" "--passwordfile=$(Get-PasswordFile $Vm $User)" $TargetDir.TrimEnd('\')
  & $Vbm guestcontrol $Vm copyto --recursive "--username=$User" "--passwordfile=$(Get-PasswordFile $Vm $User)" "--target-directory=$TargetDir" @Sources
}

# True once Guest Additions report a signed-in user, which means setup and first logon are done.
function Test-GuestReady($Vm) {
  $users = & $Vbm guestproperty get $Vm '/VirtualBox/GuestInfo/OS/LoggedInUsers' 2>$null
  $ga = & $Vbm guestproperty get $Vm '/VirtualBox/GuestAdd/Version' 2>$null
  ($ga -match 'Value: ') -and ($users -match 'Value: [1-9]')
}

function Get-VmState($Vm) { ((& $Vbm showvminfo $Vm --machinereadable) -match '^VMState=') -replace 'VMState=|"', '' }

# Ready means guestcontrol can run a command, not only that a guest property says so: after a
# restart inside the guest, the old properties can still be there for a while.
function Wait-GuestReady($Vm, [string] $User = 'tester', [int] $TimeoutMin = 15) {
  $deadline = (Get-Date).AddMinutes($TimeoutMin)
  while ((Get-Date) -lt $deadline) {
    if (Test-GuestReady $Vm) {
      $echo = Invoke-Guest $Vm 'C:\Windows\System32\cmd.exe' @('/c', 'echo', 'guest-ok') $User 60 2>$null
      if ("$echo" -match 'guest-ok') { return $true }
    }
    Start-Sleep -Seconds 10
  }
  $false
}

# Shuts the VM down cleanly, takes the "clean" snapshot Task 8 asks for, and starts it again.
# 'separate' starts it with a window the founder can watch; closing that window leaves the VM running.
function Save-Clean($Vm, [string] $User = 'tester', [ValidateSet('separate', 'headless')] [string] $StartType = 'separate') {
  # Windows 10 Home ignored the ACPI power button here, so shut down from inside Windows first.
  Invoke-Guest $Vm 'C:\Windows\System32\shutdown.exe' @('/s', '/t', '0') $User 60 2>$null | Out-Null
  Start-Sleep -Seconds 20
  if ((Get-VmState $Vm) -ne 'poweroff') { & $Vbm controlvm $Vm acpipowerbutton }
  $deadline = (Get-Date).AddMinutes(5)
  while ((Get-VmState $Vm) -ne 'poweroff' -and (Get-Date) -lt $deadline) { Start-Sleep -Seconds 5 }
  if ((Get-VmState $Vm) -ne 'poweroff') { throw "$Vm did not power off within 5 minutes" }
  & $Vbm snapshot $Vm take clean --description 'Fresh unattended install, before any probe'
  & $Vbm startvm $Vm --type $StartType
  Wait-GuestReady $Vm $User
}

# Win+R, the text, Enter: the same path a person takes, typed into the VM only.
function Send-Run($Vm, [string] $Text) {
  & $Vbm controlvm $Vm keyboardputscancode e0 5b 13 93 e0 db
  Start-Sleep -Seconds 2
  & $Vbm controlvm $Vm keyboardputstring $Text
  Start-Sleep -Milliseconds 500
  & $Vbm controlvm $Vm keyboardputscancode 1c 9c
}

# How many pixels of the screenshot have exactly this color: an objective "is the scheme on screen".
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
public static class PixelCount {
  public static int Of(string png, int r, int g, int b) {
    using (var bmp = new System.Drawing.Bitmap(png)) {
      var rect = new System.Drawing.Rectangle(0, 0, bmp.Width, bmp.Height);
      var data = bmp.LockBits(rect, System.Drawing.Imaging.ImageLockMode.ReadOnly, System.Drawing.Imaging.PixelFormat.Format32bppArgb);
      var bytes = new byte[data.Stride * bmp.Height];
      System.Runtime.InteropServices.Marshal.Copy(data.Scan0, bytes, 0, bytes.Length);
      bmp.UnlockBits(data);
      int n = 0;
      for (int i = 0; i + 3 < bytes.Length; i += 4) if (bytes[i] == b && bytes[i + 1] == g && bytes[i + 2] == r) n++;
      return n;
    }
  }
}
'@
function Measure-Color([string] $Png, [string] $Hex) {
  [PixelCount]::Of($Png, [Convert]::ToInt32($Hex.Substring(1, 2), 16), [Convert]::ToInt32($Hex.Substring(3, 2), 16), [Convert]::ToInt32($Hex.Substring(5, 2), 16))
}

function Save-Shot($Vm, [string] $Label) {
  New-Item -ItemType Directory -Force $Shots | Out-Null
  $file = Join-Path $Shots "$Vm-$Label.png"
  & $Vbm controlvm $Vm screenshotpng $file
  $file
}
