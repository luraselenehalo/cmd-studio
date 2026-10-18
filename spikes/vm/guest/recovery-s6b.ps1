# Runs inside a test VM as the test user, on the signed-in user's desktop (Invoke-GuestDesktop, for
# every switch below: -Run needs the desktop, and the user's own HKCU and LOCALAPPDATA are the
# ones that count). Phase 3b-1 spike round 2, S6b: a coroutine registered at the first edit line
# (clink.addcoroutine from clink.onbeginedit, and the documented clink.promptcoroutine from a
# prompt filter, both in the same window) must run only after every prompt filter, Oh My Posh's
# included, has returned and the prompt is drawn, with no key pressed; with a filter that hangs it
# must not run at all. spikes\recovery\probe-s6b.lua writes the lines this script prints, among
# them "omp filter start/end" for Oh My Posh's own filter calls.
#   -Setup            lays out the same profile as recovery-setup.ps1 (Clink 1.9.34, Oh My Posh 31.5.0
#                     through omp-init.lua, the welcome box, CMD Studio's guarded AutoRun) by calling
#                     it with -NoProbe, then loads probe-s6b.lua as aa-probe.lua in its place. The
#                     timing marker (zz-timing.lua) is left out so the filter chain is the app's own.
#   -Run <n> [-Hang [-HangKind loop|popen]] [-WaitSeconds <s>]
#                     opens <n> cmd windows one after another with Start-Process cmd.exe (no /d:
#                     AutoRun is what is tested), waits 30 s for each (50 s with -Hang; -WaitSeconds
#                     overrides, e.g. 10 to close a window while its filter still hangs), closes it,
#                     then removes the hang file. With -Hang the file <profile>\p3b2-hang exists for
#                     the whole run, and the probe's filter at priority 0 holds the thread for 20 s in
#                     each window before it returns: -HangKind loop (default) spins on os.clock(),
#                     popen waits on io.popen("ping -n 21 127.0.0.1 >nul"), as Oh My Posh waits on its
#                     own child. Without -Hang a hang file left by an earlier run is removed first.
#   -Report           prints every p3b2 line of the profile's clink.log (and the "cmdstudio:" lines
#                     of init.lua beside them: "prompt loaded" shows that Oh My Posh's filter is in
#                     the chain), then <profile>\p3b2.txt, which the probe writes with io.open for
#                     every line (a window that is killed may leave clink.log short), then the
#                     windows -Run opened. Changes nothing.
# recovery-setup.ps1 -Remove puts AutoRun back when the spike is over.
param([switch] $Setup, [int] $Run = 0, [switch] $Hang, [ValidateSet('loop', 'popen')] [string] $HangKind = 'loop', [int] $WaitSeconds = 0, [switch] $Report)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM, and this script rewrites the user's cmd AutoRun and opens cmd windows" }
if (-not ($Setup -or $Run -gt 0 -or $Report)) { throw 'give -Setup, -Run <n> [-Hang [-HangKind loop|popen]] [-WaitSeconds <s>] or -Report' }
if (($Hang -or $WaitSeconds -gt 0) -and $Run -le 0) { throw '-Hang and -WaitSeconds go with -Run <n>' }
if ($PSBoundParameters.ContainsKey('HangKind') -and -not $Hang) { throw '-HangKind goes with -Hang' }
try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch { }
# The layout of recovery-setup.ps1: its data folder, Clink profile, and the way it starts Clink.
$data = Join-Path $env:LOCALAPPDATA 'ai.resincore.cmdstudio-spike3b'
$gen = Join-Path $data 'generated'
$profileDir = Join-Path $data 'clink-profile'
$log = Join-Path $profileDir 'clink.log'
$mirror = Join-Path $profileDir 'p3b2.txt'
$hangFile = Join-Path $profileDir 'p3b2-hang'
$windows = Join-Path $profileDir 'p3b2-windows.txt'
# Clink prints a Lua run-time error with puts, on stdout, and a native program's stderr line would
# end this script under 'Stop': so every line comes back as a string, with 'Continue' inside.
function Clink-Set([string[]] $Words) {
  $ErrorActionPreference = 'Continue'
  & $launch --profile $profileDir set @Words 2>&1 | ForEach-Object { "$_" }
}
# The lines of a file that match, or why they could not be read.
function Show-Lines([string] $Path, [string] $Pattern) {
  if (-not (Test-Path -LiteralPath $Path)) { '(none)'; return }
  $err = $null
  $lines = @(Get-Content -LiteralPath $Path -Encoding UTF8 -ErrorAction SilentlyContinue -ErrorVariable err | Where-Object { $_ -match $Pattern })
  if ($err.Count) { "  error: $($err[0].FullyQualifiedErrorId): $($err[0].Exception.Message)" }
  if ($lines.Count) { $lines } elseif (-not $err.Count) { '(no matching line)' }
}

if ($Setup) {
  $here = if ($PSScriptRoot) { $PSScriptRoot } else { 'C:\cmdstudio\spikes\vm\guest' }
  & (Join-Path $here 'recovery-setup.ps1') -NoProbe
  # Clink starts through the 8.3 path of the data folder when the long one is not ASCII (G19).
  $exe = Join-Path $data 'runtime\clink-1.9.34\clink_x64.exe'
  $launch = $exe
  if ($exe -notmatch '^[\x00-\x7F]*$') {
    $short = (New-Object -ComObject Scripting.FileSystemObject).GetFolder($data).ShortPath
    $launch = Join-Path $short 'runtime\clink-1.9.34\clink_x64.exe'
  }
  Remove-Item -LiteralPath (Join-Path $gen 'zz-timing.lua') -ErrorAction SilentlyContinue
  $utf8 = New-Object Text.UTF8Encoding $false
  $text = "local p3b2_profile = [==[$profileDir]==]`n" + [IO.File]::ReadAllText('C:\cmdstudio\spikes\recovery\probe-s6b.lua')
  [IO.File]::WriteAllText((Join-Path $gen 'aa-probe.lua'), $text, $utf8)
  # Clink's own program loads the probe too: its load lines are printed there, and so is a Lua error.
  'clink set said:'
  Clink-Set @('clink.logo')
  # What that run wrote is not a cmd window's: -Report starts from nothing.
  Remove-Item -LiteralPath $log, $mirror, $windows, $hangFile -ErrorAction SilentlyContinue
  "probe: $(Join-Path $gen 'aa-probe.lua')"
  "profile: $profileDir"
}

if ($Run -gt 0) {
  # The probe on clink.path must be this one (a later recovery-setup.ps1 writes another or none).
  if (-not (Select-String -LiteralPath (Join-Path $gen 'aa-probe.lua') -SimpleMatch 'p3b2_profile' -Quiet -ErrorAction SilentlyContinue)) { throw 'the S6b probe is not on clink.path: run -Setup first' }
  $seconds = if ($WaitSeconds -gt 0) { $WaitSeconds } elseif ($Hang) { 50 } else { 30 }
  # A hang file left by an earlier run (one that ended before its cleanup) would hang these windows.
  if (-not $Hang) { Remove-Item -LiteralPath $hangFile -ErrorAction SilentlyContinue }
  try {
    if ($Hang) {
      [IO.File]::WriteAllText($hangFile, "$HangKind`n")
      "hang file written ($HangKind): $hangFile"
    }
    foreach ($i in 1..$Run) {
      $p = Start-Process -FilePath "$env:SystemRoot\System32\cmd.exe" -PassThru
      $opened = Get-Date
      Start-Sleep -Seconds $seconds
      $how = if ($p.HasExited) { "exited by itself (code $($p.ExitCode))" } else { 'closed by the script' }
      Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
      $line = "window $i pid=$($p.Id) hang=$(if ($Hang) { $HangKind } else { 'none' }) opened=$($opened.ToString('HH:mm:ss')) waited=${seconds}s $how"
      $line
      Add-Content -LiteralPath $windows -Value $line -Encoding ASCII
      Start-Sleep -Seconds 2
    }
  } finally {
    if ($Hang) {
      Remove-Item -LiteralPath $hangFile -ErrorAction SilentlyContinue
      'hang file removed'
    }
  }
}

if ($Report) {
  '--- clink.log: p3b2 lines (and init.lua''s cmdstudio: lines)'
  Show-Lines $log 'p3b2 |cmdstudio: '
  '--- p3b2.txt (written by io.open, line by line)'
  Show-Lines $mirror 'p3b2 '
  '--- windows opened by -Run'
  Show-Lines $windows '.'
  "--- hang file now: $(if (Test-Path -LiteralPath $hangFile) { 'present' } else { 'absent' })"
}
