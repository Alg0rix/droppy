#Requires -Version 5.1
<#
  Builds release, a portable zip and the modern per-user installer.
  Usage: powershell -ExecutionPolicy Bypass -File tools/package.ps1
#>
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

# Ensure brand assets & installer art are up to date
python tools/make_brand.py

cargo build -p teardrop --release
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item "target/release/teardrop.exe" "dist/teardrop.exe" -Force
Compress-Archive -Path "dist/teardrop.exe", "README.md", "LICENSE" -DestinationPath "dist/teardrop-portable.zip" -Force

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value

# 1. Look for Inno Setup compiler (ISCC)
$iscc = Get-Command iscc -ErrorAction SilentlyContinue
if (-not $iscc) {
    $searchPaths = @(
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Antigravity IDE\resources\app\node_modules\innosetup\bin\ISCC.exe"
    )
    foreach ($p in $searchPaths) {
        if (Test-Path $p) {
            $iscc = $p
            break
        }
    }
}

if ($iscc) {
    $exe = if ($iscc -is [string]) { $iscc } else { $iscc.Source }
    & $exe "/DAppVersion=$version" installer/teardrop.iss
    Write-Host "[OK] dist/teardrop-setup-$version.exe ready (built with Inno Setup)"
} else {
    $nsis = Get-Command makensis -ErrorAction SilentlyContinue
    if (-not $nsis) {
        $nsisCandidates = @(
            "$env:ProgramFiles\NSIS\makensis.exe",
            "C:\Program Files (x86)\NSIS\makensis.exe"
        )
        foreach ($p in $nsisCandidates) {
            if (Test-Path $p) {
                $nsis = $p
                break
            }
        }
    }
    if ($nsis) {
        $exe = if ($nsis -is [string]) { $nsis } else { $nsis.Source }
        & $exe "/DVERSION=$version" installer/teardrop.nsi
        Write-Host "[OK] dist/teardrop-setup-$version.exe ready (built with NSIS)"
    } else {
        Write-Host "dist/teardrop.exe + dist/teardrop-portable.zip ready (install Inno Setup or NSIS to build the installer)"
    }
}

Write-Host "Packaging complete."
