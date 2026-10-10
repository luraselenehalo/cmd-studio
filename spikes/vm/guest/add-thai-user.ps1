# Runs inside a test VM, elevated (started through UAC on the VM's own screen). Adds the local
# administrator "ผู้ทดสอบ ไทย" (a Thai name with a space), switches auto-logon to it, and restarts.
# On English Windows (code page 1252) this is the Thai-name-on-English-Windows case for G19.
# The password comes from C:\cmdstudio\new-user.password, copied in by the host; the file is deleted here.
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script adds a user and changes auto-logon" }
$log = 'C:\cmdstudio\add-thai-user.log'
Start-Transcript -Path $log -Append | Out-Null
try {
  $name = 'ผู้ทดสอบ ไทย'
  $pwFile = 'C:\cmdstudio\new-user.password'
  $pw = (Get-Content -LiteralPath $pwFile -Raw).Trim()
  New-LocalUser -Name $name -FullName $name -Password (ConvertTo-SecureString $pw -AsPlainText -Force) -PasswordNeverExpires | Out-Null
  Add-LocalGroupMember -SID 'S-1-5-32-544' -Member $name
  $w = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
  Set-ItemProperty $w AutoAdminLogon '1'
  Set-ItemProperty $w DefaultUserName $name
  Set-ItemProperty $w DefaultPassword $pw
  Set-ItemProperty $w DefaultDomainName '.'
  Remove-Item -LiteralPath $pwFile
  "user added: $((Get-LocalUser -Name $name).Name), ANSI code page $([Text.Encoding]::Default.CodePage)"
} finally {
  Stop-Transcript | Out-Null
}
shutdown.exe /r /t 5
