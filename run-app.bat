@echo off
rem Runs the Windows app in dev mode for quick testing: no installer, and
rem UI changes reload live. Close the app window or press Ctrl+C to stop.
rem
rem Rust changes (app\src-tauri, core\, migrations) are NOT picked up while it
rem runs: restart it to rebuild. Auto-restarting on every save could run a
rem migration that was still half written and mark it done in tasks.db.
rem
rem   run-app.bat            the app only
rem   run-app.bat --server   also starts tasks-server in the tray; its own
rem                          window just shows the build. Use Exit on the icon
rem                          to stop it. (http://localhost:8050, data in .\data)
rem   run-app.bat --watch    rebuild and restart the app whenever Rust code
rem                          changes (the old behaviour). Flags can be combined.
setlocal

set "SERVER="
set "WATCH=--no-watch"
:args
if "%~1"=="" goto run
if /i "%~1"=="--server" set "SERVER=1"
if /i "%~1"=="--watch" set "WATCH="
shift
goto args

:run
if defined SERVER (
    rem Started from the repo root so it always uses the same data\ folder.
    start "tasks-server" /d "%~dp0" cmd /k cargo run -p tasks-server
)

cd /d "%~dp0app" || exit /b 1
if not exist node_modules (call npm install || exit /b 1)
call npm run tauri dev -- %WATCH%
