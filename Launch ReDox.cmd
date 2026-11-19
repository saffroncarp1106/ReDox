@echo off
setlocal
cd /d "%~dp0"
if errorlevel 1 goto failed
if /i "%~1"=="--qualified-ollama" goto qualified_ollama
set "REDOX_PROFILE=release"
if /i "%~1"=="--debug" (
    set "REDOX_PROFILE=debug"
) else if not "%~1"=="" if /i not "%~1"=="--release" goto usage
echo Updating ReDox from the current project files...
if "%REDOX_PROFILE%"=="debug" (
    call "%~dp0Build Debug.bat" --no-pause
) else (
    call "%~dp0Build Release.bat" --no-pause
)
if errorlevel 1 goto failed
start "" /d "%~dp0" "%~dp0target\%REDOX_PROFILE%\redox.exe"
if errorlevel 1 goto failed
exit /b 0
:qualified_ollama
cargo build --locked --bin ollama-qualifier
if errorlevel 1 goto failed
where py >nul 2>nul
if errorlevel 1 (
    python -B "%~dp0src\ollama_service.py"
) else (
    py -3 -B "%~dp0src\ollama_service.py"
)
exit /b %errorlevel%
:usage
echo Usage: "%~nx0" [--debug ^| --release ^| --qualified-ollama]
exit /b 1
:failed
echo Build or launch failed. Copy the error above when reporting the problem.
pause
exit /b 1
