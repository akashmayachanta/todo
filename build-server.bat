@echo off
rem Builds the sync server: target\release\tasks-server.exe
setlocal
cd /d "%~dp0" || exit /b 1

cargo build --release -p tasks-server || exit /b 1

rem The browser copy of the UI, served at http://<pc>:<port>/tasks-app/ from web\ next to the exe.
rem build:web also puts it in target\release\web.
pushd app || exit /b 1
call npm run build:web
if errorlevel 1 (
	popd
	exit /b 1
)
popd

echo.
echo Done: %~dp0target\release\tasks-server.exe
echo Web UI: %~dp0target\release\web, at http://^<this PC^>:8050/tasks-app/
echo Double-click it to start it in the tray. Use Exit on its icon before building again.
