<#
.SYNOPSIS
  Writes the TypeScript bindings of the Rust types the UI consumes (ts-rs 12, G11) into
  app\ui\src\bindings, from the core (feature `ts`) and the app crate. CI runs this and fails
  when `git status --porcelain -- app/ui/src/bindings` prints anything (a changed file, a new
  one or a deleted one): the committed bindings must be what the Rust types export now.
.DESCRIPTION
  ts-rs exports through tests named export_bindings_*; TS_RS_EXPORT_DIR tells them where. The
  folder holds generated files only, and its .ts files are removed first, so a type that was
  renamed or removed leaves no stale file behind: git shows it as deleted. The app crate's
  export runs through the check crate when given -NoTauri, for a machine where the Tauri build
  is blocked (Windows Application Control, os error 4551); CI runs the app crate.
#>
param(
  [switch] $NoTauri
)
$ErrorActionPreference = 'Stop'

# Runs one export with cargo; its exit code alone says whether it failed. Windows PowerShell 5.1
# turns each line a program writes to stderr (cargo's progress) into an error record when the
# script's output is redirected, and with 'Stop' the first one would end the script halfway, so
# the preference is 'Continue' in here (PowerShell 7 does not do this).
function Export-From([string] $what, [string[]] $cargoArgs) {
  $ErrorActionPreference = 'Continue'
  & cargo @cargoArgs
  if ($LASTEXITCODE -ne 0) { throw "$what bindings export failed ($LASTEXITCODE)" }
}

$root = Split-Path $PSScriptRoot -Parent
$out = Join-Path $root 'app\ui\src\bindings'
New-Item -ItemType Directory -Force $out | Out-Null
Get-ChildItem -LiteralPath $out -Filter '*.ts' -File | Remove-Item
$env:TS_RS_EXPORT_DIR = $out
Push-Location $root
try {
  Export-From 'core' @('test', '-p', 'cmdstudio-core', '--features', 'ts', '--lib', '--', 'export_bindings')
  if ($NoTauri) {
    Write-Host 'skipping the app crate (-NoTauri): run this on CI for the app bindings'
  } else {
    Export-From 'app' @('test', '-p', 'cmdstudio-app', '--lib', '--', 'export_bindings')
  }
} finally {
  Pop-Location
  Remove-Item Env:TS_RS_EXPORT_DIR -ErrorAction SilentlyContinue
}
Write-Host "bindings written to $out"
