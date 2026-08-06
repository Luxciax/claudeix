@echo off
setlocal

set "VSDEVCMD=C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\Tools\VsDevCmd.bat"
if exist "%VSDEVCMD%" goto vs_found
set "VSDEVCMD=C:\Program Files\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"
if exist "%VSDEVCMD%" goto vs_found
set "VSDEVCMD=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"
if exist "%VSDEVCMD%" goto vs_found
echo Visual Studio Build Tools not found.
exit /b 1

:vs_found

call "%VSDEVCMD%" -arch=x64 -host_arch=x64
if errorlevel 1 exit /b %errorlevel%

call pnpm tauri:build
exit /b %errorlevel%
