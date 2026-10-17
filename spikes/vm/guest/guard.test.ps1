# Checks the safety rules of the guest scripts, without a VM and without touching this machine:
#  1. every guest script that changes the user's settings starts with the test-VM guard, and the guard
#     really refuses (run only once the guard is there, so an old unguarded script is never run here);
#  2. wt-cmd-scheme.ps1 keeps the first backup of settings.json when -Setup runs twice.
# Scripts that run do so in a child PowerShell with LOCALAPPDATA pointed at a temp folder.
$ErrorActionPreference = 'Stop'
$fail = 0
function Expect($what, $ok) { if ($ok) { "pass: $what" } else { "FAIL: $what"; $script:fail++ } }
# Each script with the arguments it needs to get past parameter binding to its first statement.
$changers = [ordered]@{
  'wt-cmd-scheme.ps1' = @(); 'proto-setup.ps1' = @(); 'thai-clink-workaround.ps1' = @('-Mode', 'short')
  'font-persist.ps1' = @(); 'thai-prefs.ps1' = @(); 'run-ci-probe.ps1' = @('-Probe', 'fonts')
  'add-thai-user.ps1' = @(); 'phase1b-check.ps1' = @('-Step', 'Snapshot')
  'recovery-setup.ps1' = @(); 'recovery-crash.ps1' = @('-Swap'); 'recovery-timing.ps1' = @(); 'recovery-cmdc.ps1' = @()
}

function Run-Guest($script, [string[]] $argv, $computer, $localAppData) {
  # The child's stderr comes back as error records in Windows PowerShell 5.1; 'Stop' would end the test.
  $ErrorActionPreference = 'Continue'
  $saved = $env:COMPUTERNAME, $env:LOCALAPPDATA
  $env:COMPUTERNAME = $computer; $env:LOCALAPPDATA = $localAppData
  try { $out = & powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot $script) @argv 2>&1 | Out-String; @{ Exit = $LASTEXITCODE; Out = $out } }
  finally { $env:COMPUTERNAME, $env:LOCALAPPDATA = $saved }
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("cmdstudio-guard-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory $tmp | Out-Null
try {
  foreach ($s in $changers.Keys) {
    $ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $s), [ref]$null, [ref]$null)
    $first = $ast.EndBlock.Statements | Select-Object -First 3 | ForEach-Object { $_.Extent.Text }
    $guarded = ($first -join "`n") -match "COMPUTERNAME -notlike 'CMDSTUDIO-\*'\) \{ throw"
    Expect "$s starts with the test-VM guard" $guarded
    if ($guarded) {
      $r = Run-Guest $s $changers[$s] 'NOT-A-TEST-VM' $tmp
      Expect "$s refuses to run on a machine that is not a test VM" ($r.Exit -ne 0 -and $r.Out -match 'VM only')
    }
  }
  Expect 'nothing was written while refusing' (@(Get-ChildItem $tmp -Recurse -Force).Count -eq 0)

  $wt = Join-Path $tmp 'Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState'
  New-Item -ItemType Directory -Force $wt | Out-Null
  Set-Content -LiteralPath (Join-Path $wt 'settings.json') -Value '{ "the user''s": "original" }' -Encoding UTF8
  $r1 = Run-Guest 'wt-cmd-scheme.ps1' @('-Setup') 'CMDSTUDIO-TEST' $tmp
  $r2 = Run-Guest 'wt-cmd-scheme.ps1' @('-Setup') 'CMDSTUDIO-TEST' $tmp
  $backup = Get-Content -LiteralPath (Join-Path $wt 'settings.json.before-cmdstudio') -Raw -ErrorAction SilentlyContinue
  Expect 'wt-cmd-scheme.ps1 -Setup ran twice inside a test VM' ($r1.Exit -eq 0 -and $r2.Exit -eq 0)
  Expect 'the backup still holds the original settings.json after the second -Setup' ($backup -match 'original')
} finally {
  Remove-Item -LiteralPath $tmp -Recurse -Force
}
$total = 2 * $changers.Count + 3
if ($fail) { throw "guest guards: $fail of $total check(s) failed" }
"guest guards: $total/$total checks pass"
