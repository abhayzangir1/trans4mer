# Trans4mers Local Packaging Script (Windows)
# Builds frontend and packages native Windows installer (.exe / .msi)

$ErrorActionPreference = "Stop"

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "   Trans4mers Desktop Packaging Script   " -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan

$WorkspaceRoot = (Get-Item $PSScriptRoot).Parent.FullName
$DesktopDir = Join-Path $WorkspaceRoot "apps\desktop"

Write-Host "`n[1/3] Validating Rust and Cargo..." -ForegroundColor Yellow
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "Cargo is not found on PATH. Please install Rust from https://rustup.rs."
}

Write-Host "[2/3] Building Vite Production Frontend..." -ForegroundColor Yellow
Push-Location $DesktopDir
try {
    npm run build
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Frontend build failed with exit code $LASTEXITCODE."
    }
} finally {
    Pop-Location
}

Write-Host "`n[3/3] Compiling Tauri Release Installer..." -ForegroundColor Yellow
Push-Location $DesktopDir
try {
    npm run tauri build
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Tauri build failed with exit code $LASTEXITCODE."
    }
} finally {
    Pop-Location
}

Write-Host "`n=========================================" -ForegroundColor Green
Write-Host "  Build Complete! Installer located in: " -ForegroundColor Green
Write-Host "  $DesktopDir\src-tauri\target\release\bundle\nsis\" -ForegroundColor White
Write-Host "=========================================" -ForegroundColor Green
