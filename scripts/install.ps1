# Trans4mers developer environment installer (Windows).
$ErrorActionPreference = "Stop"

Write-Host "==> Trans4mers development setup" -ForegroundColor Cyan

# Rust
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "==> Installing Rust (rustup)"
    winget install --id Rustlang.Rustup -e --accept-source-agreements --accept-package-agreements
    $env:Path += ";$env:USERPROFILE\.cargo\bin"
}

# Node prerequisite check
if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
    Write-Host "!! Node.js/npm is required but not found — install Node 22+ first:" -ForegroundColor Red
    Write-Host "      https://nodejs.org/"
    exit 1
}

# WebView2 (Tauri backend on Windows) — usually preinstalled on Windows 10/11.
$webview2 = Get-ItemProperty -Path "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" -ErrorAction SilentlyContinue
if (-not $webview2) {
    Write-Host "==> Installing WebView2 Runtime"
    winget install --id Microsoft.EdgeWebView2Runtime -e --accept-source-agreements --accept-package-agreements
}

# Ollama (optional). ASK FIRST — nothing is installed without explicit
# consent, and NO MODEL is ever installed by this script or by the app
# (you pull models yourself, e.g. `ollama pull qwen2.5-coder:32b`).
if (-not (Get-Command ollama -ErrorAction SilentlyContinue)) {
    Write-Host "==> Ollama (the local LLM runtime) is not installed." -ForegroundColor Yellow
    $answer = Read-Host "    Install Ollama now? [y/N]"
    if ($answer -eq "y" -or $answer -eq "Y") {
        winget install --id Ollama.Ollama -e --accept-source-agreements --accept-package-agreements
    } else {
        Write-Host "    Skipped. Install it yourself from https://ollama.com if you want local models."
    }
}

Write-Host "==> Frontend dependencies"
Push-Location apps\desktop
npm install
Pop-Location

Write-Host ""
Write-Host "Setup complete. Run the app:" -ForegroundColor Green
Write-Host "  .\run-app.bat"
Write-Host "  or: cd apps\desktop; npm run tauri dev"
Write-Host "Run the test suite:"
Write-Host "  cargo test --workspace --exclude trans4mers-desktop"
