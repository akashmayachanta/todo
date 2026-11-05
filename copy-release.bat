@echo off
rem Copies the latest built tasks-app.exe and tasks-server.exe to the folder they
rem run from. Only these two files are replaced; .env and data\ there are left alone.
rem
rem Build first with build-windows.bat and build-server.bat. Then use Exit on both
rem tray icons, since Windows won't replace an exe that is running.
setlocal
set "FROM=%~dp0target\release"
set "TO=C:\Drive\Software\Apps\todo"
set "FAILED=0"

if not exist "%TO%\" mkdir "%TO%" || set "FAILED=1"

call :copy_exe tasks-app.exe
call :copy_exe tasks-server.exe

echo.
if "%FAILED%"=="0" (
    echo Done: %TO%
) else (
    echo Not everything was copied. If an app is running, use Exit on its tray icon and run this again.
)
pause
exit /b %FAILED%

:copy_exe
if not exist "%FROM%\%~1" (
    echo Missing: %~1 hasn't been built yet.
    set "FAILED=1"
    exit /b 0
)
copy /y "%FROM%\%~1" "%TO%\" >nul
if errorlevel 1 (
    echo Failed:  %~1
    set "FAILED=1"
    exit /b 0
)
for %%F in ("%FROM%\%~1") do echo Copied:  %~1, built %%~tF
exit /b 0
