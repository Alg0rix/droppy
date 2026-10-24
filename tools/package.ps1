#Requires -Version 5.1
<#
  Builds release + portable zip. NSIS installer step is optional.
  Usage: powershell -ExecutionPolicy Bypass -File tools/package.ps1
#>
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
cargo build -p teardrop --release
New-Item -ItemType Directory -Force dist | Out-Null
$exe = "target/release/teardrop.exe"
Copy-Item $exe "dist/teardrop.exe" -Force
Compress-Archive -Path "dist/teardrop.exe", "README.md", "LICENSE*" -DestinationPath "dist/teardrop-portable.zip" -Force -ErrorAction SilentlyContinue
# Optional NSIS (per-user, no admin):
# makensis /DVERSION=0.1.0 tools/installer.nsi
Write-Host "dist/teardrop.exe + dist/teardrop-portable.zip ready"
