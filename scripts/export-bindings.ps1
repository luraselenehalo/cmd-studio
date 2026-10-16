<#
.SYNOPSIS
  Writes the TypeScript bindings of the Rust types the UI consumes (ts-rs 12, G11) into
  app\ui\src\bindings, from the core (feature `ts`) and the app crate. CI runs this and fails
  when `git status --porcelain -- app/ui/src/bindings` prints anything (a changed file, a new
  one or a deleted one): the committed bindings must be what the Rust types export now.
.DESCRIPTION
  ts-rs exports through tests named export_bindings_*; TS_RS_EXPORT_DIR tells them where. The
  folder holds generated files only. A full run (no switch, as CI runs it) removes its .ts
  files first, so a type that was renamed or removed leaves no stale file behind: git shows it
  as deleted.
.PARAMETER NoTauri
  Exports the core's types only, for a machine where the app crate cannot be built (Windows
  Application Control, os error 4551). Nothing is removed in this mode. The core's files are
  written over, and the app crate's are left as they are, because nothing here exports them
  again (appendix B's check crate can, with TS_RS_EXPORT_DIR set to this folder). The app's
  export writes core types too, so a core type that was renamed or removed cannot be told from
  an app type here: its old file stays until a full run, which CI makes.
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
if (-not $NoTauri) {
  Get-ChildItem -LiteralPath $out -Filter '*.ts' -File | Remove-Item
}
$env:TS_RS_EXPORT_DIR = $out
Push-Location $root
try {
  Export-From 'core' @('test', '-p', 'cmdstudio-core', '--features', 'ts', '--lib', '--', 'export_bindings')
  if ($NoTauri) {
    Write-Host 'core only (-NoTauri): the app crate''s bindings were left as they are; a full run (CI) checks them'
  } else {
    Export-From 'app' @('test', '-p', 'cmdstudio-app', '--lib', '--', 'export_bindings')
  }
} finally {
  Pop-Location
  Remove-Item Env:TS_RS_EXPORT_DIR -ErrorAction SilentlyContinue
}
Write-Host "bindings written to $out"
