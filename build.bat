@echo off
setlocal
cd /d "%~dp0"

set CGO_ENABLED=0

go build -ldflags "-w -s -H windowsgui" -o keyboard-plus.exe ./cmd/keyboard
if errorlevel 1 (
    echo.
    echo [build] failed: keyboard-plus.exe
    exit /b 1
)

go build -tags noupdate -ldflags "-w -s -H windowsgui" -o keyboard-plus-slim.exe ./cmd/keyboard
if errorlevel 1 (
    echo.
    echo [build] failed: keyboard-plus-slim.exe
    exit /b 1
)

echo.
echo [build] ok: %cd%\keyboard-plus.exe
echo [build] ok: %cd%\keyboard-plus-slim.exe
