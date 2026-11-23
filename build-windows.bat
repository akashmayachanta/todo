@echo off
rem Builds the Windows app: target\release\tasks-app.exe.
rem   build-windows.bat          just the exe (no installer), the quick one
rem   build-windows.bat nsis     the exe and the NSIS installer in target\release\bundle\nsis\
rem The MSI is never built: WiX is slow, and it usually needs admin to install.
rem app\scripts\bump-version.mjs picks the version: the Android build's when it is
rem newer than the last Windows one, else one higher. It is written to
rem target\release\tasks-app.exe.version, which copy-release.bat puts next to the
rem server so the apps can update from it.
rem One-time setup: see BUILD.md, section 2.
setlocal
set "BUNDLE=--no-bundle"
if /i "%~1"=="nsis" set "BUNDLE=--bundles nsis"
if /i "%~1"=="--nsis" set "BUNDLE=--bundles nsis"
cd /d "%~dp0app" || exit /b 1

rem npm install only when node_modules is missing or package.json /
rem package-lock.json changed since the last install (copies kept in node_modules).
set "STAMP=node_modules\.build-stamp"
if not exist node_modules goto npm_install
fc /b package.json "%STAMP%-package.json" >nul 2>&1 || goto npm_install
fc /b package-lock.json "%STAMP%-package-lock.json" >nul 2>&1 || goto npm_install
echo npm packages unchanged, skipping npm install.
goto npm_done
:npm_install
call npm install || exit /b 1
copy /y package.json "%STAMP%-package.json" >nul
copy /y package-lock.json "%STAMP%-package-lock.json" >nul
:npm_done

set "VERSION="
for /f "delims=" %%v in ('node scripts\bump-version.mjs "%~dp0target\release\tasks-app.exe.version"') do set "VERSION=%%v"
if not defined VERSION exit /b 1
call npm run tauri build -- %BUNDLE% || exit /b 1
>"%~dp0target\release\tasks-app.exe.version" echo %VERSION%

echo.
echo Done: version %VERSION%
echo   exe:       %~dp0target\release\tasks-app.exe
if not "%BUNDLE%"=="--no-bundle" echo   installer: %~dp0target\release\bundle\nsis\
