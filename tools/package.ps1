#Requires -Version 5.1
<#
  Builds release, a portable zip and (when NSIS is installed) the per-user installer.
  Usage: powershell -ExecutionPolicy Bypass -File tools/package.ps1
  Installer needs NSIS: winget install NSIS.NSIS   (or choco install nsis)
#>
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
cargo build -p teardrop --release
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item "target/release/teardrop.exe" "dist/teardrop.exe" -Force
Compress-Archive -Path "dist/teardrop.exe", "README.md", "LICENSE" -DestinationPath "dist/teardrop-portable.zip" -Force

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
$nsis = Get-Command makensis -ErrorAction SilentlyContinue
if (-not $nsis) {
    $pf = ${env:ProgramFiles(x86)}, $env:ProgramFiles | Where-Object { $_ }
    $nsis = $pf | ForEach-Object { Join-Path $_ 'NSIS\makensis.exe' } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if ($nsis) {
    $exe = if ($nsis -is [string]) { $nsis } else { $nsis.Source }
    & $exe "/DVERSION=$version" tools/installer.nsi
    Write-Host "dist/teardrop-setup-$version.exe, dist/teardrop.exe and dist/teardrop-portable.zip ready"
} else {
    Write-Host "dist/teardrop.exe + dist/teardrop-portable.zip ready (install NSIS to also build the installer)"
}
