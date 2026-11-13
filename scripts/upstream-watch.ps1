<#
.SYNOPSIS
  Watches upstream for newer releases of the add-ons CMD Studio bundles (spec §6.2, §6.4, phase
  4b): Oh My Posh, Clink and Nerd Fonts. For each newer release it downloads what
  fetch-resources.ps1 pins, computes every SHA-256 (the downloads and the files taken out of the
  archives), and rewrites that script's pins in place. Nothing else changes: the versions in the
  core, credits.ts, docs/credits.md, the release notes, the README and the parity test are a
  person's, which the pull request the workflow opens lists. Works in Windows PowerShell 5.1 and
  PowerShell 7.
.PARAMETER Script
  The fetch-resources.ps1 to read and rewrite. Default: the one beside this script.
.PARAMETER Fixture
  A folder that stands in for GitHub: <owner>-<repo>.json (the release document, as the API
  shapes it) and the assets under their names. No network is used then (upstream-watch.test.ps1).
.PARAMETER Work
  Where the downloads and the extracted files go, in a folder of their own made under it and
  removed at the end. Default: the process's temp folder, for the workflow's runner; on the dev PC
  it is always given (a folder under the repo's target\), never %TEMP%.
.PARAMETER WhatIf
  Print the lines that would change; write nothing.
#>
param(
  [string] $Script = (Join-Path $PSScriptRoot 'fetch-resources.ps1'),
  [string] $Fixture = '',
  [string] $Work = [IO.Path]::GetTempPath(),
  [switch] $WhatIf
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$Script = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Script)
$Work = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Work)
New-Item -ItemType Directory -Force -Path $Work | Out-Null
$text = [IO.File]::ReadAllText($Script)

function Hash([string] $Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function NewDir([string] $Parent) {
  $d = Join-Path $Parent ('cmdstudio-upstream-' + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Path $d | Out-Null
  $d
}
# The version pinned for an asset: the `$Downloads` line that names it, .../download/v<version>/...
function PinnedVersion([string] $Text, [string] $AssetName) {
  $m = [regex]::Match($Text, "Name = '" + [regex]::Escape($AssetName) + "'[^\r\n]*?/releases/download/v(?<v>[^/']+)/")
  if (-not $m.Success) { throw "fetch-resources.ps1 has no download line for $AssetName" }
  $m.Groups['v'].Value
}
# Clink's zip is named with the version and a commit hash: both come from the line.
function PinnedClink([string] $Text) {
  $m = [regex]::Match($Text, "Name = 'clink\.(?<v>[0-9.]+)\.(?<h>[0-9a-f]+)\.zip'")
  if (-not $m.Success) { throw 'fetch-resources.ps1 has no Clink download line' }
  @{ Version = $m.Groups['v'].Value; Name = 'clink.' + $m.Groups['v'].Value + '.' + $m.Groups['h'].Value + '.zip' }
}
function LatestRelease([string] $Repo) {
  if ($Fixture) { return (Get-Content -LiteralPath (Join-Path $Fixture (($Repo -replace '/', '-') + '.json')) -Raw | ConvertFrom-Json) }
  Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ 'User-Agent' = 'cmd-studio-upstream-watch'; Accept = 'application/vnd.github+json' } -UseBasicParsing
}
function Fetch([string] $Url, [string] $Name, [string] $Dir) {
  $file = Join-Path $Dir $Name
  if ($Fixture) { Copy-Item -LiteralPath (Join-Path $Fixture $Name) -Destination $file }
  else { Invoke-WebRequest -Uri $Url -OutFile $file -UseBasicParsing }
  $file
}
function AssetUrl($Release, [string] $Name) {
  $a = @($Release.assets | Where-Object { $_.name -eq $Name })
  if ($a.Count -ne 1) { throw "release $($Release.tag_name) has $($a.Count) assets named $Name" }
  $a[0].browser_download_url
}
function Newer([string] $Old, [string] $New) { [version] $New -gt [version] $Old }
function Tag($Release) { "$($Release.tag_name)".TrimStart('v') }
# One replacement that must match exactly once, so a reshaped fetch-resources.ps1 stops the
# script instead of being half rewritten. In the replacement, `${1}` is the first group and `$$`
# a literal dollar.
function ReplaceOnce([string] $Text, [string] $Pattern, [string] $Replacement, [string] $What) {
  $count = [regex]::Matches($Text, $Pattern).Count
  if ($count -ne 1) { throw "${What}: expected one match of '$Pattern', found $count" }
  [regex]::Replace($Text, $Pattern, $Replacement)
}

$work = NewDir $Work
$changes = @()
try {
  $new = $text

  # Oh My Posh: the exe, and COPYING at the tag (raw.githubusercontent, not a release asset).
  $ompOld = PinnedVersion $text 'posh-windows-amd64.exe'
  $omp = LatestRelease 'JanDeDobbeleer/oh-my-posh'
  $ompNew = Tag $omp
  if (Newer $ompOld $ompNew) {
    $dir = NewDir $work
    $exe = Fetch (AssetUrl $omp 'posh-windows-amd64.exe') 'posh-windows-amd64.exe' $dir
    $copying = Fetch "https://raw.githubusercontent.com/JanDeDobbeleer/oh-my-posh/v$ompNew/COPYING" 'COPYING' $dir
    $exeHash = Hash $exe
    $copyingHash = Hash $copying
    $o = [regex]::Escape($ompOld)
    $new = ReplaceOnce $new ("(Name = 'posh-windows-amd64\.exe'; Url = '[^']*/v)" + $o + "(/posh-windows-amd64\.exe'; Sha256 = ')[0-9a-f]{64}") ('${1}' + $ompNew + '${2}' + $exeHash) 'omp download'
    $new = ReplaceOnce $new ("(Name = 'oh-my-posh-COPYING'; Url = '[^']*/v)" + $o + "(/COPYING'; Sha256 = ')[0-9a-f]{64}") ('${1}' + $ompNew + '${2}' + $copyingHash) 'omp COPYING'
    $new = ReplaceOnce $new "('omp\\oh-my-posh\.exe'\s*= ')[0-9a-f]{64}" ('${1}' + $exeHash) 'omp pinned'
    $new = ReplaceOnce $new "('LICENSES\\oh-my-posh-MIT\.txt'\s*= ')[0-9a-f]{64}" ('${1}' + $copyingHash) 'omp license pinned'
    $new = ReplaceOnce $new ("(& \`$bundle ')" + $o + "(' 'omp')") ('${1}' + $ompNew + '${2}') 'omp manifest'
    $new = ReplaceOnce $new ('Oh My Posh ' + $o) ('Oh My Posh ' + $ompNew) 'omp synopsis'
    $changes += "Oh My Posh $ompOld -> $ompNew"
  }

  # Clink: the one zip asset named clink.<version>.<hash>.zip; three files taken out of it.
  $clinkOld = PinnedClink $text
  $clink = LatestRelease 'chrisant996/clink'
  $clinkNew = Tag $clink
  if (Newer $clinkOld.Version $clinkNew) {
    $asset = @($clink.assets | Where-Object { $_.name -match '^clink\.[0-9.]+\.[0-9a-f]+\.zip$' })
    if ($asset.Count -ne 1) { throw "release $($clink.tag_name) has $($asset.Count) assets named like clink.<version>.<hash>.zip" }
    $zipName = $asset[0].name
    $dir = NewDir $work
    $zip = Fetch $asset[0].browser_download_url $zipName $dir
    $x = NewDir $dir
    Expand-Archive -LiteralPath $zip -DestinationPath $x
    $zipHash = Hash $zip
    $exeHash = Hash (Join-Path $x 'clink_x64.exe')
    $dllHash = Hash (Join-Path $x 'clink_dll_x64.dll')
    $licenseHash = Hash (Join-Path $x 'LICENSE')
    $oldName = [regex]::Escape($clinkOld.Name)
    $o = [regex]::Escape($clinkOld.Version)
    $new = ReplaceOnce $new ("Name = '" + $oldName + "'; Url = '([^']*/v)" + $o + '/' + $oldName + "'; Sha256 = '[0-9a-f]{64}") ("Name = '" + $zipName + "'; Url = '" + '${1}' + $clinkNew + '/' + $zipName + "'; Sha256 = '" + $zipHash) 'clink download'
    $new = ReplaceOnce $new ('\$got\[' + "'" + $oldName + "'" + '\]') ('$$got[' + "'" + $zipName + "'" + ']') 'clink expand'
    $new = ReplaceOnce $new "('clink\\clink_x64\.exe'\s*= ')[0-9a-f]{64}" ('${1}' + $exeHash) 'clink exe pinned'
    $new = ReplaceOnce $new "('clink\\clink_dll_x64\.dll'\s*= ')[0-9a-f]{64}" ('${1}' + $dllHash) 'clink dll pinned'
    $new = ReplaceOnce $new "('LICENSES\\clink-GPL-3\.0\.txt'\s*= ')[0-9a-f]{64}" ('${1}' + $licenseHash) 'clink license pinned'
    $new = ReplaceOnce $new ('"Clink ' + $o + ' is GPL') ('"Clink ' + $clinkNew + ' is GPL') 'clink source line'
    $new = ReplaceOnce $new ('/clink/tree/v' + $o) ('/clink/tree/v' + $clinkNew) 'clink source url'
    $new = ReplaceOnce $new ("(& \`$bundle ')" + $o + "(' 'clink')") ('${1}' + $clinkNew + '${2}') 'clink manifest'
    $new = ReplaceOnce $new ('Clink ' + $o + ' \(x64') ('Clink ' + $clinkNew + ' (x64') 'clink synopsis'
    $changes += "Clink $($clinkOld.Version) -> $clinkNew"
  }

  # Nerd Fonts: the CascadiaCode archive; the one font file and its license taken out of it.
  $fontOld = PinnedVersion $text 'CascadiaCode.zip'
  $fonts = LatestRelease 'ryanoasis/nerd-fonts'
  $fontNew = Tag $fonts
  if (Newer $fontOld $fontNew) {
    $dir = NewDir $work
    $zip = Fetch (AssetUrl $fonts 'CascadiaCode.zip') 'CascadiaCode.zip' $dir
    $x = NewDir $dir
    Expand-Archive -LiteralPath $zip -DestinationPath $x
    $zipHash = Hash $zip
    $ttfHash = Hash (Join-Path $x 'CaskaydiaCoveNerdFont-Regular.ttf')
    $licenseHash = Hash (Join-Path $x 'LICENSE')
    $o = [regex]::Escape($fontOld)
    $new = ReplaceOnce $new ("(Name = 'CascadiaCode\.zip'; Url = '[^']*/v)" + $o + "(/CascadiaCode\.zip'; Sha256 = ')[0-9a-f]{64}") ('${1}' + $fontNew + '${2}' + $zipHash) 'font download'
    $new = ReplaceOnce $new "('fonts\\CaskaydiaCoveNerdFont-Regular\.ttf'\s*= ')[0-9a-f]{64}" ('${1}' + $ttfHash) 'font pinned'
    $new = ReplaceOnce $new "('LICENSES\\CaskaydiaCove-NF-OFL-1\.1\.txt'\s*= ')[0-9a-f]{64}" ('${1}' + $licenseHash) 'font license pinned'
    $new = ReplaceOnce $new ("(& \`$bundle ')" + $o + "(' 'fonts')") ('${1}' + $fontNew + '${2}') 'font manifest'
    $new = ReplaceOnce $new ('Nerd Fonts ' + $o) ('Nerd Fonts ' + $fontNew) 'font synopsis'
    $changes += "Nerd Fonts $fontOld -> $fontNew"
  }

  if ($changes.Count -eq 0) { 'nothing newer upstream'; return }
  if ($WhatIf) {
    'would change:'
    $changes
    $oldLines = $text -split "`r?`n"
    $newLines = $new -split "`r?`n"
    for ($i = 0; $i -lt $newLines.Count; $i++) {
      if ($oldLines[$i] -ne $newLines[$i]) { '- ' + $oldLines[$i]; '+ ' + $newLines[$i] }
    }
    return
  }
  [IO.File]::WriteAllText($Script, $new, (New-Object Text.UTF8Encoding $false))
  'changed:'
  $changes
} finally {
  Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
