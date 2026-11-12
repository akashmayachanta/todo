@echo off
rem Builds the Windows app: target\release\tasks-app.exe and the NSIS installer.
rem The MSI is skipped: WiX is slow, and it usually needs admin to install.
rem app\scripts\bump-version.mjs picks the version: the Android build's when it is
rem newer than the last Windows one, else one higher. It is written to
rem target\release\tasks-app.exe.version, which copy-release.bat puts next to the
rem server so the apps can update from it.
rem One-time setup: see BUILD.md, section 2.
setlocal
cd /d "%~dp0app" || exit /b 1

call npm install || exit /b 1
set "VERSION="
for /f "delims=" %%v in ('node scripts\bump-version.mjs "%~dp0target\release\tasks-app.exe.version"') do set "VERSION=%%v"
if not defined VERSION exit /b 1
call npm run tauri build -- --bundles nsis || exit /b 1
>"%~dp0target\release\tasks-app.exe.version" echo %VERSION%

echo.
echo Done: version %VERSION%
echo   exe:       %~dp0target\release\tasks-app.exe
echo   installer: %~dp0target\release\bundle\nsis\
