@echo off
rem Builds the sync server: target\release\tasks-server.exe
setlocal
cd /d "%~dp0" || exit /b 1

cargo build --release -p tasks-server || exit /b 1

echo.
echo Done: %~dp0target\release\tasks-server.exe
echo Double-click it to start it in the tray. Use Exit on its icon before building again.
