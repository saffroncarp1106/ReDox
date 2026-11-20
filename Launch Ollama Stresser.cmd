@echo off
setlocal
title Ollama Stresser
where py >nul 2>nul
if errorlevel 1 (
    python -B "%~dp0ollama_bench.py" %*
) else (
    py -3 -B "%~dp0ollama_bench.py" %*
)
set "stresser_exit=%errorlevel%"
if not "%stresser_exit%"=="0" if "%~1"=="" pause
exit /b %stresser_exit%
