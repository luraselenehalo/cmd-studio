# Guest side of the Windows Terminal pre-check, for a CMD Studio test VM only: does Windows
# Terminal, started for the first time, use a settings.json that was written before it?
# This is the question spec section 5.4 still marks as unconfirmed ("a Terminal that was never
# opened reads the file CMD Studio made"). The file written here is hand-made, so the answer
# does not depend on CMD Studio. run.ps1 on the host drives the steps in this order:
#   state   what is there now (the package folder, LocalState, settings.json, the wt.exe alias)
#   write   settings.json with a "CMD Studio" scheme (background #1F1E1D) on the cmd profile;
#           refuses to run when a settings.json exists, because then the Terminal was opened
#   start   start Windows Terminal once
#   read    print settings.json as Windows Terminal left it
#   close   close Windows Terminal
# Run it on the user's desktop (Invoke-GuestDesktop), after step 0 of protocol.md has recorded
# the snapshot's state without starting a console program.
param([Parameter(Mandatory)][ValidateSet('state', 'write', 'start', 'read', 'close')] [string] $Step)
$ErrorActionPreference = 'Stop'
if ($env:COMPUTERNAME -notlike 'CMDSTUDIO-*') { throw "VM only: $env:COMPUTERNAME is not a CMD Studio test VM" }
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$dir = Join-Path $env:LOCALAPPDATA 'Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState'
$file = Join-Path $dir 'settings.json'
$wt = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\wt.exe'
switch ($Step) {
  'state' {
    "package folder: $(Test-Path -LiteralPath (Split-Path $dir))"
    "LocalState: $(Test-Path -LiteralPath $dir)"
    "settings.json: $(Test-Path -LiteralPath $file)"
    "wt.exe: $(Test-Path -LiteralPath $wt)"
    if (Test-Path -LiteralPath $dir) { Get-ChildItem -LiteralPath $dir -Force | ForEach-Object { "  $($_.Name) $($_.Length)" } }
  }
  'write' {
    if (Test-Path -LiteralPath $file) { throw 'settings.json exists already: this VM is not a never-opened Terminal' }
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $json = @'
{
    "$schema": "https://aka.ms/terminal-profiles-schema",
    "defaultProfile": "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}",
    "profiles": {
        "list": [
            {
                "guid": "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}",
                "colorScheme": "CMD Studio",
                "font": { "face": "Cascadia Mono", "size": 12 }
            }
        ]
    },
    "schemes": [
        {
            "name": "CMD Studio",
            "background": "#1F1E1D",
            "foreground": "#FAF9F5",
            "cursorColor": "#D97757",
            "selectionBackground": "#3A3836",
            "black": "#2B2A28", "red": "#E06C5E", "green": "#8FBF7A", "yellow": "#E5B567",
            "blue": "#7BA7D9", "purple": "#B794D6", "cyan": "#6FC0C0", "white": "#D8D5CC",
            "brightBlack": "#6E6A63", "brightRed": "#F08A7E", "brightGreen": "#A8D394", "brightYellow": "#F0C887",
            "brightBlue": "#9BC0E8", "brightPurple": "#CDB0E6", "brightCyan": "#8FD4D4", "brightWhite": "#FAF9F5"
        }
    ]
}
'@
    [IO.File]::WriteAllText($file, $json, (New-Object Text.UTF8Encoding $false))
    "wrote $((Get-Item -LiteralPath $file).Length) bytes"
  }
  'start' { Start-Process -FilePath $wt; 'started' }
  'read' {
    "--- settings.json after Terminal ran ($((Get-Item -LiteralPath $file).Length) bytes):"
    Get-Content -LiteralPath $file -Raw
  }
  'close' { Get-Process WindowsTerminal -ErrorAction SilentlyContinue | Stop-Process -Force; 'closed' }
}
