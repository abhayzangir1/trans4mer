@echo off
echo =======================================================
echo          Starting Trans4mers Sovereign Agent OS
echo =======================================================

:: Auto-detect PROTOC if present in WinGet packages
if not defined PROTOC (
    for /d %%P in ("%LOCALAPPDATA%\Microsoft\WinGet\Packages\Google.Protobuf*") do (
        if exist "%%P\bin\protoc.exe" (
            set "PROTOC=%%P\bin\protoc.exe"
            set "PATH=%%P\bin;%PATH%"
        )
    )
)

:: 1. Check Ollama
echo [1/3] Checking Ollama daemon on http://127.0.0.1:11434...
curl.exe -s http://127.0.0.1:11434/api/tags >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo Starting Ollama serve...
    where ollama >nul 2>&1
    if %ERRORLEVEL% EQU 0 (
        start "" ollama serve
    ) else if exist "%LOCALAPPDATA%\Programs\Ollama\ollama.exe" (
        start "" "%LOCALAPPDATA%\Programs\Ollama\ollama.exe" serve
    ) else (
        echo [!] Warning: Ollama not found on PATH or LocalAppData.
        echo     Please start Ollama manually or visit https://ollama.com
    )
    timeout /t 4 /nobreak >nul
) else (
    echo Ollama daemon is active!
)

:: 2. Check Vite UI Server
echo [2/3] Checking Vite UI server on http://localhost:1420...
curl.exe -s http://localhost:1420 >nul 2>&1
if %ERRORLEVEL% EQU 0 goto vite_ready

echo Starting Vite UI server on http://localhost:1420...
start "Trans4mers-Vite" /min cmd /c "cd /d \"%~dp0apps\desktop\" && npm run dev"
echo Waiting for Vite UI server to be ready on http://localhost:1420...

set RETRY_COUNT=0
:wait_vite
timeout /t 1 /nobreak >nul
curl.exe -s http://localhost:1420 >nul 2>&1
if %ERRORLEVEL% EQU 0 (
    echo Vite UI server is ready!
    goto vite_ready
)
set /a RETRY_COUNT+=1
if %RETRY_COUNT% LSS 30 goto wait_vite
echo [!] Warning: Vite UI server did not respond within 30 seconds.
echo     Please verify node/npm and http://localhost:1420 manually.

:vite_ready
echo Vite UI server is active!


:: 3. Launch Native Trans4mers Desktop App
echo [3/3] Launching Trans4mers Desktop OS...
cd /d "%~dp0"
if exist "%~dp0target\release\trans4mers-desktop.exe" (
    start "" "%~dp0target\release\trans4mers-desktop.exe"
) else if exist "%~dp0target\debug\trans4mers-desktop.exe" (
    start "" "%~dp0target\debug\trans4mers-desktop.exe"
) else (
    echo Building Trans4mers Desktop binary...
    cd /d "%~dp0apps\desktop"
    call npm run build
    cd /d "%~dp0"
    cargo build --release -p trans4mers-desktop
    start "" "%~dp0target\release\trans4mers-desktop.exe"
)

echo =======================================================
echo Application launched! Check your taskbar for Trans4mers.
echo =======================================================
