<#
.SYNOPSIS
  Fills resources\ with the runtimes CMD Studio ships (spec sections 5.1, 6.1, 6.6):
  Oh My Posh 31.5.0, Clink 1.9.34 (x64 files only), the CaskaydiaCove NF font
  from Nerd Fonts 3.5.1, and their licenses. Writes resources\manifest.json
  with the SHA-256 of every file, which the app checks (doctor D1).
.DESCRIPTION
  Every download is checked against the SHA-256 pinned below before it is
  used, and every file taken out of an archive against its own pin. A
  resources\ folder that already matches is left alone, so CI can cache it.
  Works in Windows PowerShell 5.1 and PowerShell 7.
.PARAMETER Destination
  Where to put the files. Default: resources\ at the repo root. An existing
  Destination is replaced only when it is an empty folder or holds a CMD Studio
  manifest.json; any other folder or file is refused before anything is
  downloaded.
.PARAMETER Cache
  A folder that may already hold the downloads below under their file names;
  a file there is used only when its hash matches.
#>
param(
  [string] $Destination = (Join-Path (Split-Path $PSScriptRoot -Parent) 'resources'),
  [string] $Cache = ''
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # Invoke-WebRequest is very slow with a progress bar in 5.1
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# One absolute path from here on: PowerShell and .NET resolve a relative one against different
# places, Split-Path gives '' for a bare name, and the staging folder is made next to it.
# An empty value would resolve to the current folder, so it is refused (an unset variable passed on).
if ([string]::IsNullOrWhiteSpace($Destination)) { throw '-Destination must not be empty' }
$Destination = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Destination)

# Downloads. Hashes checked on 2026-10-11:
#  - posh-windows-amd64.exe: publisher's checksums.txt, GitHub's asset digest, and a local download agree.
#  - COPYING: computed from a local download of the v31.5.0 tag (GitHub blob 0052a411de0b7179803ca0723ddc782fe8249227).
#  - clink zip: GitHub's asset digest and a local download agree (Clink publishes a .cat catalog, not a checksum file).
#  - CascadiaCode.zip: publisher's SHA-256.txt, GitHub's asset digest, a local download and spike G12 agree.
$Downloads = @(
  @{ Name = 'posh-windows-amd64.exe'; Url = 'https://github.com/JanDeDobbeleer/oh-my-posh/releases/download/v31.5.0/posh-windows-amd64.exe'; Sha256 = '9dd2c1297d5ae89f23f19b02f74a25510fcb56d84da5139fa3c3ca6e6d008a73' },
  @{ Name = 'oh-my-posh-COPYING'; Url = 'https://raw.githubusercontent.com/JanDeDobbeleer/oh-my-posh/v31.5.0/COPYING'; Sha256 = 'a5308c4e51268229a039ec4ec9a251a4cdb89d9380383e6e13aeba64a74f19ad' },
  @{ Name = 'clink.1.9.34.4e8ae4.zip'; Url = 'https://github.com/chrisant996/clink/releases/download/v1.9.34/clink.1.9.34.4e8ae4.zip'; Sha256 = '50f6ee0474e3bd02b48ce8c8f63ed933a14aee5127fc7072cb976ce10a43ce23' },
  @{ Name = 'CascadiaCode.zip'; Url = 'https://github.com/ryanoasis/nerd-fonts/releases/download/v3.5.1/CascadiaCode.zip'; Sha256 = '1298bf92698afa06185cf1d05e6ae05f2d8a1e8c3cb45ddf4c3035168ab342a1' }
)
# Files taken out of the archives (computed 2026-10-11 from the archives above).
$Pinned = @{
  'omp\oh-my-posh.exe'                        = '9dd2c1297d5ae89f23f19b02f74a25510fcb56d84da5139fa3c3ca6e6d008a73'
  'clink\clink_x64.exe'                       = 'becacc801270b8282c4cdff9378dc40f061bfe1b76bb87ca621c4bc572c2d8eb'
  'clink\clink_dll_x64.dll'                   = '889b712037886dfde205e4b637a021848734ed218ee21c5718e2bf23a9e9d8f2'
  'fonts\CaskaydiaCoveNerdFont-Regular.ttf'   = 'fae37ce4fdc965ba5d3caea122b856418dc5dec2e4e638a024c49d681ffce5e9'
  'LICENSES\oh-my-posh-MIT.txt'               = 'a5308c4e51268229a039ec4ec9a251a4cdb89d9380383e6e13aeba64a74f19ad'
  'LICENSES\clink-GPL-3.0.txt'                = '5f631fae467c82b8cd28fd1ec425c816895a35f9d94e36bee0e0164570e8e0f6'
  'LICENSES\CaskaydiaCove-NF-OFL-1.1.txt'     = '82c05d6c53dfa0c9025985c19e371810020b74ed2c61d51d370f2a8ab2506d52'
}
$ClinkSource = "Clink 1.9.34 is GPL-3.0. Its source code for this exact release:`nhttps://github.com/chrisant996/clink/tree/v1.9.34`n"

function Hash([string] $Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function NewDir([string] $Parent) {
  $d = Join-Path $Parent ("cmdstudio-" + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Path $d | Out-Null
  $d
}
function Manifest([hashtable] $files) {
  # Hand-written JSON: stable order and LF endings in both PowerShell versions.
  $bundle = {
    param($version, $prefix)
    $lines = @($files.Keys | Where-Object { $_.StartsWith("$prefix\") } | Sort-Object | ForEach-Object {
        '      "{0}": "{1}"' -f $_.Substring($prefix.Length + 1), $files[$_] })
    "    `"version`": `"$version`",`n    `"files`": {`n" + ($lines -join ",`n") + "`n    }"
  }
  "{`n  `"omp`": {`n" + (& $bundle '31.5.0' 'omp') + "`n  },`n  `"clink`": {`n" + (& $bundle '1.9.34' 'clink') +
    "`n  },`n  `"fonts`": {`n" + (& $bundle '3.5.1' 'fonts') + "`n  }`n}`n"
}
function Complete([string] $Dir) {
  $m = Join-Path $Dir 'manifest.json'
  if (-not (Test-Path -LiteralPath $m) -or [IO.File]::ReadAllText($m) -ne (Manifest $Pinned)) { return $false }
  foreach ($rel in $Pinned.Keys) {
    $p = Join-Path $Dir $rel
    if (-not (Test-Path -LiteralPath $p) -or (Hash $p) -ne $Pinned[$rel]) { return $false }
  }
  $true
}
# This script deletes the folder it replaces, so that folder must be ours: absent, an empty
# folder, or a folder holding a CMD Studio manifest.json (JSON with omp, clink and fonts).
function Replaceable([string] $Dir) {
  if (-not (Test-Path -LiteralPath $Dir)) { return $true }
  if (-not (Test-Path -LiteralPath $Dir -PathType Container)) { return $false }   # a file
  if (-not (Get-ChildItem -LiteralPath $Dir -Force)) { return $true }              # an empty folder
  try {
    $json = [IO.File]::ReadAllText((Join-Path $Dir 'manifest.json')) | ConvertFrom-Json
  } catch { return $false }                                                        # no manifest, or not JSON
  if ($null -eq $json) { return $false }
  $names = @($json.PSObject.Properties | ForEach-Object { $_.Name })
  foreach ($key in 'omp', 'clink', 'fonts') { if ($names -cnotcontains $key) { return $false } }
  $true
}

if (Complete $Destination) { "resources are complete: $Destination"; return }

# Before any download, so a wrong -Destination fails fast and nothing has been touched.
$NotOurs = "$Destination is not a CMD Studio resources folder; choose another -Destination or remove it yourself"
if (-not (Replaceable $Destination)) { throw $NotOurs }

$work = NewDir ([IO.Path]::GetTempPath())
$stage = ''
try {
  $got = @{}
  foreach ($d in $Downloads) {
    $cached = if ($Cache) { Join-Path $Cache $d.Name } else { '' }
    $dir = NewDir $work   # one new, empty folder per download
    $file = Join-Path $dir $d.Name
    if ($cached -and (Test-Path -LiteralPath $cached) -and (Hash $cached) -eq $d.Sha256) {
      Copy-Item -LiteralPath $cached -Destination $file
    } else {
      "downloading $($d.Url)"
      Invoke-WebRequest -Uri $d.Url -OutFile $file -UseBasicParsing
    }
    $h = Hash $file
    if ($h -ne $d.Sha256) { throw "$($d.Name): SHA-256 $h, expected $($d.Sha256)" }
    $got[$d.Name] = $file
  }
  $clinkX = NewDir $work
  Expand-Archive -LiteralPath $got['clink.1.9.34.4e8ae4.zip'] -DestinationPath $clinkX
  $fontX = NewDir $work
  Expand-Archive -LiteralPath $got['CascadiaCode.zip'] -DestinationPath $fontX

  # Staged next to the destination, so the final step is a rename on one volume.
  $parent = Split-Path $Destination -Parent
  New-Item -ItemType Directory -Force -Path $parent | Out-Null
  $stage = NewDir $parent
  $copy = @{
    'omp\oh-my-posh.exe'                      = $got['posh-windows-amd64.exe']
    'clink\clink_x64.exe'                     = Join-Path $clinkX 'clink_x64.exe'
    'clink\clink_dll_x64.dll'                 = Join-Path $clinkX 'clink_dll_x64.dll'
    'fonts\CaskaydiaCoveNerdFont-Regular.ttf' = Join-Path $fontX 'CaskaydiaCoveNerdFont-Regular.ttf'
    'LICENSES\oh-my-posh-MIT.txt'             = $got['oh-my-posh-COPYING']
    'LICENSES\clink-GPL-3.0.txt'              = Join-Path $clinkX 'LICENSE'
    'LICENSES\CaskaydiaCove-NF-OFL-1.1.txt'   = Join-Path $fontX 'LICENSE'
  }
  foreach ($rel in $copy.Keys) {
    $to = Join-Path $stage $rel
    New-Item -ItemType Directory -Force -Path (Split-Path $to -Parent) | Out-Null
    Copy-Item -LiteralPath $copy[$rel] -Destination $to
    $h = Hash $to
    if ($h -ne $Pinned[$rel]) { throw "${rel}: SHA-256 $h, expected $($Pinned[$rel])" }
  }
  $utf8 = New-Object Text.UTF8Encoding $false
  [IO.File]::WriteAllText((Join-Path $stage 'LICENSES\clink-SOURCE.txt'), $ClinkSource, $utf8)
  [IO.File]::WriteAllText((Join-Path $stage 'manifest.json'), (Manifest $Pinned), $utf8)

  if (Test-Path -LiteralPath $Destination) {
    if (-not (Replaceable $Destination)) { throw $NotOurs }   # again: the downloads took a while
    Remove-Item -LiteralPath $Destination -Recurse -Force
  }
  Move-Item -LiteralPath $stage -Destination $Destination
  $stage = ''
  if (-not (Complete $Destination)) { throw "resources at $Destination are incomplete after the copy" }
  "resources ready: $Destination"
} finally {
  Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
  if ($stage) { Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue }
}
