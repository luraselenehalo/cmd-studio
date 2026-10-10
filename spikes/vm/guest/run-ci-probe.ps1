# Runs a CI probe from spikes/windows inside a test VM. The probes expect GitHub's RUNNER_TEMP, and
# Windows PowerShell 5.1 on a client may refuse Invoke-WebRequest without basic parsing.
param([Parameter(Mandatory)] [ValidateSet('fonts', 'autorun')] [string] $Probe)
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and the probes change AutoRun and fonts (on CI, run spikes/windows directly)" }
$env:RUNNER_TEMP = $env:TEMP
$PSDefaultParameterValues['Invoke-WebRequest:UseBasicParsing'] = $true
& (Join-Path $PSScriptRoot "..\..\windows\$Probe.ps1")
