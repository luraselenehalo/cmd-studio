# Checks that autorun.ps1 puts AutoRun back exactly as it found it: the same text, unexpanded, and the
# same value type. Get-ItemProperty expands REG_EXPAND_SZ, so writing its result back would change both.
# Runs autorun.ps1's own Save-Value / Restore-Value against a scratch key, never Command Processor.
$ErrorActionPreference = 'Stop'
$ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'autorun.ps1'), [ref]$null, [ref]$null)
$ast.FindAll({ $args[0] -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $false) |
  Where-Object Name -in 'Save-Value', 'Restore-Value' | ForEach-Object { . ([scriptblock]::Create($_.Extent.Text)) }
if (-not (Get-Command Save-Value, Restore-Value -ErrorAction SilentlyContinue)) { throw 'FAIL: autorun.ps1 has no Save-Value / Restore-Value' }

$scratch = 'HKCU:\Software\CmdStudioSpikeRestoreTest'
if (-not (Test-Path $scratch)) { New-Item -Path $scratch | Out-Null }
$fail = 0
function Expect($what, $ok) { if ($ok) { "pass: $what" } else { "FAIL: $what"; $script:fail++ } }
try {
  $k = Get-Item $scratch
  New-ItemProperty $scratch AutoRun -Value '%SystemRoot%\probe.cmd & echo two' -PropertyType ExpandString -Force | Out-Null
  $saved = Save-Value $scratch 'AutoRun'
  Set-ItemProperty $scratch AutoRun 'changed by the probe'
  Restore-Value $scratch 'AutoRun' $saved
  $k = Get-Item $scratch
  Expect 'REG_EXPAND_SZ keeps its type' ($k.GetValueKind('AutoRun') -eq 'ExpandString')
  Expect 'REG_EXPAND_SZ keeps its unexpanded text' ($k.GetValue('AutoRun', $null, 'DoNotExpandEnvironmentNames') -eq '%SystemRoot%\probe.cmd & echo two')

  New-ItemProperty $scratch AutoRun -Value 'echo one & "C:\a b\c.cmd"' -PropertyType String -Force | Out-Null
  $saved = Save-Value $scratch 'AutoRun'
  Remove-ItemProperty $scratch AutoRun
  Restore-Value $scratch 'AutoRun' $saved
  $k = Get-Item $scratch
  Expect 'REG_SZ comes back with its exact text' ($k.GetValueKind('AutoRun') -eq 'String' -and $k.GetValue('AutoRun') -eq 'echo one & "C:\a b\c.cmd"')

  Remove-ItemProperty $scratch AutoRun
  $saved = Save-Value $scratch 'AutoRun'
  Set-ItemProperty $scratch AutoRun 'added by the probe'
  Restore-Value $scratch 'AutoRun' $saved
  Expect 'a value that was absent is absent again' ((Get-Item $scratch).GetValueNames() -notcontains 'AutoRun')
} finally {
  Remove-Item -LiteralPath 'HKCU:\Software\CmdStudioSpikeRestoreTest' -Recurse
}
if ($fail) { throw "autorun restore: $fail check(s) failed" }
'autorun restore: 4/4 checks pass'
