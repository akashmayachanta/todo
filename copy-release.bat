@echo off
rem Copies the latest built tasks-app.exe, tasks-server.exe and tasks.apk to the
rem folder the server runs from. Only these and their .version files are replaced;
rem .env and data\ there are left alone. A file whose size and timestamp match the
rem copy already there is skipped.
rem
rem Build first with build-windows.bat, build-server.bat and build-android.sh.
rem Windows won't replace a running exe, so an exe that changed and is running
rem from that folder is stopped, replaced and started again.
rem
rem The server hands tasks-app.exe and tasks.apk to the apps as updates, with the
rem version in the .version file beside each; the apps ask before installing.
rem
rem Optional: tasks.apk is also uploaded to the phone's FTP server when it differs
rem from the last upload (kept in target\apk\uploaded). If the phone can't be
rem reached, you're asked for its IP (port 4165 unless you type ip:port); press
rem Enter to skip, which doesn't count as a failure: the phone can take the update
rem from the server instead. Delete target\apk\uploaded to force another upload.
setlocal
set "FROM=%~dp0target\release"
set "TO=C:\Drive\Software\Apps\todo"
set "APK_DIR=%~dp0target\apk"
set "UPLOADED=%~dp0target\apk\uploaded"
set "PHONE_PORT=4165"
set "PHONE=ftp://192.168.1.102:%PHONE_PORT%"
set "PHONE_DIR=/device/FTP"
set "FAILED=0"
rem Robocopy exits with 0 for nothing to do, 1 for copied and 8 or more for failed.
rem /xx stops other things in the folder, like data\, from changing that. /r:0 fails
rem at once on a running exe, where the default is to keep retrying for weeks.
set "ROBOCOPY_OPTIONS=/xx /r:0 /w:0 /njh /njs /ndl /nfl /nc /ns /np"

if not exist "%TO%\" mkdir "%TO%" || set "FAILED=1"

call :copy_file "%FROM%" tasks-app.exe
call :copy_file "%FROM%" tasks-server.exe
call :copy_file "%APK_DIR%" tasks.apk
call :upload_apk

echo.
if "%FAILED%"=="0" (
    echo Done: %TO%
) else (
    echo Not everything was copied, see above.
)
pause
exit /b %FAILED%

rem Copies %2 from folder %1, then its .version file if it has one. The version
rem goes only after the file itself, so the server never shows a version with
rem the build before it.
:copy_file
if not exist "%~1\%~2" (
    echo Missing: %~2 hasn't been built yet.
    set "FAILED=1"
    exit /b 0
)
set "WAS_RUNNING="
if /i "%~x2"==".exe" (
    robocopy "%~1" "%TO%" "%~2" %ROBOCOPY_OPTIONS% /l >nul
    if not errorlevel 8 if errorlevel 1 call :stop_exe "%~2"
)
robocopy "%~1" "%TO%" "%~2" %ROBOCOPY_OPTIONS% >nul
if errorlevel 8 (
    echo Failed:  %~2
    set "FAILED=1"
    call :start_exe "%~2"
    exit /b 0
)
if errorlevel 1 (
    for %%F in ("%~1\%~2") do echo Copied:  %~2, built %%~tF
) else (
    echo Skipped: %~2, unchanged
)
call :start_exe "%~2"
if exist "%~1\%~2.version" (
    robocopy "%~1" "%TO%" "%~2.version" %ROBOCOPY_OPTIONS% >nul
    if errorlevel 8 (
        echo Failed:  %~2.version, so the apps won't see this build as an update
        set "FAILED=1"
    )
)
exit /b 0

rem Stops %1 if it is running from %TO%, so the new build can replace it, and
rem sets WAS_RUNNING so :start_exe starts it again. A copy running from anywhere
rem else (target\release) is left alone.
:stop_exe
powershell -NoProfile -Command "$p = @(Get-Process -Name '%~n1' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq '%TO%\%~1' }); if (-not $p) { exit 0 }; $p | Stop-Process -Force; $p | Wait-Process -Timeout 15 -ErrorAction SilentlyContinue; exit 1"
if errorlevel 1 (
    set "WAS_RUNNING=1"
    echo Stopped: %~1, to replace it
)
exit /b 0

rem Starts %1 again from %TO% if :stop_exe stopped it. The app goes back to the
rem tray, as it does when it starts with Windows.
:start_exe
if not defined WAS_RUNNING exit /b 0
set "WAS_RUNNING="
if /i "%~1"=="tasks-app.exe" (
    start "" /d "%TO%" "%TO%\%~1" --minimized
) else (
    start "" /d "%TO%" "%TO%\%~1"
)
echo Started: %~1 again
exit /b 0

rem Uploads tasks.apk to the phone when it differs from the last upload.
:upload_apk
if not exist "%APK_DIR%\tasks.apk" exit /b 0
rem /l only compares; the copy in UPLOADED is only updated after an upload works.
robocopy "%APK_DIR%" "%UPLOADED%" tasks.apk %ROBOCOPY_OPTIONS% /l >nul
if errorlevel 8 exit /b 0
if not errorlevel 1 (
    echo Skipped: tasks.apk upload, the phone has this build
    exit /b 0
)
call :find_phone
if errorlevel 1 (
    echo Skipped: tasks.apk upload, the phone isn't reachable. It can update from the server.
    exit /b 0
)
rem The speed limit stops the upload if it stalls, such as when the phone sleeps.
curl.exe -sS --connect-timeout 3 --speed-time 30 --speed-limit 1024 -T "%APK_DIR%\tasks.apk" "%PHONE%%PHONE_DIR%/tasks.apk"
if errorlevel 1 (
    echo Failed:  tasks.apk upload, the copy on the phone may be incomplete. The next run will try again.
    exit /b 0
)
robocopy "%APK_DIR%" "%UPLOADED%" tasks.apk %ROBOCOPY_OPTIONS% >nul
for %%F in ("%APK_DIR%\tasks.apk") do echo Uploaded: tasks.apk to the phone, built %%~tF
exit /b 0

rem Sets PHONE to an FTP server that answers, asking for the IP until one does.
rem Exits with 1 if Enter is pressed without an IP.
:find_phone
curl.exe -s --connect-timeout 3 --max-time 10 -l "%PHONE%/" >nul && exit /b 0
echo The phone isn't reachable at %PHONE%.
:ask_phone_ip
set "PHONE_IP="
set /p "PHONE_IP=Phone IP (port %PHONE_PORT% unless you type ip:port), or Enter to skip: "
if not defined PHONE_IP exit /b 1
set "PHONE_IP=%PHONE_IP: =%"
if not defined PHONE_IP exit /b 1
if "%PHONE_IP::=%"=="%PHONE_IP%" (
    set "PHONE=ftp://%PHONE_IP%:%PHONE_PORT%"
) else (
    set "PHONE=ftp://%PHONE_IP%"
)
curl.exe -s --connect-timeout 3 --max-time 10 -l "%PHONE%/" >nul && exit /b 0
echo Not reachable at %PHONE% either.
goto ask_phone_ip
