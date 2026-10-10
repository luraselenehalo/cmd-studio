# G8: does Windows Terminal reload settings.json and recolor a tab that is already open?
# Run it from the open tab you are watching. It turns the "Claude" scheme background
# reddish for 10 seconds, writes the original bytes back, and checks the hash.
$ErrorActionPreference = 'Stop'
$s = Join-Path $env:LOCALAPPDATA 'Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json'
$tmp = Join-Path $PSScriptRoot 'tmp'
New-Item -ItemType Directory -Force $tmp | Out-Null
$log = Join-Path $tmp 'g8.log'
$backup = Join-Path $tmp 'settings.g8-backup.json'
function Log($line) { $line; Add-Content -LiteralPath $log -Value "$(Get-Date -Format s) $line" -Encoding UTF8 }

# Bytes, not Get-Content: Windows PowerShell would decode the file with the ANSI code page.
$orig = [IO.File]::ReadAllBytes($s)
$hash = (Get-FileHash $s -Algorithm SHA256).Hash
[IO.File]::WriteAllBytes($backup, $orig)
Log "before: $($orig.Length) bytes sha256=$hash backup=$backup"
$text = [Text.Encoding]::UTF8.GetString($orig)
$from = '"background": "#1F1E1D"'
$to = '"background": "#3A1E1D"'
$n = ([regex]::Matches($text, [regex]::Escape($from))).Count
if ($n -ne 1) { Log "stop: expected the background string once, found $n. Nothing changed."; exit 1 }
try {
  [IO.File]::WriteAllText($s, $text.Replace($from, $to), (New-Object Text.UTF8Encoding $false))
  Log 'changed: watch this tab for 10 seconds. Does the background turn reddish?'
  Start-Sleep -Seconds 10
} finally {
  [IO.File]::WriteAllBytes($s, $orig)
  $after = (Get-FileHash $s -Algorithm SHA256).Hash
  if ($after -eq $hash) { Log "restored: sha256 matches $after" }
  else { Log "RESTORE MISMATCH: $after, expected $hash. Copy the backup over settings.json." }
}
