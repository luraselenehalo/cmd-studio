# Host-side helpers for the phase 3 VM check (protocol.md). Dot-source this after
# spikes\vm\vm.ps1, in Windows PowerShell 5.1 on the host, from the repo root:
#   . .\spikes\vm\vm.ps1
#   . .\docs\spikes\phase3-vm-check\helpers.ps1
# They call only vm.ps1's functions and VBoxManage, and only the CMD Studio test VMs get anything
# from them. The dev PC's own Windows Terminal, registry and fonts are never touched. Nothing here
# is needed by run.ps1.
#
# Two kinds of reading exist, and the difference matters on a Terminal that was never opened:
#   console-free  Invoke-GuestPs, Copy-ToGuest, wscript.exe with a .vbs, and the app itself.
#                 Started through guestcontrol, a program does not open Windows Terminal.
#   desktop       Invoke-GuestSnippet and the Get-Guest* functions built on it. They start a
#                 hidden PowerShell on the user's desktop through a scheduled task, which sees the
#                 user's environment and HKCU, but is a console program. Use it only once Terminal
#                 has been opened, or where the state of Terminal does not matter.

$P3Docs = $PSScriptRoot

# What VBoxManage prints from the guest is UTF-8 (Invoke-GuestPs asks the guest for it); the host
# would otherwise decode Thai as '?'. A host without a console cannot set this, and then ASCII
# still works.
try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch { }

# Where things are inside every VM. All ASCII and without spaces, so that Win+R can type them.
$P3 = @{
  App      = 'C:\cmdstudio\phase3\app'       # cmdstudio.exe and resources\ of the CI artifact
  Check    = 'C:\cmdstudio\phase3\check'     # the scripts of this folder
  Snippets = 'C:\cmdstudio\phase3\snippets'  # one-off scripts that Invoke-GuestSnippet copies in
  Out      = 'C:\cmdstudio\out\phase3'       # what the scripts write
}

# Scan codes (set 1, make then break) for controlvm keyboardputscancode. VBoxManage cannot move
# the mouse, so the app is driven with the keyboard.
$P3Keys = @{
  Tab = '0f 8f'; ShiftTab = '2a 0f 8f aa'; Enter = '1c 9c'; Space = '39 b9'; Esc = '01 81'
  Left = 'e0 4b e0 cb'; Right = 'e0 4d e0 cd'; Up = 'e0 48 e0 c8'; Down = 'e0 50 e0 d0'
  AltF4 = '38 3e be b8'; WinD = 'e0 5b 20 a0 e0 db'; WinSpace = 'e0 5b 39 b9 e0 db'
  CtrlA = '1d 1e 9e 9d'; CtrlV = '1d 2f af 9d'
}

# Restores the "clean" snapshot, starts the VM and waits until guestcontrol can run a command.
# 'separate' gives a window to watch; closing that window leaves the VM running.
function Start-CleanVm($Vm, [string] $User = 'tester', [ValidateSet('headless', 'separate')] [string] $Type = 'headless') {
  if ((Get-VmState $Vm) -ne 'poweroff') { & $Vbm controlvm $Vm poweroff; Start-Sleep -Seconds 5 }
  & $Vbm snapshot $Vm restore clean
  & $Vbm startvm $Vm --type $Type
  if (-not (Wait-GuestReady $Vm $User)) { throw "$Vm is not ready" }
}

# Puts the scripts of this folder and the CI artifact into the VM and compares the exe's hash.
# Console-free. $Artifact is the folder that holds cmdstudio.exe and resources\.
function Install-Phase3App($Vm, [string] $User, [string] $Artifact) {
  $repo = (Resolve-Path (Join-Path $P3Docs '..\..\..')).Path
  Invoke-GuestPs $Vm "New-Item -ItemType Directory -Force -Path '$($P3.Out)' | Out-Null" $User 60 | Out-Null
  # Invoke-GuestDesktop runs this file; a snapshot taken before any probe may not have it.
  Copy-ToGuest $Vm @((Join-Path $repo 'spikes\vm\guest\run-on-desktop.ps1')) 'C:\cmdstudio\spikes\vm\guest\' $User | Out-Null
  $scripts = 'wt-precreate.ps1', 'step0-wt-state.vbs', 'start-app.vbs', 'exit-code.vbs' | ForEach-Object { Join-Path $P3Docs $_ }
  Copy-ToGuest $Vm $scripts ($P3.Check + '\') $User | Out-Null
  Copy-ToGuest $Vm @((Join-Path $Artifact 'cmdstudio.exe'), (Join-Path $Artifact 'resources')) ($P3.App + '\') $User | Out-Null
  $hostHash = (Get-FileHash -LiteralPath (Join-Path $Artifact 'cmdstudio.exe') -Algorithm SHA256).Hash
  $inVm = "$(Invoke-GuestPs $Vm "(Get-FileHash -LiteralPath '$($P3.App)\cmdstudio.exe' -Algorithm SHA256).Hash" $User 180)"
  'exe sha256 on the host: ' + $hostHash
  'exe sha256 in the VM:   ' + $inVm.Trim()
  'same exe: ' + ($inVm -match $hostHash)
  'files in resources, host: ' + @(Get-ChildItem -LiteralPath (Join-Path $Artifact 'resources') -Recurse -File).Count
  'files in resources, VM:   ' + "$(Invoke-GuestPs $Vm "@(Get-ChildItem -LiteralPath '$($P3.App)\resources' -Recurse -File).Count" $User 180)".Trim()
}

function Send-Key($Vm, [string] $Name, [int] $Times = 1, [int] $PauseMs = 500) {
  if (-not $P3Keys.ContainsKey($Name)) { throw "unknown key '$Name'; known: $($P3Keys.Keys -join ', ')" }
  $codes = $P3Keys[$Name] -split ' '
  for ($i = 0; $i -lt $Times; $i++) {
    & $Vbm controlvm $Vm keyboardputscancode @codes
    Start-Sleep -Milliseconds $PauseMs
  }
}

# One argument of a command line, by the Win32 rules (CommandLineToArgvW): always quoted, the
# backslashes in front of a double quote doubled, and a quote itself escaped.
function ConvertTo-CommandLineArg([string] $Text) {
  '"' + (($Text -replace '(\\*)"', '$1$1\"') -replace '(\\+)$', '$1$1') + '"'
}

# Types a text into the VM. Windows PowerShell 5.1 does not escape double quotes inside an argument
# for a native program, so vm.ps1's Send-Run cannot type a text that has a quote in it; this builds
# the whole command line itself. -Enter presses Enter afterwards.
function Send-Text($Vm, [string] $Text, [switch] $Enter) {
  $line = 'controlvm {0} keyboardputstring {1}' -f (ConvertTo-CommandLineArg $Vm), (ConvertTo-CommandLineArg $Text)
  Start-Process -FilePath $Vbm -ArgumentList $line -NoNewWindow -Wait
  if ($Enter) { Start-Sleep -Milliseconds 400; Send-Key $Vm Enter }
}

# Runs PowerShell code on the signed-in user's desktop (their environment, their HKCU) and prints
# what it wrote. The code is copied in as a file, because Invoke-GuestDesktop runs files. The
# file is UTF-8 with a byte order mark, which Windows PowerShell 5.1 needs to read Thai in it.
# This is a desktop reading (see the top): a console program started on the desktop.
function Invoke-GuestSnippet($Vm, [string] $Code, [string] $User = 'tester', [int] $TimeoutSec = 180) {
  New-Item -ItemType Directory -Force $Shots | Out-Null
  $name = 'snippet-{0}.ps1' -f ([guid]::NewGuid().ToString('N').Substring(0, 8))
  $local = Join-Path $Shots $name
  $head = "if (`$env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw 'VM only' }`r`n[Console]::OutputEncoding = [Text.Encoding]::UTF8`r`n"
  [IO.File]::WriteAllText($local, $head + $Code, (New-Object Text.UTF8Encoding $true))
  Copy-ToGuest $Vm @($local) ($P3.Snippets + '\') $User | Out-Null
  Invoke-GuestDesktop $Vm ($P3.Snippets + '\' + $name) '' $User $TimeoutSec
  Remove-Item -LiteralPath $local -ErrorAction SilentlyContinue
}

# A text file in the VM, read through guestcontrol (console-free; no user environment, so give a
# full path). Files the vbs scripts write are UTF-16: pass 'Unicode'.
function Get-GuestText($Vm, [string] $Path, [string] $User = 'tester', [string] $Encoding = 'UTF8') {
  Invoke-GuestPs $Vm "if (Test-Path -LiteralPath '$Path') { Get-Content -LiteralPath '$Path' -Raw -Encoding $Encoding } else { '(no such file: $Path)' }" $User 120
}

# Waits until a file exists in the VM; true when it does. Console-free.
function Wait-GuestFile($Vm, [string] $Path, [string] $User = 'tester', [int] $TimeoutSec = 60) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    if ("$(Invoke-GuestPs $Vm "Test-Path -LiteralPath '$Path'" $User 60)" -match 'True') { return $true }
    Start-Sleep -Seconds 3
  }
  $false
}

# Runs cmdstudio.exe with these arguments the way an NSIS ExecWait does (exit-code.vbs under
# wscript.exe: hidden, no console, waiting), and returns the file it wrote: "exit=<code>" and the
# command that ran. Typed with Win+R, so it runs as the signed-in user and %NAME% in $ArgLine
# is expanded for that user. $ArgLine holds no double quote and no space: the script quotes
# every argument itself. Console-free.
function Get-ExitCode($Vm, [string] $Label, [string] $ArgLine, [string] $User = 'tester', [int] $WaitSec = 90) {
  $result = "$($P3.Out)\exit-$Label.txt"
  Invoke-GuestPs $Vm "New-Item -ItemType Directory -Force -Path '$($P3.Out)' | Out-Null; Remove-Item -LiteralPath '$result' -ErrorAction SilentlyContinue" $User 60 | Out-Null
  Send-Run $Vm ('wscript.exe //B //Nologo {0}\exit-code.vbs {1} {2}\cmdstudio.exe {3}' -f $P3.Check, $result, $P3.App, $ArgLine)
  if (Wait-GuestFile $Vm $result $User $WaitSec) { Get-GuestText $Vm $result $User 'Unicode' } else { "(no result file after $WaitSec s: $result)" }
}

# Which of these processes run, and since when. Console-free.
function Get-GuestProcesses($Vm, [string] $User = 'tester') {
  $code = @'
Get-Process cmdstudio, msedgewebview2, WindowsTerminal, OpenConsole, cmd -ErrorAction SilentlyContinue |
  ForEach-Object { '{0} pid {1} started {2:s}' -f $_.ProcessName, $_.Id, $_.StartTime }
'done'
'@
  Invoke-GuestPs $Vm $code $User 120
}

# The facts an undo must put back: AutoRun, Windows Terminal's settings.json (its hash) and
# LocalState, the user's fonts, and the data folder's top level. Take it before an apply and after
# an undo; the lines that do not start with "data" must be equal. With -NoConsole it is read
# through guestcontrol from C:\Users\<user>\AppData\Local, which cannot see HKCU, so the AutoRun
# and font registrations are not read.
function Get-GuestFacts($Vm, [string] $User = 'tester', [switch] $NoConsole) {
  $code = @'
$local = __LOCAL__
$readReg = __READREG__
$data = Join-Path $local 'ai.resincore.cmdstudio'
$pkg = Join-Path $local 'Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe'
$state = Join-Path $pkg 'LocalState'
$wtFile = Join-Path $state 'settings.json'
if ($readReg) {
  $run = (Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Command Processor' -ErrorAction SilentlyContinue).AutoRun
  'AutoRun: ' + $(if ($null -eq $run) { '(absent)' } else { $run })
} else { 'AutoRun: (not read in console-free mode)' }
'WT package folder: ' + (Test-Path -LiteralPath $pkg)
'WT LocalState: ' + (Test-Path -LiteralPath $state)
if (Test-Path -LiteralPath $wtFile) {
  'WT settings.json: {0} bytes, sha256 {1}' -f (Get-Item -LiteralPath $wtFile).Length, (Get-FileHash -LiteralPath $wtFile -Algorithm SHA256).Hash
} else { 'WT settings.json: (absent)' }
if (Test-Path -LiteralPath $state) {
  Get-ChildItem -LiteralPath $state -Force | ForEach-Object { 'WT LocalState entry: {0} {1}' -f $_.Name, $_.Length }
}
$fonts = Join-Path $local 'Microsoft\Windows\Fonts'
'user fonts folder: ' + $(if (Test-Path -LiteralPath $fonts) { (Get-ChildItem -LiteralPath $fonts -Force | ForEach-Object { $_.Name }) -join ', ' } else { '(absent)' })
if ($readReg) {
  $reg = Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts' -ErrorAction SilentlyContinue
  'user font registrations: ' + $(if ($reg) { ($reg.PSObject.Properties | Where-Object { $_.Name -notlike 'PS*' } | ForEach-Object { $_.Name + '=' + $_.Value }) -join ', ' } else { '(none)' })
}
'data folder: ' + (Test-Path -LiteralPath $data)
if (Test-Path -LiteralPath $data) {
  Get-ChildItem -LiteralPath $data -Force | ForEach-Object { 'data entry: {0} {1}' -f $_.Name, $(if ($_.PSIsContainer) { 'dir' } else { $_.Length }) }
}
'@
  if ($NoConsole) {
    $code = $code.Replace('__LOCAL__', "'C:\Users\$User\AppData\Local'").Replace('__READREG__', '$false')
    Invoke-GuestPs $Vm $code $User 180
  } else {
    $code = $code.Replace('__LOCAL__', "[Environment]::GetFolderPath('LocalApplicationData')").Replace('__READREG__', '$true')
    Invoke-GuestSnippet $Vm $code $User
  }
}

# From the process start of cmdstudio.exe to the write of settings.json, which the wizard makes
# when the apply commits (the page "Done" is up within a second of it). Both read in the guest.
# With -NoConsole it is read through guestcontrol (the process list does not need the user's
# environment; the file is found from the user's name).
function Get-GuestTiming($Vm, [string] $User = 'tester', [switch] $NoConsole) {
  $code = @'
$p = Get-Process cmdstudio -ErrorAction SilentlyContinue | Select-Object -First 1
$s = Join-Path (__LOCAL__) 'ai.resincore.cmdstudio\settings.json'
if ($p -and (Test-Path -LiteralPath $s)) {
  $t0 = $p.StartTime.ToUniversalTime()
  $t1 = (Get-Item -LiteralPath $s).LastWriteTimeUtc
  'process started (UTC): {0:o}' -f $t0
  'settings.json written (UTC): {0:o}' -f $t1
  'seconds from the first window to Done: {0:N1}' -f ($t1 - $t0).TotalSeconds
} else { 'no cmdstudio.exe running, or no settings.json yet' }
'@
  if ($NoConsole) {
    $code = $code.Replace('__LOCAL__', "'C:\Users\$User\AppData\Local'")
    Invoke-GuestPs $Vm $code $User 120
  } else {
    $code = $code.Replace('__LOCAL__', "[Environment]::GetFolderPath('LocalApplicationData')")
    Invoke-GuestSnippet $Vm $code $User
  }
}

# The windows on the desktop that have a title, which window is in front and whether it is
# minimized, and how many cmdstudio.exe processes run. For "no console window appeared" and for
# "a second start focuses the first window". A desktop reading.
function Get-GuestWindows($Vm, [string] $User = 'tester') {
  $code = @'
Add-Type -Name Win -Namespace P3 -MemberDefinition '[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid); [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);'
$fg = [P3.Win]::GetForegroundWindow()
$fgPid = [uint32]0
[void][P3.Win]::GetWindowThreadProcessId($fg, [ref]$fgPid)
'foreground: pid {0} {1}, minimized: {2}' -f $fgPid, (Get-Process -Id $fgPid -ErrorAction SilentlyContinue).ProcessName, [P3.Win]::IsIconic($fg)
Get-Process | Where-Object { $_.MainWindowHandle -ne 0 } | ForEach-Object { 'window: {0} (pid {1}) "{2}"' -f $_.ProcessName, $_.Id, $_.MainWindowTitle }
'cmdstudio.exe processes: ' + @(Get-Process cmdstudio -ErrorAction SilentlyContinue).Count
'WindowsTerminal.exe processes: ' + @(Get-Process WindowsTerminal -ErrorAction SilentlyContinue).Count
'console host processes with a window (conhost, cmd, OpenConsole): ' + @(Get-Process conhost, cmd, OpenConsole -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 }).Count
'@
  Invoke-GuestSnippet $Vm $code $User
}

# Every cmd.exe that runs, and whether Clink's DLL is loaded in it: the proof that AutoRun (or
# the Terminal profile's command line) injected Clink into that window. A desktop reading.
function Get-GuestCmdWindows($Vm, [string] $User = 'tester') {
  $code = @'
$all = @(Get-Process cmd -ErrorAction SilentlyContinue)
'cmd.exe processes: ' + $all.Count
foreach ($p in $all) {
  $loaded = try { @($p.Modules | Where-Object { $_.ModuleName -like 'clink*' } | ForEach-Object { $_.ModuleName }) -join ',' } catch { '(modules not readable)' }
  'cmd pid {0} started {1:T}, Clink loaded: {2}' -f $p.Id, $p.StartTime, $(if ($loaded) { $loaded } else { 'no' })
}
'@
  Invoke-GuestSnippet $Vm $code $User
}

# What the clipboard holds after "Copy details" in Help, Report a problem: tested against the
# signed-in user's name, the 8.3 name of their profile folder and their SID, and saved to
# Out\clipboard.txt. A desktop reading (the clipboard is the user's).
function Get-GuestClipboardReport($Vm, [string] $User = 'tester') {
  $code = @'
$text = Get-Clipboard -Raw
if ([string]::IsNullOrEmpty($text)) { 'clipboard: empty'; return }
$short = (New-Object -ComObject Scripting.FileSystemObject).GetFolder($env:USERPROFILE).ShortName
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
New-Item -ItemType Directory -Force -Path 'C:\cmdstudio\out\phase3' | Out-Null
[IO.File]::WriteAllText('C:\cmdstudio\out\phase3\clipboard.txt', $text, (New-Object Text.UTF8Encoding $false))
'clipboard: {0} characters, saved to C:\cmdstudio\out\phase3\clipboard.txt' -f $text.Length
'%USERNAME% present: ' + $text.Contains('%USERNAME%')
'user name present: ' + ($text.IndexOf($env:USERNAME, [StringComparison]::OrdinalIgnoreCase) -ge 0)
'8.3 name of the profile folder (' + $short + ') present: ' + ($text.IndexOf($short, [StringComparison]::OrdinalIgnoreCase) -ge 0)
'this account SID present: ' + ($text.IndexOf($sid, [StringComparison]::OrdinalIgnoreCase) -ge 0)
'any account SID present: ' + ($text -match 'S-1-5-21-\d+-\d+-\d+-\d+')
'<SID> present: ' + $text.Contains('<SID>')
'@
  Invoke-GuestSnippet $Vm $code $User
}

# Which control has the keyboard focus, by name, through UI Automation. Best effort: it is how
# a screen reader sees WebView2, but it has not been run against this app. The first call may see
# only the window; ask again after a second. The screenshot is the fallback. A desktop reading.
function Get-GuestFocus($Vm, [string] $User = 'tester') {
  $code = @'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$f = [System.Windows.Automation.AutomationElement]::FocusedElement
if ($f) { '{0} | "{1}" | pid {2}' -f $f.Current.ControlType.ProgrammaticName, $f.Current.Name, $f.Current.ProcessId } else { 'no focused element' }
'@
  Invoke-GuestSnippet $Vm $code $User
}

# Presses the control with this name in the app's window through UI Automation's Invoke, which is
# one activation, as a click is. Best effort, like Get-GuestFocus. The name is the button's text
# in the dictionaries (app/ui/src/i18n/en.ts, th.ts). A desktop reading.
function Invoke-GuestButton($Vm, [string] $Name, [string] $User = 'tester') {
  $code = @'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$ae = [System.Windows.Automation.AutomationElement]
$scope = [System.Windows.Automation.TreeScope]
$app = Get-Process cmdstudio -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $app) { 'cmdstudio.exe is not running'; return }
$win = $ae::RootElement.FindFirst($scope::Children, (New-Object System.Windows.Automation.PropertyCondition($ae::ProcessIdProperty, [int]$app.Id)))
if (-not $win) { 'no top-level window of cmdstudio.exe'; return }
$canInvoke = New-Object System.Windows.Automation.PropertyCondition($ae::IsInvokePatternAvailableProperty, $true)
$named = New-Object System.Windows.Automation.PropertyCondition($ae::NameProperty, '__NAME__')
$hits = $win.FindAll($scope::Descendants, (New-Object System.Windows.Automation.AndCondition($canInvoke, $named)))
'buttons named "__NAME__": ' + $hits.Count
if ($hits.Count -ge 1) { $hits[0].GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); 'invoked' }
'@
  Invoke-GuestSnippet $Vm $code.Replace('__NAME__', $Name.Replace("'", "''")) $User
}

# Pixel counts for a screenshot. vm.ps1's Measure-Color counts one exact color. A window with
# opacity or acrylic does not paint its scheme's background exactly (Terracotta sets opacity 96
# and acrylic), so Measure-ColorNear counts the pixels whose channels are each within $Tolerance
# of the color. Get-ColorColumns lists the columns of the screenshot that hold a vertical run of
# at least $MinRun pixels of one color: the left and right edges of a box drawn in the frame color.
if (-not ('PixelNear' -as [type])) {
  Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
public static class PixelNear {
  static byte[] Load(string png, out int w, out int h, out int stride) {
    using (var bmp = new System.Drawing.Bitmap(png)) {
      w = bmp.Width; h = bmp.Height;
      var rect = new System.Drawing.Rectangle(0, 0, w, h);
      var data = bmp.LockBits(rect, System.Drawing.Imaging.ImageLockMode.ReadOnly, System.Drawing.Imaging.PixelFormat.Format32bppArgb);
      stride = data.Stride;
      var bytes = new byte[data.Stride * h];
      System.Runtime.InteropServices.Marshal.Copy(data.Scan0, bytes, 0, bytes.Length);
      bmp.UnlockBits(data);
      return bytes;
    }
  }
  static bool Near(byte[] p, int i, int r, int g, int b, int tol) {
    return System.Math.Abs(p[i] - b) <= tol && System.Math.Abs(p[i + 1] - g) <= tol && System.Math.Abs(p[i + 2] - r) <= tol;
  }
  public static int Count(string png, int r, int g, int b, int tol) {
    int w, h, stride;
    byte[] p = Load(png, out w, out h, out stride);
    int n = 0;
    for (int y = 0; y < h; y++)
      for (int x = 0; x < w; x++)
        if (Near(p, y * stride + x * 4, r, g, b, tol)) n++;
    return n;
  }
  // For each column with a run of at least minRun pixels: x, the row the run starts in, its length.
  public static int[][] Columns(string png, int r, int g, int b, int tol, int minRun) {
    int w, h, stride;
    byte[] p = Load(png, out w, out h, out stride);
    var found = new System.Collections.Generic.List<int[]>();
    for (int x = 0; x < w; x++) {
      int run = 0, best = 0, bestEnd = 0;
      for (int y = 0; y < h; y++) {
        if (Near(p, y * stride + x * 4, r, g, b, tol)) { run++; if (run > best) { best = run; bestEnd = y; } } else { run = 0; }
      }
      if (best >= minRun) found.Add(new int[] { x, bestEnd - best + 1, best });
    }
    return found.ToArray();
  }
}
'@
}
function ConvertFrom-Hex([string] $Hex) {
  @([Convert]::ToInt32($Hex.Substring(1, 2), 16), [Convert]::ToInt32($Hex.Substring(3, 2), 16), [Convert]::ToInt32($Hex.Substring(5, 2), 16))
}
function Measure-ColorNear([string] $Png, [string] $Hex, [int] $Tolerance = 12) {
  $c = ConvertFrom-Hex $Hex
  [PixelNear]::Count($Png, $c[0], $c[1], $c[2], $Tolerance)
}
function Get-ColorColumns([string] $Png, [string] $Hex, [int] $Tolerance = 0, [int] $MinRun = 40) {
  $c = ConvertFrom-Hex $Hex
  foreach ($col in [PixelNear]::Columns($Png, $c[0], $c[1], $c[2], $Tolerance, $MinRun)) {
    [pscustomobject]@{ X = $col[0]; Y = $col[1]; Length = $col[2] }
  }
}
