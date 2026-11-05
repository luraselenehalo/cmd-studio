# Tests scripts/upstream-watch.ps1 without the network and without touching the repo's own files:
# a fixture folder stands in for GitHub (three release documents and the assets fetch-resources.ps1
# pins, with made-up bytes), and the script rewrites a copy of fetch-resources.ps1 under -Work,
# where its own downloads go too (-Work\watch). Runs on the dev PC (-Work: a folder under the
# repo's gitignored target\, never %TEMP%) and in CI (-Work: $RUNNER_TEMP). Prints one line per
# check and a total; exits 1 when one failed.
param([Parameter(Mandatory = $true)] [string] $Work)
$ErrorActionPreference = 'Stop'
$fail = 0; $total = 0
function Expect($what, $ok) { $script:total++; if ($ok) { "pass: $what" } else { "FAIL: $what"; $script:fail++ } }
function Hash([string] $Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
$watch = Join-Path $PSScriptRoot 'upstream-watch.ps1'
$original = Join-Path $PSScriptRoot 'fetch-resources.ps1'
$Work = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Work)
New-Item -ItemType Directory -Force -Path $Work | Out-Null
$fixture = Join-Path $Work 'fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
$copy = Join-Path $Work 'fetch-resources.ps1'
$originalText = [IO.File]::ReadAllText($original)
$utf8 = New-Object Text.UTF8Encoding $false

# A release document as GitHub's API shapes it, under <owner>-<repo>.json.
function Release([string] $Repo, [string] $Tag, [string[]] $Assets) {
  $doc = @{ tag_name = $Tag; assets = @($Assets | ForEach-Object { @{ name = $_; browser_download_url = "fixture://$_" } }) }
  [IO.File]::WriteAllText((Join-Path $fixture (($Repo -replace '/', '-') + '.json')), ($doc | ConvertTo-Json -Depth 5), $utf8)
}
# An archive with these files at its root, as Clink's and Nerd Fonts' are.
function Zip([string] $Name, [hashtable] $Files) {
  $src = Join-Path $Work ('src-' + $Name)
  if (Test-Path -LiteralPath $src) { Remove-Item -LiteralPath $src -Recurse -Force }
  New-Item -ItemType Directory -Path $src | Out-Null
  foreach ($k in $Files.Keys) { [IO.File]::WriteAllText((Join-Path $src $k), $Files[$k], $utf8) }
  $zip = Join-Path $fixture $Name
  if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip }
  Compress-Archive -Path (Join-Path $src '*') -DestinationPath $zip
}
[IO.File]::WriteAllText((Join-Path $fixture 'posh-windows-amd64.exe'), 'MZ posh 31.6.0', $utf8)
[IO.File]::WriteAllText((Join-Path $fixture 'COPYING'), 'MIT 31.6.0', $utf8)
Zip 'clink.1.9.35.abc123.zip' @{ 'clink_x64.exe' = 'MZ clink 1.9.35'; 'clink_dll_x64.dll' = 'MZ dll 1.9.35'; 'LICENSE' = 'GPL' }
Zip 'CascadiaCode.zip' @{ 'CaskaydiaCoveNerdFont-Regular.ttf' = 'ttf 3.6.0'; 'LICENSE' = 'OFL' }
Release 'JanDeDobbeleer/oh-my-posh' 'v31.6.0' @('posh-windows-amd64.exe')
Release 'chrisant996/clink' 'v1.9.35' @('clink.1.9.35.abc123.zip')
Release 'ryanoasis/nerd-fonts' 'v3.6.0' @('CascadiaCode.zip')

# 1. -WhatIf names the change and writes nothing.
Copy-Item -LiteralPath $original -Destination $copy -Force
$out = & $watch -Script $copy -Fixture $fixture -Work (Join-Path $Work 'watch') -WhatIf | Out-String
Expect '-WhatIf names the three versions' ($out -match 'Oh My Posh 31\.5\.0 -> 31\.6\.0' -and $out -match 'Clink 1\.9\.34 -> 1\.9\.35' -and $out -match 'Nerd Fonts 3\.5\.1 -> 3\.6\.0')
Expect '-WhatIf shows the new download lines' ($out -match '\+ .*releases/download/v31\.6\.0/posh-windows-amd64\.exe')
Expect '-WhatIf writes nothing' ([IO.File]::ReadAllText($copy) -eq $originalText)

# 2. The rewrite: every pin of the three components, the manifest's versions, the Clink source
# line, the synopsis; the rewritten script still parses; the repo's own script is untouched.
$out = & $watch -Script $copy -Fixture $fixture -Work (Join-Path $Work 'watch') | Out-String
$new = [IO.File]::ReadAllText($copy)
$exe = Hash (Join-Path $fixture 'posh-windows-amd64.exe')
$cop = Hash (Join-Path $fixture 'COPYING')
$zip = Hash (Join-Path $fixture 'clink.1.9.35.abc123.zip')
$font = Hash (Join-Path $fixture 'CascadiaCode.zip')
Expect 'the script says changed' ($out -match 'changed:' -and $out -match 'Clink 1\.9\.34 -> 1\.9\.35')
Expect 'omp url and hash' ($new.Contains("releases/download/v31.6.0/posh-windows-amd64.exe'; Sha256 = '$exe'"))
Expect 'omp COPYING url and hash' ($new.Contains("oh-my-posh/v31.6.0/COPYING'; Sha256 = '$cop'"))
Expect 'omp pinned' (($new -match "'omp\\oh-my-posh\.exe'\s*= '$exe'") -and ($new -match "'LICENSES\\oh-my-posh-MIT\.txt'\s*= '$cop'"))
Expect 'clink download line' ($new.Contains("Name = 'clink.1.9.35.abc123.zip'; Url = 'https://github.com/chrisant996/clink/releases/download/v1.9.35/clink.1.9.35.abc123.zip'; Sha256 = '$zip'"))
Expect 'clink expand key' ($new.Contains("`$got['clink.1.9.35.abc123.zip']"))
Expect 'clink source' ($new.Contains('Clink 1.9.35 is GPL-3.0') -and $new.Contains('/clink/tree/v1.9.35'))
Expect 'font url and hash' ($new.Contains("nerd-fonts/releases/download/v3.6.0/CascadiaCode.zip'; Sha256 = '$font'"))
Expect 'manifest versions' ($new.Contains("'31.6.0' 'omp'") -and $new.Contains("'1.9.35' 'clink'") -and $new.Contains("'3.6.0' 'fonts'"))
Expect 'synopsis' ($new.Contains('Oh My Posh 31.6.0, Clink 1.9.35 (x64') -and $new.Contains('Nerd Fonts 3.6.0'))
$x = Join-Path $Work 'x'
if (Test-Path -LiteralPath $x) { Remove-Item -LiteralPath $x -Recurse -Force }
Expand-Archive -LiteralPath (Join-Path $fixture 'clink.1.9.35.abc123.zip') -DestinationPath $x
Expect 'clink files pinned' (($new -match "'clink\\clink_x64\.exe'\s*= '$(Hash (Join-Path $x 'clink_x64.exe'))'") -and ($new -match "'LICENSES\\clink-GPL-3\.0\.txt'\s*= '$(Hash (Join-Path $x 'LICENSE'))'"))
$errors = $null
[System.Management.Automation.Language.Parser]::ParseFile($copy, [ref]$null, [ref]$errors) | Out-Null
Expect 'the rewritten script parses' ($errors.Count -eq 0)
Expect 'the repo script is untouched' ([IO.File]::ReadAllText($original) -eq $originalText)

# 3. Nothing newer: said, and nothing changes.
Copy-Item -LiteralPath $original -Destination $copy -Force
Release 'JanDeDobbeleer/oh-my-posh' 'v31.5.0' @('posh-windows-amd64.exe')
Release 'chrisant996/clink' 'v1.9.34' @('clink.1.9.34.4e8ae4.zip')
Release 'ryanoasis/nerd-fonts' 'v3.5.1' @('CascadiaCode.zip')
$out = & $watch -Script $copy -Fixture $fixture -Work (Join-Path $Work 'watch') | Out-String
Expect 'nothing newer is said' ($out -match 'nothing newer upstream')
Expect 'nothing newer changes nothing' ([IO.File]::ReadAllText($copy) -eq $originalText)

"upstream-watch tests: $($total - $fail)/$total pass"
if ($fail) { exit 1 }
