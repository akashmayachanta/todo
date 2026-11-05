@echo off
rem Runs the Windows app in dev mode for quick testing: no installer, and
rem UI changes reload live. Close the app window or press Ctrl+C to stop.
rem
rem   run-app.bat            the app only
rem   run-app.bat --server   also starts tasks-server in the tray; its own
rem                          window just shows the build. Use Exit on the icon
rem                          to stop it. (http://localhost:8091, data in .\data)
setlocal

if /i "%~1"=="--server" (
    rem Started from the repo root so it always uses the same data\ folder.
    start "tasks-server" /d "%~dp0" cmd /k cargo run -p tasks-server
)

cd /d "%~dp0app" || exit /b 1
if not exist node_modules (call npm install || exit /b 1)
call npm run tauri dev
