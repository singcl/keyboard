@echo off
setlocal
cd /d "%~dp0"

set "DEFAULT_PROXY=http://127.0.0.1:10808"
set "ACTION=%~1"
set "PROXY=%~2"

if not defined ACTION set "ACTION=set"
if /I "%ACTION%"=="set" goto configure
if /I "%ACTION%"=="on" goto configure
if /I "%ACTION%"=="unset" goto unconfigure
if /I "%ACTION%"=="off" goto unconfigure
if /I "%ACTION%"=="remove" goto unconfigure
goto usage

:configure
if not defined PROXY set "PROXY=%DEFAULT_PROXY%"

git config --local http.proxy "%PROXY%"
if errorlevel 1 (
    echo [git-proxy] failed to configure HTTP proxy
    exit /b 1
)

git config --local https.proxy "%PROXY%"
if errorlevel 1 (
    echo [git-proxy] failed to configure HTTPS proxy
    exit /b 1
)

echo [git-proxy] configured:
echo   http.proxy  %PROXY%
echo   https.proxy %PROXY%
exit /b 0

:unconfigure
git config --local --unset-all http.proxy >nul 2>&1
git config --local --unset-all https.proxy >nul 2>&1

echo [git-proxy] proxy configuration removed for this repository
exit /b 0

:usage
echo Usage:
echo   git-proxy.bat set [proxy]       Configure the repository proxy
echo   git-proxy.bat unset             Remove the repository proxy
echo.
echo Default proxy: %DEFAULT_PROXY%
exit /b 2
