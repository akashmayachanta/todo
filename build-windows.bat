@echo off
rem Builds the Windows app: target\release\tasks-app.exe and the NSIS installer.
rem The MSI is skipped: WiX is slow, and it usually needs admin to install.
rem One-time setup: see BUILD.md, section 2.
setlocal
cd /d "%~dp0app" || exit /b 1

call npm install || exit /b 1
call npm run tauri build -- --bundles nsis || exit /b 1

echo.
echo Done.
echo   exe:       %~dp0target\release\tasks-app.exe
echo   installer: %~dp0target\release\bundle\nsis\
