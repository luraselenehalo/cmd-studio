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
$root = Split-Path $PSScriptRoot -Parent
$out = Join-Path $root 'app\ui\src\bindings'
New-Item -ItemType Directory -Force $out | Out-Null
Get-ChildItem -LiteralPath $out -Filter '*.ts' -File | Remove-Item
$env:TS_RS_EXPORT_DIR = $out
Push-Location $root
try {
  cargo test -p cmdstudio-core --features ts --lib -- export_bindings
  if ($LASTEXITCODE -ne 0) { throw "core bindings export failed ($LASTEXITCODE)" }
  if ($NoTauri) {
    Write-Host 'skipping the app crate (-NoTauri): run this on CI for the app bindings'
  } else {
    cargo test -p cmdstudio-app --lib -- export_bindings
    if ($LASTEXITCODE -ne 0) { throw "app bindings export failed ($LASTEXITCODE)" }
  }
} finally {
  Pop-Location
  Remove-Item Env:TS_RS_EXPORT_DIR -ErrorAction SilentlyContinue
}
Write-Host "bindings written to $out"
