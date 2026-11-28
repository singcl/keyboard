@echo off
setlocal
cd /d "%~dp0"

cargo build --release
if errorlevel 1 (
    echo.
    echo [build] failed: keyboard-plus.exe
    exit /b 1
)
copy /y target\release\keyboard-plus.exe keyboard-plus.exe >nul

cargo build --release --no-default-features
if errorlevel 1 (
    echo.
    echo [build] failed: keyboard-plus-slim.exe
    exit /b 1
)
copy /y target\release\keyboard-plus.exe keyboard-plus-slim.exe >nul
cargo build --release >nul

echo.
echo [build] ok: %cd%\keyboard-plus.exe
echo [build] ok: %cd%\keyboard-plus-slim.exe
