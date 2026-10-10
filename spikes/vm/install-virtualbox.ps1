# Installs VirtualBox for the Phase 0 test VMs (Task 8).
# Run it through Windows Terminal (wt -w new powershell -File ...), not from the Claude app's shell:
# that shell is MSIX-virtualized, and the installer's temp files would land where msiserver can't see them.
#  - Only the main program (ADDLOCAL=VBoxApplication): no bridged or host-only network drivers,
#    so the host's network does not drop during install. NAT needs no host driver.
#  - --no-silent-cert: silent mode would otherwise add Oracle's certificate to the machine's
#    TrustedPublisher store (VBoxStub.cpp, InstallCertificates).
$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:USERPROFILE 'VMs\dl-virtualbox'
$exe = Join-Path $dir 'VirtualBox-7.2.20-175154-Win.exe'
$sha = 'A81777D2B36380CE042A29E9C554CF032EB46A793F62E3CC82E7411E535C2C26'
$result = Join-Path $dir 'install-result.txt'
function Done($line) { $line; Set-Content -LiteralPath $result -Value $line -Encoding UTF8; Start-Sleep -Seconds 5; exit }

if ((Get-FileHash $exe -Algorithm SHA256).Hash -ne $sha) { Done 'stop: installer hash does not match the published SHA256' }
'Installing VirtualBox 7.2.20. Approve the UAC prompt.'
$argv = @('--silent', '--no-silent-cert', '--ignore-reboot',
  '--msi-log-file', (Join-Path $dir 'install-msi.log'),
  '--msiparams', 'ADDLOCAL=VBoxApplication',
  '--msiparams', 'VBOX_INSTALLDESKTOPSHORTCUT=0',
  '--msiparams', 'VBOX_INSTALLQUICKLAUNCHSHORTCUT=0',
  '--msiparams', 'VBOX_REGISTERFILEEXTENSIONS=0',
  '--msiparams', 'VBOX_START=0')
try { $p = Start-Process -FilePath $exe -ArgumentList $argv -Verb RunAs -Wait -PassThru }
catch { Done "stop: $($_.Exception.Message)" }
$vbm = 'C:\Program Files\Oracle\VirtualBox\VBoxManage.exe'
$ver = if (Test-Path $vbm) { & $vbm --version } else { 'VBoxManage missing' }
Done "exit=$($p.ExitCode) vboxmanage=$ver"
