@echo off
rem Copies the latest built tasks-app.exe and tasks-server.exe to the folder they
rem run from. Only these and tasks.apk are replaced; .env and data\ there are left
rem alone. An exe whose size and timestamp match the copy already there is skipped.
rem
rem Build first with build-windows.bat and build-server.bat. Then use Exit on the
rem tray icon of any app that changed, since Windows won't replace a running exe.
rem
rem tasks.apk (from build-android.sh) is uploaded to the phone's FTP server when it
rem differs from the copy in that folder, which is only updated after an upload
rem works, so it is always the build the phone has. If the phone can't be reached,
rem that doesn't count as a failure and the next run tries again. Delete the folder's
rem tasks.apk to force another upload.
setlocal
set "FROM=%~dp0target\release"
set "TO=C:\Drive\Software\Apps\todo"
set "APK_DIR=%~dp0target\apk"
set "PHONE=ftp://192.168.1.102:4165"
set "PHONE_DIR=/device/FTP"
set "FAILED=0"
rem Robocopy exits with 0 for nothing to do, 1 for copied and 8 or more for failed.
rem /xx stops other things in the folder, like data\, from changing that. /r:0 fails
rem at once on a running exe, where the default is to keep retrying for weeks.
set "ROBOCOPY_OPTIONS=/xx /r:0 /w:0 /njh /njs /ndl /nfl /nc /ns /np"

if not exist "%TO%\" mkdir "%TO%" || set "FAILED=1"

call :copy_exe tasks-app.exe
call :copy_exe tasks-server.exe
call :copy_apk

echo.
if "%FAILED%"=="0" (
    echo Done: %TO%
) else (
    echo Not everything was copied, see above. If an app is running, use Exit on its tray icon and run this again.
)
pause
exit /b %FAILED%

:copy_apk
if not exist "%APK_DIR%\tasks.apk" (
    echo Missing: tasks.apk hasn't been built yet.
    set "FAILED=1"
    exit /b 0
)
rem /l only compares, so nothing is copied until the upload has worked.
robocopy "%APK_DIR%" "%TO%" tasks.apk %ROBOCOPY_OPTIONS% /l >nul
if errorlevel 8 (
    echo Failed:  tasks.apk
    set "FAILED=1"
    exit /b 0
)
if not errorlevel 1 (
    echo Skipped: tasks.apk, unchanged
    exit /b 0
)
curl.exe -s --connect-timeout 3 --max-time 10 -l "%PHONE%/" >nul
if errorlevel 1 (
    echo Skipped: tasks.apk, the phone isn't reachable at %PHONE%. The next run will try again.
    exit /b 0
)
rem The speed limit stops the upload if it stalls, such as when the phone sleeps.
curl.exe -sS --connect-timeout 3 --speed-time 30 --speed-limit 1024 -T "%APK_DIR%\tasks.apk" "%PHONE%%PHONE_DIR%/tasks.apk"
if errorlevel 1 (
    echo Failed:  tasks.apk upload, the copy on the phone may be incomplete. The next run will try again.
    set "FAILED=1"
    exit /b 0
)
robocopy "%APK_DIR%" "%TO%" tasks.apk %ROBOCOPY_OPTIONS% >nul
if errorlevel 8 (
    echo Failed:  tasks.apk reached the phone but wasn't copied to %TO%
    set "FAILED=1"
    exit /b 0
)
for %%F in ("%APK_DIR%\tasks.apk") do echo Copied:  tasks.apk to the phone and the folder, built %%~tF
exit /b 0

:copy_exe
if not exist "%FROM%\%~1" (
    echo Missing: %~1 hasn't been built yet.
    set "FAILED=1"
    exit /b 0
)
robocopy "%FROM%" "%TO%" "%~1" %ROBOCOPY_OPTIONS% >nul
if errorlevel 8 (
    echo Failed:  %~1
    set "FAILED=1"
    exit /b 0
)
if not errorlevel 1 (
    echo Skipped: %~1, unchanged
    exit /b 0
)
for %%F in ("%FROM%\%~1") do echo Copied:  %~1, built %%~tF
exit /b 0
