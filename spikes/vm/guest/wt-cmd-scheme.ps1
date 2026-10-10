# Runs inside a test VM (G7, G8). -Setup writes a minimal Windows Terminal settings.json whose cmd
# profile keeps WT's stock commandline but uses the prototype's "Claude" scheme. -Background
# changes only that scheme's background, the same edit as spikes/wt/g8-live-reload.ps1.
param([switch] $Setup, [string] $Background, [string] $FontFace)
$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:LOCALAPPDATA 'Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState'
$s = Join-Path $dir 'settings.json'
$utf8 = New-Object Text.UTF8Encoding $false
if ($Setup) {
  New-Item -ItemType Directory -Force $dir | Out-Null
  if (Test-Path $s) { Copy-Item $s "$s.before-cmdstudio" -Force; "existing settings.json backed up" }
  $font = if ($FontFace) { ", `"font`": { `"face`": `"$FontFace`" }" } else { '' }
  $json = @"
{
    "`$schema": "https://aka.ms/terminal-profiles-schema",
    "profiles": {
        "list": [
            { "guid": "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}", "colorScheme": "Claude"$font }
        ]
    },
    "schemes": [
        {
            "name": "Claude",
            "background": "#1F1E1D", "foreground": "#FAF9F5", "cursorColor": "#D97757", "selectionBackground": "#4A4845",
            "black": "#3A3936", "red": "#E5736A", "green": "#8FBF7F", "yellow": "#E3B66B",
            "blue": "#7FA7D9", "purple": "#B79BDB", "cyan": "#7FC1B8", "white": "#D8D6CF",
            "brightBlack": "#8A8780", "brightRed": "#F0907F", "brightGreen": "#A8D196", "brightYellow": "#EECB8A",
            "brightBlue": "#9BBDE8", "brightPurple": "#CBB4E8", "brightCyan": "#9DD3CB", "brightWhite": "#FAF9F5"
        }
    ]
}
"@
  [IO.File]::WriteAllText($s, $json, $utf8)
  "settings.json written: cmd profile -> Claude scheme$(if ($FontFace) { ", font $FontFace" })"
}
if ($Background) {
  $text = [IO.File]::ReadAllText($s, $utf8)
  $new = [regex]::Replace($text, '"background": "#[0-9A-Fa-f]{6}"', "`"background`": `"$Background`"", 1)
  [IO.File]::WriteAllText($s, $new, $utf8)
  "scheme background -> $Background"
}
